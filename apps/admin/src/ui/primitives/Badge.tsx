import type { ReactNode } from "react";
import { cx } from "../cx";

export type BadgeTone = "neutral" | "green" | "amber" | "light";
export type BadgeProps = {
  children: ReactNode;
  /** `green` = good/current, `amber` = attention, `light` = on the dark hero banner. */
  tone?: BadgeTone;
  /** Keep the text's own casing (badges capitalise by default). */
  plain?: boolean;
  "data-testid"?: string;
};

export function Badge({ children, tone = "neutral", plain, ...rest }: BadgeProps) {
  return (
    <span className={cx("badge", tone !== "neutral" && tone, plain && "plain")} {...rest}>
      {children}
    </span>
  );
}

/** A wrapping row of badges, placed under a title. */
export function BadgeRow({ children, ...rest }: { children: ReactNode; "data-testid"?: string }) {
  return (
    <span className="badge-row" {...rest}>
      {children}
    </span>
  );
}

/** "Current" (green) or "Stale" (amber): the freshness of one observation. */
export function FreshnessBadge({ stale }: { stale: boolean }) {
  return <Badge tone={stale ? "amber" : "green"}>{stale ? "Stale" : "Current"}</Badge>;
}
