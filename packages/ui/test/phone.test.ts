import { PhoneClient, type Engine, type EngineOutput } from "../src/phone/client";

class FakeSocket {
  static last: FakeSocket;
  sent: Uint8Array[] = [];
  readyState = 0;
  binaryType = "";
  onopen: (() => void) | null = null;
  onmessage: ((m: { data: ArrayBuffer }) => void) | null = null;
  onclose: (() => void) | null = null;
  constructor(readonly url: string) {
    FakeSocket.last = this;
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
  send(b: Uint8Array) {
    this.sent.push(b);
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
}

function out(
  p: Partial<{
    frames: number[][];
    events: unknown[];
    dirty: boolean;
    result: unknown;
    error: string;
    code: number;
  }>,
): EngineOutput {
  const frames = (p.frames ?? []).map((f) => new Uint8Array(f));
  return {
    frameCount: frames.length,
    frame: (i) => frames[i]!,
    events: JSON.stringify(p.events ?? []),
    dirty: p.dirty ?? false,
    result: p.result === undefined ? undefined : JSON.stringify(p.result),
    error: p.error,
    code: p.code ?? 0,
  };
}

/** Scripted engine: records what it was given and answers from a table. */
class FakeEngine implements Engine {
  relay = "wss://relay.test/ws";
  hasRoom = false;
  frames: number[][] = [];
  calls: string[] = [];
  joined: string[] = [];
  saved = 0;
  connected() {
    return out({ frames: [[1]] });
  }
  disconnected() {
    return out({ events: [{ room: "r1", event: { type: "connection", connected: false } }] });
  }
  frame(b: Uint8Array) {
    this.frames.push([...b]);
    if (b[0] === 9)
      return out({
        frames: [[2]],
        events: [{ room: null, event: { type: "relay", connected: true } }],
      });
    if (b[0] === 7) {
      this.hasRoom = true;
      return out({
        dirty: true,
        events: [{ room: null, event: { type: "joined", room: "r1", name: "payments" } }],
      });
    }
    if (b[0] === 8)
      return out({
        events: [
          {
            room: "r1",
            event: {
              type: "approval_pending",
              approval: { id: "a1", agent: { member: "m1", label: "Ann/claude" } },
            },
          },
        ],
      });
    return out({});
  }
  tick() {
    return out({});
  }
  join(code: string) {
    this.joined.push(code);
    if (code === "bad") return out({ error: "codes look like A1B2-CDEF", code: -32602 });
    return out({ frames: [[3]] });
  }
  call(method: string, params: string) {
    this.calls.push(method);
    if (method === "whoami") return out({ result: { member: "m1" } });
    if (method === "status") return out({ result: { room: "r1", agents: [], approvals: [] } });
    if (method === "chat.send") return out({ result: {}, frames: [[4]], dirty: true });
    if (method === "agent.start")
      return out({ error: "not available on this phone", code: -32601 });
    return out({ result: JSON.parse(params) });
  }
  save() {
    this.saved++;
    return new Uint8Array([5]);
  }
}

const ab = (b: number[]) => new Uint8Array(b).buffer;

describe("PhoneClient", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("authenticates, joins, sends frames and passes events and status on", async () => {
    const engine = new FakeEngine();
    const saves: Uint8Array[] = [];
    const c = new PhoneClient(engine, {
      socket: (u) => new FakeSocket(u) as unknown as WebSocket,
      now: () => 100,
      save: (b) => {
        saves.push(b);
      },
      describe: (ev, me) =>
        ev.event?.type === "approval_pending" && ev.event.approval.agent.member === me
          ? "Ann/claude needs you"
          : null,
    });
    const states: string[] = [];
    const events: any[] = [];
    const statuses: unknown[] = [];
    c.onState = (s) => states.push(s);
    c.onEvent = (e) => events.push(e);
    c.onStatus = (s) => statuses.push(s);
    c.start();
    const ws = FakeSocket.last;
    expect(ws.url).toBe("wss://relay.test/ws");
    expect(ws.binaryType).toBe("arraybuffer");
    ws.open();
    expect(ws.sent.map((b) => [...b])).toEqual([[1]]);
    // The relay's welcome: now the app may call.
    ws.onmessage?.({ data: ab([9]) });
    expect(states).toEqual(["open"]);
    expect([...ws.sent[1]!]).toEqual([2]);

    // join resolves when the engine reports the room.
    const joined = c.call<{ id: string }>("join", { code: "A1B2-CDEF", name: "Ann" });
    expect(engine.joined).toEqual(["A1B2-CDEF"]);
    expect([...ws.sent[2]!]).toEqual([3]);
    ws.onmessage?.({ data: ab([7]) });
    await expect(joined).resolves.toEqual({ id: "r1", name: "payments" });
    // A new room is saved at once: its keys exist nowhere else on this phone.
    expect(saves.length).toBe(1);
    await expect(c.call("join", { code: "bad" })).rejects.toThrow("A1B2-CDEF");

    // Calls answer from the engine and send what it produced.
    await expect(c.call("chat.send", { room: "r1", text: "hi" })).resolves.toEqual({});
    expect([...ws.sent[3]!]).toEqual([4]);
    await expect(c.call("agent.start", {})).rejects.toThrow("not available on this phone");

    // An approval for this person becomes a notification event too.
    ws.onmessage?.({ data: ab([8]) });
    expect(events.some((e) => e.event.type === "approval_pending")).toBe(true);
    expect(events.find((e) => e.event.type === "notification")?.event.notification.text).toBe(
      "Ann/claude needs you",
    );
    await vi.advanceTimersByTimeAsync(150);
    expect(statuses.at(-1)).toEqual({ room: "r1", agents: [], approvals: [] });
    // Later changes are saved soon after, coalesced.
    await vi.advanceTimersByTimeAsync(1100);
    expect(saves.length).toBe(2);

    // Losing the relay reconnects with backoff.
    ws.close();
    expect(states.at(-1)).toBe("closed");
    await vi.advanceTimersByTimeAsync(600);
    expect(FakeSocket.last).not.toBe(ws);
    c.stop();
  });

  it("answers local calls while the relay is unreachable", async () => {
    const engine = new FakeEngine();
    const c = new PhoneClient(engine, {
      socket: (u) => new FakeSocket(u) as unknown as WebSocket,
      now: () => 100,
    });
    c.start();
    await expect(c.call("chat.list", { room: "r1" })).resolves.toEqual({ room: "r1" });
    c.stop();
  });
});
