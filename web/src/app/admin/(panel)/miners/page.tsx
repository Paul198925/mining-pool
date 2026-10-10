import type { Metadata } from "next";
import Link from "next/link";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { Box, Empty, Skeleton } from "@/components/ui";
import { getAdminMiners } from "@/lib/admin-queries";
import { formatAge, formatAmount, formatHashrate } from "@/lib/format";

export const metadata: Metadata = { title: "Miners" };

export default function MinersPage() {
  return (
    <>
      <AutoRefresh seconds={30} />
      <Suspense fallback={<Skeleton height={400} />}>
        <Miners />
      </Suspense>
    </>
  );
}

async function Miners() {
  const miners = await getAdminMiners(200);
  return (
    <Box title="Miners (200 most recently active)">
      {miners.length === 0 ? (
        <Empty>No miner has connected yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th>Coin</th>
              <th>Address</th>
              <th className="right">Workers</th>
              <th className="right">Hashrate</th>
              <th className="right">Balance</th>
              <th className="right">Total paid</th>
              <th className="right">Last share</th>
            </tr>
          </thead>
          <tbody>
            {miners.map((miner) => (
              <tr key={`${miner.symbol}-${miner.address}`}>
                <td>
                  <b>{miner.symbol}</b>
                </td>
                <td>
                  <Link href={`/wallet/${miner.address}`} className="mono">
                    {miner.address}
                  </Link>
                </td>
                <td className="right">{miner.workers}</td>
                <td className="right">{formatHashrate(miner.hashrate)}</td>
                <td className="right">{formatAmount(miner.balance, 4)}</td>
                <td className="right">{formatAmount(miner.paid, 4)}</td>
                <td className="right">{formatAge(miner.lastShareAt)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Box>
  );
}
