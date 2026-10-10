"use client";

import { useActionState } from "react";

import { type AddressState, changePayoutAddress } from "../actions";

export function AddressForm({ id, symbol, address }: { id: string; symbol: string; address: string }) {
  const [state, action, pending] = useActionState<AddressState, FormData>(changePayoutAddress, {});
  return (
    <form action={action} className="grid gap-2 p-1">
      <input type="hidden" name="id" value={id} />
      <div className="generator">
        <label htmlFor="payout-address">{symbol} address</label>
        <input
          id="payout-address"
          name="address"
          className="text-input"
          defaultValue={address}
          spellCheck={false}
          maxLength={100}
          required
        />
      </div>
      <label>
        <input type="checkbox" name="confirm" /> I have checked the new address with the miner. Coins sent to a wrong
        address cannot be brought back.
      </label>
      <div>
        <button type="submit" className="button" disabled={pending}>
          Change payout address
        </button>{" "}
        {state.saved && !pending && <span className="block confirmed">saved</span>}
      </div>
      {state.error && (
        <p role="alert" className="alert">
          {state.error}
        </p>
      )}
    </form>
  );
}
