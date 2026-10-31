import type { ReactNode } from "react";

/**
 * Panel is the card surface every page is built from. Compose it from its parts:
 *
 * ```tsx
 * <Panel>
 *   <PanelHeader title="SSH" action={<Badge>On</Badge>} />
 *   <PanelDescription>Key-only login…</PanelDescription>
 *   …body: FactList, DataTable, form…
 * </Panel>
 * ```
 *
 * or use the `title`/`action` shorthand, which renders the header for you.
 */
export function Panel({
  title,
  action,
  children,
}: {
  title?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <section className="panel">
      {title !== undefined && <PanelHeader title={title} action={action} />}
      {children}
    </section>
  );
}

/** Title (h2) on the left, one optional action or status badge on the right. */
export function PanelHeader({ title, action }: { title: ReactNode; action?: ReactNode }) {
  return (
    <div className="panel-heading">
      <h2>{title}</h2>
      {action}
    </div>
  );
}

/** Muted explanatory paragraph directly inside a panel. */
export function PanelDescription({ children }: { children: ReactNode }) {
  return <p>{children}</p>;
}
