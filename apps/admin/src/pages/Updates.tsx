import { useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api, errorMessage, request } from "../api";
import type { UpdateStatus } from "../api";
import type { RemoteStatus } from "../schema";
import { clearPendingUpdate, pendingUpdate, rememberPendingUpdate } from "../lib/pendingUpdate";
import {
  ActionMessages, ActionRow, Badge, Button, CheckboxField, Disclosure, FactList, Field, FieldHelp, InlineLink, Notice,
  Panel, PanelDescription, ProgressBar,
} from "../ui";
import type { BadgeTone } from "../ui";

/** What the install flow on this page is doing right now. */
type Step = "downloading" | "uploading" | "installing" | "restarting" | null;

export function Updates({ version }: { version: string }) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [remote, setRemote] = useState<RemoteStatus | null>(null);
  const [step, setStep] = useState<Step>(null);
  const [target, setTarget] = useState("");
  const [slowRestart, setSlowRestart] = useState(false);
  const [pending] = useState(pendingUpdate);
  const [file, setFile] = useState<File | null>(null);
  const [confirmDowngrade, setConfirmDowngrade] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const refresh = useCallback(() => api.updates().then(setStatus).catch(e => setError(errorMessage(e))), []);
  useEffect(() => { api.remoteUpdates().then(setRemote).catch(() => {}); }, []);
  // The session ends when the gateway restarts, so stop polling protected routes once installing.
  useEffect(() => {
    if (step === "installing" || step === "restarting") return;
    void refresh();
    const timer = setInterval(() => { void refresh(); }, 5000);
    return () => clearInterval(timer);
  }, [refresh, step]);
  // Follow a download (here or in another tab) once a second.
  const following = step === "downloading" || Boolean(remote?.staging);
  useEffect(() => {
    if (!following) return;
    const timer = setInterval(() => { api.remoteUpdates().then(setRemote).catch(() => {}); }, 1000);
    return () => clearInterval(timer);
  }, [following]);
  // While restarting, wait for /health (no session needed) to report the new version, then load its UI.
  useEffect(() => {
    if (step !== "restarting") return;
    const started = Date.now();
    const timer = setInterval(() => {
      if (Date.now() - started > 180_000) setSlowRestart(true);
      void request<{ version: string; ready: boolean }>("/health")
        .then(health => { if (health.ready && health.version === target) location.reload(); })
        .catch(() => {});
    }, 2000);
    return () => clearInterval(timer);
  }, [step, target]);

  const state = status?.state;
  const available = status?.available === true;
  const latest = remote?.latest;
  const ready = state?.phase === "staged" ? state.version ?? null : null;
  const offered = latest?.newer ? latest : null;
  const installable = ready ?? offered?.version ?? null;
  const working = busy || step !== null || Boolean(remote?.staging);

  async function run(operation: () => Promise<unknown>, text = "") {
    setBusy(true); setError(""); setMessage("");
    try { await operation(); setMessage(text); }
    catch (e) { setError(errorMessage(e)); }
    finally { setBusy(false); }
  }
  async function activate(next: string) {
    setStep("installing");
    await api.activate();
    rememberPendingUpdate(next, version);
    setTarget(next);
    setStep("restarting");
  }
  function confirmInstall(next: string) {
    return window.confirm(
      `Install version ${next}?\n\nThe gateway restarts and is unavailable for about a minute; you will need to sign in again. ` +
      "If the new version does not run correctly, the current version is restored automatically.",
    );
  }
  function installOnline() {
    if (!installable || !confirmInstall(installable)) return;
    void run(async () => {
      try {
        if (!ready) {
          setStep("downloading");
          await api.stageRemoteUpdate(installable);
        }
        await activate(installable);
      } catch (e) { setStep(null); api.remoteUpdates().then(setRemote).catch(() => {}); throw e; }
    });
  }
  function installFile() {
    if (!file) return;
    const label = file.name.match(/(\d+\.\d+\.\d+[^\s]*?)\.tar$/)?.[1] ?? "from this file";
    if (!confirmInstall(label)) return;
    void run(async () => {
      try {
        setStep("uploading");
        const staged = await api.upload(file, confirmDowngrade);
        await activate(staged.state?.version ?? label);
      } catch (e) { setStep(null); throw e; }
    });
  }

  const summary = installable && step === null && (state?.phase === "complete" || state?.phase === "staged" || !state?.phase)
    ? { ...describe(state, available), badge: ready ? "Ready to install" : "Update available", tone: "amber" as BadgeTone }
    : describe(state, available);
  return <>
    <ActionMessages error={error} message={message} />
    {pending && step === null && <Outcome pending={pending} version={version} state={state} />}
    <Panel title="Software update" action={<Badge tone={summary.tone}>{summary.badge}</Badge>}>
      <FactList items={[
        ["Installed version", version],
        ["Status", summary.text],
        ...(remote?.enabled ? [["Latest version", latest
          ? <InlineLink href={latest.url} target="_blank" rel="noreferrer noopener">{latest.version}{latest.published_at ? ` · ${new Date(latest.published_at).toLocaleDateString()}` : ""}{latest.newer ? ` · ${mib(latest.asset_size)} MiB` : " · up to date"}</InlineLink>
          : remote.error ? remoteError(remote.error) : "Not checked yet"] as [string, ReactNode]] : []),
      ]} />
      <Progress step={step} remote={remote} target={installable ?? target} slow={slowRestart} />
      {!available && <FieldHelp>Updates are unavailable in this build. The update service is not connected.</FieldHelp>}
      <ActionRow>
        {installable && <Button disabled={!available || working} onClick={installOnline}>Install {installable}</Button>}
        {remote?.enabled && <Button variant="secondary" disabled={working} onClick={() => void run(async () => {
          const r = await api.checkRemoteUpdates(); setRemote(r);
          if (r.error) throw new Error(remoteError(r.error));
          return r;
        }, "")}>Check for updates</Button>}
      </ActionRow>
      {remote && !remote.enabled && <FieldHelp>Online updates are turned off on this gateway (<code>--update-repository none</code>); install from a file instead.</FieldHelp>}
      <Disclosure summary="Install from a file">
        <PanelDescription>For a signed release file (<code>nibe-gateway-x.y.z.tar</code>) you downloaded yourself. It is checked exactly like an online update.</PanelDescription>
        <Field label="Release file (.tar)"><input type="file" accept=".tar" disabled={!available || working} onChange={e => setFile(e.target.files?.[0] || null)} /></Field>
        <CheckboxField label="Allow installing an older version" checked={confirmDowngrade} disabled={!available || working} onChange={setConfirmDowngrade} />
        <ActionRow><Button variant="secondary" disabled={!available || working || !file} onClick={installFile}>Install from file</Button></ActionRow>
      </Disclosure>
      <Disclosure summary="Technical details">
        <FactList items={[
          ["Updater phase", state?.phase ?? "Unknown"],
          ["Last outcome", state?.outcome ?? "None"],
          ["Active slot", state?.active ?? "Unknown"],
          ["Release source", remote?.repository ?? "Off"],
          ["Last checked", remote?.checked_at_ms ? new Date(remote.checked_at_ms).toLocaleString() : "Never"],
        ]} />
        <ActionRow><Button variant="secondary" disabled={working} onClick={() => void refresh()}>Refresh</Button></ActionRow>
      </Disclosure>
    </Panel>
    <Panel title="Restart & recovery">
      <PanelDescription>Restarting ends your session. Restoring the previous version keeps your settings. Network recovery opens the open setup network, where an admin password is still required, and may disconnect this browser.</PanelDescription>
      <ActionRow>
        <Button variant="secondary" disabled={!available || working} onClick={() => { if (window.confirm("Restart the gateway now?")) void run(api.restart, "Restart accepted. Sign in again after startup."); }}>Restart gateway</Button>
        <Button variant="secondary" disabled={!available || working || !state?.previous || state?.phase !== "complete"} onClick={() => { if (window.confirm("Restore the previously installed version?")) void run(api.rollback, "Restore accepted. Sign in again when the gateway is back."); }}>Restore previous version</Button>
        <Button variant="secondary" disabled={working} onClick={() => { if (window.confirm("Start network recovery? This may disconnect your browser.")) void run(api.recovery, "Network recovery started. Reconnect to the setup network if disconnected."); }}>Start recovery</Button>
      </ActionRow>
    </Panel>
  </>;
}

