use std::{env, fs};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database: Database,
    pub redis: Redis,
    pub stratum: Stratum,
    #[serde(default)]
    pub coins: Vec<Coin>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Redis {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stratum {
    pub listen: String,
    /// Symbol of the coin this listener serves.
    pub coin: String,
    pub start_difficulty: f64,
    pub min_difficulty: f64,
    pub max_difficulty: f64,
    /// Vardiff aims for one share per this many seconds.
    pub share_target_secs: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Coin {
    pub symbol: String,
    pub name: String,
    pub algo: String,
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
    /// Where block rewards go. Unset means "ask the daemon wallet", which is only sane in dev.
    pub pool_address: Option<String>,
}

impl Config {
    /// Reads the file named by `POOL_CONFIG`, or `config/dev.toml`.
    pub fn load() -> Result<Self> {
        let path = env::var("POOL_CONFIG").unwrap_or_else(|_| "config/dev.toml".into());
        let raw = fs::read_to_string(&path).with_context(|| format!("reading config {path}"))?;
        toml::from_str(&raw).with_context(|| format!("parsing config {path}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_config_parses() {
        let raw = include_str!("../../../config/dev.toml");
        let cfg: Config = toml::from_str(raw).unwrap();
        assert!(!cfg.coins.is_empty());
    }
}
