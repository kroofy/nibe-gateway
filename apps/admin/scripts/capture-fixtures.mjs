// Records real API responses from a fresh simulated gateway as Storybook fixtures.
//   node scripts/capture-fixtures.mjs
// Writes src/stories/fixtures/simulated.json. Re-run after API changes; stories derive their
// variants (hardware mode, stale data, notes) from this recording in src/stories/fixtures.ts.
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, readFile, rm, writeFile, mkdir } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as pause } from "node:timers/promises";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const out = fileURLToPath(new URL("../src/stories/fixtures/", import.meta.url));
const stateDir = await mkdtemp(path.join(os.tmpdir(), "nibe-fixtures-"));
const socket = net.createServer();
socket.listen(0, "127.0.0.1");
await once(socket, "listening");
const port = socket.address().port;
await new Promise((resolve) => socket.close(resolve));
const base = `http://127.0.0.1:${port}`;
const server = spawn(
  "cargo",
  ["run", "-q", "-p", "nibe-gateway", "--features", "dev", "--", "--device", "simulated",
    "--bind", `127.0.0.1:${port}`, "--data-dir", stateDir],
  { cwd: root, stdio: ["ignore", "ignore", "inherit"] },
);
try {
  for (let i = 0; i < 400; i++) {
    try { if ((await fetch(`${base}/health`)).ok) break; } catch {}
    await pause(250);
  }
  const { setup_token } = JSON.parse(await readFile(path.join(stateDir, "setup-secrets.json"), "utf8"));
  const recorded = {};
  const open = async (p) => {
    const response = await fetch(`${base}${p}`);
    return { status: response.status, body: await response.json() };
  };
  recorded["GET /api/v1/setup/status"] = await open("/api/v1/setup/status");
  recorded["GET /api/v1/auth/session (setup)"] = await open("/api/v1/auth/session");
  const setup = await globalThis.fetch(`${base}/api/v1/setup/admin`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-Requested-With": "nibe-gateway", "X-Setup-Token": setup_token },
    body: JSON.stringify({ password: "simulated-admin-password" }),
  });
  const cookie = setup.headers.get("set-cookie").split(";")[0];
  const get = async (p) => {
    const response = await fetch(`${base}${p}`, { headers: { cookie } });
    return { status: response.status, body: await response.json() };
  };
  // Let the simulator produce a few observations first.
  await pause(3000);
  for (const p of ["/api/v1/auth/session", "/health", "/api/v1/device", "/api/v1/status", "/api/v1/network/status",
    "/api/v1/network/scan", "/api/v1/registers/explorer", "/api/v1/access/api-keys", "/api/v1/integrations/mqtt",
    "/api/v1/access/ssh", "/api/v1/access/tailscale", "/api/v1/updates/status", "/api/v1/diagnostics",
    "/api/v1/diagnostics/developer"]) {
    recorded[`GET ${p}`] = await get(p);
  }
  await mkdir(out, { recursive: true });
  await writeFile(path.join(out, "simulated.json"), `${JSON.stringify(recorded, null, 2)}\n`);
  console.log(`recorded ${Object.keys(recorded).length} responses`);
} finally {
  server.kill("SIGTERM");
  await rm(stateDir, { recursive: true, force: true });
}
