"use server";

import { headers } from "next/headers";
import { revalidatePath } from "next/cache";

import { requireAdmin } from "@/lib/admin-auth";
import { COINS } from "@/lib/catalog";
import { db } from "@/lib/db";
import { newSecret, sha256 } from "@/lib/node-auth";
import { publicUrl } from "@/lib/public-url";

const CODE_VALID_MINUTES = 30;

export type AddServerState = { error?: string; command?: string; expiresInMinutes?: number };
export type UpdateServerState = { error?: string; saved?: boolean; needsConfirmation?: boolean };

type Roles = { stratum: boolean; coins: string[]; wallets: string[]; core: boolean };

/** What the form says the server should run. A wallet node needs no second node of its coin. */
function roles(form: FormData): Roles {
  const chosen = (prefix: string) => COINS.map((coin) => coin.symbol).filter((symbol) => form.get(`${prefix}-${symbol}`) === "on");
  const wallets = chosen("wallet");
  return {
    stratum: form.get("stratum") === "on",
    coins: chosen("coin").filter((symbol) => !wallets.includes(symbol)),
    wallets,
    core: form.get("core") === "on",
  };
}

/** A coin has one pool wallet and the pool one accounting service: says what is already taken. */
async function taken({ wallets, core }: Roles, exceptId: number | null): Promise<string | null> {
  const { rows } = await db().query(
    "SELECT region, wallets, core FROM stratum_instances WHERE id IS DISTINCT FROM $1 AND (wallets && $2::text[] OR (core AND $3))",
    [exceptId, wallets, core],
  );
  for (const row of rows) {
    const symbol = wallets.find((wallet) => row.wallets.includes(wallet));
    if (symbol) return `The ${symbol} wallet is already on server "${row.region}". A coin has exactly one pool wallet.`;
    if (core && row.core) return `Server "${row.region}" is already marked as the pool host.`;
  }
  return null;
}

/** Reserves an instance id for a new server and returns the command that enrolls it. */
export async function addServer(_previous: AddServerState, form: FormData): Promise<AddServerState> {
  await requireAdmin();
  const region = String(form.get("region") ?? "").trim();
  const host = String(form.get("host") ?? "").trim();
  const chosen = roles(form);
  const { stratum, coins, wallets, core } = chosen;
  if (region.length < 1 || region.length > 40) return { error: "Give the server a region name of up to 40 characters." };
  if (!/^[A-Za-z0-9.-]{1,253}$/.test(host)) return { error: "Enter the server's public hostname or IP address." };
  if (!stratum && !core && coins.length + wallets.length === 0) return { error: "Choose at least one thing for the server to do." };
  const conflict = await taken(chosen, null);
  if (conflict) return { error: conflict };

  const code = newSecret();
  // The lowest free instance id; a server that was removed frees its id again.
  // Ids end at 254: the id is also the last byte of the server's private network address.
  const { rows } = await db().query(
    `INSERT INTO stratum_instances (id, region, host, stratum, coins, wallets, core, enroll_code_hash, enroll_expires_at)
     SELECT free.id, $1, $2, $3, $4, $7, $8, $5, now() + make_interval(mins => $6)
     FROM generate_series(1, 254) AS free (id)
     WHERE NOT EXISTS (SELECT 1 FROM stratum_instances used WHERE used.id = free.id)
     ORDER BY free.id
     LIMIT 1
     RETURNING id`,
    [region, host, stratum, coins, sha256(code), CODE_VALID_MINUTES, wallets, core],
  );
  if (rows.length === 0) return { error: "All 254 server slots are taken." };

  const pool = publicUrl(await headers());
  revalidatePath("/admin/servers");
  return {
    command: `curl -fsSL ${pool}/install.sh | sudo bash -s -- ${pool} ${code}`,
    expiresInMinutes: CODE_VALID_MINUTES,
  };
}

/** Changes what a server runs. It takes effect once the update command is run on that server. */
export async function updateServer(_previous: UpdateServerState, form: FormData): Promise<UpdateServerState> {
  await requireAdmin();
  const id = Number(form.get("id"));
  if (!Number.isInteger(id)) return { error: "Unknown server." };
  const chosen = roles(form);
  const { stratum, coins, wallets, core } = chosen;
  const conflict = await taken(chosen, id);
  if (conflict) return { error: conflict };
  const before = await db().query("SELECT wallets, core FROM stratum_instances WHERE id = $1", [id]);
  if (before.rows.length === 0) return { error: "Unknown server." };
  const dropped = (before.rows[0].wallets as string[]).filter((symbol) => !wallets.includes(symbol));
  if (dropped.length > 0 && form.get("confirm-drop") !== "on") {
    return {
      error: `This takes the ${dropped.join(", ")} wallet off the server. The update command then removes the node; its wallet file stays in the Docker volume, but the pool stops using it. Tick the confirmation to go on.`,
      needsConfirmation: true,
    };
  }
  await db().query(
    `UPDATE stratum_instances SET stratum = $2, coins = $3, wallets = $4, core = $5, config_version = config_version + 1
     WHERE id = $1 AND (stratum, coins, wallets, core) IS DISTINCT FROM ($2, $3::text[], $4::text[], $5)`,
    [id, stratum, coins, wallets, core],
  );
  if (wallets.length !== before.rows[0].wallets.length || core !== before.rows[0].core || dropped.length > 0) {
    // Wallet nodes list who may command them, and that just changed.
    await db().query("UPDATE stratum_instances SET config_version = config_version + 1 WHERE id <> $1 AND wallets <> '{}'", [id]);
  }
  revalidatePath("/admin/servers");
  return { saved: true };
}

/** Takes a server out of the pool: its token stops working at once. */
export async function removeServer(form: FormData): Promise<void> {
  await requireAdmin();
  const id = Number(form.get("id"));
  if (Number.isInteger(id)) {
    await db().query("DELETE FROM stratum_instances WHERE id = $1", [id]);
  }
  revalidatePath("/admin/servers");
}
