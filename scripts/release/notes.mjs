#!/usr/bin/env node
// Prints one version's section of CHANGELOG.md, for the release page.
// Usage: node scripts/release/notes.mjs v0.1.0
//
// The section is the one headed exactly `## 0.1.0` (also `## v0.1.0`,
// `## [0.1.0]`, or with a date after it); `## 0.1.0` never matches
// `## 0.1.01` or `## 0.1.0-rc.1`. Without one, it prints the
// `## Unreleased` section, and without that a one-line note, saying so on
// stderr: the release is a draft a person reads before publishing.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { normalize } from "./version.mjs";

const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** The release notes for `version` from a changelog's text: `{ text, from }`. */
export function notesFor(changelog, version) {
  const v = normalize(version);
  const lines = changelog.split("\n");
  const section = (heading) => {
    const start = lines.findIndex((l) => heading.test(l));
    if (start < 0) return null;
    const end = lines.findIndex((l, i) => i > start && l.startsWith("## "));
    const text = lines
      .slice(start + 1, end < 0 ? undefined : end)
      .join("\n")
      .trim();
    return text || null;
  };
  const own = section(new RegExp(`^## \\[?v?${escape(v)}\\]?(?=\\s|$)`));
  if (own) return { text: own, from: v };
  const unreleased = section(/^## \[?Unreleased\]?(?=\s|$)/i);
  if (unreleased) return { text: unreleased, from: "Unreleased" };
  return { text: `Polyph ${v}.`, from: null };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const root = fileURLToPath(new URL("../..", import.meta.url));
  try {
    const n = notesFor(readFileSync(join(root, "CHANGELOG.md"), "utf8"), process.argv[2]);
    if (n.from === "Unreleased") {
      console.error(
        `CHANGELOG.md has no section for ${normalize(process.argv[2])}; using its Unreleased section`,
      );
    } else if (n.from === null) {
      console.error(
        `CHANGELOG.md has no section for ${normalize(process.argv[2])} and no Unreleased notes`,
      );
    }
    console.log(n.text);
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
