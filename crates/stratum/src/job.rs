use std::{collections::HashSet, sync::Mutex};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;

use crate::{
    algo::Algo,
    util::{hash_difficulty, hash_from_display_hex, hex32, push_script_number, sha256d, write_varint},
};

pub const EXTRANONCE1_SIZE: usize = 4;
pub const EXTRANONCE2_SIZE: usize = 4;
/// Nodes reject blocks dated more than two hours ahead.
const MAX_NTIME_AHEAD: u32 = 7200;
const COINBASE_TAG: &[u8] = b"/pool/";

/// The subset of `getblocktemplate` this pool uses.
#[derive(Debug, Clone, Deserialize)]
pub struct BlockTemplate {
    pub version: u32,
    pub previousblockhash: String,
    pub transactions: Vec<TemplateTx>,
    pub coinbasevalue: u64,
    pub target: String,
    pub mintime: u32,
    pub curtime: u32,
    pub bits: String,
    pub height: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateTx {
    pub data: String,
    pub txid: Option<String>,
    pub hash: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ShareError {
    Malformed,
    BadNtime,
    Duplicate,
}

pub struct Share {
    /// In stratum units, i.e. comparable to what `mining.set_difficulty` sent.
    pub difficulty: f64,
    pub block: Option<FoundBlock>,
}

pub struct FoundBlock {
    pub hash: String,
    pub height: u64,
    pub reward: u64,
    /// In diff-1 units.
    pub network_difficulty: f64,
    pub hex: String,
}

/// One unit of work handed to miners, built from a block template.
pub struct Job {
    pub id: String,
    pub height: u64,
    /// Miners must drop older work when this is set (new chain tip).
    pub clean: bool,
    reward: u64,
    /// The `mining.notify` line; identical for every miner because the extranonce is sent separately.
    pub notify: String,
    version: u32,
    prev_hash: [u8; 32],
    nbits: u32,
    curtime: u32,
    mintime: u32,
    /// Network target, big-endian.
    target: [u8; 32],
    coinb1: Vec<u8>,
    coinb2: Vec<u8>,
    branches: Vec<[u8; 32]>,
    tx_data: Vec<Vec<u8>>,
    seen: Mutex<HashSet<([u8; EXTRANONCE1_SIZE + EXTRANONCE2_SIZE], u32, u32)>>,
}

impl Job {
    pub fn new(id: String, template: &BlockTemplate, payout_script: &[u8], clean: bool) -> Result<Self> {
        let prev_hash = hash_from_display_hex(&template.previousblockhash)?;
        let nbits = u32::from_str_radix(&template.bits, 16).context("template bits")?;
        let mut txids = Vec::with_capacity(template.transactions.len());
        let mut tx_data = Vec::with_capacity(template.transactions.len());
        for tx in &template.transactions {
            txids.push(hash_from_display_hex(tx.txid.as_deref().unwrap_or(&tx.hash))?);
            tx_data.push(hex::decode(&tx.data).context("template transaction data")?);
        }
        let branches = merkle_branches(txids);
        let (coinb1, coinb2) = build_coinbase(template.height, template.coinbasevalue, payout_script);

        let notify = json!({
            "id": null,
            "method": "mining.notify",
            "params": [
                id,
                stratum_prev_hash(&prev_hash),
                hex::encode(&coinb1),
                hex::encode(&coinb2),
                branches.iter().map(hex::encode).collect::<Vec<_>>(),
                format!("{:08x}", template.version),
                format!("{nbits:08x}"),
                format!("{:08x}", template.curtime),
                clean,
            ],
        })
        .to_string();

        Ok(Self {
            id,
            height: template.height,
            clean,
            reward: template.coinbasevalue,
            notify,
            version: template.version,
            prev_hash,
            nbits,
            curtime: template.curtime,
            mintime: template.mintime,
            target: hex32(&template.target)?,
            coinb1,
            coinb2,
            branches,
            tx_data,
            seen: Mutex::new(HashSet::new()),
        })
    }

    /// Rebuilds the block header a miner claims to have hashed and scores it.
    pub fn check_share(
        &self,
        algo: Algo,
        extranonce1: &[u8; EXTRANONCE1_SIZE],
        extranonce2: &[u8],
        ntime: u32,
        nonce: u32,
    ) -> Result<Share, ShareError> {
        let mut extranonce = [0u8; EXTRANONCE1_SIZE + EXTRANONCE2_SIZE];
        if extranonce2.len() != EXTRANONCE2_SIZE {
            return Err(ShareError::Malformed);
        }
        extranonce[..EXTRANONCE1_SIZE].copy_from_slice(extranonce1);
        extranonce[EXTRANONCE1_SIZE..].copy_from_slice(extranonce2);
        if ntime < self.mintime || ntime > self.curtime.saturating_add(MAX_NTIME_AHEAD) {
            return Err(ShareError::BadNtime);
        }
        if !self.seen.lock().unwrap().insert((extranonce, ntime, nonce)) {
            return Err(ShareError::Duplicate);
        }

        let coinbase = [&self.coinb1[..], &extranonce, &self.coinb2].concat();
        let mut root = sha256d(&coinbase);
        for branch in &self.branches {
            root = sha256d(&[root, *branch].concat());
        }

        let mut header = [0u8; 80];
        header[0..4].copy_from_slice(&self.version.to_le_bytes());
        header[4..36].copy_from_slice(&self.prev_hash);
        header[36..68].copy_from_slice(&root);
        header[68..72].copy_from_slice(&ntime.to_le_bytes());
        header[72..76].copy_from_slice(&self.nbits.to_le_bytes());
        header[76..80].copy_from_slice(&nonce.to_le_bytes());

        let pow = algo.hash(&header);
        let mut pow_be = pow;
        pow_be.reverse();
        let block = (pow_be <= self.target).then(|| {
            let mut raw = header.to_vec();
            write_varint(&mut raw, 1 + self.tx_data.len() as u64);
            raw.extend_from_slice(&coinbase);
            self.tx_data.iter().for_each(|tx| raw.extend_from_slice(tx));
            let mut hash = sha256d(&header);
            hash.reverse();
            let mut target_le = self.target;
            target_le.reverse();
            FoundBlock {
                hash: hex::encode(hash),
                height: self.height,
                reward: self.reward,
                network_difficulty: hash_difficulty(&target_le),
                hex: hex::encode(raw),
            }
        });

        Ok(Share { difficulty: hash_difficulty(&pow) * algo.diff_multiplier(), block })
    }
}

/// Splits the coinbase transaction around the 8 extranonce bytes.
fn build_coinbase(height: u64, value: u64, payout_script: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut script_head = Vec::new();
    push_script_number(&mut script_head, height);
    script_head.push((EXTRANONCE1_SIZE + EXTRANONCE2_SIZE) as u8);
    let mut script_tail = vec![COINBASE_TAG.len() as u8];
    script_tail.extend_from_slice(COINBASE_TAG);
    let script_len = script_head.len() + EXTRANONCE1_SIZE + EXTRANONCE2_SIZE + script_tail.len();

    let mut coinb1 = Vec::new();
    coinb1.extend_from_slice(&1u32.to_le_bytes()); // tx version
    coinb1.push(1); // input count
    coinb1.extend_from_slice(&[0u8; 32]); // null prevout
    coinb1.extend_from_slice(&u32::MAX.to_le_bytes());
    write_varint(&mut coinb1, script_len as u64);
    coinb1.extend_from_slice(&script_head);

    let mut coinb2 = script_tail;
    coinb2.extend_from_slice(&u32::MAX.to_le_bytes()); // sequence
    coinb2.push(1); // output count
    coinb2.extend_from_slice(&value.to_le_bytes());
    write_varint(&mut coinb2, payout_script.len() as u64);
    coinb2.extend_from_slice(payout_script);
    coinb2.extend_from_slice(&0u32.to_le_bytes()); // locktime
    (coinb1, coinb2)
}

/// Sibling hashes needed to fold the coinbase (always leaf 0) up to the merkle root.
/// `hashes` are the txids of every other transaction, in block order.
fn merkle_branches(mut hashes: Vec<[u8; 32]>) -> Vec<[u8; 32]> {
    let mut branches = Vec::new();
    while !hashes.is_empty() {
        branches.push(hashes[0]);
        hashes = hashes[1..]
            .chunks(2)
            .map(|pair| sha256d(&[pair[0], pair[pair.len() - 1]].concat()))
            .collect();
    }
    branches
}

/// Stratum sends the previous block hash with each 4-byte word byte-swapped.
fn stratum_prev_hash(internal: &[u8; 32]) -> String {
    internal.chunks(4).flat_map(|word| word.iter().rev()).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A template whose target accepts every hash, with `tx_count` dummy transactions.
    pub fn easy_template(tx_count: u8) -> BlockTemplate {
        BlockTemplate {
            version: 0x0062_0004,
            previousblockhash: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into(),
            transactions: (0..tx_count)
                .map(|i| TemplateTx {
                    data: hex::encode([i; 60]),
                    txid: Some(hex::encode(sha256d(&[i]))),
                    hash: String::new(),
                })
                .collect(),
            coinbasevalue: 500_000 * 100_000_000,
            target: "ff".repeat(32),
            mintime: 1_700_000_000,
            curtime: 1_700_000_100,
            bits: "207fffff".into(),
            height: 102,
        }
    }

    fn naive_merkle_root(mut level: Vec<[u8; 32]>) -> [u8; 32] {
        while level.len() > 1 {
            level = level.chunks(2).map(|p| sha256d(&[p[0], p[p.len() - 1]].concat())).collect();
        }
        level[0]
    }

    #[test]
    fn branches_fold_to_the_full_tree_root() {
        let coinbase = sha256d(b"coinbase");
        for n in 0..=9u8 {
            let others: Vec<[u8; 32]> = (0..n).map(|i| sha256d(&[i])).collect();
            let mut all = vec![coinbase];
            all.extend(&others);
            let folded = merkle_branches(others).iter().fold(coinbase, |root, b| sha256d(&[root, *b].concat()));
            assert_eq!(folded, naive_merkle_root(all), "{n} transactions");
        }
    }

    #[test]
    fn prev_hash_is_word_swapped() {
        let internal: [u8; 32] = std::array::from_fn(|i| i as u8);
        assert!(stratum_prev_hash(&internal).starts_with("0302010007060504"));
    }

    #[test]
    fn coinbase_script_length_covers_the_extranonce() {
        let (coinb1, coinb2) = build_coinbase(102, 1, &[0xac]);
        let script_len = coinb1[41] as usize;
        let script_bytes = (coinb1.len() - 42) + 8 + 1 + COINBASE_TAG.len();
        assert_eq!(script_len, script_bytes);
        assert_eq!(&coinb2[1..1 + COINBASE_TAG.len()], COINBASE_TAG);
    }

    #[test]
    fn share_becomes_a_well_formed_block() {
        let template = easy_template(3);
        let job = Job::new("1".into(), &template, &[0x51], true).unwrap();
        let share = job.check_share(Algo::Scrypt, &[0, 0, 0, 1], &[0, 0, 0, 2], template.curtime, 7).unwrap();
        let raw = hex::decode(share.block.expect("easy target").hex).unwrap();

        let coinbase = [&job.coinb1[..], &[0, 0, 0, 1, 0, 0, 0, 2], &job.coinb2].concat();
        let mut txids = vec![sha256d(&coinbase)];
        txids.extend(template.transactions.iter().map(|t| hash_from_display_hex(t.txid.as_ref().unwrap()).unwrap()));
        assert_eq!(&raw[36..68], &naive_merkle_root(txids));
        assert_eq!(raw[80], 4, "transaction count");
        assert_eq!(&raw[81..81 + coinbase.len()], &coinbase[..]);
        assert_eq!(raw.len(), 81 + coinbase.len() + 3 * 60);
    }

    #[test]
    fn rejects_duplicates_and_bad_input() {
        let template = easy_template(0);
        let job = Job::new("1".into(), &template, &[0x51], true).unwrap();
        let en1 = [0, 0, 0, 1];
        assert!(job.check_share(Algo::Scrypt, &en1, &[0; 4], template.curtime, 1).is_ok());
        assert_eq!(job.check_share(Algo::Scrypt, &en1, &[0; 4], template.curtime, 1).err(), Some(ShareError::Duplicate));
        assert_eq!(job.check_share(Algo::Scrypt, &en1, &[0; 3], template.curtime, 2).err(), Some(ShareError::Malformed));
        assert_eq!(job.check_share(Algo::Scrypt, &en1, &[0; 4], template.mintime - 1, 3).err(), Some(ShareError::BadNtime));
    }
}
