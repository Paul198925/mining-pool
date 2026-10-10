"use client";

import { useActionState, useState } from "react";

import { addServer, type AddServerState } from "./actions";

export type CoinChoice = { symbol: string; name: string; algo: string; available: boolean };

export function AddServerForm({ coins }: { coins: CoinChoice[] }) {
  const [state, action, pending] = useActionState<AddServerState, FormData>(addServer, {});
  const [copied, setCopied] = useState(false);
  return (
    <form action={action} className="grid gap-2 p-1" onSubmit={() => setCopied(false)}>
      <div className="generator">
        <label htmlFor="server-region">Region</label>
        <input id="server-region" name="region" className="text-input" placeholder="Shown to miners, e.g. Europe" required />
        <label htmlFor="server-host">Host</label>
        <input
          id="server-host"
          name="host"
          className="text-input"
          placeholder="Public hostname or IP miners connect to"
          spellCheck={false}
          required
        />
        <span>
          <b>Coins</b>
        </span>
        <div className="flex flex-wrap gap-x-4 gap-y-1">
          {coins.map((coin) => (
            <label key={coin.symbol} title={coin.available ? undefined : "No pool address configured"}>
              <input type="checkbox" name={`coin-${coin.symbol}`} disabled={!coin.available} /> {coin.name} (
              {coin.symbol}, {coin.algo})
            </label>
          ))}
        </div>
      </div>
      <div>
        <button type="submit" className="button" disabled={pending}>
          {pending ? "Creating…" : "Create install command"}
        </button>
      </div>
      {state.error && (
        <p role="alert" className="alert">
          {state.error}
        </p>
      )}
      {state.command && (
        <div className="grid gap-2">
          <p className="notice">
            Run this on the new server as root within {state.expiresInMinutes} minutes. The code works once and is not
            shown again.
          </p>
          <div className="flex items-start gap-2">
            <p className="cmdline flex-1">{state.command}</p>
            <button
              type="button"
              className="button"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(state.command ?? "");
                  setCopied(true);
                } catch {
                  setCopied(false);
                }
              }}
            >
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
        </div>
      )}
    </form>
  );
}
