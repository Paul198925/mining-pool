import type { ReactNode } from "react";

import type { Block } from "@/lib/queries";

/** A YiiMP-style box: grey title bar over the content. */
export function Box({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="box">
      <div className="box-title">{title}</div>
      <div className="box-inner">{children}</div>
    </section>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="empty">{children}</p>;
}

export function Skeleton({ height = 120 }: { height?: number }) {
  return <div className="box animate-pulse" style={{ height, backgroundColor: "#f4f4f4" }} />;
}

const STATUS_LABEL: Record<Block["status"], string> = {
  pending: "Immature",
  confirmed: "Confirmed",
  orphaned: "Orphan",
};

export function BlockStatus({ status, progress }: { status: Block["status"]; progress?: string }) {
  return (
    <span className={`block ${status}`}>
      {STATUS_LABEL[status]}
      {progress && ` (${progress})`}
    </span>
  );
}
