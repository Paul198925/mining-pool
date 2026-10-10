"use server";

import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { ADMIN_COOKIE, adminConfigured, newSessionValue, passwordMatches, SESSION_SECONDS } from "@/lib/admin-auth";

const MAX_FAILURES = 5;
const FAILURE_WINDOW_MS = 5 * 60 * 1000;
// Per server process and for everyone at once: crude, but it caps guessing at one try a minute.
let failures: number[] = [];

export async function login(_previous: string | null, form: FormData): Promise<string | null> {
  if (!adminConfigured()) {
    return "The admin area is switched off: ADMIN_PASSWORD is not set.";
  }
  const now = Date.now();
  failures = failures.filter((at) => now - at < FAILURE_WINDOW_MS);
  if (failures.length >= MAX_FAILURES) {
    return "Too many failed attempts. Try again in a few minutes.";
  }
  if (!passwordMatches(String(form.get("password") ?? ""))) {
    failures.push(now);
    return "Wrong password.";
  }
  (await cookies()).set(ADMIN_COOKIE, newSessionValue(), {
    httpOnly: true,
    // "lax" still keeps the cookie off cross-site POSTs, but lets a link or bookmark open the
    // admin area without a new login. Every admin action is a POST.
    sameSite: "lax",
    secure: process.env.NODE_ENV === "production",
    path: "/admin",
    maxAge: SESSION_SECONDS,
  });
  redirect("/admin");
}

export async function logout(): Promise<void> {
  (await cookies()).delete({ name: ADMIN_COOKIE, path: "/admin" });
  redirect("/admin/login");
}
