/** Categorical slots in fixed order (dataviz reference palette, light steps). */
export const SERIES_COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100"];

export type ChartPoint = { t: number; v: number };
export type ChartLine = { name: string; unit: string; color: string; points: ChartPoint[] };

/** Round axis ticks (1, 2, 2.5, 5 × 10ⁿ steps) covering [min, max]. */
export function niceTicks(min: number, max: number, count = 5): number[] {
  if (!(max > min)) return [min];
  const raw = (max - min) / count;
  const step = [1, 2, 2.5, 5, 10].map((m) => m * 10 ** Math.floor(Math.log10(raw))).find((s) => s >= raw) ?? raw;
  const ticks = [];
  for (let v = Math.ceil(min / step) * step; v <= max + step * 1e-9; v += step) ticks.push(Number(v.toFixed(10)));
  return ticks;
}

/** Time label granularity follows the visible span: clock time, day, or month. */
export function formatTime(t: number, span: number): string {
  const d = new Date(t);
  if (span <= 2 * 86400e3) return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (span <= 60 * 86400e3) return d.toLocaleDateString([], { month: "short", day: "numeric" });
  return d.toLocaleDateString([], { year: "numeric", month: "short" });
}

/** Three significant digits at most for chart values. */
export const formatValue = (v: number) =>
  Math.abs(v) >= 100 ? v.toFixed(0) : Math.abs(v) >= 10 ? v.toFixed(1) : v.toFixed(2);
