use anyhow::{Result, bail};
use serde_json::{Value, json};

/// How a coin lays out its blocks and proof of work, which decides what miners get to vary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    /// Bitcoin and its plain forks: the proof of work is over the block header,
    /// miners vary the coinbase transaction.
    Bitcoin,
    /// Xaya and SpaceXpanse: the proof of work is over a second "fake" header that commits
    /// to the block header, miners vary the block header's otherwise unused nonce.
    Xaya,
    /// Ravencoin: the header carries the block height, a 64-bit nonce and a mix hash.
    /// Miners get only the hash of the header's fixed part and vary the nonce; each
    /// connection owns the nonces that start with its extranonce.
    Ravencoin,
}

impl Chain {
    pub fn from_name(name: &str) -> Result<Self> {
        match name {
            "bitcoin" => Ok(Self::Bitcoin),
            "xaya" => Ok(Self::Xaya),
            "ravencoin" => Ok(Self::Ravencoin),
            other => bail!("unsupported chain type: {other}"),
        }
    }

    pub fn extranonce1_size(self) -> usize {
        match self {
            Self::Bitcoin => 4,
            // Both extranonces together are the 4-byte nonce field of the block header,
            // and miners refuse an extranonce2 shorter than two bytes.
            Self::Xaya => 2,
            // The first two bytes of the 8-byte nonce, as KawPoW miners expect.
            Self::Ravencoin => 2,
        }
    }

    pub fn extranonce2_size(self) -> usize {
        match self {
            Self::Bitcoin => 4,
            Self::Xaya => 2,
            // Not part of this dialect: the miner fills the rest of the nonce itself.
            Self::Ravencoin => 0,
        }
    }

    /// Whether extranonce1 starts with the server's instance id. Where it does not,
    /// the instance id has to enter the hashed data some other way (see `Job`).
    pub fn instance_in_extranonce(self) -> bool {
        match self {
            Self::Bitcoin => true,
            Self::Xaya | Self::Ravencoin => false,
        }
    }

    /// Parameters for the daemon's `getblocktemplate`.
    pub fn template_request(self, rules: &[String]) -> Value {
        match self {
            Self::Bitcoin | Self::Ravencoin => json!([{ "rules": rules }]),
            // Neoscrypt is the only algorithm Xaya allows outside merged mining.
            Self::Xaya => json!([{ "rules": ["segwit"], "algo": "neoscrypt" }]),
        }
    }
}
