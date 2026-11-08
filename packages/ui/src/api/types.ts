// Shapes returned by the daemon's local API (see crates/polyph-daemon).

export type Hex = string;

export interface Room {
  id: Hex;
  name: string;
  open: boolean;
  connected: boolean;
  quick: boolean;
  role: "owner" | "maintainer" | "member" | "viewer";
  members: number;
  members_online: number;
  relay?: string | null;
}

export interface Member {
  member: Hex;
  name: string;
  role: string;
  online: boolean;
  hue: number;
  devices: number;
  me: boolean;
}

export interface ChatMessage {
  id: Hex;
  author: Hex;
  name: string;
  agent: string | null;
  thread: Hex | null;
  text: string;
  at: number;
}

export interface ThreadInfo {
  id: Hex;
  title: string;
  card: Hex | null;
  created_at: number;
  created_by: Hex;
}

export interface ThreadSummary {
  thread: ThreadInfo;
  posts: number;
  last_at: number;
}

export interface Column {
  id: string;
  title: string;
}

export interface Card {
  id: Hex;
  title: string;
  description: string;
  column: string;
  order: number;
  assignee: Hex | null;
  suggested_assignee: Hex | null;
  branch: string | null;
  pr: { number?: number; url?: string } | number | null;
  url: string | null;
  /** What the agent that finished it says it did. */
  summary?: string | null;
  draft: boolean;
  plan: string | null;
  files: string[];
  acceptance: string[];
  depends_on: Hex[];
  size: string | null;
  kind: string | null;
  thread: Hex | null;
  created_at: number;
  created_by: Hex;
}

export interface Board {
  columns: Column[];
  cards: Card[];
}

export type AgentStateName =
  "starting" | "idle" | "running" | "waiting_approval" | "blocked" | "error" | "done" | "stopped";

export interface Agent {
  id: Hex;
  kind: string;
  label: string;
  member: Hex;
  mine: boolean;
  state: AgentStateName;
}

export interface AgentRef {
  id: Hex;
  member: Hex;
  device: Hex;
  kind: string;
  label: string;
}

export interface Tagged<T extends string = string, C = Record<string, unknown>> {
  t: T;
  c: C;
}

export interface AgentEvent {
  agent: AgentRef;
  seq: number;
  at: number;
  kind: Tagged;
}

export interface Approval {
  id: Hex;
  agent: AgentRef;
  kind: Tagged;
  task: Hex | null;
  created_at: number;
  escalate_at: number;
}

export interface ActivityEntry {
  id: Hex;
  actor: Hex;
  agent: string | null;
  kind: string;
  summary_key: string;
  args: Record<string, unknown>;
  at: number;
  checkpoint: unknown;
}

export interface MergeEntry {
  id?: Hex;
  card?: Hex;
  title?: string;
  branch?: string;
  state?: string;
  agent?: string;
  [k: string]: unknown;
}

export interface Terminal {
  id: Hex;
  title: string;
  owner: Hex;
  device: Hex;
  cols: number;
  rows: number;
  closed: boolean;
  exit: number | null;
  writer: Hex | null;
}

export interface WhiteboardInfo {
  id: Hex;
  name: string;
  created_by: Hex;
  created_at: number;
}

export interface StatusSnapshot {
  room: Hex | null;
  room_name: string | null;
  connected: boolean;
  members_online: number;
  agents: {
    id: Hex;
    label: string;
    owner: string;
    color: string;
    state: AgentStateName;
    task: string | null;
    mine: boolean;
    cloud: boolean;
  }[];
  approvals: { id: Hex; agent: string; title: string; created_at: number; mine: boolean }[];
  budgets: unknown[];
}

export interface Whoami {
  name: string;
  member: Hex;
  device: Hex;
  relay: string;
  keystore: string;
}

/** `{"method":"event","params":...}` from the daemon. */
export interface DaemonEvent {
  room?: Hex;
  event?: Tagged | Record<string, unknown>;
  [k: string]: unknown;
}

/** Someone waiting to join a room (owners and maintainers decide). */
export interface Knock {
  id: Hex;
  display_name: string;
  member: Hex;
  device: Hex;
  requested_role: string;
  at: number;
  /** Another device joining as this person, through a pairing or agent host code made here. */
  own_device?: boolean;
}
