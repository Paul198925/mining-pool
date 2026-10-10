import { meshEnabled, setWireguardKey, wireguardConfig } from "@/lib/mesh";
import { authenticateNode, badRequest, text, unauthorized } from "@/lib/node-auth";

/**
 * The calling server's WireGuard configuration with its current peers. Servers fetch
 * it every minute, which is how they learn about servers that joined or left.
 * Answers 204 when the pool runs without the private network.
 */
export async function GET(request: Request) {
  const server = await authenticateNode(request);
  if (!server) return unauthorized();
  if (!meshEnabled()) return new Response(null, { status: 204 });
  return text(await wireguardConfig(server));
}

/** A server hands in the public half of the key it made for itself. */
export async function POST(request: Request) {
  const server = await authenticateNode(request);
  if (!server) return unauthorized();
  const body = await request.json().catch(() => null);
  const publicKey = body?.public_key;
  // A WireGuard key is 32 bytes in base64.
  if (typeof publicKey !== "string" || !/^[A-Za-z0-9+/]{43}=$/.test(publicKey)) return badRequest("invalid WireGuard key");
  await setWireguardKey(server, publicKey);
  return Response.json({ ok: true });
}
