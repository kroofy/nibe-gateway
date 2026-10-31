import type { ReactNode } from "react";
import { Icon } from "../primitives/Icon";
import type { IconName } from "../primitives/Icon";
import { StatusDot } from "../primitives/StatusDot";

/** Logo lock-up: the "N" tile and the NIBE / GATEWAY word mark. */
export function Brand() {
  return (
    <div className="brand">
      <span className="brand-symbol">N</span>
      <div>
        NIBE<span>GATEWAY</span>
      </div>
    </div>
  );
}

/**
 * Signed-in frame: `Sidebar` on the left (a top strip on phones) and a `Workspace` holding the
 * `Topbar` and the page `main`.
 */
export function AppShell({ sidebar, children }: { sidebar: ReactNode; children: ReactNode }) {
  return (
    <div className="app-shell">
      {sidebar}
      <div className="workspace">{children}</div>
    </div>
  );
}

/** Sidebar: Brand, `SidebarCaption`, `SidebarNav` of `NavItem`s, `SidebarFooter`. */
export function Sidebar({ children }: { children: ReactNode }) {
  return <aside className="sidebar">{children}</aside>;
}
export function SidebarCaption({ children }: { children: ReactNode }) {
  return <div className="nav-caption">{children}</div>;
}
export function SidebarNav({ label, children }: { label: string; children: ReactNode }) {
  return <nav aria-label={label}>{children}</nav>;
}
export function NavItem({
  href,
  icon,
  current,
  children,
}: {
  href: string;
  icon?: IconName;
  current?: boolean;
  children: ReactNode;
}) {
  return (
    <a href={href} aria-current={current ? "page" : undefined}>
      <Icon name={icon} />
      {children}
    </a>
  );
}
/** Connection summary pinned to the bottom of the sidebar (hidden on phones). */
export function SidebarFooter({ label, detail }: { label: ReactNode; detail: ReactNode }) {
  return (
    <div className="sidebar-foot">
      <StatusDot />
      {label}
      <span>{detail}</span>
    </div>
  );
}

/** Top bar: breadcrumb on the left, mode badge and session actions on the right. */
export function Topbar({ trail, current, children }: { trail: string; current: ReactNode; children?: ReactNode }) {
  return (
    <header className="topbar">
      <span>
        {`${trail} `}
        <span className="slash">/</span> <strong>{current}</strong>
      </span>
      <div>{children}</div>
    </header>
  );
}

/** Page content column (max 1410 px). Holds banners, `PageHeading`, the page and `PageFooter`. */
export function PageContent({ children }: { children: ReactNode }) {
  return <main id="main">{children}</main>;
}

/** Eyebrow, h1 title and description on the left; `aside` (usually LiveStatus) on the right. */
export function PageHeading({
  eyebrow,
  title,
  description,
  aside,
}: {
  eyebrow: ReactNode;
  title: ReactNode;
  description: ReactNode;
  aside?: ReactNode;
}) {
  return (
    <div className="page-heading">
      <div>
        <p className="eyebrow">{eyebrow}</p>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      {aside}
    </div>
  );
}

/** Pill showing the event-stream state ("Live", "Reconnecting"). */
export function LiveStatus({ live, children }: { live: boolean; children: ReactNode }) {
  return (
    <div className="live-status">
      <StatusDot tone={live ? "ok" : "warning"} />
      {children}
    </div>
  );
}

export function PageFooter({ children, meta }: { children: ReactNode; meta: ReactNode }) {
  return (
    <footer>
      {children}
      <span>{meta}</span>
    </footer>
  );
}

/** Signed-out frame: centred Brand, one Panel, footnote. */
export function AuthLayout({ children, footnote }: { children: ReactNode; footnote: ReactNode }) {
  return (
    <div className="auth-shell">
      <Brand />
      {children}
      <p className="auth-foot">{footnote}</p>
    </div>
  );
}
