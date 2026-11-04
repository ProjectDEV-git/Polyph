// One daemon connection for the whole extension, shared by the status bar,
// claim decorations, approval prompts and every webview.
import { DaemonClient, RpcError, type ConnState } from "@polyph/ui/core";
import WebSocket from "ws";
import type { ToHost, ToView } from "./bridge";

type Ev = { room?: string | null; event?: Record<string, any> };

export class Hub {
  client: DaemonClient | null = null;
  state: ConnState = "connecting";
  status: Record<string, any> | null = null;
  private events = new Set<(e: Ev) => void>();
  private statuses = new Set<(s: unknown) => void>();
  private states = new Set<(s: ConnState) => void>();

  /** Connects to `url` (replacing an earlier connection). */
  connect(url: string) {
    this.client?.stop();
    // Node's WebSocket from the `ws` package: it sends no Origin header, as a
    // local client should not.
    const c = new DaemonClient(url, (u) => new WebSocket(u) as unknown as globalThis.WebSocket);
    c.onEvent = (p) => {
      for (const f of this.events) f(p as Ev);
    };
    c.onStatus = (p) => {
      const s = p as Record<string, any>;
      if (s && "agents" in s && "approvals" in s) this.status = s;
      else
        void c.call<Record<string, any>>("status").then(
          (x) => {
            this.status = x;
            for (const f of this.statuses) f(x);
          },
          () => {},
        );
      for (const f of this.statuses) f(p);
    };
    c.onState = (s) => {
      this.state = s;
      for (const f of this.states) f(s);
    };
    this.client = c;
    c.start();
  }

  stop() {
    this.client?.stop();
    this.client = null;
  }

  call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    if (!this.client) return Promise.reject(new RpcError("disconnected", -1));
    return this.client.call<T>(method, params);
  }

  onEvent(f: (e: Ev) => void) {
    this.events.add(f);
    return { dispose: () => void this.events.delete(f) };
  }
  onStatus(f: (s: unknown) => void) {
    this.statuses.add(f);
    return { dispose: () => void this.statuses.delete(f) };
  }
  onState(f: (s: ConnState) => void) {
    this.states.add(f);
    return { dispose: () => void this.states.delete(f) };
  }

  /** Serves one webview: forwards its calls, and events, status and state to it. */
  serve(post: (m: ToView) => void, retry: () => void, open: (url: string) => void) {
    const offs = [
      this.onEvent((params) => post({ t: "event", params })),
      this.onStatus((params) => post({ t: "status", params })),
      this.onState((state) => post({ t: "state", state })),
    ];
    const receive = (m: ToHost) => {
      switch (m.t) {
        case "ready":
          post({ t: "state", state: this.state });
          if (this.status) post({ t: "status", params: this.status });
          return;
        case "retry":
          return retry();
        case "open":
          return open(m.url);
        case "call":
          this.call(m.method, m.params).then(
            (result) => post({ t: "result", id: m.id, result }),
            (e: unknown) =>
              post({
                t: "result",
                id: m.id,
                error: {
                  message: e instanceof Error ? e.message : String(e),
                  code: e instanceof RpcError ? e.code : -1,
                },
              }),
          );
      }
    };
    return { receive, dispose: () => offs.forEach((o) => o.dispose()) };
  }
}
