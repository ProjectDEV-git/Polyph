#!/usr/bin/env node
// End-to-end test of the phone PWA: a phone-sized browser links to its
// owner in a real room through a real relay, with no daemon of its own,
// then chats, answers an agent's approval and survives a reload.
//
// Needs: `cargo build -p polyph-cli -p polyph-testkit --bins`,
// `pnpm --filter @polyph/web build` and Playwright with Chromium.
// Usage: node scripts/e2e-phone.mjs [--shots dir]
import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const bin = join(root, "target/debug/polyph");
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
    if (await f().catch(() => false)) return;
    await new Promise((r) => setTimeout(r, 200));
  }
  fail(`timed out: ${what}`);
};

const tmp = mkdtempSync(join(tmpdir(), "polyph-phone-"));
const home = join(tmp, "home");
const relayHome = join(tmp, "relay");
const repo = join(tmp, "repo");
mkdirSync(repo);
const relayPort = await freePort();
let apiPort = await freePort();
while (apiPort === relayPort) apiPort = await freePort();
const env = {
  ...process.env,
  POLYPH_HOME: home,
  POLYPH_KEYSTORE: "file",
  POLYPH_LANG: "en",
  POLYPH_WEB_DIR: join(root, "apps/web/out"),
};
const cli = (...args) => execFileSync(bin, args, { env, encoding: "utf8", cwd: repo });
const procs = [];
let browser;
try {
  procs.push(
    spawn(bin, ["relay", "serve", "--bind", `127.0.0.1:${relayPort}`], {
      env: { ...env, POLYPH_HOME: relayHome },
      stdio: "ignore",
    }),
  );
  cli("config", "set", "name", "Ann");
  cli("config", "set", "relay", `ws://127.0.0.1:${relayPort}/ws`);
  cli("config", "set", "api_port", String(apiPort));
  cli("config", "set", "phone_url", `http://127.0.0.1:${apiPort}/m/`);
  // The CLI started the daemon (with this environment); restart it on the chosen port.
  cli("daemon", "stop");
  cli("whoami");
  await until("daemon", async () => (await fetch(`http://127.0.0.1:${apiPort}/healthz`)).ok);
  const room = JSON.parse(cli("--json", "host", "--name", "e2e-phone"));
  const token = readFileSync(join(home, "api.token"), "utf8").trim();
  const rpc = async (method, params = {}) => {
    const r = await fetch(`http://127.0.0.1:${apiPort}/rpc`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    });
    const j = await r.json();
    if (j.error) throw new Error(j.error.message);
    return j.result;
  };
  const rid = room.room?.id ?? room.id;
  if (!rid) fail(`no room id in ${JSON.stringify(room)}`);
  await until("room connected", async () => (await rpc("room.get", { room: rid })).connected).catch(
    (e) => {
      console.error(
        JSON.stringify(room),
        "apiPort",
        apiPort,
        "relayPort",
        relayPort,
        readFileSync(join(home, "daemon.json"), "utf8"),
        readFileSync(join(home, "config.toml"), "utf8"),
      );
      throw e;
    },
  );
  const link = JSON.parse(cli("--json", "link"));
  if (!link.phone || !link.url.startsWith(`http://127.0.0.1:${apiPort}/m/#link=`))
    fail(`bad link ${JSON.stringify(link)}`);

  browser = await chromium.launch();
  const ctx = await browser.newContext({
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
  });
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto(link.url);
  await page.getByRole("heading", { name: "Link this phone" }).waitFor();
  if (page.url().includes("link=")) fail("the link code stayed in the address bar");
  await page.getByLabel("Your name").fill("Ann");
  await page.getByRole("button", { name: "Link this phone" }).click();
  // Ann links her own phone and may admit people, so it is let in at once.
  await page.getByRole("heading", { name: "e2e-phone", level: 1 }).waitFor({ timeout: 20000 });
  const members = await rpc("room.members", { room: rid });
  if (members.length !== 1 || members[0].devices !== 2)
    fail(`expected Ann with two devices: ${JSON.stringify(members)}`);

  // Chat both ways.
  await rpc("chat.send", { room: rid, text: "hello phone" });
  await page.getByText("hello phone").waitFor();
  await page.getByTestId("composer").fill("from the phone");
  await page.getByTestId("composer").press("Enter");
  await until("the desktop sees the phone's message", async () =>
    (await rpc("chat.list", { room: rid })).some(
      (m) => m.text === "from the phone" && m.name === "Ann",
    ),
  );

  // Ann's agent asks; she approves on the phone.
  writeFileSync(
    join(repo, ".fake-script.json"),
    JSON.stringify({
      turns: [
        [{ permission: { kind: "execute", title: "Run cargo publish", command: "cargo publish" } }],
      ],
    }),
  );
  writeFileSync(
    join(repo, "polyph.toml"),
    `[agents.fake]\ncommand = "${join(root, "target/debug/polyph-fake-agent")}"\nenv = { POLYPH_FAKE_SCRIPT = "${join(repo, ".fake-script.json")}" }\n`,
  );
  const started = await rpc("agent.start", {
    room: rid,
    kind: "fake",
    cwd: repo,
    prompt: "publish",
  });
  await page.getByRole("button", { name: /1 approval/ }).click();
  await page.getByText("cargo publish").first().waitFor();
  if (shots) await page.screenshot({ path: join(shots, "phone-approval.png") });
  await page.getByRole("button", { name: "Approve once" }).click();
  await until("the agent goes on", async () => {
    const t = await rpc("agent.transcript", { room: rid, agent: started.id });
    return t.events.some(
      (e) => e.kind.t === "message" && String(e.kind.c.text).includes("permission: allowed"),
    );
  });

  // The phone keeps its room across a reload, and the app works offline-first.
  await page.reload();
  await page.getByRole("heading", { name: "e2e-phone", level: 1 }).waitFor({ timeout: 20000 });
  const sw = await page.evaluate(
    async () => !!(await navigator.serviceWorker.getRegistration("/m/")),
  );
  if (!sw) fail("no service worker");
  if (shots) await page.screenshot({ path: join(shots, "phone-chat.png") });
  if (errors.length) fail(`page errors: ${errors.join("; ")}`);
  console.log("e2e phone: ok");
} catch (e) {
  console.error(e);
  process.exitCode = 1;
} finally {
  await browser?.close();
  try {
    cli("daemon", "stop");
  } catch {}
  for (const p of procs) p.kill();
  rmSync(tmp, { recursive: true, force: true });
  process.exit();
}
