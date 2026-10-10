use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Scrypt,
    Neoscrypt,
}

unsafe extern "C" {
    /// `input` is 80 bytes, `output` 32 bytes.
    fn neoscrypt(input: *const u8, output: *mut u8, profile: u32);
}

/// The standard NeoScrypt profile used by coins: FastKDF, ChaCha20/20 + Salsa20/20, N=128, r=2.
const NEOSCRYPT_PROFILE: u32 = 0x8000_0620;

impl Algo {
    pub fn from_name(name: &str) -> Result<Self> {
        match name {
            "scrypt" => Ok(Self::Scrypt),
            "neoscrypt" => Ok(Self::Neoscrypt),
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
            Self::Neoscrypt => {
                let mut out = [0u8; 32];
                // SAFETY: the function reads exactly 80 bytes and writes exactly 32.
                unsafe { neoscrypt(header.as_ptr(), out.as_mut_ptr(), NEOSCRYPT_PROFILE) };
                out
            }
        }
    }

    /// Miners scale stratum difficulty by this factor for some algos (see Yiimp's `g_algos` table).
    pub fn diff_multiplier(self) -> f64 {
        match self {
            Self::Scrypt | Self::Neoscrypt => 65536.0,
        }
    }
}
