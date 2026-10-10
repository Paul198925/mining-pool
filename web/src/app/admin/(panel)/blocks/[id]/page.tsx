import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { Suspense } from "react";

import { BlockStatus, Box, Empty, Skeleton } from "@/components/ui";
import { getAdminBlock } from "@/lib/admin-queries";
import { formatAge, formatAmount } from "@/lib/format";

export const metadata: Metadata = { title: "Block" };

export default function BlockPage({ params }: PageProps<"/admin/blocks/[id]">) {
  return (
    <Suspense fallback={<Skeleton height={300} />}>
      <Block params={params} />
    </Suspense>
  );
}

async function Block({ params }: Pick<PageProps<"/admin/blocks/[id]">, "params">) {
  const { id } = await params;
  const found = await getAdminBlock(id);
  if (!found) {
    notFound();
  }
  const { block, credits } = found;
  const reward = BigInt(block.reward);
  return (
    <>
      <Box title={`${block.symbol} block ${block.height}`}>
        <table className="datagrid">
          <tbody>
            <tr>
              <th>Hash</th>
              <td className="mono">{block.hash}</td>
            </tr>
            <tr>
              <th>Status</th>
              <td>
                <BlockStatus status={block.status} /> {block.confirmations} confirmations
              </td>
            </tr>
            <tr>
              <th>Found</th>
              <td>
                {formatAge(block.foundAt)} by{" "}
                <Link href={`/wallet/${block.finder}`} className="mono">
                  {block.finder}
                </Link>{" "}
                ({block.mode})
              </td>
            </tr>
            <tr>
              <th>Reward</th>
              <td>{formatAmount(block.reward)}</td>
            </tr>
            <tr>
              <th>Pool fee</th>
              <td>{block.credited ? formatAmount((reward - BigInt(block.toMiners)).toString()) : "not split yet"}</td>
            </tr>
          </tbody>
        </table>
      </Box>
      <Box title={`Reward split (${credits.length} miners)`}>
        {credits.length === 0 ? (
          <Empty>{block.credited ? "No miner was credited." : "The reward has not been split yet."}</Empty>
        ) : (
          <table className="datagrid">
            <thead>
              <tr>
                <th>Address</th>
                <th className="right">Amount</th>
                <th className="right">Share of reward</th>
              </tr>
            </thead>
            <tbody>
              {credits.map((credit) => (
                <tr key={credit.address}>
                  <td>
                    <Link href={`/wallet/${credit.address}`} className="mono">
                      {credit.address}
                    </Link>
                  </td>
                  <td className="right">{formatAmount(credit.amount)}</td>
                  <td className="right">
                    {reward === BigInt(0) ? "–" : `${(Number((BigInt(credit.amount) * BigInt(10000)) / reward) / 100).toFixed(2)}%`}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Box>
    </>
  );
}
