use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
const FLUSH_AT: usize = 1000;
const QUEUE_SIZE: usize = 100_000;
/// With the database down, keep at most this many shares in memory.
const MAX_BUFFERED: usize = 500_000;
const BLOCK_INSERT_ATTEMPTS: u32 = 10;

#[derive(Debug, Clone, Copy)]
pub struct ShareEvent {
    pub account_id: i64,
    pub worker_id: i64,
    pub solo: bool,
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
    pub height: u64,
    pub hash: String,
    pub reward: u64,
    pub network_difficulty: f64,
    pub at: f64,
}

enum Event {
    Share(ShareEvent),
    Block(BlockEvent),
    Flush(oneshot::Sender<()>),
}

/// Hands shares and found blocks to a background task that writes them to the database.
#[derive(Clone)]
pub struct Recorder {
    events: mpsc::Sender<Event>,
}

impl Recorder {
    pub fn spawn(db: PgPool, coin_id: i32) -> Self {
        let (events, queue) = mpsc::channel(QUEUE_SIZE);
        tokio::spawn(run(db, coin_id, queue));
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

    /// Waits until everything queued so far is written.
    pub async fn flush(&self) {
        let (done, wait) = oneshot::channel();
        if self.events.send(Event::Flush(done)).await.is_ok() {
            let _ = wait.await;
        }
    }
}

async fn run(db: PgPool, coin_id: i32, mut queue: mpsc::Receiver<Event>) {
    let mut shares: Vec<ShareEvent> = Vec::new();
    let mut tick = tokio::time::interval(FLUSH_INTERVAL);
    loop {
        tokio::select! {
            event = queue.recv() => match event {
                None => break,
                Some(Event::Share(share)) => {
                    shares.push(share);
                    if shares.len() >= FLUSH_AT {
                        flush(&db, coin_id, &mut shares).await;
                    }
                }
                Some(Event::Block(block)) => {
                    flush(&db, coin_id, &mut shares).await;
                    insert_block(&db, coin_id, &block).await;
                }
                Some(Event::Flush(done)) => {
                    flush(&db, coin_id, &mut shares).await;
                    let _ = done.send(());
                }
            },
            _ = tick.tick() => flush(&db, coin_id, &mut shares).await,
        }
    }
    flush(&db, coin_id, &mut shares).await;
}

async fn flush(db: &PgPool, coin_id: i32, shares: &mut Vec<ShareEvent>) {
    if shares.is_empty() {
        return;
    }
    let result = sqlx::query(
        "INSERT INTO shares (coin_id, account_id, worker_id, difficulty, created_at, solo)
         SELECT $1, s.account_id, s.worker_id, s.difficulty, to_timestamp(s.at), s.solo
         FROM UNNEST($2::int8[], $3::int8[], $4::float8[], $5::float8[], $6::bool[])
              AS s (account_id, worker_id, difficulty, at, solo)",
    )
    .bind(coin_id)
    .bind(shares.iter().map(|s| s.account_id).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.worker_id).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.difficulty).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.at).collect::<Vec<_>>())
    .bind(shares.iter().map(|s| s.solo).collect::<Vec<_>>())
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
            "INSERT INTO blocks (coin_id, height, hash, account_id, worker_id, reward, network_difficulty, found_at, solo)
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8), $9)
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
