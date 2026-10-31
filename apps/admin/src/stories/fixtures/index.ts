// Storybook fixtures. `simulated.json` is recorded from a real simulated gateway
// (scripts/capture-fixtures.mjs); the hardware variants are built from the real F730 catalog in
// fixtures/register-catalogs/f730.json with plausible values, so stories show realistic density.
import recorded from "./simulated.json";
import f730 from "../../../../../fixtures/register-catalogs/f730.json";
import type {
  DeviceSnapshot, HistoryResponse, HistorySeries, Measurement, MqttView, PersistedExplorer, RegisterDefinition,
  RegisterNote, RegisterReading, RegisterValue, RemoteStatus, SetupStatus, Status,
} from "../../schema";
import type { ApiKeyInfo, SshStatus, TailscaleStatus, UpdateStatus, WifiStatus, DeveloperStatus } from "../../api";

type Recorded = Record<string, { status: number; body: unknown }>;
const rec = recorded as Recorded;
const body = <T,>(key: string) => structuredClone(rec[key].body) as T;

/** Fixed clock so ages and dates render the same on every run. */
export const NOW = Date.UTC(2026, 9, 26, 12, 0, 0);

export const simulatedDevice = (): DeviceSnapshot => {
  const d = body<DeviceSnapshot>("GET /api/v1/device");
  const shift = NOW - d.now_ms;
  d.now_ms = NOW;
  d.last_update_ms = d.last_update_ms ? d.last_update_ms + shift : null;
  d.registers.forEach((r) => (r.observed_at_ms += shift));
  d.measurements.forEach((m) => (m.observed_at_ms += shift));
  return d;
};
export const setupStatus = () => body<SetupStatus>("GET /api/v1/setup/status");
export const gatewayStatus = (): Status => ({ ...body<Status>("GET /api/v1/status"), uptime_s: 93_784, now_ms: NOW });
export const wifiStatus = () => body<WifiStatus>("GET /api/v1/network/status");
export const wifiScan = () => body<{ networks: { ssid: string; signal: number; secured: boolean }[] }>("GET /api/v1/network/scan");
export const mqttView = () => body<MqttView>("GET /api/v1/integrations/mqtt");
export const diagnostics = () => body<Record<string, number>>("GET /api/v1/diagnostics");
export const developer = () => body<DeveloperStatus>("GET /api/v1/diagnostics/developer");
export const updatesUnavailable = () => body<UpdateStatus>("GET /api/v1/updates/status");
export const health = () => body<{ version: string }>("GET /health");

// ---- Hardware (F730) -------------------------------------------------------------------------

type CatalogEntry = (typeof f730.registers)[number];
const definition = (e: CatalogEntry): RegisterDefinition => ({
  id: e.id,
  name: e.name,
  description: e.description ?? e.title,
  data_type: e.data_type as RegisterDefinition["data_type"],
  scale: 1 / e.scale,
  unit: e.unit,
  access: e.access === "read" ? "read_only" : "read_write",
  verification: [40004, 40013, 47011, 48132, 45001].includes(e.id) ? "hardware_verified" : "unverified",
  models: [f730.model],
  firmware: null,
  source: e.sources.join(", "),
  catalog_version: f730.catalog_version,
});

/** Plausible decoded values for the registers the appliance polls. */
const VALUES: Record<number, number> = {
  40004: 4.2, 40008: 34.6, 40012: 29.8, 40013: 49.5, 40014: 46.1, 40017: 41.3, 40018: 71.2, 40019: 27.4,
  40020: -6.1, 40025: 21.7, 40026: 3.9, 40033: 21.4, 40050: 196.2, 40079: 3.1, 40081: 2.9, 40083: 3.4,
  41848: 1824.6, 41850: 5279.3, 43005: -142, 43009: 35.1, 43108: 2, 43136: 54.3, 43141: 1.21, 45001: 0,
  47011: -1, 48132: 0,
};
const POLLED = Object.keys(VALUES).map(Number);

function value(id: number, scale: number): { raw: RegisterValue; decoded: RegisterValue } {
  const v = VALUES[id];
  const raw = Math.round(v * scale);
  return {
    raw: { type: "integer", value: raw } as RegisterValue,
    decoded: (Number.isInteger(v) ? { type: "integer", value: v } : { type: "decimal", value: v }) as RegisterValue,
  };
}

export function hardwareDevice({ stale = false, ageMs = 4_000 }: { stale?: boolean; ageMs?: number } = {}): DeviceSnapshot {
  const catalog = f730.registers.map(definition);
  const observedAt = NOW - ageMs;
  const registers: RegisterReading[] = f730.registers
    .filter((e) => POLLED.includes(e.id))
    .map((e) => ({
      id: e.id,
      ...value(e.id, e.scale),
      observed_at_ms: observedAt,
      source: "f730",
      observation: "confirmed_readable",
      error: null,
    }));
  const measurements: Measurement[] = registers.map((r) => {
    const def = catalog.find((d) => d.id === r.id)!;
    return {
      id: r.id,
      name: def.name,
      value: r.decoded,
      unit: def.unit,
      observed_at_ms: observedAt,
      age_ms: ageMs,
      stale,
      error: null,
    };
  });
  const sim = simulatedDevice();
  return {
    ...sim,
    mode: "hardware",
    identity: { model: "F730 Cu 3x400V", firmware: "9721", serial: null, source: "0xEE version reply", verification: "hardware_verified" } as DeviceSnapshot["identity"],
    connection: "connected",
    catalog,
    registers,
    measurements,
    controls_enabled: true,
    stale,
    stale_after_ms: 120_000,
    now_ms: NOW,
    last_update_ms: observedAt,
    diagnostics: { ...sim.diagnostics, polls: 48_211, updates: 48_102, timeouts: 7, malformed_frames: 1, commands_accepted: 14 },
  };
}

