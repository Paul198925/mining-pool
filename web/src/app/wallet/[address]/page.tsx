import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { HashrateChart } from "@/components/hashrate-chart";
import { WalletSearch } from "@/components/miner-search";
import { PagedTable } from "@/components/paged-table";
import { PoolStatus } from "@/components/pool-status";
import { RememberWallet, SavedWallets } from "@/components/saved-wallets";
import { BlockStatus, Box, Empty, Skeleton } from "@/components/ui";
import {
  formatAge,
  formatAmount,
  formatDifficulty,
  formatDuration,
  formatHashrate,
  formatUsd,
  shorten,
  timeToFind,
  valueUsd,
} from "@/lib/format";
import { getMinerExtras, type MinerExtras } from "@/lib/miner-queries";
import { getMiner, isPlausibleAddress, type MinerAccount } from "@/lib/queries";

export const metadata: Metadata = { title: "Wallet" };

export default function WalletPage({ params }: PageProps<"/wallet/[address]">) {
  return (
    <>
      <AutoRefresh />
      <Suspense fallback={<Skeleton height={480} />}>
        <Wallet params={params} />
      </Suspense>
    </>
  );
}

/** One account of the address with everything the page shows about it. */
// The detailed per-coin rows replace the short ones.
type Account = Omit<MinerAccount, "mining"> & MinerExtras;

const sum = (...amounts: string[]) => amounts.reduce((total, amount) => total + BigInt(amount), BigInt(0)).toString();

async function Wallet({ params }: Pick<PageProps<"/wallet/[address]">, "params">) {
  const { address } = await params;
  if (!isPlausibleAddress(address)) {
    notFound();
  }
  const basics = await getMiner(address);
  const accounts: Account[] = await Promise.all(
    basics.map(async (account) => ({ ...account, ...(await getMinerExtras(account.id, account.symbol)) })),
  );
  return (
    <div className="columns">
      <div className="column">
        {accounts.length === 0 ? (
          <Box title={<>Earnings Summary: {address}</>}>
            <Empty>This address has not submitted any shares yet.</Empty>
          </Box>
        ) : (
          accounts.map((account) => <Summary key={account.symbol} account={account} />)
        )}
        {accounts.map((account) => (
          <Box key={account.symbol} title={`Last 24 Hours Balance (${account.symbol})`}>
            <div className="p-2">
              <HashrateChart points={account.balanceHistory} label={`${account.symbol} balance`} unit={account.symbol} />
            </div>
          </Box>
        ))}
        {accounts.map((account) => (
          <Box key={account.symbol} title={`Last 24h Miners Hashrate (${account.symbol} account)`}>
            <div className="p-2">
              <HashrateChart points={account.history} label={`${account.symbol} account hashrate`} />
            </div>
          </Box>
        ))}
        {accounts.map((account) => (
          <BlocksFound key={account.symbol} account={account} />
        ))}
        <Box title="Search">
          <WalletSearch initial={address} />
          {/* Only addresses the pool knows: a typo should not end up in the visitor's list. */}
          {accounts.length > 0 && <RememberWallet address={address} />}
          <SavedWallets />
        </Box>
      </div>
      <div className="column">
        {accounts.map((account) => (
          <MiningActivity key={account.symbol} account={account} />
        ))}
        {accounts.map((account) => (
          <Earnings key={account.symbol} account={account} />
        ))}
        {accounts.map((account) => (
          <Payouts key={account.symbol} account={account} />
        ))}
        {accounts
          .filter((account) => account.waiting.length > 0 || account.exchanges.length > 0)
          .map((account) => (
            <Exchanges key={account.symbol} account={account} />
          ))}
        <Suspense fallback={<Skeleton height={160} />}>
          <PoolStatus />
        </Suspense>
      </div>
    </div>
  );
}

/**
 * Reasons why this account's payout may be late, in the miner's words. Nothing here
 * means money is lost: a balance stays the miner's until it is paid.
 */
