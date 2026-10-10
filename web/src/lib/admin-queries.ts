import { requireAdmin } from "./admin-auth";
import { db } from "./db";

// Every function here checks the session itself, so a page cannot leak data by forgetting to.

const HASHES_PER_DIFF = 4294967296;
const LIVE_WINDOW_SECS = 600;

export type CoinFinance = {
  symbol: string;
  name: string;
  /** All amounts are base units (1e-8 coin) as decimal strings. */
  mined: string;
  poolFee: string;
  paidOut: string;
  networkFee: string;
  minerBalances: string;
  payoutsOpen: string;
  immature: string;
  ledgerSum: string;
  walletBalance: string | null;
  walletCheckedAt: Date | null;
  payoutsSending: number;
  oldestSendingAt: Date | null;
  blocksPending: number;
  blocksConfirmed: number;
  blocksOrphaned: number;
  blocks24h: number;
};

export async function getFinances(): Promise<CoinFinance[]> {
  await requireAdmin();
  const book = (name: string) =>
    `COALESCE((SELECT sum(amount) FROM ledger_entries WHERE coin_id = c.id AND system_account = '${name}'), 0)`;
  const blocks = (where: string) => `(SELECT count(*) FROM blocks WHERE coin_id = c.id AND ${where})`;
  const { rows } = await db().query(
    `SELECT c.symbol, c.name, c.wallet_balance, c.wallet_checked_at,
            -${book("mined")} AS mined,
            ${book("pool_fee")} AS pool_fee,
            ${book("paid_out")} AS paid_out,
            ${book("network_fee")} AS network_fee,
            COALESCE((SELECT sum(amount) FROM ledger_entries WHERE coin_id = c.id AND account_id IS NOT NULL), 0) AS miner_balances,
            COALESCE((SELECT sum(amount) FROM ledger_entries WHERE coin_id = c.id), 0) AS ledger_sum,
            COALESCE((SELECT sum(amount) FROM payouts WHERE coin_id = c.id AND status <> 'sent'), 0) AS payouts_open,
            (SELECT count(*) FROM payouts WHERE coin_id = c.id AND status = 'sending') AS payouts_sending,
            (SELECT min(created_at) FROM payouts WHERE coin_id = c.id AND status = 'sending') AS oldest_sending_at,
            COALESCE((SELECT sum(bc.amount) FROM block_credits bc JOIN blocks b ON b.id = bc.block_id
                      WHERE b.coin_id = c.id AND b.status = 'pending'), 0) AS immature,
            ${blocks("status = 'pending'")} AS blocks_pending,
            ${blocks("status = 'confirmed'")} AS blocks_confirmed,
            ${blocks("status = 'orphaned'")} AS blocks_orphaned,
            ${blocks("found_at > now() - interval '24 hours'")} AS blocks_24h
     FROM coins c
     WHERE c.enabled OR EXISTS (SELECT 1 FROM ledger_entries l WHERE l.coin_id = c.id)
     ORDER BY c.symbol`,
  );
  return rows.map((r) => ({
    symbol: r.symbol,
    name: r.name,
    mined: r.mined,
    poolFee: r.pool_fee,
    paidOut: r.paid_out,
    networkFee: r.network_fee,
    minerBalances: r.miner_balances,
    payoutsOpen: r.payouts_open,
    immature: r.immature,
    ledgerSum: r.ledger_sum,
    walletBalance: r.wallet_balance,
    walletCheckedAt: r.wallet_checked_at,
    payoutsSending: Number(r.payouts_sending),
    oldestSendingAt: r.oldest_sending_at,
    blocksPending: Number(r.blocks_pending),
    blocksConfirmed: Number(r.blocks_confirmed),
    blocksOrphaned: Number(r.blocks_orphaned),
    blocks24h: Number(r.blocks_24h),
  }));
}

export type AdminPayout = {
  id: string;
  symbol: string;
  address: string;
  amount: string;
  networkFee: string | null;
  status: "pending" | "sending" | "sent";
  batchId: string | null;
  txid: string | null;
  createdAt: Date;
  sentAt: Date | null;
};

