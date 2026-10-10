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

pub async fn refresh_chain_stats(db: &PgPool, rpc: &RpcClient, coin_id: i32) -> Result<()> {
    let info: Value = rpc.call("getmininginfo", json!([])).await?;
    sqlx::query(
        "UPDATE coins SET height = $2, network_difficulty = $3, network_hashrate = $4, stats_at = now() WHERE id = $1",
    )
    .bind(coin_id)
    .bind(info["blocks"].as_i64())
    .bind(info["difficulty"].as_f64())
    .bind(info["networkhashps"].as_f64())
    .execute(db)
    .await?;
    Ok(())
}