function payoutWarnings(account: Account): string[] {
  const { symbol } = account;
  const warnings: string[] = [];
  const balance = BigInt(account.balance);
  const queued = BigInt(account.queued);
  // What this account is waiting for: a payout already queued, or a balance that is due.
  const due = queued + (account.minPayout !== null && balance >= BigInt(account.minPayout) ? balance : BigInt(0));
  if (due > BigInt(0)) {
    if (!account.payoutsEnabled) {
      warnings.push(`Payouts in ${symbol} are paused at the moment. Your ${formatAmount(due.toString())} ${symbol} are paid as soon as they are switched on again.`);
    } else if (!account.walletReachable) {
      warnings.push(`The pool's ${symbol} wallet cannot be reached at the moment, so payouts are delayed. Your ${formatAmount(due.toString())} ${symbol} are paid once it is back.`);
    } else if (account.walletBalance !== null) {
      const wallet = BigInt(account.walletBalance);
      // The wallet sends a round's payouts in one transaction: if it is short for all
      // of them, nobody in that round is paid yet.
      const needed = BigInt(account.coinQueued) > due ? BigInt(account.coinQueued) : due;
      if (wallet < needed) {
        warnings.push(
          `The pool's wallet does not hold enough ${symbol} right now to pay this round (${formatAmount(wallet.toString())} of ${formatAmount(needed.toString())} ${symbol} available). Your ${formatAmount(due.toString())} ${symbol} stay yours and are paid automatically as soon as the wallet has enough, usually when recent blocks have matured.`,
        );
      }
    }
  }
  if (account.waiting.length > 0) {
    const amounts = account.waiting.map((item) => `${formatAmount(item.amount)} ${item.symbol}`).join(", ");
    warnings.push(
      `${amounts} are waiting to be swapped into ${symbol}. The pool swaps only between its miners, so this takes until someone wants those coins for ${symbol}; they are not paid out in the meantime.`,
    );
  }
  return warnings;
}

