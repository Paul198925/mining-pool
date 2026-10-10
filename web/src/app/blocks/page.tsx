import type { Metadata } from "next";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { Card, Skeleton } from "@/components/ui";
import { getBlocks } from "@/lib/queries";

export const metadata: Metadata = { title: "Blocks" };

export default function BlocksPage() {
  return (
    <>
      <AutoRefresh />
      <Suspense fallback={<Skeleton height={400} />}>
        <Blocks />
      </Suspense>
    </>
  );
}

async function Blocks() {
  const blocks = await getBlocks(100);
  return (
    <Card title="Latest 100 blocks">
      <BlocksTable blocks={blocks} />
    </Card>
  );
}
