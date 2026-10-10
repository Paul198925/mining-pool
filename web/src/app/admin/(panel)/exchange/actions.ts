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
