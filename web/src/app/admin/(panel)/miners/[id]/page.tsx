import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlockStatus, Box, Empty, Skeleton } from "@/components/ui";
import { getAdminAccount } from "@/lib/admin-queries";
import { formatAge, formatAmount, formatDuration, formatHashrate, formatUsd, shorten, timeToFind, valueUsd } from "@/lib/format";
import { getMiner } from "@/lib/queries";

import { AddressForm } from "./address-form";

export const metadata: Metadata = { title: "Miner" };

export default function MinerPage({ params }: PageProps<"/admin/miners/[id]">) {
  return (
    <>
      <AutoRefresh seconds={60} />
      <Suspense fallback={<Skeleton height={400} />}>
        <Miner params={params} />
      </Suspense>
    </>
  );
}

async function Miner({ params }: Pick<PageProps<"/admin/miners/[id]">, "params">) {
  const { id } = await params;
  const account = await getAdminAccount(id);
  if (!account) {
    notFound();
  }
  // The same numbers the miner sees on their wallet page, for this one account.
  const stats = (await getMiner(account.address)).find((candidate) => candidate.symbol === account.symbol);
  return (
    <>
      <Box title={`Miner: ${account.symbol} account ${account.id}`}>
        <table className="datagrid">
          <tbody>
            <tr>
              <th>Payout address</th>
              <td className="mono">{account.address}</td>
            </tr>
            <tr>
              <th>First seen</th>
              <td>{formatAge(account.createdAt)}</td>
            </tr>
            {stats && (
              <>
                <tr>
                  <th>Hashrate</th>
                  <td>{formatHashrate(stats.hashrate)}</td>
                </tr>
                {stats.mining.map((coin) => (
                  <tr key={coin.symbol}>
                    <th>Mining {coin.symbol}</th>
                    <td>
                      {formatHashrate(coin.hashrate)}, alone a block every{" "}
                      {formatDuration(timeToFind(coin.networkDifficulty, coin.hashrate))}
                    </td>
                  </tr>
                ))}
                <tr>
                  <th>Balance</th>
                  <td>
                    <b>
                      {formatAmount(stats.balance)} {account.symbol}
                    </b>
                    {stats.priceUsd !== null && <> (about {formatUsd(valueUsd(stats.balance, stats.priceUsd))})</>}
                  </td>
                </tr>
                <tr>
                  <th>Immature</th>
                  <td>
                    {formatAmount(stats.immature)} {account.symbol}
                  </td>
                </tr>
                <tr>
                  <th>Paid so far</th>
                  <td>
                    {formatAmount(stats.paid)} {account.symbol}
                  </td>
                </tr>
                {stats.waiting.length > 0 && (
                  <tr>
                    <th>Waiting for exchange</th>
                    <td>{stats.waiting.map((item) => `${formatAmount(item.amount)} ${item.symbol}`).join(", ")}</td>
                  </tr>
                )}
              </>
            )}
            <tr>
              <th>Payouts queued</th>
              <td>{account.queued}</td>
            </tr>
          </tbody>
        </table>
        <ul>
          <li>
            <Link href={`/wallet/${account.address}`}>The miner&apos;s own wallet page</Link> ·{" "}
            <Link href="/admin/miners">Back to all miners</Link>
          </li>
        </ul>
      </Box>

      <Box title="Change payout address">
        <AddressForm id={account.id} symbol={account.symbol} address={account.address} />
        <ul>
          <li>
            The balance, queued payouts and everything this account earns from now on go to the new address. A
            payment the wallet has already been asked to send is not changed.
          </li>
          <li>
            The miner has to mine with the new address from now on. If they keep using the old one, the pool sees a
            new, separate account under the old address.
          </li>
          <li>
            The pool&apos;s wallet checks the address before it pays. If it is not a valid {account.symbol} address,
            the payout stays queued and nothing is lost; correct the address here.
          </li>
        </ul>
      </Box>

      {stats && (
        <>
          <Box title="Workers">
            {stats.workers.length === 0 ? (
              <Empty>No worker yet.</Empty>
            ) : (
              <table className="datagrid">
                <thead>
                  <tr>
                    <th>Worker</th>
                    <th className="right">Hashrate</th>
                    <th className="right">Last share</th>
                  </tr>
                </thead>
                <tbody>
                  {stats.workers.map((worker) => (
                    <tr key={worker.name}>
                      <td>{worker.name || "(default)"}</td>
                      <td className="right">{formatHashrate(worker.hashrate)}</td>
                      <td className="right">{formatAge(worker.lastShareAt)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Box>
          <Box title="Last payouts">
            {stats.payouts.length === 0 ? (
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
                  {stats.payouts.map((payout) => (
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
          <Box title="Last 20 earnings">
            {stats.credits.length === 0 ? (
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
                  {stats.credits.map((credit, index) => (
                    <tr key={index}>
                      <td className="right">
                        {credit.symbol} {credit.height}
                      </td>
                      <td className="right">
                        {formatAmount(credit.amount)} {credit.symbol}
                      </td>
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
        </>
      )}
    </>
  );
}
