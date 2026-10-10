import type { Metadata } from "next";
import Link from "next/link";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlockStatus, Box, Empty, Skeleton } from "@/components/ui";
import { getAdminBlocks } from "@/lib/admin-queries";
import { formatAge, formatAmount, shorten } from "@/lib/format";

export const metadata: Metadata = { title: "Blocks" };

export default function BlocksPage() {
  return (
    <>
      <AutoRefresh seconds={30} />
      <Suspense fallback={<Skeleton height={400} />}>
        <Blocks />
      </Suspense>
    </>
  );
}

async function Blocks() {
  const blocks = await getAdminBlocks(200);
  return (
    <Box title="Latest 200 blocks">
      {blocks.length === 0 ? (
        <Empty>No blocks found yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th>Coin</th>
              <th className="right">Block</th>
              <th>Status</th>
              <th>Mode</th>
              <th className="right">Reward</th>
              <th className="right">To miners</th>
              <th className="right">Pool fee</th>
              <th className="right">Recipients</th>
              <th>Found by</th>
              <th className="right">Time</th>
            </tr>
          </thead>
          <tbody>
            {blocks.map((block) => (
              <tr key={block.id}>
                <td>
                  <b>{block.symbol}</b>
                </td>
                <td className="right">
                  <Link href={`/admin/blocks/${block.id}`}>{block.height}</Link>
                </td>
                <td>
                  <BlockStatus status={block.status} />
                </td>
                <td>{block.mode}</td>
                <td className="right">{formatAmount(block.reward, 4)}</td>
                <td className="right">{block.credited ? formatAmount(block.toMiners, 4) : "not split yet"}</td>
                <td className="right">
                  {block.credited ? formatAmount((BigInt(block.reward) - BigInt(block.toMiners)).toString(), 4) : "–"}
                </td>
                <td className="right">{block.recipients}</td>
                <td>
                  <Link href={`/wallet/${block.finder}`} className="mono">
                    {shorten(block.finder)}
                  </Link>
                </td>
                <td className="right">{formatAge(block.foundAt)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Box>
  );
}
