import type { Metadata } from "next";

import { WalletSearch } from "@/components/miner-search";
import { SavedWallets } from "@/components/saved-wallets";
import { Box } from "@/components/ui";

export const metadata: Metadata = { title: "Wallet" };

export default function WalletPage() {
  return (
    <div className="columns">
      <Box title="YOUR WALLET">
        <WalletSearch />
        <SavedWallets />
        <ul>
          <li>Enter the wallet address you mine with to see your balance, workers and payouts.</li>
          <li>
            Addresses you have looked up are listed here so you find them again. The list is kept in this browser
            only; the pool does not store it.
          </li>
        </ul>
      </Box>
    </div>
  );
}
