import { nodeCompose } from "@/lib/mesh";
import { authenticateNode, text, unauthorized } from "@/lib/node-auth";
import { publicUrl } from "@/lib/public-url";

/** The docker-compose file for the calling server, fetched by the installer. */
export async function GET(request: Request) {
  const server = await authenticateNode(request);
  if (!server) return unauthorized();
  // The installer reports this version back once the file is applied.
  return text(await nodeCompose(server, publicUrl(request.headers)), { "x-pool-config-version": server.configVersion });
}
