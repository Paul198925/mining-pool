use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use pool_common::{config::Coin, rpc::RpcClient};
use serde_json::{Value, json};
use tokio::sync::watch;
use tracing::{info, warn};

use crate::{
    chain::Chain,
    job::{BlockTemplate, Job},
    kawpow::{self, Epoch},
    pool::Pool,
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Even without new transactions, refresh so miners get a current ntime.
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const DAEMON_RETRY: Duration = Duration::from_secs(10);

/// The output script block rewards are paid to.
async fn payout_script(rpc: &RpcClient, coin: &Coin) -> Result<Vec<u8>> {
    let address = match &coin.pool_address {
        Some(address) => address.clone(),
        None => {
            let address: String = rpc.call("getnewaddress", json!([])).await?;
            warn!(%address, "no pool_address configured, paying rewards to a fresh daemon wallet address");
            address
        }
    };
    let info: Value = rpc.call("validateaddress", json!([address])).await?;
    ensure!(info["isvalid"] == true, "pool address {address} is not valid for {}", coin.symbol);
    let script = info["scriptPubKey"].as_str().context("validateaddress returned no scriptPubKey")?;
    Ok(hex::decode(script)?)
}

/// Polls the daemon for block templates and publishes a job whenever the work changes.
pub async fn run(
    rpc: RpcClient,
    pool: Arc<Pool>,
    chain: Chain,
    coin: Coin,
    instance_id: u8,
    jobs: watch::Sender<Option<Arc<Job>>>,
) {
    // The daemon may be on another server, or still starting: the other coins must not wait for it.
    let mut reported = false;
    let payout_script = loop {
        match payout_script(&rpc, &coin).await {
            Ok(script) => break script,
            Err(err) => {
                if !reported {
                    warn!(coin = %coin.symbol, "daemon not usable yet, retrying: {err:#}");
                    reported = true;
                }
                tokio::time::sleep(DAEMON_RETRY).await;
            }
        }
    };
    let request: Value = chain.template_request(&coin.template_rules);
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

        if chain == Chain::Ravencoin {
            // Seconds of work the first time an epoch is seen; `Job::new` then finds it ready.
            let epoch = Epoch::of_height(template.height);
            let built = tokio::task::spawn_blocking(move || kawpow::EPOCHS.get(epoch).map(drop)).await;
            if let Ok(Err(err)) = built {
                warn!("cannot prepare KawPoW epoch {epoch}: {err:#}");
                continue;
            }
            // Have the next epoch ready before its first block needs it.
            if template.height % kawpow::EPOCH_LENGTH >= kawpow::EPOCH_LENGTH - 30 {
                tokio::task::spawn_blocking(move || kawpow::EPOCHS.get(epoch + 1).map(drop));
            }
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
