import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { Box, Empty, Skeleton } from "@/components/ui";
import { type CoinFinance, getFinances, getServers } from "@/lib/admin-queries";
import { formatAge, formatAmount } from "@/lib/format";

const STUCK_AFTER_MS = 10 * 60 * 1000;

export default function AdminOverview() {
  return (
    <>
      <AutoRefresh seconds={30} />
      <Suspense fallback={<Skeleton height={300} />}>
        <Overview />
      </Suspense>
    </>
  );
}

/** What the pool must be able to pay right now: balances plus payouts that are debited but not sent. */
function owed(coin: CoinFinance): bigint {
  return BigInt(coin.minerBalances) + BigInt(coin.payoutsOpen);
}

function problems(coins: CoinFinance[], liveServers: number, unreachable: string[]): string[] {
  const found: string[] = [];
  if (liveServers === 0) found.push("No stratum server is running.");
  for (const symbol of unreachable) {
    found.push(`${symbol}: pool-core has not reached the coin daemon for over two minutes.`);
  }
  for (const coin of coins) {
    if (coin.ledgerSum !== "0") {
      found.push(`${coin.symbol}: the ledger does not sum to zero (${formatAmount(coin.ledgerSum)}).`);
    }
    if (coin.walletBalance !== null && BigInt(coin.walletBalance) < owed(coin)) {
      found.push(`${coin.symbol}: the wallet holds less than the pool owes its miners.`);
    }
    if (coin.oldestSendingAt && Date.now() - coin.oldestSendingAt.getTime() > STUCK_AFTER_MS) {
      found.push(`${coin.symbol}: ${coin.payoutsSending} payout(s) have been "sending" since ${formatAge(coin.oldestSendingAt)}.`);
    }
  }
  return found;
}

async function Overview() {
  const [coins, servers] = await Promise.all([getFinances(), getServers()]);
  const liveServers = servers.instances.filter((instance) => instance.live).length;
  const unreachable = servers.nodes.filter((node) => !node.reachable).map((node) => node.symbol);
  const alerts = problems(coins, liveServers, unreachable);
  for (const node of servers.nodes) {
    if (node.reachable && node.peers === 0) {
      alerts.push(`${node.symbol}: the wallet node has no connection to the coin's network.`);
    }
  }

  return (
    <>
      <Box title="Status">
        {alerts.length === 0 ? (
          <p className="notice">No problems found.</p>
        ) : (
          <ul>
            {alerts.map((alert) => (
              <li key={alert}>
                <p className="alert">{alert}</p>
              </li>
            ))}
          </ul>
        )}
      </Box>

      <Box title="Money">
        {coins.length === 0 ? (
          <Empty>No coins are configured yet.</Empty>
        ) : (
          <table className="datagrid">
            <thead>
              <tr>
                <th>Coin</th>
                <th className="right">Mined</th>
                <th className="right">Pool income</th>
                <th className="right">Paid out</th>
                <th className="right">Tx fees</th>
                <th className="right">Miner balances</th>
                <th className="right">Payouts queued</th>
                <th className="right">Immature</th>
                <th className="right">Wallet</th>
                <th className="right">Wallet − owed</th>
              </tr>
            </thead>
            <tbody>
              {coins.map((coin) => (
                <tr key={coin.symbol}>
                  <td>
                    <b>{coin.symbol}</b>
                  </td>
                  <td className="right">{formatAmount(coin.mined, 2)}</td>
                  <td className="right">{formatAmount(coin.poolFee, 4)}</td>
                  <td className="right">{formatAmount(coin.paidOut, 2)}</td>
                  <td className="right">{formatAmount(coin.networkFee, 4)}</td>
                  <td className="right">{formatAmount(coin.minerBalances, 4)}</td>
                  <td className="right">{formatAmount(coin.payoutsOpen, 4)}</td>
                  <td className="right">{formatAmount(coin.immature, 2)}</td>
                  <td className="right" title={`checked ${formatAge(coin.walletCheckedAt)}`}>
                    {coin.walletBalance === null ? "–" : formatAmount(coin.walletBalance, 2)}
                  </td>
                  <td className="right">
                    {coin.walletBalance === null
                      ? "–"
                      : formatAmount((BigInt(coin.walletBalance) - owed(coin)).toString(), 2)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <ul>
          <li>
            Mined = pool income + paid out + tx fees + miner balances. &quot;Wallet − owed&quot; is the spendable
            wallet balance minus miner balances and queued payouts; it must not be negative.
          </li>
        </ul>
      </Box>

      <Box title="Blocks">
        <table className="datagrid">
          <thead>
            <tr>
              <th>Coin</th>
              <th className="right">Last 24 hours</th>
              <th className="right">Immature</th>
              <th className="right">Confirmed</th>
              <th className="right">Orphaned</th>
              <th className="right">Orphan rate</th>
            </tr>
          </thead>
          <tbody>
            {coins.map((coin) => {
              const settled = coin.blocksConfirmed + coin.blocksOrphaned;
              return (
                <tr key={coin.symbol}>
                  <td>
                    <b>{coin.symbol}</b>
                  </td>
                  <td className="right">{coin.blocks24h}</td>
                  <td className="right">{coin.blocksPending}</td>
                  <td className="right">{coin.blocksConfirmed}</td>
                  <td className="right">{coin.blocksOrphaned}</td>
                  <td className="right">
                    {settled === 0 ? "–" : `${((coin.blocksOrphaned / settled) * 100).toFixed(1)}%`}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </Box>
    </>
  );
}
