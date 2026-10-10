import { connection } from "next/server";

import { db } from "./db";

// Hashes per second = difficulty (diff-1 units) * 2^32 / seconds.
const HASHES_PER_DIFF = 4294967296;
const LIVE_WINDOW_SECS = 600;
const CHART_BUCKET_SECS = 600;

export type Coin = {
  symbol: string;
  name: string;
  algo: string;
  height: number | null;
  networkDifficulty: number | null;
  networkHashrate: number | null;
  stratumPort: number | null;
  blockMaturity: number | null;
  /** Base units (1e-8 coin) as a decimal string. */
  minPayout: string | null;
  poolFeePercent: number | null;
  hashrate: number;
  miners: number;
  workers: number;
  blocksFound: number;
  lastBlockAt: Date | null;
};

export type Block = {
  id: string;
  symbol: string;
  height: number;
  hash: string;
  status: "pending" | "confirmed" | "orphaned";
  confirmations: number;
  blockMaturity: number | null;
  reward: string;
  networkDifficulty: number;
  finder: string;
  foundAt: Date;
};

export type HashratePoint = { t: string; v: number };

export type Worker = { name: string; hashrate: number; lastShareAt: Date | null };

export type Payout = { id: string; amount: string; status: string; txid: string | null; createdAt: Date };

export type Credit = { height: number; status: Block["status"]; amount: string; foundAt: Date };

export type MinerAccount = {
  symbol: string;
  address: string;
  hashrate: number;
  balance: string;
  immature: string;
  paid: string;
  workers: Worker[];
  payouts: Payout[];
  credits: Credit[];
  history: HashratePoint[];
};

export async function getCoins(): Promise<Coin[]> {
  await connection();
  const { rows } = await db().query(
    `SELECT c.symbol, c.name, c.algo, c.height, c.network_difficulty, c.network_hashrate, c.stratum_port,
            c.block_maturity, c.min_payout, c.pool_fee_percent,
            COALESCE(live.difficulty, 0) * $1 / $2 AS hashrate,
            COALESCE(live.miners, 0) AS miners, COALESCE(live.workers, 0) AS workers,
            (SELECT count(*) FROM blocks b WHERE b.coin_id = c.id AND b.status <> 'orphaned') AS blocks_found,
            (SELECT max(found_at) FROM blocks b WHERE b.coin_id = c.id) AS last_block_at
     FROM coins c
     LEFT JOIN LATERAL (
         SELECT sum(difficulty) AS difficulty, count(DISTINCT account_id) AS miners, count(DISTINCT worker_id) AS workers
         FROM shares WHERE coin_id = c.id AND created_at > now() - make_interval(secs => $2)
     ) live ON TRUE
     WHERE c.enabled
     ORDER BY c.symbol`,
    [HASHES_PER_DIFF, LIVE_WINDOW_SECS],
  );
  return rows.map((r) => ({
    symbol: r.symbol,
    name: r.name,
    algo: r.algo,
    height: r.height === null ? null : Number(r.height),
    networkDifficulty: r.network_difficulty,
    networkHashrate: r.network_hashrate,
    stratumPort: r.stratum_port,
    blockMaturity: r.block_maturity,
    minPayout: r.min_payout,
    poolFeePercent: r.pool_fee_percent,
    hashrate: r.hashrate,
    miners: Number(r.miners),
    workers: Number(r.workers),
    blocksFound: Number(r.blocks_found),
    lastBlockAt: r.last_block_at,
  }));
}

export async function getBlocks(limit: number): Promise<Block[]> {
  await connection();
  const { rows } = await db().query(
    `SELECT b.id, c.symbol, b.height, b.hash, b.status, b.confirmations, c.block_maturity, b.reward,
            b.network_difficulty, a.payout_address AS finder, b.found_at
     FROM blocks b JOIN coins c ON c.id = b.coin_id JOIN accounts a ON a.id = b.account_id
     ORDER BY b.found_at DESC, b.id DESC
     LIMIT $1`,
    [limit],
  );
  return rows.map((r) => ({
    id: r.id,
    symbol: r.symbol,
    height: Number(r.height),
    hash: r.hash,
    status: r.status,
    confirmations: r.confirmations,
    blockMaturity: r.block_maturity,
    reward: r.reward,
    networkDifficulty: r.network_difficulty,
    finder: r.finder,
    foundAt: r.found_at,
  }));
}

