"use server";

import { revalidatePath } from "next/cache";

import { requireAdmin } from "@/lib/admin-auth";
import { db } from "@/lib/db";

export type SaveState = { error?: string; saved?: boolean };

const positive = (value: FormDataEntryValue | null): number | null => {
  const number = Number(String(value ?? "").replace(",", "."));
  return Number.isFinite(number) && number > 0 ? number : null;
};

/** Changes an algorithm's stratum port and difficulty settings. */
export async function saveAlgo(_previous: SaveState, form: FormData): Promise<SaveState> {
  await requireAdmin();
  const name = String(form.get("name") ?? "");
  const port = Number(form.get("port"));
  const start = positive(form.get("start"));
  const min = positive(form.get("min"));
  const max = positive(form.get("max"));
  const target = positive(form.get("target"));
  if (!Number.isInteger(port) || port < 1024 || port > 65535) return { error: "The port must be between 1024 and 65535." };
  if (start === null || min === null || max === null) return { error: "Difficulties must be numbers above zero." };
  if (min > start || start > max) return { error: "Use minimum ≤ start ≤ maximum difficulty." };
  if (target === null || target < 1 || target > 300) return { error: "The share time must be between 1 and 300 seconds." };

  const client = await db().connect();
  try {
    await client.query("BEGIN");
    const taken = await client.query("SELECT name FROM algos WHERE port = $1 AND name <> $2", [port, name]);
    if (taken.rows.length > 0) {
      await client.query("ROLLBACK");
      return { error: `Port ${port} is already used by ${taken.rows[0].name}.` };
    }
    const before = await client.query("SELECT port FROM algos WHERE name = $1 FOR UPDATE", [name]);
    if (before.rows.length === 0) {
      await client.query("ROLLBACK");
      return { error: "Unknown algorithm." };
    }
    await client.query(
      `UPDATE algos SET port = $2, start_difficulty = $3, min_difficulty = $4, max_difficulty = $5, share_target_secs = $6
       WHERE name = $1`,
      [name, port, start, min, max, target],
    );
    if (before.rows[0].port !== port) {
      // What the website tells miners, and a reason for stratum servers to re-apply their setup.
      await client.query("UPDATE coins SET stratum_port = $2 WHERE algo = $1", [name, port]);
      await client.query("UPDATE stratum_instances SET config_version = config_version + 1 WHERE stratum");
    }
    await client.query("COMMIT");
  } catch (error) {
    await client.query("ROLLBACK");
    console.error("saving algo failed", error);
    return { error: "Could not save." };
  } finally {
    client.release();
  }
  revalidatePath("/admin/stratum");
  return { saved: true };
}
