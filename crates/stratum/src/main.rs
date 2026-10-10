mod accounts;
mod algo;
mod chain;
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
use sqlx::PgPool;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{debug, error, info, warn};

use crate::{
    accounts::{Candidate, WorkerIds},
    algo::Algo,
    chain::Chain,
    job::{FoundBlock, Job},
    pool::{Pool, Port},
    recorder::{BlockEvent, Recorder, ShareEvent},
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
    db: PgPool,
    coins: Arc<Vec<CoinCtx>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    pool_common::init_logging();
    let cfg = Config::load()?;
    ensure!(!cfg.stratum.is_empty(), "no [[stratum]] listener configured");
    let db = pool_common::db::connect(&cfg).await?;

    let mut recorders = Vec::new();
    for listener in &cfg.stratum {
        let coins: Vec<&Coin> = cfg.coins.iter().filter(|coin| coin.algo == listener.algo).collect();
        ensure!(!coins.is_empty(), "no coin under [[coins]] uses algo {}", listener.algo);
        let started = start_listener(listener, &coins, db.clone())
            .await
            .with_context(|| format!("starting stratum for {}", listener.algo))?;
        recorders.extend(started);
    }
    for coin in cfg.coins.iter().filter(|coin| !cfg.stratum.iter().any(|listener| listener.algo == coin.algo)) {
        warn!(coin = %coin.symbol, algo = %coin.algo, "no [[stratum]] port for this coin's algo, it will not be mined");
    }

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    for recorder in recorders {
        recorder.flush().await;
    }
    Ok(())
}

/// Sets up every coin of one algorithm and starts accepting miners on the algorithm's port.
async fn start_listener(settings: &Stratum, coins: &[&Coin], db: PgPool) -> Result<Vec<Recorder>> {
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
    ));
    let port_number: Option<i32> = settings.listen.rsplit(':').next().and_then(|port| port.parse().ok());

    let mut contexts = Vec::new();
    for coin in coins {
        let coin_id = pool_common::db::ensure_coin(&db, coin).await?;
        // Published so the website can tell miners where to connect.
        sqlx::query("UPDATE coins SET stratum_port = $2 WHERE id = $1")
            .bind(coin_id)
            .bind(port_number)
            .execute(&db)
            .await?;
        let rpc = RpcClient::new(coin)?;
        let payout_script = payout_script(&rpc, coin).await.with_context(|| format!("coin {}", coin.symbol))?;
        let pool = Arc::new(Pool::default());
        let (job_tx, jobs) = watch::channel(None);
        tokio::spawn(template::run(rpc.clone(), pool.clone(), chain, payout_script, job_tx));
        let recorder = Recorder::spawn(db.clone(), coin_id);
        contexts.push(CoinCtx { symbol: coin.symbol.clone(), coin_id, rpc, pool, recorder, jobs });
    }
    let recorders = contexts.iter().map(|coin| coin.recorder.clone()).collect();
    let symbols: Vec<&str> = coins.iter().map(|coin| coin.symbol.as_str()).collect();
    let ctx = Server { port, db, coins: Arc::new(contexts) };

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

/// The output script block rewards are paid to.
async fn payout_script(rpc: &RpcClient, coin: &Coin) -> Result<Vec<u8>> {
    let address = match &coin.pool_address {
        Some(address) => address.clone(),
        None => {
            let address: String = rpc.call("getnewaddress", json!([])).await?;
            warn!(%address, "no pool_address configured, paying rewards to a fresh daemon wallet address");
            address
        }
    };
    let info: Value = rpc.call("validateaddress", json!([address])).await?;
    ensure!(info["isvalid"] == true, "pool address {address} is not valid for {}", coin.symbol);
    let script = info["scriptPubKey"].as_str().context("validateaddress returned no scriptPubKey")?;
    Ok(hex::decode(script)?)
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
        .filter(|coin| current.as_ref().is_none_or(|current| current.coin_id == coin.coin_id))
        .map(|coin| Candidate { coin_id: coin.coin_id, symbol: &coin.symbol, rpc: &coin.rpc })
        .collect();
    let Some((index, worker)) = accounts::resolve(&ctx.db, &candidates, user, password).await? else {
        return Ok(None);
    };
    let coin_id = candidates[index].coin_id;
    Ok(ctx.coins.iter().find(|coin| coin.coin_id == coin_id).map(|coin| (coin.clone(), worker)))
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
