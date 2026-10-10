import { connection } from "next/server";

import { COINS } from "./catalog";
import { db } from "./db";
import type { HashratePoint } from "./queries";

// What the wallet page shows beyond the basics in `getMiner`: one account's earnings by
// period, what it is mining right now, its connections, and its history.

const HASHES_PER_DIFF = 4294967296;
const LIVE_WINDOW_SECS = 600;
const CHART_BUCKET_SECS = 600;
const LIST_LIMIT = 100;

/** Miners scale the difficulty they are told by this for some algorithms. */
const DIFF_MULTIPLIER: Record<string, number> = { scrypt: 65536, neoscrypt: 65536, "neoscrypt-xaya": 65536 };

export type MiningRow = {
  symbol: string;
  name: string;
  algo: string;
  /** Workers that sent a share for this coin in the last ten minutes. */
  workers: number;
  /** The account's hashrate in the pool (or its party) and solo. */
  sharedHashrate: number;
  soloHashrate: number;
  /** Everyone's hashrate in the pool on this coin, without solo miners. */
  poolHashrate: number;
  networkDifficulty: number | null;
  /** Reward of the coin's most recent block, base units; null before the first one. */
  lastReward: string | null;
  poolFeePercent: number | null;
};

export type ConnectionRow = {
  name: string;
  version: string | null;
  password: string | null;
  algo: string | null;
  /** The difficulty of the worker's latest share, as its miner displays it. */
  difficulty: number | null;
  hashrate: number;
  sharesPerMinute: number;
  lastShareAt: Date | null;
};

export type FoundBlock = {
  id: string;
  symbol: string;
  height: number;
  reward: string;
  networkDifficulty: number;
  foundAt: Date;
  type: "solo" | "party" | "shared";
  status: "pending" | "confirmed" | "orphaned";
};

export type Earning = {
  symbol: string;
  height: number;
  amount: string;
  /** The account's part of the block's reward, in percent. */
  percent: number;
  foundAt: Date;
  status: "pending" | "confirmed" | "orphaned";
};

export type MinerExtras = {
  /** Smallest balance that is paid out, base units. */
  minPayout: string | null;
  payoutsEnabled: boolean;
  /** Credited in the account's own coin, by block rewards and by the exchange. Base units. */
  earned: { day: string; twoWeeks: string; month: string; all: string };
  mining: MiningRow[];
  connections: ConnectionRow[];
  /** The balance in coins at the end of each ten minutes of the last 24 hours. */
  balanceHistory: HashratePoint[];
  found: FoundBlock[];
  earnings: Earning[];
};

