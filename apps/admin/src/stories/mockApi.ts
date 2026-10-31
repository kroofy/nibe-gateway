// In-browser stand-in for the gateway API, used only by Storybook. Real page components call
// `fetch` and `EventSource` exactly as in production; this module answers them from fixtures.
import * as fx from "./fixtures";

export type MockResponse = { status?: number; body: unknown; delayMs?: number };
export type MockHandler = MockResponse | ((url: URL, body: unknown) => MockResponse);
/** Keys are `"GET /api/v1/device"` style (method + path, query ignored). */
export type MockApi = Record<string, MockHandler>;

const ok = (body: unknown): MockResponse => ({ body });
const fail = (status: number, code: string, message: string): MockResponse => ({
  status,
  body: { error: { code, message } },
});

/** A signed-in administrator against the recorded simulated gateway. */
export function simulatedApi(): MockApi {
  return {
    "GET /api/v1/auth/session": ok({ authenticated: true, setup_required: false }),
    "GET /api/v1/setup/status": ok(fx.setupStatus()),
    "GET /health": ok(fx.health()),
    "GET /api/v1/device": ok(fx.simulatedDevice()),
    "GET /api/v1/status": ok(fx.gatewayStatus()),
    "GET /api/v1/network/status": ok(fx.wifiStatus()),
    "GET /api/v1/network/scan": ok(fx.wifiScan()),
    "GET /api/v1/registers/explorer": fail(404, "unsupported", "route not found"),
    "GET /api/v1/access/api-keys": ok([]),
    "GET /api/v1/audit": ok({ entries: [], more: false }),
    "GET /api/v1/integrations/mqtt": ok(fx.mqttView()),
    "GET /api/v1/access/ssh": fail(503, "system_helper_unavailable", "system helper unavailable — needs a newer OS image"),
    "GET /api/v1/access/tailscale": fail(503, "system_helper_unavailable", "system helper unavailable — needs a newer OS image"),
    "GET /api/v1/updates/status": ok(fx.updatesUnavailable()),
    "GET /api/v1/updates/remote": ok(fx.remoteUpdates(false)),
    "POST /api/v1/updates/remote/check": ok(fx.remoteUpdates()),
    "GET /api/v1/diagnostics": ok(fx.diagnostics()),
    "GET /api/v1/diagnostics/developer": ok(fx.developer()),
    "GET /api/v1/history": (url) => {
      const registers = (url.searchParams.get("registers") ?? "").split(",").map(Number);
      return ok(fx.history(registers, Number(url.searchParams.get("from")), Number(url.searchParams.get("to"))));
    },
    "POST /api/v1/network/connect": (_, body) => {
      const { ssid, password } = body as { ssid: string; password: string };
      return ssid === "Home WiFi" && password === "correct-password"
        ? ok({ ...fx.wifiStatus(), state: "connected", ssid, setup_ssid: null })
        : fail(422, "rejected", "The network rejected the password. The setup network is active again.");
    },
    "POST /api/v1/controls/heating-offset": (_, body) =>
      ok({ accepted: true, detail: `Heating offset set to ${(body as { offset: number }).offset} and read back.` }),
    "POST /api/v1/controls/temporary-lux": ok({ accepted: true, detail: "Temporary Lux set and read back." }),
    "POST /api/v1/controls/alarm-reset": ok({ accepted: true, detail: "Alarm reset sent; alarm 45001 now reads 0." }),
    "POST /api/v1/discovery/scan": ok({ detail: "Collected 5 passive observations." }),
    "POST /api/v1/auth/login": (_, body) =>
      (body as { password: string }).password === "correct horse battery"
        ? ok({ authenticated: true, setup_required: false })
        : fail(401, "unauthorized", "Incorrect password."),
    "POST /api/v1/access/api-keys": (_, body) =>
      ok({ ...(body as object), id: "k3", created_at_ms: fx.NOW, last_used_ms: null, token: "ngw_3f9a1c27e4b84d0f9a6e2c51b7d08e46" }),
    "POST /api/v1/integrations/mqtt": (_, body) => ok({ ...fx.mqttConnected(), ...(body as object) }),
  };
}

/** The appliance on the owner's F730: hardware mode, notes, keys, Tailscale, MQTT, updater. */
export function hardwareApi(): MockApi {
  return {
    ...simulatedApi(),
    "GET /api/v1/device": ok(fx.hardwareDevice()),
    "GET /api/v1/network/status": ok({ ...fx.wifiStatus(), state: "connected", ssid: "Home WiFi", setup_ssid: null, simulated: false, ip_addresses: ["192.168.1.42"] }),
    "GET /api/v1/registers/explorer": ok(fx.explorer()),
    "GET /api/v1/access/api-keys": ok(fx.apiKeys()),
    "GET /api/v1/status": ok({ ...fx.gatewayStatus(), mode: "hardware", ip_addresses: ["192.168.1.42", "100.101.102.103"], register_leases: fx.registerLeases() }),
    "GET /api/v1/audit": ok({ entries: fx.auditEntries(), more: true }),
    "GET /api/v1/access/ssh": ok(fx.ssh()),
    "GET /api/v1/access/tailscale": ok(fx.tailscale()),
    "GET /api/v1/integrations/mqtt": ok(fx.mqttConnected()),
    "GET /api/v1/updates/status": ok(fx.updatesAvailable()),
    "GET /api/v1/updates/remote": ok(fx.remoteUpdates()),
    "GET /api/v1/diagnostics/developer": ok({ enabled: true, seconds_remaining: 241, capture_available: true }),
  };
}

export type StreamState = "live" | "reconnecting";

let installed: { fetch: typeof fetch; EventSource: typeof EventSource } | null = null;

/** Replaces `fetch` and `EventSource` until the returned cleanup runs. */
export function installMockApi(routes: MockApi, stream: StreamState = "live"): () => void {
  if (!installed) installed = { fetch: window.fetch, EventSource: window.EventSource };
  const original = installed;
  window.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, location.href);
    // Let Storybook's own requests (modules, assets) through.
    if (!/^\/(api|health|openapi\.json)/.test(url.pathname)) return original.fetch(input, init);
    const method = (init?.method ?? "GET").toUpperCase();
    const handler = routes[`${method} ${url.pathname}`];
    let payload: unknown = undefined;
    if (typeof init?.body === "string") payload = JSON.parse(init.body);
    const response: MockResponse = !handler
      ? { status: 404, body: { error: { code: "unsupported", message: `No story fixture for ${method} ${url.pathname}` } } }
      : typeof handler === "function"
        ? handler(url, payload)
        : handler;
    if (response.delayMs) await new Promise((resolve) => setTimeout(resolve, response.delayMs));
    return new Response(JSON.stringify(response.body), {
      status: response.status ?? 200,
      headers: { "Content-Type": "application/json" },
    });
  };
  class MockEventSource extends EventTarget {
    onopen: ((event: Event) => void) | null = null;
    onerror: ((event: Event) => void) | null = null;
    readyState = 0;
    constructor(public url: string) {
      super();
      setTimeout(() => {
        if (stream === "live") {
          this.readyState = 1;
          this.onopen?.(new Event("open"));
        } else {
          this.onerror?.(new Event("error"));
        }
      }, 0);
    }
    close() {
      this.readyState = 2;
    }
  }
  window.EventSource = MockEventSource as unknown as typeof EventSource;
  return () => {
    window.fetch = original.fetch;
    window.EventSource = original.EventSource;
  };
}
