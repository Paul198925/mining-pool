use anyhow::Result;
use pool_common::{
    config::{Accounting, Coin},
    rpc::RpcClient,
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// Makes the coin's payout settings visible to the website.
pub async fn publish_settings(db: &PgPool, coin_id: i32, coin: &Coin, accounting: &Accounting) -> Result<()> {
    sqlx::query(
        "UPDATE coins SET block_maturity = $2, min_payout = $3, pool_fee_percent = $4, solo_fee_percent = $5 WHERE id = $1",
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
    sqlx::query(
        "UPDATE coins SET height = $2, network_difficulty = $3, network_hashrate = $4, stats_at = now() WHERE id = $1",
    )
    .bind(coin_id)
    .bind(info["blocks"].as_i64())
    .bind(per_algo("difficulty"))
    .bind(per_algo("networkhashps"))
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
