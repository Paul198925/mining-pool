use anyhow::Result;
use pool_common::rpc::RpcClient;
use serde_json::{Value, json};
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerIds {
    pub account_id: i64,
    pub worker_id: i64,
}

/// Resolves a stratum username of the form `<payout address>[.<worker name>]`.
/// Returns `None` if the address is not valid for the coin.
pub async fn resolve(db: &PgPool, rpc: &RpcClient, coin_id: i32, user: &str) -> Result<Option<WorkerIds>> {
    let (address, worker) = user.split_once('.').unwrap_or((user, ""));
    // Cheap rejection of junk before asking the daemon.
    let plausible = (20..=100).contains(&address.len()) && address.chars().all(|c| c.is_ascii_alphanumeric());
    if !plausible || worker.len() > 64 {
        return Ok(None);
    }
    let info: Value = rpc.call("validateaddress", json!([address])).await?;
    if info["isvalid"] != true {
        return Ok(None);
    }

    let account_id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (payout_coin_id, payout_address) VALUES ($1, $2)
         ON CONFLICT (payout_coin_id, payout_address) DO UPDATE SET payout_address = EXCLUDED.payout_address
         RETURNING id",
    )
    .bind(coin_id)
    .bind(address)
    .fetch_one(db)
    .await?;
    let worker_id: i64 = sqlx::query_scalar(
        "INSERT INTO workers (account_id, name, last_seen_at) VALUES ($1, $2, now())
         ON CONFLICT (account_id, name) DO UPDATE SET last_seen_at = now()
         RETURNING id",
    )
    .bind(account_id)
    .bind(worker)
    .fetch_one(db)
    .await?;
    Ok(Some(WorkerIds { account_id, worker_id }))
}