export async function getPayouts(limit: number): Promise<AdminPayout[]> {
  await requireAdmin();
  const { rows } = await db().query(
    `SELECT p.id, c.symbol, p.address, p.amount, p.network_fee, p.status, p.batch_id, p.txid, p.created_at, p.sent_at
     FROM payouts p JOIN coins c ON c.id = p.coin_id
     ORDER BY (p.status <> 'sent') DESC, p.id DESC
     LIMIT $1`,
    [limit],
  );
  return rows.map((r) => ({
    id: r.id,
    symbol: r.symbol,
    address: r.address,
    amount: r.amount,
    networkFee: r.network_fee,
    status: r.status,
    batchId: r.batch_id,
    txid: r.txid,
    createdAt: r.created_at,
    sentAt: r.sent_at,
  }));
}

export type AdminBlock = {
  id: string;
  symbol: string;
  height: number;
  hash: string;
  status: "pending" | "confirmed" | "orphaned";
  confirmations: number;
  reward: string;
  credited: boolean;
  /** Sum of what miners get; the rest of the reward is the pool fee. */
  toMiners: string;
  recipients: number;
  finder: string;
  mode: "pool" | "solo" | "party";
  foundAt: Date;
};

const BLOCK_COLUMNS = `b.id, c.symbol, b.height, b.hash, b.status, b.confirmations, b.reward, b.credited, b.found_at,
       a.payout_address AS finder,
       CASE WHEN b.solo THEN 'solo' WHEN b.party_id IS NOT NULL THEN 'party' ELSE 'pool' END AS mode,
       COALESCE((SELECT sum(amount) FROM block_credits WHERE block_id = b.id), 0) AS to_miners,
       (SELECT count(*) FROM block_credits WHERE block_id = b.id) AS recipients`;

function toBlock(r: Record<string, never>): AdminBlock {
  const row = r as Record<string, unknown>;
  return {
    id: row.id as string,
    symbol: row.symbol as string,
    height: Number(row.height),
    hash: row.hash as string,
    status: row.status as AdminBlock["status"],
    confirmations: row.confirmations as number,
    reward: row.reward as string,
    credited: row.credited as boolean,
    toMiners: row.to_miners as string,
    recipients: Number(row.recipients),
    finder: row.finder as string,
    mode: row.mode as AdminBlock["mode"],
    foundAt: row.found_at as Date,
  };
}

export async function getAdminBlocks(limit: number): Promise<AdminBlock[]> {
  await requireAdmin();
  const { rows } = await db().query(
    `SELECT ${BLOCK_COLUMNS}
     FROM blocks b JOIN coins c ON c.id = b.coin_id JOIN accounts a ON a.id = b.account_id
     ORDER BY b.found_at DESC, b.id DESC
     LIMIT $1`,
    [limit],
  );
  return rows.map(toBlock);
}

export type BlockCredit = { address: string; amount: string };

export async function getAdminBlock(id: string): Promise<{ block: AdminBlock; credits: BlockCredit[] } | null> {
  await requireAdmin();
  if (!/^\d{1,18}$/.test(id)) return null;
  const block = await db().query(
    `SELECT ${BLOCK_COLUMNS}
     FROM blocks b JOIN coins c ON c.id = b.coin_id JOIN accounts a ON a.id = b.account_id
     WHERE b.id = $1`,
    [id],
  );
  if (block.rows.length === 0) return null;
  const credits = await db().query(
    `SELECT a.payout_address AS address, bc.amount
     FROM block_credits bc JOIN accounts a ON a.id = bc.account_id
     WHERE bc.block_id = $1
     ORDER BY bc.amount DESC`,
    [id],
  );
  return { block: toBlock(block.rows[0]), credits: credits.rows };
}

export type AdminMiner = {
  symbol: string;
  address: string;
  workers: number;
  hashrate: number;
  balance: string;
  paid: string;
  lastShareAt: Date | null;
};

