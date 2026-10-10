"use client";

import { useActionState } from "react";

import { saveExchange, type SaveState } from "./actions";

export function SettingsForm({ enabled, feePercent }: { enabled: boolean; feePercent: number }) {
  const [state, action, pending] = useActionState<SaveState, FormData>(saveExchange, {});
  return (
    <form action={action} className="flex flex-wrap items-center gap-x-4 gap-y-2 p-1">
      <label>
        <input type="checkbox" name="enabled" defaultChecked={enabled} /> <b>Exchange on</b>
      </label>
      <label>
        <b>Fee %</b>{" "}
        <input name="fee" className="text-input right" size={6} inputMode="decimal" defaultValue={feePercent} required />
      </label>
      <button type="submit" className="button" disabled={pending}>
        Save
      </button>
      {state.error && (
        <span role="alert" className="block orphaned">
          {state.error}
        </span>
      )}
      {state.saved && !pending && <span className="block confirmed">saved</span>}
    </form>
  );
}
