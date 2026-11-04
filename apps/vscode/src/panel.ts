// The panel: the shared app (packages/ui) in a webview, on the same daemon.
import type { Localizer } from "@polyph/ui/i18n";
import * as crypto from "node:crypto";
import * as vscode from "vscode";
import type { Conn } from "./daemon";

export function panelHtml(
  webview: vscode.Webview,
  root: vscode.Uri,
  conn: Conn,
  theme: "light" | "dark",
  l: Localizer,
): string {
  const asset = (f: string) =>
    webview.asWebviewUri(vscode.Uri.joinPath(root, "dist", "webview", f)).toString();
  const nonce = crypto.randomBytes(16).toString("base64");
  const config = JSON.stringify({
    url: `ws://127.0.0.1:${conn.port}/ws?token=${encodeURIComponent(conn.token)}`,
    theme,
  });
  const csp = [
    "default-src 'none'",
    `script-src 'nonce-${nonce}' ${webview.cspSource}`,
    `style-src ${webview.cspSource} 'unsafe-inline'`,
    `font-src ${webview.cspSource} data:`,
    `img-src ${webview.cspSource} data: blob:`,
    `connect-src ws://127.0.0.1:${conn.port}`,
  ].join("; ");
  return `<!doctype html>
<html lang="${l.lang}" data-theme="${theme}">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${l.t("app-name")}</title>
<link rel="stylesheet" href="${asset("webview.css")}">
</head>
<body>
<div id="root"></div>
<script nonce="${nonce}">window.POLYPH = ${config};</script>
<script nonce="${nonce}" type="module" src="${asset("webview.js")}"></script>
</body>
</html>`;
}
