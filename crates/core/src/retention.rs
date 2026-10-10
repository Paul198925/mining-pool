//! Deletes what is no longer needed, so that the database does not grow without end.

use std::time::Duration;

use anyhow::Result;
use sqlx::PgPool;
use tracing::{info, warn};

const INTERVAL: Duration = Duration::from_secs(3600);
/// Rows per statement: small enough not to hold locks for long.
const BATCH: i64 = 20_000;

/// Shares older than what any block can still need. PPLNS looks back seven days from a
/// block, so eight days are safe once every older block has been split. If a block is
/// still waiting for that (pool-core was down), its window is kept.
async fn prune_shares(db: &PgPool) -> Result<u64> {
    let mut deleted = 0;
    loop {
        let rows = sqlx::query(
            "DELETE FROM shares WHERE id IN (
                 SELECT id FROM shares
                 WHERE created_at < LEAST(
                     now() - interval '8 days',
                     COALESCE((SELECT min(found_at) FROM blocks WHERE NOT credited AND status <> 'orphaned'), now())
                         - interval '8 days')
                 ORDER BY created_at
                 LIMIT $1)",
        )
        .bind(BATCH)
        .execute(db)
        .await?
        .rows_affected();
        deleted += rows;
        if rows < BATCH as u64 {
            return Ok(deleted);
        }
        // Let other work in between.
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Batch ids only guard against a server repeating a delivery, which it does within minutes.
async fn prune_batches(db: &PgPool) -> Result<u64> {
    Ok(sqlx::query("DELETE FROM node_batches WHERE received_at < now() - interval '2 days'")
        .execute(db)
        .await?
        .rows_affected())
}

pub async fn run(db: PgPool) {
    let mut tick = tokio::time::interval(INTERVAL);
    loop {
        tick.tick().await;
        match prune_shares(&db).await {
            Ok(0) => {}
            Ok(deleted) => info!(deleted, "old shares removed"),
            Err(err) => warn!("removing old shares failed: {err:#}"),
        }
        if let Err(err) = prune_batches(&db).await {
            warn!("removing old batch ids failed: {err:#}");
        }
    }
}
