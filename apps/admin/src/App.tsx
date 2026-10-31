import { useCallback, useEffect, useState } from "react";
import { api, ApiError, errorMessage, request } from "./api";
import type { AuthStatus } from "./api";
import { useDevice } from "./useDevice";
import { human } from "./lib/format";
import { pendingUpdate } from "./lib/pendingUpdate";
import { descriptions, icons, pageFromHash, pages, slugs } from "./navigation";
import type { Page } from "./navigation";
import { Access } from "./pages/Access";
import { AuthForm } from "./pages/AuthForm";
import { Controls } from "./pages/Controls";
import { Debug, Diagnostics } from "./pages/Diagnostics";
import { History } from "./pages/History";
import { Integrations } from "./pages/Integrations";
import { Overview } from "./pages/Overview";
import { Readings } from "./pages/Readings";
import { Registers } from "./pages/Registers";
import { Updates } from "./pages/Updates";
import { Wifi } from "./pages/Wifi";
import {
  AppShell, AuthLayout, Badge, Brand, Button, LiveStatus, NavItem, Notice, PageContent, PageFooter, PageHeading, Panel,
  Sidebar, SidebarCaption, SidebarFooter, SidebarNav, Topbar,
} from "./ui";

export default function App() {
  const [page, setPage] = useState<Page>(pageFromHash);
  const [auth, setAuth] = useState<AuthStatus | "checking" | "unavailable" | "error">("checking");
  const [authError, setAuthError] = useState("");
  const [version, setVersion] = useState("Unavailable");
  const checkAuth = useCallback(async () => {
    try {
      setAuth(await api.auth());
      setAuthError("");
    } catch (err) {
      if (err instanceof ApiError && [404, 501].includes(err.status)) setAuth("unavailable");
      else {
        setAuth("error");
        setAuthError(errorMessage(err));
      }
    }
  }, []);
  useEffect(() => {
    void checkAuth();
    window.addEventListener("session-expired", checkAuth);
    const changed = () => setPage(pageFromHash());
    window.addEventListener("hashchange", changed);
    return () => {
      window.removeEventListener("session-expired", checkAuth);
      window.removeEventListener("hashchange", changed);
    };
  }, [checkAuth]);
  const allowed = auth === "unavailable" || (typeof auth === "object" && auth.authenticated);
  const state = useDevice(allowed);
  useEffect(() => {
    if (!allowed) return;
    // /health reports the running release; it is what the updater checks too. Polled so an open tab
    // notices an update (a single-page app keeps running its old code until reloaded).
    const check = () =>
      void request<{ version: string }>("/health")
        .then((health) => setVersion(health.version))
        .catch(() => {});
    check();
    const timer = setInterval(check, 60_000);
    return () => clearInterval(timer);
  }, [allowed]);
  const outdated = version !== "Unavailable" && __UI_VERSION__ !== "unknown" && version !== __UI_VERSION__;
  // After an update this tab installed, load the new admin UI as soon as the gateway reports that version.
  const installing = pendingUpdate();
  useEffect(() => {
    if (outdated && installing?.version === version) location.reload();
  }, [outdated, installing?.version, version]);
  async function logout() {
    try {
      await api.logout();
      await checkAuth();
    } catch (err) {
      setAuthError(errorMessage(err));
    }
  }
  if (!allowed)
    return (
      <AuthLayout footnote="Local administration · No cloud account required">
        {installing && (
          <Notice>
            Installing version {installing.version}. The gateway restarts and is back in about a minute; sign in
            again then.
          </Notice>
        )}
        {auth === "checking" ? (
          <Panel title="Connecting to your gateway">
            <p role="status">Checking setup and session…</p>
          </Panel>
        ) : auth === "error" ? (
          <Panel title="Gateway unavailable">
            <Notice kind="error">{authError}</Notice>
            <Button onClick={checkAuth}>Try again</Button>
          </Panel>
        ) : typeof auth === "object" ? (
          <AuthForm setup={auth.setup_required} onSuccess={checkAuth} />
        ) : null}
      </AuthLayout>
    );
  const { device, stale, stream, elapsed, error, refresh } = state;
  return (
    <AppShell
      sidebar={
        <Sidebar>
          <Brand />
          <SidebarCaption>ADMINISTRATION</SidebarCaption>
          <SidebarNav label="Main navigation">
            {pages.map((name, index) => (
              <NavItem key={name} href={`#${slugs[index]}`} icon={icons[name]} current={page === name}>
                {name}
              </NavItem>
            ))}
          </SidebarNav>
          <SidebarFooter label="Local connection" detail="nibe-gateway.local" />
        </Sidebar>
      }
    >
      <Topbar trail="Gateway" current="Administration">
        <Badge tone={device?.mode === "simulated" ? "amber" : "neutral"}>
          {device ? human(device.mode) : "Connecting"}
        </Badge>
        {auth !== "unavailable" && (
          <Button variant="text" onClick={logout}>
            Sign out
          </Button>
        )}
      </Topbar>
      <PageContent>
        {outdated && (
          <Notice kind="warning">
            Gateway version {version} is installed; this page is still {__UI_VERSION__}.{" "}
            <Button onClick={() => location.reload()}>Reload</Button>
          </Notice>
        )}
        <PageHeading
          eyebrow="NIBE GATEWAY"
          title={page}
          description={descriptions[page]}
          aside={<LiveStatus live={stream === "Live"}>{stream}</LiveStatus>}
        />
        {auth === "unavailable" && (
          <Notice>
            Local development API · Authentication is not installed. Login and protected network setup are
            unavailable.
          </Notice>
        )}
        {authError && <Notice kind="error">{authError}</Notice>}
        {error && (
          <Notice kind="error">
            {error}{" "}
            <Button variant="text" onClick={refresh}>
              Retry connection
            </Button>
          </Notice>
        )}
        {device && stale && (
          <Notice kind="warning">
            Readings are stale. Last known values are shown; check device communication before relying on them.
          </Notice>
        )}
        {device?.last_error && <Notice kind="error">Device: {device.last_error}</Notice>}
        {!device && !error && <p role="status">Loading device state…</p>}
        {device && (
          <>
            {page === "Overview" && <Overview device={device} stale={stale} elapsed={elapsed} version={version} />}
            {page === "Live readings" && <Readings device={device} elapsed={elapsed} />}
            {page === "History" && <History />}
            {page === "Controls" && <Controls device={device} stale={stale} refresh={refresh} />}
            {page === "Register explorer" && <Registers device={device} elapsed={elapsed} refresh={refresh} />}
            {page === "Wi-Fi setup" && <Wifi />}
            {page === "Access" && <Access />}
            {page === "Integrations" && <Integrations />}
            {page === "Updates" && <Updates version={version} />}
            {page === "Diagnostics" && <Diagnostics device={device} />}
            {page === "Debug" && <Debug />}
          </>
        )}
        <PageFooter meta={<>Gateway v{version}</>}>Local-first. Your gateway stays in your home.</PageFooter>
      </PageContent>
    </AppShell>
  );
}
