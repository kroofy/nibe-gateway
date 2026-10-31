import { useCallback, useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import type { UpdateStatus } from "../api";
import type { RemoteStatus } from "../schema";
import {
  ActionMessages, ActionRow, Badge, Button, CheckboxField, Columns, FactList, Field, FieldHelp, Panel, PanelDescription,
} from "../ui";

export function Updates({ version }: { version: string }) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [remote, setRemote] = useState<RemoteStatus | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [confirmDowngrade, setConfirmDowngrade] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const refresh = useCallback(() => api.updates().then(setStatus).catch(e => setError(errorMessage(e))), []);
  useEffect(() => { api.remoteUpdates().then(setRemote).catch(() => {}); }, []);
  useEffect(() => { void refresh(); const timer = setInterval(() => {void refresh();}, 5000); return () => clearInterval(timer); }, [refresh]);
  async function action(operation: () => Promise<unknown>, text: string, confirmation?: string) {
    if (confirmation && !window.confirm(confirmation)) return;
    setBusy(true); setError(""); setMessage("");
    try { await operation(); setMessage(text); await refresh(); }
    catch (e) { setError(errorMessage(e)); }
    finally { setBusy(false); }
  }
  const available = status?.available === true;
  const latest = remote?.latest;
  const offered = latest?.newer ? latest : null;
  return <>
    <ActionMessages error={error} message={message} />
    <Panel title="Installed software" action={<Badge>Local appliance</Badge>}>
      <FactList items={[["Gateway version", version], ["Update service", available ? "Available" : "Unavailable"], ["Update phase", status?.state?.phase || "Unknown"], ["Last outcome", status?.state?.outcome || "None"]]} />
      <ActionRow>
        <Button disabled={!available || busy || status?.state?.phase !== "staged"} onClick={() => void action(api.activate, "Activation accepted. The gateway restarts; sign in again to inspect the result.", "Activate this release and restart the gateway?")}>Activate staged release</Button>
        <Button variant="secondary" disabled={busy} onClick={() => void refresh()}>Refresh update status</Button>
      </ActionRow>
    </Panel>
    <Columns>
      <Panel title="Online updates" action={<Badge>{!remote ? "…" : !remote.enabled ? "Off" : offered ? `${offered.version} available` : remote.checked_at_ms ? "Up to date" : "Not checked"}</Badge>}>
        <PanelDescription>Checks for a newer official release only when you ask. A downloaded release goes through the same signature and compatibility checks as an uploaded one, and is staged but not activated.</PanelDescription>
        {remote && !remote.enabled && <FieldHelp>The update source is turned off on this gateway (<code>--update-repository none</code>).</FieldHelp>}
        {remote?.enabled && <FactList items={[
          ["Release source", remote.repository ? <a href={`https://github.com/${remote.repository}/releases`} target="_blank" rel="noreferrer noopener">{remote.repository}</a> : "None"],
          ["Latest release", latest ? <a href={latest.url} target="_blank" rel="noreferrer noopener">{latest.version}{latest.published_at ? ` (${new Date(latest.published_at).toLocaleDateString()})` : ""}</a> : remoteError(remote.error)],
          ["Download size", latest ? `${(latest.asset_size / 1048576).toFixed(1)} MiB` : "None"],
          ["Last checked", remote.checked_at_ms ? new Date(remote.checked_at_ms).toLocaleString() : "Never"],
        ]} />}
        <ActionRow>
          <Button variant="secondary" disabled={!remote?.enabled || busy} onClick={() => void action(async () => { const r = await api.checkRemoteUpdates(); setRemote(r); if (r.error) throw new Error(remoteError(r.error)); }, "Checked for updates.")}>Check for updates</Button>
          <Button disabled={!available || busy || !offered || remote?.downloading} onClick={() => { if (offered) void action(async () => { try { await api.stageRemoteUpdate(offered.version); } finally { setRemote(await api.remoteUpdates()); } }, `Release ${offered.version} downloaded, verified and staged. Activate when ready.`, `Download and stage ${offered.version} (${(offered.asset_size / 1048576).toFixed(1)} MiB)?`); }}>{offered ? `Download and stage ${offered.version}` : "Download and stage"}</Button>
        </ActionRow>
      </Panel>
      <Panel title="Upload a release">
        <PanelDescription>Upload a signed release. The updater verifies compatibility and stages it in the inactive slot. Activation keeps the previous release until the new one has been ready for at least 30 seconds and finished an error-free poll round (at most 10 minutes); otherwise it rolls back automatically.</PanelDescription>
        {!available && <FieldHelp>Updates are unavailable in this build. The updater service must be connected.</FieldHelp>}
        <Field label="Signed release (.tar)"><input type="file" accept=".tar" disabled={!available || busy} onChange={e => setFile(e.target.files?.[0] || null)} /></Field>
        <CheckboxField label="I explicitly allow installing an older signed version" checked={confirmDowngrade} disabled={!available || busy} onChange={setConfirmDowngrade} />
        <ActionRow>
          <Button disabled={!available || busy || !file} onClick={() => {if(file) void action(() => api.upload(file, confirmDowngrade), "Release verified and staged. Activate when ready.");}}>Upload and verify</Button>
        </ActionRow>
      </Panel>
    </Columns>
    <Panel title="Restart & recovery">
        <PanelDescription>Restarting ends your session. Rollback restores the previous application and keeps persistent settings. Network recovery opens the secured setup network and may disconnect this browser.</PanelDescription>
        <ActionRow>
          <Button variant="secondary" disabled={!available || busy} onClick={() => void action(api.restart, "Restart accepted. Sign in again after startup.", "Restart the gateway now?")}>Restart gateway</Button>
          <Button variant="secondary" disabled={!available || busy || !status?.state?.previous || status?.state?.phase !== "complete"} onClick={() => void action(api.rollback, "Rollback accepted. Sign in again to inspect readiness.", "Restore the previous application release?")}>Roll back release</Button>
          <Button variant="secondary" disabled={busy} onClick={() => void action(api.recovery, "Network recovery started. Reconnect to the secured setup network if disconnected.", "Start secured network recovery? This may disconnect your browser.")}>Start recovery</Button>
        </ActionRow>
    </Panel>
  </>;
}

function remoteError(code?: string | null): string {
  switch (code) {
    case "unreachable": return "The release server could not be reached";
    case "no_release": return "No published release";
    case "no_asset": return "The latest release has no signed gateway file";
    case "invalid_release": return "The latest release is malformed";
    case null: case undefined: return "Not checked";
    default: return "Unknown error";
  }
}
