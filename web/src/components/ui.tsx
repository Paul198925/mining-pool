import type { ReactNode } from "react";

import type { Block } from "@/lib/queries";

export function Card({ title, children, action }: { title?: string; children: ReactNode; action?: ReactNode }) {
  return (
    <section className="rounded-lg border border-border bg-surface p-4">
      {title && (
        <div className="mb-3 flex items-baseline justify-between">
          <h2 className="text-sm font-semibold">{title}</h2>
          {action}
        </div>
      )}
      {children}
    </section>
  );
}

export function Stat({ label, value, hint }: { label: string; value: ReactNode; hint?: ReactNode }) {
  return (
    <div>
      <div className="text-xs text-muted">{label}</div>
      <div className="font-mono text-lg font-medium">{value}</div>
      {hint && <div className="text-xs text-muted">{hint}</div>}
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="py-6 text-center text-sm text-muted">{children}</p>;
}

export function Skeleton({ height = 120 }: { height?: number }) {
  return <div className="animate-pulse rounded-lg border border-border bg-surface" style={{ height }} />;
}

const STATUS: Record<Block["status"], { label: string; symbol: string; color: string }> = {
  pending: { label: "Pending", symbol: "◷", color: "text-warning" },
  confirmed: { label: "Confirmed", symbol: "✓", color: "text-good" },
  orphaned: { label: "Orphaned", symbol: "✕", color: "text-critical" },
};

/** Status is carried by icon and word; colour only reinforces it. */
export function BlockStatus({ status, progress }: { status: Block["status"]; progress?: string }) {
  const { label, symbol, color } = STATUS[status];
  return (
    <span className="whitespace-nowrap">
      <span className={color} aria-hidden>
        {symbol}
      </span>{" "}
      {label}
      {progress && <span className="text-muted"> {progress}</span>}
    </span>
  );
}

export const th = "px-3 py-2 text-left text-xs font-medium text-muted";
export const td = "px-3 py-2 text-sm";
