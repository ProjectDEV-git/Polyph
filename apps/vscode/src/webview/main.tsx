// The shared app inside a VS Code webview, connected through the extension.
import { PolyphApp, type Platform } from "@polyph/ui";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { WebviewClient, type Port, type ToHost, type ToView } from "../bridge";
import "./styles.css";

declare function acquireVsCodeApi(): { postMessage(m: unknown): void };

const vscode = acquireVsCodeApi();
const port: Port = {
  post: (m: ToHost) => vscode.postMessage(m),
  listen: (f: (m: ToView) => void) => {
    const on = (e: MessageEvent) => f(e.data as ToView);
    window.addEventListener("message", on);
    return () => window.removeEventListener("message", on);
  },
};
const client = new WebviewClient(port);
// Approvals and alerts arrive as VS Code notifications from the extension.
const platform: Platform = {
  kind: "vscode",
  openExternal: (url) => port.post({ t: "open", url }),
};

const root = document.getElementById("root");
if (root)
  createRoot(root).render(
    <StrictMode>
      <PolyphApp client={client} platform={platform} />
    </StrictMode>,
  );
