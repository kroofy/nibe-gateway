import type { ReactNode, TdHTMLAttributes } from "react";
import { Button } from "../primitives/Button";

/**
 * Tables are native `table`/`thead`/`tbody` markup styled by the design system; these parts
 * cover the repeated patterns:
 * - `TableScroll` lets a wide table scroll sideways on phones.
 * - `Table compact` drops the wide first column used by the register explorer.
 * - `CellTitle`/`CellLabel`/`CellMeta` stack a title, a highlighted label and muted lines in a cell.
 * - `RowActionCell` right-aligns a compact button; `EmptyRow` spans all columns.
 */
/** Sideways-scrolling wrapper; focusable and named so keyboard users can scroll it too. */
export function TableScroll({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="table-scroll" tabIndex={0} role="region" aria-label={label}>
      {children}
    </div>
  );
}
export function Table({ compact, children }: { compact?: boolean; children: ReactNode }) {
  return <table className={compact ? "compact" : undefined}>{children}</table>;
}
export function CellTitle({ children }: { children: ReactNode }) {
  return <strong>{children}</strong>;
}
export function CellLabel({ children }: { children: ReactNode }) {
  return <small className="cell-label">{children}</small>;
}
export function CellMeta({ children, error, ...rest }: { children: ReactNode; error?: boolean; "data-testid"?: string }) {
  return (
    <small className={error ? "error-text" : undefined} {...rest}>
      {children}
    </small>
  );
}
export function Cell({ mono, wrap, className, ...props }: TdHTMLAttributes<HTMLTableCellElement> & { mono?: boolean; wrap?: boolean }) {
  return <td className={[mono && "mono", wrap && "wrap", className].filter(Boolean).join(" ") || undefined} {...props} />;
}
/** Header for a row-action column: empty to the eye, named for screen readers. */
export function ActionHeader({ children = "Actions" }: { children?: ReactNode }) {
  return (
    <th>
      <span className="visually-hidden">{children}</span>
    </th>
  );
}
export function RowActionCell({ children }: { children: ReactNode }) {
  return <td className="row-action">{children}</td>;
}
export function EmptyRow({ colSpan, children }: { colSpan: number; children: ReactNode }) {
  return (
    <tr>
      <td colSpan={colSpan}>{children}</td>
    </tr>
  );
}

/** "Page 2 of 12 · 576 matches" with previous/next buttons. */
export function Pagination({
  page,
  pageCount,
  total,
  onPage,
  label,
}: {
  /** Zero-based. */
  page: number;
  pageCount: number;
  total: number;
  onPage: (page: number) => void;
  label: string;
}) {
  return (
    <div className="pagination" aria-label={label}>
      <span>
        Page {page + 1} of {pageCount} · {total} matches
      </span>
      <div>
        <Button variant="secondary" disabled={page === 0} onClick={() => onPage(page - 1)}>
          Previous page
        </Button>
        <Button variant="secondary" disabled={page >= pageCount - 1} onClick={() => onPage(page + 1)}>
          Next page
        </Button>
      </div>
    </div>
  );
}
