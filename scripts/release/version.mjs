#!/usr/bin/env node
// One version for everything that ships: the Rust workspace, the hosted
// service, the desktop app (Cargo and Tauri), and the npm packages.
//
//   node scripts/release/version.mjs              print the versions found
//   node scripts/release/version.mjs 0.2.0        set them all (then: cargo update -w)
//   node scripts/release/version.mjs --check v0.2.0   fail unless all equal the tag
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("../..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");

const toml = (file, pattern) => ({
  file,
  get: (s) => pattern.exec(s)?.[2],
  set: (s, v) => s.replace(pattern, (_, a, _old, b) => `${a}${v}${b}`),
});
const json = (file) => ({
  file,
  get: (s) => JSON.parse(s).version,
  set: (s, v) => s.replace(/("version"\s*:\s*")[^"]+(")/, `$1${v}$2`),
});

const FILES = [
  toml("Cargo.toml", /(\[workspace\.package\][^[]*?\nversion\s*=\s*")([^"]+)(")/s),
  toml("cloud/Cargo.toml", /(\[package\][^[]*?\nversion\s*=\s*")([^"]+)(")/s),
  toml("apps/desktop/src-tauri/Cargo.toml", /(\[package\][^[]*?\nversion\s*=\s*")([^"]+)(")/s),
  json("apps/desktop/src-tauri/tauri.conf.json"),
  json("apps/desktop/package.json"),
  json("apps/web/package.json"),
  json("apps/vscode/package.json"),
  json("packages/ui/package.json"),
];

// The workspace pins its own crates too (`polyph-x = { path = ..., version = "..." }`
// under [workspace.dependencies]); they must move with the version.
const PIN = /(\npolyph-[\w-]+ = \{ path = "crates\/[^"]+", version = ")([^"]+)(")/g;
FILES.push({
  file: "Cargo.toml",
  get: (s) => {
    const pins = [...s.matchAll(PIN)].map((m) => m[2]);
    return pins.length && pins.every((v) => v === pins[0])
      ? pins[0]
      : `mixed (${[...new Set(pins)].join(", ")})`;
  },
  set: (s, v) => s.replace(PIN, (_, a, _old, b) => `${a}${v}${b}`),
  label: "Cargo.toml [workspace.dependencies] pins",
});

const read = (f) => readFileSync(join(root, f.file), "utf8");
const found = FILES.map((f) => [f.label ?? f.file, f.get(read(f))]);
const args = process.argv.slice(2);

if (args[0] === "--check") {
  const want = (args[1] ?? "").replace(/^v/, "");
  const bad = found.filter(([, v]) => v !== want);
  if (!want || bad.length) {
    for (const [f, v] of bad)
      console.error(`${f}: ${v ?? "no version"} (tag says ${want || "nothing"})`);
    process.exit(1);
  }
  console.log(`all versions are ${want}`);
} else if (args[0]) {
  const v = args[0].replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(v)) throw new Error(`not a version: ${v}`);
  for (const f of FILES) writeFileSync(join(root, f.file), f.set(read(f), v));
  console.log(
    `set ${FILES.length} files to ${v}; now run: cargo update -w && (cd cloud && cargo update -w) && (cd apps/desktop/src-tauri && cargo update -w)`,
  );
} else {
  for (const [f, v] of found) console.log(`${v ?? "?"}\t${f}`);
}
