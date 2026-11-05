#!/usr/bin/env node
// Loads the built webview bundle (media/webview) in Chromium with a stand-in
// for VS Code's message API and a real daemon behind it, the way the
// extension's Hub forwards calls. The shared app must render the room.
// Needs `pnpm build` here and `cargo build -p polyph-cli`.
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";

// The test daemon gets a free port (never a stale daemon's default one) and
// commands wait until it has written daemon.json.
async function waitForFile(file) {
  for (let i = 0; i < 300; i++) {
    if (existsSync(file)) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${file} did not appear`);
}

const here = new URL("../..", import.meta.url).pathname;
const root = join(here, "../..");
const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const bin = process.env.POLYPH_BIN ?? join(root, "target/debug/polyph");
const home = mkdtempSync(join(tmpdir(), "polyph-e2e-webview-"));
const env = { ...process.env, POLYPH_HOME: home, POLYPH_KEYSTORE: "file", POLYPH_LANG: "en" };
const cli = (...a) => execFileSync(bin, a, { env, encoding: "utf8" });
writeFileSync(join(home, "config.toml"), "api_port = 0\n");
const daemon = spawn(bin, ["daemon", "run"], { env, stdio: "ignore" });
await waitForFile(join(home, "daemon.json"));
const types = {
  ".js": "text/javascript",
  ".css": "text/css",
  ".woff2": "font/woff2",
  ".html": "text/html",
};
const media = join(here, "media/webview");
const files = createServer((req, res) => {
  const p = req.url === "/" ? null : join(media, decodeURIComponent(req.url.split("?")[0]));
  try {
    const body = p
      ? readFileSync(p)
      : Buffer.from(
          `<!doctype html><html data-theme="dark"><head><link rel="stylesheet" href="/webview.css"></head>` +
            `<body><div id="root"></div><script>window.POLYPH_ASSET_BASE = "/";</script>` +
            `<script type="module" src="/webview.js"></script></body></html>`,
        );
    res.writeHead(200, {
      "content-type": types[extname(p ?? "index.html")] ?? "application/octet-stream",
    });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
});
let browser;
try {
  cli("config", "set", "name", "Ann");
  cli("host", "--lan", "--name", "e2e-webview");
  const port = JSON.parse(readFileSync(join(home, "daemon.json"), "utf8")).port;
  const token = readFileSync(join(home, "api.token"), "utf8").trim();
  await new Promise((ok) => files.listen(0, "127.0.0.1", ok));
  browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 360, height: 800 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  // The extension's side: calls go to the daemon's /rpc.
  await page.exposeFunction("__rpc", async (method, params) => {
    const r = await fetch(`http://127.0.0.1:${port}/rpc`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    });
    return r.json();
  });
  await page.addInitScript(() => {
    window.acquireVsCodeApi = () => ({
      postMessage: async (m) => {
        const reply = (d) => window.postMessage(d, "*");
        if (m.t === "ready") reply({ t: "state", state: "open" });
        if (m.t === "call") {
          const v = await window.__rpc(m.method, m.params);
          reply(
            v.error
              ? { t: "result", id: m.id, error: v.error }
              : { t: "result", id: m.id, result: v.result },
          );
        }
      },
    });
  });
  await page.goto(`http://127.0.0.1:${files.address().port}/`);
  await page.getByRole("heading", { name: "e2e-webview", level: 1 }).waitFor();
  if (process.argv.includes("--shots"))
    await page.screenshot({
      path: join(process.argv[process.argv.indexOf("--shots") + 1], "vscode-webview.png"),
    });
  if (errors.length) throw new Error(`page errors: ${errors.join("; ")}`);
  console.log("e2e webview: ok");
} catch (e) {
  console.error(e);
  process.exitCode = 1;
} finally {
  await browser?.close();
  files.close();
  try {
    cli("daemon", "stop");
  } catch {
    daemon.kill();
  }
  rmSync(home, { recursive: true, force: true });
  process.exit();
}
