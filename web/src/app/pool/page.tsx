import type { Metadata } from "next";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { BlocksTable } from "@/components/blocks-table";
import { CoinsTable } from "@/components/coins-table";
import { Box, Skeleton } from "@/components/ui";
import { formatUsd } from "@/lib/format";
import { type Coin, getBlocks, getCoins, getExchangeSettings } from "@/lib/queries";

export const metadata: Metadata = { title: "Pool" };

export default function PoolPage() {
  return (
    <div className="columns">
      <AutoRefresh />
      <div className="column">
        <Suspense fallback={<Skeleton height={200} />}>
          <Coins />
        </Suspense>
      </div>
      <div className="column">
        <Suspense fallback={<Skeleton height={500} />}>
          <Blocks />
        </Suspense>
      </div>
    </div>
  );
}

async function Coins() {
  const [coins, exchange] = await Promise.all([getCoins(), getExchangeSettings()]);
  const priced = coins.filter((coin): coin is Coin & { priceUsd: number } => coin.priceUsd !== null);
  return (
    <>
      <Box title="Pool Status">
        <CoinsTable coins={coins} />
      </Box>
      {exchange.enabled && priced.length >= 2 && <Rates coins={priced} feePercent={exchange.feePercent} />}
    </>
  );
}

/** A rate with about four significant digits, whatever its size. */
function formatRate(rate: number): string {
  if (rate >= 1000) return Math.round(rate).toLocaleString("en-US");
  return Number(rate.toPrecision(4)).toString();
}

/** What one coin is worth in each of the others, at the prices the internal exchange uses. */
function Rates({ coins, feePercent }: { coins: (Coin & { priceUsd: number })[]; feePercent: number }) {
  return (
    <Box title="Exchange rates">
      <table className="datagrid">
        <thead>
          <tr>
            <th>1 coin</th>
            <th className="right">US dollars</th>
            {coins.map((coin) => (
              <th key={coin.symbol} className="right">
                in {coin.symbol}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {coins.map((from) => (
            <tr key={from.symbol}>
              <td>
                <b>{from.symbol}</b>
              </td>
              <td className="right">{formatUsd(from.priceUsd)}</td>
              {coins.map((to) => (
                <td key={to.symbol} className="right">
                  {from.symbol === to.symbol ? "–" : formatRate(from.priceUsd / to.priceUsd)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <ul>
        <li>
          Mine one coin and be paid in another: use your address of the coin you want and add{" "}
          <b>pay=&lt;SYMBOL&gt;</b> to the password. The pool swaps between its miners at these market prices
          {feePercent > 0 ? `, for a fee of ${feePercent}%` : ""}. It sells nothing on an exchange, so a swap waits
          until other miners want your coin.
        </li>
      </ul>
    </Box>
  );
}

async function Blocks() {
  const blocks = await getBlocks(50);
  return (
    <Box title="Last 50 Blocks">
      <BlocksTable blocks={blocks} />
    </Box>
  );
}
