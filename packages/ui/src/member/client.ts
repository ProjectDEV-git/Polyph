// The phone's connection: instead of a daemon, a room member running in this
// page (crates/polyph-web as WebAssembly) talking to the relay. It answers the
// same API methods and streams the same events, so the app runs unchanged.
import type { AppClient, ConnState } from "../api/client";
import { RpcError } from "../api/client";
import init, { WebMember, newIdentity } from "./pkg/polyph_web.js";
import { memberStorage } from "./storage";

const now = () => Date.now() / 1000;

let ready: Promise<unknown> | null = null;
/** Loads the WebAssembly module once. */
export const loadEngine = () => (ready ??= init());

export class MemberClient implements AppClient {
  state: ConnState = "connecting";
  onEvent: (params: unknown) => void = () => {};
  onStatus: (params: unknown) => void = () => {};
  onState: (s: ConnState) => void = () => {};
  private ws: WebSocket | null = null;
  private retry = 0;
  private stopped = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private ticker: ReturnType<typeof setInterval> | null = null;
  private saving = false;
  private waiters: ((ev: Record<string, any>) => boolean)[] = [];

  constructor(readonly member: WebMember) {}

  /** A member for a device that has not joined yet. */
  static async fresh(name: string, relay: string): Promise<MemberClient> {
    await loadEngine();
    const id = await memberStorage.identity(() => new Uint8Array(newIdentity()));
    await memberStorage.saveProfile({ name, relay });
    return new MemberClient(new WebMember(id, name, relay));
  }

  /** The member saved on this device, if it has joined a room. */
  static async restore(): Promise<MemberClient | null> {
    const [room, profile] = await Promise.all([memberStorage.room(), memberStorage.profile()]);
    if (!room || !profile) return null;
    await loadEngine();
    const id = await memberStorage.identity(() => new Uint8Array(newIdentity()));
    return new MemberClient(WebMember.load(id, profile.name, room, now()));
  }

  start(): void {
    this.stopped = false;
    this.open();
    this.ticker ??= setInterval(() => {
      this.member.tick(now());
      this.pump();
    }, 5000);
  }

  stop(): void {
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
    if (this.ticker) clearInterval(this.ticker);
    this.ticker = null;
    this.ws?.close();
  }

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
      ws = new WebSocket(this.member.relay);
    } catch {
      this.schedule();
      return;
    }
    ws.binaryType = "arraybuffer";
    this.ws = ws;
    ws.onopen = () => {
      this.retry = 0;
      this.member.connected();
      this.pump();
      this.set("open");
    };
    ws.onmessage = (m) => {
      if (m.data instanceof ArrayBuffer) this.member.onFrame(new Uint8Array(m.data), now());
      this.pump();
    };
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.member.disconnected();
      this.pump();
      this.set("closed");
      this.schedule();
    };
  }

  private schedule() {
    if (this.stopped) return;
    const delay = Math.min(15_000, 500 * 2 ** this.retry++);
    this.timer = setTimeout(() => this.open(), delay);
  }

  /** Sends what the engine produced, hands events to the app, saves the room. */
  private pump() {
    const frames = this.member.takeFrames();
    if (this.ws?.readyState === WebSocket.OPEN)
      for (const f of frames) this.ws.send(f as Uint8Array<ArrayBuffer>);
    const events = JSON.parse(this.member.takeEvents()) as Record<string, any>[];
    let statusChanged = false;
    for (const e of events) {
      this.waiters = this.waiters.filter((w) => !w(e));
      this.onEvent(e);
      const t = e.event?.type;
      if (
        t === "approval_pending" ||
        t === "approval_resolved" ||
        t === "agent" ||
        t === "connection"
      )
        statusChanged = true;
    }
    if (statusChanged || events.length)
      this.onStatus(JSON.parse(this.member.call("status", "{}", now())));
    if (this.member.takeDirty()) void this.save();
  }

  private async save() {
    if (this.saving) return;
    this.saving = true;
    try {
      const data = this.member.save();
      if (data) await memberStorage.saveRoom(new Uint8Array(data));
    } finally {
      this.saving = false;
    }
  }

  async call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    if (method === "join") return this.join(params) as Promise<T>;
    let out: string;
    try {
      out = this.member.call(method, JSON.stringify(params), now());
    } catch (e) {
      throw new RpcError(e instanceof Error ? e.message : String(e), -32000);
    } finally {
      this.pump();
    }
    return JSON.parse(out) as T;
  }

  /** Joins (or links) and resolves once the room admits this device. */
  private join(params: Record<string, unknown>): Promise<unknown> {
    return new Promise((ok, no) => {
      this.waiters.push((e) => {
        const t = e.event?.type;
        if (t === "joined") ok({ id: e.event.room });
        else if (t === "join_failed") no(new RpcError(String(e.event.message), -32001));
        else return false;
        return true;
      });
      try {
        this.member.call("join", JSON.stringify(params), now());
      } catch (e) {
        this.waiters.pop();
        no(new RpcError(e instanceof Error ? e.message : String(e), -32602));
      }
      this.pump();
    });
  }
}
