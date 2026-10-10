import type { Metadata } from "next";
import { connection } from "next/server";
import { Suspense } from "react";

import { Box, Empty, Skeleton } from "@/components/ui";
import { requireAdmin } from "@/lib/admin-auth";
import { COINS } from "@/lib/catalog";
import { db } from "@/lib/db";
import { poolAddresses } from "@/lib/mesh";

import { type AdminCoin, CoinRow } from "./coin-row";

export const metadata: Metadata = { title: "Coins" };

export default function CoinsPage() {
  return (
    <Suspense fallback={<Skeleton height={200} />}>
      <Coins />
    </Suspense>
  );
}

async function getCoins(): Promise<{ coins: AdminCoin[]; unknown: string[] }> {
  const [rows, servers, addresses, backups] = await Promise.all([
    db().query("SELECT symbol, name, algo, enabled, payouts_enabled, pool_fee_percent, solo_fee_percent, min_payout FROM coins ORDER BY symbol"),
    // A node that holds the pool wallet serves miners too.
    db().query("SELECT region, coins || wallets AS coins FROM stratum_instances WHERE token_hash IS NOT NULL ORDER BY id"),
    poolAddresses(),
    db().query("SELECT symbol, max(received_at) AS at FROM wallet_backups GROUP BY symbol"),
  ]);
  const coins = rows.rows.map((r) => ({
    symbol: r.symbol,
    name: r.name,
    algo: r.algo,
    enabled: r.enabled,
    payoutsEnabled: r.payouts_enabled,
    poolFeePercent: r.pool_fee_percent,
    soloFeePercent: r.solo_fee_percent,
    minPayout: r.min_payout === null ? null : Number(r.min_payout) / 1e8,
    hasWallet: Boolean(addresses[r.symbol]),
    backupAt: backups.rows.find((backup) => backup.symbol === r.symbol)?.at ?? null,
    nodes: servers.rows.filter((server) => server.coins.includes(r.symbol)).map((server) => server.region),
  }));
  // The pool can run these, but pool-core has no wallet node configured for them yet.
  const unknown = COINS.map((coin) => coin.symbol).filter((symbol) => !coins.some((coin) => coin.symbol === symbol));
  return { coins, unknown };
}

async function Coins() {
  await connection();
  await requireAdmin();
  const { coins, unknown } = await getCoins();
  return (
    <Box title="Coins">
      {coins.length === 0 ? (
        <Empty>No coin is set up yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th>Coin</th>
              <th>Algo</th>
              <th>Mining on</th>
              <th>Payouts on</th>
              <th className="right">Pool fee %</th>
              <th className="right">Solo fee %</th>
              <th className="right">Minimum payout</th>
              <th>Pool wallet</th>
              <th>Wallet copy</th>
              <th>Nodes on servers</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {coins.map((coin) => (
              <CoinRow key={coin.symbol} coin={coin} />
            ))}
          </tbody>
        </table>
      )}
      <ul>
        <li>
          A coin can be mined when mining is on, the pool wallet is connected and at least one server runs a node for
          it. Switching a coin off stops mining on the stratum servers within about a minute and hides it from the
          website; open balances are still paid out.
        </li>
        <li>
          With payouts off, nothing is sent from the coin&apos;s wallet. Miners keep earning and their balances
          stay; they are paid in the first round after payouts are switched on again. A payment the wallet has
          already been asked to send is not stopped.
        </li>
        <li>Fees apply to blocks found from now on. The minimum payout applies from the next payout round.</li>
        <li>
          Wallet copy: pool-core has every wallet node write a backup of its wallet once a day, and the server
          hands a copy to the pool&apos;s database. It is the encrypted wallet; its passphrase is only in
          pool-core&apos;s environment. Click the age to download the newest copy. With the database backed up
          elsewhere, the wallets are too.
        </li>
        <li>Which server runs a node is set on the Servers page.</li>
        {unknown.length > 0 && (
          <li>
            Not listed: {unknown.join(", ")}. The pool supports them, but pool-core has no wallet node for them yet.
          </li>
        )}
      </ul>
    </Box>
  );
}
