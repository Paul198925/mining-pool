mod blocks;
mod exchange;
mod payouts;
mod pplns;
mod prices;
mod retention;
mod stats;
mod wallet;

use std::{
    env,
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};
use pool_common::{
    config::{Accounting, Coin, Config},
    rpc::RpcClient,
};
use sqlx::PgPool;
use tokio::{sync::watch, task::JoinSet};
use tracing::{error, info, warn};

const BLOCK_INTERVAL: Duration = Duration::from_secs(10);
const PRICE_INTERVAL: Duration = Duration::from_secs(300);
const WALLET_CHECK_INTERVAL: Duration = Duration::from_secs(60);
/// The servers' private network; a server's last address byte is its id (see the web app's mesh.ts).
const MESH_NETWORK: &str = "10.77.0";

/// Which enrolled server holds the pool wallet of which coin, as set in the admin area.
async fn wallet_servers(db: &PgPool) -> Result<Vec<(String, i32)>> {
    Ok(sqlx::query_as(
        "SELECT wallet, id::int4 FROM stratum_instances, unnest(wallets) AS wallet
         WHERE token_hash IS NOT NULL ORDER BY wallet, id",
    )
    .fetch_all(db)
    .await?)
}

/// Points a coin at its wallet server. An address set for this deployment by hand
/// (`POOL_RPC_URL_<SYMBOL>`) is left alone.
fn use_wallet_server(coin: &mut Coin, server_id: i32, password: Option<&str>) {
    if env::var(format!("POOL_RPC_URL_{}", coin.symbol)).is_ok_and(|url| !url.is_empty()) {
        return;
    }
    // The configured address already names the coin's RPC port.
    let Some(port) = coin.rpc_url.rsplit(':').next().map(|port| port.trim_end_matches('/').to_owned()) else {
        return;
    };
    coin.rpc_url = format!("http://{MESH_NETWORK}.{server_id}:{port}");
    if let Some(password) = password {
        coin.rpc_password = password.to_owned();
    }
}
/// Arbitrary but fixed: identifies "the pool-core instance" among Postgres advisory locks.
const SINGLETON_LOCK: i64 = 0x706f_6f6c_636f_7265;

#[tokio::main]
async fn main() -> Result<()> {
    pool_common::init_logging();
    let cfg = Config::load()?;
    let db = pool_common::db::connect(&cfg).await?;

    // Two instances paying out at once could pay twice. The lock lives as long as this connection.
    let mut lock = db.acquire().await?;
    let locked: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(SINGLETON_LOCK).fetch_one(&mut *lock).await?;
    ensure!(locked, "another pool-core instance is already running against this database");
    info!("database ready");

    // A coin that is no longer configured stays in the books but leaves the website.
    // Never switched back on here: an admin may have switched a coin off on purpose.
    let symbols: Vec<&str> = cfg.coins.iter().map(|coin| coin.symbol.as_str()).collect();
    sqlx::query("UPDATE coins SET enabled = FALSE WHERE NOT (symbol = ANY($1))").bind(&symbols).execute(&db).await?;

    let wallets = wallet_servers(&db).await?;
    let wallet_password: Option<String> =
        sqlx::query_scalar("SELECT value FROM pool_secrets WHERE name = 'wallet_rpc_password'").fetch_optional(&db).await?;
    let (stop, stopping) = watch::channel(false);
    let mut coins = JoinSet::new();
    for coin in &cfg.coins {
        let coin_id = pool_common::db::ensure_coin(&db, coin).await?;
        let mut coin = coin.clone();
        if let Some((_, server_id)) = wallets.iter().find(|(symbol, _)| *symbol == coin.symbol) {
            use_wallet_server(&mut coin, *server_id, wallet_password.as_deref());
            info!(coin = %coin.symbol, server = server_id, "pool wallet is on an enrolled server");
        }
        coins.spawn(run_coin(db.clone(), coin_id, coin, cfg.accounting.clone(), stopping.clone()));
    }

    tokio::spawn(run_exchange(db.clone(), cfg.coins.clone()));
    tokio::spawn(retention::run(db.clone()));

    // Where the wallets are is fixed at startup. When an admin moves one, finish the
    // round in progress and leave; the supervisor starts us again with the new layout.
    let moved = async {
        let mut tick = tokio::time::interval(WALLET_CHECK_INTERVAL);
        loop {
            tick.tick().await;
            if wallet_servers(&db).await.is_ok_and(|now| now != wallets) {
                return;
            }
        }
    };
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result?;
            info!("shutting down");
        }
        _ = moved => {
            info!("a pool wallet was moved to another server, restarting to use it");
            stop.send_replace(true);
            while coins.join_next().await.is_some() {}
        }
    }
    Ok(())
}