export async function getAdminMiners(limit: number): Promise<AdminMiner[]> {
  await requireAdmin();
  const { rows } = await db().query(
    `SELECT c.symbol, a.payout_address AS address,
            (SELECT count(*) FROM workers w WHERE w.account_id = a.id) AS workers,
            COALESCE((SELECT sum(difficulty) FROM shares s
                      WHERE s.account_id = a.id AND s.created_at > now() - make_interval(secs => $2)), 0) * $1 / $2 AS hashrate,
            COALESCE((SELECT sum(amount) FROM ledger_entries l WHERE l.account_id = a.id), 0) AS balance,
            COALESCE((SELECT sum(amount) FROM payouts p WHERE p.account_id = a.id AND p.status = 'sent'), 0) AS paid,
            (SELECT max(created_at) FROM shares s WHERE s.account_id = a.id) AS last_share_at
     FROM accounts a JOIN coins c ON c.id = a.payout_coin_id
     ORDER BY last_share_at DESC NULLS LAST
     LIMIT $3`,
    [HASHES_PER_DIFF, LIVE_WINDOW_SECS, limit],
  );
  return rows.map((r) => ({
    symbol: r.symbol,
    address: r.address,
    workers: Number(r.workers),
    hashrate: r.hashrate,
    balance: r.balance,
    paid: r.paid,
    lastShareAt: r.last_share_at,
  }));
}

export type StratumInstance = {
  id: number;
  region: string;
  host: string;
  coins: string[];
  lastSeenAt: Date | null;
  live: boolean;
  /** "waiting": added, install command not run yet. "expired": its one-time code ran out unused. */
  enrollment: "enrolled" | "waiting" | "expired" | "manual";
};

export type NodeStatus = {
  symbol: string;
  algo: string;
  stratumPort: number | null;
  height: number | null;
  /** Block headers the node knows; ahead of `height` while it is still downloading blocks. */
  headers: number | null;
  peers: number | null;
  /** 0 to 1, the node's own estimate of how much of the chain it has verified. */
  syncProgress: number | null;
  /** When pool-core last reached the coin's daemon. */
  statsAt: Date | null;
  reachable: boolean;
  miners: number;
  hashrate: number;
};

export async function getServers(): Promise<{ instances: StratumInstance[]; nodes: NodeStatus[] }> {
  await requireAdmin();
  const [instances, nodes] = await Promise.all([
    db().query(
      `SELECT id, region, host, coins, last_seen_at,
              COALESCE(last_seen_at > now() - interval '120 seconds', FALSE) AS live,
              CASE WHEN token_hash IS NOT NULL THEN 'enrolled'
                   WHEN enroll_code_hash IS NULL THEN 'manual'
                   WHEN enroll_expires_at > now() THEN 'waiting'
                   ELSE 'expired' END AS enrollment
       FROM stratum_instances ORDER BY id`,
    ),
    db().query(
      `SELECT c.symbol, c.algo, c.stratum_port, c.height, c.headers, c.peers, c.sync_progress, c.stats_at,
              COALESCE(c.stats_at > now() - interval '120 seconds', FALSE) AS reachable,
              (SELECT count(DISTINCT account_id) FROM shares s
               WHERE s.coin_id = c.id AND s.created_at > now() - make_interval(secs => $2)) AS miners,
              COALESCE((SELECT sum(difficulty) FROM shares s
                        WHERE s.coin_id = c.id AND s.created_at > now() - make_interval(secs => $2)), 0) * $1 / $2 AS hashrate
       FROM coins c WHERE c.enabled ORDER BY c.symbol`,
      [HASHES_PER_DIFF, LIVE_WINDOW_SECS],
    ),
  ]);
  return {
    instances: instances.rows.map((r) => ({
      id: r.id,
      region: r.region,
      host: r.host,
      coins: r.coins,
      lastSeenAt: r.last_seen_at,
      live: r.live,
      enrollment: r.enrollment,
    })),
    nodes: nodes.rows.map((r) => ({
      symbol: r.symbol,
      algo: r.algo,
      stratumPort: r.stratum_port,
      height: r.height === null ? null : Number(r.height),
      headers: r.headers === null ? null : Number(r.headers),
      peers: r.peers,
      syncProgress: r.sync_progress,
      statsAt: r.stats_at,
      reachable: r.reachable,
      miners: Number(r.miners),
      hashrate: r.hashrate,
    })),
  };
}
