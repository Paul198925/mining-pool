use anyhow::Result;
use pool_common::{
    config::{Accounting, Coin},
    rpc::{DaemonError, RpcClient},
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Makes the coin's payout settings visible to the website. Fees and the minimum payout
/// from the config file are only starting values: after that the admin area owns them.
pub async fn publish_settings(db: &PgPool, coin_id: i32, coin: &Coin, accounting: &Accounting) -> Result<()> {
    sqlx::query(
        "UPDATE coins SET block_maturity = $2, min_payout = COALESCE(min_payout, $3),
                          pool_fee_percent = COALESCE(pool_fee_percent, $4),
                          solo_fee_percent = COALESCE(solo_fee_percent, $5)
         WHERE id = $1",
    )
    .bind(coin_id)
    .bind(coin.block_maturity as i32)
    .bind((coin.min_payout * 1e8).round() as i64)
    .bind(accounting.pool_fee_percent)
    .bind(accounting.solo_fee_percent)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn refresh_chain_stats(db: &PgPool, rpc: &RpcClient, coin_id: i32, coin: &Coin) -> Result<()> {
    let info: Value = rpc.call("getmininginfo", json!([])).await?;
    // Multi-algo chains such as Xaya report one value per algorithm.
    let algo = coin.algo.trim_end_matches("-xaya");
    let per_algo = |field: &str| info[field].as_f64().or_else(|| info[field][algo].as_f64());
    // A node that is still catching up reports its height as it goes; headers and peers
    // show whether it is making progress at all.
    let chain: Value = rpc.call("getblockchaininfo", json!([])).await?;
    let peers: i64 = rpc.call("getconnectioncount", json!([])).await?;
    sqlx::query(
        "UPDATE coins SET height = $2, network_difficulty = $3, network_hashrate = $4, stats_at = now(),
                          headers = $5, sync_progress = $6, peers = $7
         WHERE id = $1",
    )
    .bind(coin_id)
    .bind(info["blocks"].as_i64())
    .bind(per_algo("difficulty"))
    .bind(per_algo("networkhashps"))
    .bind(chain["headers"].as_i64())
    .bind(chain["verificationprogress"].as_f64())
    .bind(peers as i32)
    .execute(db)
    .await?;
    Ok(())
}

/// Publishes the wallet's spendable balance for the admin area.
pub async fn refresh_wallet_balance(db: &PgPool, rpc: &RpcClient, coin_id: i32) -> Result<()> {
    let balance: f64 = rpc.call("getbalance", json!([])).await?;
    sqlx::query("UPDATE coins SET wallet_balance = $2, wallet_checked_at = now() WHERE id = $1")
        .bind(coin_id)
        .bind((balance * 1e8).round() as i64)
        .execute(db)
        .await?;
    Ok(())
}

/// Makes sure the coin has a reward address, taking a fresh one from the pool wallet if
/// none is set yet. Self-enrolling servers get the address from here.
/// Returns whether the coin has an address now.
pub async fn publish_pool_address(db: &PgPool, rpc: &RpcClient, coin_id: i32, coin: &Coin) -> Result<bool> {
    let stored: Option<String> =
        sqlx::query_scalar("SELECT pool_address FROM coins WHERE id = $1").bind(coin_id).fetch_one(db).await?;
    let address = match (&coin.pool_address, stored) {
        // An address from the configuration always wins.
        (Some(configured), stored) if stored.as_ref() != Some(configured) => configured.clone(),
        (_, Some(_)) => return Ok(true),
        (_, None) => match rpc.call("getnewaddress", json!(["pool", "legacy"])).await {
            Ok(address) => address,
            // Older daemons know no address types.
            Err(err) if err.downcast_ref::<DaemonError>().is_some() => rpc.call("getnewaddress", json!(["pool"])).await?,
            Err(err) => return Err(err),
        },
    };
    sqlx::query("UPDATE coins SET pool_address = $2 WHERE id = $1").bind(coin_id).bind(&address).execute(db).await?;
    tracing::info!(coin = %coin.symbol, %address, "pool address published");
    Ok(true)
}
