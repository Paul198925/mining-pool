"use client";

import { useActionState } from "react";

import { login } from "../actions";

export function LoginForm() {
  const [error, action, pending] = useActionState(login, null);
  return (
    <form action={action} className="grid gap-2 p-1">
      <div className="flex items-center gap-2">
        <label htmlFor="admin-password">
          <b>Password:</b>
        </label>
        <input
          id="admin-password"
          name="password"
          type="password"
          autoComplete="current-password"
          required
          className="text-input"
        />
        <button type="submit" className="button" disabled={pending}>
          {pending ? "Checking…" : "Log in"}
        </button>
      </div>
      {error && (
        <p role="alert" className="alert">
          {error}
        </p>
      )}
    </form>
  );
}
