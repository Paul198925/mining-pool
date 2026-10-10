use std::time::Duration;

use anyhow::{Result, ensure};
use pool_common::config::Instance;
use sqlx::PgPool;
use tracing::warn;

const HEARTBEAT: Duration = Duration::from_secs(30);
/// An instance that has not reported for this long counts as gone. The website uses the same limit.
const STALE_AFTER_SECS: f64 = 120.0;

/// Claims this process's instance id and keeps the claim alive.
///
/// Fails if another server is currently running with the same id: two processes with
/// one id would hand out identical extranonces, and their miners would duplicate work.
pub async fn register(db: &PgPool, instance: &Instance) -> Result<()> {
    let claimed = sqlx::query(
        "INSERT INTO stratum_instances (id, region, host) VALUES ($1, $2, $3)
         ON CONFLICT (id) DO UPDATE SET region = EXCLUDED.region, host = EXCLUDED.host, last_seen_at = now()
         WHERE stratum_instances.host = EXCLUDED.host
            OR stratum_instances.last_seen_at < now() - make_interval(secs => $4)",
    )
    .bind(i16::from(instance.id))
    .bind(&instance.region)
    .bind(&instance.host)
    .bind(STALE_AFTER_SECS)
    .execute(db)
    .await?;
    ensure!(
        claimed.rows_affected() == 1,
        "instance id {} is in use by another running stratum server; give every server its own POOL_INSTANCE_ID",
        instance.id
    );

    let (db, id) = (db.clone(), i16::from(instance.id));
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(HEARTBEAT);
        loop {
            tick.tick().await;
            if let Err(err) = sqlx::query("UPDATE stratum_instances SET last_seen_at = now() WHERE id = $1")
                .bind(id)
                .execute(&db)
                .await
            {
                warn!("instance heartbeat failed: {err}");
            }
        }
    });
    Ok(())
}

/// Takes the server off the website right away on a clean shutdown.
pub async fn unregister(db: &PgPool, instance: &Instance) {
    let _ = sqlx::query("UPDATE stratum_instances SET last_seen_at = to_timestamp(0) WHERE id = $1")
        .bind(i16::from(instance.id))
        .execute(db)
        .await;
}
