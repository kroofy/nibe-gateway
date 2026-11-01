import type {
  AccessPoint,
  PollLists,
  PollMode,
  Status,
  PersistedExplorer,
  CommandReceipt,
  DeviceSnapshot,
  DiscoveryStatus,
  NetworkStatus,
  ScanResponse,
  SessionStatus,
  SetupStatus,
  TemporaryLux,
  MqttUpdate,
  MqttView,
  HistoryResponse,
  RemoteStatus,
  AuditPage,
} from "./schema";

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}
export type ApiScope = "read" | "control";
export interface ApiKeyInfo {
  id: string;
  name: string;
  scope: ApiScope;
  created_at_ms: number;
  last_used_ms: number | null;
}
export interface CreatedApiKey extends ApiKeyInfo {
  token: string;
}
export interface SshKeyInfo {
  type: string;
  comment: string;
  fingerprint_sha256: string | null;
}
export interface SshStatus {
  enabled: boolean;
  keys: SshKeyInfo[];
}
export interface TailscaleStatus {
  enabled: boolean;
  backend_state: string;
  dns_name: string | null;
  ips: string[];
  key_expiry: string | null;
}
export async function request<T>(
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
  method?: "DELETE",
): Promise<T> {
  const response = await fetch(path, {
    method: method ?? (body === undefined ? "GET" : "POST"),
    credentials: "same-origin",
    cache: "no-store",
    headers: {
      ...headers,
      ...(method !== undefined ? { "X-Requested-With": "nibe-gateway" } : {}),
      ...(body === undefined
        ? {}
        : {
            "Content-Type": body instanceof Blob ? "application/octet-stream" : "application/json",
            "X-Requested-With": "nibe-gateway",
          }),
    },
    body: body === undefined ? undefined : body instanceof Blob ? body : JSON.stringify(body),
    signal: AbortSignal.timeout(body instanceof Blob ? 120_000 : 15_000),
  });
  const data = await response.json().catch(() => null);
  if (!response.ok) {
    if (
      response.status === 401 &&
      !["/api/v1/auth/login", "/api/v1/setup/admin"].includes(path)
    )
      window.dispatchEvent(new Event("session-expired"));
    throw new ApiError(
      response.status,
      data?.error?.code || "http_error",
      data?.error?.message || `Gateway request failed (${response.status})`,
    );
  }
  if (data === null)
    throw new ApiError(
      response.status,
      "invalid_response",
      "Gateway returned an invalid response.",
    );
  return data as T;
}

