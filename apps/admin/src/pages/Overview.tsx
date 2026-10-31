import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import type { WifiStatus } from "../api";
import type { DeviceSnapshot, Status } from "../schema";
import { age, display, human } from "../lib/format";
import {
  Columns, FactList, HeroBanner, InlineLink, MetricCard, MetricGrid, Panel, QuietNote, SnapshotList, SnapshotRow,
} from "../ui";

export function Overview({
  device,
  stale,
  elapsed,
  version,
}: {
  device: DeviceSnapshot;
  stale: boolean;
  elapsed: number;
  version: string;
}) {
  const [gatewayStatus, setGatewayStatus] = useState<Status | null>(null);
  useEffect(() => {
    let active = true;
    const load = () => { void api.status().then(s => { if (active) setGatewayStatus(s); }).catch(() => {}); };
    load(); const timer = setInterval(load, 15000);
    return () => { active = false; clearInterval(timer); };
  }, []);
  const [network, setNetwork] = useState<WifiStatus | null>(null);
  const [networkError, setNetworkError] = useState("");
  useEffect(() => {
    let active = true;
    const load = () =>
      api
        .wifi()
        .then((next) => {
          if (active) {
            setNetwork(next);
            setNetworkError("");
          }
        })
        .catch((err) => {
          if (active) setNetworkError(errorMessage(err));
        });
    void load();
    const interval = window.setInterval(load, 15_000);
    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, []);
  return (
    <OverviewView
      device={device}
      stale={stale}
      elapsed={elapsed}
      version={version}
      status={gatewayStatus}
      network={network}
      networkError={networkError}
    />
  );
}

/** The overview's presentation, separate from its polling so stories can render any state. */
export function OverviewView({
  device: d,
  stale,
  elapsed,
  version,
  status,
  network,
  networkError,
}: {
  device: DeviceSnapshot;
  stale: boolean;
  elapsed: number;
  version: string;
  status: Status | null;
  network: WifiStatus | null;
  networkError: string;
}) {
  return (
    <>
      <HeroBanner
        badge={d.mode === "simulated" ? "SIMULATED DEVICE" : human(d.mode).toUpperCase()}
        title={d.identity?.model || "Device identity unavailable"}
        description={
          d.mode === "simulated"
            ? "A safe environment for gateway setup and testing."
            : "Connected through your local heat pump gateway."
        }
      />
      <MetricGrid>
        <MetricCard
          label="Device communication"
          value={human(d.connection)}
          detail={stale ? "Attention needed · stale readings" : "Receiving current observations"}
          tone={stale ? "amber" : "green"}
        />
        <MetricCard
          label="Network"
          value={
            networkError
              ? "Unavailable"
              : network
                ? network.state === "ap"
                  ? "Setup access point"
                  : human(network.state)
                : "Checking…"
          }
          detail={networkError || (network?.simulated ? "Simulated network adapter" : "Gateway network adapter")}
          tone={networkError || network?.state === "offline" ? "amber" : "neutral"}
        />
        <MetricCard
          label="Data freshness"
          value={stale ? "Stale" : "Current"}
          detail={
            d.last_update_ms
              ? `Last observation ${age(d.now_ms - d.last_update_ms + elapsed)} ago`
              : "No observations yet"
          }
          tone={stale ? "amber" : "green"}
        />
      </MetricGrid>
      <Columns>
        <Panel title="Live snapshot" action={<InlineLink href="#readings">All readings ↗</InlineLink>}>
          <SnapshotList>
            {d.measurements.slice(0, 4).map((m) => (
              <SnapshotRow key={m.id} label={human(m.name)} value={display(m.value)} unit={m.unit} />
            ))}
          </SnapshotList>
        </Panel>
        <Panel title="Gateway details">
          <FactList
            items={[
              ["Software", version],
              ["Device firmware", d.identity?.firmware],
              ["Identity verification", human(d.identity?.verification || "unverified")],
              ["Uptime", status ? `${status.uptime_s} s` : "Loading…"],
              ["IP addresses", status?.ip_addresses.join(", ") || "None assigned"],
              [
                "Controls",
                d.controls_enabled
                  ? d.mode === "hardware"
                    ? "Enabled (verified controls only)"
                    : "Simulation enabled"
                  : "Disabled until verified",
              ],
            ]}
          />
        </Panel>
      </Columns>
      <QuietNote>
        {d.mode === "simulated"
          ? "Synthetic readings and register IDs are test fixtures. They do not verify an F730 installation."
          : "Register observations do not grant write permission. Controls require physical verification."}
      </QuietNote>
    </>
  );
}
