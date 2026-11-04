// App state fed by the daemon: fetched in parts, refreshed when an event
// says a part changed (the same model as the TUI, crates/polyph-tui).
import { useSyncExternalStore } from "react";
import type { ConnState, Transport } from "./api/client";
import type {
  ActivityEntry,
  Agent,
  AgentEvent,
  Approval,
  Board,
  Knock,
  ChatMessage,
  MergeEntry,
  Member,
  Room,
  StatusSnapshot,
  Terminal,
  ThreadSummary,
  Whoami,
  WhiteboardInfo,
} from "./api/types";

export const VIEWS = [
  "chat",
  "board",
  "agents",
  "activity",
  "merges",
  "terminals",
  "whiteboards",
  "settings",
] as const;
export type View = (typeof VIEWS)[number];

export const Part = {
  ROOMS: 1,
  MEMBERS: 2,
  CHAT: 4,
  BOARD: 8,
  AGENTS: 16,
  APPROVALS: 32,
  ACTIVITY: 64,
  MERGES: 128,
  TERMS: 256,
  TRANSCRIPT: 512,
  WHITEBOARDS: 1024,
  ME: 2048,
} as const;
export const ALL = 4095;

export type Load = "loading" | "ready" | "error";

export interface State {
  conn: ConnState;
  me: Whoami | null;
  rooms: Room[];
  room: string | null;
  members: Member[];
  chat: ChatMessage[];
  threads: ThreadSummary[];
  board: Board;
  agents: Agent[];
  approvals: Approval[];
  knocks: Knock[];
  activity: ActivityEntry[];
  merges: MergeEntry[];
  terms: Terminal[];
  whiteboards: WhiteboardInfo[];
  status: StatusSnapshot | null;
  agent: string | null;
  transcript: AgentEvent[];
  thread: string | null;
  view: View;
  /** Per part: whether it has loaded, so views show honest loading and error states. */
  load: Record<number, Load>;
  /** A short notice (an action's result or a notification). */
  notice: { text: string; at: number } | null;
}

const initial: State = {
  conn: "connecting",
  me: null,
  rooms: [],
  room: null,
  members: [],
  chat: [],
  threads: [],
  board: { columns: [], cards: [] },
  agents: [],
  approvals: [],
  knocks: [],
  activity: [],
  merges: [],
  terms: [],
  whiteboards: [],
  status: null,
  agent: null,
  transcript: [],
  thread: null,
  view: "chat",
  load: {},
  notice: null,
};

type Ev = { room?: string | null; event?: Record<string, any> };

/** Which parts of the state an event invalidates. */
export function partsFor(s: Pick<State, "room" | "agent">, ev: Ev): number {
  const e = ev.event ?? {};
  if (ev.room && s.room && ev.room !== s.room) return e.type === "connection" ? Part.ROOMS : 0;
  switch (e.type) {
    case "chat":
      return Part.CHAT;
    case "doc_changed": {
      const doc = e.doc;
      if (doc === "board") return Part.BOARD;
      if (doc === "activity") return Part.ACTIVITY;
      if (doc === "threads") return Part.CHAT;
      if (doc === "settings") return Part.WHITEBOARDS;
      if (doc && typeof doc === "object" && "whiteboard" in doc) return Part.WHITEBOARDS;
      return 0;
    }
    case "agent": {
      let p = 0;
      const t = e.event?.kind?.t;
      if (t === "started" || t === "state" || t === "stopped") p |= Part.AGENTS;
      if (s.agent && e.event?.agent?.id === s.agent) p |= Part.TRANSCRIPT;
      return p;
    }
    case "approval_pending":
    case "approval_resolved":
    case "knock":
    case "knock_decided":
      return Part.APPROVALS;
    case "member_online":
    case "member_offline":
    case "member_joined":
    case "member_removed":
      return Part.MEMBERS | Part.ROOMS;
    case "connection":
    case "keys_rotated":
      return Part.ROOMS;
    case "merge_queue":
      return Part.MERGES;
    case "terminal_opened":
    case "terminal_closed":
    case "terminal_control":
    case "terminal_resized":
      return Part.TERMS;
    default:
      return 0;
  }
}

export type TermListener = (bytes: Uint8Array) => void;
export type DocListener = (doc: unknown) => void;

