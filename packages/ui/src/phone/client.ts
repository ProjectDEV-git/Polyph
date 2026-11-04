// The phone's transport: the shared app talks to the phone engine
// (crates/polyph-phone, compiled to wasm) exactly as it talks to a daemon.
// The engine has no I/O; this class owns the relay WebSocket, the tick and
// the saving, and turns the engine's output into calls, events and status.
import { RpcError, type ConnState, type Transport } from "../api/client";
import type { StatusSnapshot } from "../api/types";

/** One call's output, as the wasm `PhoneOutput` exposes it. */
export interface EngineOutput {
  readonly frameCount: number;
  frame(i: number): Uint8Array;
  readonly events: string;
  readonly dirty: boolean;
  readonly result?: string;
  readonly error?: string;
  readonly code: number;
  free?(): void;
}

/** The wasm `PhoneEngine`; tests pass a fake. */
export interface Engine {
  readonly relay: string;
  readonly hasRoom: boolean;
  connected(): EngineOutput;
  disconnected(): EngineOutput;
  frame(bytes: Uint8Array, now: number): EngineOutput;
  tick(now: number): EngineOutput;
  join(code: string, name?: string): EngineOutput;
  call(method: string, params: string, now: number): EngineOutput;
  save(): Uint8Array;
}

type Ev = { room?: string | null; event?: Record<string, any> };

export interface PhoneOptions {
  socket?: (url: string) => WebSocket;
  /** Unix seconds. */
  now?: () => number;
  /** Stores the engine's saved state (sealed by the caller). */
  save?: (bytes: Uint8Array) => void | Promise<void>;
  /** Text for a native notification about an event, or null for none. */
  describe?: (ev: Ev, me: string | null) => string | null;
  /** Milliseconds between engine ticks. */
  tickMs?: number;
}

/** Events after which the status snapshot (pill, "Needs you") is rebuilt. */
const STATUS_EVENTS = new Set([
  "agent",
  "approval_pending",
  "approval_resolved",
  "member_online",
  "member_offline",
  "member_joined",
  "member_removed",
  "connection",
  "relay",
]);

export class PhoneClient implements Transport {
  state: ConnState = "connecting";
  onEvent: (params: unknown) => void = () => {};
  onStatus: (params: unknown) => void = () => {};
  onState: (s: ConnState) => void = () => {};
  private ws: WebSocket | null = null;
  private retry = 0;
  private stopped = true;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private ticker: ReturnType<typeof setTimeout> | null = null;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private statusTimer: ReturnType<typeof setTimeout> | null = null;
  private joining: { resolve: (v: unknown) => void; reject: (e: Error) => void } | null = null;
  private me: string | null = null;
  private readonly now: () => number;
  private readonly socket: (url: string) => WebSocket;

  constructor(
    readonly engine: Engine,
    private opts: PhoneOptions = {},
  ) {
    this.now = opts.now ?? (() => Math.floor(Date.now() / 1000));
    this.socket = opts.socket ?? ((u) => new WebSocket(u));
  }

  start(): void {
    this.stopped = false;
    this.me = this.whoami();
    this.open();
    this.loop();
    if (typeof document !== "undefined") document.addEventListener("visibilitychange", this.onHide);
  }

  /** Engine ticks and saving, on a chain of timeouts. */
  private loop = () => {
    if (this.stopped) return;
    try {
      this.handle(this.engine.tick(this.now()));
    } catch (e) {
      console.error("polyph: engine tick failed", e);
    }
    this.ticker = setTimeout(this.loop, this.opts.tickMs ?? 1000);
  };

  stop(): void {
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
    if (this.ticker) clearTimeout(this.ticker);
    if (typeof document !== "undefined")
      document.removeEventListener("visibilitychange", this.onHide);
    this.ws?.close();
    this.flushSave();
  }

  retryNow(): void {
    if (this.state === "open") return;
    if (this.timer) clearTimeout(this.timer);
    this.retry = 0;
    this.open();
  }

  private whoami(): string | null {
    try {
      const out = this.engine.call("whoami", "{}", this.now());
      return out.result ? (JSON.parse(out.result).member ?? null) : null;
    } catch {
      return null;
    }
  }

  private set(s: ConnState) {
    if (this.state === s) return;
    this.state = s;
    this.onState(s);
  }

