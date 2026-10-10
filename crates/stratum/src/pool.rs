use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU32, Ordering},
    },
};

use tokio::sync::Notify;

use crate::{algo::Algo, chain::Chain, job::Job, vardiff::VarDiffConfig};

/// What every connection on one stratum port has in common. A port serves one algorithm,
/// and with it one block layout, for any number of coins.
pub struct Port {
    pub algo: Algo,
    pub chain: Chain,
    pub vardiff: VarDiffConfig,
    instance_id: u8,
    next_extranonce1: AtomicU32,
}

impl Port {
    pub fn new(algo: Algo, chain: Chain, vardiff: VarDiffConfig, instance_id: u8) -> Self {
        Self { algo, chain, vardiff, instance_id, next_extranonce1: AtomicU32::new(1) }
    }

    /// A per-connection counter, led by the instance id where the chain has room for it.
    /// Distinct per connection until the counter wraps (after 2^24 connections, or 2^16
    /// on Xaya-style chains).
    pub fn next_extranonce1(&self) -> Vec<u8> {
        let counter = self.next_extranonce1.fetch_add(1, Ordering::Relaxed).to_be_bytes();
        let size = self.chain.extranonce1_size();
        if self.chain.instance_in_extranonce() {
            let mut extranonce1 = vec![self.instance_id];
            extranonce1.extend_from_slice(&counter[4 - (size - 1)..]);
            extranonce1
        } else {
            counter[4 - size..].to_vec()
        }
    }
}

/// The current work of one coin.
#[derive(Default)]
pub struct Pool {
    /// Wakes the template poller ahead of its timer, e.g. right after we found a block.
    pub refresh: Notify,
    jobs: RwLock<HashMap<String, Arc<Job>>>,
}

impl Pool {
    pub fn job(&self, id: &str) -> Option<Arc<Job>> {
        self.jobs.read().unwrap().get(id).cloned()
    }

    /// A clean job means a new chain tip, which makes all earlier jobs stale.
    pub fn add_job(&self, job: Arc<Job>) {
        let mut jobs = self.jobs.write().unwrap();
        if job.clean {
            jobs.clear();
        }
        jobs.insert(job.id.clone(), job);
    }
}
