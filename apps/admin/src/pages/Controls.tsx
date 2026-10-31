import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import type { DeviceSnapshot, LeaseStatus, TemporaryLux } from "../schema";
import { Button, Columns, Field, Notice, Panel } from "../ui";

export function Controls({
  device: d,
  stale,
  refresh,
}: {
  device: DeviceSnapshot;
  stale: boolean;
  refresh: () => void;
}) {
  // Kept as typed text: converting on every keystroke turned a lone "-" into 0, so "-2" became "02".
  const [offset, setOffset] = useState("0");
  const [lux, setLux] = useState<TemporaryLux>("one_time_increase");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState("");
  const [error, setError] = useState("");
  const [lease, setLease] = useState<LeaseStatus | null>(null);
  // The device snapshot changes on every event (including a lease restore), so this stays current.
  useEffect(() => {
    api.status().then((s) => setLease(s.heating_offset_lease ?? null), () => setLease(null));
  }, [d]);
  const hardware = d.mode === "hardware";
  const disabled = !d.controls_enabled || stale || busy;
  async function run(action: () => Promise<{ detail: string }>) {
    setBusy(true);
    setError("");
    setResult("");
    try {
      const receipt = await action();
      setResult(receipt.detail);
      refresh();
    } catch (err) {
      setError(`${errorMessage(err)} No automatic retry was made. Check the device before retrying.`);
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      {!d.controls_enabled ? (
        <Notice kind="warning">
          Controls disabled until hardware verified. No register writes are available.
        </Notice>
      ) : hardware ? (
        <Notice kind="warning">
          Hardware controls are enabled for owner-approved registers only. Each change is sent once and confirmed
          by the pump and a read-back; check the value in Registers before changing it again.
        </Notice>
      ) : (
        <Notice>
          Simulation-only controls are enabled. These limits do not authorize physical F730 writes.
        </Notice>
      )}
      {error && <Notice kind="error">{error}</Notice>}
      {result && <Notice>{result}</Notice>}
      <Columns>
        <Panel title="Temporary Lux">
          <p>
            {hardware
              ? "Extra hot water (register 48132). Needs 48132 in writable_registers."
              : "Extra hot water on the synthetic device."}
          </p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void run(() => api.temporaryLux(lux));
            }}
          >
            <Field label="Mode">
              <select value={lux} disabled={disabled} onChange={(e) => setLux(e.target.value as TemporaryLux)}>
                <option value="one_time_increase">One-time increase</option>
                <option value="three_hours">3 hours</option>
                <option value="six_hours">6 hours</option>
                <option value="twelve_hours">12 hours</option>
                <option value="off">Off</option>
              </select>
            </Field>
            <Button disabled={disabled}>Set Temporary Lux</Button>
          </form>
        </Panel>
        <Panel title="Heating offset">
          <p>{hardware ? "Heat offset S1 (register 47011), -10 to 10." : "Adjust the synthetic heating offset."}</p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void run(() => api.heatingOffset(Number(offset)));
            }}
          >
            <Field label="Offset">
              <input
                type="number"
                min="-10"
                max="10"
                value={offset}
                disabled={disabled}
                onChange={(e) => setOffset(e.target.value)}
                required
              />
            </Field>
            <Button disabled={disabled}>Apply offset</Button>
          </form>
          {lease && <LeaseLine lease={lease} />}
        </Panel>
        {hardware && (
          <Panel title="Alarm">
            <p>
              Acknowledge the pump&apos;s current alarm (register 45171), then read back the alarm number (45001).
            </p>
            <Button disabled={disabled} onClick={() => void run(() => api.alarmReset())}>
              Reset alarm
            </Button>
          </Panel>
        )}
      </Columns>
    </>
  );
}

/** Control lease (ADR 0123): a writer such as a planner holds the offset and the gateway restores it if the lease lapses. */
function LeaseLine({ lease }: { lease: LeaseStatus }) {
  const active = lease.heating_offset;
  const last = lease.last_restore;
  return (
    <p>
      {active
        ? `Leased by ${active.holder}: ${active.offset}, back to ${active.baseline} in ${lease.remaining_s ?? 0} s unless renewed.`
        : "No lease. Your next change stays until changed again."}
      {last &&
        ` Last restore: ${last.baseline} after ${last.holder}'s lease, ${new Date(last.at_ms).toLocaleString()}${last.ok ? "" : ` (failed: ${last.detail})`}.`}
    </p>
  );
}
