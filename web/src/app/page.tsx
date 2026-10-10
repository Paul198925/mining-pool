import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { WalletSearch } from "@/components/miner-search";
import { PoolStatus } from "@/components/pool-status";
import { StratumGenerator } from "@/components/stratum-generator";
import { Box, Empty, Skeleton } from "@/components/ui";
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
  const payoutCoins = coins.filter((coin) => coin.exchangeable).map((coin) => ({ symbol: coin.symbol, name: coin.name }));
  return (
    <Box title="STRATUM SERVERS">
      <ul>
        <li>
          {mineable.length === 0 || servers.length === 0 ? (
            <Empty>No stratum server is running.</Empty>
          ) : (
            <StratumGenerator coins={mineable} servers={servers} payoutCoins={payoutCoins} />
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
          &lt;WALLET_ADDRESS&gt; is where you are paid: an address of the coin you mine, or of another coin of this
          pool if you want to be paid in that one (see Exchange below).
        </li>
        <li>
          Options: <b>c=&lt;SYMBOL&gt;</b> names the coin (otherwise it is recognised from your address),{" "}
          <b>m=solo</b> mines solo, <b>m=party.&lt;NAME&gt;</b> mines in a private group. Separate several options
          with commas.
        </li>
        <li>
          Exchange: to be paid in another coin of this pool, use your address of that coin and add{" "}
          <b>pay=&lt;SYMBOL&gt;</b>. The pool swaps what you mine with miners who want it the other way round, at
          the market price and for a small fee; nothing is sold on an exchange, so a swap waits until someone wants
          your coin.
        </li>
        <li>
          Solo: you keep every block you find and get nothing from other miners&apos; blocks. Party: everyone using
          the same name shares the blocks the group finds.
        </li>
      </ul>
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
