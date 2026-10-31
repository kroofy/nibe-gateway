import { useCallback, useEffect, useState } from "react";
import { api, ApiError, errorMessage } from "../api";
import type { ApiKeyInfo, ApiScope, CreatedApiKey, SshStatus, TailscaleStatus } from "../api";
import { copyText } from "../lib/browser";
import {
  ActionHeader, ActionMessages, ActionRow, Badge, Button, Cell, Columns, Disclosure, EmptyRow, FactList, Field, Notice,
  OneTimeSecret, Panel, PanelDescription, RowActionCell, Table,
} from "../ui";

const when = (ms: number | null) => (ms ? new Date(ms).toLocaleString() : "Not since start");

export function Access() {
  const [keys, setKeys] = useState<ApiKeyInfo[]>([]);
  const [name, setName] = useState("");
  const [scope, setScope] = useState<ApiScope>("read");
  const [created, setCreated] = useState<CreatedApiKey | null>(null);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const refresh = useCallback(() => api.apiKeys().then(setKeys).catch(e => setError(errorMessage(e))), []);
  useEffect(() => { void refresh(); }, [refresh]);
  async function action(operation: () => Promise<unknown>, text: string) {
    setBusy(true); setError(""); setMessage("");
    try { await operation(); setMessage(text); await refresh(); }
    catch (e) { setError(errorMessage(e)); }
    finally { setBusy(false); }
  }
  return <>
    <ActionMessages error={error} message={message} />
    {created && (
      <OneTimeSecret
        title="Copy this key now; it is not shown again."
        value={created.token}
        onCopy={() => void copyText(created.token).then(ok => setMessage(ok ? "Key copied to the clipboard." : "Copy failed: select the key above and copy it by hand."))}
        onDone={() => setCreated(null)}
      />
    )}
    <Panel title="API keys" action={<Badge>{`${keys.length} / 16`}</Badge>}>
      <PanelDescription>Keys let scripts and integrations use the API with <code>Authorization: Bearer &lt;key&gt;</code>. <strong>Read</strong> keys see status, readings and the live stream. <strong>Control</strong> keys can also use the controls. Keys never reach setup, Wi-Fi, updates, debug or key management. Changing the admin password revokes all keys.</PanelDescription>
      <ApiKeyTable
        keys={keys}
        busy={busy}
        onRevoke={k => { if (window.confirm(`Revoke the key "${k.name}"? Anything using it stops working.`)) void action(() => api.revokeApiKey(k.id), `Key "${k.name}" revoked.`); }}
      />
      <form onSubmit={e => { e.preventDefault(); void action(async () => { setCreated(await api.createApiKey(name, scope)); setName(""); }, "Key created."); }}>
        <Field label="Name"><input value={name} maxLength={40} required placeholder="e.g. home-assistant" disabled={busy} onChange={e => setName(e.target.value)} /></Field>
        <Field label="Scope"><select value={scope} disabled={busy} onChange={e => setScope(e.target.value as ApiScope)}>
          <option value="read">Read</option><option value="control">Control</option>
        </select></Field>
        <Button disabled={busy || !name.trim()}>Create key</Button>
      </form>
    </Panel>
    <Panel title="Admin password">
      <PanelDescription>Changing the password signs out every session and revokes all API keys.</PanelDescription>
      <form onSubmit={e => { e.preventDefault(); void action(async () => { await api.changePassword(current, next); setCurrent(""); setNext(""); window.dispatchEvent(new Event("session-expired")); }, "Password changed. Sign in again."); }}>
        <Field label="Current password"><input type="password" autoComplete="current-password" value={current} required disabled={busy} onChange={e => setCurrent(e.target.value)} /></Field>
        <Field label="New password (12+ characters)"><input type="password" autoComplete="new-password" minLength={12} value={next} required disabled={busy} onChange={e => setNext(e.target.value)} /></Field>
        <Button disabled={busy || next.length < 12 || !current}>Change password</Button>
      </form>
    </Panel>
    <SystemAccess />
  </>;
}

/** API keys with their scope, creation and last use; each row can be revoked. */
export function ApiKeyTable({ keys, busy, onRevoke }: { keys: ApiKeyInfo[]; busy?: boolean; onRevoke: (key: ApiKeyInfo) => void }) {
  return (
    <Table compact>
      <thead><tr><th>Name</th><th>Scope</th><th>Created</th><th>Last used</th><ActionHeader /></tr></thead>
      <tbody>
        {keys.length === 0 && <EmptyRow colSpan={5}>No API keys.</EmptyRow>}
        {keys.map(k => <tr key={k.id}>
          <td>{k.name}</td><td>{k.scope}</td><td>{when(k.created_at_ms)}</td><td>{when(k.last_used_ms)}</td>
          <RowActionCell><Button variant="secondary" size="compact" disabled={busy} onClick={() => onRevoke(k)}>Revoke</Button></RowActionCell>
        </tr>)}
      </tbody>
    </Table>
  );
}

