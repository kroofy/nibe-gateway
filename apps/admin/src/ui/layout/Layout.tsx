import type { ReactNode } from "react";
import { cx } from "../cx";

/** Two equal columns on desktop, one column under 760 px. Panels inside lose their bottom margin. */
export function Columns({ children }: { children: ReactNode }) {
  return <div className="two-columns">{children}</div>;
}

/** A wrapping row of buttons with an 8 px gap. `compact` drops the top margin (panel headers). */
export function ActionRow({ children, compact }: { children: ReactNode; compact?: boolean }) {
  return <div className={cx("actions", compact && "compact-actions")}>{children}</div>;
}

/** A field that grows plus trailing actions, bottom-aligned; stacks under 1100 px. */
export function Toolbar({ children }: { children: ReactNode }) {
  return <div className="toolbar">{children}</div>;
}

/** Filters above a chart: fields and segmented controls in a wrapping row. */
export function FilterBar({ children }: { children: ReactNode }) {
  return <div className="chart-filters">{children}</div>;
}
