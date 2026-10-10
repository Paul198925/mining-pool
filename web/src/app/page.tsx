import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { WalletSearch } from "@/components/miner-search";
import { Box, Empty, Skeleton } from "@/components/ui";
import { formatHashrate } from "@/lib/format";
import { getBlocks, getCoins } from "@/lib/queries";

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
  // Reading the coins first makes this render per request, so the host can come from the environment.
  const coins = await getCoins();
  const host = process.env.POOL_STRATUM_HOST ?? "localhost";
  const example = coins.find((coin) => coin.stratumPort !== null);
  return (
    <Box title="STRATUM SERVERS">
      <ul>
        <li>
          <p className="cmdline">
            -a &lt;ALGO&gt; -o stratum+tcp://{host}:&lt;PORT&gt; -u &lt;WALLET_ADDRESS&gt;[.&lt;WORKER&gt;] [-p
            &lt;OPTIONS&gt;]
          </p>
        </li>
        <li>
          &lt;WALLET_ADDRESS&gt; must be valid for the currency you mine. <b>Do not use an address of another coin.</b>
        </li>
        <li>See the &quot;Pool Status&quot; area for ALGO and PORT numbers.</li>
        <li>
          The coin is recognised from your wallet address. As optional password, you can use{" "}
          <b>-p c=&lt;SYMBOL&gt;</b> to name it yourself.
        </li>
        <li>
          Add <b>m=solo</b> to mine solo: you keep every block you find and get nothing from
          other miners&apos; blocks.
        </li>
        <li>
          Add <b>m=party.&lt;NAME&gt;</b> to mine in a private group: everyone using the same name shares the blocks
          the group finds. Separate several options with commas, e.g. <b>-p c=DOGE,m=solo</b>.
        </li>
        {example && (
          <li>
            Example:
            <p className="cmdline">
              -a {example.algo} -o stratum+tcp://{host}:{example.stratumPort} -u &lt;{example.symbol}_ADDRESS&gt;.rig1
              -p c={example.symbol}
            </p>
          </li>
        )}
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
                    <b>{algo}</b>
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
