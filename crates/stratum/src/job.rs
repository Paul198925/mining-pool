use std::{collections::HashSet, sync::Mutex};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;

use crate::{
    algo::Algo,
    chain::Chain,
    util::{hash_difficulty, hash_from_display_hex, hex32, push_script_number, sha256d, write_varint},
};

/// Nodes reject blocks dated more than two hours ahead.
const MAX_NTIME_AHEAD: u32 = 7200;
const COINBASE_TAG: &[u8] = b"/pool/";
/// Xaya's PowData algo byte for standalone Neoscrypt.
const XAYA_ALGO_NEOSCRYPT: u8 = 0x02;
/// The fake header's version and previous-hash fields mean nothing; these are the values miners are sent.
const XAYA_FAKE_VERSION: [u8; 4] = [0x20, 0, 0, 0];

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
    /// Output script committing to the witness data, on segwit chains.
    pub default_witness_commitment: Option<String>,
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
    curtime: u32,
    mintime: u32,
    /// Network target, big-endian.
    target: [u8; 32],
    extranonce2_size: usize,
    tx_data: Vec<Vec<u8>>,
    work: Work,
    seen: Mutex<HashSet<(Vec<u8>, u32, u32)>>,
}

enum Work {
    Bitcoin {
        version: u32,
        prev_hash: [u8; 32],
        nbits: u32,
        coinb1: Vec<u8>,
        coinb2: Vec<u8>,
        branches: Vec<[u8; 32]>,
    },
    Xaya {
        /// The block header up to and including its zeroed nBits; the extranonce completes it.
        header_prefix: Vec<u8>,
        coinbase: Vec<u8>,
        /// Compact target of the proof of work. Lives in the PowData, not in the block header.
        pow_bits: u32,
    },
}

impl Job {
    pub fn new(id: String, chain: Chain, template: &BlockTemplate, payout_script: &[u8], clean: bool) -> Result<Self> {
        let prev_hash = hash_from_display_hex(&template.previousblockhash)?;
        let nbits = u32::from_str_radix(&template.bits, 16).context("template bits")?;
        let mut txids = Vec::with_capacity(template.transactions.len());
        let mut tx_data = Vec::with_capacity(template.transactions.len());
        for tx in &template.transactions {
            txids.push(hash_from_display_hex(tx.txid.as_deref().unwrap_or(&tx.hash))?);
            tx_data.push(hex::decode(&tx.data).context("template transaction data")?);
        }
        let branches = merkle_branches(txids);
        let commitment = template
            .default_witness_commitment
            .as_deref()
            .map(hex::decode)
            .transpose()
            .context("template witness commitment")?;
        let ntime = format!("{:08x}", template.curtime);

        let (work, params) = match chain {
            Chain::Bitcoin => {
                let extranonce_len = chain.extranonce1_size() + chain.extranonce2_size();
                let (coinb1, coinb2) = build_coinbase(template, payout_script, commitment.as_deref(), extranonce_len);
                let params = json!([
                    id,
                    stratum_prev_hash(&prev_hash),
                    hex::encode(&coinb1),
                    hex::encode(&coinb2),
                    branches.iter().map(hex::encode).collect::<Vec<_>>(),
                    format!("{:08x}", template.version),
                    format!("{nbits:08x}"),
                    ntime,
                    clean,
                ]);
                (Work::Bitcoin { version: template.version, prev_hash, nbits, coinb1, coinb2, branches }, params)
            }
            Chain::Xaya => {
                // Nothing in the coinbase varies, so the block header is fixed up to its nonce.
                let (coinb1, coinb2) = build_coinbase(template, payout_script, commitment.as_deref(), 0);
                let coinbase = [coinb1, coinb2].concat();
                let root = fold_branches(sha256d(&coinbase), &branches);
                let mut header_prefix = Vec::with_capacity(76);
                header_prefix.extend_from_slice(&template.version.to_le_bytes());
                header_prefix.extend_from_slice(&prev_hash);
                header_prefix.extend_from_slice(&root);
                header_prefix.extend_from_slice(&template.curtime.to_le_bytes());
                header_prefix.extend_from_slice(&[0u8; 4]); // nBits must be zero, the real ones are in the PowData
                // To the miner the block header plays the role of the coinbase: it appends the
                // extranonces, hashes the result, and that hash is the "merkle root" of the
                // header it grinds, which is the fake header.
                let params = json!([
                    id,
                    "00".repeat(32),
                    hex::encode(&header_prefix),
                    "",
                    [],
                    hex::encode(XAYA_FAKE_VERSION),
                    format!("{nbits:08x}"),
                    ntime,
                    clean,
                ]);
                (Work::Xaya { header_prefix, coinbase, pow_bits: nbits }, params)
            }
        };

        Ok(Self {
            notify: json!({ "id": null, "method": "mining.notify", "params": params }).to_string(),
            id,
            height: template.height,
            clean,
            reward: template.coinbasevalue,
            curtime: template.curtime,
            mintime: template.mintime,
            target: hex32(&template.target)?,
            extranonce2_size: chain.extranonce2_size(),
            tx_data,
            work,
            seen: Mutex::new(HashSet::new()),
        })
    }

