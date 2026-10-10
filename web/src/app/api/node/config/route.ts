import { db } from "@/lib/db";
import { nodeConfig } from "@/lib/mesh";
import { authenticateNode, unauthorized } from "@/lib/node-auth";

/**
 * The stratum service on a server asks for its configuration here when it starts, and
 * again every minute to notice changes.
 */
export async function GET(request: Request) {
  const server = await authenticateNode(request);
  if (!server) return unauthorized();
  const config = await nodeConfig(server);

  // Make the coins this server offers known to the website, with the port miners use.
  const ports = new Map(config.stratum.map((listener) => [listener.algo, Number(listener.listen.split(":")[1])]));
  for (const coin of config.coins) {
    await db().query(
      `INSERT INTO coins (symbol, name, algo, stratum_port) VALUES ($1, $2, $3, $4)
       ON CONFLICT (symbol) DO UPDATE SET name = EXCLUDED.name, algo = EXCLUDED.algo, stratum_port = EXCLUDED.stratum_port`,
      [coin.symbol, coin.name, coin.algo, ports.get(coin.algo) ?? null],
    );
  }
  return Response.json(config);
}
