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

use anyhow::{Context, Result, anyhow, ensure};
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
    accounts::WorkerIds,
    algo::Algo,
    chain::Chain,
    job::{FoundBlock, Job},
    pool::Pool,
    recorder::{BlockEvent, Recorder, ShareEvent},
    session::{AcceptedShare, Session},
    vardiff::VarDiffConfig,
};

const MAX_LINE_BYTES: usize = 16 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const VARDIFF_TICK: Duration = Duration::from_secs(30);

type JobFeed = watch::Receiver<Option<Arc<Job>>>;

/// What every connection needs besides its own socket.
#[derive(Clone)]
struct Server {
    pool: Arc<Pool>,
    rpc: RpcClient,
    db: PgPool,
    coin_id: i32,
    recorder: Recorder,
    jobs: JobFeed,
}

#[tokio::main]
async fn main() -> Result<()> {
    pool_common::init_logging();
    let cfg = Config::load()?;
    ensure!(!cfg.stratum.is_empty(), "no [[stratum]] listener configured");
    let db = pool_common::db::connect(&cfg).await?;

    let mut recorders = Vec::new();
    for listener in &cfg.stratum {
        let coin = cfg
            .coins
            .iter()
            .find(|c| c.symbol == listener.coin)
            .ok_or_else(|| anyhow!("stratum coin {} is not configured under [[coins]]", listener.coin))?;
        let recorder = start_listener(listener, coin, db.clone())
            .await
            .with_context(|| format!("starting stratum for {}", coin.symbol))?;
        recorders.push(recorder);
    }

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    for recorder in recorders {
        recorder.flush().await;
    }
    Ok(())
}

/// Sets up everything one coin needs and starts accepting miners on its port.
async fn start_listener(settings: &Stratum, coin: &Coin, db: PgPool) -> Result<Recorder> {
    let coin_id = pool_common::db::ensure_coin(&db, coin).await?;
    let recorder = Recorder::spawn(db.clone(), coin_id);
    // Published so the website can tell miners where to connect.
    let port: Option<i32> = settings.listen.rsplit(':').next().and_then(|port| port.parse().ok());
    sqlx::query("UPDATE coins SET stratum_port = $2 WHERE id = $1").bind(coin_id).bind(port).execute(&db).await?;
    let rpc = RpcClient::new(coin)?;
    let payout_script = payout_script(&rpc, coin).await?;
    let chain = Chain::from_name(&coin.chain)?;
    let pool = Arc::new(Pool::new(
        Algo::from_name(&coin.algo)?,
        chain,
        VarDiffConfig {
            start: settings.start_difficulty,
            min: settings.min_difficulty,
            max: settings.max_difficulty,
            share_target_secs: settings.share_target_secs,
        },
    ));

    let (job_tx, job_rx) = watch::channel(None);
    let request = chain.template_request(&coin.algo);
    tokio::spawn(template::run(rpc.clone(), pool.clone(), request, payout_script, job_tx));
    let ctx = Server { pool, rpc, db, coin_id, recorder: recorder.clone(), jobs: job_rx };

    let listener = TcpListener::bind(&settings.listen).await.with_context(|| format!("binding {}", settings.listen))?;
    info!(listen = %settings.listen, coin = %coin.symbol, algo = %coin.algo, "stratum listening");
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
    Ok(recorder)
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
    let mut jobs = ctx.jobs.clone();
    stream.set_nodelay(true)?;
    let mut lines = Framed::new(stream, LinesCodec::new_with_max_length(MAX_LINE_BYTES));
    let mut session = Session::new(ctx.pool.clone(), Instant::now());
    let mut tick = tokio::time::interval(VARDIFF_TICK);
    let mut last_message = tokio::time::Instant::now();
    debug!(%peer, "connected");

    loop {
        let outgoing = tokio::select! {
            line = lines.next() => {
                let Some(line) = line else { return Ok(()) };
                last_message = tokio::time::Instant::now();
                let current = jobs.borrow().clone();
                let mut out = session.handle_line(&line?, current.as_deref(), Instant::now());
                if let Some(share) = out.share {
                    record_share(&ctx, share);
                }
                if let Some(request) = out.auth {
                    let worker = accounts::resolve(&ctx.db, &ctx.rpc, ctx.coin_id, &request.user, &request.password).await;
                    let worker = worker.unwrap_or_else(|err| {
                        warn!(%peer, "authorize failed: {err:#}");
                        None
                    });
                    out.lines.push(session.finish_authorize(request, worker));
                }
                out.lines
            }
            changed = jobs.changed() => {
                changed?;
                let job = jobs.borrow_and_update().clone();
                job.and_then(|job| session.on_job(&job)).into_iter().collect()
            }
            _ = tick.tick() => {
                let current = jobs.borrow().clone();
                session.on_tick(current.as_deref(), Instant::now())
            }
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

fn record_share(ctx: &Server, share: AcceptedShare) {
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    let AcceptedShare { worker, difficulty, block } = share;
    let WorkerIds { account_id, worker_id, solo, party_id } = worker;
    ctx.recorder.share(ShareEvent { account_id, worker_id, solo, party_id, difficulty, at });
    if let Some(block) = block {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if submit_block(&ctx.rpc, &block).await {
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
                ctx.recorder.block(event).await;
            }
            ctx.pool.refresh.notify_one();
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
