mod accounts;
mod algo;
mod backend;
mod chain;
mod instance;
mod job;
mod kawpow;
mod merged;
mod pool;
mod recorder;
mod session;
mod template;
mod util;
mod vardiff;

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use futures_util::{SinkExt, StreamExt};
use pool_common::{
    config::{Coin, Config, Stratum},
    rpc::RpcClient,
};
use serde_json::{Value, json};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{debug, error, info, warn};

use crate::{
    accounts::{AuxCandidate, Candidate, PayoutCoin, WorkerIds},
    algo::Algo,
    backend::{ApiClient, Backend},
    chain::Chain,
    job::{FoundBlock, Job},
    pool::{Pool, Port},
    recorder::{BlockEvent, Recorder, ShareEvent, Sink},
    session::{AcceptedShare, Login, Session},
    vardiff::VarDiffConfig,
};

const MAX_LINE_BYTES: usize = 16 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const VARDIFF_TICK: Duration = Duration::from_secs(30);

type JobFeed = watch::Receiver<Option<Arc<Job>>>;

/// One coin on a port: its daemon, its current work and where its shares are stored.
#[derive(Clone)]
struct CoinCtx {
    symbol: String,
    coin_id: i32,
    rpc: RpcClient,
    pool: Arc<Pool>,
    recorder: Recorder,
    jobs: JobFeed,
    aux: Option<AuxCtx>,
}

/// A coin that is merge-mined along with another one.
#[derive(Clone)]
struct AuxCtx {
    symbol: String,
    coin_id: i32,
    rpc: RpcClient,
    recorder: Recorder,
}

/// What every connection on a port needs besides its own socket.
#[derive(Clone)]
struct Server {
    port: Arc<Port>,
    backend: Backend,
    coins: Arc<Vec<CoinCtx>>,
    /// Every coin this server has a daemon for, on whatever port: miners can be paid in these.
    payout_coins: Arc<Vec<PayoutCtx>>,
}

struct PayoutCtx {
    symbol: String,
    coin_id: i32,
    rpc: RpcClient,
}

#[tokio::main]
async fn main() -> Result<()> {
    pool_common::init_logging();
    // An enrolled server gets its settings from the pool; otherwise there is a config file and a database.
    let (cfg, backend, sent) = match ApiClient::from_env()? {
        Some(api) => {
            let (cfg, sent) = api.config().await.context("asking the pool for this server's configuration")?;
            api.spawn_heartbeat();
            (cfg, Backend::Api(api), Some(sent))
        }
        None => {
            let cfg = Config::load()?;
            let db = pool_common::db::connect(&cfg).await?;
            instance::register(&db, &cfg.instance).await?;
            (cfg, Backend::Direct(db), None)
        }
    };
    if cfg.stratum.is_empty() {
        // Normal for a fresh server: no coin has a daemon and a pool wallet yet.
        warn!("the pool offers no coin to mine here yet, waiting for that to change");
    }
    info!(id = cfg.instance.id, region = %cfg.instance.region, host = %cfg.instance.host, "stratum instance started");

    let mut payout_coins = Vec::new();
    for coin in &cfg.coins {
        let (coin_id, _) = open_coin(&backend, coin, None).await?;
        payout_coins.push(PayoutCtx { symbol: coin.symbol.clone(), coin_id, rpc: RpcClient::new(coin)? });
    }
    let payout_coins = Arc::new(payout_coins);

    let mut recorders = Vec::new();
    for listener in &cfg.stratum {
        let coins: Vec<&Coin> = cfg.coins.iter().filter(|coin| coin.algo == listener.algo).collect();
        if coins.is_empty() {
            warn!(algo = %listener.algo, "no coin uses this algo, its port stays closed");
            continue;
        }
        let started = start_listener(listener, &coins, cfg.instance.id, backend.clone(), payout_coins.clone())
            .await
            .with_context(|| format!("starting stratum for {}", listener.algo))?;
        recorders.extend(started);
    }
    for coin in cfg.coins.iter().filter(|coin| !cfg.stratum.iter().any(|listener| listener.algo == coin.algo)) {
        warn!(coin = %coin.symbol, algo = %coin.algo, "no [[stratum]] port for this coin's algo, it will not be mined");
    }

    let changed = async {
        match (&backend, sent) {
            (Backend::Api(api), Some(sent)) => api.config_changed(sent).await,
            _ => std::future::pending().await,
        }
    };
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result?;
            info!("shutting down");
        }
        // Ports, difficulties and coins are fixed at startup; the supervisor starts us again.
        _ = changed => info!("the pool changed this server's configuration, restarting to apply it"),
    }
    for recorder in recorders {
        recorder.flush().await;
    }
    if let Backend::Direct(db) = &backend {
        instance::unregister(db, &cfg.instance).await;
    }
    Ok(())
}

