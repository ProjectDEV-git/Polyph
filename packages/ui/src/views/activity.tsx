// The activity log, with replay: pick a moment and see the board as it was.
import clsx from "clsx";
import { History } from "lucide-react";
import { useState } from "react";
import type { ActivityEntry } from "../api/types";
import { Button, Empty, Rows } from "../components/ui";
import { VoiceName, agentSteps, hueOf } from "../components/voice";
import { useApp, useS } from "../ctx";
import { type Args, useT } from "../i18n";
import { Part } from "../store";
import { clock, dateTime } from "../time";
import { LoadError } from "./common";
import { columnName, localizeColumns } from "./names";

type Replay = { at: number; columns: Record<string, [string, string][]> };

function Who({ e }: { e: ActivityEntry }) {
  const members = useS((s) => s.members);
  const agents = useS((s) => s.agents);
  if (e.agent) {
    const a = agents.find((x) => x.label === e.agent);
    return (
      <VoiceName
        hue={hueOf(members, a?.member ?? e.actor)}
        step={a ? (agentSteps(agents).get(a.id) ?? 0) : 0}
      >
        {e.agent}
      </VoiceName>
    );
  }
  const m = members.find((x) => x.member === e.actor);
  return <VoiceName hue={m?.hue}>{m?.name ?? ""}</VoiceName>;
}

export function ActivityView() {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const entries = useS((s) => s.activity);
  const columns = useS((s) => s.board.columns);
  const load = useS((s) => s.load[Part.ACTIVITY]);
  const [replay, setReplay] = useState<Replay | null>(null);
  const [sel, setSel] = useState<number | null>(null);
  const list = [...entries].sort((a, b) => a.at - b.at);
  const show = async (i: number) => {
    const e = list[i];
    if (!e) return;
    setSel(i);
    const r = await app.act<Replay>("activity.replay", { room, at: e.at });
    if (r) setReplay(r);
  };
  if (load === "error" && entries.length === 0)
    return (
      <div className="p-4">
        <LoadError part={Part.ACTIVITY} />
      </div>
    );
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div className="scroll-quiet min-w-0 flex-1 overflow-y-auto px-4 py-3">
        {load === "loading" && entries.length === 0 ? (
          <Rows n={8} />
        ) : list.length === 0 ? (
          <Empty>{l.t("tui-empty-activity")}</Empty>
        ) : (
          <ol className="flex flex-col">
            {list.map((e, i) => (
              <li key={e.id}>
                <button
                  type="button"
                  onClick={() => void show(i)}
                  aria-pressed={sel === i}
                  className={clsx(
                    "grid min-h-7 w-full grid-cols-[64px_1fr] items-baseline gap-2 rounded-control px-2 py-1 text-start text-sm",
                    sel === i ? "bg-s3" : "hover:bg-s2",
                  )}
                >
                  <time
                    className="whitespace-nowrap num text-xs text-ink-2"
                    title={dateTime(l, e.at)}
                  >
                    {clock(l, e.at)}
                  </time>
                  <span className="min-w-0">
                    <Who e={e} />{" "}
                    <span className="text-ink">
                      {l.t(e.summary_key, localizeColumns(l, e.args) as Args)}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ol>
        )}
      </div>
      <aside
        aria-labelledby="replay-h"
        className="hidden w-80 shrink-0 flex-col gap-3 border-s border-line px-4 py-3 lg:flex"
      >
        <div className="flex h-8 items-center justify-between">
          <h2 id="replay-h" className="flex items-center gap-2 text-sm font-medium text-ink-2">
            <History size={14} strokeWidth={1.5} />
            {replay ? l.t("tui-replay-at", { time: dateTime(l, replay.at) }) : l.t("app-replay")}
          </h2>
          {replay && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setReplay(null);
                setSel(null);
              }}
            >
              {l.t("app-replay-live")}
            </Button>
          )}
        </div>
        {replay ? (
          <>
            <input
              type="range"
              min={0}
              max={Math.max(0, list.length - 1)}
              value={sel ?? list.length - 1}
              aria-label={l.t("app-replay")}
              onChange={(e) => void show(Number(e.target.value))}
              className="w-full accent-[var(--accent)]"
            />
            <dl className="flex flex-col gap-2">
              {(columns.length ? columns.map((c) => c.id) : Object.keys(replay.columns)).map(
                (id) => (
                  <div key={id}>
                    <dt className="flex justify-between text-xs text-ink-2">
                      <span>{columnName(l, id)}</span>
                      <span className="num">{replay.columns[id]?.length ?? 0}</span>
                    </dt>
                    {(replay.columns[id] ?? []).map(([cid, title]) => (
                      <dd key={cid} className="truncate text-sm text-ink">
                        {title}
                      </dd>
                    ))}
                  </div>
                ),
              )}
            </dl>
          </>
        ) : (
          <p className="text-sm text-ink-2">{l.t("app-replay-hint")}</p>
        )}
      </aside>
    </div>
  );
}
