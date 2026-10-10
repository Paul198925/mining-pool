"use server";

import { headers } from "next/headers";
import { revalidatePath } from "next/cache";

import { requireAdmin } from "@/lib/admin-auth";
import { COINS, poolAddress } from "@/lib/catalog";
import { db } from "@/lib/db";
import { newSecret, sha256 } from "@/lib/node-auth";
import { publicUrl } from "@/lib/public-url";

const CODE_VALID_MINUTES = 30;

export type AddServerState = { error?: string; command?: string; expiresInMinutes?: number };

/** Reserves an instance id for a new server and returns the command that enrolls it. */
export async function addServer(_previous: AddServerState, form: FormData): Promise<AddServerState> {
  await requireAdmin();
  const region = String(form.get("region") ?? "").trim();
  const host = String(form.get("host") ?? "").trim();
  const coins = COINS.map((coin) => coin.symbol).filter((symbol) => form.get(`coin-${symbol}`) === "on");
  if (region.length < 1 || region.length > 40) return { error: "Give the server a region name of up to 40 characters." };
  if (!/^[A-Za-z0-9.-]{1,253}$/.test(host)) return { error: "Enter the server's public hostname or IP address." };
  if (coins.length === 0) return { error: "Choose at least one coin." };
  const missing = coins.filter((symbol) => poolAddress(symbol) === null);
  if (missing.length > 0) {
    return { error: `No pool address is configured for ${missing.join(", ")} (POOL_ADDRESS_<SYMBOL>).` };
  }

  const code = newSecret();
  // The lowest free instance id; a server that was removed frees its id again.
  const { rows } = await db().query(
    `INSERT INTO stratum_instances (id, region, host, coins, enroll_code_hash, enroll_expires_at)
     SELECT free.id, $1, $2, $3, $4, now() + make_interval(mins => $5)
     FROM generate_series(1, 255) AS free (id)
     WHERE NOT EXISTS (SELECT 1 FROM stratum_instances used WHERE used.id = free.id)
     ORDER BY free.id
     LIMIT 1
     RETURNING id`,
    [region, host, coins, sha256(code), CODE_VALID_MINUTES],
  );
  if (rows.length === 0) return { error: "All 255 server slots are taken." };

  const pool = publicUrl(await headers());
  revalidatePath("/admin/servers");
  return {
    command: `curl -fsSL ${pool}/install.sh | sudo bash -s -- ${pool} ${code}`,
    expiresInMinutes: CODE_VALID_MINUTES,
  };
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
