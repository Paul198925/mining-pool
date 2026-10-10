mod algo;
mod job;
mod pool;
mod session;
mod template;
mod util;
mod vardiff;

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, ensure};
use futures_util::{SinkExt, StreamExt};
use pool_common::{
    config::{Coin, Config},
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
    algo::Algo,
    job::{FoundBlock, Job},
    pool::Pool,
    session::Session,
    vardiff::VarDiffConfig,
};

const MAX_LINE_BYTES: usize = 16 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const VARDIFF_TICK: Duration = Duration::from_secs(30);

type JobFeed = watch::Receiver<Option<Arc<Job>>>;

#[tokio::main]
async fn main() -> Result<()> {
    pool_common::init_logging();
    let cfg = Config::load()?;
    let coin = cfg
        .coins
        .iter()
        .find(|c| c.symbol == cfg.stratum.coin)
        .ok_or_else(|| anyhow!("stratum.coin {} is not configured under [[coins]]", cfg.stratum.coin))?;

    let rpc = RpcClient::new(coin)?;
    let payout_script = payout_script(&rpc, coin).await?;
    let pool = Arc::new(Pool::new(
        Algo::from_name(&coin.algo)?,
        VarDiffConfig {
            start: cfg.stratum.start_difficulty,
            min: cfg.stratum.min_difficulty,
            max: cfg.stratum.max_difficulty,
            share_target_secs: cfg.stratum.share_target_secs,
        },
    ));

    let (job_tx, job_rx) = watch::channel(None);
    tokio::spawn(template::run(rpc.clone(), pool.clone(), payout_script, job_tx));

    let listener = TcpListener::bind(&cfg.stratum.listen)
        .await
        .with_context(|| format!("binding {}", cfg.stratum.listen))?;
    info!(listen = %cfg.stratum.listen, coin = %coin.symbol, algo = %coin.algo, "stratum listening");

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                let (pool, rpc, jobs) = (pool.clone(), rpc.clone(), job_rx.clone());
                tokio::spawn(async move {
                    if let Err(err) = serve(stream, peer, pool, rpc, jobs).await {
                        debug!(%peer, "connection closed: {err:#}");
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    info!("shutting down");
    Ok(())
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

async fn serve(stream: TcpStream, peer: SocketAddr, pool: Arc<Pool>, rpc: RpcClient, mut jobs: JobFeed) -> Result<()> {
    stream.set_nodelay(true)?;
    let mut lines = Framed::new(stream, LinesCodec::new_with_max_length(MAX_LINE_BYTES));
    let mut session = Session::new(pool.clone(), Instant::now());
    let mut tick = tokio::time::interval(VARDIFF_TICK);
    let mut last_message = tokio::time::Instant::now();
    debug!(%peer, "connected");

    loop {
        let outgoing = tokio::select! {
            line = lines.next() => {
                let Some(line) = line else { return Ok(()) };
                last_message = tokio::time::Instant::now();
                let current = jobs.borrow().clone();
                let out = session.handle_line(&line?, current.as_deref(), Instant::now());
                if let Some(block) = out.block {
                    tokio::spawn(submit_block(rpc.clone(), pool.clone(), block));
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

async fn submit_block(rpc: RpcClient, pool: Arc<Pool>, block: FoundBlock) {
    // submitblock answers null on success and a reason string on rejection.
    match rpc.call::<Value>("submitblock", json!([block.hex])).await {
        Ok(Value::Null) => info!(height = block.height, hash = %block.hash, "block accepted"),
        // Valid, but the node already has another block at this height: we lost a race.
        Ok(Value::String(reason)) if reason == "inconclusive" || reason == "duplicate" => {
            warn!(height = block.height, hash = %block.hash, %reason, "block came too late")
        }
        Ok(reason) => error!(height = block.height, hash = %block.hash, %reason, "block rejected"),
        Err(err) => error!(height = block.height, hash = %block.hash, "block submission failed: {err:#}"),
    }
    pool.refresh.notify_one();
}
