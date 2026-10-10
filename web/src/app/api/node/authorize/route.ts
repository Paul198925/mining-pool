import { db } from "@/lib/db";
import { authenticateNode, badRequest, mayServe, unauthorized } from "@/lib/node-auth";

/**
 * Registers a miner login. The server has already checked the address with its own
 * coin daemon; this only maps address, worker and party to the pool's ids.
 *
 * For a merge-mined coin the account can be one of another coin (`payout_symbol`):
 * the miner then has no address on the mined coin's chain and only collects a balance.
 */
export async function POST(request: Request) {
  const node = await authenticateNode(request);
  if (!node) return unauthorized();
  const body = await request.json().catch(() => null);
  const { symbol, address, worker, solo, party_key_hash: partyKeyHash } = body ?? {};
  const payoutSymbol = body?.payout_symbol ?? symbol;
  if (
    !(await mayServe(node, symbol)) ||
    !(await mayServe(node, payoutSymbol)) ||
    typeof address !== "string" ||
    !/^[A-Za-z0-9]{20,100}$/.test(address) ||
    typeof worker !== "string" ||
    worker.length > 64 ||
    (partyKeyHash != null && !/^[0-9a-f]{64}$/.test(partyKeyHash))
  ) {
    return badRequest("invalid login");
  }

  const coins = await db().query("SELECT symbol, id FROM coins WHERE symbol = ANY($1)", [[symbol, payoutSymbol]]);
  const idOf = (wanted: string) => coins.rows.find((row) => row.symbol === wanted)?.id;
  const coinId = idOf(symbol);
  const payoutCoinId = idOf(payoutSymbol);
  if (coinId === undefined || payoutCoinId === undefined) return badRequest("unknown coin");
  const account = await db().query(
    `INSERT INTO accounts (payout_coin_id, payout_address) VALUES ($1, $2)
     ON CONFLICT (payout_coin_id, payout_address) DO UPDATE SET payout_address = EXCLUDED.payout_address
     RETURNING id`,
    [payoutCoinId, address],
  );
  const workerRow = await db().query(
    `INSERT INTO workers (account_id, name, last_seen_at) VALUES ($1, $2, now())
     ON CONFLICT (account_id, name) DO UPDATE SET last_seen_at = now()
     RETURNING id`,
    [account.rows[0].id, worker],
  );
  let partyId: string | null = null;
  if (partyKeyHash) {
    const party = await db().query(
      `INSERT INTO parties (coin_id, key_hash) VALUES ($1, $2)
       ON CONFLICT (coin_id, key_hash) DO UPDATE SET key_hash = EXCLUDED.key_hash
       RETURNING id`,
      [coinId, partyKeyHash],
    );
    partyId = party.rows[0].id;
  }
  // int8 columns arrive as strings; the ids are far below 2^53.
  return Response.json({
    account_id: Number(account.rows[0].id),
    worker_id: Number(workerRow.rows[0].id),
    solo: solo === true,
    party_id: partyId === null ? null : Number(partyId),
  });
}
