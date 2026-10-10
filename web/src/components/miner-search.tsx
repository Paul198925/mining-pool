"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

export function WalletSearch({ initial = "" }: { initial?: string }) {
  const router = useRouter();
  const [address, setAddress] = useState(initial);
  return (
    <form
      className="flex items-center gap-2 p-1"
      onSubmit={(event) => {
        event.preventDefault();
        const value = address.trim();
        if (value) router.push(`/wallet/${encodeURIComponent(value)}`);
      }}
    >
      <label htmlFor="wallet-address">
        <b>Wallet:</b>
      </label>
      <input
        id="wallet-address"
        value={address}
        onChange={(event) => setAddress(event.target.value)}
        placeholder="Your wallet address"
        spellCheck={false}
        className="text-input"
      />
      <button type="submit" className="button">
        Submit
      </button>
    </form>
  );
}
