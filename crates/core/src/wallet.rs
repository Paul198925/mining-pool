use anyhow::{Context, Result};
use pool_common::{config::Coin, rpc::RpcClient};
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing::info;

/// How long the wallet stays unlocked for one payout batch.
const UNLOCK_SECS: u32 = 120;

/// Encrypts the pool wallet with the configured passphrase if it is not encrypted yet,
/// and records its state for the admin area. Without a passphrase the wallet stays as it is.
///
/// Encryption protects the keys in the wallet file and in every backup of it. It cannot
/// be undone, and without the passphrase the coins are lost.
pub async fn secure(db: &PgPool, rpc: &RpcClient, coin_id: i32, coin: &Coin) -> Result<()> {
    let info: Value = rpc.call("getwalletinfo", json!([])).await?;
    // Only an encrypted wallet reports until when it is unlocked.
    let mut encrypted = info.get("unlocked_until").is_some();
    if !encrypted && let Some(passphrase) = &coin.wallet_passphrase {
        rpc.call::<Value>("encryptwallet", json!([passphrase])).await.context("encrypting the wallet")?;
        info!(coin = %coin.symbol, "wallet encrypted");
        encrypted = true;
        // Backups made before this hold the keys in the clear and miss the keys created now.
        sqlx::query("UPDATE coins SET wallet_backup_at = NULL WHERE id = $1").bind(coin_id).execute(db).await?;
    }
    sqlx::query("UPDATE coins SET wallet_encrypted = $2 WHERE id = $1").bind(coin_id).bind(encrypted).execute(db).await?;
    Ok(())
}

/// Has the wallet node write a copy of the wallet once a day.
///
/// The copy lands in the node's data directory under a name that repeats every month,
/// which keeps a month of history without anything having to delete old files. It
/// guards against a damaged wallet file; copies on another machine are a separate job.
pub async fn backup(db: &PgPool, rpc: &RpcClient, coin_id: i32, coin: &Coin) -> Result<()> {
    let Some(dir) = &coin.wallet_backup_dir else {
        return Ok(());
    };
    let due_day: Option<String> = sqlx::query_scalar(
        "SELECT to_char(now() AT TIME ZONE 'UTC', 'DD') FROM coins
         WHERE id = $1 AND (wallet_backup_at IS NULL OR wallet_backup_at < now() - interval '24 hours')",
    )
    .bind(coin_id)
    .fetch_optional(db)
    .await?;
    let Some(day) = due_day else {
        return Ok(());
    };
    let path = format!("{}/wallet-backup-{}-{day}.dat", dir.trim_end_matches('/'), coin.symbol.to_lowercase());
    rpc.call::<Value>("backupwallet", json!([path])).await.context("backing up the wallet")?;
    sqlx::query("UPDATE coins SET wallet_backup_at = now() WHERE id = $1").bind(coin_id).execute(db).await?;
    info!(coin = %coin.symbol, %path, "wallet backed up on its node");
    Ok(())
}

/// Unlocks an encrypted wallet for a payout. Does nothing without a passphrase.
pub async fn unlock(rpc: &RpcClient, coin: &Coin) -> Result<()> {
    if let Some(passphrase) = &coin.wallet_passphrase {
        rpc.call::<Value>("walletpassphrase", json!([passphrase, UNLOCK_SECS])).await.context("unlocking the wallet")?;
    }
    Ok(())
}

/// Locks the wallet again right after a payout instead of waiting for the timeout.
pub async fn lock(rpc: &RpcClient, coin: &Coin) {
    if coin.wallet_passphrase.is_some() {
        let _ = rpc.call::<Value>("walletlock", json!([])).await;
    }
}
