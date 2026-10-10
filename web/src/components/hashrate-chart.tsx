"use client";

import { useEffect, useRef, useState } from "react";

import { formatHashrate } from "@/lib/format";
import type { HashratePoint } from "@/lib/queries";

const HEIGHT = 180;
const MARGIN = { top: 12, right: 12, bottom: 22, left: 92 };
const Y_TICKS = 2;

const time = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });

/** Rounds up to 1, 2 or 5 times a power of ten so the axis ends on a readable number. */
function niceCeil(value: number): number {
  if (value <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(value));
  const step = [1, 2, 5, 10].find((m) => m * power >= value) ?? 10;
  return step * power;
}

/** A coin amount with up to six significant digits. */
function formatCoins(value: number): string {
  return Number(value.toPrecision(6)).toLocaleString("en-US", { maximumFractionDigits: 8 });
}

/** Draws a hashrate over the last 24 hours, or with `unit` an amount of that coin. */
export function HashrateChart({ points, label, unit }: { points: HashratePoint[]; label: string; unit?: string }) {
  const show = (value: number) => (unit ? `${formatCoins(value)} ${unit}` : formatHashrate(value));
  const container = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [hover, setHover] = useState<number | null>(null);

  useEffect(() => {
    const element = container.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const values = points.map((p) => p.v);
  const peak = Math.max(0, ...values);
  const average = values.length ? values.reduce((a, b) => a + b, 0) / values.length : 0;
  const yMax = niceCeil(peak);
  const plotWidth = Math.max(0, width - MARGIN.left - MARGIN.right);
  const plotHeight = HEIGHT - MARGIN.top - MARGIN.bottom;
  const x = (i: number) => MARGIN.left + (points.length > 1 ? (i / (points.length - 1)) * plotWidth : 0);
  const y = (v: number) => MARGIN.top + plotHeight - (v / yMax) * plotHeight;
  const line = points.map((p, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(p.v).toFixed(1)}`).join("");
  const baseline = MARGIN.top + plotHeight;
  const xLabels = points.length > 1 ? [0, Math.floor((points.length - 1) / 2), points.length - 1] : [];
  const hovered = hover === null ? null : points[hover];

  return (
    <figure>
      <div
        ref={container}
        className="relative"
        role="img"
        aria-label={`${label}, last 24 hours. Average ${show(average)}, peak ${show(peak)}.`}
      >
        {/* Rendered only once the width is known, which also keeps local-time labels out of the server HTML. */}
        {width > 0 && points.length > 1 ? (
          <svg
            width={width}
            height={HEIGHT}
            onPointerMove={(event) => {
              const offset = event.clientX - event.currentTarget.getBoundingClientRect().left - MARGIN.left;
              const index = Math.round((offset / plotWidth) * (points.length - 1));
              setHover(Math.min(points.length - 1, Math.max(0, index)));
            }}
            onPointerLeave={() => setHover(null)}
          >
            {Array.from({ length: Y_TICKS + 1 }, (_, i) => (yMax / Y_TICKS) * i).map((tick) => (
              <g key={tick}>
                <line x1={MARGIN.left} x2={width - MARGIN.right} y1={y(tick)} y2={y(tick)} stroke="var(--border)" />
                <text x={MARGIN.left - 8} y={y(tick)} dy="0.32em" textAnchor="end" fontSize={11} fill="var(--muted)">
                  {show(tick)}
                </text>
              </g>
            ))}
            {xLabels.map((i, n) => (
              <text
                key={i}
                x={x(i)}
                y={HEIGHT - 4}
                textAnchor={n === 0 ? "start" : n === xLabels.length - 1 ? "end" : "middle"}
                fontSize={11}
                fill="var(--muted)"
              >
                {time.format(new Date(points[i].t))}
              </text>
            ))}
            <path d={`${line}L${x(points.length - 1)},${baseline}L${x(0)},${baseline}Z`} fill="var(--accent)" opacity={0.1} />
            <path d={line} fill="none" stroke="var(--accent)" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
            {hovered && hover !== null && (
              <g>
                <line x1={x(hover)} x2={x(hover)} y1={MARGIN.top} y2={baseline} stroke="var(--muted)" />
                <circle cx={x(hover)} cy={y(hovered.v)} r={4} fill="var(--accent)" stroke="var(--surface)" strokeWidth={2} />
              </g>
            )}
          </svg>
        ) : (
          <div style={{ height: HEIGHT }} />
        )}
        {hovered && hover !== null && (
          <div
            className="pointer-events-none absolute top-0 rounded border border-border bg-surface px-2 py-1 shadow-sm"
            style={x(hover) > width / 2 ? { right: width - x(hover) + 10 } : { left: x(hover) + 10 }}
          >
            <div className="font-bold">{show(hovered.v)}</div>
            <div className="text-muted">{time.format(new Date(hovered.t))}</div>
          </div>
        )}
      </div>
      <figcaption className="mt-2 flex gap-6 text-muted">
        <span>
          {unit ? "Now" : "24h average"}{" "}
          <span className="font-bold text-foreground">{show(unit ? (values.at(-1) ?? 0) : average)}</span>
        </span>
        <span>
          Peak <span className="font-bold text-foreground">{show(peak)}</span>
        </span>
      </figcaption>
    </figure>
  );
}
