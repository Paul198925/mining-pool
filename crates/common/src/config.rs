use std::{env, fs};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database: Database,
    #[serde(default)]
    pub instance: Instance,
    /// One listener per algorithm.
    #[serde(default)]
    pub stratum: Vec<Stratum>,
    pub accounting: Accounting,
    #[serde(default)]
    pub coins: Vec<Coin>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    pub url: String,
}

/// Identifies one stratum process among several serving the same pool.
#[derive(Debug, Clone, Deserialize)]
pub struct Instance {
    /// Must differ between stratum processes: it prefixes every miner's extranonce,
    /// which keeps miners on different servers from doing the same work.
    pub id: u8,
    /// Shown to miners when they choose a server, e.g. "Europe".
    pub region: String,
    /// The public hostname miners connect to.
    pub host: String,
}

impl Default for Instance {
    fn default() -> Self {
        Self { id: 0, region: "Default".into(), host: "localhost".into() }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stratum {
    pub listen: String,
    /// The algorithm this port serves; every coin with this `algo` is mined here.
    pub algo: String,
    pub start_difficulty: f64,
    pub min_difficulty: f64,
    pub max_difficulty: f64,
    /// Vardiff aims for one share per this many seconds.
    pub share_target_secs: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Accounting {
    /// PPLNS window as a multiple of the network difficulty.
    pub pplns_factor: f64,
    pub pool_fee_percent: f64,
    /// Fee on blocks found by solo miners.
    pub solo_fee_percent: f64,
    pub payout_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Coin {
    pub symbol: String,
    pub name: String,
    pub algo: String,
    /// Block layout family: "bitcoin" (default) or "xaya".
    #[serde(default = "default_chain")]
    pub chain: String,
    /// Rule sets to announce when asking the node for a block template, e.g. `["segwit"]`.
    /// Chains with segwit or MWEB refuse a client that does not name them.
    #[serde(default)]
    pub template_rules: Vec<String>,
    pub rpc_url: String,
    pub rpc_user: String,
    #[serde(default)]
    pub rpc_password: String,
    /// Where block rewards go. Unset means "ask the daemon wallet", which is only sane in dev.
    pub pool_address: Option<String>,
    /// Encrypts the pool wallet and unlocks it for payouts. Comes from the environment.
    #[serde(default)]
    pub wallet_passphrase: Option<String>,
    /// Directory inside the wallet node where it writes daily wallet backups.
    #[serde(default = "default_backup_dir")]
    pub wallet_backup_dir: Option<String>,
    /// The chain's coinbase maturity: confirmations before a block reward can be spent.
    pub block_maturity: i64,
    /// Balances below this many coins are not paid out yet.
    pub min_payout: f64,
}

fn default_chain() -> String {
    "bitcoin".into()
}

/// Every node image of this project keeps its data in /data.
fn default_backup_dir() -> Option<String> {
    Some("/data".into())
}

impl Config {
    /// Reads the file named by `POOL_CONFIG`, or `config/dev.toml`, then applies
    /// overrides from the environment (see [`Config::apply_env`]).
    pub fn load() -> Result<Self> {
        let path = env::var("POOL_CONFIG").unwrap_or_else(|_| "config/dev.toml".into());
        let raw = fs::read_to_string(&path).with_context(|| format!("reading config {path}"))?;
        let mut cfg: Self = toml::from_str(&raw).with_context(|| format!("parsing config {path}"))?;
        cfg.apply_env(|name| env::var(name).ok().filter(|value| !value.is_empty()))?;
        Ok(cfg)
    }

    /// Builds the configuration from what the pool's node API sends an enrolled server.
    /// The server's own secrets (daemon passwords) still come from its environment.
    pub fn from_json(value: serde_json::Value) -> Result<Self> {
        let mut cfg: Self = serde_json::from_value(value)?;
        cfg.apply_env(|name| env::var(name).ok().filter(|value| !value.is_empty()))?;
        Ok(cfg)
    }

    /// Lets a deployment keep secrets and per-server settings out of the config file:
    ///
    /// - `DATABASE_URL`
    /// - `POOL_INSTANCE_ID`, `POOL_REGION`, `POOL_STRATUM_HOST`
    /// - per coin, with its symbol: `POOL_RPC_URL_<SYMBOL>`, `POOL_RPC_USER_<SYMBOL>`,
    ///   `POOL_RPC_PASSWORD_<SYMBOL>`, `POOL_ADDRESS_<SYMBOL>`, `POOL_WALLET_PASSPHRASE_<SYMBOL>`
    fn apply_env(&mut self, var: impl Fn(&str) -> Option<String>) -> Result<()> {
        if let Some(url) = var("DATABASE_URL") {
            self.database.url = url;
        }
        if let Some(id) = var("POOL_INSTANCE_ID") {
            self.instance.id = id.parse().context("POOL_INSTANCE_ID must be a number from 0 to 255")?;
        }
        if let Some(region) = var("POOL_REGION") {
            self.instance.region = region;
        }
        if let Some(host) = var("POOL_STRATUM_HOST") {
            self.instance.host = host;
        }
        for coin in &mut self.coins {
            let symbol = coin.symbol.to_uppercase();
            if let Some(url) = var(&format!("POOL_RPC_URL_{symbol}")) {
                coin.rpc_url = url;
            }
            if let Some(user) = var(&format!("POOL_RPC_USER_{symbol}")) {
                coin.rpc_user = user;
            }
            if let Some(password) = var(&format!("POOL_RPC_PASSWORD_{symbol}")) {
                coin.rpc_password = password;
            }
            if let Some(address) = var(&format!("POOL_ADDRESS_{symbol}")) {
                coin.pool_address = Some(address);
            }
            if let Some(passphrase) = var(&format!("POOL_WALLET_PASSPHRASE_{symbol}")) {
                coin.wallet_passphrase = Some(passphrase);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev() -> Config {
        toml::from_str(include_str!("../../../config/dev.toml")).unwrap()
    }

    #[test]
    fn dev_config_parses() {
        assert!(!dev().coins.is_empty());
    }

    #[test]
    fn production_config_parses() {
        let cfg: Config = toml::from_str(include_str!("../../../config/production.toml")).unwrap();
        assert!(cfg.coins.iter().all(|coin| coin.rpc_password.is_empty()), "secrets come from the environment");
    }

    #[test]
    fn environment_overrides_the_file() {
        let mut cfg = dev();
        cfg.apply_env(|name| match name {
            "DATABASE_URL" => Some("postgres://prod".into()),
            "POOL_INSTANCE_ID" => Some("7".into()),
            "POOL_REGION" => Some("Europe".into()),
            "POOL_RPC_PASSWORD_DOGE" => Some("secret".into()),
            "POOL_ADDRESS_DOGE" => Some("DPoolAddress".into()),
            _ => None,
        })
        .unwrap();
        let doge = cfg.coins.iter().find(|coin| coin.symbol == "DOGE").unwrap();
        assert_eq!(cfg.database.url, "postgres://prod");
        assert_eq!((cfg.instance.id, cfg.instance.region.as_str()), (7, "Europe"));
        assert_eq!((doge.rpc_password.as_str(), doge.pool_address.as_deref()), ("secret", Some("DPoolAddress")));
        assert_eq!(doge.rpc_user, "pool", "untouched without an override");
    }

    #[test]
    fn bad_instance_id_is_an_error() {
        assert!(dev().apply_env(|name| (name == "POOL_INSTANCE_ID").then(|| "300".into())).is_err());
    }
}
