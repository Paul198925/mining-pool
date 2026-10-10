import { coinSpec } from "./catalog";
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
  /** The internal exchange has a current price for it. */
  exchangeable: boolean;
  /** That price in US dollars; null without a current one. */
  priceUsd: number | null;
  blockMaturity: number | null;
  /** Base units (1e-8 coin) as a decimal string. */
  minPayout: string | null;
  poolFeePercent: number | null;
  soloFeePercent: number | null;
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
  solo: boolean;
  party: boolean;
  foundAt: Date;
};

export type HashratePoint = { t: string; v: number };

export type Worker = { name: string; hashrate: number; lastShareAt: Date | null };

export type Payout = {
  id: string;
  amount: string;
  /** Transaction fee taken out of `amount`; null until the payout is sent. */
  networkFee: string | null;
  status: string;
  txid: string | null;
  createdAt: Date;
};

export type Credit = { symbol: string; height: number; status: Block["status"]; amount: string; foundAt: Date };

export type MinerAccount = {
  /** The account's id in the database. */
  id: string;
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
  /** The current price of the account's coin in US dollars, if the pool has one. */
  priceUsd: number | null;
  /** What the account is mining right now, per coin, with that coin's network difficulty. */
  mining: { symbol: string; hashrate: number; networkDifficulty: number | null }[];
  /** Earned in other coins and not yet swapped into this account's coin. */
  waiting: { symbol: string; amount: string }[];
  exchanges: Exchange[];
};

/** A swap by the pool's internal exchange. Amounts are base units as decimal strings. */
export type Exchange = {
  id: string;
  from: string;
  to: string;
  fromAmount: string;
  toAmount: string;
  feeAmount: string;
  createdAt: Date;
};

