// The status pill: one line for a corner of the screen. Same status feed as
// `polyph status --watch`; the first approval can be answered inline.
import { useState } from "react";
import { Mark } from "../components/mark";
import { StatusIcon, statusOf } from "../components/status";
import { useApp, usePlatform, useS } from "../ctx";
import { useT } from "../i18n";
import { ApprovalCard } from "./approvals";

export function Pill({ onOpenApp }: { onOpenApp?: () => void }) {
  const l = useT();
  const app = useApp();
  const platform = usePlatform();
  const status = useS((s) => s.status);
  const conn = useS((s) => s.conn);
  useS((s) => s.approvals);
  const mine = app.myApprovals();
  const [open, setOpen] = useState(false);
  const running = status?.agents.filter((a) => a.state !== "stopped") ?? [];
  return (
    <div className="flex flex-col gap-2 p-1" data-testid="pill">
      <div
        className="flex h-11 items-center gap-2 rounded-full bg-s2 px-3 shadow-float"
        data-tauri-drag-region
      >
        <button
          type="button"
          onClick={onOpenApp}
          aria-label={l.t("app-pill-open")}
          className="text-accent"
        >
          <Mark size={18} />
        </button>
        <span className="max-w-32 truncate font-serif text-base text-ink" data-tauri-drag-region>
          {conn !== "open" ? l.t("tui-offline") : (status?.room_name ?? l.t("tui-no-room"))}
        </span>
        <span
          className="flex items-center gap-1"
          aria-label={l.t("app-pill-agents", { n: running.length })}
          role="img"
        >
          {running.slice(0, 6).map((a) => (
            <span
              key={a.id}
              title={`${a.label} · ${l.t(`tui-state-${statusOf(a.state)}`)}`}
              style={{ color: a.color || undefined }}
              className="inline-flex items-center"
            >
              <StatusIcon s={statusOf(a.state)} size={12} />
            </span>
          ))}
        </span>
        <span className="flex-1" />
        {mine.length > 0 ? (
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            className="inline-flex h-7 items-center gap-1.5 rounded-full px-2 text-sm text-waiting hover:bg-s3"
          >
            <StatusIcon s="waiting" size={14} />
            <span className="num">{l.t("tui-approvals", { n: mine.length })}</span>
          </button>
        ) : (
          <span className="text-xs text-ink-2">{l.t("app-nothing-needs-you")}</span>
        )}
      </div>
      {open && mine[0] && <ApprovalCard a={mine[0]} compact={platform.kind === "desktop"} />}
    </div>
  );
}
