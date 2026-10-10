use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU32, Ordering},
    },
};

use tokio::sync::Notify;

use crate::{algo::Algo, job::Job, vardiff::VarDiffConfig};

/// State shared by every connection of one stratum listener.
pub struct Pool {
    pub algo: Algo,
    pub vardiff: VarDiffConfig,
    /// Wakes the template poller ahead of its timer, e.g. right after we found a block.
    pub refresh: Notify,
    jobs: RwLock<HashMap<String, Arc<Job>>>,
    next_extranonce1: AtomicU32,
}

impl Pool {
    pub fn new(algo: Algo, vardiff: VarDiffConfig) -> Self {
        Self { algo, vardiff, refresh: Notify::new(), jobs: RwLock::new(HashMap::new()), next_extranonce1: AtomicU32::new(1) }
    }

    pub fn next_extranonce1(&self) -> [u8; 4] {
        self.next_extranonce1.fetch_add(1, Ordering::Relaxed).to_be_bytes()
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
