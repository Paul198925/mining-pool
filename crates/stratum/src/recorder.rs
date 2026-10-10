use std::{
    collections::VecDeque,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use crate::backend::ApiClient;

const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
const FLUSH_AT: usize = 1000;
const QUEUE_SIZE: usize = 100_000;
/// With the database down, keep at most this many shares in memory.
const MAX_BUFFERED: usize = 500_000;
const BLOCK_INSERT_ATTEMPTS: u32 = 10;
/// With the pool unreachable, keep at most this many undelivered batches (one per second).
const MAX_OUTBOX: usize = 3600;

#[derive(Debug, Clone, Copy)]
pub struct ShareEvent {
    pub account_id: i64,
    pub worker_id: i64,
    pub solo: bool,
    pub party_id: Option<i64>,
    /// In diff-1 units, i.e. already divided by the algo's stratum multiplier.
    pub difficulty: f64,
    /// Unix time in seconds.
    pub at: f64,
}

#[derive(Debug)]
pub struct BlockEvent {
    pub account_id: i64,
    pub worker_id: i64,
    pub solo: bool,
    pub party_id: Option<i64>,
    pub height: u64,
    pub hash: String,
    pub reward: u64,
    pub network_difficulty: f64,
    pub at: f64,
}

/// Where one coin's shares and blocks are stored.
pub enum Sink {
    Direct { db: PgPool, coin_id: i32 },
    Api { api: ApiClient, symbol: String },
}

enum Event {
    Share(ShareEvent),
    Block(BlockEvent),
    Flush(oneshot::Sender<()>),
}

/// Hands shares and found blocks to a background task that stores them.
#[derive(Clone)]
pub struct Recorder {
    events: mpsc::Sender<Event>,
}

impl Recorder {
    pub fn spawn(sink: Sink) -> Self {
        let (events, queue) = mpsc::channel(QUEUE_SIZE);
        tokio::spawn(Writer::new(sink).run(queue));
        Self { events }
    }

    pub fn share(&self, share: ShareEvent) {
        if self.events.try_send(Event::Share(share)).is_err() {
            error!("share queue is full, share lost");
        }
    }

    /// Events are written in order, so every share sent before this is stored before the block.
    pub async fn block(&self, block: BlockEvent) {
        if self.events.send(Event::Block(block)).await.is_err() {
            error!("recorder stopped, found block was not stored");
        }
    }

    /// Waits until everything queued so far is written, or at least tried once.
    pub async fn flush(&self) {
        let (done, wait) = oneshot::channel();
        if self.events.send(Event::Flush(done)).await.is_ok() {
            let _ = wait.await;
        }
    }
}

/// A set of events on its way to the pool's node API. Its id lets the pool recognise a repeat.
struct Batch {
    id: String,
    events: Vec<Value>,
    has_block: bool,
}

struct Writer {
    sink: Sink,
    shares: Vec<ShareEvent>,
    outbox: VecDeque<Batch>,
    /// Makes batch ids unique across restarts of this process.
    run_id: u128,
    next_batch: u64,
}

impl Writer {
    fn new(sink: Sink) -> Self {
        let run_id = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        Self { sink, shares: Vec::new(), outbox: VecDeque::new(), run_id, next_batch: 0 }
    }

    async fn run(mut self, mut queue: mpsc::Receiver<Event>) {
        let mut tick = tokio::time::interval(FLUSH_INTERVAL);
        loop {
            tokio::select! {
                event = queue.recv() => match event {
                    None => break,
                    Some(Event::Share(share)) => {
                        self.shares.push(share);
                        if self.shares.len() >= FLUSH_AT {
                            self.flush(None).await;
                        }
                    }
                    Some(Event::Block(block)) => self.flush(Some(block)).await,
                    Some(Event::Flush(done)) => {
                        self.flush(None).await;
                        let _ = done.send(());
                    }
                },
                _ = tick.tick() => self.flush(None).await,
            }
        }
        self.flush(None).await;
    }

    /// Stores the pending shares and then, if given, a block found after them.
    async fn flush(&mut self, block: Option<BlockEvent>) {
        match &self.sink {
            Sink::Direct { db, coin_id } => {
                store_shares(db, *coin_id, &mut self.shares).await;
                if let Some(block) = block {
                    insert_block(db, *coin_id, &block).await;
                }
            }
            Sink::Api { .. } => {
                self.seal_batch(block);
                self.deliver().await;
            }
        }
    }

    /// Moves the pending events into a batch whose content never changes again,
    /// so that repeating it after a lost answer cannot count anything twice.
    fn seal_batch(&mut self, block: Option<BlockEvent>) {
        if self.shares.is_empty() && block.is_none() {
            return;
        }
        let mut events: Vec<Value> = self
            .shares
            .drain(..)
            .map(|s| {
                json!({
                    "type": "share", "account_id": s.account_id, "worker_id": s.worker_id, "solo": s.solo,
                    "party_id": s.party_id, "difficulty": s.difficulty, "at": s.at,
                })
            })
            .collect();
        let has_block = block.is_some();
        if let Some(b) = block {
            events.push(json!({
                "type": "block", "account_id": b.account_id, "worker_id": b.worker_id, "solo": b.solo,
                "party_id": b.party_id, "height": b.height, "hash": b.hash, "reward": b.reward,
                "network_difficulty": b.network_difficulty, "at": b.at,
            }));
        }
        self.next_batch += 1;
        self.outbox.push_back(Batch { id: format!("{:x}-{}", self.run_id, self.next_batch), events, has_block });
        while self.outbox.len() > MAX_OUTBOX {
            let dropped = self.outbox.pop_front().expect("outbox is not empty");
            error!(batch = %dropped.id, events = dropped.events.len(), block = dropped.has_block, "pool unreachable for too long, batch lost");
        }
    }

    /// Sends batches oldest first and stops at the first failure, which keeps their order.
    async fn deliver(&mut self) {
        let Sink::Api { api, symbol } = &self.sink else { return };
        while let Some(batch) = self.outbox.front() {
            let body = json!({ "batch": batch.id, "symbol": symbol, "events": batch.events });
            match api.post::<Value>("/api/node/events", &body).await {
                Ok(_) => {
                    if batch.has_block {
                        info!(coin = %symbol, "block delivered to the pool");
                    }
                    self.outbox.pop_front();
                }
                Err(err) => {
                    warn!(waiting = self.outbox.len(), "could not deliver to the pool, will retry: {err:#}");
                    return;
                }
            }
        }
    }
}

async fn store_shares(db: &PgPool, coin_id: i32, shares: &mut Vec<ShareEvent>) {
    if shares.is_empty() {
        return;
    }
    let result = sqlx::query(
        "INSERT INTO shares (coin_id, account_id, worker_id, difficulty, created_at, solo, party_id)
         SELECT $1, s.account_id, s.worker_id, s.difficulty, to_timestamp(s.at), s.solo, s.party_id
         FROM UNNEST($2::int8[], $3::int8[], $4::float8[], $5::float8[], $6::bool[], $7::int8[])
              AS s (account_id, worker_id, difficulty, at, solo, party_id)",
    )
    .bind(coin_id)
    .bind(shares.iter().map(|s| s.account_id).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.worker_id).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.difficulty).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.at).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.solo).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.party_id).collect::<Vec<_>>())
    .execute(db)
    .await;
    match result {
        Ok(_) => shares.clear(),
        // Keep them for the next attempt.
        Err(err) => {
            warn!(buffered = shares.len(), "could not store shares: {err}");
            if shares.len() > MAX_BUFFERED {
                let excess = shares.len() - MAX_BUFFERED;
                shares.drain(..excess);
                error!(lost = excess, "share buffer overflow");
            }
        }
    }
}

async fn insert_block(db: &PgPool, coin_id: i32, block: &BlockEvent) {
    for attempt in 1..=BLOCK_INSERT_ATTEMPTS {
        let result = sqlx::query(
            "INSERT INTO blocks
                 (coin_id, height, hash, account_id, worker_id, reward, network_difficulty, found_at, solo, party_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8), $9, $10)
             ON CONFLICT (coin_id, hash) DO NOTHING",
        )
        .bind(coin_id)
        .bind(block.height as i64)
        .bind(&block.hash)
        .bind(block.account_id)
        .bind(block.worker_id)
        .bind(block.reward as i64)
        .bind(block.network_difficulty)
        .bind(block.at)
        .bind(block.solo)
        .bind(block.party_id)
        .execute(db)
        .await;
        match result {
            Ok(_) => {
                info!(height = block.height, hash = %block.hash, "block recorded");
                return;
            }
            Err(err) => {
                warn!(attempt, "could not store block {}: {err}", block.hash);
                tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
            }
        }
    }
    // The block is on chain but unknown to accounting; this line is what a manual fix needs.
    error!(?block, "giving up storing found block");
}
