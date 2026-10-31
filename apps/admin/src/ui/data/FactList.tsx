import type { ReactNode } from "react";

/** One term/value row. A missing value reads "Unavailable", never blank. */
export function Fact({ label, children }: { label: ReactNode; children?: ReactNode }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{children ?? "Unavailable"}</dd>
    </div>
  );
}

export type FactItem = [label: string, value: ReactNode];

/**
 * Label/value list (a `dl`). Pass `items` for the common case, or compose `<Fact>` children
 * for custom rows.
 */
export function FactList({ items, children }: { items?: FactItem[]; children?: ReactNode }) {
  return (
    <dl className="facts">
      {items?.map(([label, value]) => (
        <Fact key={label} label={label}>
          {value}
        </Fact>
      ))}
      {children}
    </dl>
  );
}
