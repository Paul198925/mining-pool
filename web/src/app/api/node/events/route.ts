import type { PoolClient } from "pg";

import { db } from "@/lib/db";
import { authenticateNode, badRequest, mayServe, unauthorized } from "@/lib/node-auth";

const MAX_EVENTS = 20_000;

type Share = { account_id: number; worker_id: number; solo: boolean; party_id: number | null; difficulty: number; at: number };
type Block = Share & { height: number; hash: string; reward: number; network_difficulty: number };

const isId = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) > 0;
const isPositive = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value) && value > 0;

function validShare(event: Record<string, unknown>): boolean {
  return (
    isId(event.account_id) &&
    isId(event.worker_id) &&
    typeof event.solo === "boolean" &&
    (event.party_id === null || isId(event.party_id)) &&
    isPositive(event.at)
  );
}

// A server's clock may be a little off, but it cannot move its work far into the past
// (into an older PPLNS window) or into the future.
const CLAMPED_TIME = "LEAST(now(), GREATEST(to_timestamp($TIME), now() - interval '10 minutes'))";

async function insertShares(client: PoolClient, coinId: number, shares: Share[]) {
  if (shares.length === 0) return;
  await client.query(
    `INSERT INTO shares (coin_id, account_id, worker_id, difficulty, created_at, solo, party_id)
     SELECT $1, s.account_id, s.worker_id, s.difficulty, ${CLAMPED_TIME.replace("$TIME", "s.at")}, s.solo, s.party_id
     FROM UNNEST($2::int8[], $3::int8[], $4::float8[], $5::float8[], $6::bool[], $7::int8[])
          AS s (account_id, worker_id, difficulty, at, solo, party_id)`,
    [
      coinId,
      shares.map((s) => s.account_id),
      shares.map((s) => s.worker_id),
      shares.map((s) => s.difficulty),
      shares.map((s) => s.at),
      shares.map((s) => s.solo),
      shares.map((s) => s.party_id),
    ],
  );
}

/**
 * Takes a batch of shares and found blocks from a server, in the order they happened.
 * A batch is stored completely or not at all, and a batch id is accepted only once,
 * so a server can safely repeat a delivery it got no answer for.
 */
export async function POST(request: Request) {
  const node = await authenticateNode(request);
  if (!node) return unauthorized();
  const body = await request.json().catch(() => null);
  const { batch, symbol, events } = body ?? {};
  if (typeof batch !== "string" || batch.length < 8 || batch.length > 100) return badRequest("invalid batch id");
  if (!(await mayServe(node, symbol))) return badRequest("coin not assigned to this server");
  if (!Array.isArray(events) || events.length > MAX_EVENTS) return badRequest("invalid events");
  for (const event of events) {
    const ok =
      event?.type === "share"
        ? validShare(event) && isPositive(event.difficulty)
        : event?.type === "block" &&
          validShare(event) &&
          Number.isSafeInteger(event.height) &&
          event.height >= 0 &&
          typeof event.hash === "string" &&
          /^[0-9a-f]{64}$/.test(event.hash) &&
          Number.isSafeInteger(event.reward) &&
          event.reward >= 0 &&
          isPositive(event.network_difficulty);
    if (!ok) return badRequest("invalid event");
  }

  const client = await db().connect();
  try {
    await client.query("BEGIN");
    const first = await client.query(
      "INSERT INTO node_batches (node_id, batch_id) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING batch_id",
      [node.id, batch],
    );
    if (first.rows.length === 0) {
      await client.query("ROLLBACK");
      return Response.json({ ok: true, duplicate: true });
    }
    const coin = await client.query("SELECT id FROM coins WHERE symbol = $1", [symbol]);
    if (coin.rows.length === 0) throw new Error(`coin ${symbol} is not registered`);
    const coinId = coin.rows[0].id;

    // Shares before a block must be stored before it: they are part of its PPLNS window.
    let shares: Share[] = [];
    for (const event of events as (Share & { type: string })[]) {
      if (event.type === "share") {
        shares.push(event);
        continue;
      }
      await insertShares(client, coinId, shares);
      shares = [];
      const block = event as unknown as Block;
      await client.query(
        `INSERT INTO blocks
             (coin_id, height, hash, account_id, worker_id, reward, network_difficulty, found_at, solo, party_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, ${CLAMPED_TIME.replace("$TIME", "$8")}, $9, $10)
         ON CONFLICT (coin_id, hash) DO NOTHING`,
        [coinId, block.height, block.hash, block.account_id, block.worker_id, block.reward, block.network_difficulty, block.at, block.solo, block.party_id],
      );
    }
    await insertShares(client, coinId, shares);
    await client.query("UPDATE stratum_instances SET last_seen_at = now() WHERE id = $1", [node.id]);
    await client.query("COMMIT");
    return Response.json({ ok: true });
  } catch (error) {
    await client.query("ROLLBACK");
    console.error("node events failed", error);
    return Response.json({ error: "could not store the batch" }, { status: 500 });
  } finally {
    client.release();
  }
}
