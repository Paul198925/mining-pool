use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Scrypt,
}

impl Algo {
    pub fn from_name(name: &str) -> Result<Self> {
        match name {
            "scrypt" => Ok(Self::Scrypt),
            other => bail!("unsupported algo: {other}"),
        }
    }

    /// Proof-of-work hash of a block header, as a little-endian number.
    pub fn hash(self, header: &[u8; 80]) -> [u8; 32] {
        match self {
            Self::Scrypt => {
                let params = scrypt::Params::new(10, 1, 1, 32).expect("valid scrypt params");
                let mut out = [0u8; 32];
                scrypt::scrypt(header, header, &params, &mut out).expect("valid output length");
                out
            }
        }
    }

    /// Miners scale stratum difficulty by this factor for some algos (see Yiimp's `g_algos` table).
    pub fn diff_multiplier(self) -> f64 {
        match self {
            Self::Scrypt => 65536.0,
        }
    }
}
