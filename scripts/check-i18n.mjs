#!/usr/bin/env node
// Fails when user-facing strings are hard-coded in UI code, or when a locale is
// missing keys that English has. Run with `pnpm check:i18n`.
//
// UI code: *.tsx under apps/ and packages/ui, plus Rust TUI/CLI output modules.
// Allowed: lines with `i18n-allow` comments, files listed in scripts/i18n-allowlist.txt,
// and strings with no letters (punctuation, numbers, symbols).
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, relative, sep } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const allowFile = join(root, "scripts/i18n-allowlist.txt");
const allow = existsSync(allowFile)
  ? readFileSync(allowFile, "utf8").split("\n").map((l) => l.trim()).filter((l) => l && !l.startsWith("#"))
  : [];

const SKIP_DIRS = new Set(["node_modules", ".next", "dist", "target", "pkg", ".turbo", "out", "test", "tests", "__tests__", "e2e"]);

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    if (SKIP_DIRS.has(name)) continue;
    const p = join(dir, name);
    const s = statSync(p);
    if (s.isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}

const isAllowed = (file) => {
  const rel = relative(root, file).split(sep).join("/");
  return allow.some((a) => (a.endsWith("/") ? rel.startsWith(a) : rel === a));
};

const hasWords = (s) => /\p{L}{2,}/u.test(s);
const problems = [];

// TSX: JSX text nodes and user-facing attributes.
const ATTRS = ["title", "aria-label", "placeholder", "alt", "label", "aria-description"];
for (const file of [...walk(join(root, "apps")), ...walk(join(root, "packages/ui"))]) {
  if (!file.endsWith(".tsx") || file.includes(".test.") || file.includes(".stories.") || isAllowed(file)) continue;
  const lines = readFileSync(file, "utf8").split("\n");
  lines.forEach((line, i) => {
    if (line.includes("i18n-allow")) return;
    const trimmed = line.trim();
    if (trimmed.startsWith("//") || trimmed.startsWith("*") || trimmed.startsWith("import ")) return;
    // >Text< between tags on one line
    for (const m of line.matchAll(/>([^<>{}]+)</g)) {
      if (hasWords(m[1])) problems.push(`${relative(root, file)}:${i + 1}: JSX text "${m[1].trim()}"`);
    }
    // Text alone on a line inside JSX (starts with a letter, no code punctuation)
    if (/^[\p{L}][^=;(){}<>]*$/u.test(trimmed) && hasWords(trimmed) && !/^(return|const|let|export|type|interface|case|default|else|if)\b/.test(trimmed)) {
      const prev = lines[i - 1]?.trim() ?? "";
      if (prev.endsWith(">")) problems.push(`${relative(root, file)}:${i + 1}: JSX text "${trimmed}"`);
    }
    for (const a of ATTRS) {
      for (const m of line.matchAll(new RegExp(`\\s${a}="([^"]*)"`, "g"))) {
        if (hasWords(m[1])) problems.push(`${relative(root, file)}:${i + 1}: ${a}="${m[1]}"`);
      }
    }
  });
}

// Rust TUI rendering: string literals passed to ratatui text constructors.
for (const file of walk(join(root, "crates/polyph-cli/src/tui"))) {
  if (!file.endsWith(".rs") || isAllowed(file)) continue;
  const lines = readFileSync(file, "utf8").split("\n");
  let inTests = false;
  lines.forEach((line, i) => {
    if (line.includes("#[cfg(test)]")) inTests = true;
    if (inTests || line.includes("i18n-allow")) return;
    for (const m of line.matchAll(/(?:Span::(?:raw|styled)|Line::from|Paragraph::new|Block::(?:bordered\(\)\.)?title)\(\s*"([^"]*)"/g)) {
      if (hasWords(m[1])) problems.push(`${relative(root, file)}:${i + 1}: TUI literal "${m[1]}"`);
    }
  });
}

// Locale parity: every key in en must exist in every other locale.
const keyRe = /^([a-zA-Z][\w-]*)\s*=/gm;
const locales = readdirSync(join(root, "locales")).filter((d) => statSync(join(root, "locales", d)).isDirectory());
const keysOf = (loc) => {
  const keys = new Map();
  for (const f of readdirSync(join(root, "locales", loc)).filter((f) => f.endsWith(".ftl"))) {
    const text = readFileSync(join(root, "locales", loc, f), "utf8");
    for (const m of text.matchAll(keyRe)) keys.set(m[1], f);
  }
  return keys;
};
const en = keysOf("en");
for (const loc of locales.filter((l) => l !== "en")) {
  const k = keysOf(loc);
  for (const [key, f] of en) if (!k.has(key)) problems.push(`locales/${loc}/${f}: missing key ${key}`);
  for (const key of k.keys()) if (!en.has(key)) problems.push(`locales/${loc}: key ${key} is not in en`);
}

if (problems.length) {
  console.error(`i18n check failed with ${problems.length} problem(s):`);
  for (const p of problems) console.error("  " + p);
  console.error("Move the text into locales/*/ *.ftl, or mark a deliberate exception with an i18n-allow comment.");
  process.exit(1);
}
console.log(`i18n check passed (${en.size} keys, ${locales.length} locales).`);
