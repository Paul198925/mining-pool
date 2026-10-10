import { db } from "@/lib/db";
import { authenticateNode, unauthorized } from "@/lib/node-auth";

/**
 * Keeps a server listed as running; the website drops it after two minutes of silence.
 * After applying its setup, a server also reports which version of it that was.
 */
export async function POST(request: Request) {
  const server = await authenticateNode(request);
  if (!server) return unauthorized();
  const body = await request.json().catch(() => null);
  const applied = /^\d{1,18}$/.test(String(body?.applied_version ?? "")) ? String(body.applied_version) : null;
  await db().query(
    "UPDATE stratum_instances SET last_seen_at = now(), applied_version = COALESCE($2::bigint, applied_version) WHERE id = $1",
    [server.id, applied],
  );
  return Response.json({ ok: true });
}
