use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use pool_common::rpc::RpcClient;
use serde_json::Value;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::{
    chain::Chain,
    job::{BlockTemplate, Job},
    pool::Pool,
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Even without new transactions, refresh so miners get a current ntime.
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Polls the daemon for block templates and publishes a job whenever the work changes.
pub async fn run(
    rpc: RpcClient,
    pool: Arc<Pool>,
    chain: Chain,
    segwit: bool,
    instance_id: u8,
    payout_script: Vec<u8>,
    jobs: watch::Sender<Option<Arc<Job>>>,
) {
    let request: Value = chain.template_request(segwit);
    let mut last: Option<(String, Vec<String>, Instant)> = None;
    let mut last_error = None;
    let mut next_id = 1u64;
    let mut tick = tokio::time::interval(POLL_INTERVAL);

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = pool.refresh.notified() => {}
        }
        let template: BlockTemplate = match rpc.call("getblocktemplate", request.clone()).await {
            Ok(template) => template,
            Err(err) => {
                let message = format!("{err:#}");
                if last_error.as_ref() != Some(&message) {
                    warn!("no block template: {message}");
                    last_error = Some(message);
                }
                continue;
            }
        };
        last_error = None;

        let txids: Vec<String> = template.transactions.iter().map(|tx| tx.hash.clone()).collect();
        let (clean, changed) = match &last {
            None => (true, true),
            Some((prev, last_txids, at)) => {
                let clean = *prev != template.previousblockhash;
                (clean, clean || *last_txids != txids || at.elapsed() >= REFRESH_INTERVAL)
            }
        };
        if !changed {
            continue;
        }

        match Job::new(format!("{next_id:x}"), chain, instance_id, &template, &payout_script, clean) {
            Ok(job) => {
                next_id += 1;
                if clean {
                    info!(height = job.height, txs = txids.len(), "new block template");
                }
                let job = Arc::new(job);
                pool.add_job(job.clone());
                jobs.send_replace(Some(job));
                last = Some((template.previousblockhash, txids, Instant::now()));
            }
            Err(err) => warn!("unusable block template: {err:#}"),
        }
    }
}
