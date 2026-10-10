mod blocks;
mod payouts;
mod pplns;
mod stats;

use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use pool_common::{
    config::{Accounting, Coin, Config},
    rpc::RpcClient,
};
use sqlx::PgPool;
use tracing::{error, info, warn};

const BLOCK_INTERVAL: Duration = Duration::from_secs(10);
/// Arbitrary but fixed: identifies "the pool-core instance" among Postgres advisory locks.
const SINGLETON_LOCK: i64 = 0x706f_6f6c_636f_7265;

#[tokio::main]
async fn main() -> Result<()> {
    pool_common::init_logging();
    let cfg = Config::load()?;
    let db = pool_common::db::connect(&cfg).await?;

    // Two instances paying out at once could pay twice. The lock lives as long as this connection.
    let mut lock = db.acquire().await?;
    let locked: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(SINGLETON_LOCK).fetch_one(&mut *lock).await?;
    ensure!(locked, "another pool-core instance is already running against this database");
    info!("database ready");

    for coin in &cfg.coins {
        let coin_id = pool_common::db::ensure_coin(&db, coin).await?;
        tokio::spawn(run_coin(db.clone(), coin_id, coin.clone(), cfg.accounting.clone()));
    }

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    Ok(())
}

/// All accounting for one coin runs in this single loop, so its steps never overlap.
async fn run_coin(db: PgPool, coin_id: i32, coin: Coin, accounting: Accounting) {
    let rpc = match RpcClient::new(&coin) {
        Ok(rpc) => rpc,
        Err(err) => return error!(coin = %coin.symbol, "cannot create rpc client: {err:#}"),
    };
    let payout_interval = Duration::from_secs(accounting.payout_interval_secs);
    let mut last_payout = Instant::now();
    let mut tick = tokio::time::interval(BLOCK_INTERVAL);
    if let Err(err) = stats::publish_settings(&db, coin_id, &coin, &accounting).await {
        error!(coin = %coin.symbol, "publishing settings failed: {err:#}");
    }
    info!(coin = %coin.symbol, "accounting started");

    loop {
        tick.tick().await;
        if let Err(err) = stats::refresh_chain_stats(&db, &rpc, coin_id).await {
            warn!(coin = %coin.symbol, "refreshing chain stats failed: {err:#}");
        }
        if let Err(err) = blocks::credit_new_blocks(&db, coin_id, &accounting).await {
            error!(coin = %coin.symbol, "crediting blocks failed: {err:#}");
        }
        if let Err(err) = blocks::update_confirmations(&db, &rpc, coin_id, &coin).await {
            error!(coin = %coin.symbol, "updating confirmations failed: {err:#}");
        }
        if last_payout.elapsed() >= payout_interval {
            last_payout = Instant::now();
            if let Err(err) = payouts::run(&db, &rpc, coin_id, &coin).await {
                error!(coin = %coin.symbol, "payout round failed: {err:#}");
            }
        }
    }
}
