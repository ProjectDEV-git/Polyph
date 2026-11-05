#!/usr/bin/env node
// Builds the phone engine (crates/polyph-phone) to wasm, generates its
// JavaScript bindings and copies both into a directory the web app serves
// (default apps/web/public/phone, not committed): engine.js and engine.wasm.
// The PWA loads them at run time (apps/web/src/phone/engine.ts).
//
// Needs the wasm32-unknown-unknown target and wasm-bindgen-cli at the same
// version as the wasm-bindgen crate in Cargo.lock.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

const target = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const wasm = join(target, "wasm32-unknown-unknown/wasm/polyph_phone.wasm");
const pkg = join(target, "phone-pkg");
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: "inherit" });

const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
const want = /name = "wasm-bindgen"\nversion = "([^"]+)"/.exec(lock)?.[1];
let have = "";
try {
  have = execFileSync("wasm-bindgen", ["--version"], { encoding: "utf8" }).trim().split(" ")[1];
} catch {
  console.error(`wasm-bindgen is not installed: cargo install wasm-bindgen-cli --version ${want}`);
  process.exit(1);
}
if (want && have !== want) {
  console.error(
    `wasm-bindgen ${have} does not match the crate (${want}): cargo install wasm-bindgen-cli --version ${want}`,
  );
  process.exit(1);
}

run("cargo", [
  "build",
  "-p",
  "polyph-phone",
  "--target",
  "wasm32-unknown-unknown",
  "--features",
  "wasm",
  "--profile",
  "wasm",
]);
mkdirSync(pkg, { recursive: true });
run("wasm-bindgen", ["--target", "web", "--out-dir", pkg, "--out-name", "engine", wasm]);
const out = join(root, process.argv[2] ?? "apps/web/public/phone");
mkdirSync(out, { recursive: true });
copyFileSync(join(pkg, "engine_bg.wasm"), join(out, "engine.wasm"));
copyFileSync(join(pkg, "engine.js"), join(out, "engine.js"));
const size = statSync(join(pkg, "engine_bg.wasm")).size;
console.log(`phone engine: ${(size / 1024).toFixed(0)} KiB`);
