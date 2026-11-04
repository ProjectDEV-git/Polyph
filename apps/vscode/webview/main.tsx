// The webview's entry: the shared app on the daemon the extension found.
import { PolyphApp, type Platform } from "@polyph/ui";
import "@polyph/ui/styles.css";
import { createRoot } from "react-dom/client";

declare global {
  interface Window {
    POLYPH: { url: string; theme: "light" | "dark" };
  }
}
declare function acquireVsCodeApi(): { postMessage(m: unknown): void };

const vscode = acquireVsCodeApi();
const platform: Platform = {
  kind: "web",
  openExternal: (url) => vscode.postMessage({ type: "open", url }),
};

// Follow VS Code's theme unless the person chose one in Polyph's settings.
try {
  const prefs = JSON.parse(localStorage.getItem("polyph.prefs") ?? "{}");
  if (!prefs.theme)
    localStorage.setItem("polyph.prefs", JSON.stringify({ ...prefs, theme: window.POLYPH.theme }));
} catch {}

createRoot(document.getElementById("root")!).render(
  <PolyphApp url={window.POLYPH.url} platform={platform} />,
);
