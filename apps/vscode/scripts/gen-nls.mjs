// VS Code reads the manifest's texts from package.nls*.json; they come from
// locales/*/vscode.ftl (keys `vscode-nls-*`), like every other string.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("../../..", import.meta.url).pathname;
const here = new URL("..", import.meta.url).pathname;
const map = { en: "", th: ".th", ja: ".ja", "zh-Hans": ".zh-cn" };
for (const [lang, suffix] of Object.entries(map)) {
  const text = readFileSync(join(root, "locales", lang, "vscode.ftl"), "utf8");
  const out = {};
  for (const m of text.matchAll(/^vscode-nls-([\w-]+)\s*=\s*(.+)$/gm))
    out[m[1].replace(/-/g, ".")] = m[2].trim();
  writeFileSync(join(here, `package.nls${suffix}.json`), JSON.stringify(out, null, 2) + "\n");
}
