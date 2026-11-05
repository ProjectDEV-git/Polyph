// Activates the extension against a fake daemon (JSON-RPC over a real
// WebSocket) with a stand-in `vscode` module.
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WebSocketServer, type WebSocket } from "ws";
import * as extension from "../src/extension";
import { Uri, recorded, workspace } from "./vscode-mock";

const ME = "aa".repeat(16);
const ROOM = "bb".repeat(16);
const status = {
  room: ROOM,
  room_name: "payments",
  connected: true,
  members_online: 2,
  agents: [
    {
      id: "a1",
      label: "Bo/claude",
      owner: "Bo",
      color: "oklch(0.7 0.1 30)",
      state: "running",
      task: null,
      mine: false,
      cloud: false,
    },
  ],
  approvals: [],
  budgets: [],
};
const claims = [
  {
    id: "c1",
    target: { path: "src/lib.rs", symbol: null },
    owner: { id: "a1", member: "cc".repeat(16), label: "Bo/claude", kind: "claude", device: "d" },
    reason: "Add OAuth login",
    expires_at: 0,
  },
];
const approval = {
  id: "ap1",
  agent: { id: "a2", member: ME, label: "Ann/claude", kind: "claude", device: "d" },
  kind: { t: "command", c: { command: "cargo publish", cwd: "/r" } },
  task: null,
  created_at: 0,
  escalate_at: 0,
};

let server: Server;
let socket: WebSocket | null = null;
const calls: { method: string; params: any }[] = [];

beforeAll(async () => {
  server = createServer((req, res) => res.end(req.url === "/healthz" ? "ok" : ""));
  const wss = new WebSocketServer({ server, path: "/ws" });
  wss.on("connection", (ws, req) => {
    if (!req.url?.includes("token=t0k3n")) return ws.close();
    socket = ws;
    ws.on("message", (raw) => {
      const m = JSON.parse(String(raw));
      calls.push({ method: m.method, params: m.params });
      const reply = (result: unknown) =>
        ws.send(JSON.stringify({ jsonrpc: "2.0", id: m.id, result }));
      switch (m.method) {
        case "status.watch":
          return ws.send(JSON.stringify({ jsonrpc: "2.0", method: "status", params: status }));
        case "whoami":
          return reply({ member: ME });
        case "status":
          return reply(status);
        case "claims.list":
          return reply(claims);
        case "approvals.list":
          return reply([approval]);
        case "phone.pair":
          return reply({ link: "https://polyph.dev/phone/#join=X", code: "A1B2-CDEF" });
        default:
          return reply({});
      }
    });
  });
  await new Promise<void>((ok) => server.listen(0, "127.0.0.1", ok));
  const port = (server.address() as { port: number }).port;
  const home = mkdtempSync(join(tmpdir(), "polyph-vscode-"));
  writeFileSync(join(home, "daemon.json"), JSON.stringify({ port }));
  writeFileSync(join(home, "api.token"), "t0k3n\n");
  recorded.config.home = home;
  const repo = mkdtempSync(join(tmpdir(), "polyph-repo-"));
  mkdirSync(join(repo, ".git"));
  workspace.workspaceFolders = [{ uri: Uri.file(join(repo, "src")) }];
  (globalThis as any).__repo = repo;
});

afterAll(() => {
  extension.deactivate();
  server.close();
});

const until = async (what: string, f: () => boolean) => {
  for (let i = 0; i < 100; i++) {
    if (f()) return;
    await new Promise((r) => setTimeout(r, 20));
  }
  throw new Error(`timed out: ${what}`);
};

describe("extension", () => {
  it("registers Polyph as an MCP server for Copilot", () => {
    extension.activate({ subscriptions: [], extensionUri: Uri.file("/ext") } as any);
    const p = recorded.mcpProviders.get("polyph");
    expect(p).toBeDefined();
    const [d] = p!.provideMcpServerDefinitions();
    expect(d.command).toBe("polyph");
    expect(d.args).toEqual(["mcp", "--client", "vscode"]);
    expect(d.env.POLYPH_HOME).toBe(recorded.config.home);
    expect(d.cwd.fsPath).toContain("polyph-repo-");
    extension.deactivate();
  });

  it("shows the room, claims and approvals from the daemon", async () => {
    extension.activate({ subscriptions: [], extensionUri: Uri.file("/ext") } as any);
    await until("status bar", () => !!recorded.bar?.text.includes("payments"));
    expect(recorded.bar!.text).toContain("1 agent");

    // Claims become file decorations, relative to the repository root.
    await until("claims", () => calls.some((c) => c.method === "claims.list"));
    const repo = (globalThis as any).__repo as string;
    const deco = recorded.decorationProviders[0]!;
    await until(
      "decoration",
      () => !!deco.provideFileDecoration(Uri.file(join(repo, "src/lib.rs"))),
    );
    const d = deco.provideFileDecoration(Uri.file(join(repo, "src/lib.rs")));
    expect(d.badge).toBe("BC");
    expect(d.tooltip).toBe("Claimed by Bo/claude: Add OAuth login");
    expect(d.color.id).toBe("polyph.claimedByOther");
    expect(deco.provideFileDecoration(Uri.file(join(repo, "src/main.rs")))).toBeUndefined();

    // An approval for one of my agents asks me, with the exact command.
    recorded.answer = "Approve once";
    socket!.send(
      JSON.stringify({
        jsonrpc: "2.0",
        method: "event",
        params: { room: ROOM, event: { type: "approval_pending", approval } },
      }),
    );
    await until("approve sent", () => calls.some((c) => c.method === "approve"));
    expect(recorded.warnings[0]!.text).toContain("cargo publish");
    expect(recorded.warnings[0]!.items).toEqual(["Approve once", "Approve for this task", "Deny"]);
    expect(calls.find((c) => c.method === "approve")!.params).toEqual({ room: ROOM, id: "ap1" });

    // Deny with a note, through the command.
    recorded.answer = "Deny";
    recorded.input = "not on a Friday";
    await recorded.commands.get("polyph.approvals")!();
    await until("deny sent", () => calls.some((c) => c.method === "deny"));
    expect(calls.find((c) => c.method === "deny")!.params).toEqual({
      room: ROOM,
      id: "ap1",
      note: "not on a Friday",
    });

    // Pairing a phone copies the link.
    await recorded.commands.get("polyph.pairPhone")!();
    expect(recorded.clipboard).toBe("https://polyph.dev/phone/#join=X");
  });
});
