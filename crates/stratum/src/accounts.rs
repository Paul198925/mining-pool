use anyhow::Result;
use pool_common::rpc::RpcClient;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerIds {
    pub account_id: i64,
    pub worker_id: i64,
    /// The miner opted out of PPLNS for this connection.
    pub solo: bool,
    /// The private group this connection mines for, if any.
    pub party_id: Option<i64>,
}

/// How a connection wants its work rewarded, chosen in the stratum password.
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Pool,
    Solo,
    Party(String),
}

/// Resolves a stratum login. The username is `<payout address>[.<worker name>]`; the password
/// carries comma-separated options as in Yiimp, of which `m=solo` and `m=party.<name>` are understood.
/// Returns `None` if the address is not valid for the coin.
pub async fn resolve(
    db: &PgPool,
    rpc: &RpcClient,
    coin_id: i32,
    user: &str,
    password: &str,
) -> Result<Option<WorkerIds>> {
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
    let mode = mode(password);
    let party_id = match &mode {
        Mode::Party(name) => {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO parties (coin_id, key_hash) VALUES ($1, $2)
                 ON CONFLICT (coin_id, key_hash) DO UPDATE SET key_hash = EXCLUDED.key_hash
                 RETURNING id",
            )
            .bind(coin_id)
            .bind(hex::encode(Sha256::digest(name.as_bytes())))
            .fetch_one(db)
            .await?;
            Some(id)
        }
        _ => None,
    };
    Ok(Some(WorkerIds { account_id, worker_id, solo: mode == Mode::Solo, party_id }))
}

/// The first recognised `m=` option wins; anything else means ordinary pool mining.
fn mode(password: &str) -> Mode {
    for option in password.split(',').map(str::trim) {
        if option.eq_ignore_ascii_case("m=solo") {
            return Mode::Solo;
        }
        let prefix = option.get(..8).unwrap_or_default();
        let name = option.get(8..).unwrap_or_default();
        if prefix.eq_ignore_ascii_case("m=party.") && !name.is_empty() && name.len() <= 64 {
            return Mode::Party(name.to_owned());
        }
    }
    Mode::Pool
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_is_an_explicit_password_option() {
        assert_eq!(mode("m=solo"), Mode::Solo);
        assert_eq!(mode("c=DOGE, M=SOLO"), Mode::Solo);
        assert_eq!(mode("x"), Mode::Pool);
        assert_eq!(mode(""), Mode::Pool);
        assert_eq!(mode("m=solo2"), Mode::Pool);
    }

    #[test]
    fn party_needs_a_name() {
        assert_eq!(mode("c=DOGE,m=party.Friends42"), Mode::Party("Friends42".into()));
        assert_eq!(mode("M=PARTY.x"), Mode::Party("x".into()));
        assert_eq!(mode("m=party."), Mode::Pool);
        assert_eq!(mode("m=party"), Mode::Pool);
        assert_eq!(mode(&format!("m=party.{}", "a".repeat(65))), Mode::Pool);
    }
}
