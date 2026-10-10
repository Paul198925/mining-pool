import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { CoinsTable } from "@/components/coins-table";
import { HashrateChart } from "@/components/hashrate-chart";
import { StratumGenerator } from "@/components/stratum-generator";
import { Box, Empty, Skeleton } from "@/components/ui";
import { formatHashrate } from "@/lib/format";
import { mineableCoins } from "@/lib/catalog";
import { type Coin, getBlocks, getCoins, getPoolHistory, getStratumServers } from "@/lib/queries";

export const metadata: Metadata = { title: "Algorithm" };

export default function AlgoPage({ params }: PageProps<"/algo/[algo]">) {
  return (
    <>
      <AutoRefresh />
      <Suspense fallback={<Skeleton height={480} />}>
        <Algo params={params} />
      </Suspense>
    </>
  );
}

async function Algo({ params }: Pick<PageProps<"/algo/[algo]">, "params">) {
  const { algo } = await params;
  const coins = /^[a-z0-9-]{1,40}$/.test(algo) ? (await getCoins()).filter((coin) => coin.algo === algo) : [];
  if (coins.length === 0) {
    notFound();
  }
  const [servers, blocks] = await Promise.all([getStratumServers(), getBlocks(50, algo)]);
  const mineable = mineableCoins(coins);
  const hashrate = coins.reduce((sum, coin) => sum + coin.hashrate, 0);
  const miners = coins.reduce((sum, coin) => sum + coin.miners, 0);

  return (
    <div className="columns">
      <div className="column">
        <Box
          title={
            <>
              {algo} · {formatHashrate(hashrate)} · {miners} {miners === 1 ? "miner" : "miners"} ·{" "}
              <Link href="/pool">all algorithms</Link>
            </>
          }
        >
          <CoinsTable coins={coins} />
        </Box>
        <Box title={`Mine ${algo}`}>
          {mineable.length === 0 || servers.length === 0 ? (
            <Empty>No stratum server is running for this algorithm.</Empty>
          ) : (
            <StratumGenerator coins={mineable} servers={servers} />
          )}
        </Box>
        {coins.map((coin) => (
          <CoinGraph key={coin.symbol} coin={coin} />
        ))}
      </div>
      <div className="column">
        <Box title={`Last 50 Blocks (${algo})`}>
          <BlocksTable blocks={blocks} />
        </Box>
      </div>
    </div>
  );
}

async function CoinGraph({ coin }: { coin: Coin }) {
  const history = await getPoolHistory(coin.symbol);
  return (
    <Box title={`Last 24 Hours Hashrate (${coin.symbol})`}>
      <div className="p-2">
        <HashrateChart points={history} label={`${coin.name} pool hashrate`} />
      </div>
    </Box>
  );
}
