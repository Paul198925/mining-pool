"use client";

import { useActionState } from "react";

import { formatAge } from "@/lib/format";

import { type SaveState, saveManualPrice } from "./actions";

export type PriceRow = {
  symbol: string;
  /** From the price API. */
  fetched: number | null;
  fetchedAt: Date | null;
  fetchedUsable: boolean;
  /** Set by hand; wins over the fetched one. */
  manual: number | null;
};

const show = (price: number) => Number(price.toPrecision(6)).toString();

export function PriceRowForm({ price }: { price: PriceRow }) {
  const [state, action, pending] = useActionState<SaveState, FormData>(saveManualPrice, {});
  const form = `price-${price.symbol}`;
  const [label, ok] =
    price.manual !== null
      ? ["set by hand", true]
      : price.fetchedUsable
        ? ["from the price API", true]
        : [price.fetched === null ? "no price, not exchanged" : "too old, paused", false];
  return (
    <tr>
      <td>
        <b>{price.symbol}</b>
      </td>
      <td className="right">{price.fetched === null ? "–" : show(price.fetched)}</td>
      <td className="right">{price.fetchedAt ? formatAge(price.fetchedAt) : "–"}</td>
      <td className="right">
        <input
          form={form}
          name="price"
          className="text-input right"
          size={12}
          inputMode="decimal"
          defaultValue={price.manual === null ? "" : show(price.manual)}
          placeholder="none"
          aria-label={`${price.symbol} price set by hand, in US dollars`}
        />
      </td>
      <td>
        <span className={`block ${ok ? "confirmed" : "orphaned"}`}>{label}</span>
      </td>
      <td className="right">
        <form id={form} action={action}>
          <input type="hidden" name="symbol" value={price.symbol} />
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
