import { ALGOS, coinSpec, nodeConfig, poolAddresses } from "@/lib/catalog";
import { db } from "@/lib/db";
import { authenticateNode, unauthorized } from "@/lib/node-auth";

/** The stratum service on a server asks for its configuration here when it starts. */
export async function GET(request: Request) {
  const node = await authenticateNode(request);
  if (!node) return unauthorized();

  // Make the server's coins known to the website, with the port miners use.
  for (const symbol of node.coins) {
    const coin = coinSpec(symbol);
    if (!coin) continue;
    await db().query(
      `INSERT INTO coins (symbol, name, algo, stratum_port) VALUES ($1, $2, $3, $4)
       ON CONFLICT (symbol) DO UPDATE SET name = EXCLUDED.name, algo = EXCLUDED.algo, stratum_port = EXCLUDED.stratum_port`,
      [coin.symbol, coin.name, coin.algo, ALGOS[coin.algo].port],
    );
  }
  return Response.json(nodeConfig(node, await poolAddresses()));
}
