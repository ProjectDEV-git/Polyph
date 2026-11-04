// The webview's connection: the shared app runs in a VS Code webview and
// reaches the daemon through the extension (postMessage), so the API token
// never enters the webview and the daemon sees no foreign page origin.
import { RpcError, type AppClient, type ConnState } from "@polyph/ui/core";

export type ToHost =
  | { t: "ready" }
  | { t: "call"; id: number; method: string; params: Record<string, unknown> }
  | { t: "retry" }
  | { t: "open"; url: string };

export type ToView =
  | { t: "result"; id: number; result?: unknown; error?: { message: string; code: number } }
  | { t: "event"; params: unknown }
  | { t: "status"; params: unknown }
  | { t: "state"; state: ConnState };

export interface Port {
  post(m: ToHost): void;
  listen(f: (m: ToView) => void): () => void;
}

export class WebviewClient implements AppClient {
  state: ConnState = "connecting";
  onEvent: (params: unknown) => void = () => {};
  onStatus: (params: unknown) => void = () => {};
  onState: (s: ConnState) => void = () => {};
  private next = 1;
  private pending = new Map<
    number,
    { resolve: (v: unknown) => void; reject: (e: Error) => void }
  >();
  private off: (() => void) | null = null;

  constructor(private port: Port) {}

  start(): void {
    this.off = this.port.listen((m) => this.receive(m));
    this.port.post({ t: "ready" });
  }

  stop(): void {
    this.off?.();
    this.off = null;
  }

  retryNow(): void {
    this.port.post({ t: "retry" });
  }

  private receive(m: ToView) {
    switch (m.t) {
      case "state":
        this.state = m.state;
        if (m.state !== "open") {
          for (const p of this.pending.values()) p.reject(new RpcError("disconnected", -1));
          this.pending.clear();
        }
        this.onState(m.state);
        return;
      case "event":
        return this.onEvent(m.params);
      case "status":
        return this.onStatus(m.params);
      case "result": {
        const p = this.pending.get(m.id);
        if (!p) return;
        this.pending.delete(m.id);
        if (m.error) p.reject(new RpcError(m.error.message, m.error.code));
        else p.resolve(m.result);
      }
    }
  }

  call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    if (this.state !== "open") return Promise.reject(new RpcError("disconnected", -1));
    const id = this.next++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.port.post({ t: "call", id, method, params });
    });
  }
}
