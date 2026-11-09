#!/usr/bin/env node
// One version for everything that ships: the Rust workspace, the hosted
// service, the desktop app (Cargo and Tauri), the npm packages, and the
// workspace's own crates in each Cargo.lock.
//
//   node scripts/release/version.mjs                  print the versions found
//   node scripts/release/version.mjs 0.2.0            set them all, lock files included
//   node scripts/release/version.mjs --check v0.2.0   fail unless all equal the tag
//   node scripts/release/version.mjs --normalize v1.2 print the full version (1.2.0)
//   node scripts/release/version.mjs --msi 0.2.0-rc.1 print the Windows installer version (0.2.0)
//
// A version may start with `v`, may leave out the minor or patch number
// (`1` is 1.0.0, `1.2` is 1.2.0), and may carry a pre-release such as
// `-beta.1` or `-rc.2`.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../..", import.meta.url));

/** The full `major.minor.patch[-pre]` form of `input`, or an error. */
export function normalize(input) {
  const s = String(input ?? "")
    .trim()
    .replace(/^v/i, "");
  const m = /^(\d+)(?:\.(\d+))?(?:\.(\d+))?(-[0-9A-Za-z]+(?:\.[0-9A-Za-z]+)*)?$/.exec(s);
  if (!m) {
    throw new Error(
      `not a version: "${input}". Use numbers like 0.2.0 or 1.0, optionally with a pre-release such as 0.2.0-beta.1`,
    );
  }
  const [, major, minor = "0", patch = "0", pre = ""] = m;
  // Leading zeros are not allowed in semver ("01" becomes 1); nor in a
  // numeric pre-release part.
  const pre2 = pre.replace(/(?<=[-.])0+(?=\d)/g, "");
  return `${Number(major)}.${Number(minor)}.${Number(patch)}${pre2}`;
}

/**
 * The version Windows Installer (MSI) gets: numbers only, at most
 * 255.255.65535. A pre-release keeps its base version (0.2.0-rc.1 is
 * 0.2.0), which MSI upgrades treat like the release itself.
 */
export function msiVersion(version) {
  const [major, minor, patch] = normalize(version).split(/[.-]/).slice(0, 3).map(Number);
  if (major > 255 || minor > 255 || patch > 65535) {
    throw new Error(
      `${version} cannot be a Windows installer version: major and minor go up to 255, patch up to 65535`,
    );
  }
  return `${major}.${minor}.${patch}`;
}

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

// The workspace's own crates in a lock file: `polyph-*` packages with no
// `source` (path dependencies), which all carry the workspace version.
const lock = (file) => {
  const own = (block) => /^name = "polyph-[^"]+"$/m.test(block) && !/^source = /m.test(block);
  const blocks = (s) => s.split(/\n(?=\[\[package\]\]\n)/);
  return {
    file,
    get: (s) => {
      const vs = blocks(s)
        .filter(own)
        .map((b) => /^version = "([^"]+)"$/m.exec(b)?.[1]);
      return vs.length && vs.every((v) => v === vs[0])
        ? vs[0]
        : `mixed (${[...new Set(vs)].join(", ")})`;
    },
    set: (s, v) =>
      blocks(s)
        .map((b) => (own(b) ? b.replace(/^version = "[^"]+"$/m, `version = "${v}"`) : b))
        .join("\n"),
    label: `${file} (polyph crates)`,
  };
};

const FILES = [
  toml("Cargo.toml", /(\[workspace\.package\][^[]*?\nversion\s*=\s*")([^"]+)(")/s),
  toml("cloud/Cargo.toml", /(\[package\][^[]*?\nversion\s*=\s*")([^"]+)(")/s),
  toml("apps/desktop/src-tauri/Cargo.toml", /(\[package\][^[]*?\nversion\s*=\s*")([^"]+)(")/s),
  json("apps/desktop/src-tauri/tauri.conf.json"),
  json("apps/desktop/package.json"),
  json("apps/web/package.json"),
  json("apps/vscode/package.json"),
  json("packages/ui/package.json"),
  lock("Cargo.lock"),
  lock("cloud/Cargo.lock"),
  lock("apps/desktop/src-tauri/Cargo.lock"),
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

function main(args) {
  const found = () => FILES.map((f) => [f.label ?? f.file, f.get(read(f))]);
  if (args[0] === "--normalize") {
    console.log(normalize(args[1]));
  } else if (args[0] === "--msi") {
    console.log(msiVersion(args[1]));
  } else if (args[0] === "--check") {
    const want = args[1] ? normalize(args[1]) : "";
    const bad = found().filter(([, v]) => v !== want);
    if (!want || bad.length) {
      for (const [f, v] of bad)
        console.error(`${f}: ${v ?? "no version"} (tag says ${want || "nothing"})`);
      process.exit(1);
    }
    console.log(`all versions are ${want}`);
  } else if (args[0]) {
    const v = normalize(args[0]);
    for (const f of FILES) {
      const before = read(f);
      const after = f.set(before, v);
      if (after !== before) writeFileSync(join(root, f.file), after);
    }
    console.log(`set ${FILES.length} places to ${v}`);
  } else {
    for (const [f, v] of found()) console.log(`${v ?? "?"}\t${f}`);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
