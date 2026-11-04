// Polyph in VS Code: who holds which file (voice-colored claim gutters and
// explorer badges), your agents' approvals as prompts, a status bar item, and
// the whole app in a panel. A client of the local daemon like every surface.
import { describeApproval } from "@polyph/ui/approval-text";
import { DaemonClient } from "@polyph/ui/client";
import { Localizer, pickLang } from "@polyph/ui/i18n";
import type { Theme } from "@polyph/ui/voice";
import * as path from "node:path";
import * as vscode from "vscode";
import WebSocket from "ws";
import { type Claim, type FileClaim, type Member, claimFor, fileClaims } from "./claims";
import { type Conn, healthy, polyphHome, readConn, startDaemon } from "./daemon";
import { panelHtml } from "./panel";

type Approval = Parameters<typeof describeApproval>[1];
type Status = {
  room: string | null;
  room_name: string | null;
  connected: boolean;
  agents: { state: string; mine: boolean }[];
  approvals: { mine: boolean }[];
};

export function activate(ctx: vscode.ExtensionContext) {
  const l = new Localizer(pickLang([vscode.env.language]));
  const cfg = () => vscode.workspace.getConfiguration("polyph");
  const bar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  bar.name = l.t("app-name");
  ctx.subscriptions.push(bar);

  let client: DaemonClient | null = null;
  let conn: Conn | null = null;
  let room: string | null = null;
  let me: string | null = null;
  let claims: FileClaim[] = [];
  let approvals: Approval[] = [];
  const prompted = new Set<string>();
  const theme = (): Theme =>
    vscode.window.activeColorTheme.kind === vscode.ColorThemeKind.Light ||
    vscode.window.activeColorTheme.kind === vscode.ColorThemeKind.HighContrastLight
      ? "light"
      : "dark";

  // Explorer badges: the initial of the voice that holds the file.
  const fileDeco = new vscode.EventEmitter<vscode.Uri[] | undefined>();
  ctx.subscriptions.push(
    vscode.window.registerFileDecorationProvider({
      onDidChangeFileDecorations: fileDeco.event,
      provideFileDecoration(uri) {
        const c = claimOf(uri);
        if (!c || c.mine) return undefined;
        return new vscode.FileDecoration(
          c.label.slice(0, 1).toUpperCase(),
          l.t("vscode-claimed-by", { label: c.label, reason: c.reason }),
        );
      },
    }),
  );

  function relPath(uri: vscode.Uri): string | null {
    const folder = vscode.workspace.getWorkspaceFolder(uri);
    if (!folder) return null;
    return path.relative(folder.uri.fsPath, uri.fsPath);
  }
  function claimOf(uri: vscode.Uri): FileClaim | undefined {
    if (!cfg().get<boolean>("claimDecorations", true)) return undefined;
    const rel = relPath(uri);
    return rel ? claimFor(claims, rel) : undefined;
  }

  // Editor gutters: a 3px line in the holder's voice, with the reason on hover.
  const decoTypes = new Map<string, vscode.TextEditorDecorationType>();
  const decoType = (color: string) => {
    let t = decoTypes.get(color);
    if (!t) {
      t = vscode.window.createTextEditorDecorationType({
        isWholeLine: true,
        borderWidth: "0 0 0 3px",
        borderStyle: "solid",
        borderColor: color,
        overviewRulerColor: color,
        overviewRulerLane: vscode.OverviewRulerLane.Left,
      });
      decoTypes.set(color, t);
      ctx.subscriptions.push(t);
    }
    return t;
  };
  function decorate() {
    for (const ed of vscode.window.visibleTextEditors) {
      for (const t of decoTypes.values()) ed.setDecorations(t, []);
      const c = claimOf(ed.document.uri);
      if (!c || c.mine) continue;
      const last = Math.max(0, ed.document.lineCount - 1);
      const hover = new vscode.MarkdownString(
        l.t("vscode-claimed-by", { label: c.label, reason: c.reason }),
      );
      ed.setDecorations(decoType(c.color), [
        { range: new vscode.Range(0, 0, 0, 0), hoverMessage: hover },
        { range: new vscode.Range(1, 0, last, 0) },
      ]);
    }
    fileDeco.fire(undefined);
  }
  ctx.subscriptions.push(vscode.window.onDidChangeVisibleTextEditors(decorate));

  async function refreshClaims() {
    if (!client || !room) {
      claims = [];
      return decorate();
    }
    try {
      const [cl, members] = await Promise.all([
        client.call<Claim[]>("claims.list", { room }),
        client.call<Member[]>("room.members", { room }),
      ]);
      me = members.find((m) => m.me)?.member ?? null;
      claims = fileClaims(cl, members, theme());
    } catch {
      claims = [];
    }
    decorate();
  }

  async function refreshApprovals() {
    if (!client || !room) return;
    try {
      const all = await client.call<Approval[]>("approvals.list", { room });
      approvals = all.filter((a) => a.agent.member === me);
    } catch {
      approvals = [];
    }
    if (!cfg().get<boolean>("approvalPrompts", true)) return;
    for (const a of approvals) {
      if (prompted.has(a.id)) continue;
      prompted.add(a.id);
      void prompt(a);
    }
  }

  async function answer(a: Approval, approve: boolean) {
    if (!client) return;
    try {
      await client.call(approve ? "approve" : "deny", { room, id: a.id });
      void vscode.window.setStatusBarMessage(l.t(approve ? "tui-approved" : "tui-denied"), 3000);
    } catch (e) {
      void vscode.window.showErrorMessage(e instanceof Error ? e.message : String(e));
    }
  }

  async function prompt(a: Approval) {
    const once = l.t("app-approve-once");
    const deny = l.t("app-deny");
    const pick = await vscode.window.showInformationMessage(describeApproval(l, a), once, deny);
    if (pick === once) await answer(a, true);
    else if (pick === deny) await answer(a, false);
  }

  function showStatus(s: Status | null) {
    if (!client || !s) {
      bar.text = `$(circle-slash) ${l.t("app-name")}`;
      bar.tooltip = l.t("vscode-not-running");
      bar.command = "polyph.startDaemon";
      bar.show();
      return;
    }
    const running = s.agents.filter((a) => a.state === "running" || a.state === "starting").length;
    const waiting = s.approvals.filter((a) => a.mine).length;
    const parts = [`$(pulse) ${running}`];
    if (waiting) parts.push(`$(bell-dot) ${waiting}`);
    bar.text = `${s.room_name ?? l.t("app-name")} ${parts.join(" ")}`;
    bar.tooltip = [
      s.connected ? l.t("tui-connected") : l.t("tui-offline"),
      l.t("app-pill-agents", { n: running }),
      waiting ? l.t("tui-approvals", { n: waiting }) : l.t("app-nothing-needs-you"),
    ].join("\n");
    bar.command = waiting ? "polyph.approvals" : "polyph.open";
    bar.backgroundColor = waiting
      ? new vscode.ThemeColor("statusBarItem.warningBackground")
      : undefined;
    bar.show();
  }

  async function connect(): Promise<boolean> {
    const home = polyphHome(cfg().get<string>("home", ""));
    const c = readConn(home);
    if (!c || !(await healthy(c.port))) return false;
    conn = c;
    client?.stop();
    const token = c.token;
    const cl = new DaemonClient(
      `ws://127.0.0.1:${c.port}/ws`,
      (u) =>
        new WebSocket(u, {
          headers: { authorization: `Bearer ${token}` },
        }) as unknown as globalThis.WebSocket,
    );
    let refreshing: ReturnType<typeof setTimeout> | null = null;
    const soon = () => {
      if (refreshing) return;
      refreshing = setTimeout(() => {
        refreshing = null;
        void refreshClaims().then(refreshApprovals);
      }, 150);
    };
    cl.onStatus = (p) => {
      const s = p as Partial<Status>;
      if (s && "agents" in s) {
        if (s.room !== room) {
          room = s.room ?? null;
          soon();
        }
        showStatus(s as Status);
      } else {
        void cl.call<Status>("status").then(showStatus, () => {});
      }
    };
    cl.onEvent = (p) => {
      const e = (p as { room?: string; event?: { type?: string } }).event?.type;
      if (
        e === "claims_changed" ||
        e === "approval_pending" ||
        e === "approval_resolved" ||
        e?.startsWith("member_")
      )
        soon();
    };
    cl.onState = (st) => {
      if (st === "open") soon();
      else if (st === "closed") showStatus(null);
    };
    client = cl;
    cl.start();
    return true;
  }

  let panel: vscode.WebviewPanel | null = null;
  ctx.subscriptions.push(
    vscode.commands.registerCommand("polyph.open", async () => {
      if (!conn && !(await connect()))
        return void vscode.window.showWarningMessage(l.t("vscode-not-running"));
      if (panel) return panel.reveal();
      panel = vscode.window.createWebviewPanel(
        "polyph",
        l.t("app-name"),
        vscode.ViewColumn.Beside,
        {
          enableScripts: true,
          retainContextWhenHidden: true,
          localResourceRoots: [vscode.Uri.joinPath(ctx.extensionUri, "dist", "webview")],
        },
      );
      panel.iconPath = vscode.Uri.joinPath(ctx.extensionUri, "media", "icon.png");
      panel.webview.html = panelHtml(panel.webview, ctx.extensionUri, conn!, theme(), l);
      panel.webview.onDidReceiveMessage((m: { type?: string; url?: string }) => {
        if (m.type === "open" && m.url && /^https?:\/\//.test(m.url))
          void vscode.env.openExternal(vscode.Uri.parse(m.url));
      });
      panel.onDidDispose(() => (panel = null));
    }),
    vscode.commands.registerCommand("polyph.approvals", async () => {
      await refreshApprovals();
      if (!approvals.length)
        return void vscode.window.showInformationMessage(l.t("app-nothing-needs-you"));
      const pick = await vscode.window.showQuickPick(
        approvals.map((a) => ({ label: a.agent.label, detail: describeApproval(l, a), a })),
        { placeHolder: l.t("app-needs-you") },
      );
      if (pick) await prompt(pick.a);
    }),
    vscode.commands.registerCommand("polyph.startDaemon", async () => {
      const ok = await startDaemon(polyphHome(cfg().get<string>("home", "")));
      if (!ok) return void vscode.window.showErrorMessage(l.t("vscode-start-failed"));
      for (let i = 0; i < 20 && !(await connect()); i++)
        await new Promise((r) => setTimeout(r, 250));
    }),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("polyph")) void connect().then(refreshClaims);
    }),
    vscode.window.onDidChangeActiveColorTheme(() => void refreshClaims()),
    { dispose: () => client?.stop() },
  );

  // Keep trying quietly: the daemon may start after VS Code.
  showStatus(null);
  const retry = setInterval(() => {
    if (client?.state === "open") return;
    void connect();
  }, 10_000);
  ctx.subscriptions.push({ dispose: () => clearInterval(retry) });
  void connect();
}

export function deactivate() {}
