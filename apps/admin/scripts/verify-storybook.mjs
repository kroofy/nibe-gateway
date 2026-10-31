// Opens every story of the built Storybook in Chromium and reports render errors, interaction
// (play) failures and accessibility violations reported by @storybook/addon-a11y.
//   pnpm --dir apps/admin build-storybook && node apps/admin/scripts/verify-storybook.mjs [out-dir]
// Writes one screenshot per story to out-dir (default: apps/admin/storybook-static/__verify).
import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdir, readFile, realpath, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../storybook-static/", import.meta.url));
const out = path.resolve(process.argv[2] || path.join(root, "__verify"));
await mkdir(out, { recursive: true });
let cli = execFileSync("which", ["playwright"], { encoding: "utf8" }).trim();
const target = (await readFile(cli, "utf8")).match(/target=(\S+)/)?.[1];
if (target) cli = path.resolve(path.dirname(cli), target);
const { chromium } = createRequire(await realpath(cli))("playwright");

const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml", ".woff2": "font/woff2" };
const server = createServer(async (req, res) => {
  const file = path.join(root, decodeURIComponent(new URL(req.url, "http://x").pathname));
  try {
    const data = await readFile(file.endsWith("/") ? path.join(file, "index.html") : file);
    res.writeHead(200, { "Content-Type": types[path.extname(file)] || "application/octet-stream" });
    res.end(data);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const base = `http://127.0.0.1:${server.address().port}`;
const index = JSON.parse(await readFile(path.join(root, "index.json"), "utf8"));
const stories = Object.values(index.entries).filter((e) => e.type === "story");
const browser = await chromium.launch();
const results = [];
try {
  for (const story of stories) {
    // Stories named "…phone…" are judged at the service-phone width the viewport global selects.
    const phone = /phone/i.test(story.id);
    const page = await browser.newPage({ viewport: phone ? { width: 390, height: 844 } : { width: 1280, height: 900 } });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("console", (m) => {
      if (m.type() === "error" && !/favicon|Failed to load resource/.test(m.text())) errors.push(m.text());
    });
    await page.addInitScript(() => {
      window.__finished = new Promise((resolve) => {
        const hook = () => {
          const channel = window.__STORYBOOK_ADDONS_CHANNEL__;
          if (!channel) return setTimeout(hook, 20);
          channel.on("storyFinished", resolve);
        };
        hook();
      });
    });
    await page.goto(`${base}/iframe.html?id=${story.id}&viewMode=story`);
    const finished = await Promise.race([
      page.evaluate(() => window.__finished),
      new Promise((resolve) => setTimeout(() => resolve({ status: "timeout" }), 20_000)),
    ]);
    await page.waitForTimeout(250);
    const a11y = (finished.reporters || []).find((r) => r.type === "a11y");
    const violations = (a11y?.result?.violations || []).map((v) => `${v.id} (${v.nodes.length})`);
    await page.screenshot({ path: path.join(out, `${story.id}.png`), fullPage: true });
    const status = finished.status;
    results.push({ id: story.id, title: `${story.title} / ${story.name}`, status, violations, errors });
    // "warning" = rendered and played, with a11y findings reported as todo (see preview.tsx).
    const flag = ["success", "warning"].includes(status) && !errors.length ? "ok  " : "FAIL";
    console.log(`${flag} ${story.title} / ${story.name}${violations.length ? `  a11y: ${violations.join(", ")}` : ""}${errors.length ? `  errors: ${errors.join(" | ").slice(0, 300)}` : ""}${!["success", "warning"].includes(status) ? `  status: ${status}` : ""}`);
    await page.close();
  }
} finally {
  await browser.close();
  server.close();
}
await writeFile(path.join(out, "results.json"), JSON.stringify(results, null, 2));
const failed = results.filter((r) => !["success", "warning"].includes(r.status) || r.errors.length);
const rules = {};
for (const r of results) for (const v of r.violations) { const id = v.split(" ")[0]; rules[id] = (rules[id] || 0) + 1; }
console.log("a11y rules (stories affected):", JSON.stringify(rules));
console.log(`\n${results.length} stories, ${failed.length} failed, ${results.filter((r) => r.violations.length).length} with a11y findings. Screenshots: ${out}`);
process.exit(failed.length ? 1 : 0);