    /// Rebuilds what a miner claims to have hashed and scores it.
    pub fn check_share(
        &self,
        algo: Algo,
        extranonce1: &[u8],
        extranonce2: &[u8],
        ntime: u32,
        nonce: u32,
    ) -> Result<Share, ShareError> {
        if extranonce2.len() != self.extranonce2_size {
            return Err(ShareError::Malformed);
        }
        if ntime < self.mintime || ntime > self.curtime.saturating_add(MAX_NTIME_AHEAD) {
            return Err(ShareError::BadNtime);
        }
        let extranonce = [extranonce1, extranonce2].concat();
        if !self.seen.lock().unwrap().insert((extranonce.clone(), ntime, nonce)) {
            return Err(ShareError::Duplicate);
        }

        // `block_start` is everything that precedes the transaction list in the serialized block.
        let (pow, header_hash, block_start, coinbase) = match &self.work {
            Work::Bitcoin { version, prev_hash, nbits, coinb1, coinb2, branches } => {
                let coinbase = [&coinb1[..], &extranonce, coinb2].concat();
                let root = fold_branches(sha256d(&coinbase), branches);
                let mut header = [0u8; 80];
                header[0..4].copy_from_slice(&version.to_le_bytes());
                header[4..36].copy_from_slice(prev_hash);
                header[36..68].copy_from_slice(&root);
                header[68..72].copy_from_slice(&ntime.to_le_bytes());
                header[72..76].copy_from_slice(&nbits.to_le_bytes());
                header[76..80].copy_from_slice(&nonce.to_le_bytes());
                (algo.hash(&header), sha256d(&header), header.to_vec(), coinbase)
            }
            Work::Xaya { header_prefix, coinbase, pow_bits } => {
                let header = [&header_prefix[..], &extranonce].concat();
                let header_hash = sha256d(&header);
                // The fake header, byte for byte as the miner's stratum fields spell it.
                let mut fake = [0u8; 80];
                fake[0..4].copy_from_slice(&XAYA_FAKE_VERSION);
                fake[36..68].copy_from_slice(&header_hash);
                fake[68..72].copy_from_slice(&ntime.to_be_bytes());
                fake[72..76].copy_from_slice(&pow_bits.to_be_bytes());
                fake[76..80].copy_from_slice(&nonce.to_be_bytes());
                // Xaya hashes it with every 4-byte word reversed (its "getwork endianness").
                let mut input = fake;
                input.chunks_exact_mut(4).for_each(<[u8]>::reverse);

                let mut block_start = header;
                block_start.push(XAYA_ALGO_NEOSCRYPT);
                block_start.extend_from_slice(&pow_bits.to_le_bytes());
                block_start.extend_from_slice(&fake);
                (algo.hash(&input), header_hash, block_start, coinbase.clone())
            }
        };

        let mut pow_be = pow;
        pow_be.reverse();
        let block = (pow_be <= self.target).then(|| {
            let mut raw = block_start;
            write_varint(&mut raw, 1 + self.tx_data.len() as u64);
            raw.extend_from_slice(&coinbase);
            self.tx_data.iter().for_each(|tx| raw.extend_from_slice(tx));
            let mut hash = header_hash;
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

/// Builds the coinbase transaction, split around `extranonce_len` bytes the miner fills in.
/// With `extranonce_len == 0` the two halves simply concatenate to the whole transaction.
fn build_coinbase(
    template: &BlockTemplate,
    payout_script: &[u8],
    witness_commitment: Option<&[u8]>,
    extranonce_len: usize,
) -> (Vec<u8>, Vec<u8>) {
    let mut script_head = Vec::new();
    push_script_number(&mut script_head, template.height);
    if extranonce_len > 0 {
        script_head.push(extranonce_len as u8);
    }
    let mut script_tail = vec![COINBASE_TAG.len() as u8];
    script_tail.extend_from_slice(COINBASE_TAG);
    let script_len = script_head.len() + extranonce_len + script_tail.len();

    let mut coinb1 = Vec::new();
    coinb1.extend_from_slice(&1u32.to_le_bytes()); // tx version
    coinb1.push(1); // input count
    coinb1.extend_from_slice(&[0u8; 32]); // null prevout
    coinb1.extend_from_slice(&u32::MAX.to_le_bytes());
    write_varint(&mut coinb1, script_len as u64);
    coinb1.extend_from_slice(&script_head);

    let mut coinb2 = script_tail;
    coinb2.extend_from_slice(&u32::MAX.to_le_bytes()); // sequence
    coinb2.push(1 + u8::from(witness_commitment.is_some())); // output count
    coinb2.extend_from_slice(&template.coinbasevalue.to_le_bytes());
    write_varint(&mut coinb2, payout_script.len() as u64);
    coinb2.extend_from_slice(payout_script);
    if let Some(commitment) = witness_commitment {
        // Serialized without witness data; the node adds the coinbase's reserved value itself.
        coinb2.extend_from_slice(&0u64.to_le_bytes());
        write_varint(&mut coinb2, commitment.len() as u64);
        coinb2.extend_from_slice(commitment);
    }
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

fn fold_branches(leaf: [u8; 32], branches: &[[u8; 32]]) -> [u8; 32] {
    branches.iter().fold(leaf, |root, branch| sha256d(&[root, *branch].concat()))
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
            default_witness_commitment: None,
        }
    }

    fn naive_merkle_root(mut level: Vec<[u8; 32]>) -> [u8; 32] {
        while level.len() > 1 {
            level = level.chunks(2).map(|p| sha256d(&[p[0], p[p.len() - 1]].concat())).collect();
        }
        level[0]
    }

    fn template_txids(template: &BlockTemplate) -> Vec<[u8; 32]> {
        template.transactions.iter().map(|t| hash_from_display_hex(t.txid.as_ref().unwrap()).unwrap()).collect()
    }

    #[test]
    fn branches_fold_to_the_full_tree_root() {
        let coinbase = sha256d(b"coinbase");
        for n in 0..=9u8 {
            let others: Vec<[u8; 32]> = (0..n).map(|i| sha256d(&[i])).collect();
            let mut all = vec![coinbase];
            all.extend(&others);
            assert_eq!(fold_branches(coinbase, &merkle_branches(others)), naive_merkle_root(all), "{n} transactions");
        }
    }

    #[test]
    fn prev_hash_is_word_swapped() {
        let internal: [u8; 32] = std::array::from_fn(|i| i as u8);
        assert!(stratum_prev_hash(&internal).starts_with("0302010007060504"));
    }

    #[test]
    fn coinbase_script_length_covers_the_extranonce() {
        let (coinb1, coinb2) = build_coinbase(&easy_template(0), &[0xac], None, 8);
        let script_len = coinb1[41] as usize;
        let script_bytes = (coinb1.len() - 42) + 8 + 1 + COINBASE_TAG.len();
        assert_eq!(script_len, script_bytes);
        assert_eq!(&coinb2[1..1 + COINBASE_TAG.len()], COINBASE_TAG);
    }

    #[test]
    fn witness_commitment_becomes_a_second_output() {
        let commitment = [0x6a, 0x24, 0xaa, 0x21];
        let (_, plain) = build_coinbase(&easy_template(0), &[0xac], None, 0);
        let (_, with) = build_coinbase(&easy_template(0), &[0xac], Some(&commitment), 0);
        let outputs_at = 1 + COINBASE_TAG.len() + 4;
        assert_eq!((plain[outputs_at], with[outputs_at]), (1, 2));
        assert_eq!(with.len(), plain.len() + 8 + 1 + commitment.len());
        assert!(with.ends_with(&[0x6a, 0x24, 0xaa, 0x21, 0, 0, 0, 0]));
    }

    #[test]
    fn share_becomes_a_well_formed_block() {
        let template = easy_template(3);
        let job = Job::new("1".into(), Chain::Bitcoin, &template, &[0x51], true).unwrap();
        let share = job.check_share(Algo::Scrypt, &[0, 0, 0, 1], &[0, 0, 0, 2], template.curtime, 7).unwrap();
        let raw = hex::decode(share.block.expect("easy target").hex).unwrap();

        let Work::Bitcoin { coinb1, coinb2, .. } = &job.work else { panic!("bitcoin job") };
        let coinbase = [&coinb1[..], &[0, 0, 0, 1, 0, 0, 0, 2], coinb2].concat();
        let mut txids = vec![sha256d(&coinbase)];
        txids.extend(template_txids(&template));
        assert_eq!(&raw[36..68], &naive_merkle_root(txids));
        assert_eq!(raw[80], 4, "transaction count");
        assert_eq!(&raw[81..81 + coinbase.len()], &coinbase[..]);
        assert_eq!(raw.len(), 81 + coinbase.len() + 3 * 60);
    }

    #[test]
    fn xaya_block_carries_powdata_with_a_fake_header_committing_to_the_real_one() {
        let template = easy_template(2);
        let job = Job::new("1".into(), Chain::Xaya, &template, &[0x51], true).unwrap();
        let (ntime, nonce) = (template.curtime + 5, 0x0102_0304);
        let share = job.check_share(Algo::Neoscrypt, &[0xaa, 0xbb], &[0xcc, 0xdd], ntime, nonce).unwrap();
        let raw = hex::decode(share.block.expect("easy target").hex).unwrap();

        // Real header: template fields, zero nBits, extranonces as the nonce.
        let Work::Xaya { coinbase, .. } = &job.work else { panic!("xaya job") };
        let mut txids = vec![sha256d(coinbase)];
        txids.extend(template_txids(&template));
        assert_eq!(&raw[0..4], &template.version.to_le_bytes());
        assert_eq!(&raw[36..68], &naive_merkle_root(txids));
        assert_eq!(&raw[68..72], &template.curtime.to_le_bytes(), "real header keeps the job's time");
        assert_eq!(&raw[72..80], &[0, 0, 0, 0, 0xaa, 0xbb, 0xcc, 0xdd]);

        // PowData: algo, bits, fake header.
        assert_eq!(raw[80], 0x02);
        assert_eq!(&raw[81..85], &0x207f_ffffu32.to_le_bytes());
        let fake = &raw[85..165];
        assert_eq!(&fake[36..68], &sha256d(&raw[0..80]));
        assert_eq!(&fake[68..72], &ntime.to_be_bytes());
        assert_eq!(&fake[76..80], &[1, 2, 3, 4]);

        assert_eq!(raw[165], 3, "transaction count");
        assert_eq!(&raw[166..166 + coinbase.len()], &coinbase[..]);
        let mut block_hash = sha256d(&raw[0..80]);
        block_hash.reverse();
        assert_eq!(job.check_share(Algo::Neoscrypt, &[0xaa, 0xbb], &[0, 0], ntime, 1).unwrap().block.unwrap().hash.len(), 64);
        assert_eq!(hex::encode(block_hash).len(), 64);
    }

    #[test]
    fn xaya_notify_presents_the_block_header_as_the_coinbase() {
        let job = Job::new("1".into(), Chain::Xaya, &easy_template(0), &[0x51], true).unwrap();
        let notify: serde_json::Value = serde_json::from_str(&job.notify).unwrap();
        let params = &notify["params"];
        assert_eq!(params[1], "00".repeat(32));
        assert_eq!(params[2].as_str().unwrap().len(), 76 * 2);
        assert_eq!(params[3], "");
        assert_eq!(params[4], json!([]));
        assert_eq!(params[5], "20000000");
        assert_eq!(params[6], "207fffff");
    }

    #[test]
    fn rejects_duplicates_and_bad_input() {
        let template = easy_template(0);
        let job = Job::new("1".into(), Chain::Bitcoin, &template, &[0x51], true).unwrap();
        let en1 = [0, 0, 0, 1];
        assert!(job.check_share(Algo::Scrypt, &en1, &[0; 4], template.curtime, 1).is_ok());
        assert_eq!(job.check_share(Algo::Scrypt, &en1, &[0; 4], template.curtime, 1).err(), Some(ShareError::Duplicate));
        assert_eq!(job.check_share(Algo::Scrypt, &en1, &[0; 3], template.curtime, 2).err(), Some(ShareError::Malformed));
        assert_eq!(job.check_share(Algo::Scrypt, &en1, &[0; 4], template.mintime - 1, 3).err(), Some(ShareError::BadNtime));
    }
}
