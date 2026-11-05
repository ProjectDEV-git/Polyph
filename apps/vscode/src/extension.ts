// Polyph in VS Code: who holds which files (claims as file and editor
// decorations), approvals as notifications with the exact action, a status
// bar summary, and the shared app in a sidebar view and an editor tab. All of
// it comes from the person's own daemon, like every other surface.
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { Localizer, describeApproval, pickLang, type Approval } from "@polyph/ui/core";
import * as vscode from "vscode";
import type { ToHost } from "./bridge";
import { badge, claimsFor, relative, type Claim } from "./claims";
import { defaultHome, ensureDaemon, wsUrl } from "./daemon";
import { Hub } from "./hub";
import { registerMcp } from "./mcp";

let hub: Hub | null = null;

/** The repository root holding `dir`, or `dir` itself when none. */
export function repoRoot(dir: string): string {
  let d = dir;
  for (;;) {
    if (existsSync(join(d, ".git"))) return d;
    const up = dirname(d);
    if (up === d) return dir;
    d = up;
  }
}

/** What the extension exposes to other extensions and its own tests. */
export interface PolyphApi {
  state(): string;
  status(): Record<string, any> | null;
}

export function activate(ctx: vscode.ExtensionContext): PolyphApi {
  const l = new Localizer(pickLang([vscode.env.language]));
  const h = new Hub();
  hub = h;
  let me: string | null = null;
  let claims: Claim[] = [];
  const colors = new Map<string, string>();

  const cfg = () => vscode.workspace.getConfiguration("polyph");
  const home = () => cfg().get<string>("home") || defaultHome();
  const connect = async () => {
    try {
      const c = await ensureDaemon(home(), cfg().get<string>("command") || "polyph");
      h.connect(wsUrl(c));
    } catch {
      bar.text = `$(debug-disconnect) ${l.t("vscode-status-offline")}`;
      bar.tooltip = l.t("vscode-status-offline-hint");
      bar.command = "polyph.reconnect";
      bar.backgroundColor = undefined;
      bar.show();
    }
  };

  // Status bar: the room, running agents, and approvals waiting for me.
  const bar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
  ctx.subscriptions.push(bar);
  const mine = (): Record<string, any>[] =>
    (h.status?.approvals ?? []).filter((a: { mine: boolean }) => a.mine);
  const render = () => {
    const s = h.status;
    bar.command = mine().length ? "polyph.approvals" : "polyph.openApp";
    if (h.state !== "open" || !s) {
      bar.command = "polyph.reconnect";
      bar.text = `$(sync~spin) ${l.t("vscode-status-connecting")}`;
      bar.tooltip = undefined;
      bar.backgroundColor = undefined;
    } else if (!s.room) {
      bar.text = `$(organization) ${l.t("vscode-status-no-room")}`;
      bar.tooltip = undefined;
      bar.backgroundColor = undefined;
    } else {
      const running = (s.agents ?? []).filter(
        (a: { state: string }) => a.state !== "stopped",
      ).length;
      const waiting = mine().length;
      bar.text =
        `$(organization) ${s.room_name ?? ""} · ${l.t("vscode-status-agents", { n: running })}` +
        (waiting ? ` · $(bell-dot) ${l.t("vscode-status-approvals", { n: waiting })}` : "");
      bar.tooltip = waiting ? l.t("vscode-status-approvals-hint") : l.t("vscode-status-hint");
      bar.backgroundColor = waiting
        ? new vscode.ThemeColor("statusBarItem.warningBackground")
        : undefined;
      for (const a of s.agents ?? []) if (a.color) colors.set(a.id, a.color);
    }
    bar.show();
  };

  // Claims: file badges (propagated to folders) and a note in the editor.
  const roots = () => (vscode.workspace.workspaceFolders ?? []).map((f) => repoRoot(f.uri.fsPath));
  const relOf = (uri: vscode.Uri): string | null => {
    if (uri.scheme !== "file") return null;
    for (const r of roots()) {
      const rel = relative(r, uri.fsPath);
      if (rel) return rel;
    }
    return null;
  };
  const changed = new vscode.EventEmitter<vscode.Uri | vscode.Uri[] | undefined>();
  ctx.subscriptions.push(
    changed,
    vscode.window.registerFileDecorationProvider({
      onDidChangeFileDecorations: changed.event,
      provideFileDecoration(uri) {
        const rel = relOf(uri);
        if (rel === null) return undefined;
        const c = claimsFor(claims, rel)[0];
        if (!c) return undefined;
        const ours = c.owner.member === me;
        return {
          badge: badge(c),
          tooltip: l.t(ours ? "vscode-claim-mine" : "vscode-claim-other", {
            agent: c.owner.label,
            reason: c.reason,
          }),
          color: new vscode.ThemeColor(ours ? "polyph.claimedByMe" : "polyph.claimedByOther"),
          propagate: true,
        };
      },
    }),
  );
  const noteTypes = new Map<string, vscode.TextEditorDecorationType>();
  const noteType = (color: string) => {
    let t = noteTypes.get(color);
    if (!t) {
      t = vscode.window.createTextEditorDecorationType({
        isWholeLine: true,
        overviewRulerColor: color,
        overviewRulerLane: vscode.OverviewRulerLane.Left,
        borderWidth: "0 0 0 2px",
        borderStyle: "solid",
        borderColor: color,
        after: { color, margin: "0 0 0 2em" },
      });
      noteTypes.set(color, t);
      ctx.subscriptions.push(t);
    }
    return t;
  };
  const decorate = () => {
    for (const ed of vscode.window.visibleTextEditors) {
      for (const t of noteTypes.values()) ed.setDecorations(t, []);
      const rel = relOf(ed.document.uri);
      const c = rel === null ? undefined : claimsFor(claims, rel)[0];
      if (!c || c.owner.member === me) continue;
      const color = colors.get(c.owner.id) || "#e0b26a";
      const where = c.target.symbol ? `${c.target.path}#${c.target.symbol}` : c.target.path;
      ed.setDecorations(noteType(color), [
        {
          range: new vscode.Range(0, 0, 0, 0),
          renderOptions: {
            after: {
              contentText: l.t("vscode-claim-note", { agent: c.owner.label, target: where }),
            },
          },
          hoverMessage: c.reason,
        },
      ]);
    }
  };
  const warned = new Set<string>();
  ctx.subscriptions.push(
    vscode.window.onDidChangeVisibleTextEditors(decorate),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (!e.contentChanges.length) return;
      const rel = relOf(e.document.uri);
      const c = rel === null ? undefined : claimsFor(claims, rel)[0];
      if (!c || c.owner.member === me || warned.has(c.id)) return;
      warned.add(c.id);
      void vscode.window.showWarningMessage(
        l.t("vscode-claim-editing", { agent: c.owner.label, file: rel }),
      );
    }),
  );
  const refreshClaims = async () => {
    const room = h.status?.room;
    try {
      claims = room ? await h.call<Claim[]>("claims.list", { room }) : [];
    } catch {
      claims = [];
    }
    changed.fire(undefined);
    decorate();
  };

  // Approvals: the agent's owner is asked at once, with the exact action.
  const answer = async (room: string, a: Approval) => {
    const once = l.t("app-approve-once");
    const task = l.t("app-approve-task");
    const deny = l.t("app-deny");
    const pick = await vscode.window.showWarningMessage(
      l.t("vscode-approval", { agent: a.agent.label, action: describeApproval(l, a) }),
      { modal: false },
      once,
      task,
      deny,
    );
    try {
      if (pick === once) await h.call("approve", { room, id: a.id });
      else if (pick === task) await h.call("approve", { room, id: a.id, task: true });
      else if (pick === deny) {
        const note = await vscode.window.showInputBox({ prompt: l.t("vscode-deny-note") });
        await h.call("deny", { room, id: a.id, note: note || undefined });
      }
    } catch (e) {
      void vscode.window.showErrorMessage(e instanceof Error ? e.message : String(e));
    }
  };

  ctx.subscriptions.push(
    { dispose: () => h.stop() },
    h.onState((s) => {
      render();
      if (s === "open")
        void h.call<{ member: string }>("whoami").then((w) => {
          me = w.member;
          void refreshClaims();
        });
    }),
    h.onStatus(() => {
      render();
      void refreshClaims();
    }),
    h.onEvent((ev) => {
      const e = ev.event ?? {};
      if (["claims_changed", "claim_decision", "claim_revoked", "connection"].includes(e.type))
        void refreshClaims();
      if (e.type === "approval_pending" && ev.room && e.approval?.agent?.member === me)
        void answer(ev.room, e.approval as Approval);
    }),
  );

  // The shared app, in the sidebar and in an editor tab.
  const media = vscode.Uri.joinPath(ctx.extensionUri, "media");
  const html = (w: vscode.Webview) => {
    const nonce = [...crypto.getRandomValues(new Uint8Array(16))]
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
    const asset = (p: string) => w.asWebviewUri(vscode.Uri.joinPath(media, "webview", p));
    const base = `${asset("").toString().replace(/\/?$/, "/")}`;
    const csp = [
      "default-src 'none'",
      `img-src ${w.cspSource} data: blob:`,
      `font-src ${w.cspSource} data:`,
      `style-src ${w.cspSource} 'unsafe-inline'`,
      `script-src 'nonce-${nonce}' ${w.cspSource}`,
      "worker-src blob:",
    ].join("; ");
    const theme =
      vscode.window.activeColorTheme.kind === vscode.ColorThemeKind.Light ||
      vscode.window.activeColorTheme.kind === vscode.ColorThemeKind.HighContrastLight
        ? "light"
        : "dark";
    return `<!doctype html>
<html lang="${l.lang}" data-theme="${theme}">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<meta name="viewport" content="width=device-width, initial-scale=1">
<link rel="stylesheet" href="${asset("webview.css")}">
<script nonce="${nonce}">window.POLYPH_ASSET_BASE = ${JSON.stringify(base)};</script>
</head>
<body><div id="root"></div>
<script type="module" nonce="${nonce}" src="${asset("webview.js")}"></script>
</body>
</html>`;
  };
  const attach = (w: vscode.Webview): vscode.Disposable => {
    w.options = { enableScripts: true, localResourceRoots: [media] };
    w.html = html(w);
    const s = h.serve(
      (m) => void w.postMessage(m),
      () => void connect(),
      (url) => void vscode.env.openExternal(vscode.Uri.parse(url)),
    );
    const sub = w.onDidReceiveMessage((m: ToHost) => s.receive(m));
    return { dispose: () => (sub.dispose(), s.dispose()) };
  };
  // Copilot and other agents in VS Code get Polyph's tools without a settings file.
  const version =
    (ctx as { extension?: { packageJSON?: { version?: string } } }).extension?.packageJSON
      ?.version ?? "0";
  registerMcp(
    ctx,
    () => cfg().get<string>("command") || "polyph",
    () => cfg().get<string>("home") || undefined,
    version,
  );
  ctx.subscriptions.push(
    vscode.window.registerWebviewViewProvider(
      "polyph.room",
      {
        resolveWebviewView(view) {
          const d = attach(view.webview);
          view.onDidDispose(() => d.dispose());
        },
      },
      { webviewOptions: { retainContextWhenHidden: true } },
    ),
    vscode.commands.registerCommand("polyph.openApp", () => {
      const p = vscode.window.createWebviewPanel(
        "polyph.app",
        l.t("app-name"),
        vscode.ViewColumn.Active,
        { retainContextWhenHidden: true },
      );
      p.iconPath = vscode.Uri.joinPath(media, "mark.svg");
      const d = attach(p.webview);
      p.onDidDispose(() => d.dispose());
    }),
    vscode.commands.registerCommand("polyph.reconnect", () => connect()),
    vscode.commands.registerCommand("polyph.approvals", async () => {
      const room = h.status?.room;
      if (!room) return vscode.commands.executeCommand("polyph.openApp");
      let list: Approval[] = [];
      try {
        list = await h.call<Approval[]>("approvals.list", { room });
      } catch {}
      const items = list.filter((a) => a.agent.member === me);
      if (!items.length) {
        void vscode.window.showInformationMessage(l.t("vscode-no-approvals"));
        return;
      }
      const pick = await vscode.window.showQuickPick(
        items.map((a) => ({ label: a.agent.label, detail: describeApproval(l, a), a })),
        { placeHolder: l.t("vscode-pick-approval") },
      );
      if (pick) await answer(room, pick.a);
    }),
    vscode.commands.registerCommand("polyph.pairPhone", async () => {
      const room = h.status?.room;
      if (!room) {
        void vscode.window.showInformationMessage(l.t("vscode-status-no-room"));
        return;
      }
      try {
        const r = await h.call<{ link: string; code: string }>("phone.pair", { room });
        await vscode.env.clipboard.writeText(r.link);
        const open = l.t("vscode-open-app");
        const pick = await vscode.window.showInformationMessage(
          l.t("vscode-phone-copied", { code: r.code }),
          open,
        );
        if (pick === open) await vscode.commands.executeCommand("polyph.openApp");
      } catch (e) {
        void vscode.window.showErrorMessage(e instanceof Error ? e.message : String(e));
      }
    }),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("polyph")) void connect();
    }),
  );
  render();
  void connect();
  return { state: () => h.state, status: () => h.status };
}

export function deactivate() {
  hub?.stop();
  hub = null;
}
