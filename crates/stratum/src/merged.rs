//! Merged mining: a second chain accepts the proof of work of another chain's block as
//! its own (AuxPoW), provided that block's coinbase commits to the second chain's block.
//! Miners work on the parent chain as usual and never notice.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use pool_common::{config::Coin, rpc::RpcClient};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::{
    pool::Pool,
    template::payout_address,
    util::{hash_difficulty, hex32, target_from_bits, write_varint},
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const DAEMON_RETRY: Duration = Duration::from_secs(10);
/// Marks the commitment in the parent's coinbase script.
const MAGIC: [u8; 4] = [0xfa, 0xbe, b'm', b'm'];

pub type AuxFeed = watch::Receiver<Option<Arc<AuxWork>>>;

/// A block of the merged chain that is waiting for a proof of work.
pub struct AuxWork {
    /// The block's hash in the byte order it is written in hex.
    pub hash: [u8; 32],
    /// Big-endian.
    pub target: [u8; 32],
    pub height: u64,
    pub reward: u64,
    /// In diff-1 units.
    pub network_difficulty: f64,
    /// Set by the first share that solves it; the chain takes a block only once.
    solved: AtomicBool,
}

#[derive(Deserialize)]
struct AuxBlock {
    hash: String,
    bits: String,
    height: u64,
    coinbasevalue: u64,
}

impl AuxWork {
    fn from_rpc(block: &AuxBlock) -> Result<Self> {
        // The daemon's own "target" field is byte-reversed on some versions; the bits are unambiguous.
        let target = target_from_bits(u32::from_str_radix(&block.bits, 16).context("aux block bits")?);
        let mut target_le = target;
        target_le.reverse();
        Ok(Self {
            hash: hex32(&block.hash)?,
            target,
            height: block.height,
            reward: block.coinbasevalue,
            network_difficulty: hash_difficulty(&target_le),
            solved: AtomicBool::new(false),
        })
    }

    /// Whether `pow` (a little-endian number) is a proof of work for this block.
    /// True only for the first hash that is.
    pub fn solved_by(&self, pow: &[u8; 32]) -> bool {
        pow.iter().rev().le(self.target.iter()) && !self.solved.swap(true, Ordering::Relaxed)
    }

    /// What the parent's coinbase script must contain: the magic bytes, the root of the
    /// tree of merged blocks, the tree's size and a nonce. With a single merged chain
    /// the tree is just that chain's block.
    pub fn commitment(&self) -> Vec<u8> {
        let mut commitment = MAGIC.to_vec();
        commitment.extend_from_slice(&self.hash);
        commitment.extend_from_slice(&1u32.to_le_bytes());
        commitment.extend_from_slice(&0u32.to_le_bytes());
        commitment
    }
}

/// The proof handed to the merged chain: the parent's coinbase transaction, its path to
/// the parent block's merkle root, and the parent block header.
pub fn auxpow(coinbase: &[u8], coinbase_branches: &[[u8; 32]], parent_header: &[u8]) -> Vec<u8> {
    let mut proof = coinbase.to_vec();
    proof.extend_from_slice(&[0u8; 32]); // the parent block's hash; nobody reads it
    write_varint(&mut proof, coinbase_branches.len() as u64);
    coinbase_branches.iter().for_each(|branch| proof.extend_from_slice(branch));
    proof.extend_from_slice(&0i32.to_le_bytes()); // the coinbase is the first transaction
    write_varint(&mut proof, 0); // no path in the tree of merged blocks: it has one leaf
    proof.extend_from_slice(&0i32.to_le_bytes());
    proof.extend_from_slice(parent_header);
    proof
}

/// Keeps asking the merged chain's daemon for the block it wants mined next and wakes
/// the parent coin's template poller whenever that changes.
pub async fn run(rpc: RpcClient, coin: Coin, parent: Arc<Pool>, work: watch::Sender<Option<Arc<AuxWork>>>) {
    let mut reported = false;
    let address = loop {
        match payout_address(&rpc, &coin).await {
            Ok(address) => break address,
            Err(err) => {
                if !reported {
                    warn!(coin = %coin.symbol, "daemon not usable yet, retrying: {err:#}");
                    reported = true;
                }
                tokio::time::sleep(DAEMON_RETRY).await;
            }
        }
    };
    let mut last_hash = String::new();
    let mut last_error = None;
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    loop {
        tick.tick().await;
        let block = match rpc.call::<AuxBlock>("createauxblock", json!([address])).await {
            Ok(block) => block,
            Err(err) => {
                let message = format!("{err:#}");
                if last_error.as_ref() != Some(&message) {
                    warn!(coin = %coin.symbol, "no block to merge-mine: {message}");
                    last_error = Some(message);
                    // Better to mine the parent alone than to commit to a stale block.
                    work.send_replace(None);
                    last_hash.clear();
                    parent.refresh.notify_one();
                }
                continue;
            }
        };
        last_error = None;
        if block.hash == last_hash {
            continue;
        }
        match AuxWork::from_rpc(&block) {
            Ok(aux) => {
                info!(coin = %coin.symbol, height = aux.height, "new block to merge-mine");
                work.send_replace(Some(Arc::new(aux)));
                last_hash = block.hash;
                parent.refresh.notify_one();
            }
            Err(err) => warn!(coin = %coin.symbol, "unusable aux block: {err:#}"),
        }
    }
}

#[cfg(test)]
impl AuxWork {
    pub fn for_test(hash: [u8; 32], target: [u8; 32]) -> Self {
        Self { hash, target, height: 700, reward: 9, network_difficulty: 2.0, solved: AtomicBool::new(false) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commitment_is_magic_root_size_and_nonce() {
        let aux = AuxWork::for_test([7; 32], [0xff; 32]);
        let commitment = aux.commitment();
        assert_eq!(commitment.len(), 44);
        assert_eq!(&commitment[..4], &[0xfa, 0xbe, 0x6d, 0x6d]);
        assert_eq!(&commitment[4..36], &[7; 32]);
        assert_eq!(&commitment[36..], &[1, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn proof_carries_coinbase_branch_and_header() {
        let proof = auxpow(&[0xaa; 10], &[[1; 32], [2; 32]], &[0xbb; 80]);
        assert_eq!(proof.len(), 10 + 32 + 1 + 64 + 4 + 1 + 4 + 80);
        assert_eq!(proof[42], 2, "two branch hashes");
        assert!(proof.ends_with(&[0xbb; 80]));
    }
}
