use anyhow::{Context, Result};
use pool_common::{
    config::Coin,
    rpc::{DaemonError, RpcClient},
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::wallet;

const UNITS_PER_COIN: i64 = 100_000_000;
const MAX_BATCH: i64 = 200;
/// How far back to search the wallet when a send has to be reconciled.
const RECONCILE_LOOKBACK: u32 = 1000;

/// One payout round. A payout moves through three states, and money can only leave once:
///
/// 1. `pending`: the miner's balance was debited in the ledger in the same transaction
///    that created the payout, so it cannot be paid twice from the same balance.
/// 2. `sending`: set, with a batch id, *before* the wallet is asked to send. If we crash
///    or lose the connection here, the next round looks the batch up in the wallet by
///    its comment instead of sending again blindly.
/// 3. `sent`: the transaction id is recorded.
///
/// The wallet takes the transaction fee out of the amounts it sends, split evenly
/// between the recipients, so the pool never pays for payouts.
pub async fn run(db: &PgPool, rpc: &RpcClient, coin_id: i32, coin: &Coin) -> Result<()> {
    // Always: a batch that was already handed to the wallet must be accounted for.
    reconcile(db, rpc, coin_id).await?;
    let enabled: bool =
        sqlx::query_scalar("SELECT payouts_enabled FROM coins WHERE id = $1").bind(coin_id).fetch_one(db).await?;
    if !enabled {
        // Switched off in the admin area: nothing new is queued or sent, balances stay.
        return Ok(());
    }
    create(db, coin_id).await?;
    send(db, rpc, coin_id, coin).await
}

fn batch_comment(batch_id: i64) -> String {
    format!("pool-payout-batch-{batch_id}")
}

/// Settles batches whose send was started but never recorded.
async fn reconcile(db: &PgPool, rpc: &RpcClient, coin_id: i32) -> Result<()> {
    let batches: Vec<i64> =
        sqlx::query_scalar("SELECT DISTINCT batch_id FROM payouts WHERE coin_id = $1 AND status = 'sending'")
            .bind(coin_id)
            .fetch_all(db)
            .await?;
    if batches.is_empty() {
        return Ok(());
    }
    let wallet_txs: Vec<Value> = rpc.call("listtransactions", json!(["*", RECONCILE_LOOKBACK])).await?;
    for batch_id in batches {
        let comment = batch_comment(batch_id);
        let txid = wallet_txs.iter().find(|tx| tx["comment"] == comment.as_str()).and_then(|tx| tx["txid"].as_str());
        match txid {
            Some(txid) => {
                warn!(batch_id, txid, "payout batch was sent after all, recording it");
                mark_sent(db, rpc, coin_id, batch_id, txid).await?;
            }
            None => {
                warn!(batch_id, "payout batch never reached the wallet, queueing it again");
                sqlx::query("UPDATE payouts SET status = 'pending', batch_id = NULL WHERE batch_id = $1 AND status = 'sending'")
                    .bind(batch_id)
                    .execute(db)
                    .await?;
            }
        }
    }
    Ok(())
}

/// Turns every balance above the minimum into a pending payout, debiting the ledger.
async fn create(db: &PgPool, coin_id: i32) -> Result<()> {
    // Read for every round: the minimum can be changed in the admin area while the pool runs.
    let min_units: Option<i64> =
        sqlx::query_scalar("SELECT min_payout FROM coins WHERE id = $1").bind(coin_id).fetch_one(db).await?;
    let min_units = min_units.context("the coin has no minimum payout set")?.max(1);
    let mut tx = db.begin().await?;
    let due: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT b.account_id, b.amount, a.payout_address
         FROM account_balances b JOIN accounts a ON a.id = b.account_id
         WHERE b.coin_id = $1 AND a.payout_coin_id = $1 AND b.amount >= $2",
    )
    .bind(coin_id)
    .bind(min_units)
    .fetch_all(&mut *tx)
    .await?;

    for (account_id, amount, address) in &due {
        let payout_id: i64 = sqlx::query_scalar(
            "INSERT INTO payouts (coin_id, account_id, address, amount) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(coin_id)
        .bind(account_id)
        .bind(address)
        .bind(amount)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "WITH transfer AS (SELECT nextval('ledger_transfer_id') AS id)
             INSERT INTO ledger_entries (transfer_id, coin_id, account_id, system_account, amount, kind, payout_id)
             SELECT transfer.id, $1, legs.account_id, legs.system_account, legs.amount, 'payout', $4
             FROM transfer, (VALUES ($2::bigint, NULL, -$3::bigint), (NULL, 'paid_out', $3)) AS legs (account_id, system_account, amount)",
        )
        .bind(coin_id)
        .bind(account_id)
        .bind(amount)
        .bind(payout_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn send(db: &PgPool, rpc: &RpcClient, coin_id: i32, coin: &Coin) -> Result<()> {
    let pending: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT id, address, amount FROM payouts WHERE coin_id = $1 AND status = 'pending' ORDER BY id LIMIT $2",
    )
    .bind(coin_id)
    .bind(MAX_BATCH)
    .fetch_all(db)
    .await?;
    if pending.is_empty() {
        return Ok(());
    }
    // Before anything is marked as sending: if the wallet cannot be unlocked, nothing has happened yet.
    wallet::unlock(rpc, coin).await?;

    let batch_id: i64 = sqlx::query_scalar("SELECT nextval('payout_batch_id')").fetch_one(db).await?;
    let ids: Vec<i64> = pending.iter().map(|(id, ..)| *id).collect();
    sqlx::query("UPDATE payouts SET status = 'sending', batch_id = $1 WHERE id = ANY($2) AND status = 'pending'")
        .bind(batch_id)
        .bind(&ids)
        .execute(db)
        .await?;

    // An account has exactly one address per coin, so addresses are unique within a batch.
    let amounts: Map<String, Value> =
        pending.iter().map(|(_, address, amount)| (address.clone(), json!(format_amount(*amount)))).collect();
    let total: i64 = pending.iter().map(|(.., amount)| amount).sum();

    // The last argument lists the outputs the fee is subtracted from: all of them.
    let recipients: Vec<&String> = amounts.keys().collect();
    let sent = rpc.call::<String>("sendmany", json!(["", amounts, 1, batch_comment(batch_id), recipients])).await;
    wallet::lock(rpc, coin).await;
    match sent {
        Ok(txid) => {
            info!(batch_id, %txid, payouts = pending.len(), total = %format_amount(total), "payouts sent");
            mark_sent(db, rpc, coin_id, batch_id, &txid).await
        }
        // The wallet refused (e.g. not enough mature funds): nothing was sent, try again next round.
        Err(err) if err.downcast_ref::<DaemonError>().is_some() => {
            warn!(batch_id, "wallet refused payout batch: {err:#}");
            sqlx::query("UPDATE payouts SET status = 'pending', batch_id = NULL WHERE batch_id = $1 AND status = 'sending'")
                .bind(batch_id)
                .execute(db)
                .await?;
            Ok(())
        }
        // No answer: the batch may or may not be out. It stays 'sending' for reconcile().
        Err(err) => {
            error!(batch_id, "payout batch outcome unknown: {err:#}");
            Ok(())
        }
    }
}

async fn mark_sent(db: &PgPool, rpc: &RpcClient, coin_id: i32, batch_id: i64, txid: &str) -> Result<()> {
    sqlx::query("UPDATE payouts SET status = 'sent', txid = $2, sent_at = now() WHERE batch_id = $1 AND status = 'sending'")
        .bind(batch_id)
        .bind(txid)
        .execute(db)
        .await?;
    if let Err(err) = book_network_fee(db, rpc, coin_id, txid).await {
        warn!(batch_id, txid, "could not book the network fee: {err:#}");
    }
    Ok(())
}

/// Records what the miners actually received: the fee moves from 'paid_out' to 'network_fee'.
async fn book_network_fee(db: &PgPool, rpc: &RpcClient, coin_id: i32, txid: &str) -> Result<()> {
    let tx: Value = rpc.call("gettransaction", json!([txid])).await?;
    let to_units = |coins: &Value| (coins.as_f64().unwrap_or(0.0).abs() * UNITS_PER_COIN as f64).round() as i64;
    let fee = to_units(&tx["fee"]);
    if fee == 0 {
        return Ok(());
    }
    // Per payout: what was debited minus what the wallet says it sent to that address.
    for detail in tx["details"].as_array().into_iter().flatten().filter(|d| d["category"] == "send") {
        sqlx::query("UPDATE payouts SET network_fee = amount - $3 WHERE txid = $1 AND address = $2")
            .bind(txid)
            .bind(detail["address"].as_str())
            .bind(to_units(&detail["amount"]))
            .execute(db)
            .await?;
    }
    sqlx::query(
        "WITH transfer AS (SELECT nextval('ledger_transfer_id') AS id)
         INSERT INTO ledger_entries (transfer_id, coin_id, system_account, amount, kind)
         SELECT transfer.id, $1, legs.system_account, legs.amount, 'network_fee'
         FROM transfer, (VALUES ('paid_out', -$2::bigint), ('network_fee', $2)) AS legs (system_account, amount)",
    )
    .bind(coin_id)
    .bind(fee)
    .execute(db)
    .await?;
    Ok(())
}

/// Exact decimal rendering; amounts never pass through floating point on their way to the wallet.
fn format_amount(units: i64) -> String {
    format!("{}.{:08}", units / UNITS_PER_COIN, units % UNITS_PER_COIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_are_rendered_exactly() {
        assert_eq!(format_amount(1), "0.00000001");
        assert_eq!(format_amount(123_456_789_012), "1234.56789012");
        assert_eq!(format_amount(500_000 * UNITS_PER_COIN), "500000.00000000");
    }
}