export async function getMinerExtras(accountId: string, symbol: string): Promise<MinerExtras> {
  await connection();
  const auxSymbols = COINS.filter((coin) => coin.auxOf).map((coin) => coin.symbol);
  const [coin, earned, mining, balance, found, earnings] = await Promise.all([
    db().query("SELECT id, min_payout, payouts_enabled FROM coins WHERE symbol = $1", [symbol]),
    db().query(
      `SELECT COALESCE(sum(l.amount) FILTER (WHERE l.created_at > now() - interval '24 hours'), 0) AS day,
              COALESCE(sum(l.amount) FILTER (WHERE l.created_at > now() - interval '14 days'), 0) AS two_weeks,
              COALESCE(sum(l.amount) FILTER (WHERE l.created_at > now() - interval '30 days'), 0) AS month,
              COALESCE(sum(l.amount), 0) AS all_time
       FROM ledger_entries l JOIN coins c ON c.id = l.coin_id
       WHERE l.account_id = $1 AND c.symbol = $2 AND l.amount > 0 AND l.kind IN ('block_reward', 'exchange')`,
      [accountId, symbol],
    ),
    db().query(
      `SELECT c.symbol, c.name, c.algo, c.network_difficulty, c.pool_fee_percent,
              count(DISTINCT s.worker_id) AS workers,
              COALESCE(sum(s.difficulty) FILTER (WHERE NOT s.solo), 0) * $2 / $3 AS shared,
              COALESCE(sum(s.difficulty) FILTER (WHERE s.solo), 0) * $2 / $3 AS solo,
              COALESCE((SELECT sum(p.difficulty) FROM shares p
                        WHERE p.coin_id = c.id AND NOT p.solo
                          AND p.created_at > now() - make_interval(secs => $3)), 0) * $2 / $3 AS pool,
              (SELECT b.reward FROM blocks b WHERE b.coin_id = c.id ORDER BY b.id DESC LIMIT 1) AS last_reward
       FROM shares s JOIN coins c ON c.id = s.coin_id
       WHERE s.account_id = $1 AND s.created_at > now() - make_interval(secs => $3)
       GROUP BY c.id
       ORDER BY c.symbol`,
      [accountId, HASHES_PER_DIFF, LIVE_WINDOW_SECS],
    ),
    db().query(
      `WITH own AS (
           SELECT l.amount, l.created_at FROM ledger_entries l JOIN coins c ON c.id = l.coin_id
           WHERE l.account_id = $1 AND c.symbol = $2
       )
       SELECT bucket AS t,
              COALESCE((SELECT sum(amount) FROM own WHERE created_at < bucket + make_interval(secs => $3)), 0) AS v
       FROM generate_series(
                date_bin(make_interval(secs => $3), now() - interval '24 hours', TIMESTAMPTZ 'epoch'),
                date_bin(make_interval(secs => $3), now(), TIMESTAMPTZ 'epoch'),
                make_interval(secs => $3)) AS bucket
       ORDER BY bucket`,
      [accountId, symbol, CHART_BUCKET_SECS],
    ),
    db().query(
      `SELECT b.id, c.symbol, b.height, b.reward, b.network_difficulty, b.found_at, b.solo, b.party_id, b.status
       FROM blocks b JOIN coins c ON c.id = b.coin_id
       WHERE b.account_id = $1
       ORDER BY b.id DESC LIMIT $2`,
      [accountId, LIST_LIMIT],
    ),
    db().query(
      `SELECT c.symbol, b.height, b.status, bc.amount, b.reward, b.found_at
       FROM block_credits bc JOIN blocks b ON b.id = bc.block_id JOIN coins c ON c.id = b.coin_id
       WHERE bc.account_id = $1
       ORDER BY b.found_at DESC, b.id DESC
       LIMIT $2`,
      [accountId, LIST_LIMIT],
    ),
  ]);

  // A merge-mined coin gets the same shares as the coin it rides along with. Where the
  // account works on such a pair, its workers are counted on the coin they work on.
  const minedSymbols = mining.rows.map((r) => r.symbol as string);
  const doubled = minedSymbols.some((mined) => !auxSymbols.includes(mined))
    ? minedSymbols.filter((mined) => auxSymbols.includes(mined))
    : [];
  const connections = await db().query(
    `SELECT w.name, w.version, w.password, w.algo,
            COALESCE(sum(s.difficulty), 0) * $2 / $3 AS hashrate,
            count(s.id)::float8 * 60 / $3 AS shares_per_minute,
            (SELECT max(x.created_at) FROM shares x WHERE x.worker_id = w.id) AS last_share_at,
            (SELECT x.difficulty FROM shares x WHERE x.worker_id = w.id ORDER BY x.created_at DESC LIMIT 1) AS last_difficulty
     FROM workers w
     LEFT JOIN shares s ON s.worker_id = w.id AND s.created_at > now() - make_interval(secs => $3)
          AND s.coin_id NOT IN (SELECT id FROM coins WHERE symbol = ANY($4))
     WHERE w.account_id = $1
     GROUP BY w.id
     ORDER BY max(s.created_at) DESC NULLS LAST, w.name
     LIMIT $5`,
    [accountId, HASHES_PER_DIFF, LIVE_WINDOW_SECS, doubled, LIST_LIMIT],
  );

  const settings = coin.rows[0];
  const periods = earned.rows[0];
  return {
    minPayout: settings?.min_payout ?? null,
    payoutsEnabled: settings?.payouts_enabled ?? true,
    earned: { day: periods.day, twoWeeks: periods.two_weeks, month: periods.month, all: periods.all_time },
    mining: mining.rows.map((r) => ({
      symbol: r.symbol,
      name: r.name,
      algo: r.algo,
      workers: Number(r.workers),
      sharedHashrate: r.shared,
      soloHashrate: r.solo,
      poolHashrate: r.pool,
      networkDifficulty: r.network_difficulty,
      lastReward: r.last_reward,
      poolFeePercent: r.pool_fee_percent,
    })),
    connections: connections.rows.map((r) => ({
      name: r.name,
      version: r.version,
      password: r.password,
      algo: r.algo,
      difficulty: r.last_difficulty === null ? null : r.last_difficulty * (DIFF_MULTIPLIER[r.algo ?? ""] ?? 1),
      hashrate: r.hashrate,
      sharesPerMinute: r.shares_per_minute,
      lastShareAt: r.last_share_at,
    })),
    balanceHistory: balance.rows.map((r) => ({ t: (r.t as Date).toISOString(), v: Number(r.v) / 1e8 })),
    found: found.rows.map((r) => ({
      id: r.id,
      symbol: r.symbol,
      height: Number(r.height),
      reward: r.reward,
      networkDifficulty: r.network_difficulty,
      foundAt: r.found_at,
      type: r.solo ? "solo" : r.party_id !== null ? "party" : "shared",
      status: r.status,
    })),
    earnings: earnings.rows.map((r) => ({
      symbol: r.symbol,
      height: Number(r.height),
      amount: r.amount,
      percent: Number(r.reward) > 0 ? (Number(r.amount) / Number(r.reward)) * 100 : 0,
      foundAt: r.found_at,
      status: r.status,
    })),
  };
}
