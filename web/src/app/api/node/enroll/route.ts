import { db } from "@/lib/db";
import { badRequest, newSecret, sha256 } from "@/lib/node-auth";

/** Trades a one-time enrollment code for the server's permanent node token. */
export async function POST(request: Request) {
  const body = await request.json().catch(() => null);
  const code = typeof body?.code === "string" ? body.code : "";
  if (code.length < 32) return badRequest("missing code");

  const token = newSecret();
  // One statement, so a code can only ever be redeemed once.
  const { rows } = await db().query(
    `UPDATE stratum_instances
     SET token_hash = $2, enroll_code_hash = NULL, enroll_expires_at = NULL, enrolled_at = now()
     WHERE enroll_code_hash = $1 AND enroll_expires_at > now()
     RETURNING id`,
    [sha256(code), sha256(token)],
  );
  if (rows.length === 0) {
    return Response.json({ error: "this code is unknown, already used or expired" }, { status: 403 });
  }
  return Response.json({ token });
}