function b64(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** A list from the daemon, or empty when the reply is not one. */
const arr = <T>(v: unknown): T[] => (Array.isArray(v) ? (v as T[]) : []);

export class Store {
  state: State = initial;
  private subs = new Set<() => void>();
  private pending = 0;
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private termSubs = new Map<string, Set<TermListener>>();
  private boardSubs = new Set<DocListener>();
  /** Native notification hook (desktop) for `notification` events. */
  onNotification: (text: string, kind: string) => void = () => {};

  constructor(readonly api: Transport) {}

  subscribe = (f: () => void) => {
    this.subs.add(f);
    return () => {
      this.subs.delete(f);
    };
  };

  get = () => this.state;

  set(patch: Partial<State>) {
    this.state = { ...this.state, ...patch };
    for (const f of this.subs) f();
  }

  private loaded(part: number, l: Load) {
    this.set({ load: { ...this.state.load, [part]: l } });
  }

  say(text: string) {
    this.set({ notice: { text, at: Date.now() } });
  }

  setConn(c: ConnState) {
    this.set({ conn: c });
    if (c === "open") void this.refresh(ALL);
  }

  setStatus(params: unknown) {
    const p = params as Record<string, unknown>;
    // The first line is a full snapshot; later lines are changes, after which
    // the snapshot is fetched again (cheap, and never drifts).
    if (p && "agents" in p && "approvals" in p)
      this.set({ status: p as unknown as StatusSnapshot });
    else
      this.api.call<StatusSnapshot>("status").then(
        (s) => this.set({ status: s }),
        () => {},
      );
  }

  handleEvent(params: unknown) {
    const ev = params as Ev;
    const e = ev.event ?? {};
    if (e.type === "terminal_output" && typeof e.id === "string" && typeof e.data === "string") {
      const subs = this.termSubs.get(e.id);
      if (subs) {
        const bytes = b64(e.data);
        for (const f of subs) f(bytes);
      }
    }
    if (e.type === "notification" && e.notification?.text) {
      this.say(String(e.notification.text));
      this.onNotification(String(e.notification.text), String(e.notification.kind ?? ""));
    }
    if (e.type === "doc_changed" && e.doc && typeof e.doc === "object" && "whiteboard" in e.doc) {
      for (const f of this.boardSubs) f(e.doc.whiteboard);
    }
    if (e.type === "agent" && this.state.agent && e.event?.agent?.id === this.state.agent) {
      // Transcript lines arrive in the event itself; append without a fetch.
      const a = e.event as AgentEvent;
      if (!this.state.transcript.some((x) => x.seq === a.seq))
        this.set({ transcript: [...this.state.transcript, a] });
    }
    let p = partsFor(this.state, ev) & ~Part.TRANSCRIPT;
    if (p) this.queue(p);
  }

  onTerminal(id: string, f: TermListener) {
    let s = this.termSubs.get(id);
    if (!s) this.termSubs.set(id, (s = new Set()));
    s.add(f);
    return () => {
      s.delete(f);
    };
  }

  onWhiteboard(f: DocListener) {
    this.boardSubs.add(f);
    return () => {
      this.boardSubs.delete(f);
    };
  }

  /** Coalesces refreshes: one fetch per part per 100 ms. */
  queue(parts: number) {
    this.pending |= parts;
    if (this.flushTimer) return;
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      const p = this.pending;
      this.pending = 0;
      void this.refresh(p);
    }, 100);
  }

  private async fetch<T>(
    part: number,
    method: string,
    params: Record<string, unknown>,
    apply: (v: T) => void,
  ) {
    if (this.state.load[part] === undefined) this.loaded(part, "loading");
    try {
      apply(await this.api.call<T>(method, params));
      this.loaded(part, "ready");
    } catch {
      this.loaded(part, "error");
    }
  }

  async refresh(parts: number): Promise<void> {
    const jobs: Promise<void>[] = [];
    if (parts & Part.ME)
      jobs.push(this.fetch<Whoami>(Part.ME, "whoami", {}, (me) => this.set({ me })));
    if (parts & Part.ROOMS) {
      await this.fetch<Room[]>(Part.ROOMS, "room.list", {}, (v) => this.set({ rooms: arr(v) }));
      const { rooms, room } = this.state;
      if (!room || !rooms.some((r) => r.id === room)) {
        let current: string | null = null;
        try {
          const s = await this.api.call<StatusSnapshot>("status");
          this.set({ status: s });
          current = s.room;
        } catch {}
        const next = current ?? rooms.find((r) => r.open)?.id ?? null;
        if (next !== room) {
          this.set({ room: next, agent: null, transcript: [], thread: null });
          if (next) parts = ALL & ~Part.ROOMS;
        }
      }
    }
    const room = this.state.room;
    if (!room) {
      await Promise.all(jobs);
      return;
    }
    const r = { room };
    if (parts & Part.MEMBERS)
      jobs.push(
        this.fetch<Member[]>(Part.MEMBERS, "room.members", r, (v) => this.set({ members: arr(v) })),
      );
    if (parts & Part.CHAT) {
      jobs.push(
        this.fetch<ChatMessage[]>(Part.CHAT, "chat.list", { room, limit: 500 }, (chat) =>
          this.set({ chat: arr<ChatMessage>(chat).sort((a, b) => a.at - b.at) }),
        ),
      );
      jobs.push(
        this.fetch<ThreadSummary[]>(Part.CHAT, "thread.list", r, (v) =>
          this.set({ threads: arr(v) }),
        ),
      );
    }
    if (parts & Part.BOARD)
      jobs.push(
        this.fetch<Board>(Part.BOARD, "board.get", r, (board) =>
          this.set({ board: { columns: arr(board?.columns), cards: arr(board?.cards) } }),
        ),
      );
    if (parts & Part.AGENTS)
      jobs.push(
        this.fetch<Agent[]>(Part.AGENTS, "agent.list", r, (v) => this.set({ agents: arr(v) })),
      );
    if (parts & Part.APPROVALS)
      jobs.push(
        this.fetch<Approval[]>(Part.APPROVALS, "approvals.list", r, (v) =>
          this.set({ approvals: arr(v) }),
        ),
      );
    // Only owners and maintainers can list people waiting to join.
    if (parts & Part.APPROVALS)
      jobs.push(
        this.api.call<Knock[]>("knock.list", r).then(
          (v) => this.set({ knocks: arr(v) }),
          () => this.set({ knocks: [] }),
        ),
      );
    if (parts & Part.ACTIVITY)
      jobs.push(
        this.fetch<ActivityEntry[]>(Part.ACTIVITY, "activity.list", { room, limit: 300 }, (v) =>
          this.set({ activity: arr(v) }),
        ),
      );
    if (parts & Part.MERGES)
      jobs.push(
        this.fetch<MergeEntry[]>(Part.MERGES, "merge.queue", r, (v) =>
          this.set({ merges: arr(v) }),
        ),
      );
    if (parts & Part.TERMS)
      jobs.push(
        this.fetch<Terminal[]>(Part.TERMS, "term.list", r, (v) => this.set({ terms: arr(v) })),
      );
    if (parts & Part.WHITEBOARDS)
      jobs.push(
        this.fetch<WhiteboardInfo[]>(Part.WHITEBOARDS, "whiteboard.list", r, (v) =>
          this.set({ whiteboards: arr(v) }),
        ),
      );
    if (parts & (Part.TRANSCRIPT | Part.AGENTS) && this.state.agent) {
      const agent = this.state.agent;
      jobs.push(
        this.fetch<{ events: AgentEvent[] }>(
          Part.TRANSCRIPT,
          "agent.transcript",
          { room, agent },
          (v) => {
            if (this.state.agent === agent) this.set({ transcript: v.events ?? [] });
          },
        ),
      );
    }
    await Promise.all(jobs);
  }

  selectRoom(id: string) {
    if (id === this.state.room) return;
    this.set({
      room: id,
      agent: null,
      transcript: [],
      thread: null,
      members: [],
      chat: [],
      threads: [],
      board: { columns: [], cards: [] },
      agents: [],
      approvals: [],
      knocks: [],
      activity: [],
      merges: [],
      terms: [],
      whiteboards: [],
      load: { [Part.ROOMS]: "ready", [Part.ME]: this.state.load[Part.ME] ?? "loading" },
    });
    // Make it the daemon's current room too, so the CLI and pill follow.
    this.api.call("room.open", { room: id }).catch(() => {});
    void this.refresh(ALL & ~Part.ROOMS & ~Part.ME);
  }

  selectAgent(id: string | null) {
    this.set({ agent: id, transcript: [], view: "agents" });
    if (id) void this.refresh(Part.TRANSCRIPT);
  }

  /** Runs an action; on failure the error text becomes the notice. */
  async act<T>(
    method: string,
    params: Record<string, unknown>,
    done?: string,
  ): Promise<T | undefined> {
    try {
      const v = await this.api.call<T>(method, params);
      if (done) this.say(done);
      return v;
    } catch (e) {
      this.say(e instanceof Error ? e.message : String(e));
      return undefined;
    }
  }

  /** The person's approvals: the ones their own agents wait on. */
  myApprovals(): Approval[] {
    const me = this.state.me?.member;
    return this.state.approvals.filter((a) => !me || a.agent.member === me);
  }
}

export function useStore<T>(store: Store, sel: (s: State) => T): T {
  return useSyncExternalStore(
    store.subscribe,
    () => sel(store.state),
    () => sel(store.state),
  );
}
