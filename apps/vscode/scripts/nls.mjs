// Writes VS Code's manifest translations (package.nls*.json) from the
// `vscode-*` strings in locales/*/vscode.ftl, so the manifest has no
// hard-coded text and shares the product's four locales.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../..", import.meta.url));
const here = fileURLToPath(new URL("..", import.meta.url));
// Polyph locale -> VS Code display language id.
const LANGS = { en: "", th: "th", ja: "ja", "zh-Hans": "zh-cn" };
const manifest = readFileSync(join(here, "package.json"), "utf8");
const used = [...new Set([...manifest.matchAll(/"%([a-z0-9-]+)%"/g)].map((m) => m[1]))];

function parse(text) {
  const out = {};
  for (const line of text.split("\n")) {
    const m = /^([a-z0-9-]+)\s*=\s*(.*)$/.exec(line);
    if (m) out[m[1]] = m[2].trim();
  }
  return out;
}

for (const [lang, id] of Object.entries(LANGS)) {
  const strings = parse(readFileSync(join(root, "locales", lang, "vscode.ftl"), "utf8"));
  const nls = {};
  for (const k of used) if (strings[k]) nls[k] = strings[k];
  const missing = used.filter((k) => !nls[k]);
  if (lang === "en" && missing.length) throw new Error(`missing in en/vscode.ftl: ${missing}`);
  writeFileSync(
    join(here, id ? `package.nls.${id}.json` : "package.nls.json"),
    `${JSON.stringify(nls, null, 2)}\n`,
  );
}
