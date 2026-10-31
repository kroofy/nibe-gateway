import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import type { DeviceSnapshot, RegisterDefinition, RegisterNote, RegisterReading, RegisterVerification } from "../schema";
import { age, display, human } from "../lib/format";
import {
  Badge, BadgeRow, Button, Cell, CellLabel, CellMeta, CellTitle, FieldHelp, Field, FilterGroup, Notice, Pagination,
  Panel, Table, TableScroll, Toolbar,
} from "../ui";

const PAGE_SIZE = 50;

export function Registers({
  device: d,
  elapsed,
  refresh,
}: {
  device: DeviceSnapshot;
  elapsed: number;
  refresh: () => void;
}) {
  const [overlay, setOverlay] = useState<Record<string, RegisterVerification>>({});
  const [notes, setNotes] = useState<Record<string, RegisterNote>>({});
  const [overlayError, setOverlayError] = useState("");
  // null = not chosen yet: relevant when this installation has notes, otherwise all.
  const [show, setShow] = useState<"relevant" | "all" | null>(null);
  useEffect(() => {
    if (d.mode === "simulated") return;
    let active = true;
    const load = () => { void api.explorer().then(s => { if (active) { setOverlay(s.verification_overlay); setNotes(s.register_notes ?? {}); setOverlayError(""); } }).catch(e => { if (active) setOverlayError(errorMessage(e)); }); };
    load(); const timer = setInterval(load, 5000);
    return () => { active = false; clearInterval(timer); };
  }, [d.mode]);
  const [search, setSearch] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [pageIndex, setPageIndex] = useState(0);
  const definitions = new Map(d.catalog.map((r) => [r.id, r]));
  const readings = new Map(d.registers.map((r) => [r.id, r]));
  const ids = [...new Set([...definitions.keys(), ...readings.keys()])];
  const hasNotes = Object.keys(notes).length > 0;
  const view = show ?? (hasNotes ? "relevant" : "all");
  const rows = ids
    .map((id) => ({
      id,
      definition: definitions.get(id),
      reading: readings.get(id),
      note: notes[id] as RegisterNote | undefined,
    }))
    // "Relevant" hides only what this pump's notes call absent, and registers without a note
    // that have no reading: nothing live disappears.
    .filter((r) =>
      view === "all" || (r.note ? r.note.relevance !== "absent" : r.reading !== undefined),
    )
    .filter((r) =>
      `${r.id} ${r.definition?.name || "unknown"} ${r.definition?.description || ""} ${r.definition?.verification || "unverified"} ${r.note?.label || ""}`
        .toLowerCase()
        .includes(search.toLowerCase()),
    );
  const lastPage = Math.max(0, Math.ceil(rows.length / PAGE_SIZE) - 1);
  const currentPage = Math.min(pageIndex, lastPage);
  const visibleRows = rows.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE);
  async function discover() {
    setBusy(true);
    setError("");
    try {
      const status = await api.discovery();
      setMessage(status.detail);
      refresh();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Panel title="Register catalog" action={<Badge>{rows.length} registers</Badge>}>
      <Toolbar>
        <Field label="Search registers" className="search-label">
          <input
            type="search"
            placeholder="Search name, ID or verification…"
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setPageIndex(0);
            }}
          />
        </Field>
        <Button variant="secondary" disabled={busy} onClick={discover}>
          {busy ? "Collecting…" : "Collect passive observations"}
        </Button>
      </Toolbar>
      {hasNotes && (
        <FilterGroup
          id="register-show"
          label="Show"
          options={[
            { value: "relevant", label: "Relevant on this pump" },
            { value: "all", label: "All registers" },
          ]}
          value={view}
          onChange={(option) => {
            setShow(option);
            setPageIndex(0);
          }}
        />
      )}
      <FieldHelp>
        Passive discovery only. Observation and catalog metadata never grant write permission.
        {hasNotes &&
          " Notes for this pump describe what each register means here; they change nothing on the pump."}
      </FieldHelp>
      {message && <Notice>{message}</Notice>}
      {error && <Notice kind="error">{error}</Notice>}
      {overlayError && <Notice kind="error">{overlayError}</Notice>}
      <TableScroll label="Registers">
        <RegisterTable
          rows={visibleRows}
          overlay={overlay}
          ageOf={(r) => age(Math.max(0, d.now_ms - r.observed_at_ms + elapsed))}
        />
      </TableScroll>
      {!rows.length && <p role="status">No registers match your search.</p>}
      {rows.length > PAGE_SIZE && (
        <Pagination
          label="Register pages"
          page={currentPage}
          pageCount={lastPage + 1}
          total={rows.length}
          onPage={setPageIndex}
        />
      )}
    </Panel>
  );
}

