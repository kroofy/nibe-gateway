import type { ReactNode } from "react";
import { cx } from "../cx";
import { Icon } from "../primitives/Icon";
import { Quantity } from "./Quantity";

/** Compact label → quantity rows, separated by hairlines (overview "Live snapshot"). */
export function SnapshotList({ children }: { children: ReactNode }) {
  return <div className="snapshot-list">{children}</div>;
}
export function SnapshotRow({ label, value, unit }: { label: ReactNode; value: ReactNode; unit?: ReactNode }) {
  return (
    <div>
      <span>{label}</span>
      <Quantity value={value} unit={unit} />
    </div>
  );
}

/** Large numbers with a caption and a left rule; three per row (two on phones). */
export function CounterGrid({ children }: { children: ReactNode }) {
  return <div className="counter-grid">{children}</div>;
}
export function Counter({ value, label }: { value: ReactNode; label: ReactNode }) {
  return (
    <div>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

/** Selectable list of Wi-Fi networks. */
export function NetworkList({ children }: { children: ReactNode }) {
  return <div className="network-list">{children}</div>;
}
export function NetworkOption({
  name,
  detail,
  selected,
  onSelect,
}: {
  name: ReactNode;
  detail: ReactNode;
  selected?: boolean;
  onSelect: () => void;
}) {
  return (
    <button type="button" className={cx("network", selected && "selected")} onClick={onSelect}>
      <Icon name="wifi" />
      <strong>{name}</strong>
      <span>{detail}</span>
    </button>
  );
}

/** Small closing remark under a page's content. */
export function QuietNote({ children }: { children: ReactNode }) {
  return <div className="quiet-note">{children}</div>;
}
