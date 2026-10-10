mod accounts;
mod algo;
mod backend;
mod chain;
mod instance;
mod job;
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
    accounts::{Candidate, WorkerIds},
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
}

/// What every connection on a port needs besides its own socket.
#[derive(Clone)]
struct Server {
    port: Arc<Port>,
    backend: Backend,
    coins: Arc<Vec<CoinCtx>>,
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

    let mut recorders = Vec::new();
    for listener in &cfg.stratum {
        let coins: Vec<&Coin> = cfg.coins.iter().filter(|coin| coin.algo == listener.algo).collect();
        if coins.is_empty() {
            warn!(algo = %listener.algo, "no coin uses this algo, its port stays closed");
            continue;
        }
        let started = start_listener(listener, &coins, cfg.instance.id, backend.clone())
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
) -> Result<Vec<Recorder>> {
    // The extranonce layout is announced at subscribe, before the miner names a coin.
    let chain = Chain::from_name(&coins[0].chain)?;
    ensure!(
        coins.iter().all(|coin| coin.chain == coins[0].chain),
        "coins sharing a port must have the same chain type"
    );
    let port = Arc::new(Port::new(
        Algo::from_name(&settings.algo)?,
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
    for coin in coins {
        let (coin_id, sink) = match &backend {
            Backend::Direct(db) => {
                let coin_id = pool_common::db::ensure_coin(db, coin).await?;
                // Published so the website can tell miners where to connect.
                sqlx::query("UPDATE coins SET stratum_port = $2 WHERE id = $1")
                    .bind(coin_id)
                    .bind(port_number)
                    .execute(db)
                    .await?;
                (coin_id, Sink::Direct { db: db.clone(), coin_id })
            }
            // The pool registered the coin when it handed out this server's configuration.
            Backend::Api(api) => (0, Sink::Api { api: api.clone(), symbol: coin.symbol.clone() }),
        };
        let rpc = RpcClient::new(coin)?;
        let pool = Arc::new(Pool::default());
        let (job_tx, jobs) = watch::channel(None);
        tokio::spawn(template::run(rpc.clone(), pool.clone(), chain, (*coin).clone(), instance_id, job_tx));
        let recorder = Recorder::spawn(sink);
        contexts.push(CoinCtx { symbol: coin.symbol.clone(), coin_id, rpc, pool, recorder, jobs });
    }
    let recorders = contexts.iter().map(|coin| coin.recorder.clone()).collect();
    let symbols: Vec<&str> = coins.iter().map(|coin| coin.symbol.as_str()).collect();
    let ctx = Server { port, backend, coins: Arc::new(contexts) };

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
                        Some((mut chosen, worker)) => {
                            let current = chosen.jobs.borrow_and_update().clone();
                            let login = Login { worker, pool: chosen.pool.clone() };
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
) -> Result<Option<(CoinCtx, WorkerIds)>> {
    let candidates: Vec<Candidate> = ctx
        .coins
        .iter()
        .filter(|coin| current.as_ref().is_none_or(|current| current.symbol == coin.symbol))
        .map(|coin| Candidate { coin_id: coin.coin_id, symbol: &coin.symbol, rpc: &coin.rpc })
        .collect();
    let Some((index, worker)) = accounts::resolve(&ctx.backend, &candidates, user, password).await? else {
        return Ok(None);
    };
    let symbol = candidates[index].symbol;
    Ok(ctx.coins.iter().find(|coin| coin.symbol == symbol).map(|coin| (coin.clone(), worker)))
}

fn record_share(coin: &CoinCtx, share: AcceptedShare) {
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    let AcceptedShare { worker, difficulty, block } = share;
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
