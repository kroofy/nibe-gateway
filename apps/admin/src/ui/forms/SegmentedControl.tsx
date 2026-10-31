import type { ReactNode } from "react";
import { Button } from "../primitives/Button";

export type SegmentedOption<T> = { value: T; label: ReactNode };

/**
 * One-of-many toggle built from buttons; the active option is filled. Give it either an
 * `aria-label` or the id of a visible label (`aria-labelledby`).
 */
export function SegmentedControl<T extends string | number>({
  options,
  value,
  onChange,
  ...labelling
}: {
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  "aria-label"?: string;
  "aria-labelledby"?: string;
}) {
  return (
    <div className="segmented" role="group" {...labelling}>
      {options.map((option) => {
        const active = option.value === value;
        return (
          <Button
            key={option.value}
            type="button"
            variant={active ? "primary" : "secondary"}
            className={active ? "active" : undefined}
            aria-pressed={active}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </Button>
        );
      })}
    </div>
  );
}

/** A visible label followed by a segmented control ("Show: Relevant | All"). */
export function FilterGroup<T extends string | number>({
  id,
  label,
  ...control
}: {
  id: string;
  label: ReactNode;
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="filter-group">
      <span id={id}>{label}</span>
      <SegmentedControl aria-labelledby={id} {...control} />
    </div>
  );
}