/** Balances and totals of one account, in the coin it is paid in. */
function Summary({ account }: { account: Account }) {
  const { symbol } = account;
  const unpaid = sum(account.balance, account.immature);
  const amount = (units: string) => `${formatAmount(units)} ${symbol}`;
  const totals: [string, string][] = [
    ["Total Unpaid", unpaid],
    ["Total Paid", account.paid],
    ["Total Earned (24 hours)", account.earned.day],
    ["Total Earned (14 days)", account.earned.twoWeeks],
    ["Total Earned (30 days)", account.earned.month],
    ["Total Earned (all time)", account.earned.all],
  ];
  return (
    <Box title={<>Earnings Summary: {account.address}</>}>
      {payoutWarnings(account).map((warning) => (
        <p key={warning} role="status" className="alert">
          {warning}
        </p>
      ))}
      <table className="datagrid">
        <thead>
          <tr>
            <th>Name</th>
            <th className="right">Immature</th>
            <th className="right">Confirmed</th>
            <th className="right">Total</th>
            <th className="right">Estimated value*</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td className="wrap">
              <b>{symbol}</b> (pending earnings)
            </td>
            <td className="right">{formatAmount(account.immature)}</td>
            <td className="right">{formatAmount(account.balance)}</td>
            <td className="right">{formatAmount(unpaid)}</td>
            <td className="right">{formatUsd(valueUsd(unpaid, account.priceUsd))}</td>
          </tr>
          {account.waiting.map((item) => (
            <tr key={item.symbol}>
              <td className="wrap">
                <b>{item.symbol}</b> (waiting to be swapped into {symbol})
              </td>
              <td className="right">–</td>
              <td className="right">{formatAmount(item.amount)}</td>
              <td className="right">{formatAmount(item.amount)}</td>
              <td className="right">–</td>
            </tr>
          ))}
          <tr>
            <td colSpan={3} className="wrap">
              <b>Balance</b>{" "}
              {!account.payoutsEnabled
                ? `payouts in ${symbol} are paused at the moment`
                : account.minPayout === null
                  ? ""
                  : `is paid out from ${formatAmount(account.minPayout)} ${symbol}; the transaction fee is taken from the payment`}
            </td>
            <td className="right">
              <b>{amount(account.balance)}</b>
            </td>
            <td className="right">{formatUsd(valueUsd(account.balance, account.priceUsd))}</td>
          </tr>
          {totals.map(([label, units]) => (
            <tr key={label}>
              <td colSpan={3}>
                <b>{label}</b>
              </td>
              <td className="right">{amount(units)}</td>
              <td className="right">{formatUsd(valueUsd(units, account.priceUsd))}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <ul>
        <li>
          {account.priceUsd === null
            ? `* the pool has no current price for ${symbol}.`
            : `* at the current price of ${formatUsd(account.priceUsd)} for 1 ${symbol}.`}
        </li>
      </ul>
    </Box>
  );
}

/** What the account works on right now, and each of its workers. */
function MiningActivity({ account }: { account: Account }) {
  const active = account.mining.reduce((most, coin) => Math.max(most, coin.workers), 0);
  return (
    <Box title={<>Mining Activity: {account.address}</>}>
      {account.mining.length === 0 ? (
        <Empty>No share in the last ten minutes.</Empty>
      ) : (
        <table className="datagrid compact">
          <thead>
            <tr>
              <th>Name</th>
              <th className="right">Workers (active)</th>
              <th className="right" title="Your part of the pool's hashrate on this coin, and so of its next block">
                Block share
              </th>
              <th className="right" title="What that part of the next block would be, after the pool fee">
                Reward (estimated)
              </th>
              <th className="right">Hashrate (shared)</th>
              <th className="right" title="How long the pool needs on average for a block of this coin">
                TTF (shared)
              </th>
              <th className="right">Hashrate (solo)</th>
              <th className="right" title="How long your solo hashrate needs on average for a block">
                TTF (solo)
              </th>
            </tr>
          </thead>
          <tbody>
            {account.mining.map((coin) => {
              const share = coin.poolHashrate > 0 ? coin.sharedHashrate / coin.poolHashrate : 0;
              const reward =
                coin.lastReward === null || share === 0
                  ? null
                  : (Number(coin.lastReward) / 1e8) * share * (1 - (coin.poolFeePercent ?? 0) / 100);
              return (
                <tr key={coin.symbol}>
                  <td>
                    <b>{coin.name}</b> ({coin.algo})
                  </td>
                  <td className="right">{coin.workers}</td>
                  <td className="right">{share === 0 ? "–" : `${Number((share * 100).toPrecision(3))}%`}</td>
                  <td className="right">{reward === null ? "–" : `${Number(reward.toPrecision(4))} ${coin.symbol}`}</td>
                  <td className="right">{coin.sharedHashrate > 0 ? formatHashrate(coin.sharedHashrate) : "–"}</td>
                  <td className="right">
                    {coin.sharedHashrate > 0 ? formatDuration(timeToFind(coin.networkDifficulty, coin.poolHashrate)) : "–"}
                  </td>
                  <td className="right">{coin.soloHashrate > 0 ? formatHashrate(coin.soloHashrate) : "–"}</td>
                  <td className="right">{formatDuration(timeToFind(coin.networkDifficulty, coin.soloHashrate))}</td>
                </tr>
              );
            })}
            <tr>
              <td>Total workers:</td>
              <td className="right">{active}</td>
              <td colSpan={6}></td>
            </tr>
          </tbody>
        </table>
      )}
      {account.connections.length > 0 && (
        <table className="datagrid compact">
          <thead>
            <tr>
              <th>Worker</th>
              <th>Version</th>
              <th>Password</th>
              <th>Algo</th>
              <th className="right">Diff</th>
              <th className="right">Hashrate</th>
              <th className="right">Shares per min</th>
              <th className="right">Last share</th>
            </tr>
          </thead>
          <tbody>
            {account.connections.map((worker) => (
              <tr key={worker.name}>
                <td>{worker.name || "(default)"}</td>
                <td className="break">{worker.version ?? "–"}</td>
                <td className="mono break">{worker.password ?? "–"}</td>
                <td>{worker.algo ?? "–"}</td>
                <td className="right">{formatDifficulty(worker.difficulty)}</td>
                <td className="right">{formatHashrate(worker.hashrate)}</td>
                <td className="right">{Number(worker.sharesPerMinute.toFixed(1))}</td>
                <td className="right">{formatAge(worker.lastShareAt)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <ul>
        <li>
          Estimates from the last ten minutes. You are paid for the blocks the pool really finds, which is a matter of
          luck; TTF is the average time for one.
        </li>
      </ul>
    </Box>
  );
}

function BlocksFound({ account }: { account: Account }) {
  return (
    <Box title={<>Last Blocks found: {account.address}</>}>
      <PagedTable
        empty="This address has not found a block yet."
        columns={[
          { label: "Name" },
          { label: "Amount", right: true },
          { label: "Diff", right: true },
          { label: "Block", right: true },
          { label: "Time", right: true },
          { label: "Type" },
          { label: "Status", right: true },
        ]}
        rows={account.found.map((block) => ({
          key: block.id,
          text: `${block.symbol} ${block.height} ${block.type} ${block.status}`,
          cells: [
            <b key="name">{block.symbol}</b>,
            formatAmount(block.reward, 4),
            formatDifficulty(block.networkDifficulty),
            block.height,
            formatAge(block.foundAt),
            block.type,
            <BlockStatus key="status" status={block.status} />,
          ],
        }))}
      />
    </Box>
  );
}

function Earnings({ account }: { account: Account }) {
  return (
    <Box title={<>Last Earnings: {account.address}</>}>
      <PagedTable
        empty="No earnings yet."
        columns={[
          { label: "Name" },
          { label: "Block", right: true },
          { label: "Amount", right: true },
          { label: "%", right: true, title: "Your part of the block's reward" },
          { label: "Time", right: true },
          { label: "Status", right: true },
        ]}
        rows={account.earnings.map((earning, index) => ({
          key: `${earning.symbol}-${earning.height}-${index}`,
          text: `${earning.symbol} ${earning.height} ${earning.status}`,
          cells: [
            <b key="name">{earning.symbol}</b>,
            earning.height,
            formatAmount(earning.amount),
            `${Number(earning.percent.toPrecision(3))}%`,
            formatAge(earning.foundAt),
            <BlockStatus key="status" status={earning.status} />,
          ],
        }))}
      />
    </Box>
  );
}

function Payouts({ account }: { account: Account }) {
  return (
    <Box title={`Last Payouts (${account.symbol})`}>
      <PagedTable
        empty="No payouts yet."
        columns={[
          { label: "Time" },
          { label: "Amount", right: true },
          { label: "Tx fee", right: true },
          { label: "Tx" },
          { label: "Status", right: true },
        ]}
        rows={account.payouts.map((payout) => ({
          key: payout.id,
          text: `${payout.txid ?? ""} ${payout.status}`,
          cells: [
            formatAge(payout.createdAt),
            formatAmount(payout.amount),
            payout.networkFee === null ? "–" : formatAmount(payout.networkFee),
            <span key="tx" className="mono" title={payout.txid ?? undefined}>
              {payout.txid ? shorten(payout.txid) : "–"}
            </span>,
            payout.status === "sent" ? "Sent" : "Queued",
          ],
        }))}
      />
    </Box>
  );
}

function Exchanges({ account }: { account: Account }) {
  return (
    <Box title={`Exchange into ${account.symbol}`}>
      {account.waiting.length > 0 && (
        <ul>
          <li>
            Waiting to be swapped into {account.symbol}:{" "}
            {account.waiting.map((item) => `${formatAmount(item.amount)} ${item.symbol}`).join(", ")}. The pool swaps
            between its miners, so this happens as other miners swap the other way.
          </li>
        </ul>
      )}
      {account.exchanges.length > 0 && (
        <table className="datagrid">
          <thead>
            <tr>
              <th>Time</th>
              <th className="right">Gave</th>
              <th className="right">Got</th>
              <th className="right">Fee</th>
            </tr>
          </thead>
          <tbody>
            {account.exchanges.map((exchange) => (
              <tr key={exchange.id}>
                <td>{formatAge(exchange.createdAt)}</td>
                <td className="right">
                  {formatAmount(exchange.fromAmount)} {exchange.from}
                </td>
                <td className="right">
                  {formatAmount(exchange.toAmount)} {exchange.to}
                </td>
                <td className="right">
                  {formatAmount(exchange.feeAmount)} {exchange.to}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Box>
  );
}