/// Fetches prices and then lets miners swap what they earned in a coin that is not theirs.
async fn run_exchange(db: PgPool, coins: Vec<Coin>) {
    // The price API turns away requests that do not say who is asking.
    let http = match reqwest::Client::builder().user_agent(concat!("mining-pool/", env!("CARGO_PKG_VERSION"))).build() {
        Ok(http) => http,
        Err(err) => return error!("cannot create the http client for prices: {err:#}"),
    };
    let mut prices = prices::Prices::default();
    let mut tick = tokio::time::interval(PRICE_INTERVAL);
    loop {
        tick.tick().await;
        // A failed fetch leaves the old prices, which the exchange stops using once they are stale.
        if let Err(err) = prices.refresh(&db, &http, &coins).await {
            warn!("refreshing prices failed: {err:#}");
        }
        if let Err(err) = exchange::run(&db).await {
            error!("exchange round failed: {err:#}");
        }
    }
}

/// All accounting for one coin runs in this single loop, so its steps never overlap.
async fn run_coin(db: PgPool, coin_id: i32, coin: Coin, accounting: Accounting, stopping: watch::Receiver<bool>) {
    let rpc = match RpcClient::new(&coin) {
        Ok(rpc) => rpc,
        Err(err) => return error!(coin = %coin.symbol, "cannot create rpc client: {err:#}"),
    };
    let payout_interval = Duration::from_secs(accounting.payout_interval_secs);
    let mut last_payout = Instant::now();
    let mut tick = tokio::time::interval(BLOCK_INTERVAL);
    let mut has_address = false;
    if let Err(err) = stats::publish_settings(&db, coin_id, &coin, &accounting).await {
        error!(coin = %coin.symbol, "publishing settings failed: {err:#}");
    }
    info!(coin = %coin.symbol, "accounting started");

    loop {
        tick.tick().await;
        // Only between rounds, so that a payout is never cut off halfway.
        if *stopping.borrow() {
            return;
        }
        if let Err(err) = stats::refresh_chain_stats(&db, &rpc, coin_id, &coin).await {
            warn!(coin = %coin.symbol, "refreshing chain stats failed: {err:#}");
        }
        if !has_address {
            // Fails quietly until the wallet node is reachable; the stats warning above covers that.
            has_address = stats::publish_pool_address(&db, &rpc, coin_id, &coin).await.unwrap_or(false);
        }
        // Before payouts: a wallet that was just encrypted must not be asked to send while unlocked state is unknown.
        if let Err(err) = wallet::secure(&db, &rpc, coin_id, &coin).await {
            warn!(coin = %coin.symbol, "securing the wallet failed: {err:#}");
        } else if let Err(err) = wallet::backup(&db, &rpc, coin_id, &coin).await {
            warn!(coin = %coin.symbol, "wallet backup failed: {err:#}");
        }
        if let Err(err) = stats::refresh_wallet_balance(&db, &rpc, coin_id).await {
            warn!(coin = %coin.symbol, "reading the wallet balance failed: {err:#}");
        }
        if let Err(err) = blocks::credit_new_blocks(&db, coin_id, accounting.pplns_factor).await {
            error!(coin = %coin.symbol, "crediting blocks failed: {err:#}");
        }
        if let Err(err) = blocks::update_confirmations(&db, &rpc, coin_id, &coin).await {
            error!(coin = %coin.symbol, "updating confirmations failed: {err:#}");
        }
        if last_payout.elapsed() >= payout_interval {
            last_payout = Instant::now();
            if let Err(err) = payouts::run(&db, &rpc, coin_id, &coin).await {
                error!(coin = %coin.symbol, "payout round failed: {err:#}");
            }
        }
    }
}
