// The merge queue: what is about to land, and its state.
import { GitMerge } from "lucide-react";
import type { MergeEntry } from "../api/types";
import { StatusIcon, type Status } from "../components/status";
import { Empty, Rows } from "../components/ui";
import { VoiceName, hueOf } from "../components/voice";
import { useS } from "../ctx";
import { useT } from "../i18n";
import { Part } from "../store";
import { LoadError } from "./common";

function statusOfMerge(state: string): Status {
  switch (state) {
    case "merged":
      return "done";
    case "failed":
      return "error";
    case "changes_requested":
      return "blocked";
    case "awaiting_approval":
      return "waiting";
    case "queued":
      return "idle";
    default:
      return "running";
  }
}

function Row({ e }: { e: MergeEntry }) {
  const l = useT();
  const members = useS((s) => s.members);
  const cards = useS((s) => s.board.cards);
  const state = String(e.state ?? "queued");
  const card = cards.find((c) => c.id === e.card);
  return (
    <tr className="border-b border-line align-top last:border-0">
      <td className="py-2 pe-3">
        <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-xs">
          <StatusIcon s={statusOfMerge(state)} />
          {l.t(`merge-state-${state.replace(/_/g, "-")}`)}
        </span>
      </td>
      <td className="py-2 pe-3">
        <p className="text-sm text-ink">{card?.title ?? ""}</p>
        <p className="font-mono text-xs text-ink-2">
          {String(e.branch ?? "")} → {String(e.into ?? "")}
        </p>
        {typeof e.detail === "string" && e.detail && (
          <p className="mt-1 text-xs text-ink-2">{e.detail}</p>
        )}
      </td>
      <td className="py-2 text-end text-sm">
        <VoiceName hue={hueOf(members, e.member as string)} step={e.agent ? 0 : null}>
          {String(e.agent ?? members.find((m) => m.member === e.member)?.name ?? "")}
        </VoiceName>
      </td>
    </tr>
  );
}

export function MergesView() {
  const l = useT();
  const merges = useS((s) => s.merges);
  const load = useS((s) => s.load[Part.MERGES]);
  return (
    <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto px-4 py-3">
      {load === "loading" && merges.length === 0 ? (
        <Rows n={4} pitch={44} />
      ) : load === "error" && merges.length === 0 ? (
        <LoadError part={Part.MERGES} />
      ) : merges.length === 0 ? (
        <Empty title={l.t("app-empty-merges-title")}>{l.t("tui-empty-merges")}</Empty>
      ) : (
        <table className="w-full max-w-4xl border-collapse">
          <caption className="sr-only">{l.t("app-view-merges")}</caption>
          <thead>
            <tr className="text-start text-xs text-ink-2">
              <th className="h-8 pe-3 text-start font-normal">{l.t("app-merge-state")}</th>
              <th className="h-8 pe-3 text-start font-normal">
                <span className="inline-flex items-center gap-1">
                  <GitMerge size={12} strokeWidth={1.5} />
                  {l.t("app-merge-change")}
                </span>
              </th>
              <th className="h-8 text-end font-normal">{l.t("app-merge-by")}</th>
            </tr>
          </thead>
          <tbody>
            {merges.map((e, i) => (
              <Row key={String(e.id ?? i)} e={e} />
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
