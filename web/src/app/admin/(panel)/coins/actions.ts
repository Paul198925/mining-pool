"use server";

import { revalidatePath } from "next/cache";

import { requireAdmin } from "@/lib/admin-auth";
import { db } from "@/lib/db";

export type SaveState = { error?: string; saved?: boolean };

const number = (value: FormDataEntryValue | null): number => Number(String(value ?? "").replace(",", "."));

/** Switches a coin on or off for mining and sets its fees and minimum payout. */
export async function saveCoin(_previous: SaveState, form: FormData): Promise<SaveState> {
  await requireAdmin();
  const symbol = String(form.get("symbol") ?? "");
  const enabled = form.get("enabled") === "on";
  const poolFee = number(form.get("poolFee"));
  const soloFee = number(form.get("soloFee"));
  const minPayout = number(form.get("minPayout"));
  const validFee = (fee: number) => Number.isFinite(fee) && fee >= 0 && fee <= 50;
  if (!validFee(poolFee) || !validFee(soloFee)) return { error: "Fees must be between 0 and 50 percent." };
  if (!Number.isFinite(minPayout) || minPayout <= 0 || minPayout > 1e9) return { error: "The minimum payout must be above zero." };

  const { rowCount } = await db().query(
    "UPDATE coins SET enabled = $2, pool_fee_percent = $3, solo_fee_percent = $4, min_payout = $5 WHERE symbol = $1",
    [symbol, enabled, poolFee, soloFee, Math.round(minPayout * 1e8)],
  );
  if (rowCount === 0) return { error: "Unknown coin." };
  revalidatePath("/admin/coins");
  return { saved: true };
}
