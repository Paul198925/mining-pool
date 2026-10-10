import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { Suspense } from "react";

import type { ServerReport } from "@/app/api/node/report/route";
import { AutoRefresh } from "@/components/auto-refresh";
import { Box, Empty, Skeleton } from "@/components/ui";
import { getServerReport } from "@/lib/admin-queries";
import { formatAge } from "@/lib/format";

export const metadata: Metadata = { title: "Server" };

export default function ServerPage({ params }: PageProps<"/admin/servers/[id]">) {
  return (
    <>
      <AutoRefresh seconds={30} />
      <Suspense fallback={<Skeleton height={400} />}>
        <Server params={params} />
      </Suspense>
    </>
  );
}

function gigabytes(kilobytes: number): string {
  return `${(kilobytes / 1024 / 1024).toFixed(1)} GB`;
}

function duration(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  return days > 0 ? `${days} d ${hours} h` : `${hours} h ${Math.floor((seconds % 3600) / 60)} min`;
}

/** The numbers a server sent about itself, as rows of a small table. */
function systemRows(system: ServerReport["system"]): [string, string][] {
  const number = (key: string) => (/^\d+$/.test(system[key] ?? "") ? Number(system[key]) : null);
  const rows: [string, string][] = [];
  if (system.load) rows.push(["Load (1, 5, 15 min)", `${system.load}${system.cpus ? ` on ${system.cpus} CPUs` : ""}`]);
  const [memTotal, memFree] = [number("mem_total_kb"), number("mem_available_kb")];
  if (memTotal !== null && memFree !== null) {
    rows.push(["Memory", `${gigabytes(memTotal - memFree)} of ${gigabytes(memTotal)} in use`]);
  }
  const [diskTotal, diskUsed] = [number("disk_total_kb"), number("disk_used_kb")];
  if (diskTotal !== null && diskUsed !== null) {
    rows.push(["Disk", `${gigabytes(diskUsed)} of ${gigabytes(diskTotal)} in use (${Math.round((diskUsed / diskTotal) * 100)} %)`]);
  }
  const uptime = number("uptime_secs");
  if (uptime !== null) rows.push(["Up since boot", duration(uptime)]);
  if (system.wg_peers !== undefined) {
    rows.push(["Private network", `${system.wg_peers_recent ?? "0"} of ${system.wg_peers} other servers heard from in the last 3 minutes`]);
  }
  return rows;
}

async function Server({ params }: Pick<PageProps<"/admin/servers/[id]">, "params">) {
  const { id } = await params;
  const found = /^\d{1,3}$/.test(id) ? await getServerReport(Number(id)) : null;
  if (!found) {
    notFound();
  }
  const { server, report, reportedAt, fresh } = found;
  return (
    <>
      <Box title={`Server ${server.id}: ${server.region}`}>
        <table className="datagrid">
          <tbody>
            <tr>
              <th>Host</th>
              <td className="mono">{server.host}</td>
            </tr>
            <tr>
              <th>Report</th>
              <td>
                {reportedAt ? (
                  <>
                    <span className={`block ${fresh ? "confirmed" : "orphaned"}`}>{fresh ? "current" : "outdated"}</span>{" "}
                    {formatAge(reportedAt)}
                  </>
                ) : (
                  "none yet"
                )}
              </td>
            </tr>
            {report && systemRows(report.system).map(([label, value]) => (
              <tr key={label}>
                <th>{label}</th>
                <td>{value}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <ul>
          <li>
            The server sends this itself, once a minute; the pool never logs in to it. A server set up before this
            page existed starts reporting after the update command has been run on it once.
          </li>
          <li>
            <Link href="/admin/servers">Back to all servers</Link>
          </li>
        </ul>
      </Box>
      <Box title="Services">
        {!report || report.services.length === 0 ? (
          <Empty>No service is running on this server.</Empty>
        ) : (
          <table className="datagrid">
            <thead>
              <tr>
                <th>Service</th>
                <th>State</th>
                <th>Since</th>
                <th className="right">CPU</th>
                <th className="right">Memory</th>
                <th
                  className="right"
                  title="Size of the service's data folder. For a coin node that is its part of the blockchain (nodes keep about 4 GB of blocks), the chain state and, on a wallet node, the wallet and its copies."
                >
                  Data on disk
                </th>
                <th>Image</th>
              </tr>
            </thead>
            <tbody>
              {report.services.map((service) => (
                <tr key={service.name}>
                  <td>
                    <a href={`#log-${service.name}`}>
                      <b>{service.name}</b>
                    </a>
                  </td>
                  <td>
                    <span className={`block ${service.state === "running" ? "confirmed" : "orphaned"}`}>
                      {service.state || "unknown"}
                    </span>
                  </td>
                  <td>{service.status}</td>
                  <td className="right">{service.cpu ?? "–"}</td>
                  <td className="right">{service.memory ?? "–"}</td>
                  <td className="right">{service.dataKb === null ? "–" : gigabytes(service.dataKb)}</td>
                  <td className="mono">{service.image}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Box>
      {report?.services.map((service) => (
        <Box key={service.name} title={`Log: ${service.name} (last ${service.logs.length} lines)`}>
          {/* The wrapper starts scrolled to the end, where the newest lines are. */}
          <div id={`log-${service.name}`} className="logview">
            <pre>{service.logs.length === 0 ? "(no output)" : service.logs.join("\n")}</pre>
          </div>
        </Box>
      ))}
    </>
  );
}
