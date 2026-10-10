import { Pool } from "pg";

const DEV_DATABASE_URL = "postgres://pool:pooldev@127.0.0.1:55432/pool";

function createPool(): Pool {
  const url = process.env.DATABASE_URL ?? (process.env.NODE_ENV === "production" ? undefined : DEV_DATABASE_URL);
  if (!url) {
    throw new Error("DATABASE_URL is not set");
  }
  return new Pool({ connectionString: url, max: 5 });
}

// Survive dev hot reloads without opening a new pool each time.
const globalForDb = globalThis as { poolDb?: Pool };

export function db(): Pool {
  return (globalForDb.poolDb ??= createPool());
}
