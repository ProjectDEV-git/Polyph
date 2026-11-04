// Builds the extension (esbuild, CommonJS for VS Code) and the panel's
// webview (Vite: the shared app with its styles and fonts).
import { build } from "esbuild";
import { build as vite } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL("..", import.meta.url));
await build({
  entryPoints: [`${here}src/extension.ts`],
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node20",
  outfile: `${here}dist/extension.js`,
  external: ["vscode", "bufferutil", "utf-8-validate"],
  sourcemap: false,
  minify: true,
  logLevel: "warning",
});
await vite({
  configFile: false,
  root: here,
  logLevel: "warn",
  plugins: [react(), tailwindcss()],
  define: { "process.env.NODE_ENV": '"production"' },
  build: {
    outDir: `${here}dist/webview`,
    emptyOutDir: true,
    lib: {
      entry: `${here}webview/main.tsx`,
      formats: ["es"],
      fileName: () => "webview.js",
      cssFileName: "webview",
    },
    rollupOptions: { output: { assetFileNames: "[name][extname]" } },
  },
});
