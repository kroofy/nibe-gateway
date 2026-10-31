// Visual parity capture: every admin page at desktop and phone width against a fresh simulated
// gateway. Used to compare the UI before and after design-system refactors.
//   node tests/e2e/capture-pages.mjs <out-dir>
// Not part of the assertion suite (run.mjs); it only writes screenshots and a DOM text dump.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as pause } from "node:timers/promises";

const root = fileURLToPath(new URL("../../", import.meta.url));
const out = path.resolve(process.argv[2] || path.join(root, "tests/e2e/artifacts/pages"));
await mkdir(out, { recursive: true });
let cli = execFileSync("which", ["playwright"], { encoding: "utf8" }).trim();
const target = (await readFile(cli, "utf8")).match(/target=(\S+)/)?.[1];
if (target) cli = path.resolve(path.dirname(cli), target);
const require = createRequire(await realpath(cli));
const { chromium } = require("playwright");

const stateDir = await mkdtemp(path.join(os.tmpdir(), "nibe-ui-capture-"));
const socket = net.createServer();
socket.listen(0, "127.0.0.1");
await once(socket, "listening");
const port = socket.address().port;
await new Promise((resolve) => socket.close(resolve));
const baseURL = `http://127.0.0.1:${port}`;
const server = spawn(
  "cargo",
  ["run", "-q", "-p", "nibe-gateway", "--features", "dev", "--", "--device", "simulated",
    "--bind", `127.0.0.1:${port}`, "--data-dir", stateDir, "--admin-dir", path.join(root, "apps/admin/dist")],
  { cwd: root, stdio: ["ignore", "ignore", "inherit"] },
);
const pages = ["overview", "readings", "history", "controls", "registers", "wifi", "access",
  "integrations", "updates", "diagnostics", "debug"];
let browser;
try {
  for (let i = 0; i < 400; i++) {
    try { if ((await fetch(`${baseURL}/health`)).ok) break; } catch {}
    await pause(250);
  }
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ baseURL, viewport: { width: 1440, height: 1000 }, reducedMotion: "reduce" });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/");
  await page.getByRole("heading", { name: "Set up your gateway" }).waitFor();
  await page.screenshot({ path: path.join(out, "desktop-setup.png"), fullPage: true });
  const { setup_token } = JSON.parse(await readFile(path.join(stateDir, "setup-secrets.json"), "utf8"));
  const password = "simulated-admin-password";
  await page.getByLabel("Device setup token").fill(setup_token);
  await page.getByLabel("Administrator password", { exact: true }).fill(password);
  await page.getByLabel("Confirm password").fill(password);
  await page.getByRole("button", { name: "Create administrator" }).click();
  await page.getByRole("heading", { name: "Overview", exact: true, level: 1 }).waitFor();
  const dump = {};
  for (const [label, width, height] of [["desktop", 1440, 1000], ["mobile", 390, 844]]) {
    await page.setViewportSize({ width, height });
    for (const slug of pages) {
      await page.goto(`/#${slug}`);
      await page.waitForSelector("h1");
      await page.waitForLoadState("networkidle").catch(() => {});
      await pause(slug === "wifi" || slug === "integrations" || slug === "access" ? 900 : 500);
      // Mask values that tick on their own (ages, uptime, debug JSON) to keep diffs meaningful.
      await page.screenshot({
        path: path.join(out, `${label}-${slug}.png`),
        fullPage: true,
        mask: [page.locator("pre"), page.locator("td:last-child"), page.locator(".reading p"), page.locator(".facts dd")],
      });
      dump[`${label}-${slug}`] = await page.locator("#main").innerText();
    }
  }
  await writeFile(path.join(out, "text.json"), JSON.stringify(dump, null, 2));
  assert.deepEqual(errors, [], "browser errors");
  console.log(`captured ${pages.length * 2 + 1} screenshots in ${out}`);
} finally {
  await browser?.close();
  server.kill("SIGTERM");
  await rm(stateDir, { recursive: true, force: true });
}
