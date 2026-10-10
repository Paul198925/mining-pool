"use server";

import { revalidatePath } from "next/cache";

import { requireAdmin } from "@/lib/admin-auth";
import { db } from "@/lib/db";

export type AddressState = { error?: string; saved?: boolean };

/**
 * Changes where an account is paid: its balance and everything it earns from now on go
 * to the new address, and payouts that are queued but not yet sent are redirected too.
 * For a miner who mined to a wrong or lost address.
 */
export async function changePayoutAddress(_previous: AddressState, form: FormData): Promise<AddressState> {
  await requireAdmin();
  const id = String(form.get("id") ?? "");
  const address = String(form.get("address") ?? "").trim();
  if (!/^\d{1,18}$/.test(id)) return { error: "Unknown account." };
  if (!/^[A-Za-z0-9]{20,100}$/.test(address)) return { error: "That does not look like an address." };
  if (form.get("confirm") !== "on") {
    return { error: "Tick the box to confirm: coins sent to a wrong address cannot be brought back." };
  }

  const client = await db().connect();
  try {
    await client.query("BEGIN");
    const account = await client.query("SELECT payout_coin_id, payout_address FROM accounts WHERE id = $1 FOR UPDATE", [id]);
    if (account.rows.length === 0) {
      await client.query("ROLLBACK");
      return { error: "Unknown account." };
    }
    const { payout_coin_id: coinId, payout_address: before } = account.rows[0];
    if (before === address) {
      await client.query("ROLLBACK");
      return { error: "That is the address the account already has." };
    }
    const other = await client.query("SELECT 1 FROM accounts WHERE payout_coin_id = $1 AND payout_address = $2", [coinId, address]);
    if (other.rows.length > 0) {
      await client.query("ROLLBACK");
      return { error: "Another account already uses this address. Two accounts cannot be merged here." };
    }
    await client.query("UPDATE accounts SET payout_address = $2 WHERE id = $1", [id, address]);
    // Not the ones the wallet has already been asked to send.
    const queued = await client.query(
      "UPDATE payouts SET address = $2 WHERE account_id = $1 AND status = 'pending'",
      [id, address],
    );
    await client.query("COMMIT");
    // Money was redirected: leave a trace in the server log.
    console.warn(`admin changed the payout address of account ${id} from ${before} to ${address} (${queued.rowCount} queued payouts redirected)`);
  } catch (error) {
    await client.query("ROLLBACK");
    console.error("changing a payout address failed", error);
    return { error: "Could not save." };
  } finally {
    client.release();
  }
  revalidatePath(`/admin/miners/${id}`);
  revalidatePath("/admin/miners");
  return { saved: true };
}
