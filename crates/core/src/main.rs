use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result};
use pool_common::{config::Config, rpc::RpcClient};
use sqlx::postgres::PgPoolOptions;
use tracing::{info, warn};

const POLL_INTERVAL: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> Result<()> {
    pool_common::init_logging();
    let cfg = Config::load()?;

    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&cfg.database.url)
        .await
        .context("connecting to database")?;
    sqlx::migrate!("../../migrations").run(&db).await.context("running migrations")?;
    info!("database ready");

    let mut daemons = Vec::new();
    for coin in &cfg.coins {
        sqlx::query(
            "INSERT INTO coins (symbol, name, algo) VALUES ($1, $2, $3)
             ON CONFLICT (symbol) DO UPDATE SET name = EXCLUDED.name, algo = EXCLUDED.algo",
        )
        .bind(&coin.symbol)
        .bind(&coin.name)
        .bind(&coin.algo)
        .execute(&db)
        .await?;
        daemons.push((coin.symbol.clone(), RpcClient::new(coin)?));
    }

    // Placeholder for the block tracker: just follow each daemon's chain tip.
    let mut heights: HashMap<String, u64> = HashMap::new();
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = tokio::signal::ctrl_c() => break,
        }
        for (symbol, rpc) in &daemons {
            match rpc.blockchain_info().await {
                Ok(tip) => {
                    if heights.insert(symbol.clone(), tip.blocks) != Some(tip.blocks) {
                        info!(coin = %symbol, chain = %tip.chain, height = tip.blocks, hash = %tip.bestblockhash, "chain tip");
                    }
                }
                Err(err) => warn!(coin = %symbol, "daemon unreachable: {err:#}"),
            }
        }
    }
    info!("shutting down");
    Ok(())
}
