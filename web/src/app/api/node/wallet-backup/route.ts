import { createHash } from "node:crypto";

import { db } from "@/lib/db";
import { authenticateNode, badRequest, unauthorized } from "@/lib/node-auth";

const MAX_BYTES = 32 * 1024 * 1024;
/** Copies kept per coin; pool-core has the wallet write one a day. */
const KEEP = 7;

/**
 * Takes a copy of a pool wallet's backup file from the server that holds the wallet.
 * The file is an encrypted wallet: its passphrase is not in the database. Keeping it
 * here means the database backup carries the wallets off the server as well.
 */
export async function POST(request: Request) {
  const server = await authenticateNode(request);
  if (!server) return unauthorized();
  const symbol = request.headers.get("x-pool-symbol") ?? "";
  // Only the server that holds a coin's wallet may hand in its backup.
  if (!server.wallets.includes(symbol)) return badRequest("this server does not hold that wallet");
  const declared = Number(request.headers.get("content-length") ?? 0);
  if (declared > MAX_BYTES) return badRequest("backup too large");
  const data = Buffer.from(await request.arrayBuffer());
  if (data.length === 0 || data.length > MAX_BYTES) return badRequest("backup empty or too large");

  const sha256 = createHash("sha256").update(data).digest("hex");
  await db().query(
    `INSERT INTO wallet_backups (symbol, server_id, sha256, size, data) VALUES ($1, $2, $3, $4, $5)
     ON CONFLICT (symbol, sha256) DO UPDATE SET received_at = now()`,
    [symbol, server.id, sha256, data.length, data],
  );
  await db().query(
    `DELETE FROM wallet_backups WHERE symbol = $1 AND id NOT IN (
       SELECT id FROM wallet_backups WHERE symbol = $1 ORDER BY received_at DESC LIMIT $2)`,
    [symbol, KEEP],
  );
  return Response.json({ ok: true });
}
