"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

export function MinerSearch() {
  const router = useRouter();
  const [address, setAddress] = useState("");
  return (
    <form
      className="flex gap-2"
      onSubmit={(event) => {
        event.preventDefault();
        const value = address.trim();
        if (value) router.push(`/miner/${encodeURIComponent(value)}`);
      }}
    >
      <input
        value={address}
        onChange={(event) => setAddress(event.target.value)}
        placeholder="Your payout address"
        aria-label="Payout address"
        spellCheck={false}
        className="min-w-0 flex-1 rounded border border-border bg-surface px-3 py-2 font-mono text-sm outline-none focus:border-accent"
      />
      <button type="submit" className="rounded bg-accent px-4 py-2 text-sm font-medium text-white">
        Show stats
      </button>
    </form>
  );
}
