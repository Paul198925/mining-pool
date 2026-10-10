"use server";

import { revalidatePath } from "next/cache";

import { requireAdmin } from "@/lib/admin-auth";
import { db } from "@/lib/db";

export type SaveState = { error?: string; saved?: boolean };

/** Switches the internal exchange on or off and sets its fee. Applies from the next round. */
export async function saveExchange(_previous: SaveState, form: FormData): Promise<SaveState> {
  await requireAdmin();
  const enabled = form.get("enabled") === "on";
  const fee = Number(String(form.get("fee") ?? "").replace(",", "."));
  if (!Number.isFinite(fee) || fee < 0 || fee > 50) return { error: "The fee must be between 0 and 50 percent." };
  await db().query(
    `INSERT INTO pool_settings (name, value) VALUES ('exchange_enabled', $1), ('exchange_fee_percent', $2)
     ON CONFLICT (name) DO UPDATE SET value = EXCLUDED.value`,
    [String(enabled), String(fee)],
  );
  revalidatePath("/admin/exchange");
  return { saved: true };
}

/**
 * Sets a coin's price by hand, or with an empty field goes back to the price API.
 * A price set by hand is used as it is until it is changed: the pool does not check it.
 */
export async function saveManualPrice(_previous: SaveState, form: FormData): Promise<SaveState> {
  await requireAdmin();
  const symbol = String(form.get("symbol") ?? "");
  const text = String(form.get("price") ?? "").trim().replace(",", ".");
  const price = text === "" ? null : Number(text);
  if (price !== null && !(Number.isFinite(price) && price > 0 && price < 1e9)) {
    return { error: "Enter a price above zero in US dollars, or leave the field empty." };
  }
  const { rowCount } = await db().query("UPDATE coins SET price_manual_usd = $2 WHERE symbol = $1", [symbol, price]);
  if (rowCount === 0) return { error: "Unknown coin." };
  if (price !== null) console.warn(`admin set the price of ${symbol} by hand to ${price} USD`);
  revalidatePath("/admin/exchange");
  return { saved: true };
}
