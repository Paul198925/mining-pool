use anyhow::{Result, bail};
use sha3::{Digest, Sha3_256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algo {
    Scrypt,
    Neoscrypt,
    /// Rincoin: BLAKE3, then Argon2d, then SHA3-256.
    RinHash,
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
            // "neoscrypt-xaya" is what miners call Neoscrypt over Xaya's block layout.
            "neoscrypt" | "neoscrypt-xaya" => Ok(Self::Neoscrypt),
            "rinhash" => Ok(Self::RinHash),
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
            Self::RinHash => {
                // Parameters from rincoin's src/crypto/rinhash.cpp: 64 KiB, 2 passes, 1 lane.
                let params = argon2::Params::new(64, 2, 1, Some(32)).expect("valid argon2 params");
                let argon = argon2::Argon2::new(argon2::Algorithm::Argon2d, argon2::Version::V0x13, params);
                let mut stretched = [0u8; 32];
                argon
                    .hash_password_into(blake3::hash(header).as_bytes(), b"RinCoinSalt", &mut stretched)
                    .expect("valid argon2 input lengths");
                Sha3_256::digest(stretched).into()
            }
        }
    }

    /// Whether a block is identified by its proof-of-work hash. Bitcoin and most forks
    /// identify blocks by the double SHA-256 of the header instead.
    pub fn block_id_is_pow_hash(self) -> bool {
        match self {
            Self::Scrypt | Self::Neoscrypt => false,
            Self::RinHash => true,
        }
    }

    /// Miners scale stratum difficulty by this factor for some algos (see Yiimp's `g_algos` table).
    pub fn diff_multiplier(self) -> f64 {
        match self {
            Self::Scrypt | Self::Neoscrypt => 65536.0,
            Self::RinHash => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hashes a header given as hex and returns the hash as block explorers show it.
    fn rinhash_display(header_hex: &str) -> String {
        let header: [u8; 80] = hex::decode(header_hex).unwrap().try_into().unwrap();
        let mut hash = Algo::RinHash.hash(&header);
        hash.reverse();
        hex::encode(hash)
    }

    // Headers of real Rincoin blocks with the hashes the chain itself assigns them.
    #[test]
    fn rinhash_matches_the_chain() {
        assert_eq!(
            rinhash_display("010000000000000000000000000000000000000000000000000000000000000000000000adcd471c60b9dc56b5dc049e567106388fdf078f936a722b42edd23085c0908500e8e467ffff001f28850000"),
            "000096bdd6e4613ca89b074ebd6f609aba6fe3f868b34ee79380aa3bc7a8c9db",
            "mainnet genesis"
        );
        assert_eq!(
            rinhash_display("01000000000000000000000000000000000000000000000000000000000000000000000012ff1583df949307dd3915983bb0f7b2a62f689966e5cb003adc338bbf2cc1e3b0f8e467ffff7f2000000000"),
            "7d2c8c57ce2597f86c9fe41f9865ad664b04d2aad4321fdaab48ed3da1805fe7",
            "regtest genesis"
        );
        assert_eq!(
            rinhash_display("00000020a71053a466138feb491ccd7d099141907d0a5fc196e90b932e0e950100000000589bec83f1fdd12d0fd974e05082d072257c2c304064608fdd12e59f4f1221bcf513ca6a32c6011d72c00454"),
            "00000001a52af2de366ea7765f89be5887c18b1216a7d2a049e85f651f1a6391",
            "mainnet block 772913"
        );
    }
}