  private open() {
    this.set("connecting");
    let ws: WebSocket;
    try {
      ws = this.socket(this.engine.relay);
    } catch {
      this.schedule();
      return;
    }
    ws.binaryType = "arraybuffer";
    this.ws = ws;
    ws.onopen = () => this.handle(this.engine.connected());
    ws.onmessage = (m) => {
      if (m.data instanceof ArrayBuffer)
        this.handle(this.engine.frame(new Uint8Array(m.data), this.now()));
    };
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.handle(this.engine.disconnected());
      this.set("closed");
      this.schedule();
    };
  }

  private schedule() {
    if (this.stopped) return;
    const delay = Math.min(10_000, 500 * 2 ** this.retry++);
    this.timer = setTimeout(() => this.open(), delay);
  }

  /** Sends an output's frames and passes on its events. */
  private handle(out: EngineOutput) {
    try {
      const ws = this.ws;
      for (let i = 0; i < out.frameCount; i++) {
        const f = out.frame(i);
        if (ws && ws.readyState === 1) ws.send(f as Uint8Array<ArrayBuffer>);
      }
      let status = false;
      for (const ev of JSON.parse(out.events || "[]") as Ev[]) {
        const e = ev.event ?? {};
        switch (e.type) {
          case "relay":
            // Authenticated with the relay: the app may call now.
            this.retry = 0;
            this.set("open");
            status = true;
            continue;
          case "joined":
            // The room's keys exist only here until saved: save at once.
            this.queueSave();
            this.flushSave();
            this.joining?.resolve({ id: e.room, name: e.name });
            this.joining = null;
            break;
          case "join_failed":
            this.joining?.reject(new RpcError(String(e.message ?? ""), 403));
            this.joining = null;
            break;
        }
        if (STATUS_EVENTS.has(e.type)) status = true;
        try {
          this.onEvent(ev);
          const text = this.opts.describe?.(ev, this.me);
          if (text)
            this.onEvent({
              room: ev.room ?? null,
              event: { type: "notification", notification: { text, kind: e.type } },
            });
        } catch (err) {
          // One bad event must not stop the rest, or the saving below.
          console.error("polyph: event handler failed", err);
        }
      }
      if (status) this.queueStatus();
    } finally {
      if (out.dirty) this.queueSave();
      out.free?.();
    }
  }

  private queueStatus() {
    if (this.statusTimer) return;
    this.statusTimer = setTimeout(() => {
      this.statusTimer = null;
      this.call<StatusSnapshot>("status").then(
        (s) => this.onStatus(s),
        () => {},
      );
    }, 100);
  }

  private onHide = () => {
    // A phone may kill a hidden page at any time.
    if (document.visibilityState === "hidden") this.flushSave();
  };

  /** Saves soon after a change: a page can be closed or reloaded at any time. */
  private queueSave() {
    if (this.saveTimer) return;
    this.saveTimer = setTimeout(() => this.flushSave(), 250);
  }

  private flushSave() {
    if (!this.saveTimer) return;
    clearTimeout(this.saveTimer);
    this.saveTimer = null;
    if (!this.opts.save) return;
    try {
      void this.opts.save(this.engine.save());
    } catch (e) {
      console.error("polyph: saving failed", e);
    }
  }

  call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    if (method === "join") {
      if (this.joining) return Promise.reject(new RpcError("a join is already waiting", -1));
      const code = String(params.code ?? "");
      const name = typeof params.name === "string" ? params.name : undefined;
      return new Promise<T>((resolve, reject) => {
        this.joining = { resolve: resolve as (v: unknown) => void, reject };
        const out = this.engine.join(code, name);
        if (out.error) {
          this.joining = null;
          const e = new RpcError(out.error, out.code);
          out.free?.();
          reject(e);
          return;
        }
        this.handle(out);
      }).then((v) => {
        this.me = this.whoami();
        return v;
      });
    }
    let out: EngineOutput;
    try {
      out = this.engine.call(method, JSON.stringify(params), this.now());
    } catch (e) {
      return Promise.reject(new RpcError(e instanceof Error ? e.message : String(e), -1));
    }
    if (out.error !== undefined && out.error !== null) {
      const e = new RpcError(out.error, out.code);
      out.free?.();
      return Promise.reject(e);
    }
    const result = out.result;
    this.handle(out);
    return Promise.resolve((result ? JSON.parse(result) : null) as T);
  }
}
