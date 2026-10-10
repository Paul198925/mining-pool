/** The address servers reach the pool at: `POOL_PUBLIC_URL`, or else wherever the request came in. */
export function publicUrl(headers: Headers): string {
  const configured = process.env.POOL_PUBLIC_URL;
  if (configured) return configured.replace(/\/+$/, "");
  const host = headers.get("x-forwarded-host") ?? headers.get("host") ?? "localhost";
  const proto = headers.get("x-forwarded-proto") ?? "http";
  return `${proto}://${host}`;
}
