import { cx } from "../cx";

/** A 7 px dot: green when healthy, amber when attention is needed. Decorative; pair it with text. */
export function StatusDot({ tone = "ok" }: { tone?: "ok" | "warning" }) {
  return <span className={cx("status-dot", tone === "warning" && "warning")} />;
}
