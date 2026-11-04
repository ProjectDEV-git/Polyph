import { WebviewClient, type Port, type ToHost, type ToView } from "../src/bridge";
import { badge, claimsFor, relative, type Claim } from "../src/claims";
import { defaultHome } from "../src/daemon";

const claim = (path: string, label = "Bo/claude", member = "m2"): Claim => ({
  id: path,
  target: { path, symbol: null },
  owner: { id: "a1", member, label, kind: "claude", device: "d" },
  reason: "refactor",
  expires_at: 0,
});

describe("claims", () => {
  it("matches files and directories, most specific first", () => {
    const cs = [claim("src/"), claim("src/lib.rs", "Ann/codex"), claim("docs/a.md")];
    expect(claimsFor(cs, "src/lib.rs").map((c) => c.target.path)).toEqual(["src/lib.rs", "src/"]);
    expect(claimsFor(cs, "src/main.rs").map((c) => c.target.path)).toEqual(["src/"]);
    expect(claimsFor(cs, "srcx/a.rs")).toEqual([]);
    expect(badge(cs[1]!)).toBe("AC");
  });
  it("makes paths repository-relative", () => {
    expect(relative("/r/repo", "/r/repo/src/a.rs")).toBe("src/a.rs");
    expect(relative("/r/repo/", "/r/repo2/a.rs")).toBeNull();
    expect(relative("C:\\r\\repo", "C:\\r\\repo\\src\\a.rs")).toBe("src/a.rs");
  });
});

describe("daemon home", () => {
  it("follows the CLI's directories", () => {
    expect(defaultHome("linux", {}, "/home/a")).toBe("/home/a/.config/polyph");
    expect(defaultHome("linux", { XDG_CONFIG_HOME: "/x" }, "/home/a")).toBe("/x/polyph");
    expect(defaultHome("darwin", {}, "/Users/a")).toBe(
      "/Users/a/Library/Application Support/dev.polyph.polyph",
    );
    expect(defaultHome("linux", { POLYPH_HOME: "/p" }, "/home/a")).toBe("/p");
  });
});

describe("WebviewClient", () => {
  it("calls through the extension and receives events", async () => {
    const sent: ToHost[] = [];
    let deliver: (m: ToView) => void = () => {};
    const port: Port = { post: (m) => sent.push(m), listen: (f) => ((deliver = f), () => {}) };
    const c = new WebviewClient(port);
    const states: string[] = [];
    const events: unknown[] = [];
    c.onState = (s) => states.push(s);
    c.onEvent = (e) => events.push(e);
    c.start();
    expect(sent[0]).toEqual({ t: "ready" });
    await expect(c.call("ping")).rejects.toThrow("disconnected");
    deliver({ t: "state", state: "open" });
    const p = c.call<{ ok: boolean }>("ping", { a: 1 });
    const call = sent[1] as Extract<ToHost, { t: "call" }>;
    expect(call).toMatchObject({ t: "call", method: "ping", params: { a: 1 } });
    deliver({ t: "result", id: call.id, result: { ok: true } });
    await expect(p).resolves.toEqual({ ok: true });
    const q = c.call("nope");
    deliver({
      t: "result",
      id: (sent[2] as { id: number }).id,
      error: { message: "no", code: 404 },
    });
    await expect(q).rejects.toThrow("no");
    deliver({ t: "event", params: { room: "r" } });
    expect(events).toEqual([{ room: "r" }]);
    expect(states).toEqual(["open"]);
  });
});