/** 24 hours of hashrate in complete 10-minute buckets, for the whole pool or one account. */
async function getHistory(filter: { coinSymbol: string } | { accountId: string }): Promise<HashratePoint[]> {
  const byAccount = "accountId" in filter;
  const { rows } = await db().query(
    `SELECT bucket AS t, COALESCE(sum(s.difficulty), 0) * $1 / $2 AS v
     FROM generate_series(
              date_bin(make_interval(secs => $2), now() - interval '24 hours', TIMESTAMPTZ 'epoch'),
              date_bin(make_interval(secs => $2), now(), TIMESTAMPTZ 'epoch') - make_interval(secs => $2),
              make_interval(secs => $2)) AS bucket
     LEFT JOIN shares s
            ON s.created_at >= bucket AND s.created_at < bucket + make_interval(secs => $2)
           AND ${byAccount ? "s.account_id = $3" : "s.coin_id = (SELECT id FROM coins WHERE symbol = $3)"}
     GROUP BY bucket
     ORDER BY bucket`,
    [HASHES_PER_DIFF, CHART_BUCKET_SECS, byAccount ? filter.accountId : filter.coinSymbol],
  );
  return rows.map((r) => ({ t: (r.t as Date).toISOString(), v: r.v }));
}

export async function getPoolHistory(coinSymbol: string): Promise<HashratePoint[]> {
  await connection();
  return getHistory({ coinSymbol });
}

/** One entry per coin the address has mined; empty if the pool has never seen it. */
export async function getMiner(address: string): Promise<MinerAccount[]> {
  await connection();
  const accounts = await db().query(
    `SELECT a.id, c.id AS coin_id, c.symbol
     FROM accounts a JOIN coins c ON c.id = a.payout_coin_id
     WHERE a.payout_address = $1
     ORDER BY c.symbol`,
    [address],
  );

  return Promise.all(
    accounts.rows.map(async ({ id, coin_id, symbol }) => {
      const [totals, workers, payouts, credits, history] = await Promise.all([
        db().query(
          `SELECT
             (SELECT COALESCE(sum(amount), 0) FROM ledger_entries WHERE account_id = $1 AND coin_id = $2) AS balance,
             (SELECT COALESCE(sum(bc.amount), 0) FROM block_credits bc JOIN blocks b ON b.id = bc.block_id
               WHERE bc.account_id = $1 AND b.status = 'pending') AS immature,
             (SELECT COALESCE(sum(amount), 0) FROM payouts WHERE account_id = $1 AND status = 'sent') AS paid,
             (SELECT COALESCE(sum(difficulty), 0) * $3 / $4 FROM shares
               WHERE account_id = $1 AND created_at > now() - make_interval(secs => $4)) AS hashrate`,
          [id, coin_id, HASHES_PER_DIFF, LIVE_WINDOW_SECS],
        ),
        db().query(
          `SELECT w.name,
                  COALESCE(sum(s.difficulty) FILTER (WHERE s.created_at > now() - make_interval(secs => $3)), 0) * $2 / $3 AS hashrate,
                  max(s.created_at) AS last_share_at
           FROM workers w
           LEFT JOIN shares s ON s.worker_id = w.id AND s.created_at > now() - interval '24 hours'
           WHERE w.account_id = $1
           GROUP BY w.id
           ORDER BY w.name`,
          [id, HASHES_PER_DIFF, LIVE_WINDOW_SECS],
        ),
        db().query(
          `SELECT id, amount, status, txid, created_at FROM payouts WHERE account_id = $1 ORDER BY id DESC LIMIT 20`,
          [id],
        ),
        db().query(
          `SELECT b.height, b.status, bc.amount, b.found_at
           FROM block_credits bc JOIN blocks b ON b.id = bc.block_id
           WHERE bc.account_id = $1
           ORDER BY b.found_at DESC, b.id DESC
           LIMIT 20`,
          [id],
        ),
        getHistory({ accountId: id }),
      ]);
      const t = totals.rows[0];
      return {
        symbol,
        address,
        hashrate: t.hashrate,
        balance: t.balance,
        immature: t.immature,
        paid: t.paid,
        workers: workers.rows.map((r) => ({ name: r.name, hashrate: r.hashrate, lastShareAt: r.last_share_at })),
        payouts: payouts.rows.map((r) => ({
          id: r.id,
          amount: r.amount,
          status: r.status,
          txid: r.txid,
          createdAt: r.created_at,
        })),
        credits: credits.rows.map((r) => ({
          height: Number(r.height),
          status: r.status,
          amount: r.amount,
          foundAt: r.found_at,
        })),
        history,
      };
    }),
  );
}

/** Payout addresses are base58 or bech32; anything else cannot be a miner. */
export function isPlausibleAddress(address: string): boolean {
  return /^[A-Za-z0-9]{20,100}$/.test(address);
}
