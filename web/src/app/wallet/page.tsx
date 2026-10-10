import type { Metadata } from "next";

import { WalletSearch } from "@/components/miner-search";
import { Box } from "@/components/ui";

export const metadata: Metadata = { title: "Wallet" };

export default function WalletPage() {
  return (
    <div className="columns">
      <Box title="YOUR WALLET">
        <WalletSearch />
        <ul>
          <li>Enter the wallet address you mine with to see your balance, workers and payouts.</li>
        </ul>
      </Box>
    </div>
  );
}
