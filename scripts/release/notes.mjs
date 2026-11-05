#!/usr/bin/env node
// Prints one version's section of CHANGELOG.md, for the release page.
// Usage: node scripts/release/notes.mjs v0.1.0
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../..", import.meta.url));
const v = (process.argv[2] ?? "").replace(/^v/, "");
const lines = readFileSync(join(root, "CHANGELOG.md"), "utf8").split("\n");
const start = lines.findIndex((l) => l.startsWith(`## ${v}`));
if (!v || start < 0) {
  console.error(`CHANGELOG.md has no section for ${v || "(no version given)"}`);
  process.exit(1);
}
const end = lines.findIndex((l, i) => i > start && l.startsWith("## "));
console.log(
  lines
    .slice(start + 1, end < 0 ? undefined : end)
    .join("\n")
    .trim(),
);
