use anyhow::Result;
use pool_common::rpc::RpcClient;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::backend::Backend;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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

/// A coin served on the port the miner connected to.
pub struct Candidate<'a> {
    /// The coin's database id; unused on servers that go through the node API.
    pub coin_id: i32,
    pub symbol: &'a str,
    pub rpc: &'a RpcClient,
    /// The coin that is merge-mined along with this one.
    pub aux: Option<AuxCandidate<'a>>,
}

pub struct AuxCandidate<'a> {
    pub coin_id: i32,
    pub symbol: &'a str,
    pub rpc: &'a RpcClient,
}

/// A coin this server knows a daemon for: miners can ask to be paid in it.
pub struct PayoutCoin<'a> {
    pub coin_id: i32,
    pub symbol: &'a str,
    pub rpc: &'a RpcClient,
}

/// Whether the coin's daemon takes `address` for one of its own. A daemon that is away
/// counts as "no": it must not lock miners out of the other coins.
async fn valid_on(rpc: &RpcClient, symbol: &str, address: &str) -> bool {
    match rpc.call::<Value>("validateaddress", json!([address])).await {
        Ok(info) => info["isvalid"] == true,
        Err(err) => {
            tracing::warn!(coin = %symbol, "cannot check the address: {err:#}");
            false
        }
    }
}

