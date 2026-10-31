import type { ReactNode } from "react";
import { cx } from "../cx";

export type MetricTone = "neutral" | "green" | "amber";

/**
 * Headline status card. Composed of `MetricLabel`, `MetricValue` and `MetricDetail`; the
 * `label`/`value`/`detail` props are a shorthand for exactly that composition.
 * `tone` colours the value: green = healthy, amber = attention.
 */
export function MetricCard({
  label,
  value,
  detail,
  tone = "neutral",
  children,
}: {
  label?: ReactNode;
  value?: ReactNode;
  detail?: ReactNode;
  tone?: MetricTone;
  children?: ReactNode;
}) {
  return (
    <section className={cx("metric", tone !== "neutral" && tone)}>
      {children ?? (
        <>
          <MetricLabel>{label}</MetricLabel>
          <MetricValue>{value}</MetricValue>
          <MetricDetail>{detail}</MetricDetail>
        </>
      )}
    </section>
  );
}
export function MetricLabel({ children }: { children: ReactNode }) {
  return <p>{children}</p>;
}
export function MetricValue({ children }: { children: ReactNode }) {
  return <strong>{children}</strong>;
}
export function MetricDetail({ children }: { children: ReactNode }) {
  return <span>{children}</span>;
}

/** Three metric cards across; one column on phones, where cards turn into label/value rows. */
export function MetricGrid({ children }: { children: ReactNode }) {
  return <div className="metric-grid">{children}</div>;
}
