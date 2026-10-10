import type { Metadata } from "next";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { HashrateChart } from "@/components/hashrate-chart";
import { Box, Empty, Skeleton } from "@/components/ui";
import { type Coin, getCoins, getPoolHistory } from "@/lib/queries";

export const metadata: Metadata = { title: "Graphs" };

export default function GraphsPage() {
  return (
    <>
      <AutoRefresh seconds={60} />
      <Suspense fallback={<Skeleton height={300} />}>
        <Graphs />
      </Suspense>
    </>
  );
}

async function Graphs() {
  const coins = await getCoins();
  if (coins.length === 0) {
    return (
      <Box title="Pool Hashrate">
        <Empty>No coins are configured yet.</Empty>
      </Box>
    );
  }
  return (
    <div className="columns">
      {coins.map((coin) => (
        <CoinGraph key={coin.symbol} coin={coin} />
      ))}
    </div>
  );
}

async function CoinGraph({ coin }: { coin: Coin }) {
  const history = await getPoolHistory(coin.symbol);
  return (
    <Box title={`Last 24 Hours Hashrate (${coin.symbol}, ${coin.algo})`}>
      <div className="p-2">
        <HashrateChart points={history} label={`${coin.name} pool hashrate`} />
      </div>
    </Box>
  );
}
