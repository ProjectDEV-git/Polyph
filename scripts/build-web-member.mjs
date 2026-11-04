#!/usr/bin/env node
// Builds the browser room member (crates/polyph-web) to WebAssembly and
// writes its bindings to packages/ui/src/member/pkg. Needs the wasm32 target
// and wasm-bindgen-cli 0.2.130 (`cargo install wasm-bindgen-cli --version 0.2.130`).
import { execFileSync } from "node:child_process";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: "inherit" });
run("cargo", [
  "build",
  "-p",
  "polyph-web",
  "--target",
  "wasm32-unknown-unknown",
  "--profile",
  "wasm",
]);
run("wasm-bindgen", [
  "--target",
  "web",
  "--remove-name-section",
  "--remove-producers-section",
  "--out-dir",
  join(root, "packages/ui/src/member/pkg"),
  join(root, "target/wasm32-unknown-unknown/wasm/polyph_web.wasm"),
]);
