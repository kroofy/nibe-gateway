import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { api, ApiError, errorMessage } from "../api";
import type { WifiNetwork, WifiStatus } from "../api";
import { human } from "../lib/format";
import { ActionRow, Button, Columns, FactList, Field, FieldHelp, NetworkList, NetworkOption, Notice, Panel } from "../ui";

export function Wifi() {
  const [status, setStatus] = useState<WifiStatus | null>(null);
  const [networks, setNetworks] = useState<WifiNetwork[]>([]);
  const [ssid, setSsid] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [unavailable, setUnavailable] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => {
    let active = true;
    api
      .wifi()
      .then((next) => {
        if (active) setStatus(next);
      })
      .catch((err) => {
        if (active) {
          setUnavailable(err instanceof ApiError && [404, 501].includes(err.status));
          setError(errorMessage(err));
        }
      });
    return () => {
      active = false;
    };
  }, []);
  async function scan() {
    setBusy(true);
    setError("");
    try {
      setNetworks((await api.scanWifi()).networks);
      setMessage("Scan complete. Select a network or enter a hidden SSID.");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }
  async function connect(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    setMessage("Connecting and verifying the network…");
    try {
      const next = await api.connectWifi(ssid, password);
      setPassword("");
      setStatus(next);
      setMessage(
        next.state === "connected"
          ? "Connection verified. You can reconnect at nibe-gateway.local:8080."
          : "Connection attempt finished. Check the network state below.",
      );
    } catch (err) {
      setPassword("");
      setError(errorMessage(err));
      setMessage("");
      try {
        setStatus(await api.wifi());
      } catch {
        /* Preserve the last state if radio transition disconnects us. */
      }
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      {status?.simulated && (
        <Notice>
          Simulated network adapter · Connection results are synthetic; no radio settings are changed.
        </Notice>
      )}
      <Notice>
        Changing Wi-Fi can disconnect this page on the Pi’s single radio. If the setup connection drops, reconnect
        to your home network and open <a href="http://nibe-gateway.local:8080/">nibe-gateway.local:8080</a>.
        Failed setup should return to the secured setup network.
      </Notice>
      {unavailable ? (
        <Notice kind="warning">
          Wi-Fi provisioning is unavailable in this gateway build. NetworkManager integration is required.
        </Notice>
      ) : (
        <>
          {error && <Notice kind="error">{error}</Notice>}
          {message && <Notice>{message}</Notice>}
          <Columns>
            <Panel title="Network connection">
              <FactList
                items={[
                  ["State", status ? human(status.state) : "Checking…"],
                  ["Connected SSID", status?.ssid || "Not connected"],
                  ["IP addresses", status?.ip_addresses.join(", ") || "None assigned"],
                  ["Setup network", status?.setup_ssid || "Inactive"],
                ]}
              />
              {status?.last_error && <Notice kind="error">{status.last_error}</Notice>}
              <ActionRow>
                <Button variant="secondary" onClick={scan} disabled={busy}>
                  Scan networks
                </Button>
              </ActionRow>
              <NetworkList>
                {networks.map((network, index) => (
                  <NetworkOption
                    key={`${network.ssid}-${index}`}
                    name={network.ssid || "Hidden network"}
                    detail={`${network.signal}% · ${network.secured ? "Secured" : "Open"}`}
                    selected={ssid === network.ssid}
                    onSelect={() => setSsid(network.ssid)}
                  />
                ))}
              </NetworkList>
            </Panel>
            <Panel title="Join a network">
              <form onSubmit={connect}>
                <Field label="Network name (SSID)">
                  <input value={ssid} onChange={(e) => setSsid(e.target.value)} maxLength={32} required autoComplete="off" />
                </Field>
                <Field label="Wi-Fi password">
                  <input
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    autoComplete="off"
                    maxLength={63}
                  />
                </Field>
                <FieldHelp>
                  Credentials are sent only to this gateway and are never included in support reports.
                </FieldHelp>
                <Button disabled={busy || !ssid}>{busy ? "Please wait…" : "Connect and verify"}</Button>
              </form>
            </Panel>
          </Columns>
        </>
      )}
    </>
  );
}
