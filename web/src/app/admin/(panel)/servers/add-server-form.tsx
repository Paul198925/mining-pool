"use client";

import { useActionState, useState } from "react";

import { addServer, type AddServerState, updateServer, type UpdateServerState } from "./actions";

export type CoinChoice = { symbol: string; name: string; algo: string };
export type Roles = { stratum: boolean; coins: string[]; wallets: string[]; core: boolean };

/** The checkboxes that say what a server runs; shared by the add and the edit form. */
export function RoleChoices({ coins, roles }: { coins: CoinChoice[]; roles: Roles }) {
  return (
    <div className="grid gap-1">
      <label title="Accepts miners for every algorithm, using the coin nodes of all servers">
        <input type="checkbox" name="stratum" defaultChecked={roles.stratum} /> <b>Stratum</b> (all algos)
      </label>
      {coins.map((coin) => (
        <div key={coin.symbol} className="flex flex-wrap gap-x-4">
          <span>
            <b>{coin.name}</b> ({coin.symbol}, {coin.algo}):
          </span>
          <label title={`Runs a ${coin.name} node without a wallet, for stratum services to mine on`}>
            <input type="checkbox" name={`coin-${coin.symbol}`} defaultChecked={roles.coins.includes(coin.symbol)} /> node
          </label>
          <label title={`Runs the ${coin.name} node that holds the pool's wallet. One server per coin.`}>
            <input type="checkbox" name={`wallet-${coin.symbol}`} defaultChecked={roles.wallets.includes(coin.symbol)} />{" "}
            node with pool wallet
          </label>
        </div>
      ))}
      <label title="The machine pool-core runs on. It only joins the private network, so pool-core can reach the wallets.">
        <input type="checkbox" name="core" defaultChecked={roles.core} /> <b>Pool host</b> (the machine pool-core runs on)
      </label>
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
        <RoleChoices coins={coins} roles={{ stratum: true, coins: [], wallets: [], core: false }} />
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

export function EditServerForm({ id, coins, roles }: { id: number; coins: CoinChoice[]; roles: Roles }) {
  const [state, action, pending] = useActionState<UpdateServerState, FormData>(updateServer, {});
  return (
    <form action={action} className="grid gap-2 p-1">
      <input type="hidden" name="id" value={id} />
      <RoleChoices coins={coins} roles={roles} />
      {state.error && (
        <p role="alert" className="alert">
          {state.error}
        </p>
      )}
      {state.needsConfirmation && (
        <label>
          <input type="checkbox" name="confirm-drop" /> Yes, take the wallet off this server
        </label>
      )}
      <div>
        <button type="submit" className="button" disabled={pending}>
          Save
        </button>{" "}
        {state.saved && !pending && <span className="block confirmed">saved</span>}
      </div>
    </form>
  );
}
