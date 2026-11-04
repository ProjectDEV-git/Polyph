import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";
import { defineConfig } from "vite";

// The webview bundle: fixed names, loaded by the extension from media/webview.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "./",
  publicDir: false,
  build: {
    target: "es2022",
    outDir: "media/webview",
    emptyOutDir: true,
    cssCodeSplit: false,
    rollupOptions: {
      input: resolve(__dirname, "src/webview/main.tsx"),
      output: {
        entryFileNames: "webview.js",
        chunkFileNames: "chunks/[name]-[hash].js",
        assetFileNames: (a) =>
          a.names?.some((n) => n.endsWith(".css"))
            ? "webview.css"
            : "assets/[name]-[hash][extname]",
      },
    },
  },
});
