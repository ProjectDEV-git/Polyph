#!/usr/bin/env node
// End-to-end test of the browser client against a real daemon: sign in with
// a one-time link, chat, add a card, open the pill, and check that the link
// works once and that other origins are refused.
//
// Needs: `cargo build -p polyph-cli`, `pnpm --filter @polyph/web build`, and
// Playwright with Chromium (resolved from NODE_PATH or node_modules).
// Usage: node scripts/e2e-web.mjs [--shots dir]
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";

// The test daemon gets a free port (never a stale daemon's default one) and
// commands wait until it has written daemon.json.
async function waitForFile(file) {
  for (let i = 0; i < 300; i++) {
    if (existsSync(file)) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${file} did not appear`);
}

const root = new URL("..", import.meta.url).pathname;
const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const bin = process.env.POLYPH_BIN ?? join(root, "target/debug/polyph");
const shots = process.argv.includes("--shots")
  ? process.argv[process.argv.indexOf("--shots") + 1]
  : null;
const home = mkdtempSync(join(tmpdir(), "polyph-e2e-"));
const env = {
  ...process.env,
  POLYPH_HOME: home,
  POLYPH_KEYSTORE: "file",
  POLYPH_LANG: "en",
  POLYPH_WEB_DIR: join(root, "apps/web/out"),
};
const cli = (...args) => execFileSync(bin, args, { env, encoding: "utf8" });
const fail = (m) => {
  throw new Error(m);
};

writeFileSync(join(home, "config.toml"), "api_port = 0\n");

const daemon = spawn(bin, ["daemon", "run"], { env, stdio: "ignore" });

await waitForFile(join(home, "daemon.json"));
let browser;
try {
  cli("config", "set", "name", "Ann");
  cli("host", "--lan", "--name", "e2e-room");
  const { url } = JSON.parse(cli("web", "--no-open", "--json"));
  const port = JSON.parse(readFileSync(join(home, "daemon.json"), "utf8")).port;
  const token = readFileSync(join(home, "api.token"), "utf8").trim();

  // Another site's page may not use the API, even with the token.
  const evil = await fetch(`http://127.0.0.1:${port}/rpc`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      origin: "https://evil.example",
      "content-type": "application/json",
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "ping" }),
  });
  if (evil.status !== 403) fail(`foreign origin got ${evil.status}`);
  // A rebound DNS name may not load the client.
  // (fetch drops a custom Host header, so ask with node:http.)
  const rebound = await new Promise((ok, no) =>
    http
      .get(
        { host: "127.0.0.1", port, path: "/app/", headers: { host: `evil.example:${port}` } },
        (r) => ok(r.statusCode),
      )
      .on("error", no),
  );
  if (rebound !== 403) fail(`foreign host got ${rebound}`);

  browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto(url);
  await page.getByRole("heading", { name: "e2e-room", level: 1 }).waitFor();
  if (page.url().includes("code=")) fail("the one-time code stayed in the address bar");

  await page.getByTestId("composer").fill("hello from the browser");
  await page.getByTestId("composer").press("Enter");
  await page.getByText("hello from the browser").waitFor();
  // What the browser wrote is the room's chat for every surface.
  if (!cli("--json", "activity").length) fail("cli");

  await page.getByRole("button", { name: "Board", exact: true }).click();
  await page.getByRole("button", { name: "Add a card" }).click();
  await page.getByLabel("Card title").fill("Ship the browser client");
  await page.getByLabel("Card title").press("Enter");
  await page.getByTestId("card").filter({ hasText: "Ship the browser client" }).waitFor();
  if (shots) await page.screenshot({ path: join(shots, "e2e-board.png") });

  const pill = await ctx.newPage();
  await pill.goto(`http://127.0.0.1:${port}/pill/`);
  await pill.getByTestId("pill").getByText("e2e-room").waitFor();

  const again = await (await browser.newContext()).newPage();
  await again.goto(url);
  await again.getByText("This link has expired or was already used").waitFor();

  if (errors.length) fail(`page errors: ${errors.join("; ")}`);
  console.log("e2e web: ok");
} catch (e) {
  console.error(e);
  process.exitCode = 1;
} finally {
  await browser?.close();
  try {
    cli("daemon", "stop");
  } catch {
    daemon.kill();
  }
  rmSync(home, { recursive: true, force: true });
  // Keep-alive sockets from fetch would hold the process open.
  process.exit();
}
