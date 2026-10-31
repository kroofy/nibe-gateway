import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../api";
import type { HistoryResponse, HistorySeries } from "../schema";
import {
  ActionRow, Button, ButtonLink, Field, FieldHelp, FilterBar, Notice, Panel, SERIES_COLORS, SegmentedControl,
  Table, TimeSeriesChart, formatValue,
} from "../ui";
import type { ChartLine } from "../ui";

type Preset = {
  title: string;
  registers: number[];
  /** Counters are drawn as usage per bucket (difference of consecutive `last` values). */
  usage?: boolean;
};
/** One unit per chart: never two scales on one axis. */
export const PRESETS: Preset[] = [
  { title: "Heating temperatures", registers: [40004, 40008, 40012, 43009] },
  { title: "Hot water", registers: [40013, 40014] },
  { title: "Indoor and ventilation air", registers: [40033, 40025, 40026] },
  { title: "Refrigerant circuit", registers: [40018, 40017, 40019, 40020] },
  { title: "Compressor frequency", registers: [43136] },
  { title: "Compressor power", registers: [43141] },
  { title: "Phase currents", registers: [40083, 40081, 40079] },
  { title: "Energy use per period", registers: [41850, 41848], usage: true },
  { title: "Degree minutes", registers: [43005] },
];
const RANGES = [
  { label: "24 h", ms: 24 * 3600e3 },
  { label: "7 days", ms: 7 * 86400e3 },
  { label: "30 days", ms: 30 * 86400e3 },
  { label: "1 year", ms: 366 * 86400e3 },
  { label: "2 years", ms: 731 * 86400e3 },
];

export function toLines(series: HistorySeries[], usage: boolean): ChartLine[] {
  return series.map((s, i) => {
    const color = SERIES_COLORS[i % SERIES_COLORS.length];
    const unit = s.unit ?? "";
    if (!usage) {
      return { name: s.name, unit, color, points: s.points.map((p) => ({ t: p.t, v: s.kind === "counter" ? p.last : p.mean })) };
    }
    const points = s.points.slice(1).map((p, k) => ({ t: p.t, v: Math.max(0, p.last - s.points[k].last) }));
    return { name: s.name, unit, color, points };
  });
}

export function History() {
  const [preset, setPreset] = useState(0);
  const [range, setRange] = useState(0);
  const [data, setData] = useState<HistoryResponse | null>(null);
  const [error, setError] = useState("");
  const [table, setTable] = useState(false);
  const current = PRESETS[preset];
  const load = useCallback(async () => {
    const to = Date.now();
    const from = to - RANGES[range].ms;
    try {
      setError("");
      setData(await api.history(current.registers, from, to, current.usage ? (range === 0 ? "1h" : "1d") : "auto"));
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [current, range]);
  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), 60_000);
    return () => window.clearInterval(timer);
  }, [load]);
  const lines = useMemo(() => (data ? toLines(data.series, !!current.usage) : []), [data, current]);
  const csv = data
    ? `/api/v1/history/export.csv?registers=${current.registers.join(",")}&from=${data.from}&to=${data.to}`
    : undefined;
  return (
    <>
      {error && <Notice kind="error">{error}</Notice>}
      <FilterBar>
        <Field label="Chart">
          <select value={preset} onChange={(e) => setPreset(Number(e.target.value))}>
            {PRESETS.map((p, i) => <option key={p.title} value={i}>{p.title}</option>)}
          </select>
        </Field>
        <SegmentedControl
          aria-label="Time range"
          options={RANGES.map((r, i) => ({ value: i, label: r.label }))}
          value={range}
          onChange={setRange}
        />
      </FilterBar>
      <Panel
        title={current.title}
        action={
          <ActionRow compact>
            <Button type="button" variant="secondary" size="compact" onClick={() => setTable(!table)}>{table ? "Hide table" : "Show table"}</Button>
            {csv && <ButtonLink href={csv}>Download CSV</ButtonLink>}
          </ActionRow>
        }
      >
        {data && <TimeSeriesChart lines={lines} from={data.from} to={data.to} bars={!!current.usage} />}
        {data && data.series[0] && (
          <FieldHelp>
            {data.series[0].resolution === "raw" ? "Every reading" : `${data.series[0].resolution} buckets`}
            {current.usage ? "; bars show the counter increase per bucket." : "; lines show the bucket mean."}
          </FieldHelp>
        )}
        {table && <HistoryTable lines={lines} />}
      </Panel>
    </>
  );
}

/** The chart's data as a table, newest first, at most 200 rows. */
export function HistoryTable({ lines }: { lines: ChartLine[] }) {
  return (
    <Table compact>
      <thead><tr><th>Time</th>{lines.map((l) => <th key={l.name}>{l.name}{l.unit ? ` (${l.unit})` : ""}</th>)}</tr></thead>
      <tbody>
        {Array.from(new Set(lines.flatMap((l) => l.points.map((p) => p.t)))).sort((a, b) => b - a).slice(0, 200).map((t) => (
          <tr key={t}><td>{new Date(t).toLocaleString()}</td>{lines.map((l) => <td key={l.name}>{(() => { const p = l.points.find((q) => q.t === t); return p ? formatValue(p.v) : "–"; })()}</td>)}</tr>
        ))}
      </tbody>
    </Table>
  );
}
