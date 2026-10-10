const UNITS_PER_COIN = BigInt(100_000_000);

/** Renders base units exactly; `digits` only trims trailing decimals for display. */
export function formatAmount(units: string, digits = 8): string {
  const value = BigInt(units);
  const negative = value < 0;
  const abs = negative ? -value : value;
  const whole = (abs / UNITS_PER_COIN).toLocaleString("en-US");
  const fraction = (abs % UNITS_PER_COIN).toString().padStart(8, "0").slice(0, digits);
  return `${negative ? "-" : ""}${whole}${digits > 0 ? `.${fraction}` : ""}`;
}

/** Plain decimal string for machine consumers, e.g. "1234.50000000". */
export function unitsToDecimal(units: string): string {
  return formatAmount(units).replaceAll(",", "");
}

function withUnit(value: number, units: string[], base: number): string {
  let index = 0;
  while (value >= base && index < units.length - 1) {
    value /= base;
    index += 1;
  }
  const digits = value >= 100 || index === 0 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[index]}`;
}

export function formatHashrate(hashesPerSecond: number): string {
  return withUnit(hashesPerSecond, ["H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s"], 1000);
}

export function formatDifficulty(difficulty: number | null): string {
  if (difficulty === null) return "–";
  if (difficulty < 1) return difficulty.toPrecision(3);
  return withUnit(difficulty, ["", "k", "M", "G", "T", "P"], 1000).trim();
}

/**
 * How long it takes on average to find a block at `hashrate`: a block needs
 * difficulty × 2^32 hashes. An average only; the next block can come at any moment.
 */
export function timeToFind(networkDifficulty: number | null, hashrate: number): number | null {
  if (networkDifficulty === null || !(networkDifficulty > 0) || !(hashrate > 0)) return null;
  return (networkDifficulty * 4294967296) / hashrate;
}

/** A time span in its two largest units, e.g. "3h 20m" or "12d 4h". */
export function formatDuration(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return "–";
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${Math.round(seconds % 60)}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours}h ${minutes % 60}m`;
  const days = Math.floor(hours / 24);
  if (days < 730) return `${days}d ${hours % 24}h`;
  return `${(days / 365).toFixed(days < 36500 ? 1 : 0)} years`;
}

export function formatAge(date: Date | null, now = new Date()): string {
  if (!date) return "never";
  const seconds = Math.max(0, Math.round((now.getTime() - date.getTime()) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

export function shorten(text: string, head = 8, tail = 6): string {
  return text.length <= head + tail + 1 ? text : `${text.slice(0, head)}…${text.slice(-tail)}`;
}
