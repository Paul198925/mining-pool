"use client";

import { useActionState } from "react";

import { formatAge } from "@/lib/format";

import { saveCoin, type SaveState } from "./actions";

export type AdminCoin = {
  symbol: string;
  name: string;
  algo: string;
  enabled: boolean;
  payoutsEnabled: boolean;
  poolFeePercent: number | null;
  soloFeePercent: number | null;
  /** In coins, not base units. */
  minPayout: number | null;
  hasWallet: boolean;
  /** When the newest copy of the wallet's backup reached the pool's database. */
  backupAt: Date | null;
  /** Regions of the servers that run a node for this coin. */
  nodes: string[];
};

export function CoinRow({ coin }: { coin: AdminCoin }) {
  const [state, action, pending] = useActionState<SaveState, FormData>(saveCoin, {});
  const form = `coin-${coin.symbol}`;
  const number = (name: string, value: number | null, label: string) => (
    <td className="right">
      <input
        form={form}
        name={name}
        className="text-input right"
        size={8}
        inputMode="decimal"
        defaultValue={value ?? ""}
        aria-label={`${coin.symbol} ${label}`}
        required
      />
    </td>
  );
  return (
    <tr>
      <td>
        <b>{coin.symbol}</b> {coin.name}
      </td>
      <td>{coin.algo}</td>
      <td>
        <input form={form} type="checkbox" name="enabled" defaultChecked={coin.enabled} aria-label={`${coin.symbol} mining on`} />
      </td>
      <td>
        <input
          form={form}
          type="checkbox"
          name="payouts"
          defaultChecked={coin.payoutsEnabled}
          aria-label={`${coin.symbol} payouts on`}
        />
      </td>
      {number("poolFee", coin.poolFeePercent, "pool fee percent")}
      {number("soloFee", coin.soloFeePercent, "solo fee percent")}
      {number("minPayout", coin.minPayout, "minimum payout")}
      <td>
        <span className={`block ${coin.hasWallet ? "confirmed" : "orphaned"}`}>{coin.hasWallet ? "connected" : "missing"}</span>
      </td>
      <td>
        {coin.backupAt ? (
          <a href={`/admin/wallet-backup/${coin.symbol}`} title="Download the newest copy (the encrypted wallet file)">
            {formatAge(coin.backupAt)}
          </a>
        ) : (
          <span className="block orphaned">none</span>
        )}
      </td>
      <td>{coin.nodes.join(", ") || <span className="block orphaned">none</span>}</td>
      <td className="right">
        <form id={form} action={action}>
          <input type="hidden" name="symbol" value={coin.symbol} />
          {state.error && (
            <span role="alert" className="block orphaned">
              {state.error}
            </span>
          )}
          {state.saved && !pending && <span className="block confirmed">saved</span>}{" "}
          <button type="submit" className="button" disabled={pending}>
            Save
          </button>
        </form>
      </td>
    </tr>
  );
}
