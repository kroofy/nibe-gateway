import { useCallback, useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import type { AuditEntry, AuditSource, RegisterLeaseStatus } from "../schema";
import {
  ActionRow, Badge, Button, Cell, CellMeta, CellTitle, EmptyRow, FilterGroup, Notice, Panel, PanelDescription, Table,
  TableScroll,
} from "../ui";

const SOURCES: Record<AuditSource, string> = {
  http: "Admin / API",
  mqtt: "MQTT",
  gateway: "Gateway",
  pump: "Pump",
};
type Filter = "all" | AuditSource;
const FILTERS: { value: Filter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "http", label: "Admin / API" },
  { value: "mqtt", label: "MQTT" },
  { value: "gateway", label: "Gateway" },
  { value: "pump", label: "Pump" },
];

const time = (ms: number) => new Date(ms).toLocaleString();
function remaining(seconds: number) {
  if (seconds <= 0) return "lapsed, restoring";
  const m = Math.floor(seconds / 60);
  return m > 0 ? `${m} min ${seconds % 60} s` : `${seconds} s`;
}
const registerName = (register: number) => (register === 47011 ? "47011 heating offset" : String(register));

/** Every change made to the gateway and the pump, and the leases in force now. */
export function Audit() {
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [more, setMore] = useState(false);
  const [leases, setLeases] = useState<RegisterLeaseStatus | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const refresh = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      const [page, status] = await Promise.all([api.audit(), api.status()]);
      setEntries(page.entries);
      setMore(page.more);
      setLeases(status.register_leases ?? null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => { void refresh(); }, [refresh]);
  async function older() {
    const last = entries.at(-1);
    if (!last) return;
    setBusy(true);
    try {
      const page = await api.audit(last.seq);
      setEntries([...entries, ...page.entries]);
      setMore(page.more);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const shown = filter === "all" ? entries : entries.filter((e) => e.source === filter);
  return (
    <>
      {error && <Notice kind="error">{error}</Notice>}
      <LeasesPanel leases={leases} />
      <Panel
        title="Changes"
        action={<ActionRow compact><Button variant="secondary" size="compact" disabled={busy} onClick={() => void refresh()}>Refresh</Button></ActionRow>}
      >
        <PanelDescription>
          Every change sent to the gateway or the pump, newest first: by the admin, an API key, MQTT, the gateway
          itself (lapsed leases) and the pump's answer to each register write. Passwords and keys are never stored.
          Reads and sign-ins are not listed. The gateway keeps the latest 1000 entries.
        </PanelDescription>
        <FilterGroup id="audit-source" label="Source" options={FILTERS} value={filter} onChange={setFilter} />
        <AuditTable entries={shown} />
        {more && (
          <ActionRow>
            <Button variant="secondary" disabled={busy} onClick={() => void older()}>Show older</Button>
          </ActionRow>
        )}
      </Panel>
    </>
  );
}

/** Leases in force: who holds them, the value they wrote and the value restored when they lapse. */
export function LeasesPanel({ leases }: { leases: RegisterLeaseStatus | null }) {
  const active = leases?.leases ?? [];
  return (
    <Panel title="Active leases" action={<Badge tone={active.length ? "amber" : "neutral"}>{String(active.length)}</Badge>}>
      <PanelDescription>
        A leased write is undone unless its writer renews it in time: the gateway then writes back the value it read
        before the first leased write. Leasable registers: {leases ? leases.leasable.map(registerName).join(", ") : "none"}.
      </PanelDescription>
      <TableScroll label="Active leases">
        <Table compact>
          <thead><tr><th>Register</th><th>Holder</th><th>Leased value</th><th>Restores to</th><th>Ends in</th></tr></thead>
          <tbody>
            {active.length === 0 && <EmptyRow colSpan={5}>No active leases.</EmptyRow>}
            {active.map(({ lease, remaining_s }) => (
              <tr key={lease.register}>
                <Cell mono>{registerName(lease.register)}</Cell>
                <td>{lease.holder}</td>
                <Cell mono>{lease.raw}</Cell>
                <Cell mono>{lease.baseline}</Cell>
                <td>{remaining(remaining_s)}<br /><CellMeta>since {time(lease.granted_at_ms)}</CellMeta></td>
              </tr>
            ))}
          </tbody>
        </Table>
      </TableScroll>
    </Panel>
  );
}

/** Audit entries as a table: time, source and actor, what changed, and the outcome. */
export function AuditTable({ entries }: { entries: AuditEntry[] }) {
  return (
    <TableScroll label="Audit log">
      <Table compact>
        <thead><tr><th>Time</th><th>Who</th><th>Change</th><th>Result</th></tr></thead>
        <tbody>
          {entries.length === 0 && <EmptyRow colSpan={4}>No changes recorded yet.</EmptyRow>}
          {entries.map((e) => (
            <tr key={e.seq} data-testid="audit-entry">
              <td>{time(e.at_ms)}</td>
              <td>{e.actor}<br /><CellMeta>{SOURCES[e.source]}</CellMeta></td>
              <Cell wrap>
                <CellTitle>{e.action}{e.target ? ` · ${e.target}` : ""}</CellTitle>
                {e.detail && <><br /><CellMeta><code>{e.detail}</code></CellMeta></>}
              </Cell>
              <Cell wrap>
                <Badge tone={e.ok ? "green" : "amber"}>{e.ok ? "OK" : "Failed"}</Badge>
                {e.outcome && <><br /><CellMeta error={!e.ok}>{e.outcome}</CellMeta></>}
              </Cell>
            </tr>
          ))}
        </tbody>
      </Table>
    </TableScroll>
  );
}
