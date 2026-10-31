import { useCallback, useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import type { MqttUpdate, MqttView } from "../schema";
import { ActionMessages, Badge, Button, CheckboxField, Columns, FactList, Field, Notice, Panel, PanelDescription } from "../ui";

/** MQTT bridge with Home Assistant discovery (ADR 0115). */
export function Integrations() {
  const [view, setView] = useState<MqttView | null>(null);
  const [form, setForm] = useState<MqttUpdate | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const load = useCallback(async (resetForm: boolean) => {
    try {
      const v = await api.mqtt();
      setView(v);
      if (resetForm) setForm({
        enabled: v.enabled, host: v.host, port: v.port, username: v.username ?? "", password: null,
        topic_prefix: v.topic_prefix, discovery: v.discovery, discovery_prefix: v.discovery_prefix,
        allow_commands: v.allow_commands,
      });
    } catch (e) { setError(errorMessage(e)); }
  }, []);
  useEffect(() => {
    void load(true);
    const timer = window.setInterval(() => void load(false), 5000);
    return () => window.clearInterval(timer);
  }, [load]);
  if (!view || !form) return error ? <Notice kind="error">{error}</Notice> : <p>Loading…</p>;
  const set = (patch: Partial<MqttUpdate>) => setForm({ ...form, ...patch });
  const status = view.status;
  const since = status.connected_since_ms ? new Date(status.connected_since_ms).toLocaleString() : null;
  return <>
    <ActionMessages error={error} message={message} />
    <Columns>
      <Panel title="MQTT broker" action={<Badge>{status.state}</Badge>}>
        <PanelDescription>Publishes every polled reading to an MQTT broker you run (for example Mosquitto next to Home Assistant). Off by default. Plain TCP on your network; no TLS yet.</PanelDescription>
        <form onSubmit={e => {
          e.preventDefault();
          setBusy(true); setError(""); setMessage("");
          api.saveMqtt(form)
            .then(v => { setView(v); set({ password: null }); setMessage("Saved. The gateway reconnects on its own."); })
            .catch(err => setError(errorMessage(err)))
            .finally(() => setBusy(false));
        }}>
          <CheckboxField label="Enable MQTT" checked={form.enabled} disabled={busy} onChange={enabled => set({ enabled })} />
          <Field label="Broker host"><input value={form.host} required={form.enabled} placeholder="192.168.1.10 or broker.lan" disabled={busy} onChange={e => set({ host: e.target.value })} /></Field>
          <Field label="Port"><input type="number" min={1} max={65535} value={form.port} disabled={busy} onChange={e => set({ port: Number(e.target.value) })} /></Field>
          <Field label="Username"><input value={form.username ?? ""} autoComplete="off" disabled={busy} onChange={e => set({ username: e.target.value })} /></Field>
          <Field label="Password"><input type="password" autoComplete="new-password" value={form.password ?? ""} disabled={busy}
            placeholder={view.password_set ? "Stored; leave empty to keep" : "None"} onChange={e => set({ password: e.target.value || null })} /></Field>
          {view.password_set && <Button type="button" variant="secondary" size="compact" disabled={busy} onClick={() => set({ password: "" })}>Clear stored password on save</Button>}
          <Field label="Topic prefix"><input value={form.topic_prefix} required disabled={busy} onChange={e => set({ topic_prefix: e.target.value })} /></Field>
          <CheckboxField label="Home Assistant discovery" checked={form.discovery} disabled={busy} onChange={discovery => set({ discovery })} />
          <Field label="Discovery prefix"><input value={form.discovery_prefix} required disabled={busy || !form.discovery} onChange={e => set({ discovery_prefix: e.target.value })} /></Field>
          <CheckboxField label="Accept commands over MQTT" checked={form.allow_commands} disabled={busy} onChange={allow_commands => set({ allow_commands })} />
          <Button disabled={busy}>Save</Button>
        </form>
      </Panel>
      <Panel title="Connection">
        <FactList items={[
          ["State", status.state],
          ["Broker", status.broker ?? "None"],
          ["Connected since", since ?? "Not connected"],
          ["Last error", status.last_error ?? "None"],
          ["Messages published", String(status.messages_published)],
          ["Commands received", String(status.commands_received)],
        ]} />
        <h3>Topics</h3>
        <FactList items={[
          ["Readings", <code key="r">{view.base_topic}/register/&lt;id&gt;</code>],
          ["Availability", <code key="a">{view.base_topic}/availability</code>],
          ["Status", <code key="s">{view.base_topic}/status</code>],
          ["Commands", view.available_commands.length
            ? <code key="c">{view.base_topic}/set/{"{"}{view.available_commands.join(",")}{"}"}</code>
            : "None enabled in the gateway configuration"],
          ["Register commands", <code key="g">{view.base_topic}/register/&lt;id&gt;/{"{"}set,set_raw,get,poll{"}"}</code>],
          ["Command results", <code key="x">{view.base_topic}/result</code>],
        ]} />
        <PanelDescription>
          Named commands go through the same checks as the Controls page. Register commands read or write any
          register by number, like the API, when "Accept commands" is on: <code>set</code> takes a scaled value,
          {" "}<code>set_raw</code> a raw integer, and <code>get</code> publishes the value on the register's topic, and <code>poll</code> takes
          {" "}<code>fast</code>, <code>slow</code> or <code>off</code> like the Register explorer.
          {" "}The part after <code>{view.topic_prefix}/</code> is this gateway's ID, from its hardware serial number.
        </PanelDescription>
      </Panel>
    </Columns>
  </>;
}
