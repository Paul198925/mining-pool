import type { Metadata } from "next";
import { headers } from "next/headers";
import { Suspense } from "react";

import { AutoRefresh } from "@/components/auto-refresh";
import { Box, Empty, Skeleton } from "@/components/ui";
import { getServers, type NodeStatus, type StratumInstance } from "@/lib/admin-queries";
import { COINS } from "@/lib/catalog";
import { formatAge, formatHashrate } from "@/lib/format";
import { meshEnabled, WG_PORT } from "@/lib/mesh";
import { publicUrl } from "@/lib/public-url";

import { removeServer, updateServer } from "./actions";
import { AddServerForm, CopyCommand, RoleChoices } from "./add-server-form";

export const metadata: Metadata = { title: "Servers" };

export default function ServersPage() {
  return (
    <>
      {/* Slow enough to tick the boxes of a server's roles without the page reloading underneath. */}
      <AutoRefresh seconds={60} />
      <Suspense fallback={<Skeleton height={300} />}>
        <Servers />
      </Suspense>
    </>
  );
}

function State({ ok, yes, no }: { ok: boolean; yes: string; no: string }) {
  return <span className={`block ${ok ? "confirmed" : "orphaned"}`}>{ok ? yes : no}</span>;
}

/** What a node is doing right now, in a word or a percentage. */
function syncState(node: NodeStatus): string {
  if (!node.reachable || node.syncProgress === null) return "–";
  if (node.peers === 0) return "no peers";
  if (node.syncProgress > 0.9999) return "yes";
  // While only headers are loaded the block height stays at zero.
  if (node.height === 0 && (node.headers ?? 0) > 0) return "loading headers";
  return `${(node.syncProgress * 100).toFixed(1)}%`;
}

const ENROLLMENT_NOTE = {
  enrolled: "",
  manual: "set up by hand",
  waiting: "waiting for the install command",
  expired: "code expired, remove and add again",
};

function runs(instance: StratumInstance): string {
  const parts = [...(instance.stratum ? ["Stratum"] : []), ...instance.coins.map((symbol) => `${symbol} node`)];
  return parts.join(", ") || "nothing";
}

async function Servers() {
  const { instances, nodes } = await getServers();
  const pool = publicUrl(await headers());
  const choices = COINS.map((coin) => ({ symbol: coin.symbol, name: coin.name, algo: coin.algo }));
  const mesh = meshEnabled();
  return (
    <>
      <Box title="Add a server">
        <AddServerForm coins={choices} />
        <ul>
          <li>
            A server runs the stratum service, coin nodes, or both. One stratum service accepts miners for every
            algorithm and uses the coin nodes of all servers, so a coin needs a node on only one of them.
          </li>
          <li>
            It needs Docker-capable Linux and enough disk for the chosen chains.
            {mesh
              ? ` The servers talk to each other over a private WireGuard network: open UDP port ${WG_PORT} to the internet, and the stratum ports on servers that run stratum.`
              : " The private network is switched off (POOL_WIREGUARD=off), so a stratum service only sees coin nodes on its own server."}
          </li>
        </ul>
      </Box>
      <Box title="Servers">
        {instances.length === 0 ? (
          <Empty>No server has been added yet.</Empty>
        ) : (
          <table className="datagrid">
            <thead>
              <tr>
                <th className="right">Id</th>
                <th>Region</th>
                <th>Host</th>
                <th>Runs</th>
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
                  <td>
                    {instance.enrollment === "manual" ? (
                      "Stratum, set up by hand"
                    ) : (
                      <details>
                        <summary>{runs(instance)}</summary>
                        <form action={updateServer} className="grid gap-2 p-1">
                          <input type="hidden" name="id" value={instance.id} />
                          <RoleChoices coins={choices} stratum={instance.stratum} chosen={instance.coins} />
                          <div>
                            <button type="submit" className="button">
                              Save
                            </button>
                          </div>
                        </form>
                      </details>
                    )}
                  </td>
                  <td>
                    <State ok={instance.live} yes="Running" no="Offline" /> {ENROLLMENT_NOTE[instance.enrollment]}
                    {instance.updatePending && <span className="block pending">update pending</span>}
                    {mesh && instance.enrollment === "enrolled" && !instance.meshed && (
                      <span className="block orphaned">not in the private network</span>
                    )}
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
        <ul>
          <li>
            Click what a server runs to change it. The change is applied when you run this on that server as root;
            the same command also updates its services to the latest version:
          </li>
        </ul>
        <CopyCommand command={`curl -fsSL ${pool}/install.sh | sudo bash -s -- ${pool}`} />
      </Box>
      <Box title="Wallet nodes, as seen by pool-core">
        <table className="datagrid">
          <thead>
            <tr>
              <th>Coin</th>
              <th>Algo</th>
              <th className="right">Port</th>
              <th>Daemon</th>
              <th className="right">Peers</th>
              <th className="right">Height</th>
              <th className="right">Headers</th>
              <th className="right">Synced</th>
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
                <td className="right">{node.peers ?? "–"}</td>
                <td className="right">{node.height ?? "–"}</td>
                <td className="right">{node.headers ?? "–"}</td>
                <td className="right">{syncState(node)}</td>
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
