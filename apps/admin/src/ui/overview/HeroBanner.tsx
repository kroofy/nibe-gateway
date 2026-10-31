import type { ReactNode } from "react";
import { Badge } from "../primitives/Badge";

/** Dark brand card at the top of the overview: mode badge, device model, one sentence, artwork. */
export function HeroBanner({
  badge,
  title,
  description,
  art = <PumpGlyph />,
}: {
  badge: ReactNode;
  title: ReactNode;
  description: ReactNode;
  art?: ReactNode;
}) {
  return (
    <div className="overview-banner">
      <div>
        <Badge tone="light">{badge}</Badge>
        <h2>{title}</h2>
        <p>{description}</p>
      </div>
      {art}
    </div>
  );
}

/** Line drawing of an indoor heat pump unit (display, vents, logo). Decorative. */
export function PumpGlyph() {
  return (
    <div className="pump-glyph" aria-hidden="true">
      <div />
      <i />
      <i />
      <i />
      <span>NIBE</span>
    </div>
  );
}
