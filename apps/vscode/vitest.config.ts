import { resolve } from "node:path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: { alias: { vscode: resolve(__dirname, "test/vscode-mock.ts") } },
  test: { environment: "node", globals: true, include: ["test/**/*.test.ts"] },
});