export type RegisterRow = {
  id: number;
  definition?: RegisterDefinition;
  reading?: RegisterReading;
  note?: RegisterNote;
};

/** The explorer's table: one row per register, identity cell first. */
export function RegisterTable({
  rows,
  overlay = {},
  ageOf,
}: {
  rows: RegisterRow[];
  overlay?: Record<string, RegisterVerification>;
  ageOf: (reading: RegisterReading) => string;
}) {
  return (
    <Table>
      <thead>
        <tr>
          <th>Register</th>
          <th>Raw</th>
          <th>Decoded</th>
          <th>Verification</th>
          <th>Observation</th>
          <th>Age</th>
        </tr>
      </thead>
      <tbody>
        {rows.map(({ id, definition: def, reading: r, note }) => (
          <tr key={id}>
            <td>
              <RegisterSummary id={id} definition={def} reading={r} note={note} />
            </td>
            <Cell mono>{display(r?.raw)}</Cell>
            <Cell mono>
              {display(r?.decoded)} {def?.unit}
            </Cell>
            <td>
              <VerificationBadge state={def?.verification} />
              {overlay[id] && (
                <CellMeta data-testid={`evidence-${id}`}>
                  Saved: {human(overlay[id].state)} · {overlay[id].source} · {new Date(overlay[id].timestamp_ms).toLocaleString()}
                </CellMeta>
              )}
            </td>
            <td>
              {human(r?.observation || "catalog_known")}
              {r?.error && <CellMeta error>{r.error}</CellMeta>}
            </td>
            <td>{r ? ageOf(r) : "—"}</td>
          </tr>
        ))}
      </tbody>
    </Table>
  );
}

/**
 * Identity cell of a register: name, this pump's label, note badges, the note, then id/access and
 * source/catalog lines.
 */
export function RegisterSummary({
  id,
  definition: def,
  reading: r,
  note,
}: RegisterRow) {
  return (
    <>
      <CellTitle>{def ? human(def.name) : "Unknown register"}</CellTitle>
      {note?.label && <CellLabel>{note.label}</CellLabel>}
      {note && (note.display_verified || note.relevance !== "relevant") && (
        <NoteBadges note={note} data-testid={`note-${id}`} />
      )}
      {note?.note && <CellMeta>{note.note}</CellMeta>}
      <CellMeta>
        #{id} · {def?.access ? human(def.access) : "Unknown access"}
      </CellMeta>
      <CellMeta>
        {def?.source || r?.source || "Unknown source"}
        {def && ` · catalog ${def.catalog_version}`}
      </CellMeta>
    </>
  );
}

/** What this installation's notes say about a register, as badges. */
export function NoteBadges({ note, ...rest }: { note: RegisterNote; "data-testid"?: string }) {
  return (
    <BadgeRow {...rest}>
      {note.display_verified && <Badge tone="green" plain>Checked against the display</Badge>}
      {note.relevance === "suspicious" && <Badge tone="amber" plain>Suspicious</Badge>}
      {note.relevance === "unmaintained" && <Badge tone="amber" plain>Unmaintained</Badge>}
      {note.relevance === "absent" && <Badge plain>Not used on this pump</Badge>}
    </BadgeRow>
  );
}

/** Catalog verification state: green only when hardware verified. */
export function VerificationBadge({ state }: { state?: string }) {
  return (
    <Badge tone={state === "hardware_verified" ? "green" : "amber"}>{human(state || "unverified")}</Badge>
  );
}
