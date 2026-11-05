#!/usr/bin/env node
// End-to-end test of the phone app: a daemon hosts a room on a plain relay,
// `polyph phone` makes a pairing link, and Chromium at phone size opens it.
// The phone runs the room engine in wasm, joins through the relay, chats
// both ways, moves a card, survives a reload (state sealed in IndexedDB) and
// offers its manifest and service worker.
//
// Needs: `cargo build -p polyph-cli`, `pnpm --filter @polyph/web build`
// (which builds the wasm engine), and Playwright with Chromium.
// Usage: node scripts/e2e-phone.mjs [--shots dir]
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// The test daemon gets a free port (never a stale daemon's default one) and
// commands wait until it has written daemon.json.
async function waitForFile(file) {
  for (let i = 0; i < 300; i++) {
    if (existsSync(file)) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${file} did not appear`);
}

const root = fileURLToPath(new URL("..", import.meta.url));
const require = createRequire(import.meta.url);
const { chromium, devices } = require("playwright");
const bin = process.env.POLYPH_BIN ?? join(root, "target/debug/polyph");
const shots = process.argv.includes("--shots")
  ? process.argv[process.argv.indexOf("--shots") + 1]
  : null;
const fail = (m) => {
  throw new Error(m);
};
const freePort = () =>
  new Promise((ok) => {
    const s = net.createServer().listen(0, "127.0.0.1", () => {
      const p = s.address().port;
      s.close(() => ok(p));
    });
  });
const until = async (what, f, ms = 15000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await f()) return;
    await new Promise((r) => setTimeout(r, 100));
  }
  fail(`timed out: ${what}`);
};

const home = mkdtempSync(join(tmpdir(), "polyph-e2e-phone-"));
const env = {
  ...process.env,
  POLYPH_HOME: home,
  POLYPH_KEYSTORE: "file",
  POLYPH_LANG: "en",
  POLYPH_WEB_DIR: join(root, "apps/web/out"),
};
const cli = (...args) => execFileSync(bin, args, { env, encoding: "utf8" });
const relayPort = await freePort();
// The relay keeps its database in the working directory: use the temp home.
const relay = spawn(bin, ["relay", "serve", "--bind", `127.0.0.1:${relayPort}`], {
  env,
  cwd: home,
  stdio: "ignore",
});
let daemon;
let browser;
let page;
try {
  writeFileSync(join(home, "config.toml"), "api_port = 0\n");
  daemon = spawn(bin, ["daemon", "run"], { env, stdio: "ignore" });
  await waitForFile(join(home, "daemon.json"));
  cli("config", "set", "relay", `ws://127.0.0.1:${relayPort}/ws`);
  cli("config", "set", "name", "Ann");
  cli("host", "--name", "e2e-phone");
  const port = JSON.parse(readFileSync(join(home, "daemon.json"), "utf8")).port;
  const token = readFileSync(join(home, "api.token"), "utf8").trim();
  const rpc = async (method, params = {}) => {
    const r = await fetch(`http://127.0.0.1:${port}/rpc`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    });
    const v = await r.json();
    if (v.error) fail(`${method}: ${v.error.message}`);
    return v.result;
  };
  let room;
  await until("room listed", async () => {
    room = (await rpc("room.list"))[0]?.id;
    return !!room;
  });
  await until("room connected", async () => (await rpc("room.get", { room })).connected);
  const pair = JSON.parse(cli("--json", "phone", "--base", `http://127.0.0.1:${port}/phone/`));
  if (!pair.link.includes("#join=")) fail(`bad link ${pair.link}`);

  browser = await chromium.launch();
  const ctx = await browser.newContext({ ...devices["Pixel 7"] });
  page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  if (process.env.E2E_DEBUG) page.on("console", (m) => console.log("[page]", m.text()));
  await page.goto(pair.link);
  // The code arrives filled in; the fragment leaves the address bar.
  await page.getByLabel("Join code").waitFor();
  if ((await page.getByLabel("Join code").inputValue()) !== pair.code) fail("code not filled in");
  if (page.url().includes("#")) fail("the pairing fragment stayed in the address bar");
  if (await page.getByRole("button", { name: "Host", exact: true }).count())
    fail("the phone offered to host a room");
  await page.getByLabel("Your name").fill("Ann");
  if (shots) await page.screenshot({ path: join(shots, "phone-join.png") });
  await page.getByRole("button", { name: "Join", exact: true }).click();

  // The computer approves the phone's knock.
  let knock;
  await until("knock", async () => {
    knock = (await rpc("knock.list", { room }))[0];
    return !!knock;
  });
  if (knock.surface !== "phone") fail(`knock surface ${knock.surface}`);
  if (knock.member !== pair.member) fail("the phone did not join as Ann's member");
  await rpc("knock.decide", { room, knock: knock.id, approve: true });
  await page.getByRole("heading", { name: "e2e-phone", level: 1 }).waitFor();

  // Chat both ways.
  await page.getByTestId("composer").fill("from the phone");
  await page.getByTestId("composer").press("Enter");
  await until("chat reached the daemon", async () =>
    (await rpc("chat.list", { room })).some((m) => m.text === "from the phone"),
  );
  await rpc("chat.send", { room, text: "hello phone" });
  await page.getByText("hello phone").waitFor();
  if (shots) await page.screenshot({ path: join(shots, "phone-chat.png") });

  // A card added on the computer shows on the phone's board.
  await rpc("board.add", { room, title: "Pair the phone" });
  await page
    .getByRole("button", { name: /menu/i })
    .first()
    .click()
    .catch(() => {});
  await page.getByRole("button", { name: "Board", exact: true }).click();
  await page.getByTestId("card").filter({ hasText: "Pair the phone" }).waitFor();
  if (shots) await page.screenshot({ path: join(shots, "phone-board.png") });

  // The room survives a reload: sealed state in IndexedDB.
  await page.reload();
  await page.getByRole("heading", { name: "e2e-phone", level: 1 }).waitFor();
  await page.getByText("hello phone").waitFor();

  // Installable: a manifest and a service worker in scope.
  const manifest = await page.evaluate(async () => {
    const href = document.querySelector('link[rel="manifest"]')?.getAttribute("href");
    return href ? (await fetch(href)).json() : null;
  });
  if (manifest?.display !== "standalone") fail("no manifest");
  await until("service worker", () =>
    page.evaluate(async () => !!(await navigator.serviceWorker.getRegistration())),
  );

  if (errors.length) fail(`page errors: ${errors.join("; ")}`);
  console.log("e2e phone: ok");
} catch (e) {
  console.error(e);
  if (shots && page)
    await page.screenshot({ path: join(shots, "phone-failure.png") }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser?.close();
  try {
    cli("daemon", "stop");
  } catch {
    daemon?.kill();
  }
  relay.kill();
  rmSync(home, { recursive: true, force: true });
  process.exit();
}
