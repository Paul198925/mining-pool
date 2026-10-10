//! KawPoW, Ravencoin's proof of work: ProgPoW 0.9.4 over an Ethash-style data set that
//! changes every 7500 blocks (an "epoch").
//!
//! All hashes here are in the byte order they are written in hex.

use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{Arc, LazyLock, Mutex},
};

use anyhow::{Result, ensure};

pub const EPOCH_LENGTH: u64 = 7500;

unsafe extern "C" {
    fn kawpow_context_new(epoch: i32) -> *mut c_void;
    fn kawpow_context_free(context: *mut c_void);
    fn kawpow_hash(context: *const c_void, height: i32, header_hash: *const u8, nonce: u64, final_out: *mut u8, mix_out: *mut u8);
    fn kawpow_final(height: i32, header_hash: *const u8, mix_hash: *const u8, nonce: u64, final_out: *mut u8);
    fn kawpow_seed(epoch: i32, seed_out: *mut u8);
}

pub struct Pow {
    pub final_hash: [u8; 32],
    pub mix_hash: [u8; 32],
}

/// What it takes to check proofs of work of one epoch without the full data set:
/// some tens of megabytes that take seconds to compute for a late epoch.
pub struct Epoch {
    context: *mut c_void,
    pub number: u64,
}

// SAFETY: the context is only read after it is built.
unsafe impl Send for Epoch {}
unsafe impl Sync for Epoch {}

impl Drop for Epoch {
    fn drop(&mut self) {
        // SAFETY: created by `kawpow_context_new` and not freed elsewhere.
        unsafe { kawpow_context_free(self.context) }
    }
}

impl Epoch {
    pub fn of_height(height: u64) -> u64 {
        height / EPOCH_LENGTH
    }

    /// Slow for late epochs; call it off the async runtime.
    pub fn build(number: u64) -> Result<Self> {
        ensure!(number < 30_000, "epoch {number} is out of range");
        // SAFETY: plain call; a null result is handled.
        let context = unsafe { kawpow_context_new(number as i32) };
        ensure!(!context.is_null(), "out of memory building the context of epoch {number}");
        Ok(Self { context, number })
    }

    /// What miners identify the epoch by.
    pub fn seed(number: u64) -> [u8; 32] {
        let mut seed = [0u8; 32];
        // SAFETY: writes exactly 32 bytes.
        unsafe { kawpow_seed(number as i32, seed.as_mut_ptr()) };
        seed
    }

    /// The proof of work of a header at `height`, which must lie in this epoch.
    /// Takes a few milliseconds.
    pub fn hash(&self, height: u64, header_hash: &[u8; 32], nonce: u64) -> Pow {
        debug_assert_eq!(Self::of_height(height), self.number);
        let mut pow = Pow { final_hash: [0; 32], mix_hash: [0; 32] };
        // SAFETY: reads 32 bytes, writes 32 bytes to each output; the context is alive.
        unsafe {
            kawpow_hash(self.context, height as i32, header_hash.as_ptr(), nonce, pow.final_hash.as_mut_ptr(), pow.mix_hash.as_mut_ptr());
        }
        pow
    }
}

/// The final hash that would follow from `mix_hash`. Cheap, but it says nothing about
/// whether the mix hash is the real one: use it to turn away weak shares early.
pub fn final_hash(height: u64, header_hash: &[u8; 32], mix_hash: &[u8; 32], nonce: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    // SAFETY: reads 32 bytes from each input and writes 32 bytes.
    unsafe { kawpow_final(height as i32, header_hash.as_ptr(), mix_hash.as_ptr(), nonce, out.as_mut_ptr()) };
    out
}

pub static EPOCHS: LazyLock<Epochs> = LazyLock::new(Epochs::default);

/// Built epochs, shared by every KawPoW coin. Only the newest two are kept: around
/// an epoch change the old and the new one are both in use.
#[derive(Default)]
pub struct Epochs(Mutex<HashMap<u64, Arc<Epoch>>>);

impl Epochs {
    /// Slow the first time an epoch is asked for; call it off the async runtime.
    pub fn get(&self, number: u64) -> Result<Arc<Epoch>> {
        if let Some(epoch) = self.0.lock().unwrap().get(&number) {
            return Ok(epoch.clone());
        }
        let epoch = Arc::new(Epoch::build(number)?);
        let mut epochs = self.0.lock().unwrap();
        epochs.insert(number, epoch.clone());
        if epochs.len() > 2 {
            let oldest = *epochs.keys().min().expect("not empty");
            epochs.remove(&oldest);
        }
        Ok(epoch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash32(hex_str: &str) -> [u8; 32] {
        hex::decode(hex_str).unwrap().try_into().unwrap()
    }

    // From Ravencoin's src/crypto/ethash/progpow_test_vectors.hpp:
    // block number, header hash, nonce, mix hash, final hash.
    const VECTORS: [(u64, &str, u64, &str, &str); 4] = [
        (
            0,
            "0000000000000000000000000000000000000000000000000000000000000000",
            0x0000000000000000,
            "6e97b47b134fda0c7888802988e1a373affeb28bcd813b6e9a0fc669c935d03a",
            "e601a7257a70dc48fccc97a7330d704d776047623b92883d77111fb36870f3d1",
        ),
        (
            49,
            "63155f732f2bf556967f906155b510c917e48e99685ead76ea83f4eca03ab12b",
            0x0000000007073c07,
            "d36f7e815ee09e74eceb9c96993a3d681edf2bf0921fc7bb710364042db99777",
            "e7ced124598fd2500a55ad9f9f48e3569327fe50493c77a4ac9799b96efb9463",
        ),
        (
            99,
            "de37e1824c86d35d154cf65a88de6d9286aec4f7f10c3fc9f0fa1bcc2687188d",
            0x000000003917afab,
            "fa706860e5e0e830d5d1d7157e5bea7f5f8a350c7c8612ac1d1fcf2974d64244",
            "aa85340690f2e907054324a5021937910e15edfd1ef1577231843e7d32ec3a61",
        ),
        (
            29950,
            "ac7b55e801511b77e11d52e9599206101550144525b5679f2dab19386f23dcce",
            0x005d409dbc23a62a,
            "5359807b77a74878269c3a3044df8618a576ce8dc52e1c48d927d4a60e7c6b79",
            "022019e5408683f7f8326b4e46b42864a3a069f17b6151e434fcaedecaadd918",
        ),
    ];

    #[test]
    fn matches_ravencoins_test_vectors() {
        let epochs = Epochs::default();
        for (height, header, nonce, mix, final_) in VECTORS {
            let epoch = epochs.get(Epoch::of_height(height)).unwrap();
            let pow = epoch.hash(height, &hash32(header), nonce);
            assert_eq!(hex::encode(pow.mix_hash), mix, "mix hash at height {height}");
            assert_eq!(hex::encode(pow.final_hash), final_, "final hash at height {height}");
            assert_eq!(hex::encode(final_hash(height, &hash32(header), &pow.mix_hash, nonce)), final_);
        }
    }

    #[test]
    fn epoch_seed_is_keccak_iterated_from_zero() {
        assert_eq!(Epoch::seed(0), [0u8; 32]);
        assert_eq!(hex::encode(Epoch::seed(1)), "290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563");
    }
}
