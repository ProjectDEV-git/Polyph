#!/usr/bin/env node
// Packs a release archive for one target:
//
//   polyph-<version>-<target>/
//     bin/polyph[.exe]          the CLI, daemon, TUI and embedded relay
//     bin/polyph-relay[.exe]    the standalone relay
//     share/polyph/web/         the browser client and phone app (the daemon
//                               finds it at ../share/polyph/web from bin/)
//     LICENSE, README.md
//
// Writes dist/polyph-<version>-<target>.tar.gz (.zip on Windows) and a
// .sha256 file beside it. Build first:
//   cargo build --profile dist --locked -p polyph-cli -p polyph-relay [--target T]
//   pnpm --filter @polyph/web build
//
// Usage: node scripts/release/package.mjs [--target T] [--profile dist] [--web apps/web/out] [--out dist]
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("../..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"));
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce(
      (acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]]] : acc),
      [],
    ),
);

export function workspaceVersion() {
  const toml = readFileSync(join(root, "Cargo.toml"), "utf8");
  const m = /\[workspace\.package\][^[]*?version\s*=\s*"([^"]+)"/s.exec(toml);
  if (!m) throw new Error("no [workspace.package] version in Cargo.toml");
  return m[1];
}

function hostTarget() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  return /host: (\S+)/.exec(out)[1];
}

const target = args.target ?? hostTarget();
const profile = args.profile ?? "dist";
const version = args.version ?? workspaceVersion();
const web = resolve(root, args.web ?? "apps/web/out");
const out = resolve(root, args.out ?? "dist");
const windows = target.includes("windows");
const exe = windows ? ".exe" : "";
const profileDir = profile === "dev" ? "debug" : profile;
const binDir = join(root, "target", ...(args.target ? [target] : []), profileDir);

for (const b of ["polyph", "polyph-relay"]) {
  if (!existsSync(join(binDir, b + exe)))
    throw new Error(`missing ${join(binDir, b + exe)}; build it first`);
}
if (!existsSync(join(web, "app", "index.html")))
  throw new Error(`missing ${web}/app/index.html; build apps/web first`);

const name = `polyph-${version}-${target}`;
const stage = join(out, name);
rmSync(stage, { recursive: true, force: true });
mkdirSync(join(stage, "bin"), { recursive: true });
for (const b of ["polyph", "polyph-relay"])
  cpSync(join(binDir, b + exe), join(stage, "bin", b + exe));
cpSync(web, join(stage, "share", "polyph", "web"), { recursive: true });
cpSync(join(root, "LICENSE"), join(stage, "LICENSE"));
writeFileSync(
  join(stage, "README.md"),
  `# Polyph ${version} (${target})

- \`bin/polyph\`: the command line, daemon and terminal UI. Run \`polyph\` to
  open the terminal UI, \`polyph --help\` for commands.
- \`bin/polyph-relay\`: the standalone relay for self-hosting.
- \`share/polyph/web\`: the browser client and phone app, served by the daemon
  (\`polyph web\`). Keep it at \`../share/polyph/web\` relative to \`bin/\`.

Install by copying \`bin/\` and \`share/\` into a prefix such as \`~/.local\`,
or use the install script: https://github.com/ProjectDEV-git/Polyph#install
`,
);

const archive = join(out, name + (windows ? ".zip" : ".tar.gz"));
rmSync(archive, { force: true });
// bsdtar on Windows writes zip with -a; GNU and BSD tar write .tar.gz.
const tarArgs = windows ? ["-a", "-cf", archive, name] : ["-czf", archive, name];
execFileSync("tar", tarArgs, { cwd: out, stdio: "inherit" });
rmSync(stage, { recursive: true, force: true });

const sum = createHash("sha256").update(readFileSync(archive)).digest("hex");
writeFileSync(archive + ".sha256", `${sum}  ${name}${windows ? ".zip" : ".tar.gz"}\n`);
console.log(`${archive}\n${sum}`);
