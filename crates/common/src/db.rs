use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::config::{Coin, Config};

/// Connects and brings the schema up to date. Safe to call from several services at once.
pub async fn connect(cfg: &Config) -> Result<PgPool> {
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&cfg.database.url)
        .await
        .context("connecting to database")?;
    sqlx::migrate!("../../migrations").run(&db).await.context("running migrations")?;
    Ok(db)
}

/// Registers the coin if needed and returns its id.
pub async fn ensure_coin(db: &PgPool, coin: &Coin) -> Result<i32> {
    let id = sqlx::query_scalar(
        "INSERT INTO coins (symbol, name, algo) VALUES ($1, $2, $3)
         ON CONFLICT (symbol) DO UPDATE SET name = EXCLUDED.name, algo = EXCLUDED.algo
         RETURNING id",
    )
    .bind(&coin.symbol)
    .bind(&coin.name)
    .bind(&coin.algo)
    .fetch_one(db)
    .await?;
    Ok(id)
}
