import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { HashrateChart } from "@/components/hashrate-chart";
import { WalletSearch } from "@/components/miner-search";
import { BlockStatus, Box, Empty, Skeleton } from "@/components/ui";
import { formatAge, formatAmount, formatHashrate, shorten } from "@/lib/format";
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

async function Wallet({ params }: Pick<PageProps<"/wallet/[address]">, "params">) {
  const { address } = await params;
  if (!isPlausibleAddress(address)) {
    notFound();
  }
  const accounts = await getMiner(address);
  return (
    <div className="columns">
      <div className="column">
        <Box title="YOUR WALLET">
          <WalletSearch initial={address} />
        </Box>
        <Box title={<>Wallet: {address}</>}>
          {accounts.length === 0 ? (
            <Empty>This address has not submitted any shares yet.</Empty>
          ) : (
            <table className="datagrid">
              <thead>
                <tr>
                  <th>Coin</th>
                  <th className="right">Hashrate</th>
                  <th className="right">Immature</th>
                  <th className="right">Balance</th>
                  <th className="right">Total paid</th>
                  <th className="right">Total earned</th>
                </tr>
              </thead>
              <tbody>
                {accounts.map((account) => (
                  <tr key={account.symbol}>
                    <td>
                      <b>{account.symbol}</b>
                    </td>
                    <td className="right">{formatHashrate(account.hashrate)}</td>
                    <td className="right">{formatAmount(account.immature)}</td>
                    <td className="right">
                      <b>{formatAmount(account.balance)}</b>
                    </td>
                    <td className="right">{formatAmount(account.paid)}</td>
                    <td className="right">
                      {formatAmount((BigInt(account.balance) + BigInt(account.paid)).toString())}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Box>
        {accounts.map((account) => (
          <Miners key={account.symbol} account={account} />
        ))}
        {accounts.map((account) => (
          <Box key={account.symbol} title={`Last 24 Hours Hashrate (${account.symbol})`}>
            <div className="p-2">
              <HashrateChart points={account.history} label={`${account.symbol} hashrate`} />
            </div>
          </Box>
        ))}
      </div>
      <div className="column">
        {accounts.map((account) => (
          <Payouts key={account.symbol} account={account} />
        ))}
        {accounts.map((account) => (
          <Earnings key={account.symbol} account={account} />
        ))}
      </div>
    </div>
  );
}

function Miners({ account }: { account: MinerAccount }) {
  return (
    <Box title={`Miners (${account.symbol})`}>
      <table className="datagrid">
        <thead>
          <tr>
            <th>Worker</th>
            <th className="right">Hashrate</th>
            <th className="right">Last share</th>
          </tr>
        </thead>
        <tbody>
          {account.workers.map((worker) => (
            <tr key={worker.name}>
              <td>{worker.name || "(default)"}</td>
              <td className="right">{formatHashrate(worker.hashrate)}</td>
              <td className="right">{formatAge(worker.lastShareAt)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Box>
  );
}

function Payouts({ account }: { account: MinerAccount }) {
  return (
    <Box title={`Last Payouts (${account.symbol})`}>
      {account.payouts.length === 0 ? (
        <Empty>No payouts yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th>Time</th>
              <th className="right">Amount</th>
              <th className="right">Tx fee</th>
              <th>Tx</th>
              <th className="right">Status</th>
            </tr>
          </thead>
          <tbody>
            {account.payouts.map((payout) => (
              <tr key={payout.id}>
                <td>{formatAge(payout.createdAt)}</td>
                <td className="right">{formatAmount(payout.amount)}</td>
                <td className="right">{payout.networkFee === null ? "–" : formatAmount(payout.networkFee)}</td>
                <td className="mono" title={payout.txid ?? undefined}>
                  {payout.txid ? shorten(payout.txid) : "–"}
                </td>
                <td className="right">{payout.status === "sent" ? "Sent" : "Queued"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Box>
  );
}

function Earnings({ account }: { account: MinerAccount }) {
  return (
    <Box title={`Last 20 Earnings (${account.symbol})`}>
      {account.credits.length === 0 ? (
        <Empty>No earnings yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th className="right">Block</th>
              <th className="right">Amount</th>
              <th className="right">Time</th>
              <th className="right">Status</th>
            </tr>
          </thead>
          <tbody>
            {account.credits.map((credit, index) => (
              <tr key={index}>
                <td className="right">{credit.height}</td>
                <td className="right">{formatAmount(credit.amount)}</td>
                <td className="right">{formatAge(credit.foundAt)}</td>
                <td className="right">
                  <BlockStatus status={credit.status} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Box>
  );
}
