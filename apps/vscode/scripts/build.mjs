// Bundles the extension host code (CommonJS for VS Code's Node).
import { build } from "esbuild";

await build({
  entryPoints: ["src/extension.ts"],
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node20",
  outfile: "dist/extension.cjs",
  external: ["vscode", "bufferutil", "utf-8-validate"],
  logLevel: "warning",
});
