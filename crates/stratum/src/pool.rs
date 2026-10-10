use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU32, Ordering},
    },
};

use tokio::sync::Notify;

use crate::{algo::Algo, chain::Chain, job::Job, vardiff::VarDiffConfig};

/// State shared by every connection of one stratum listener.
pub struct Pool {
    pub algo: Algo,
    pub chain: Chain,
    pub vardiff: VarDiffConfig,
    /// Wakes the template poller ahead of its timer, e.g. right after we found a block.
    pub refresh: Notify,
    jobs: RwLock<HashMap<String, Arc<Job>>>,
    next_extranonce1: AtomicU32,
}

impl Pool {
    pub fn new(algo: Algo, chain: Chain, vardiff: VarDiffConfig) -> Self {
        Self {
            algo,
            chain,
            vardiff,
            refresh: Notify::new(),
            jobs: RwLock::new(HashMap::new()),
            next_extranonce1: AtomicU32::new(1),
        }
    }

    /// Distinct per connection until the counter wraps around the chain's extranonce size.
    pub fn next_extranonce1(&self) -> Vec<u8> {
        let counter = self.next_extranonce1.fetch_add(1, Ordering::Relaxed).to_be_bytes();
        counter[4 - self.chain.extranonce1_size()..].to_vec()
    }

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
