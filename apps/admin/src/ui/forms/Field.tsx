import { cloneElement, isValidElement, useId } from "react";
import type { ReactElement, ReactNode } from "react";
import { cx } from "../cx";

export type FieldProps = {
  /** Visible label; it also names the control for assistive technology. */
  label: ReactNode;
  /** The native control: input, select or textarea. */
  children: ReactNode;
  /** Help text rendered inside the label, under the control. */
  help?: ReactNode;
  className?: string;
};

/**
 * Label wrapper for one native control. Native inputs, selects and textareas are styled by the
 * design system's base layer, so they are used directly inside a Field.
 */
export function Field({ label, children, help, className }: FieldProps) {
  const helpId = useId();
  // Help sits inside the label visually but is kept out of the control's name and announced as
  // its description instead.
  const control =
    help && isValidElement(children)
      ? cloneElement(children as ReactElement<{ "aria-describedby"?: string }>, { "aria-describedby": helpId })
      : children;
  return (
    <label className={className}>
      {label}
      {control}
      {help && (
        <span className="field-help" id={helpId} aria-hidden="true">
          {help}
        </span>
      )}
    </label>
  );
}

/** A checkbox with its label on the same line. */
export function CheckboxField({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: ReactNode;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="inline">
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />{" "}
      {label}
    </label>
  );
}

/** Secondary explanatory text between form rows or under a panel's content. */
export function FieldHelp({ children, className }: { children: ReactNode; className?: string }) {
  return <p className={cx("field-help", className)}>{children}</p>;
}

/** A form section that stays collapsed until asked for ("Re-join with a new auth key"). */
export function Disclosure({ summary, children }: { summary: ReactNode; children: ReactNode }) {
  return (
    <details className="inline-form">
      <summary>{summary}</summary>
      {children}
    </details>
  );
}
