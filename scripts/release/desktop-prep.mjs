#!/usr/bin/env node
// Puts the release `polyph` binary where Tauri expects its sidecar
// (apps/desktop/src-tauri/binaries/polyph-<target>[.exe]). The desktop app
// starts the daemon from it, so the app works without polyph on PATH.
// Then build the app with the release overlay:
//   cd apps/desktop && pnpm tauri build --config src-tauri/tauri.release.conf.json [--target T]
// Usage: node scripts/release/desktop-prep.mjs [--target T] [--profile dist]
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("../..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"));
const argv = process.argv.slice(2);
const arg = (k) => (argv.includes(`--${k}`) ? argv[argv.indexOf(`--${k}`) + 1] : undefined);
const host = /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];
const target = arg("target") ?? host;
const profile = arg("profile") ?? "dist";
const exe = target.includes("windows") ? ".exe" : "";
const src = join(
  root,
  "target",
  ...(arg("target") ? [target] : []),
  profile === "dev" ? "debug" : profile,
  "polyph" + exe,
);
if (!existsSync(src)) throw new Error(`missing ${src}; build polyph-cli first`);
if (!existsSync(join(root, "apps/web/out/app/index.html")))
  throw new Error("missing apps/web/out; build @polyph/web first");
const dir = join(root, "apps/desktop/src-tauri/binaries");
mkdirSync(dir, { recursive: true });
const dst = join(dir, `polyph-${target}${exe}`);
copyFileSync(src, dst);
if (!exe) chmodSync(dst, 0o755);
console.log(dst);
