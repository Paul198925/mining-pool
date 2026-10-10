import { createHash, createHmac, timingSafeEqual } from "node:crypto";

import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { connection } from "next/server";

export const ADMIN_COOKIE = "admin_session";
export const SESSION_SECONDS = 12 * 60 * 60;
const MIN_PASSWORD_LENGTH = 12;

/** The admin password from the environment, or null if the admin area is switched off. */
function password(): string | null {
  const value = process.env.ADMIN_PASSWORD ?? "";
  return value.length >= MIN_PASSWORD_LENGTH ? value : null;
}

export function adminConfigured(): boolean {
  return password() !== null;
}

/** Keyed with the password itself, so changing the password ends every session. */
function signature(expires: number, key: string): Buffer {
  return createHmac("sha256", key).update(`admin:${expires}`).digest();
}

export function passwordMatches(input: string): boolean {
  const expected = password();
  if (expected === null) return false;
  // Hash both sides so the comparison does not depend on their lengths.
  const digest = (value: string) => createHash("sha256").update(value).digest();
  return timingSafeEqual(digest(input), digest(expected));
}

export function newSessionValue(): string {
  const key = password();
  if (key === null) throw new Error("admin area is not configured");
  const expires = Math.floor(Date.now() / 1000) + SESSION_SECONDS;
  return `${expires}.${signature(expires, key).toString("hex")}`;
}

export async function isAdmin(): Promise<boolean> {
  // The check depends on the current time, so it must never run as part of a prerender.
  await connection();
  const key = password();
  const value = (await cookies()).get(ADMIN_COOKIE)?.value;
  if (key === null || !value) return false;
  const [expiresText, mac] = value.split(".");
  const expires = Number(expiresText);
  if (!Number.isInteger(expires) || !mac || expires < Date.now() / 1000) return false;
  const given = Buffer.from(mac, "hex");
  const expected = signature(expires, key);
  return given.length === expected.length && timingSafeEqual(given, expected);
}

/** Every admin page and every admin query starts with this. */
export async function requireAdmin(): Promise<void> {
  if (!(await isAdmin())) {
    redirect("/admin/login");
  }
}