/// Sets up every coin of one algorithm and starts accepting miners on the algorithm's port.
async fn start_listener(
    settings: &Stratum,
    coins: &[&Coin],
    instance_id: u8,
    backend: Backend,
    payout_coins: Arc<Vec<PayoutCtx>>,
) -> Result<Vec<Recorder>> {
    // Merge-mined coins have no miners of their own: they ride along with their parent.
    let (merged, coins): (Vec<&Coin>, Vec<&Coin>) = coins.iter().copied().partition(|coin| coin.aux_of.is_some());
    ensure!(!coins.is_empty(), "every coin on this port is merge-mined, none can be mined directly");
    for coin in merged.iter().filter(|coin| !coins.iter().any(|parent| Some(&parent.symbol) == coin.aux_of.as_ref())) {
        warn!(coin = %coin.symbol, "the coin it is merge-mined with is not on this port, it will not be mined");
    }
    // The extranonce layout is announced at subscribe, before the miner names a coin.
    let chain = Chain::from_name(&coins[0].chain)?;
    ensure!(
        coins.iter().all(|coin| coin.chain == coins[0].chain),
        "coins sharing a port must have the same chain type"
    );
    let algo = Algo::from_name(&settings.algo)?;
    ensure!(
        (algo == Algo::Kawpow) == (chain == Chain::Ravencoin),
        "KawPoW and the ravencoin chain type only work together"
    );
    let port = Arc::new(Port::new(
        algo,
        chain,
        VarDiffConfig {
            start: settings.start_difficulty,
            min: settings.min_difficulty,
            max: settings.max_difficulty,
            share_target_secs: settings.share_target_secs,
        },
        instance_id,
    ));
    let port_number: Option<i32> = settings.listen.rsplit(':').next().and_then(|port| port.parse().ok());

    let mut contexts = Vec::new();
    for coin in &coins {
        let (coin_id, sink) = open_coin(&backend, coin, port_number).await?;
        let rpc = RpcClient::new(coin)?;
        let pool = Arc::new(Pool::default());
        let (job_tx, jobs) = watch::channel(None);

        let mut riders = merged.iter().filter(|rider| rider.aux_of.as_ref() == Some(&coin.symbol));
        let rider = riders.next();
        ensure!(riders.next().is_none(), "{} can only have one coin merge-mined with it", coin.symbol);
        let (aux, aux_feed) = match rider {
            Some(rider) => {
                ensure!(chain == Chain::Bitcoin, "merged mining needs the bitcoin chain type");
                let (coin_id, sink) = open_coin(&backend, rider, port_number).await?;
                let rpc = RpcClient::new(rider)?;
                let (work_tx, work) = watch::channel(None);
                tokio::spawn(merged::run(rpc.clone(), (*rider).clone(), pool.clone(), work_tx));
                let aux = AuxCtx { symbol: rider.symbol.clone(), coin_id, rpc, recorder: Recorder::spawn(sink) };
                (Some(aux), Some(work))
            }
            None => (None, None),
        };

        tokio::spawn(template::run(rpc.clone(), pool.clone(), chain, (*coin).clone(), instance_id, aux_feed, job_tx));
        let recorder = Recorder::spawn(sink);
        contexts.push(CoinCtx { symbol: coin.symbol.clone(), coin_id, rpc, pool, recorder, jobs, aux });
    }
    let recorders = contexts
        .iter()
        .flat_map(|coin| [Some(coin.recorder.clone()), coin.aux.as_ref().map(|aux| aux.recorder.clone())])
        .flatten()
        .collect();
    let symbols: Vec<&str> = coins.iter().map(|coin| coin.symbol.as_str()).collect();
    let ctx = Server { port, backend, coins: Arc::new(contexts), payout_coins };

    let listener = TcpListener::bind(&settings.listen).await.with_context(|| format!("binding {}", settings.listen))?;
    info!(listen = %settings.listen, algo = %settings.algo, coins = %symbols.join(","), "stratum listening");
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    let ctx = ctx.clone();
                    tokio::spawn(async move {
                        if let Err(err) = serve(stream, peer, ctx).await {
                            debug!(%peer, "connection closed: {err:#}");
                        }
                    });
                }
                Err(err) => {
                    warn!("accept failed: {err}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    });
    Ok(recorders)
}

/// The coin's database id and where its shares go.
async fn open_coin(backend: &Backend, coin: &Coin, port_number: Option<i32>) -> Result<(i32, Sink)> {
    match backend {
        Backend::Direct(db) => {
            let coin_id = pool_common::db::ensure_coin(db, coin).await?;
            if port_number.is_some() {
                // Published so the website can tell miners where to connect.
                sqlx::query("UPDATE coins SET stratum_port = $2 WHERE id = $1")
                    .bind(coin_id)
                    .bind(port_number)
                    .execute(db)
                    .await?;
            }
            Ok((coin_id, Sink::Direct { db: db.clone(), coin_id }))
        }
        // The pool registered the coin when it handed out this server's configuration.
        Backend::Api(api) => Ok((0, Sink::Api { api: api.clone(), symbol: coin.symbol.clone() })),
    }
}

async fn serve(stream: TcpStream, peer: SocketAddr, ctx: Server) -> Result<()> {
    stream.set_nodelay(true)?;
    let mut lines = Framed::new(stream, LinesCodec::new_with_max_length(MAX_LINE_BYTES));
    let mut session = Session::new(ctx.port.clone(), Instant::now());
    // Set at login, when the miner's coin is known.
    let mut coin: Option<CoinCtx> = None;
    let mut tick = tokio::time::interval(VARDIFF_TICK);
    let mut last_message = tokio::time::Instant::now();
    debug!(%peer, "connected");

    loop {
        let outgoing = tokio::select! {
            line = lines.next() => {
                let Some(line) = line else { return Ok(()) };
                last_message = tokio::time::Instant::now();
                let mut out = session.handle_line(&line?, current_job(&coin).as_deref(), Instant::now());
                if let (Some(share), Some(coin)) = (out.share, &coin) {
                    record_share(coin, share);
                }
                if let Some(request) = out.auth {
                    let login = login(&ctx, &coin, &request.user, &request.password).await.unwrap_or_else(|err| {
                        warn!(%peer, "authorize failed: {err:#}");
                        None
                    });
                    let reply = match login {
                        Some((mut chosen, worker, aux_worker)) => {
                            let current = chosen.jobs.borrow_and_update().clone();
                            let login = Login { worker, aux_worker, pool: chosen.pool.clone() };
                            debug!(%peer, coin = %chosen.symbol, "authorized");
                            coin = Some(chosen);
                            session.finish_authorize(request, Some(login), current.as_deref())
                        }
                        None => session.finish_authorize(request, None, None),
                    };
                    out.lines.extend(reply);
                }
                out.lines
            }
            job = next_job(&mut coin) => job.and_then(|job| session.on_job(&job)).into_iter().collect(),
            _ = tick.tick() => session.on_tick(current_job(&coin).as_deref(), Instant::now()),
            _ = tokio::time::sleep_until(last_message + IDLE_TIMEOUT) => {
                warn!(%peer, "idle timeout");
                return Ok(());
            }
        };
        for line in outgoing {
            lines.send(line).await?;
        }
    }
}

fn current_job(coin: &Option<CoinCtx>) -> Option<Arc<Job>> {
    coin.as_ref().and_then(|coin| coin.jobs.borrow().clone())
}

/// Resolves when the connection's coin publishes a new job; never before login.
async fn next_job(coin: &mut Option<CoinCtx>) -> Option<Arc<Job>> {
    if let Some(coin) = coin
        && coin.jobs.changed().await.is_ok()
    {
        return coin.jobs.borrow_and_update().clone();
    }
    std::future::pending().await
}

/// Finds the coin and account a login is for. A connection stays with its first coin.
async fn login(
    ctx: &Server,
    current: &Option<CoinCtx>,
    user: &str,
    password: &str,
) -> Result<Option<(CoinCtx, WorkerIds, Option<WorkerIds>)>> {
    let candidates: Vec<Candidate> = ctx
        .coins
        .iter()
        .filter(|coin| current.as_ref().is_none_or(|current| current.symbol == coin.symbol))
        .map(|coin| Candidate {
            coin_id: coin.coin_id,
            symbol: &coin.symbol,
            rpc: &coin.rpc,
            aux: coin.aux.as_ref().map(|aux| AuxCandidate { coin_id: aux.coin_id, symbol: &aux.symbol, rpc: &aux.rpc }),
        })
        .collect();
    let payout_coins: Vec<PayoutCoin> = ctx
        .payout_coins
        .iter()
        .map(|coin| PayoutCoin { coin_id: coin.coin_id, symbol: &coin.symbol, rpc: &coin.rpc })
        .collect();
    let resolved = accounts::resolve(&ctx.backend, &candidates, &payout_coins, user, password).await?;
    let Some((index, worker, aux_worker)) = resolved else {
        return Ok(None);
    };
    let symbol = candidates[index].symbol;
    Ok(ctx.coins.iter().find(|coin| coin.symbol == symbol).map(|coin| (coin.clone(), worker, aux_worker)))
}

fn record_share(coin: &CoinCtx, share: AcceptedShare) {
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    let AcceptedShare { worker, aux_worker, difficulty, block, aux_block } = share;
    // The same work counts on the merge-mined chain, for whoever the miner is there.
    if let (Some(aux), Some(ids)) = (&coin.aux, aux_worker) {
        let WorkerIds { account_id, worker_id, solo, party_id } = ids;
        aux.recorder.share(ShareEvent { account_id, worker_id, solo, party_id, difficulty, at });
        if let Some(block) = aux_block {
            let aux = aux.clone();
            tokio::spawn(async move {
                if submit_aux_block(&aux, &block).await {
                    let event = BlockEvent {
                        account_id,
                        worker_id,
                        solo,
                        party_id,
                        height: block.height,
                        hash: block.hash,
                        reward: block.reward,
                        network_difficulty: block.network_difficulty,
                        at,
                    };
                    aux.recorder.block(event).await;
                }
            });
        }
    }
    let WorkerIds { account_id, worker_id, solo, party_id } = worker;
    coin.recorder.share(ShareEvent { account_id, worker_id, solo, party_id, difficulty, at });
    if let Some(block) = block {
        let coin = coin.clone();
        tokio::spawn(async move {
            if submit_block(&coin.rpc, &block).await {
                let event = BlockEvent {
                    account_id,
                    worker_id,
                    solo,
                    party_id,
                    height: block.height,
                    hash: block.hash,
                    reward: block.reward,
                    network_difficulty: block.network_difficulty,
                    at,
                };
                coin.recorder.block(event).await;
            }
            coin.pool.refresh.notify_one();
        });
    }
}

/// Hands a proof of work to the merge-mined chain. Returns whether its node took the block.
async fn submit_aux_block(aux: &AuxCtx, block: &FoundBlock) -> bool {
    match aux.rpc.call::<Value>("submitauxblock", json!([block.hash, block.hex])).await {
        Ok(Value::Bool(true)) => {
            info!(coin = %aux.symbol, height = block.height, hash = %block.hash, "merge-mined block accepted");
            true
        }
        Ok(answer) => {
            warn!(coin = %aux.symbol, height = block.height, hash = %block.hash, %answer, "merge-mined block not accepted");
            false
        }
        // The merged chain has moved on since this work was handed out.
        Err(err) if format!("{err:#}").contains("block hash unknown") => {
            warn!(coin = %aux.symbol, height = block.height, "merge-mined block came too late");
            false
        }
        Err(err) => {
            error!(coin = %aux.symbol, height = block.height, "merge-mined block submission failed: {err:#}");
            false
        }
    }
}

/// Returns whether the node took the block into its chain.
async fn submit_block(rpc: &RpcClient, block: &FoundBlock) -> bool {
    // submitblock answers null on success and a reason string on rejection.
    match rpc.call::<Value>("submitblock", json!([block.hex])).await {
        Ok(Value::Null) => {
            info!(height = block.height, hash = %block.hash, "block accepted");
            return true;
        }
        // Valid, but the node already has another block at this height: we lost a race.
        Ok(Value::String(reason)) if reason == "inconclusive" || reason == "duplicate" => {
            warn!(height = block.height, hash = %block.hash, %reason, "block came too late")
        }
        Ok(reason) => error!(height = block.height, hash = %block.hash, %reason, "block rejected"),
        Err(err) => error!(height = block.height, hash = %block.hash, "block submission failed: {err:#}"),
    }
    false
}
