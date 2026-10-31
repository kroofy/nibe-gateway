import type { DeviceSnapshot } from "../schema";
import { age, display, human } from "../lib/format";
import { Badge, Panel, ReadingCard, ReadingGrid } from "../ui";

export function Readings({ device: d, elapsed }: { device: DeviceSnapshot; elapsed: number }) {
  return (
    <Panel title="Device measurements" action={<Badge>{d.measurements.length} readings</Badge>}>
      <ReadingGrid>
        {d.measurements.map((m) => (
          <ReadingCard
            key={m.id}
            name={human(m.name)}
            stale={m.stale || m.age_ms + elapsed > d.stale_after_ms}
            value={display(m.value)}
            unit={m.unit}
            meta={<>Register {m.id} · {age(m.age_ms + elapsed)} ago</>}
            error={m.error}
          />
        ))}
      </ReadingGrid>
      {!d.measurements.length && <p>No readings have been observed yet.</p>}
    </Panel>
  );
}