/** SSH keys and Tailscale via the root system helper (ADR 0111). */
export function SystemAccess() {
  const [ssh, setSsh] = useState<SshStatus | null>(null);
  const [tailscale, setTailscale] = useState<TailscaleStatus | null>(null);
  const [missing, setMissing] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [authKey, setAuthKey] = useState("");
  const [hostname, setHostname] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const refresh = useCallback(async () => {
    try {
      setSsh(await api.ssh());
      setTailscale(await api.tailscale());
      setMissing(false);
    } catch (e) {
      if (e instanceof ApiError && e.status === 503) setMissing(true);
      else setError(errorMessage(e));
    }
  }, []);
  useEffect(() => { void refresh(); }, [refresh]);
  async function action(operation: () => Promise<unknown>, text: string, confirmation?: string) {
    if (confirmation && !window.confirm(confirmation)) return;
    setBusy(true); setError(""); setMessage("");
    try { await operation(); setMessage(text); await refresh(); }
    catch (e) { setError(errorMessage(e)); }
    finally { setBusy(false); }
  }
  if (missing) {
    return <Panel title="SSH and Tailscale">
      <Notice kind="warning">Needs a newer OS image (system helper). Until then SSH keys and Tailscale are set with <code>nibe-provision.json</code> on the boot partition.</Notice>
    </Panel>;
  }
  const keyCount = ssh?.keys.length ?? 0;
  const joinForm = <form onSubmit={e => { e.preventDefault(); void action(async () => { await api.tailscaleJoin(authKey, hostname.trim() || undefined); setAuthKey(""); }, "Joined the tailnet."); }}>
    <Field label="Auth key"><input type="password" autoComplete="off" value={authKey} disabled={busy} placeholder="tskey-auth-…" onChange={e => setAuthKey(e.target.value)} /></Field>
    <Field label="Device name (optional)"><input value={hostname} disabled={busy} maxLength={63} placeholder="nibe-gateway" onChange={e => setHostname(e.target.value.toLowerCase())} /></Field>
    <Button disabled={busy || !authKey.startsWith("tskey-")}>Join tailnet</Button>
  </form>;
  return <>
    <ActionMessages error={error} message={message} />
    <Columns>
      <Panel title="SSH" action={<Badge>{ssh ? (ssh.enabled ? "On" : "Off") : "…"}</Badge>}>
        <PanelDescription>Key-only login for <code>nibe-admin</code>. Removing every key turns SSH off.</PanelDescription>
        <Table compact>
          <thead><tr><th>Key</th><th>Fingerprint</th><ActionHeader /></tr></thead>
          <tbody>
            {keyCount === 0 && <EmptyRow colSpan={3}>No keys. SSH is off.</EmptyRow>}
            {ssh?.keys.map((k, i) => <tr key={`${k.fingerprint_sha256}-${i}`}>
              <td>{k.comment || k.type}</td>
              <Cell wrap><code>{k.fingerprint_sha256 ?? "unreadable"}</code></Cell>
              <RowActionCell>{k.fingerprint_sha256 && <Button variant="secondary" size="compact" disabled={busy} onClick={() => void action(
                () => api.removeSshKey(k.fingerprint_sha256!), "Key removed.",
                keyCount === 1 ? "Remove the last key? This turns SSH off." : `Remove the key "${k.comment || k.type}"?`)}>Remove</Button>}</RowActionCell>
            </tr>)}
          </tbody>
        </Table>
        <form onSubmit={e => { e.preventDefault(); void action(async () => { await api.addSshKey(newKey.trim()); setNewKey(""); }, "Key added. SSH is on."); }}>
          <Field label="Add public key">
            <textarea rows={3} value={newKey} disabled={busy} placeholder="ssh-ed25519 AAAA… user@laptop" onChange={e => setNewKey(e.target.value)} />
          </Field>
          <Button disabled={busy || !newKey.trim() || newKey.trim().includes("\n")}>Add key</Button>
        </form>
      </Panel>
      <Panel title="Tailscale" action={<Badge>{tailscale?.backend_state ?? "…"}</Badge>}>
        <FactList items={[
          ["Device", tailscale?.dns_name || "Not joined"],
          ["Tailnet IP", tailscale?.ips.join(", ") || "None"],
          ["Key expiry", tailscale?.key_expiry ? new Date(tailscale.key_expiry).toLocaleDateString() : "None"],
        ]} />
        {tailscale?.backend_state === "Running"
          ? <Disclosure summary="Re-join with a new auth key">{joinForm}</Disclosure>
          : joinForm}
        <ActionRow>
          <Button variant="secondary" disabled={busy || tailscale?.backend_state !== "Running"} onClick={() => void action(() => api.tailscaleDisconnect(false), "Disconnected from the tailnet.", "Disconnect from the tailnet? Remote access over Tailscale stops until you reconnect.")}>Disconnect</Button>
          <Button variant="secondary" disabled={busy || !tailscale?.dns_name} onClick={() => void action(() => api.tailscaleDisconnect(true), "Logged out; this device left the tailnet.", "Log out? This device leaves the tailnet and needs a new auth key to rejoin.")}>Log out</Button>
        </ActionRow>
      </Panel>
    </Columns>
  </>;
}
