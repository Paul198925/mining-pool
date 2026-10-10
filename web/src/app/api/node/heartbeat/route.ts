import { db } from "@/lib/db";
import { authenticateNode, unauthorized } from "@/lib/node-auth";

/** Keeps a server listed as running; the website drops it after two minutes of silence. */
export async function POST(request: Request) {
  const node = await authenticateNode(request);
  if (!node) return unauthorized();
  await db().query("UPDATE stratum_instances SET last_seen_at = now() WHERE id = $1", [node.id]);
  return Response.json({ ok: true });
}
