import type { Metadata } from "next";
import Link from "next/link";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { Box, Empty, Skeleton } from "@/components/ui";
import { getPayouts } from "@/lib/admin-queries";
import { formatAge, formatAmount, shorten } from "@/lib/format";

export const metadata: Metadata = { title: "Payouts" };

const STATUS_LABEL = { pending: "Queued", sending: "Sending", sent: "Sent" };

export default function PayoutsPage() {
  return (
    <>
      <AutoRefresh seconds={30} />
      <Suspense fallback={<Skeleton height={400} />}>
        <Payouts />
      </Suspense>
    </>
  );
}

async function Payouts() {
  const payouts = await getPayouts(200);
  return (
    <Box title="Payouts (open first, then the latest 200)">
      {payouts.length === 0 ? (
        <Empty>No payouts yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th className="right">#</th>
              <th>Coin</th>
              <th>Address</th>
              <th className="right">Amount</th>
              <th className="right">Tx fee</th>
              <th>Status</th>
              <th className="right">Batch</th>
              <th>Transaction</th>
              <th className="right">Created</th>
              <th className="right">Sent</th>
            </tr>
          </thead>
          <tbody>
            {payouts.map((payout) => (
              <tr key={payout.id}>
                <td className="right">{payout.id}</td>
                <td>
                  <b>{payout.symbol}</b>
                </td>
                <td>
                  <Link href={`/wallet/${payout.address}`} className="mono">
                    {shorten(payout.address)}
                  </Link>
                </td>
                <td className="right">{formatAmount(payout.amount)}</td>
                <td className="right">{payout.networkFee === null ? "–" : formatAmount(payout.networkFee)}</td>
                <td>{STATUS_LABEL[payout.status]}</td>
                <td className="right">{payout.batchId ?? "–"}</td>
                <td className="mono" title={payout.txid ?? undefined}>
                  {payout.txid ? shorten(payout.txid) : "–"}
                </td>
                <td className="right">{formatAge(payout.createdAt)}</td>
                <td className="right">{payout.sentAt ? formatAge(payout.sentAt) : "–"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Box>
  );
}
