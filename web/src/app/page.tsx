import Link from "next/link";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { WalletSearch } from "@/components/miner-search";
import { StratumGenerator } from "@/components/stratum-generator";
import { Box, Empty, Skeleton } from "@/components/ui";
import { formatHashrate } from "@/lib/format";
import { mineableCoins } from "@/lib/catalog";
import { getBlocks, getCoins, getStratumServers } from "@/lib/queries";

export default function Home() {
  return (
    <div className="columns">
      <AutoRefresh />
      <div className="column">
        <Box title="YOUR WALLET">
          <WalletSearch />
        </Box>
        <Box title="MINING POOL">
          <ul>
            <li>No registration is required. Use your wallet address as the username.</li>
            <li>We do payouts in the currency you mine.</li>
            <li>
              Payouts are made automatically for all balances above the minimum shown on the Pool page, as soon as the
              blocks have matured. The transaction fee of a payout is shared by its recipients.
            </li>
            <li>Blocks are distributed proportionally among the most recent valid shares (PPLNS).</li>
          </ul>
        </Box>
        <Suspense fallback={<Skeleton height={200} />}>
          <StratumServers />
        </Suspense>
      </div>
      <div className="column">
        <Suspense fallback={<Skeleton height={160} />}>
          <PoolStatus />
        </Suspense>
        <Suspense fallback={<Skeleton height={300} />}>
          <LastBlocks />
        </Suspense>
      </div>
    </div>
  );
}

async function StratumServers() {
  const [coins, servers] = await Promise.all([getCoins(), getStratumServers()]);
  const mineable = mineableCoins(coins);
  return (
    <Box title="STRATUM SERVERS">
      <ul>
        <li>
          {mineable.length === 0 || servers.length === 0 ? (
            <Empty>No stratum server is running.</Empty>
          ) : (
            <StratumGenerator coins={mineable} servers={servers} />
          )}
        </li>
        <li>
          Choose a server near you and what you want to mine, and enter your wallet address; the line above is what your miner needs. The
          general form is:
          <p className="cmdline">
            -a &lt;ALGO&gt; -o stratum+tcp://&lt;SERVER&gt;:&lt;PORT&gt; -u &lt;WALLET_ADDRESS&gt;[.&lt;WORKER&gt;] [-p
            &lt;OPTIONS&gt;]
          </p>
        </li>
        <li>
          &lt;WALLET_ADDRESS&gt; must be valid for the currency you mine. <b>Do not use an address of another coin.</b>
        </li>
        <li>
          Options: <b>c=&lt;SYMBOL&gt;</b> names the coin (otherwise it is recognised from your address),{" "}
          <b>m=solo</b> mines solo, <b>m=party.&lt;NAME&gt;</b> mines in a private group. Separate several options
          with commas.
        </li>
        <li>
          Solo: you keep every block you find and get nothing from other miners&apos; blocks. Party: everyone using
          the same name shares the blocks the group finds.
        </li>
      </ul>
    </Box>
  );
}

/** Distinct values of one column within an algo, e.g. "3334, 3335". */
function distinct<T>(values: (T | null)[], format: (value: T) => string): string {
  const present = values.filter((value): value is T => value !== null).map(format);
  return present.length === 0 ? "–" : [...new Set(present)].join(", ");
}

/** One row per algorithm, as in YiiMP; the Pool page lists the individual coins. */
async function PoolStatus() {
  const coins = await getCoins();
  const algos = [...new Set(coins.map((coin) => coin.algo))].sort();
  return (
    <Box title="Pool Status">
      {coins.length === 0 ? (
        <Empty>No coins are configured yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th>Algo</th>
              <th className="right">Port</th>
              <th className="right">Coins</th>
              <th className="right">Miners</th>
              <th className="right">Hashrate</th>
              <th className="right">Fees</th>
              <th className="right">Solo</th>
            </tr>
          </thead>
          <tbody>
            {algos.map((algo) => {
              const members = coins.filter((coin) => coin.algo === algo);
              return (
                <tr key={algo}>
                  <td>
                    <b>
                      <Link href={`/algo/${algo}`}>{algo}</Link>
                    </b>
                  </td>
                  <td className="right">{distinct(members.map((coin) => coin.stratumPort), String)}</td>
                  <td className="right">{members.length}</td>
                  <td className="right">{members.reduce((sum, coin) => sum + coin.miners, 0)}</td>
                  <td className="right">{formatHashrate(members.reduce((sum, coin) => sum + coin.hashrate, 0))}</td>
                  <td className="right">{distinct(members.map((coin) => coin.poolFeePercent), (fee) => `${fee}%`)}</td>
                  <td className="right">{distinct(members.map((coin) => coin.soloFeePercent), (fee) => `${fee}%`)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </Box>
  );
}

async function LastBlocks() {
  const blocks = await getBlocks(15);
  return (
    <Box title="Last 15 Blocks">
      <BlocksTable blocks={blocks} />
    </Box>
  );
}
