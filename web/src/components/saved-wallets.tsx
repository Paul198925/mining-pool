"use client";

import Link from "next/link";
import { useEffect, useSyncExternalStore } from "react";

// The visitor's own wallet addresses. They are kept in this browser only and never
// sent anywhere except as part of the wallet pages the visitor opens.
const KEY = "pool.wallets";
const CHANGED = "pool-wallets-changed";
const MAX_SAVED = 10;
const EMPTY = "[]";

function read(): string {
  try {
    return window.localStorage.getItem(KEY) ?? EMPTY;
  } catch {
    // Private windows and blocked site data: the list is simply empty.
    return EMPTY;
  }
}

function parse(stored: string): string[] {
  try {
    const value: unknown = JSON.parse(stored);
    return Array.isArray(value)
      ? value.filter((item): item is string => typeof item === "string" && /^[A-Za-z0-9]{20,100}$/.test(item))
      : [];
  } catch {
    return [];
  }
}

function write(addresses: string[]) {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(addresses.slice(0, MAX_SAVED)));
    window.dispatchEvent(new Event(CHANGED));
  } catch {
    // Nothing to do: without storage there is nothing to remember.
  }
}

function subscribe(onChange: () => void): () => void {
  // "storage" fires for other tabs, the custom event for this one.
  window.addEventListener("storage", onChange);
  window.addEventListener(CHANGED, onChange);
  return () => {
    window.removeEventListener("storage", onChange);
    window.removeEventListener(CHANGED, onChange);
  };
}

/** Puts an address the visitor has just looked at on top of their list. Renders nothing. */
export function RememberWallet({ address }: { address: string }) {
  useEffect(() => {
    const saved = parse(read());
    if (saved[0] !== address) write([address, ...saved.filter((other) => other !== address)]);
  }, [address]);
  return null;
}

/** The visitor's saved addresses as links, each with a button to forget it. */
export function SavedWallets() {
  const addresses = parse(useSyncExternalStore(subscribe, read, () => EMPTY));
  if (addresses.length === 0) {
    return null;
  }
  return (
    <table className="datagrid">
      <thead>
        <tr>
          <th>Your addresses</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {addresses.map((address) => (
          <tr key={address}>
            <td className="mono">
              <Link href={`/wallet/${address}`}>{address}</Link>
            </td>
            <td className="right">
              <button
                type="button"
                className="button"
                onClick={() => write(addresses.filter((other) => other !== address))}
                aria-label={`Forget ${address}`}
              >
                Forget
              </button>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
