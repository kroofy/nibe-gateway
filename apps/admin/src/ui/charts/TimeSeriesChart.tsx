import { useRef, useState } from "react";
import type { PointerEvent, ReactNode } from "react";
import { formatTime, formatValue, niceTicks } from "./chartMath";
import type { ChartLine } from "./chartMath";

/**
 * SVG time-series chart: lines (bucket means) or grouped bars (usage per bucket), one unit per
 * chart. Composed of the plot, a `ChartTooltip` while hovering, and a `ChartLegend` when there is
 * more than one series. Renders `ChartEmpty` without data.
 */
export function TimeSeriesChart({
  lines,
  from,
  to,
  bars,
  empty = "No recorded data in this range yet. History starts when the gateway has been running a few minutes.",
  initialHover = null,
}: {
  lines: ChartLine[];
  from: number;
  to: number;
  bars?: boolean;
  empty?: ReactNode;
  /** Pin the crosshair at a time (stories and tests). */
  initialHover?: number | null;
}) {
  const [hover, setHover] = useState<number | null>(initialHover);
  const svg = useRef<SVGSVGElement>(null);
  const width = 760, height = 260, left = 48, right = 120, top = 12, bottom = 28;
  const values = lines.flatMap((l) => l.points.map((p) => p.v));
  const isEmpty = values.length === 0;
  let lo = isEmpty ? 0 : Math.min(...values);
  let hi = isEmpty ? 1 : Math.max(...values);
  if (bars) lo = Math.min(0, lo);
  if (hi === lo) { hi += 1; lo -= bars ? 0 : 1; }
  const pad = bars ? 0 : (hi - lo) * 0.08;
  const ticks = niceTicks(lo - pad, hi + pad);
  const yMin = Math.min(ticks[0], lo - pad), yMax = Math.max(ticks[ticks.length - 1], hi + pad);
  const x = (t: number) => left + ((t - from) / (to - from)) * (width - left - right);
  const y = (v: number) => top + (1 - (v - yMin) / (yMax - yMin)) * (height - top - bottom);
  const times = Array.from(new Set(lines.flatMap((l) => l.points.map((p) => p.t)))).sort((a, b) => a - b);
  const unit = lines.find((l) => l.unit)?.unit ?? "";
  const span = to - from;
  const xTicks = Array.from({ length: 5 }, (_, i) => from + (span * i) / 4);
  const nearest = (t: number) => times.reduce((best, c) => (Math.abs(c - t) < Math.abs(best - t) ? c : best), times[0]);
  function move(e: PointerEvent<SVGSVGElement>) {
    if (!svg.current || times.length === 0) return;
    const box = svg.current.getBoundingClientRect();
    const px = ((e.clientX - box.left) / box.width) * width;
    const t = from + ((px - left) / (width - left - right)) * span;
    setHover(nearest(t));
  }
  const bucketWidth = times.length > 1 ? Math.max(2, ((x(times[1]) - x(times[0])) / lines.length) - 2) : 8;
  if (isEmpty) return <ChartEmpty>{empty}</ChartEmpty>;
  return (
    <div className="chart">
      <svg ref={svg} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={lines.map((l) => l.name).join(", ")}
        onPointerMove={move} onPointerLeave={() => setHover(null)}>
        {ticks.map((v) => (
          <g key={v}>
            <line x1={left} x2={width - right} y1={y(v)} y2={y(v)} className="grid" />
            <text x={left - 8} y={y(v) + 4} className="axis" textAnchor="end">{formatValue(v)}</text>
          </g>
        ))}
        {xTicks.map((t) => (
          <text key={t} x={x(t)} y={height - 8} className="axis" textAnchor="middle">{formatTime(t, span)}</text>
        ))}
        <text x={4} y={top + 4} className="axis">{unit}</text>
        {bars
          ? lines.map((l, i) => l.points.map((p) => (
              <rect key={`${i}-${p.t}`} x={x(p.t) + i * (bucketWidth + 2)} y={y(Math.max(0, p.v))} width={bucketWidth}
                height={Math.max(0, y(0) - y(Math.max(0, p.v)))} rx={2} fill={l.color} />
            )))
          : lines.map((l, i) => {
              // Break the line where samples are missing for more than 3 buckets.
              const step = times.length > 1 ? (times[times.length - 1] - times[0]) / (times.length - 1) : span;
              let d = "";
              l.points.forEach((p, k) => {
                const gap = k > 0 && p.t - l.points[k - 1].t > step * 3;
                d += `${k === 0 || gap ? "M" : "L"}${x(p.t).toFixed(1)},${y(p.v).toFixed(1)}`;
              });
              return <path key={i} d={d} fill="none" stroke={l.color} strokeWidth={2} strokeLinejoin="round" />;
            })}
        {!bars && lines.map((l, i) => {
          const last = l.points[l.points.length - 1];
          return last ? (
            <text key={i} x={x(last.t) + 6} y={y(last.v) + 4} className="label">{l.name.length > 18 ? `${l.name.slice(0, 17)}…` : l.name}</text>
          ) : null;
        })}
        {hover !== null && (
          <g>
            <line x1={x(hover)} x2={x(hover)} y1={top} y2={height - bottom} className="crosshair" />
            {!bars && lines.map((l, i) => {
              const p = l.points.find((q) => q.t === hover);
              return p ? <circle key={i} cx={x(p.t)} cy={y(p.v)} r={4} fill={l.color} stroke="white" strokeWidth={2} /> : null;
            })}
          </g>
        )}
      </svg>
      {hover !== null && <ChartTooltip time={hover} lines={lines} />}
      {lines.length > 1 && <ChartLegend lines={lines} />}
    </div>
  );
}

/** Values of every series at one time, pinned to the chart's top-right corner. */
export function ChartTooltip({ time, lines }: { time: number; lines: ChartLine[] }) {
  return (
    <div className="chart-tooltip" role="status">
      <div className="chart-tooltip-time">{new Date(time).toLocaleString()}</div>
      {lines.map((l, i) => {
        const p = l.points.find((q) => q.t === time);
        return (
          <div key={i}><Swatch color={l.color} /><strong>{p ? formatValue(p.v) : "–"}</strong> {l.unit} <span>{l.name}</span></div>
        );
      })}
    </div>
  );
}

export function ChartLegend({ lines }: { lines: Pick<ChartLine, "name" | "color">[] }) {
  return (
    <div className="chart-legend">
      {lines.map((l, i) => <span key={i}><Swatch color={l.color} />{l.name}</span>)}
    </div>
  );
}

export function ChartEmpty({ children }: { children: ReactNode }) {
  return <div className="chart-empty">{children}</div>;
}

/** 10 px colour key for a series. */
export function Swatch({ color }: { color: string }) {
  return <span className="swatch" style={{ background: color }} />;
}
