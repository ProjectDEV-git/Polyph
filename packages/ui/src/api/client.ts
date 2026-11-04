// JSON-RPC 2.0 over the daemon's localhost WebSocket, with reconnects.

export type ConnState = "connecting" | "open" | "closed";

export class RpcError extends Error {
  constructor(
    message: string,
    readonly code: number,
  ) {
    super(message);
  }
}

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

/** What the client needs from a WebSocket; tests pass a fake. */
export type SocketFactory = (url: string) => WebSocket;

export interface Transport {
  call<T = unknown>(method: string, params?: Record<string, unknown>): Promise<T>;
}

/** A connection the app can run on: a daemon's WebSocket or the phone engine. */
export interface AppClient extends Transport {
  state: ConnState;
  onEvent: (params: unknown) => void;
  onStatus: (params: unknown) => void;
  onState: (s: ConnState) => void;
  start(): void;
  stop(): void;
  retryNow(): void;
}

export class DaemonClient implements AppClient {
  private ws: WebSocket | null = null;
  private next = 1;
  private pending = new Map<number, Pending>();
  private retry = 0;
  private stopped = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  state: ConnState = "connecting";
  onEvent: (params: unknown) => void = () => {};
  onStatus: (params: unknown) => void = () => {};
  onState: (s: ConnState) => void = () => {};

  constructor(
    private url: string,
    private socket: SocketFactory = (u) => new WebSocket(u),
  ) {}

  start(): void {
    this.stopped = false;
    this.open();
  }

  stop(): void {
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
    this.ws?.close();
  }

  /** Reconnect now instead of waiting for the backoff. */
  retryNow(): void {
    if (this.state === "open") return;
    if (this.timer) clearTimeout(this.timer);
    this.retry = 0;
    this.open();
  }

  private set(s: ConnState) {
    this.state = s;
    this.onState(s);
  }

  private open() {
    this.set("connecting");
    let ws: WebSocket;
    try {
      ws = this.socket(this.url);
    } catch {
      this.schedule();
      return;
    }
    this.ws = ws;
    ws.onopen = () => {
      this.retry = 0;
      this.set("open");
      // The status feed backs the pill and the approval count.
      ws.send(JSON.stringify({ jsonrpc: "2.0", id: 0, method: "status.watch" }));
    };
    ws.onmessage = (m) => this.receive(String(m.data));
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      for (const p of this.pending.values()) p.reject(new RpcError("disconnected", -1));
      this.pending.clear();
      this.set("closed");
      this.schedule();
    };
  }

  private schedule() {
    if (this.stopped) return;
    const delay = Math.min(10_000, 500 * 2 ** this.retry++);
    this.timer = setTimeout(() => this.open(), delay);
  }

  private receive(text: string) {
    let msg: {
      id?: number;
      method?: string;
      params?: unknown;
      result?: unknown;
      error?: { code: number; message: string };
    };
    try {
      msg = JSON.parse(text);
    } catch {
      return;
    }
    if (msg.method === "event") return this.onEvent(msg.params);
    if (msg.method === "status") return this.onStatus(msg.params);
    if (typeof msg.id !== "number") return;
    const p = this.pending.get(msg.id);
    if (!p) return;
    this.pending.delete(msg.id);
    if (msg.error) p.reject(new RpcError(msg.error.message, msg.error.code));
    else p.resolve(msg.result);
  }

  call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    const ws = this.ws;
    if (!ws || this.state !== "open") return Promise.reject(new RpcError("disconnected", -1));
    const id = this.next++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
    });
  }
}
