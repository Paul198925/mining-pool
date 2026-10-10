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
}

impl Chain {
    pub fn from_name(name: &str) -> Result<Self> {
        match name {
            "bitcoin" => Ok(Self::Bitcoin),
            "xaya" => Ok(Self::Xaya),
            other => bail!("unsupported chain type: {other}"),
        }
    }

    pub fn extranonce1_size(self) -> usize {
        match self {
            Self::Bitcoin => 4,
            // Both extranonces together are the 4-byte nonce field of the block header,
            // and miners refuse an extranonce2 shorter than two bytes.
            Self::Xaya => 2,
        }
    }

    pub fn extranonce2_size(self) -> usize {
        match self {
            Self::Bitcoin => 4,
            Self::Xaya => 2,
        }
    }

    /// Whether extranonce1 starts with the server's instance id. Where it does not,
    /// the instance id has to enter the hashed data some other way (see `Job`).
    pub fn instance_in_extranonce(self) -> bool {
        match self {
            Self::Bitcoin => true,
            Self::Xaya => false,
        }
    }

    /// Parameters for the daemon's `getblocktemplate`.
    pub fn template_request(self) -> Value {
        match self {
            Self::Bitcoin => json!([{}]),
            // Neoscrypt is the only algorithm Xaya allows outside merged mining.
            Self::Xaya => json!([{ "rules": ["segwit"], "algo": "neoscrypt" }]),
        }
    }
}
