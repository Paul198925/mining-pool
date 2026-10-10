"use client";

import { useActionState, useState } from "react";

import { addServer, type AddServerState } from "./actions";

export type CoinChoice = { symbol: string; name: string; algo: string };

/** The checkboxes that say what a server runs; shared by the add and the edit form. */
export function RoleChoices({ coins, stratum, chosen }: { coins: CoinChoice[]; stratum: boolean; chosen: string[] }) {
  return (
    <div className="flex flex-wrap gap-x-4 gap-y-1">
      <label title="Accepts miners for every algorithm, using the coin nodes of all servers">
        <input type="checkbox" name="stratum" defaultChecked={stratum} /> <b>Stratum</b> (all algos)
      </label>
      {coins.map((coin) => (
        <label key={coin.symbol} title={`Runs a ${coin.name} node on this server`}>
          <input type="checkbox" name={`coin-${coin.symbol}`} defaultChecked={chosen.includes(coin.symbol)} />{" "}
          {coin.name} node ({coin.symbol}, {coin.algo})
        </label>
      ))}
    </div>
  );
}

export function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex items-start gap-2">
      <p className="cmdline flex-1">{command}</p>
      <button
        type="button"
        className="button"
        onClick={async () => {
          try {
            await navigator.clipboard.writeText(command);
            setCopied(true);
          } catch {
            setCopied(false);
          }
        }}
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

export function AddServerForm({ coins }: { coins: CoinChoice[] }) {
  const [state, action, pending] = useActionState<AddServerState, FormData>(addServer, {});
  return (
    <form action={action} className="grid gap-2 p-1">
      <div className="generator">
        <label htmlFor="server-region">Region</label>
        <input id="server-region" name="region" className="text-input" placeholder="Shown to miners, e.g. Europe" required />
        <label htmlFor="server-host">Host</label>
        <input
          id="server-host"
          name="host"
          className="text-input"
          placeholder="Public hostname or IP of the server"
          spellCheck={false}
          required
        />
        <span>
          <b>Runs</b>
        </span>
        <RoleChoices coins={coins} stratum chosen={[]} />
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
          {/* Keyed so a new command starts with a fresh "Copy" button. */}
          <CopyCommand key={state.command} command={state.command} />
        </div>
      )}
    </form>
  );
}
