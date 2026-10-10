import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { HashrateChart } from "@/components/hashrate-chart";
import { BlockStatus, Card, Empty, Skeleton, Stat, td, th } from "@/components/ui";
import { formatAge, formatAmount, formatHashrate, shorten } from "@/lib/format";
import { getMiner, isPlausibleAddress, type MinerAccount } from "@/lib/queries";

export const metadata: Metadata = { title: "Miner" };

export default function MinerPage({ params }: PageProps<"/miner/[address]">) {
  return (
    <>
      <AutoRefresh />
      <Suspense fallback={<Skeleton height={480} />}>
        <Miner params={params} />
      </Suspense>
    </>
  );
}

async function Miner({ params }: Pick<PageProps<"/miner/[address]">, "params">) {
  const { address } = await params;
  if (!isPlausibleAddress(address)) {
    notFound();
  }
  const accounts = await getMiner(address);
  if (accounts.length === 0) {
    return (
      <Card title="Miner">
        <p className="font-mono text-sm break-all">{address}</p>
        <Empty>This address has not submitted any shares yet.</Empty>
      </Card>
    );
  }
  return accounts.map((account) => <Account key={account.symbol} account={account} />);
}

function Account({ account }: { account: MinerAccount }) {
  const { symbol } = account;
  return (
    <>
      <Card>
        <div className="mb-4">
          <div className="text-xs text-muted">{symbol} miner</div>
          <h1 className="font-mono text-sm break-all sm:text-base">{account.address}</h1>
        </div>
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <Stat label="Hashrate (10 min)" value={formatHashrate(account.hashrate)} />
          <Stat label="Balance" value={formatAmount(account.balance, 4)} hint={`${symbol}, confirmed`} />
          <Stat label="Immature" value={formatAmount(account.immature, 4)} hint={`${symbol}, awaiting confirmations`} />
          <Stat label="Paid" value={formatAmount(account.paid, 4)} hint={`${symbol}, total`} />
        </div>
        <div className="mt-6">
          <h2 className="mb-2 text-xs text-muted">Hashrate, last 24 hours</h2>
          <HashrateChart points={account.history} label={`${symbol} miner hashrate`} />
        </div>
      </Card>

      <Card title="Workers">
        <table className="w-full">
          <thead>
            <tr className="border-b border-border">
              <th className={th}>Name</th>
              <th className={`${th} text-right`}>Hashrate (10 min)</th>
              <th className={`${th} text-right`}>Last share</th>
            </tr>
          </thead>
          <tbody>
            {account.workers.map((worker) => (
              <tr key={worker.name} className="border-b border-border last:border-0">
                <td className={`${td} font-mono`}>{worker.name || "(default)"}</td>
                <td className={`${td} text-right font-mono`}>{formatHashrate(worker.hashrate)}</td>
                <td className={`${td} text-right text-muted`}>{formatAge(worker.lastShareAt)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card title="Payouts">
          {account.payouts.length === 0 ? (
            <Empty>No payouts yet.</Empty>
          ) : (
            <table className="w-full">
              <thead>
                <tr className="border-b border-border">
                  <th className={`${th} text-right`}>Amount</th>
                  <th className={th}>Status</th>
                  <th className={th}>Transaction</th>
                  <th className={`${th} text-right`}>Created</th>
                </tr>
              </thead>
              <tbody>
                {account.payouts.map((payout) => (
                  <tr key={payout.id} className="border-b border-border last:border-0">
                    <td className={`${td} text-right font-mono`}>{formatAmount(payout.amount, 4)}</td>
                    <td className={td}>{payout.status === "sent" ? "Sent" : "Queued"}</td>
                    <td className={`${td} font-mono text-muted`} title={payout.txid ?? undefined}>
                      {payout.txid ? shorten(payout.txid) : "–"}
                    </td>
                    <td className={`${td} text-right text-muted`}>{formatAge(payout.createdAt)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>

        <Card title="Block rewards">
          {account.credits.length === 0 ? (
            <Empty>No rewards yet.</Empty>
          ) : (
            <table className="w-full">
              <thead>
                <tr className="border-b border-border">
                  <th className={th}>Height</th>
                  <th className={th}>Status</th>
                  <th className={`${th} text-right`}>Your share</th>
                  <th className={`${th} text-right`}>Found</th>
                </tr>
              </thead>
              <tbody>
                {account.credits.map((credit, index) => (
                  <tr key={index} className="border-b border-border last:border-0">
                    <td className={`${td} font-mono`}>{credit.height}</td>
                    <td className={td}>
                      <BlockStatus status={credit.status} />
                    </td>
                    <td className={`${td} text-right font-mono`}>{formatAmount(credit.amount, 4)}</td>
                    <td className={`${td} text-right text-muted`}>{formatAge(credit.foundAt)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </div>
    </>
  );
}
