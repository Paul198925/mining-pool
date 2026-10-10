import type { Metadata } from "next";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { Box, Empty, Skeleton } from "@/components/ui";
import { formatAge, formatAmount, formatDifficulty, formatHashrate } from "@/lib/format";
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
      {coins.length === 0 ? (
        <Empty>No coins are configured yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th>Name</th>
              <th>Algo</th>
              <th className="right">Port</th>
              <th className="right">Height</th>
              <th className="right">Difficulty</th>
              <th className="right">Net hash</th>
              <th className="right">Pool hash</th>
              <th className="right">Miners</th>
              <th className="right">Blocks</th>
              <th className="right">Last block</th>
              <th className="right">Min. pay</th>
            </tr>
          </thead>
          <tbody>
            {coins.map((coin) => (
              <tr key={coin.symbol}>
                <td>
                  <b>{coin.name}</b> ({coin.symbol})
                </td>
                <td>{coin.algo}</td>
                <td className="right">{coin.stratumPort ?? "–"}</td>
                <td className="right">{coin.height ?? "–"}</td>
                <td className="right">{formatDifficulty(coin.networkDifficulty)}</td>
                <td className="right">
                  {coin.networkHashrate === null ? "–" : formatHashrate(coin.networkHashrate)}
                </td>
                <td className="right">{formatHashrate(coin.hashrate)}</td>
                <td className="right" title={`${coin.workers} workers`}>
                  {coin.miners}
                </td>
                <td className="right">{coin.blocksFound}</td>
                <td className="right">{formatAge(coin.lastBlockAt)}</td>
                <td className="right">{coin.minPayout === null ? "–" : formatAmount(coin.minPayout, 2)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
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
