import type { ReactNode } from "react";
import { cx } from "../cx";

export type NoticeKind = "info" | "warning" | "error";

/**
 * A full-width message. `error` is announced as an alert; `info` and `warning` as status.
 * Buttons and links may sit inline in the text.
 */
export function Notice({ children, kind = "info" }: { children: ReactNode; kind?: NoticeKind }) {
  return (
    <div role={kind === "error" ? "alert" : "status"} className={cx("notice", kind)}>
      {children}
    </div>
  );
}
