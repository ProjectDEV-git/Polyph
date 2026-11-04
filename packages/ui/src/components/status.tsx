// Status is never color alone: each state has an icon and a word (DESIGN.md).
import clsx from "clsx";
import { Ban, Check, Circle, Hand, Minus, OctagonX } from "lucide-react";
import type { AgentStateName } from "../api/types";
import { useT } from "../i18n";

export type Status = "running" | "waiting" | "blocked" | "error" | "done" | "idle" | "stopped";

export function statusOf(s: AgentStateName | string): Status {
  switch (s) {
    case "running":
    case "starting":
      return "running";
    case "waiting_approval":
      return "waiting";
    case "blocked":
      return "blocked";
    case "error":
      return "error";
    case "done":
      return "done";
    case "stopped":
      return "stopped";
    default:
      return "idle";
  }
}

const color: Record<Status, string> = {
  running: "text-running",
  waiting: "text-waiting",
  blocked: "text-blocked",
  error: "text-error",
  done: "text-ink-2",
  idle: "text-ink-2",
  stopped: "text-ink-3",
};

export function StatusIcon({ s, size = 14 }: { s: Status; size?: number }) {
  const cls = clsx("shrink-0", color[s]);
  switch (s) {
    case "running":
      return (
        <span
          aria-hidden
          className={clsx(cls, "inline-flex items-center justify-center")}
          style={{ width: size, height: size }}
        >
          <span className="status-running-dot block size-2 rounded-full bg-current" />
        </span>
      );
    case "waiting":
      return <Hand aria-hidden size={size} strokeWidth={1.5} className={cls} />;
    case "blocked":
      return <Ban aria-hidden size={size} strokeWidth={1.5} className={cls} />;
    case "error":
      return <OctagonX aria-hidden size={size} strokeWidth={1.5} className={cls} />;
    case "done":
      return <Check aria-hidden size={size} strokeWidth={1.5} className={cls} />;
    case "stopped":
      return <Minus aria-hidden size={size} strokeWidth={1.5} className={cls} />;
    default:
      return <Circle aria-hidden size={size - 4} strokeWidth={1.5} className={cls} />;
  }
}

export function StatusMark({
  state,
  compact = false,
}: {
  state: AgentStateName | string;
  compact?: boolean;
}) {
  const l = useT();
  const s = statusOf(state);
  const word = l.t(`tui-state-${s}`);
  return (
    <span
      className={clsx("inline-flex items-center gap-1.5 whitespace-nowrap text-xs", color[s])}
      title={compact ? word : undefined}
    >
      <StatusIcon s={s} />
      {compact ? <span className="sr-only">{word}</span> : <span>{word}</span>}
    </span>
  );
}
