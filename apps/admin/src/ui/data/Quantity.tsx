import type { ReactNode } from "react";

/** A number with a smaller unit after it: `48 °C`. Size comes from the containing component. */
export function Quantity({ value, unit }: { value: ReactNode; unit?: ReactNode }) {
  return (
    <strong>
      {value} <small>{unit}</small>
    </strong>
  );
}