export async function getCoins(): Promise<Coin[]> {
  await connection();
  const { rows } = await db().query(
    `SELECT c.symbol, c.name, c.algo, c.height, c.network_difficulty, c.network_hashrate, c.stratum_port,
            c.block_maturity, c.min_payout, c.pool_fee_percent, c.solo_fee_percent,
            (SELECT p.usd FROM usable_prices p WHERE p.coin_id = c.id) AS price_usd,
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
    exchangeable: r.price_usd !== null,
    priceUsd: r.price_usd,
    blockMaturity: r.block_maturity,
    minPayout: r.min_payout,
    poolFeePercent: r.pool_fee_percent,
    soloFeePercent: r.solo_fee_percent,
    hashrate: r.hashrate,
    miners: Number(r.miners),
    workers: Number(r.workers),
    blocksFound: Number(r.blocks_found),
    lastBlockAt: r.last_block_at,
  }));
}

export type StratumServer = { region: string; host: string };

/** Stratum servers that reported in recently; the limit matches the servers' own (instance.rs). */
export async function getStratumServers(): Promise<StratumServer[]> {
  await connection();
  const { rows } = await db().query(
    `SELECT DISTINCT region, host FROM stratum_instances
     WHERE last_seen_at > now() - interval '120 seconds'
     ORDER BY region, host`,
  );
  return rows;
}

/** The latest blocks, of all coins or only of those with the given algorithm. */
export async function getBlocks(limit: number, algo?: string): Promise<Block[]> {
  await connection();
  const { rows } = await db().query(
    `SELECT b.id, c.symbol, b.height, b.hash, b.status, b.confirmations, c.block_maturity, b.reward,
            b.network_difficulty, a.payout_address AS finder, b.solo,
            b.party_id IS NOT NULL AS party, b.found_at
     FROM blocks b JOIN coins c ON c.id = b.coin_id JOIN accounts a ON a.id = b.account_id
     WHERE $2::text IS NULL OR c.algo = $2
     ORDER BY b.found_at DESC, b.id DESC
     LIMIT $1`,
    [limit, algo ?? null],
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
    solo: r.solo,
    party: r.party,
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
    `SELECT a.id, c.id AS coin_id, c.symbol,
            (SELECT p.usd FROM usable_prices p WHERE p.coin_id = c.id) AS price_usd
     FROM accounts a JOIN coins c ON c.id = a.payout_coin_id
     WHERE a.payout_address = $1
     ORDER BY c.symbol`,
    [address],
  );

  return Promise.all(
    accounts.rows.map(async ({ id, coin_id, symbol, price_usd }) => {
      const [totals, workers, payouts, credits, history, waiting, exchanges, mining] = await Promise.all([
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
          `SELECT id, amount, network_fee, status, txid, created_at FROM payouts WHERE account_id = $1 ORDER BY id DESC LIMIT 20`,
          [id],
        ),
        db().query(
          `SELECT b.height, b.status, bc.amount, b.found_at, c.symbol
           FROM block_credits bc JOIN blocks b ON b.id = bc.block_id JOIN coins c ON c.id = b.coin_id
           WHERE bc.account_id = $1
           ORDER BY b.found_at DESC, b.id DESC
           LIMIT 20`,
          [id],
        ),
        getHistory({ accountId: id }),
        db().query(
          `SELECT c.symbol, sum(l.amount) AS amount
           FROM ledger_entries l JOIN coins c ON c.id = l.coin_id
           WHERE l.account_id = $1 AND l.coin_id <> $2
           GROUP BY c.symbol HAVING sum(l.amount) > 0
           ORDER BY c.symbol`,
          [id, coin_id],
        ),
        db().query(
          `SELECT e.id, f.symbol AS from_symbol, t.symbol AS to_symbol, e.from_amount, e.to_amount, e.fee_amount, e.created_at
           FROM exchanges e JOIN coins f ON f.id = e.from_coin_id JOIN coins t ON t.id = e.to_coin_id
           WHERE e.account_id = $1
           ORDER BY e.id DESC LIMIT 20`,
          [id],
        ),
        db().query(
          `SELECT c.symbol, c.network_difficulty, sum(s.difficulty) * $2 / $3 AS hashrate
           FROM shares s JOIN coins c ON c.id = s.coin_id
           WHERE s.account_id = $1 AND s.created_at > now() - make_interval(secs => $3)
           GROUP BY c.symbol, c.network_difficulty
           ORDER BY c.symbol`,
          [id, HASHES_PER_DIFF, LIVE_WINDOW_SECS],
        ),
      ]);
      const perCoin = mining.rows.map((r) => ({
        symbol: r.symbol as string,
        hashrate: r.hashrate as number,
        networkDifficulty: r.network_difficulty as number | null,
      }));
      // A merge-mined coin gets the same shares as the coin it rides along with: the
      // miner's hashrate is that of the coins they actually work on.
      const worked = perCoin.filter((coin) => !coinSpec(coin.symbol)?.auxOf);
      const t = totals.rows[0];
      return {
        id: String(id),
        symbol,
        address,
        hashrate: (worked.length > 0 ? worked : perCoin).reduce((sum, coin) => sum + coin.hashrate, 0),
        priceUsd: price_usd,
        mining: perCoin,
        balance: t.balance,
        immature: t.immature,
        paid: t.paid,
        workers: workers.rows.map((r) => ({ name: r.name, hashrate: r.hashrate, lastShareAt: r.last_share_at })),
        payouts: payouts.rows.map((r) => ({
          id: r.id,
          amount: r.amount,
          networkFee: r.network_fee,
          status: r.status,
          txid: r.txid,
          createdAt: r.created_at,
        })),
        credits: credits.rows.map((r) => ({
          symbol: r.symbol,
          height: Number(r.height),
          status: r.status,
          amount: r.amount,
          foundAt: r.found_at,
        })),
        history,
        waiting: waiting.rows.map((r) => ({ symbol: r.symbol, amount: r.amount })),
        exchanges: exchanges.rows.map((r) => ({
          id: r.id,
          from: r.from_symbol,
          to: r.to_symbol,
          fromAmount: r.from_amount,
          toAmount: r.to_amount,
          feeAmount: r.fee_amount,
          createdAt: r.created_at,
        })),
      };
    }),
  );
}

/** Payout addresses are base58 or bech32; anything else cannot be a miner. */
export function isPlausibleAddress(address: string): boolean {
  return /^[A-Za-z0-9]{20,100}$/.test(address);
}

/** Whether the internal exchange is on, and what it charges. */
export async function getExchangeSettings(): Promise<{ enabled: boolean; feePercent: number }> {
  await connection();
  const { rows } = await db().query(
    "SELECT name, value FROM pool_settings WHERE name IN ('exchange_enabled', 'exchange_fee_percent')",
  );
  const setting = (name: string) => rows.find((row) => row.name === name)?.value;
  return { enabled: setting("exchange_enabled") === "true", feePercent: Number(setting("exchange_fee_percent") ?? 0) };
}
