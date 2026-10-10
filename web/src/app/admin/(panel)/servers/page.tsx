import type { Metadata } from "next";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { Box, Empty, Skeleton } from "@/components/ui";
import { getServers } from "@/lib/admin-queries";
import { COINS, poolAddress } from "@/lib/catalog";
import { formatAge, formatHashrate } from "@/lib/format";

import { removeServer } from "./actions";
import { AddServerForm } from "./add-server-form";

export const metadata: Metadata = { title: "Servers" };

export default function ServersPage() {
  return (
    <>
      <AutoRefresh seconds={15} />
      <Suspense fallback={<Skeleton height={300} />}>
        <Servers />
      </Suspense>
    </>
  );
}

function State({ ok, yes, no }: { ok: boolean; yes: string; no: string }) {
  return <span className={`block ${ok ? "confirmed" : "orphaned"}`}>{ok ? yes : no}</span>;
}

const ENROLLMENT_NOTE = {
  enrolled: "",
  manual: "set up by hand",
  waiting: "waiting for the install command",
  expired: "code expired, remove and add again",
};

async function Servers() {
  const { instances, nodes } = await getServers();
  const choices = COINS.map((coin) => ({
    symbol: coin.symbol,
    name: coin.name,
    algo: coin.algo,
    available: poolAddress(coin.symbol) !== null,
  }));
  return (
    <>
      <Box title="Add a server">
        <AddServerForm coins={choices} />
        <ul>
          <li>
            A server runs a stratum service and a daemon for each chosen coin. It needs Docker-capable Linux, the
            stratum ports open to the internet, and enough disk for the chosen chains.
          </li>
        </ul>
      </Box>
      <Box title="Stratum servers">
        {instances.length === 0 ? (
          <Empty>No server has been added yet.</Empty>
        ) : (
          <table className="datagrid">
            <thead>
              <tr>
                <th className="right">Instance</th>
                <th>Region</th>
                <th>Host</th>
                <th>Coins</th>
                <th>State</th>
                <th className="right">Last seen</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {instances.map((instance) => (
                <tr key={instance.id}>
                  <td className="right">{instance.id}</td>
                  <td>{instance.region}</td>
                  <td className="mono">{instance.host}</td>
                  <td>{instance.coins.join(", ") || "–"}</td>
                  <td>
                    <State ok={instance.live} yes="Running" no="Offline" /> {ENROLLMENT_NOTE[instance.enrollment]}
                  </td>
                  <td className="right">{instance.live ? formatAge(instance.lastSeenAt) : "–"}</td>
                  <td className="right">
                    <form action={removeServer}>
                      <input type="hidden" name="id" value={instance.id} />
                      <button type="submit" className="button">
                        Remove
                      </button>
                    </form>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Box>
      <Box title="Coin daemons, as seen by pool-core">
        <table className="datagrid">
          <thead>
            <tr>
              <th>Coin</th>
              <th>Algo</th>
              <th className="right">Port</th>
              <th>Daemon</th>
              <th className="right">Height</th>
              <th className="right">Last contact</th>
              <th className="right">Miners</th>
              <th className="right">Hashrate</th>
            </tr>
          </thead>
          <tbody>
            {nodes.map((node) => (
              <tr key={node.symbol}>
                <td>
                  <b>{node.symbol}</b>
                </td>
                <td>{node.algo}</td>
                <td className="right">{node.stratumPort ?? "–"}</td>
                <td>
                  <State ok={node.reachable} yes="Reachable" no="No contact" />
                </td>
                <td className="right">{node.height ?? "–"}</td>
                <td className="right">{formatAge(node.statsAt)}</td>
                <td className="right">{node.miners}</td>
                <td className="right">{formatHashrate(node.hashrate)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Box>
    </>
  );
}
