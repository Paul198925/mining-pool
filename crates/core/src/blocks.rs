use anyhow::{Result, ensure};
use pool_common::{
    config::{Accounting, Coin},
    rpc::RpcClient,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing::{info, warn};

use crate::pplns::split_reward;

/// Give the stratum servers time to flush shares that were found just before the block.
const CREDIT_DELAY_SECS: f64 = 5.0;

/// Computes the reward split for newly found blocks.
///
/// A block found by a solo miner goes to that miner alone. Everything else is PPLNS over
/// the shares of the group the block belongs to: the finder's party, or the public pool.
/// The window is that group's most recent shares before the block, going back until their
/// difficulties add up to `pplns_factor` times the network difficulty. The oldest
/// share in the window only counts with the part that still fits.
pub async fn credit_new_blocks(db: &PgPool, coin_id: i32, accounting: &Accounting) -> Result<()> {
    let blocks: Vec<(i64, i64, i64, f64, bool, i64)> = sqlx::query_as(
        "SELECT id, height, reward, network_difficulty, solo, account_id FROM blocks
         WHERE coin_id = $1 AND status = 'pending' AND NOT credited
           AND found_at < now() - make_interval(secs => $2)
         ORDER BY id",
    )
    .bind(coin_id)
    .bind(CREDIT_DELAY_SECS)
    .fetch_all(db)
    .await?;

    for (block_id, height, reward, network_difficulty, solo, finder) in blocks {
        if solo {
            let credits = split_reward(reward, accounting.solo_fee_percent, &[(finder, 1.0)]);
            store_credits(db, block_id, &credits).await?;
            info!(height, "solo block reward credited");
            continue;
        }
        let weights: Vec<(i64, f64)> = sqlx::query_as(
            "SELECT account_id, SUM(LEAST(difficulty, $3 - (cumulative - difficulty)))
             FROM (
                 SELECT s.account_id, s.difficulty,
                        SUM(s.difficulty) OVER (ORDER BY s.created_at DESC, s.id DESC) AS cumulative
                 FROM shares s, blocks b
                 WHERE b.id = $2 AND s.coin_id = $1 AND NOT s.solo
                   AND s.party_id IS NOT DISTINCT FROM b.party_id
                   AND s.created_at <= b.found_at AND s.created_at > b.found_at - interval '7 days'
             ) window_shares
             WHERE cumulative - difficulty < $3
             GROUP BY account_id",
        )
        .bind(coin_id)
        .bind(block_id)
        .bind(accounting.pplns_factor * network_difficulty)
        .fetch_all(db)
        .await?;

        let credits = split_reward(reward, accounting.pool_fee_percent, &weights);
        store_credits(db, block_id, &credits).await?;
        info!(height, miners = credits.len(), "block reward split");
    }
    Ok(())
}

async fn store_credits(db: &PgPool, block_id: i64, credits: &[(i64, i64)]) -> Result<()> {
    let mut tx = db.begin().await?;
    for (account_id, amount) in credits {
        sqlx::query("INSERT INTO block_credits (block_id, account_id, amount) VALUES ($1, $2, $3)")
            .bind(block_id)
            .bind(account_id)
            .bind(amount)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE blocks SET credited = TRUE WHERE id = $1").bind(block_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Follows pending blocks on chain: books the reward once it is spendable, or marks the block orphaned.
pub async fn update_confirmations(db: &PgPool, rpc: &RpcClient, coin_id: i32, coin: &Coin) -> Result<()> {
    let blocks: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT id, height, hash FROM blocks WHERE coin_id = $1 AND status = 'pending' AND credited ORDER BY id",
    )
    .bind(coin_id)
    .fetch_all(db)
    .await?;

    for (block_id, height, hash) in blocks {
        let info: Value = match rpc.call("getblock", json!([hash])).await {
            Ok(info) => info,
            Err(err) => {
                warn!(height, %hash, "cannot look up block: {err:#}");
                continue;
            }
        };
        let Some(confirmations) = info["confirmations"].as_i64() else {
            warn!(height, %hash, "getblock returned no confirmations");
            continue;
        };

        if confirmations < 0 {
            // The node knows the block but it is not on the best chain.
            sqlx::query("UPDATE blocks SET status = 'orphaned', confirmations = 0 WHERE id = $1 AND status = 'pending'")
                .bind(block_id)
                .execute(db)
                .await?;
            warn!(height, %hash, "block orphaned");
        } else if confirmations > coin.block_maturity {
            confirm(db, coin_id, block_id, confirmations).await?;
            info!(height, %hash, "block confirmed, reward booked");
        } else {
            sqlx::query("UPDATE blocks SET confirmations = $2 WHERE id = $1")
                .bind(block_id)
                .bind(confirmations as i32)
                .execute(db)
                .await?;
        }
    }
    Ok(())
}

/// Moves a matured block's reward into the ledger, atomically with the status change.
async fn confirm(db: &PgPool, coin_id: i32, block_id: i64, confirmations: i64) -> Result<()> {
    let mut tx = db.begin().await?;
    let updated = sqlx::query(
        "UPDATE blocks SET status = 'confirmed', confirmations = $2 WHERE id = $1 AND status = 'pending' AND credited",
    )
    .bind(block_id)
    .bind(confirmations as i32)
    .execute(&mut *tx)
    .await?;
    ensure!(updated.rows_affected() == 1, "block {block_id} is not pending");

    // One transfer: the reward enters from 'mined', miners get their credits, the rest is the fee.
    sqlx::query(
        "WITH transfer AS (SELECT nextval('ledger_transfer_id') AS id),
              block AS (SELECT reward FROM blocks WHERE id = $2),
              credits AS (SELECT account_id, amount FROM block_credits WHERE block_id = $2),
              legs (account_id, system_account, amount) AS (
                  SELECT NULL::bigint, 'mined', -reward FROM block
                  UNION ALL
                  SELECT account_id, NULL, amount FROM credits
                  UNION ALL
                  SELECT NULL, 'pool_fee', reward - (SELECT COALESCE(SUM(amount), 0) FROM credits) FROM block
              )
         INSERT INTO ledger_entries (transfer_id, coin_id, account_id, system_account, amount, kind, block_id)
         SELECT transfer.id, $1, legs.account_id, legs.system_account, legs.amount, 'block_reward', $2
         FROM legs, transfer
         WHERE legs.amount <> 0",
    )
    .bind(coin_id)
    .bind(block_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
