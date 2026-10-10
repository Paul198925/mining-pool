import type { Metadata } from "next";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { CoinsTable } from "@/components/coins-table";
import { Box, Skeleton } from "@/components/ui";
import { getBlocks, getCoins } from "@/lib/queries";

export const metadata: Metadata = { title: "Pool" };

export default function PoolPage() {
  return (
    <div className="columns">
      <AutoRefresh />
      <div className="column">
        <Suspense fallback={<Skeleton height={200} />}>
          <Coins />
        </Suspense>
      </div>
      <div className="column">
        <Suspense fallback={<Skeleton height={500} />}>
          <Blocks />
        </Suspense>
      </div>
    </div>
  );
}

async function Coins() {
  const coins = await getCoins();
  return (
    <Box title="Pool Status">
      <CoinsTable coins={coins} />
    </Box>
  );
}

async function Blocks() {
  const blocks = await getBlocks(50);
  return (
    <Box title="Last 50 Blocks">
      <BlocksTable blocks={blocks} />
    </Box>
  );
}