function Progress({ step, remote, target, slow }: { step: Step; remote: RemoteStatus | null; target: string; slow: boolean }) {
  const download = remote?.staging;
  if (download?.phase === "downloading")
    return <ProgressBar label={`Downloading ${target}`} value={download.received_bytes} max={download.total_bytes} detail={`${mib(download.received_bytes)} of ${mib(download.total_bytes)} MiB`} />;
  if (download?.phase === "verifying") return <ProgressBar label="Checking the signature" />;
  switch (step) {
    case "downloading": return <ProgressBar label={`Downloading ${target}`} />;
    case "uploading": return <ProgressBar label="Uploading and checking the file" />;
    case "installing": return <ProgressBar label={`Installing ${target}`} />;
    case "restarting": return <>
      <ProgressBar label={`Restarting with ${target}`} detail="about a minute" />
      <FieldHelp>{slow
        ? "This is taking longer than usual. The gateway restores the previous version by itself if the new one does not start; reload this page in a minute."
        : "This page reloads by itself when the gateway is back. You will need to sign in again."}</FieldHelp>
    </>;
    default: return null;
  }
}

/** Result of an update this tab started, shown after the restart and the new sign-in. */
function Outcome({ pending, version, state }: { pending: { version: string; from: string }; version: string; state: UpdateStatus["state"] | undefined }) {
  const phase = state?.phase;
  const finished = phase === "complete" || phase === "rolled_back" || phase === "failed";
  useEffect(() => { if (finished) clearPendingUpdate(); }, [finished]);
  if (!state) return null;
  if (phase === "rolled_back" || phase === "failed" || (finished && version !== "Unavailable" && version !== pending.version))
    return <Notice kind="error">The update to {pending.version} did not start correctly, so version {version} was restored. Your settings are unchanged.</Notice>;
  if (phase === "complete") return <Notice>Updated from {pending.from} to {pending.version}.</Notice>;
  return <Notice>Updated to {pending.version}. The gateway is checking that it runs correctly; this takes up to 10 minutes, and the previous version is restored automatically if not.</Notice>;
}

