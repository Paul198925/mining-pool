"use client";

import { useActionState } from "react";

import type { Algo } from "@/lib/mesh";

import { saveAlgo, type SaveState } from "./actions";

export function AlgoRow({ algo, coins }: { algo: Algo; coins: string[] }) {
  const [state, action, pending] = useActionState<SaveState, FormData>(saveAlgo, {});
  const form = `algo-${algo.name}`;
  const number = (name: string, value: number, label: string) => (
    <td className="right">
      <input
        form={form}
        name={name}
        className="text-input right"
        size={10}
        inputMode="decimal"
        defaultValue={value}
        aria-label={`${algo.name} ${label}`}
        required
      />
    </td>
  );
  return (
    <tr>
      <td>
        <b>{algo.name}</b>
      </td>
      <td>{coins.join(", ") || "–"}</td>
      {number("port", algo.port, "port")}
      {number("start", algo.startDifficulty, "start difficulty")}
      {number("min", algo.minDifficulty, "minimum difficulty")}
      {number("max", algo.maxDifficulty, "maximum difficulty")}
      {number("target", algo.shareTargetSecs, "seconds per share")}
      <td className="right">
        <form id={form} action={action}>
          <input type="hidden" name="name" value={algo.name} />
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
