#!/usr/bin/env node
// Smoke test in a real VS Code: a daemon hosts a room, VS Code loads the
// built extension (pnpm build first) and the suite checks it connects.
// Needs `cargo build -p polyph-cli`, network access to download VS Code the
// first time, and a display (run under xvfb-run on Linux CI).
// POLYPH_VSCODE_EXT=<dir> loads an unpacked .vsix (its extension/ folder)
// instead of this source folder, to check the release package.
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runTests } from "@vscode/test-electron";

const here = new URL("../..", import.meta.url).pathname;
const root = join(here, "../..");
const bin = process.env.POLYPH_BIN ?? join(root, "target/debug/polyph");
const home = mkdtempSync(join(tmpdir(), "polyph-e2e-vscode-"));
const env = { ...process.env, POLYPH_HOME: home, POLYPH_KEYSTORE: "file", POLYPH_LANG: "en" };
const cli = (...a) => execFileSync(bin, a, { env, encoding: "utf8" });
const daemon = spawn(bin, ["daemon", "run"], { env, stdio: "ignore" });
try {
  cli("config", "set", "name", "Ann");
  cli("host", "--lan", "--name", "e2e-vscode");
  await runTests({
    extensionDevelopmentPath: process.env.POLYPH_VSCODE_EXT ?? here,
    extensionTestsPath: join(here, "test/e2e/suite.cjs"),
    launchArgs: ["--disable-extensions", "--disable-gpu", "--no-sandbox", here],
    extensionTestsEnv: { POLYPH_HOME: home, E2E_ROOM: "e2e-vscode" },
  });
  console.log("e2e vscode: ok");
} catch (e) {
  console.error(e);
  process.exitCode = 1;
} finally {
  try {
    cli("daemon", "stop");
  } catch {
    daemon.kill();
  }
  rmSync(home, { recursive: true, force: true });
  process.exit();
}
