import { createHash, randomBytes } from "node:crypto";

import { db } from "./db";

export type Node = { id: number; region: string; host: string; coins: string[] };

export function sha256(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

export function newSecret(): string {
  return randomBytes(32).toString("hex");
}

/** The server a request comes from, by its bearer token; null if the token is unknown. */
export async function authenticateNode(request: Request): Promise<Node | null> {
  const header = request.headers.get("authorization") ?? "";
  const token = header.startsWith("Bearer ") ? header.slice(7).trim() : "";
  if (token.length < 32) return null;
  const { rows } = await db().query(
    "SELECT id, region, host, coins FROM stratum_instances WHERE token_hash = $1",
    [sha256(token)],
  );
  return rows[0] ?? null;
}

export function unauthorized(): Response {
  return Response.json({ error: "unknown node token" }, { status: 401 });
}

export function badRequest(message: string): Response {
  return Response.json({ error: message }, { status: 400 });
}