/** Per-installation register notes (ADR 0119), a few of each kind. */
export function registerNotes(): Record<string, RegisterNote> {
  const note = (relevance: RegisterNote["relevance"], extra: Partial<RegisterNote> = {}): RegisterNote => ({
    relevance, source: "owner", timestamp_ms: NOW - 86_400_000, ...extra,
  });
  return {
    40004: note("relevant", { label: "Outdoor (north wall)", display_verified: true }),
    40013: note("relevant", { label: "Hot water, top of tank", display_verified: true }),
    40033: note("suspicious", { label: "Room sensor", note: "Reads 21.4 while the hall thermometer shows 20.1." }),
    43108: note("relevant", { label: "Fan speed step", note: "The active speed step (1–4), not a percentage." }),
    41256: note("absent"),
    41257: note("absent"),
    41258: note("absent"),
    43124: note("unmaintained", { note: "Only valid while the compressor runs." }),
  };
}

export function explorer(): PersistedExplorer {
  return {
    verification_overlay: {
      40004: { state: "hardware_verified", source: "display photo", timestamp_ms: NOW - 2 * 86_400_000 },
    },
    register_notes: registerNotes(),
  } as unknown as PersistedExplorer;
}

// ---- Access, integrations, updates ------------------------------------------------------------

export const apiKeys = (): ApiKeyInfo[] => [
  { id: "k1", name: "home-assistant", scope: "read", created_at_ms: NOW - 9 * 86_400_000, last_used_ms: NOW - 40_000 },
  { id: "k2", name: "tablet-dashboard", scope: "control", created_at_ms: NOW - 2 * 86_400_000, last_used_ms: null },
];
export const ssh = (): SshStatus => ({
  enabled: true,
  keys: [
    { type: "ssh-ed25519", comment: "admin@laptop", fingerprint_sha256: "SHA256:3q2+7wF0nIu1t9m4a2bQ0bO8m7Yp1K0n6f8F7cXy2Jk" },
    { type: "ssh-ed25519", comment: "", fingerprint_sha256: "SHA256:Zt9k1n2Lw8b0o2ZpQ1x3a8m7Yp1K0n6f8F7cXyAbCdE" },
  ],
});
export const tailscale = (running = true): TailscaleStatus =>
  running
    ? { enabled: true, backend_state: "Running", dns_name: "nibe-gateway.tail1234.ts.net.", ips: ["100.101.102.103", "fd7a:115c:a1e0::1"], key_expiry: "2027-04-20T10:00:00Z" }
    : { enabled: true, backend_state: "NeedsLogin", dns_name: null, ips: [], key_expiry: null };
export const mqttConnected = (): MqttView => ({
  ...mqttView(),
  enabled: true,
  host: "192.168.1.10",
  username: "nibe",
  password_set: true,
  allow_commands: true,
  available_commands: ["heating_offset", "temporary_lux"],
  status: { state: "connected", broker: "192.168.1.10:1883", connected_since_ms: NOW - 3 * 3_600_000, last_error: null, messages_published: 18_342, commands_received: 3 },
});
export const updatesAvailable = (phase = "complete"): UpdateStatus => ({
  available: true,
  state: { phase, active: "b", previous: "0.1.22", version: "0.1.23", outcome: "committed after probation" },
});

export const remoteUpdates = (checked = true): RemoteStatus => ({
  enabled: true,
  repository: "kroofy/nibe-gateway",
  current_version: "0.1.23",
  downloading: false,
  checked_at_ms: checked ? Date.parse("2026-10-31T10:05:00Z") : null,
  error: null,
  latest: checked
    ? { version: "0.1.26", url: "https://github.com/kroofy/nibe-gateway/releases/tag/v0.1.26", published_at: "2026-10-31T09:00:00Z", asset_size: 9_437_184, newer: true }
    : null,
});

// ---- History ----------------------------------------------------------------------------------

const UNITS: Record<number, string> = {};
const NAMES: Record<number, string> = {};
for (const e of f730.registers) {
  UNITS[e.id] = e.unit ?? "";
  NAMES[e.id] = e.title;
}
const COUNTERS = new Set([41848, 41850]);

/** Deterministic synthetic series shaped like each register (daily cycles, counters rising). */
export function history(registers: number[], from: number, to: number, points = 96): HistoryResponse {
  const step = (to - from) / points;
  const series: HistorySeries[] = registers.map((id, index) => {
    const base = VALUES[id] ?? 10;
    const counter = COUNTERS.has(id);
    return {
      register: id,
      name: NAMES[id] ?? `Register ${id}`,
      unit: UNITS[id] || null,
      kind: counter ? "counter" : "gauge",
      resolution: step < 60_000 ? "raw" : step < 3_600_000 ? "15m" : "1h",
      bucket_s: Math.round(step / 1000),
      points: Array.from({ length: points }, (_, k) => {
        const t = from + k * step;
        const phase = (k / points) * Math.PI * 4 + index;
        const v = counter
          ? base - (points - k) * (0.6 + 0.4 * Math.abs(Math.sin(phase)))
          : base + Math.abs(base || 4) * 0.12 * Math.sin(phase) + Math.sin(k * 1.7 + index) * 0.25;
        const r = Math.round(v * 100) / 100;
        return { t, mean: r, min: r - 0.3, max: r + 0.3, last: r };
      }),
    };
  });
  return { from, to, series };
}