// Generated from the live WS02 + WS06 OpenAPI contract.
export type AuthStatus = SessionStatus;
export type WifiStatus = NetworkStatus;
export type WifiNetwork = AccessPoint;
export const api = {
  updates: () => request<UpdateStatus>("/api/v1/updates/status"),
  upload: (file: File, confirmDowngrade = false) => request<UpdateStatus>(`/api/v1/updates/upload?confirm_downgrade=${confirmDowngrade}`, file),
  status: () => request<Status>("/api/v1/status"),
  explorer: () => request<PersistedExplorer>("/api/v1/registers/explorer"),
  polls: () => request<PollLists>("/api/v1/registers/polls"),
  setPoll: (register: number, mode: PollMode) =>
    request<PollLists>("/api/v1/controls/register-poll", { register, mode }),
  remoteUpdates: () => request<RemoteStatus>("/api/v1/updates/remote"),
  checkRemoteUpdates: () => request<RemoteStatus>("/api/v1/updates/remote/check", {}),
  stageRemoteUpdate: (version: string) => request<UpdateStatus>("/api/v1/updates/remote/stage", {version}),
  activate: () => request("/api/v1/updates/activate", {}),
  rollback: () => request("/api/v1/updates/rollback", {}),
  restart: () => request("/api/v1/system/restart", {}),
  recovery: () => request("/api/v1/network/recovery", {}),
  developer: () => request<DeveloperStatus>("/api/v1/diagnostics/developer"),
  authorizeDiagnostics: (seconds: number) => request<DeveloperStatus>("/api/v1/diagnostics/developer", {seconds, confirmation: "AUTHORIZE_DIAGNOSTICS"}),
  support: () => request("/api/v1/diagnostics/support"),
  device: () => request<DeviceSnapshot>("/api/v1/device"),
  discovery: () => request<DiscoveryStatus>("/api/v1/discovery/scan", {}),
  temporaryLux: (mode: TemporaryLux) =>
    request<CommandReceipt>("/api/v1/controls/temporary-lux", { mode }),
  heatingOffset: (offset: number) =>
    request<CommandReceipt>("/api/v1/controls/heating-offset", { offset }),
  alarmReset: () => request<CommandReceipt>("/api/v1/controls/alarm-reset", {}),
  apiKeys: () => request<ApiKeyInfo[]>("/api/v1/access/api-keys"),
  audit: (before?: number) =>
    request<AuditPage>(`/api/v1/audit?limit=100${before === undefined ? "" : `&before=${before}`}`),
  mqtt: () => request<MqttView>("/api/v1/integrations/mqtt"),
  history: (registers: number[], from: number, to: number, resolution = "auto") =>
    request<HistoryResponse>(
      `/api/v1/history?registers=${registers.join(",")}&from=${Math.round(from)}&to=${Math.round(to)}&resolution=${resolution}&max_points=600`,
    ),
  saveMqtt: (settings: MqttUpdate) => request<MqttView>("/api/v1/integrations/mqtt", settings),
  createApiKey: (name: string, scope: ApiScope) =>
    request<CreatedApiKey>("/api/v1/access/api-keys", { name, scope }),
  revokeApiKey: (id: string) =>
    request<{ revoked: boolean }>(`/api/v1/access/api-keys/${encodeURIComponent(id)}`, undefined, {}, "DELETE"),
  changePassword: (current_password: string, new_password: string) =>
    request<{ changed: boolean }>("/api/v1/access/password", { current_password, new_password }),
  ssh: () => request<SshStatus>("/api/v1/access/ssh"),
  setSshKeys: (keys: string[]) => request<SshStatus>("/api/v1/access/ssh", { keys }),
  addSshKey: (key: string) => request<SshStatus>("/api/v1/access/ssh/add", { key }),
  removeSshKey: (fingerprint_sha256: string) =>
    request<SshStatus>("/api/v1/access/ssh/remove", { fingerprint_sha256 }),
  tailscale: () => request<TailscaleStatus>("/api/v1/access/tailscale"),
  tailscaleJoin: (auth_key: string, hostname?: string) =>
    request<TailscaleStatus>("/api/v1/access/tailscale/join", { auth_key, hostname: hostname || null }),
  tailscaleDisconnect: (logout: boolean) =>
    request<TailscaleStatus>("/api/v1/access/tailscale/disconnect", { logout }),
  setupStatus: () => request<SetupStatus>("/api/v1/setup/status"),
  auth: () => request<AuthStatus>("/api/v1/auth/session"),
  /** `token` is empty on the open setup AP, where the gateway does not need it. */
  setup: (password: string, token: string) =>
    request<AuthStatus>(
      "/api/v1/setup/admin",
      { password },
      token ? { "X-Setup-Token": token } : {},
    ),
  login: (password: string) =>
    request<AuthStatus>("/api/v1/auth/login", { password }),
  logout: () => request<unknown>("/api/v1/auth/logout", {}),
  wifi: () => request<WifiStatus>("/api/v1/network/status"),
  scanWifi: () => request<ScanResponse>("/api/v1/network/scan"),
  connectWifi: (ssid: string, password: string) =>
    request<WifiStatus>("/api/v1/network/connect", { ssid, password }),
};
export const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : "The request failed.";

export type UpdateStatus = {available: boolean; state: {phase?: string; active?: string; previous?: string; version?: string; outcome?: string} | null};
export type DeveloperStatus = {enabled: boolean; seconds_remaining: number; capture_available: boolean};
