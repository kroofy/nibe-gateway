import type { ReactNode } from "react";
import { FreshnessBadge } from "../primitives/Badge";
import { Quantity } from "./Quantity";

/**
 * One live measurement. Parts, top to bottom:
 * `ReadingHeader` (name + FreshnessBadge), `Quantity` (big value + unit), `ReadingMeta`
 * (register and age), optional `ReadingError`. The shorthand props render that composition.
 */
export function ReadingCard({
  name,
  value,
  unit,
  stale,
  meta,
  error,
  children,
}: {
  name?: ReactNode;
  value?: ReactNode;
  unit?: ReactNode;
  stale?: boolean;
  meta?: ReactNode;
  error?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="reading">
      {children ?? (
        <>
          <ReadingHeader name={name} stale={!!stale} />
          <Quantity value={value} unit={unit} />
          <ReadingMeta>{meta}</ReadingMeta>
          {error && <ReadingError>{error}</ReadingError>}
        </>
      )}
    </div>
  );
}
export function ReadingHeader({ name, stale }: { name: ReactNode; stale: boolean }) {
  return (
    <div>
      <span>{name}</span>
      <FreshnessBadge stale={stale} />
    </div>
  );
}
export function ReadingMeta({ children }: { children: ReactNode }) {
  return <p>{children}</p>;
}
export function ReadingError({ children }: { children: ReactNode }) {
  return <p className="error-text">{children}</p>;
}
/** Two reading cards per row on desktop, one on phones. */
export function ReadingGrid({ children }: { children: ReactNode }) {
  return <div className="reading-grid">{children}</div>;
}
