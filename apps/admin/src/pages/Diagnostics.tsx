import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, request } from "../api";
import type { DeveloperStatus } from "../api";
import type { DeviceSnapshot } from "../schema";
import { downloadJson } from "../lib/browser";
import { human } from "../lib/format";
import { Button, CodeBlock, Counter, CounterGrid, Notice, Panel, PanelDescription } from "../ui";

export function Diagnostics({ device: d }: { device: DeviceSnapshot }) {
  async function download() {
    try { downloadJson(await api.support(), "nibe-gateway-support.json"); }
    catch (error) { window.alert(errorMessage(error)); }
  }
  return (
    <>
      <Panel title="Communication counters">
        <CounterGrid>
          {Object.entries(d.diagnostics).map(([key, value]) => (
            <Counter key={key} value={value} label={human(key)} />
          ))}
        </CounterGrid>
      </Panel>
      <Panel title="Sanitized support report">
        <PanelDescription>
          Download communication counters, application version, and freshness state. Passwords, sessions, SSIDs,
          serial numbers, raw register data, and logs are excluded.
        </PanelDescription>
        <Button onClick={download}>Download support report</Button>
      </Panel>
    </>
  );
}

export function Debug() {
  const [data, setData] = useState<unknown>(null);
  const [lease, setLease] = useState<DeveloperStatus | null>(null);
  const [error, setError] = useState("");
  const refresh = useCallback(async () => {
    try { setData(await request("/api/v1/diagnostics")); setLease(await api.developer()); }
    catch(e) {setError(errorMessage(e));}
  }, []);
  useEffect(() => {void refresh(); const timer = setInterval(() => {void refresh();}, 1000); return () => clearInterval(timer);}, [refresh]);
  async function authorize(seconds: number) {
    try {setLease(await api.authorizeDiagnostics(seconds)); setError("");}
    catch(e) {setError(errorMessage(e));}
  }
  return <Panel title="Protected diagnostics">
    <PanelDescription>Explicitly authorize temporary verbose logging and capture export for five minutes. Authorization expires automatically and after restart. Command safety and authentication remain enforced.</PanelDescription>
    <PanelDescription>{lease?.enabled ? `Diagnostic authorization: ${lease.seconds_remaining} seconds remaining.` : "Temporary diagnostics are disabled."}</PanelDescription>
    <Button onClick={() => void authorize(300)}>Authorize diagnostics for 5 minutes</Button>
    <Button variant="secondary" disabled={!lease?.enabled} onClick={() => void authorize(0)}>Disable diagnostics</Button>
    <PanelDescription>Serial captures contain raw device data. Review them before sharing; they are excluded from support reports.</PanelDescription>
    {lease?.enabled && lease.capture_available && <a href="/api/v1/diagnostics/capture" download="serial.capture.jsonl">Export serial capture for replay</a>}
    {error && <Notice kind="error">{error}</Notice>}
    <CodeBlock>{data ? JSON.stringify(data, null, 2) : "Loading…"}</CodeBlock>
  </Panel>;
}
