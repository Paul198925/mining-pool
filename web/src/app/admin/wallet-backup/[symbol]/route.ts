import { isAdmin } from "@/lib/admin-auth";
import { db } from "@/lib/db";

/**
 * Lets the admin download the newest copy of a pool wallet's backup, e.g. to restore
 * the wallet on a new server. The file is the encrypted wallet; opening it needs the
 * passphrase from pool-core's environment.
 */
export async function GET(_request: Request, { params }: RouteContext<"/admin/wallet-backup/[symbol]">) {
  if (!(await isAdmin())) {
    return new Response("Not found", { status: 404 });
  }
  const { symbol } = await params;
  const { rows } = await db().query(
    "SELECT data, received_at FROM wallet_backups WHERE symbol = $1 ORDER BY received_at DESC LIMIT 1",
    [symbol],
  );
  if (rows.length === 0) {
    return new Response("No backup of this wallet has arrived yet.", { status: 404 });
  }
  const day = (rows[0].received_at as Date).toISOString().slice(0, 10);
  return new Response(rows[0].data, {
    headers: {
      "content-type": "application/octet-stream",
      "content-disposition": `attachment; filename="wallet-${symbol.toLowerCase().replace(/[^a-z0-9]/g, "")}-${day}.dat"`,
      "cache-control": "no-store",
    },
  });
}
