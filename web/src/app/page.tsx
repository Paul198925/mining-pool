import Link from "next/link";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { HashrateChart } from "@/components/hashrate-chart";
import { MinerSearch } from "@/components/miner-search";
import { Card, Empty, Skeleton, Stat } from "@/components/ui";
import { formatAge, formatAmount, formatDifficulty, formatHashrate } from "@/lib/format";
import { type Coin, getBlocks, getCoins, getPoolHistory } from "@/lib/queries";

export default function Home() {
  return (
    <>
      <AutoRefresh />
      <Card title="Miner lookup">
        <MinerSearch />
      </Card>
      <Suspense fallback={<Skeleton height={420} />}>
        <Coins />
      </Suspense>
      <Suspense fallback={<Skeleton height={240} />}>
        <RecentBlocks />
      </Suspense>
    </>
  );
}

async function Coins() {
  const coins = await getCoins();
  if (coins.length === 0) {
    return (
      <Card>
        <Empty>No coins are configured yet.</Empty>
      </Card>
    );
  }
  return coins.map((coin) => <CoinCard key={coin.symbol} coin={coin} />);
}

async function CoinCard({ coin }: { coin: Coin }) {
  const history = await getPoolHistory(coin.symbol);
  const host = process.env.POOL_STRATUM_HOST ?? "localhost";
  return (
    <Card>
      <div className="mb-4 flex items-baseline gap-2">
        <h2 className="text-lg font-semibold">{coin.name}</h2>
        <span className="text-sm text-muted">
          {coin.symbol} · {coin.algo}
        </span>
      </div>
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
        <Stat label="Pool hashrate" value={formatHashrate(coin.hashrate)} />
        <Stat label="Miners" value={coin.miners} hint={`${coin.workers} ${coin.workers === 1 ? "worker" : "workers"}`} />
        <Stat
          label="Network hashrate"
          value={coin.networkHashrate === null ? "–" : formatHashrate(coin.networkHashrate)}
        />
        <Stat label="Difficulty" value={formatDifficulty(coin.networkDifficulty)} />
        <Stat label="Height" value={coin.height ?? "–"} />
        <Stat label="Blocks found" value={coin.blocksFound} hint={`last ${formatAge(coin.lastBlockAt)}`} />
      </div>
      <div className="mt-6">
        <h3 className="mb-2 text-xs text-muted">Pool hashrate, last 24 hours</h3>
        <HashrateChart points={history} label={`${coin.name} pool hashrate`} />
      </div>
      <div className="mt-6 border-t border-border pt-4">
        <h3 className="mb-2 text-xs text-muted">How to mine</h3>
        {coin.stratumPort === null ? (
          <p className="text-sm text-muted">The stratum server for this coin is not running.</p>
        ) : (
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
            <dt className="text-muted">Server</dt>
            <dd className="font-mono break-all">
              stratum+tcp://{host}:{coin.stratumPort}
            </dd>
            <dt className="text-muted">Username</dt>
            <dd>
              Your {coin.symbol} address, optionally followed by <span className="font-mono">.workername</span>
            </dd>
            <dt className="text-muted">Password</dt>
            <dd>
              <span className="font-mono">x</span>, or <span className="font-mono">m=solo</span> to mine solo: you keep
              every block you find ({coin.soloFeePercent ?? "–"}% fee) and get nothing from other miners&apos; blocks
            </dd>
            <dt className="text-muted">Payouts</dt>
            <dd>
              PPLNS, {coin.poolFeePercent ?? "–"}% fee, paid automatically from{" "}
              {coin.minPayout === null ? "–" : formatAmount(coin.minPayout, 2)} {coin.symbol} once a block has{" "}
              {coin.blockMaturity === null ? "–" : coin.blockMaturity + 1} confirmations. The transaction fee is
              shared by the miners in each payout.
            </dd>
          </dl>
        )}
      </div>
    </Card>
  );
}

async function RecentBlocks() {
  const blocks = await getBlocks(10);
  return (
    <Card
      title="Recent blocks"
      action={
        <Link href="/blocks" className="text-xs text-accent">
          All blocks
        </Link>
      }
    >
      <BlocksTable blocks={blocks} />
    </Card>
  );
}
