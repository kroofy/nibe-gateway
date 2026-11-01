// Real-browser integration against a fresh simulated gateway. No mocked API responses.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, mkdir, readFile, rm, realpath } from "node:fs/promises";
import { createRequire } from "node:module";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as pause } from "node:timers/promises";

const root = fileURLToPath(new URL("../../", import.meta.url));
// Use the mise-provided Playwright instead of adding it to the embedded UI's deps.
const executable = execFileSync("which", ["playwright"], {
  encoding: "utf8",
}).trim();
let cli = executable;
const shim = await readFile(cli, "utf8");
const target = shim.match(/target=(\S+)/)?.[1];
if (target) cli = path.resolve(path.dirname(cli), target);
// npm installs the CLI as a symlink in bin/; resolve it so the package is found next to the real file.
cli = await realpath(cli);
const require = createRequire(cli);
const { chromium } = require("playwright");
const { expect: baseExpect } = require("playwright/test");
const expect = baseExpect.configure({ timeout: 10_000 });
const artifacts = path.join(root, "tests/e2e/artifacts");
await mkdir(artifacts, { recursive: true });
const stateDir = await mkdtemp(path.join(os.tmpdir(), "nibe-ui-e2e-"));
const socket = net.createServer();
socket.listen(0, "127.0.0.1");
await once(socket, "listening");
const port = socket.address().port;
await new Promise((resolve) => socket.close(resolve));
const baseURL = `http://127.0.0.1:${port}`;
const server = spawn(
  "cargo",
  [
    "run",
    "-p",
    "nibe-gateway",
    "--features",
    "dev",
    "--",
    "--device",
    "simulated",
    "--bind",
    `127.0.0.1:${port}`,
    "--data-dir",
    stateDir,
    "--admin-dir",
    path.join(root, "apps/admin/dist"),
  ],
  { cwd: root, stdio: ["ignore", "pipe", "pipe"] },
);
let logs = "";
server.stdout.on("data", (chunk) => {
  logs = (logs + chunk).slice(-16_000);
});
server.stderr.on("data", (chunk) => {
  logs = (logs + chunk).slice(-16_000);
});
let browser;
let page;
const browserErrors = [];
let steps = 0;
async function step(name, action) {
  await action();
  steps++;
  console.log(`PASS ${name}`);
}
async function navigate(name) {
  await page.getByRole("link", { name, exact: true }).click();
  await expect(
    page.getByRole("heading", { name, exact: true, level: 1 }),
  ).toBeVisible();
}
async function shot(name) {
  await page.screenshot({
    path: path.join(artifacts, `${name}.png`),
    fullPage: true,
  });
}
try {
  let ready = false;
  for (let attempt = 0; attempt < 240; attempt++) {
    if (server.exitCode !== null) throw new Error(`Gateway exited: ${logs}`);
    try {
      if ((await fetch(`${baseURL}/health`)).ok) {
        ready = true;
        break;
      }
    } catch {}
    await pause(250);
  }
  assert.ok(ready, `Gateway did not start: ${logs}`);
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    baseURL,
    viewport: { width: 1440, height: 1000 },
    reducedMotion: "reduce",
  });
  page = await context.newPage();
  page.on("pageerror", (err) => browserErrors.push(err.message));
  const securityErrors = [];
  page.on("console", message => { if (/Content Security Policy|Refused to|violates.*directive/i.test(message.text())) securityErrors.push(message.text()); });
  await step("security headers cover SPA, assets, authentication errors and missing routes", async () => {
    const home = await context.request.get("/");
    const html = await home.text();
    const asset = html.match(/src="([^"]+\.js)"/)?.[1];
    assert.ok(asset, "built script asset missing");
    for (const url of ["/", asset, "/health", "/openapi.json", "/api/v1/device", "/api/v1/absent"]) {
      const response = await context.request.get(url);
      const headers = response.headers();
      assert.ok(headers["content-security-policy"]?.includes("frame-ancestors 'none'"), url);
      assert.equal(headers["x-content-type-options"], "nosniff", url);
      assert.equal(headers["referrer-policy"], "no-referrer", url);
    }
  });
  const password = "simulated-admin-password";
  const { setup_token: token } = JSON.parse(
    await readFile(path.join(stateDir, "setup-secrets.json"), "utf8"),
  );
  await step(
    "fresh gateway requires device proof and administrator setup",
    async () => {
      assert.equal((await context.request.get("/api/v1/device")).status(), 401);
      await page.goto("/");
      await expect(
        page.getByRole("heading", { name: "Set up your gateway" }),
      ).toBeVisible();
      await shot("setup");
      await page.getByLabel("Device setup token").fill(token);
      await page
        .getByLabel("Administrator password", { exact: true })
        .fill(password);
      await page.getByLabel("Confirm password").fill("different-password");
      await page.getByRole("button", { name: "Create administrator" }).click();
      await expect(page.getByRole("alert")).toContainText(
        "Passwords do not match",
      );
      await page.getByLabel("Confirm password").fill(password);
      await page.getByRole("button", { name: "Create administrator" }).click();
      await expect(
        page.getByRole("heading", { name: "Overview", exact: true }),
      ).toBeVisible();
      const cookies = await context.cookies();
      assert.ok(
        cookies.find(
          (c) =>
            c.name === "nibe_session" && c.httpOnly && c.sameSite === "Strict",
        ),
      );
      assert.equal(
        await page.evaluate(() => Object.keys(localStorage).length),
        0,
      );
      const status = await (await context.request.get("/api/v1/status")).json();
      assert.equal(typeof status.uptime_s, "number");
      assert.deepEqual(status.ip_addresses, ["127.0.0.1"]);
      await expect(page.getByText("Not reported by API")).toHaveCount(0);
      await shot("overview-desktop");
    },
  );
  await step("logout, rejected login, and successful login", async () => {
    await page.getByRole("button", { name: "Sign out" }).click();
    await expect(
      page.getByRole("heading", { name: "Welcome back" }),
    ).toBeVisible();
    assert.equal((await context.request.get("/api/v1/device")).status(), 401);
    await page.getByLabel("Administrator password").fill("incorrect-password");
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page.getByRole("alert")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Welcome back" }),
    ).toBeVisible();
    await page.getByLabel("Administrator password").fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(
      page.getByRole("heading", { name: "Overview", exact: true }),
    ).toBeVisible();
  });
  const post = async (url, data = {}) => {
    const response = await context.request.post(url, {
      data,
      headers: { "X-Requested-With": "nibe-gateway" },
    });
    assert.ok(response.ok(), `${url} returned ${response.status()}`);
    return response.json();
  };
  await post("/api/v1/diagnostics/developer", {seconds: 300, confirmation: "AUTHORIZE_DIAGNOSTICS"});
  await step(
    "SSE refreshes readings on scenario change without navigation",
    async () => {
      await navigate("Live readings");
      const outdoor = page
        .locator(".reading")
        .filter({ hasText: "outdoor temperature" });
      await expect(outdoor).toContainText("7.5");
      await post("/__dev/scenario", { name: "cold_weather" });
      await expect(outdoor).toContainText("-15");
      await shot("live-readings");
      await post("/__dev/scenario", { name: "disconnect" });
      await expect(
        page.getByText("Readings are stale.", { exact: false }),
      ).toBeVisible();
      await expect(outdoor).toContainText("Stale");
      await shot("stale-readings");
      await post("/__dev/scenario", { name: "normal_heating" });
      await expect(outdoor).toContainText("7.5");
      await expect(
        page.getByText("Readings are stale.", { exact: false }),
      ).toHaveCount(0);
    },
  );
  await step(
    "controls stay disabled in default simulation policy",
    async () => {
      await navigate("Controls");
      await expect(
        page.getByRole("button", { name: "Set Temporary Lux" }),
      ).toBeDisabled();
      await expect(
        page.getByRole("button", { name: "Apply offset" }),
      ).toBeDisabled();
      const response = await context.request.post(
        "/api/v1/controls/heating-offset",
        {
          data: { offset: 1 },
          headers: { "X-Requested-With": "nibe-gateway" },
        },
      );
      assert.equal(response.status(), 403);
      assert.equal((await response.json()).error.code, "controls_disabled");
    },
  );
  await step(
    "register search, raw/decoded data, and passive discovery",
    async () => {
      await navigate("Register explorer");
      await page
        .getByRole("searchbox", { name: "Search registers" })
        .fill("outdoor");
      await expect(page.locator("tbody tr")).toHaveCount(1);
      const row = page.locator("tbody tr");
      await expect(row).toContainText("75");
      await expect(row).toContainText("7.5");
      await expect(row).toContainText("synthetic");
      await shot("registers");
      await page.getByRole("searchbox").fill("does-not-exist");
      await expect(
        page.getByText("No registers match your search."),
      ).toBeVisible();
      await page.getByRole("searchbox").fill("");
      await page
        .getByRole("button", { name: "Collect passive observations" })
        .click();
      await expect(
        page.getByRole("status").filter({ hasText: "passive" }),
      ).toBeVisible();
      assert.ok((await context.request.get("/api/v1/discovery/status")).ok());
    },
  );
  await step(
    "Wi-Fi scan, failed credentials fallback, verified connection",
    async () => {
      await navigate("Wi-Fi setup");
      await page.getByRole("button", { name: "Scan networks" }).click();
      await page.getByRole("button", { name: /Home WiFi/ }).click();
      await expect(page.getByLabel("Network name (SSID)")).toHaveValue(
        "Home WiFi",
      );
      await page.getByLabel("Wi-Fi password").fill("wrong-password");
      await page.getByRole("button", { name: "Connect and verify" }).click();
      await expect(page.getByRole("alert").first()).toContainText(
        "connection failed",
      );
      await expect(page.getByLabel("Wi-Fi password")).toHaveValue("");
      assert.equal(
        (await (await context.request.get("/api/v1/network/status")).json())
          .state,
        "ap",
      );
      await page.getByLabel("Wi-Fi password").fill("correct-password");
      await page.getByRole("button", { name: "Connect and verify" }).click();
      await expect(
        page.getByText("Connection verified.", { exact: false }),
      ).toBeVisible();
      assert.equal(
        (await (await context.request.get("/api/v1/network/status")).json())
          .state,
        "connected",
      );
      await shot("wifi-connected");
    },
  );
  await step(
    "updater absence never offers an executable update/restart",
    async () => {
      await navigate("Updates");
      await page.getByText("Install from a file").click();
      await expect(
        page.getByRole("button", { name: "Install from file" }),
      ).toBeDisabled();
      await expect(
        page.getByRole("button", { name: "Restart gateway" }),
      ).toBeDisabled();
      await expect(
        page.getByText("Updates are unavailable in this build.", {
          exact: false,
        }),
      ).toBeVisible();
      await expect(page.getByLabel("Allow installing an older version")).toBeVisible();
      await shot("updates");
    },
  );
  await step("history page renders chart controls and an empty state", async () => {
    await navigate("History");
    await expect(page.getByRole("group", { name: "Time range" })).toBeVisible();
    await page.getByLabel("Chart").selectOption({ label: "Hot water" });
    await expect(page.getByRole("heading", { name: "Hot water", exact: true })).toBeVisible();
    await expect(page.getByText("No recorded data in this range yet")).toBeVisible();
    await shot("history");
  });
  await step("MQTT integration settings (broker optional)", async () => {
    await navigate("Integrations");
    await expect(page.getByText("disabled", { exact: true }).first()).toBeVisible();
    const broker = process.env.NIBE_E2E_MQTT_PORT;
    if (!broker) return;
    await page.getByLabel("Enable MQTT").check();
    await page.getByLabel("Broker host").fill("127.0.0.1");
    await page.getByLabel("Port").fill(broker);
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page.getByText("Saved. The gateway reconnects on its own.")).toBeVisible();
    await expect(page.getByText("connected", { exact: true }).first()).toBeVisible({ timeout: 15000 });
    await shot("integrations-connected");
  });
  await step("audit log lists the changes made above, without secrets", async () => {
    await navigate("Audit log");
    await expect(page.getByRole("heading", { name: "Active leases" })).toBeVisible();
    await expect(page.getByText("No active leases.")).toBeVisible();
    const entries = page.getByTestId("audit-entry");
    await expect(entries.filter({ hasText: "Wi-Fi network changed" }).first()).toBeVisible();
    // The refused heating offset from the controls step is recorded as failed.
    await expect(entries.filter({ hasText: "Heating offset" }).first()).toContainText("Failed");
    await expect(page.getByText("correct-password")).toHaveCount(0);
    await expect(page.getByText("wrong-password")).toHaveCount(0);
    await page.getByRole("button", { name: "MQTT", exact: true }).click();
    await expect(page.getByText("No changes recorded yet.")).toBeVisible();
    await page.getByRole("button", { name: "All", exact: true }).click();
    await shot("audit-log");
  });
  await step("support report allowlist and authorized debug", async () => {
    await navigate("Diagnostics");
    const downloading = page.waitForEvent("download");
    await page.getByRole("button", { name: "Download support report" }).click();
    const download = await downloading;
    const report = await readFile(await download.path(), "utf8");
    const decoded = JSON.parse(report);
    assert.equal(decoded.format_version, 1);
    assert.ok(decoded.counters.polls > 0);
    for (const secret of [
      password,
      token,
      "correct-password",
      "Home WiFi",
      "nibe_session",
    ])
      assert.ok(!report.includes(secret));
    assert.ok(!Object.hasOwn(decoded, "serial"));
    await navigate("Debug");
    await expect(page.locator("pre")).toContainText("transport_errors");
  });
  await step(
    "mobile navigation and layouts have no page overflow",
    async () => {
      await page.setViewportSize({ width: 390, height: 844 });
      for (const name of [
        "Overview",
        "Live readings",
        "Register explorer",
        "Wi-Fi setup",
        "Updates",
      ]) {
        await navigate(name);
        assert.ok(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth,
          ),
          `${name} overflows mobile viewport`,
        );
        await shot(`mobile-${name.toLowerCase().replaceAll(" ", "-")}`);
      }
    },
  );
  await step(
    "expired session clears protected UI and requires login",
    async () => {
      await context.clearCookies();
      await page.goto("/#readings");
      await expect(
        page.getByRole("heading", { name: "Welcome back" }),
      ).toBeVisible();
      await expect(page.getByRole("navigation")).toHaveCount(0);
      assert.equal(
        (await context.request.get("/api/v1/diagnostics")).status(),
        401,
      );
      assert.deepEqual(browserErrors, []);
      assert.deepEqual(securityErrors, []);
    },
  );
  await step("replay explorer shows durable evidence with source and timestamp", async () => {
    const replayDir = path.join(stateDir, "replay");
    const listener = net.createServer(); listener.listen(0, "127.0.0.1"); await once(listener, "listening");
    const replayPort = listener.address().port;
    await new Promise(resolve => listener.close(resolve));
    const replay = spawn("cargo", ["run", "-p", "nibe-gateway", "--features", "dev", "--", "--device", "replay", "--bind", `127.0.0.1:${replayPort}`, "--data-dir", replayDir, "--admin-dir", path.join(root, "apps/admin/dist")], {cwd: root, stdio: "ignore"});
    const replayContext = await browser.newContext({baseURL: `http://127.0.0.1:${replayPort}`});
    try {
      let ready = false;
      for (let i = 0; i < 240; i++) {
        try { if ((await replayContext.request.get("/health")).ok()) {ready = true; break;} } catch {}
        await pause(250);
      }
      assert.ok(ready);
      const secret = JSON.parse(await readFile(path.join(replayDir, "setup-secrets.json"), "utf8"));
      const setup = await replayContext.request.post("/api/v1/setup/admin", {data: {password}, headers: {"X-Requested-With": "nibe-gateway", "X-Setup-Token": secret.setup_token}});
      assert.ok(setup.ok());
      const replayPage = await replayContext.newPage();
      replayPage.on("pageerror", err => browserErrors.push(err.message));
      replayPage.on("console", message => { if (/Content Security Policy|Refused to|violates.*directive/i.test(message.text())) securityErrors.push(message.text()); });
      await replayPage.goto("/#registers");
      await replayPage.getByRole("searchbox").fill("40004");
      const evidence = replayPage.getByTestId("evidence-40004");
      await expect(evidence).toContainText("Saved: observed");
      await expect(evidence).toContainText("replay:");
      const overlay = JSON.parse(await readFile(path.join(replayDir, "register-verification.json"), "utf8"));
      assert.ok(overlay["40004"].timestamp_ms > 0);
      assert.deepEqual(browserErrors, []);
      assert.deepEqual(securityErrors, []);
      await replayPage.screenshot({path: path.join(artifacts, "replay-persisted-evidence.png"), fullPage: true});
      // Register notes: admin PUT, then the explorer filters and badges them.
      const notes = await replayContext.request.put("/api/v1/registers/notes", {headers: {"X-Requested-With": "nibe-gateway"}, data: {mode: "replace", notes: [
        {register: 40004, relevance: "relevant", display_verified: true, label: "outdoor air at the pump", source: "e2e"},
        {register: 41256, relevance: "unmaintained", note: "Always 0 on this firmware.", source: "e2e"},
        {register: 47265, relevance: "absent", source: "e2e"},
      ]}});
      assert.equal(notes.status(), 200);
      await replayPage.reload();
      const filter = replayPage.getByRole("group", {name: "Show"});
      await expect(filter.getByRole("button", {name: "Relevant on this pump"})).toHaveAttribute("aria-pressed", "true");
      await replayPage.getByRole("searchbox").fill("40004");
      const outdoor = replayPage.locator("tbody tr").filter({hasText: "outdoor air at the pump"});
      await expect(outdoor).toHaveCount(1);
      await expect(replayPage.getByTestId("note-40004")).toContainText("Checked against the display");
      await replayPage.getByRole("searchbox").fill("41256");
      await expect(replayPage.getByTestId("note-41256")).toContainText("Unmaintained");
      await expect(replayPage.locator("tbody tr")).toContainText("Always 0 on this firmware.");
      await replayPage.getByRole("searchbox").fill("");
      await expect(outdoor).toHaveCount(1);
      await replayPage.screenshot({path: path.join(artifacts, "replay-register-notes.png"), fullPage: true});
      // Absent on this pump: hidden in the relevant view, shown under all registers.
      await replayPage.getByRole("searchbox").fill("47265");
      await expect(replayPage.getByText("No registers match your search.")).toBeVisible();
      await filter.getByRole("button", {name: "All registers"}).click();
      await expect(replayPage.getByTestId("note-47265")).toContainText("Not used on this pump");
      await replayPage.getByRole("searchbox").fill("");
      await replayPage.setViewportSize({width: 390, height: 844});
      await pause(300); // let the button colour transition finish
      await replayPage.screenshot({path: path.join(artifacts, "replay-register-notes-mobile.png"), fullPage: false});
      assert.deepEqual(browserErrors, []);
    } finally {
      await replayContext.close(); replay.kill("SIGTERM");
      await Promise.race([once(replay, "exit"), pause(5000)]);
      if (replay.exitCode === null) { replay.kill("SIGKILL"); await once(replay, "exit"); }
    }
  });
  console.log(
    `${steps} browser checks passed; screenshots: tests/e2e/artifacts/`,
  );
} catch (error) {
  if (page) await shot("failure").catch(() => {});
  throw error;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
  await Promise.race([once(server, "exit"), pause(5000)]);
  if (server.exitCode === null) {
    server.kill("SIGKILL");
    await once(server, "exit");
  }
  await rm(stateDir, { recursive: true, force: true });
}
