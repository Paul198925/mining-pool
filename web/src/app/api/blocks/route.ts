import type { NextRequest } from "next/server";

import { unitsToDecimal } from "@/lib/format";
import { getBlocks } from "@/lib/queries";

const MAX_LIMIT = 500;

export async function GET(request: NextRequest) {
  const requested = Number(request.nextUrl.searchParams.get("limit") ?? 50);
  const limit = Number.isInteger(requested) && requested > 0 ? Math.min(requested, MAX_LIMIT) : 50;
  const blocks = await getBlocks(limit);
  return Response.json({
    blocks: blocks.map((block) => ({
      symbol: block.symbol,
      height: block.height,
      hash: block.hash,
      status: block.status,
      confirmations: block.confirmations,
      reward: unitsToDecimal(block.reward),
      networkDifficulty: block.networkDifficulty,
      finder: block.finder,
      solo: block.solo,
      foundAt: block.foundAt,
    })),
  });
}
