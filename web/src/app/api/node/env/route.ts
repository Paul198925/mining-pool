import { getAlgos, nodeRpcPassword } from "@/lib/mesh";
import { authenticateNode, text, unauthorized } from "@/lib/node-auth";

/** Settings the installer writes next to the compose file, as KEY=value lines. */
export async function GET(request: Request) {
  const server = await authenticateNode(request);
  if (!server) return unauthorized();
  // The ports miners connect to, for the server's firewall.
  const ports = server.stratum ? (await getAlgos()).map((algo) => algo.port).join(" ") : "";
  return text(`RPC_PASSWORD=${await nodeRpcPassword()}\nSTRATUM_PORTS="${ports}"\n`);
}