/// Resolves a stratum login. The username is `<payout address>[.<worker name>]`; the password
/// carries comma-separated options as in Yiimp: `c=<SYMBOL>` picks the coin to mine, `m=solo`
/// and `m=party.<name>` the reward mode.
///
/// Without `c=`, the coin is the first one on the port for which the address is valid, or
/// the port's only coin.
///
/// The address may be one of another coin of the pool: the miner is then paid in that coin,
/// and what they mine is swapped by the internal exchange. `pay=<SYMBOL>` says so explicitly,
/// for addresses that are valid on several chains.
///
/// Where a second coin is merge-mined along, `<SYMBOL>=<address>` names the miner's address
/// on that chain. Without it the second coin is credited to the same account.
///
/// Returns the index of the chosen candidate with the miner's ids for it and for its
/// merge-mined coin, or `None` if the login is not acceptable.
pub async fn resolve(
    backend: &Backend,
    candidates: &[Candidate<'_>],
    payout_coins: &[PayoutCoin<'_>],
    user: &str,
    password: &str,
) -> Result<Option<(usize, WorkerIds, Option<WorkerIds>)>> {
    let (address, worker) = user.split_once('.').unwrap_or((user, ""));
    // Cheap rejection of junk before asking the daemons.
    let plausible = (20..=100).contains(&address.len()) && address.chars().all(|c| c.is_ascii_alphanumeric());
    if !plausible || worker.len() > 64 {
        return Ok(None);
    }

    // The coin to mine.
    let mut valid_on_mined = false;
    let index = match option(password, "c") {
        Some(wanted) => candidates.iter().position(|candidate| candidate.symbol.eq_ignore_ascii_case(wanted)),
        None => {
            let mut found = None;
            for (index, candidate) in candidates.iter().enumerate() {
                if valid_on(candidate.rpc, candidate.symbol, address).await {
                    valid_on_mined = true;
                    found = Some(index);
                    break;
                }
            }
            found.or((candidates.len() == 1).then_some(0))
        }
    };
    let Some(index) = index else {
        return Ok(None);
    };
    let mined = &candidates[index];
    let coin_id = mined.coin_id;

    // The coin to be paid in: the one named, else the mined one if the address fits it,
    // else whichever other coin of the pool the address belongs to.
    let payout = match option(password, "pay") {
        Some(wanted) => {
            let Some(coin) = payout_coins.iter().find(|coin| coin.symbol.eq_ignore_ascii_case(wanted)) else {
                return Ok(None);
            };
            valid_on(coin.rpc, coin.symbol, address).await.then_some((coin.coin_id, coin.symbol))
        }
        None if valid_on_mined || valid_on(mined.rpc, mined.symbol, address).await => Some((coin_id, mined.symbol)),
        None => {
            let mut found = None;
            for coin in payout_coins.iter().filter(|coin| coin.symbol != mined.symbol) {
                if valid_on(coin.rpc, coin.symbol, address).await {
                    found = Some((coin.coin_id, coin.symbol));
                    break;
                }
            }
            found
        }
    };
    let Some((payout_coin_id, payout_symbol)) = payout else {
        return Ok(None);
    };

    let mode = mode(password);
    // Only a hash of the party name is kept, and only the hash ever leaves this server.
    let party_key = match &mode {
        Mode::Party(name) => Some(hex::encode(Sha256::digest(name.as_bytes()))),
        _ => None,
    };
    let solo = mode == Mode::Solo;
    let ids = match backend {
        Backend::Direct(db) => {
            register(db, payout_coin_id, address, worker, solo, coin_id, party_key.as_deref()).await?
        }
        Backend::Api(api) => {
            let login = json!({
                "symbol": mined.symbol, "payout_symbol": payout_symbol, "address": address, "worker": worker,
                "solo": solo, "party_key_hash": party_key,
            });
            api.post("/api/node/authorize", &login).await?
        }
    };

    let Some(aux) = &candidates[index].aux else {
        return Ok(Some((index, ids, None)));
    };
    // The account that is credited on the merged chain: one of that chain if the miner
    // named an address there, otherwise the account of this login.
    let (account_coin, account_symbol, account_address) = match address_option(password, aux.symbol) {
        Some(own) => {
            let plausible = (20..=100).contains(&own.len()) && own.chars().all(|c| c.is_ascii_alphanumeric());
            let info: Value =
                if plausible { aux.rpc.call("validateaddress", json!([own])).await? } else { Value::Null };
            if info["isvalid"] != true {
                // Refused rather than ignored: the miner would mine on and never see those coins.
                return Ok(None);
            }
            (aux.coin_id, aux.symbol, own)
        }
        None => (payout_coin_id, payout_symbol, address),
    };
    let aux_ids = match backend {
        Backend::Direct(db) => {
            register(db, account_coin, account_address, worker, solo, aux.coin_id, party_key.as_deref()).await?
        }
        Backend::Api(api) => {
            let login = json!({
                "symbol": aux.symbol, "payout_symbol": account_symbol, "address": account_address,
                "worker": worker, "solo": solo, "party_key_hash": party_key,
            });
            api.post("/api/node/authorize", &login).await?
        }
    };
    Ok(Some((index, ids, Some(aux_ids))))
}

/// Maps a login to the pool's account, worker and party ids, creating them on first sight.
/// The account belongs to the coin its address is on; a party is per mined coin.
async fn register(
    db: &PgPool,
    account_coin_id: i32,
    address: &str,
    worker: &str,
    solo: bool,
    party_coin_id: i32,
    party_key: Option<&str>,
) -> Result<WorkerIds> {
    let account_id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (payout_coin_id, payout_address) VALUES ($1, $2)
         ON CONFLICT (payout_coin_id, payout_address) DO UPDATE SET payout_address = EXCLUDED.payout_address
         RETURNING id",
    )
    .bind(account_coin_id)
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
    let party_id = match party_key {
        Some(key_hash) => {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO parties (coin_id, key_hash) VALUES ($1, $2)
                 ON CONFLICT (coin_id, key_hash) DO UPDATE SET key_hash = EXCLUDED.key_hash
                 RETURNING id",
            )
            .bind(party_coin_id)
            .bind(key_hash)
            .fetch_one(db)
            .await?;
            Some(id)
        }
        None => None,
    };
    Ok(WorkerIds { account_id, worker_id, solo, party_id })
}

/// The value of a `<key>=<value>` password option, e.g. `c=<SYMBOL>`.
fn option<'a>(password: &'a str, key: &str) -> Option<&'a str> {
    password.split(',').map(str::trim).find_map(|option| {
        let (name, value) = option.split_once('=')?;
        (name.eq_ignore_ascii_case(key) && !value.is_empty()).then_some(value)
    })
}

/// The address from a `<SYMBOL>=<address>` password option.
fn address_option<'a>(password: &'a str, symbol: &str) -> Option<&'a str> {
    option(password, symbol)
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
    fn coin_is_picked_with_c() {
        assert_eq!(option("c=CHI", "c"), Some("CHI"));
        assert_eq!(option("m=solo, C=rod", "c"), Some("rod"));
        assert_eq!(option("x", "c"), None);
        assert_eq!(option("c=", "c"), None);
        assert_eq!(option("c=LTC,pay=DOGE", "pay"), Some("DOGE"));
    }

    #[test]
    fn merged_coin_address_is_named_by_symbol() {
        assert_eq!(address_option("c=LTC,doge=DAbc", "DOGE"), Some("DAbc"));
        assert_eq!(address_option("c=LTC", "DOGE"), None);
        assert_eq!(address_option("DOGE=", "DOGE"), None);
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
