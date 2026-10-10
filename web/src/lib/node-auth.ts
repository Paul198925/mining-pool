import { createHash, randomBytes } from "node:crypto";

import { type Server, serverByToken, servedSymbols } from "./mesh";

export function sha256(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

export function newSecret(): string {
  return randomBytes(32).toString("hex");
}

/** The server a request comes from, by its bearer token; null if the token is unknown. */
export async function authenticateNode(request: Request): Promise<Server | null> {
  const header = request.headers.get("authorization") ?? "";
  const token = header.startsWith("Bearer ") ? header.slice(7).trim() : "";
  if (token.length < 32) return null;
  return serverByToken(sha256(token));
}

const SERVED_TTL_MS = 30_000;
const served = new Map<number, { at: number; symbols: string[] }>();

/** Whether this server's stratum service may hand in logins and shares for the coin. Cached briefly: shares arrive every second. */
export async function mayServe(server: Server, symbol: unknown): Promise<boolean> {
  if (typeof symbol !== "string") return false;
  let entry = served.get(server.id);
  if (!entry || Date.now() - entry.at > SERVED_TTL_MS || !entry.symbols.includes(symbol)) {
    entry = { at: Date.now(), symbols: await servedSymbols(server) };
    served.set(server.id, entry);
  }
  return entry.symbols.includes(symbol);
}

export function unauthorized(): Response {
  return Response.json({ error: "unknown node token" }, { status: 401 });
}

export function badRequest(message: string): Response {
  return Response.json({ error: message }, { status: 400 });
}

export function text(body: string, headers: Record<string, string> = {}): Response {
  return new Response(body, { headers: { "content-type": "text/plain; charset=utf-8", ...headers } });
}
