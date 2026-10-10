import { db } from "@/lib/db";
import { authenticateNode, badRequest, unauthorized } from "@/lib/node-auth";

const MAX_BYTES = 600_000;
const MAX_SERVICES = 30;
const MAX_LOG_LINES = 150;
const MAX_LINE_CHARS = 400;

export type ServiceReport = {
  name: string;
  /** Docker's words for it, e.g. "running" and "Up 3 hours". */
  state: string;
  status: string;
  image: string;
  cpu: string | null;
  memory: string | null;
  /** Size of the service's data volume. */
  dataKb: number | null;
  logs: string[];
};

export type ServerReport = {
  system: Record<string, string>;
  services: ServiceReport[];
};

/** Terminal colours and other control characters have no place in a web page. */
function clean(line: string): string {
  return line
    .replace(/\u001b\[[0-9;]*[A-Za-z]/g, "")
    .replace(/[\u0000-\u0008\u000b-\u001f\u007f]/g, "")
    .slice(0, MAX_LINE_CHARS);
}

/**
 * Reads the report format of the installer's report.sh: the first line is a marker,
 * and every later line that starts with it opens a section ("system", "service <name>",
 * "logs <name>"). Sections hold `key=value` lines, except logs.
 */
function parse(text: string): ServerReport | null {
  const lines = text.split("\n");
  const mark = lines[0]?.trim();
  if (!mark || !/^@@[0-9a-f]{24}$/.test(mark)) return null;
  const report: ServerReport = { system: {}, services: [] };
  let section: { kind: string; name: string } | null = null;
  const service = (name: string) => {
    let found = report.services.find((candidate) => candidate.name === name);
    if (!found && report.services.length < MAX_SERVICES) {
      found = { name, state: "", status: "", image: "", cpu: null, memory: null, dataKb: null, logs: [] };
      report.services.push(found);
    }
    return found;
  };
  for (const raw of lines.slice(1)) {
    if (raw.startsWith(`${mark} `)) {
      const [kind, ...name] = raw.slice(mark.length + 1).trim().split(" ");
      section = { kind, name: clean(name.join(" ")).slice(0, 60) };
      if (section.kind !== "system") service(section.name);
      continue;
    }
    if (!section) continue;
    if (section.kind === "logs") {
      const target = service(section.name);
      const line = clean(raw);
      // Daemons pad their output with empty lines; they only push the real ones out of view.
      if (target && line.trim() !== "" && target.logs.length < MAX_LOG_LINES) target.logs.push(line);
      continue;
    }
    const split = raw.indexOf("=");
    if (split < 1) continue;
    const key = raw.slice(0, split);
    const value = clean(raw.slice(split + 1)).slice(0, 200);
    if (section.kind === "system") {
      if (/^[a-z_]{1,30}$/.test(key) && Object.keys(report.system).length < 30) report.system[key] = value;
    } else if (section.kind === "service") {
      const target = service(section.name);
      if (!target) continue;
      if (key === "state") target.state = value;
      else if (key === "status") target.status = value;
      else if (key === "image") target.image = value;
      else if (key === "cpu") target.cpu = value;
      else if (key === "mem") target.memory = value;
      else if (key === "data_kb") target.dataKb = /^\d{1,15}$/.test(value) ? Number(value) : null;
    }
  }
  return report;
}

/** A server's own account of how it is doing, for its page in the admin area. */
export async function POST(request: Request) {
  const server = await authenticateNode(request);
  if (!server) return unauthorized();
  const text = await request.text();
  if (text.length > MAX_BYTES) return badRequest("report too large");
  const report = parse(text);
  if (!report) return badRequest("unreadable report");
  await db().query(
    `INSERT INTO server_reports (server_id, reported_at, data) VALUES ($1, now(), $2)
     ON CONFLICT (server_id) DO UPDATE SET reported_at = now(), data = EXCLUDED.data`,
    [server.id, JSON.stringify(report)],
  );
  return Response.json({ ok: true });
}
