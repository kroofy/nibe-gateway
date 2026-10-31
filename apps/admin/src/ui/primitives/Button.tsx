import type { AnchorHTMLAttributes, ButtonHTMLAttributes } from "react";
import { cx } from "../cx";

export type ButtonVariant = "primary" | "secondary" | "text";
export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  /** `primary` is the filled brand action; `secondary` is outlined; `text` has no border. */
  variant?: ButtonVariant;
  /** `compact` is for row actions and dense toolbars (32 px instead of 44 px). */
  size?: "default" | "compact";
};

/**
 * The only button. Native `type` defaults stay native: a Button inside a form submits it unless
 * `type="button"` is given.
 */
export function Button({ variant = "primary", size = "default", className, ...props }: ButtonProps) {
  return (
    <button
      className={cx(
        variant === "secondary" && "secondary",
        variant === "text" && "text-button",
        size === "compact" && "compact",
        className,
      ) || undefined}
      {...props}
    />
  );
}

/** A link that looks like a small outlined button (downloads, exports). */
export function ButtonLink({ className, ...props }: AnchorHTMLAttributes<HTMLAnchorElement>) {
  return <a className={cx("button-link", className)} {...props} />;
}

/** A quiet, small link placed in a panel header ("All readings ↗"). */
export function InlineLink({ className, ...props }: AnchorHTMLAttributes<HTMLAnchorElement>) {
  return <a className={cx("inline-link", className)} {...props} />;
}