function describe(state: UpdateStatus["state"] | undefined, available: boolean): { badge: string; tone: BadgeTone; text: string } {
  if (!available) return { badge: "Unavailable", tone: "neutral", text: "Update service unavailable" };
  switch (state?.phase) {
    case "staged": return { badge: "Ready to install", tone: "amber", text: `Version ${state.version} is downloaded and ready to install` };
    case "staging": return { badge: "Downloading", tone: "neutral", text: "Checking a new version" };
    case "switching": case "checking": return { badge: "Installing", tone: "neutral", text: "Installing" };
    case "probation": return { badge: "Checking", tone: "neutral", text: "Checking that the new version runs correctly (up to 10 minutes)" };
    case "rolling_back": return { badge: "Restoring", tone: "amber", text: "Restoring the previous version" };
    case "rolled_back": return { badge: "Restored", tone: "amber", text: "The last update did not start correctly; the previous version was restored" };
    case "failed": return { badge: "Needs attention", tone: "amber", text: "The last update failed; see Technical details" };
    case "complete": return { badge: "Installed", tone: "green", text: "Running normally" };
    default: return { badge: "Installed", tone: "green", text: "Running normally" };
  }
}

function mib(bytes: number): string {
  return (bytes / 1048576).toFixed(1);
}

function remoteError(code?: string | null): string {
  switch (code) {
    case "unreachable": return "The release server could not be reached";
    case "no_release": return "No published release";
    case "no_asset": return "The latest release has no signed gateway file";
    case "invalid_release": return "The latest release is malformed";
    case null: case undefined: return "Not checked yet";
    default: return "Unknown error";
  }
}
