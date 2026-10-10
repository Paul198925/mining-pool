import type { Metadata } from "next";
import { connection } from "next/server";
import { Suspense } from "react";

import { Box, Skeleton } from "@/components/ui";
import { requireAdmin } from "@/lib/admin-auth";
import { COINS } from "@/lib/catalog";
import { getAlgos } from "@/lib/mesh";

import { AlgoRow } from "./algo-row";

export const metadata: Metadata = { title: "Stratum" };

export default function StratumPage() {
  return (
    <Suspense fallback={<Skeleton height={200} />}>
      <Algos />
    </Suspense>
  );
}

async function Algos() {
  await connection();
  await requireAdmin();
  const algos = await getAlgos();
  return (
    <Box title="Stratum ports">
      <table className="datagrid">
        <thead>
          <tr>
            <th>Algo</th>
            <th>Coins</th>
            <th className="right">Port</th>
            <th className="right">Start difficulty</th>
            <th className="right">Minimum</th>
            <th className="right">Maximum</th>
            <th className="right">Seconds per share</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {algos.map((algo) => (
            <AlgoRow
              key={algo.name}
              algo={algo}
              coins={COINS.filter((coin) => coin.algo === algo.name).map((coin) => coin.symbol)}
            />
          ))}
        </tbody>
      </table>
      <ul>
        <li>
          Every stratum server offers every algorithm on its own port. A miner starts at the start difficulty; the
          server then adjusts it between minimum and maximum so that a share arrives about every “seconds per share”.
        </li>
        <li>
          Difficulty changes reach the stratum servers by themselves within about a minute; they restart to apply
          them, so connected miners reconnect once.
        </li>
        <li>
          After changing a <b>port</b>, run the update command from the Servers page on every stratum server, so its
          firewall opens the new port, and tell your miners.
        </li>
      </ul>
    </Box>
  );
}
