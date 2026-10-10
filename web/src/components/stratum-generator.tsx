"use client";

import { useState } from "react";

export type StratumCoin = { symbol: string; name: string; algo: string; port: number };

type Mode = "pool" | "solo" | "party";

/** Lets a miner pick algo, coin and reward mode and builds the matching miner arguments. */
export function StratumGenerator({ coins, host }: { coins: StratumCoin[]; host: string }) {
  const algos = [...new Set(coins.map((coin) => coin.algo))].sort();
  const [algo, setAlgo] = useState(algos[0]);
  const [symbol, setSymbol] = useState(coins.find((coin) => coin.algo === algos[0])?.symbol);
  const [mode, setMode] = useState<Mode>("pool");
  const [party, setParty] = useState("");
  const [address, setAddress] = useState("");
  const [worker, setWorker] = useState("");
  const [copied, setCopied] = useState(false);

  const choices = coins.filter((coin) => coin.algo === algo);
  const coin = choices.find((choice) => choice.symbol === symbol) ?? choices[0];
  if (!coin) {
    return null;
  }

  const user = (address.trim() || `<${coin.symbol}_ADDRESS>`) + (worker.trim() ? `.${worker.trim()}` : "");
  const options = [`c=${coin.symbol}`];
  if (mode === "solo") options.push("m=solo");
  if (mode === "party") options.push(`m=party.${party.trim() || "<NAME>"}`);
  const command = `-a ${coin.algo} -o stratum+tcp://${host}:${coin.port} -u ${user} -p ${options.join(",")}`;

  // Any edit makes a previous "Copied" stale.
  const edit = <T,>(set: (value: T) => void) => (value: T) => {
    set(value);
    setCopied(false);
  };

  return (
    <div>
      <div className="generator">
        <label htmlFor="gen-algo">Algo</label>
        <select
          id="gen-algo"
          value={algo}
          onChange={(event) => {
            edit(setAlgo)(event.target.value);
            setSymbol(coins.find((candidate) => candidate.algo === event.target.value)?.symbol);
          }}
        >
          {algos.map((name) => (
            <option key={name}>{name}</option>
          ))}
        </select>

        <label htmlFor="gen-coin">Coin</label>
        <select id="gen-coin" value={coin.symbol} onChange={(event) => edit(setSymbol)(event.target.value)}>
          {choices.map((choice) => (
            <option key={choice.symbol} value={choice.symbol}>
              {choice.name} ({choice.symbol})
            </option>
          ))}
        </select>

        <label htmlFor="gen-mode">Mode</label>
        <select id="gen-mode" value={mode} onChange={(event) => edit(setMode)(event.target.value as Mode)}>
          <option value="pool">Pool (PPLNS)</option>
          <option value="solo">Solo</option>
          <option value="party">Party</option>
        </select>

        {mode === "party" && (
          <>
            <label htmlFor="gen-party">Party name</label>
            <input
              id="gen-party"
              className="text-input"
              value={party}
              maxLength={64}
              placeholder="Shared with your group"
              spellCheck={false}
              onChange={(event) => edit(setParty)(event.target.value.replace(/[,\s]/g, ""))}
            />
          </>
        )}

        <label htmlFor="gen-address">Wallet</label>
        <input
          id="gen-address"
          className="text-input"
          value={address}
          placeholder={`Your ${coin.symbol} address`}
          spellCheck={false}
          onChange={(event) => edit(setAddress)(event.target.value.replace(/[^A-Za-z0-9]/g, ""))}
        />

        <label htmlFor="gen-worker">Worker</label>
        <input
          id="gen-worker"
          className="text-input"
          value={worker}
          maxLength={64}
          placeholder="Optional, e.g. rig1"
          spellCheck={false}
          onChange={(event) => edit(setWorker)(event.target.value.replace(/[.,\s]/g, ""))}
        />
      </div>

      <div className="mt-2 flex items-start gap-2">
        <p className="cmdline flex-1" aria-live="polite">
          {command}
        </p>
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
    </div>
  );
}
