import type { Metadata } from "next";
import { connection } from "next/server";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { Box, Empty, Skeleton } from "@/components/ui";
import { requireAdmin } from "@/lib/admin-auth";
import { db } from "@/lib/db";
import { formatAge, formatAmount, shorten } from "@/lib/format";

import { SettingsForm } from "./settings-form";

export const metadata: Metadata = { title: "Exchange" };

export default function ExchangePage() {
  return (
    <>
      <AutoRefresh seconds={60} />
      <Suspense fallback={<Skeleton height={300} />}>
        <Exchange />
      </Suspense>
    </>
  );
}

async function getExchange() {
  const [settings, prices, waiting, swaps] = await Promise.all([
    db().query("SELECT name, value FROM pool_settings WHERE name IN ('exchange_enabled', 'exchange_fee_percent')"),
    db().query(
      `SELECT symbol, price_usd, price_at, COALESCE(price_at > now() - interval '20 minutes', FALSE) AS usable
       FROM coins WHERE enabled ORDER BY symbol`,
    ),
    db().query(
      `SELECT f.symbol AS from_symbol, t.symbol AS to_symbol, count(*) AS accounts, sum(b.amount) AS amount
       FROM account_balances b
       JOIN accounts a ON a.id = b.account_id
       JOIN coins f ON f.id = b.coin_id
       JOIN coins t ON t.id = a.payout_coin_id
       WHERE b.coin_id <> a.payout_coin_id AND b.amount > 0
       GROUP BY f.symbol, t.symbol ORDER BY f.symbol, t.symbol`,
    ),
    db().query(
      `SELECT e.id, a.payout_address, f.symbol AS from_symbol, t.symbol AS to_symbol,
              e.from_amount, e.to_amount, e.fee_amount, e.created_at
       FROM exchanges e
       JOIN accounts a ON a.id = e.account_id
       JOIN coins f ON f.id = e.from_coin_id
       JOIN coins t ON t.id = e.to_coin_id
       ORDER BY e.id DESC LIMIT 50`,
    ),
  ]);
  const setting = (name: string) => settings.rows.find((row) => row.name === name)?.value;
  return {
    enabled: setting("exchange_enabled") === "true",
    feePercent: Number(setting("exchange_fee_percent") ?? 1),
    prices: prices.rows,
    waiting: waiting.rows,
    swaps: swaps.rows,
  };
}

async function Exchange() {
  await connection();
  await requireAdmin();
  const { enabled, feePercent, prices, waiting, swaps } = await getExchange();
  return (
    <>
      <Box title="Internal exchange">
        <SettingsForm enabled={enabled} feePercent={feePercent} />
        <ul>
          <li>
            Miners who are paid in another coin than they mine swap with each other every five minutes, at the
            prices below. The pool sells nothing on an exchange: a coin is handed out only as far as other miners
            hand it in during the same round, plus the fee income the pool holds in it. What does not fit waits.
          </li>
          <li>The fee is taken from what the miner receives and goes to the pool&apos;s income in that coin.</li>
        </ul>
      </Box>
      <Box title="Prices">
        <table className="datagrid">
          <thead>
            <tr>
              <th>Coin</th>
              <th className="right">US dollars</th>
              <th className="right">Fetched</th>
              <th>State</th>
            </tr>
          </thead>
          <tbody>
            {prices.map((price) => (
              <tr key={price.symbol}>
                <td>
                  <b>{price.symbol}</b>
                </td>
                <td className="right">{price.price_usd === null ? "–" : Number(price.price_usd).toPrecision(6)}</td>
                <td className="right">{price.price_at ? formatAge(price.price_at) : "–"}</td>
                <td>
                  <span className={`block ${price.usable ? "confirmed" : "orphaned"}`}>
                    {price.usable ? "in use" : price.price_usd === null ? "no price, not exchanged" : "too old, paused"}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <ul>
          <li>
            Prices come from a public price API. One older than 20 minutes is not used, and a price that jumps by
            more than 15 % has to repeat three times before it counts.
          </li>
        </ul>
      </Box>
      <Box title="Waiting to be swapped">
        {waiting.length === 0 ? (
          <Empty>Nothing is waiting.</Empty>
        ) : (
          <table className="datagrid">
            <thead>
              <tr>
                <th>From</th>
                <th>Into</th>
                <th className="right">Accounts</th>
                <th className="right">Amount</th>
              </tr>
            </thead>
            <tbody>
              {waiting.map((row) => (
                <tr key={`${row.from_symbol}-${row.to_symbol}`}>
                  <td>{row.from_symbol}</td>
                  <td>{row.to_symbol}</td>
                  <td className="right">{row.accounts}</td>
                  <td className="right">
                    {formatAmount(row.amount)} {row.from_symbol}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Box>
      <Box title="Last 50 swaps">
        {swaps.length === 0 ? (
          <Empty>No swap yet.</Empty>
        ) : (
          <table className="datagrid">
            <thead>
              <tr>
                <th>Time</th>
                <th>Miner</th>
                <th className="right">Gave</th>
                <th className="right">Got</th>
                <th className="right">Fee</th>
              </tr>
            </thead>
            <tbody>
              {swaps.map((swap) => (
                <tr key={swap.id}>
                  <td>{formatAge(swap.created_at)}</td>
                  <td className="mono" title={swap.payout_address}>
                    {shorten(swap.payout_address)}
                  </td>
                  <td className="right">
                    {formatAmount(swap.from_amount)} {swap.from_symbol}
                  </td>
                  <td className="right">
                    {formatAmount(swap.to_amount)} {swap.to_symbol}
                  </td>
                  <td className="right">
                    {formatAmount(swap.fee_amount)} {swap.to_symbol}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Box>
    </>
  );
}
