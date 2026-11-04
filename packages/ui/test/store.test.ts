import type { Transport } from "../src/api/client";
import { Part, Store, partsFor } from "../src/store";

class FakeApi implements Transport {
  calls: [string, Record<string, unknown>][] = [];
  constructor(private answers: Record<string, unknown>) {}
  async call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    this.calls.push([method, params]);
    if (!(method in this.answers)) throw new Error(`no answer for ${method}`);
    return this.answers[method] as T;
  }
}

const room = "r1";
const me = { name: "Ann", member: "m1", device: "d1", relay: "wss://x", keystore: "file" };
const approval = (id: string, member: string) => ({
  id,
  agent: { id: "a1", member, device: "d", kind: "claude", label: "Ann/claude" },
  kind: { t: "command", c: { command: "cargo publish", cwd: "/repo" } },
  task: null,
  created_at: 0,
  escalate_at: 0,
});

describe("partsFor", () => {
  const s = { room, agent: "a1" };
  it("maps events to the parts they change", () => {
    expect(partsFor(s, { room, event: { type: "chat" } })).toBe(Part.CHAT);
    expect(partsFor(s, { room, event: { type: "doc_changed", doc: "board" } })).toBe(Part.BOARD);
    expect(partsFor(s, { room, event: { type: "doc_changed", doc: { whiteboard: "w" } } })).toBe(
      Part.WHITEBOARDS,
    );
    expect(partsFor(s, { room, event: { type: "approval_pending" } })).toBe(Part.APPROVALS);
    expect(partsFor(s, { room, event: { type: "knock" } })).toBe(Part.APPROVALS);
    expect(
      partsFor(s, {
        room,
        event: { type: "agent", event: { agent: { id: "a1" }, kind: { t: "state" } } },
      }),
    ).toBe(Part.AGENTS | Part.TRANSCRIPT);
  });
  it("ignores other rooms except for connection changes", () => {
    expect(partsFor(s, { room: "other", event: { type: "chat" } })).toBe(0);
    expect(partsFor(s, { room: "other", event: { type: "connection" } })).toBe(Part.ROOMS);
  });
});

describe("Store", () => {
  it("picks the daemon's current room and loads every part", async () => {
    const api = new FakeApi({
      whoami: me,
      "room.list": [{ id: room, name: "payments", open: true }],
      status: { room, agents: [], approvals: [] },
      "room.members": [{ member: "m1", name: "Ann", hue: 255, online: true, me: true }],
      "chat.list": [
        { id: "2", at: 20, text: "second" },
        { id: "1", at: 10, text: "first" },
      ],
      "thread.list": [],
      "board.get": { columns: [], cards: [] },
      "agent.list": [],
      "approvals.list": [approval("p1", "m1"), approval("p2", "m2")],
      "knock.list": [],
      "activity.list": [],
      "merge.queue": [],
      "term.list": [],
      "whiteboard.list": [],
    });
    const s = new Store(api);
    await s.refresh(4095);
    expect(s.state.room).toBe(room);
    expect(s.state.chat.map((m) => m.text)).toEqual(["first", "second"]);
    expect(s.state.load[Part.CHAT]).toBe("ready");
    // Only approvals for this person's own agents need them.
    expect(s.myApprovals().map((a) => a.id)).toEqual(["p1"]);
  });

  it("marks a part that failed so its view can offer Retry", async () => {
    const api = new FakeApi({
      "room.list": [{ id: room, name: "x", open: true }],
      status: { room },
    });
    const s = new Store(api);
    await s.refresh(Part.ROOMS);
    await s.refresh(Part.BOARD);
    expect(s.state.load[Part.BOARD]).toBe("error");
  });

  it("streams terminal output to its listeners and appends transcript lines", () => {
    const s = new Store(new FakeApi({}));
    s.set({ room, agent: "a1" });
    const got: number[][] = [];
    const off = s.onTerminal("t1", (b) => got.push([...b]));
    s.handleEvent({ room, event: { type: "terminal_output", id: "t1", data: btoa("hi") } });
    off();
    s.handleEvent({ room, event: { type: "terminal_output", id: "t1", data: btoa("no") } });
    expect(got).toEqual([[104, 105]]);
    const ev = { agent: { id: "a1" }, seq: 7, at: 0, kind: { t: "message", c: { text: "hello" } } };
    s.handleEvent({ room, event: { type: "agent", event: ev } });
    s.handleEvent({ room, event: { type: "agent", event: ev } });
    expect(s.state.transcript).toHaveLength(1);
  });

  it("turns notifications into a notice and a native notification", () => {
    const s = new Store(new FakeApi({}));
    const seen: string[] = [];
    s.onNotification = (t) => seen.push(t);
    s.handleEvent({
      room,
      event: { type: "notification", notification: { text: "Bo/codex needs approval" } },
    });
    expect(s.state.notice?.text).toBe("Bo/codex needs approval");
    expect(seen).toEqual(["Bo/codex needs approval"]);
  });
});
