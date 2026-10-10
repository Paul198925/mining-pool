import type { NextRequest } from "next/server";

import { unitsToDecimal } from "@/lib/format";
import { getMiner, isPlausibleAddress } from "@/lib/queries";

export async function GET(_request: NextRequest, context: RouteContext<"/api/miners/[address]">) {
  const { address } = await context.params;
  const accounts = isPlausibleAddress(address) ? await getMiner(address) : [];
  if (accounts.length === 0) {
    return Response.json({ error: "unknown miner" }, { status: 404 });
  }
  return Response.json({
    address,
    accounts: accounts.map((account) => ({
      symbol: account.symbol,
      hashrate: account.hashrate,
      balance: unitsToDecimal(account.balance),
      immature: unitsToDecimal(account.immature),
      paid: unitsToDecimal(account.paid),
      workers: account.workers,
      payouts: account.payouts.map((payout) => ({
        amount: unitsToDecimal(payout.amount),
        status: payout.status,
        txid: payout.txid,
        createdAt: payout.createdAt,
      })),
      credits: account.credits.map((credit) => ({ ...credit, amount: unitsToDecimal(credit.amount) })),
      history: account.history,
    })),
  });
}
