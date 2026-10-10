import { nodeCompose } from "@/lib/catalog";
import { authenticateNode, unauthorized } from "@/lib/node-auth";
import { publicUrl } from "@/lib/public-url";

/** The docker-compose file for the calling server, fetched by the installer. */
export async function GET(request: Request) {
  const node = await authenticateNode(request);
  if (!node) return unauthorized();
  return new Response(nodeCompose(node, publicUrl(request.headers)), {
    headers: { "content-type": "text/yaml; charset=utf-8" },
  });
}
