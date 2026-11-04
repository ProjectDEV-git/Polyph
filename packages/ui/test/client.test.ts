import { DaemonClient } from "../src/api/client";

class FakeSocket {
  static last: FakeSocket;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((m: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  constructor(readonly url: string) {
    FakeSocket.last = this;
  }
  send(s: string) {
    this.sent.push(s);
  }
  close() {
    this.onclose?.();
  }
}

describe("DaemonClient", () => {
  it("watches the status feed, matches replies and routes events", async () => {
    const c = new DaemonClient(
      "ws://127.0.0.1:1/ws?token=t",
      (u) => new FakeSocket(u) as unknown as WebSocket,
    );
    const states: string[] = [];
    const events: unknown[] = [];
    c.onState = (s) => states.push(s);
    c.onEvent = (e) => events.push(e);
    c.start();
    const ws = FakeSocket.last;
    ws.onopen?.();
    expect(JSON.parse(ws.sent[0]!).method).toBe("status.watch");
    const p = c.call<{ ok: boolean }>("ping");
    const id = JSON.parse(ws.sent[1]!).id;
    ws.onmessage?.({
      data: JSON.stringify({ jsonrpc: "2.0", method: "event", params: { room: "r" } }),
    });
    ws.onmessage?.({ data: JSON.stringify({ jsonrpc: "2.0", id, result: { ok: true } }) });
    await expect(p).resolves.toEqual({ ok: true });
    const q = c.call("nope");
    const id2 = JSON.parse(ws.sent[2]!).id;
    ws.onmessage?.({
      data: JSON.stringify({
        jsonrpc: "2.0",
        id: id2,
        error: { code: -32601, message: "unknown method" },
      }),
    });
    await expect(q).rejects.toThrow("unknown method");
    expect(events).toEqual([{ room: "r" }]);
    expect(states).toEqual(["connecting", "open"]);
    c.stop();
  });

  it("fails calls while disconnected instead of hanging", async () => {
    const c = new DaemonClient("ws://x", (u) => new FakeSocket(u) as unknown as WebSocket);
    await expect(c.call("ping")).rejects.toThrow("disconnected");
  });
});
