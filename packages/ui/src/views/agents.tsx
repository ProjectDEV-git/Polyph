// Agents: who is running, and one agent's live transcript (follow mode).
import clsx from "clsx";
import { Play, Square } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { AgentEvent } from "../api/types";
import { StatusIcon, StatusMark, statusOf } from "../components/status";
import { Button, Empty, Input, Label, Rows, Select, Textarea } from "../components/ui";
import { VoiceName, agentSteps, hueOf, useVoice } from "../components/voice";
import { useApp, usePlatform, useS } from "../ctx";
import { useT } from "../i18n";
import { Part } from "../store";
import { clock } from "../time";
import { Composer } from "./chat";
import { useWorkdir } from "./workdir";

function DiffBlock({ text }: { text: string }) {
  return (
    <pre className="scroll-quiet mt-1 max-h-72 overflow-auto rounded-control bg-bg px-2 py-1.5 font-mono text-xs leading-5">
      {text.split("\n").map((line, i) => (
        <span
          key={i}
          className={clsx(
            "block",
            line.startsWith("+") && !line.startsWith("+++") && "text-running",
            line.startsWith("-") && !line.startsWith("---") && "text-error",
            line.startsWith("@@") && "text-ink-2",
          )}
        >
          {line || " "}
        </span>
      ))}
    </pre>
  );
}

function Line({ e, latest }: { e: AgentEvent; latest: Map<string, AgentEvent> }) {
  const l = useT();
  const members = useS((s) => s.members);
  const c = (e.kind.c ?? {}) as Record<string, any>;
  switch (e.kind.t) {
    case "started":
      return (
        <p className="text-xs text-ink-2">
          {c.model ? l.t("app-tr-started-model", { model: c.model }) : l.t("app-tr-started")}
        </p>
      );
    case "state":
      return statusOf(c.state) === "idle" ? null : <StatusMark state={c.state} />;
    case "prompt": {
      const m = members.find((x) => x.member === c.from);
      return (
        <div className="rounded-control bg-s2 px-2.5 py-1.5">
          <p className="text-xs">
            <VoiceName hue={m?.hue}>{m?.name ?? ""}</VoiceName>
          </p>
          <p className="whitespace-pre-wrap text-sm text-ink">{c.text}</p>
        </div>
      );
    }
    case "message":
      return <p className="whitespace-pre-wrap break-words text-sm text-ink">{c.text}</p>;
    case "thought":
      return <p className="whitespace-pre-wrap text-sm text-ink-2">{c.text}</p>;
    case "tool": {
      const t = (latest.get(c.id)?.kind.c ?? c) as Record<string, any>;
      const s = t.status === "failed" ? "error" : t.status === "completed" ? "done" : "running";
      const body = t.diff || t.output;
      return (
        <details className="group">
          <summary className="flex cursor-pointer list-none items-center gap-2 text-sm text-ink-2 hover:text-ink">
            <StatusIcon s={s} />
            <span className="min-w-0 truncate">{t.title}</span>
          </summary>
          {body ? <DiffBlock text={String(body)} /> : null}
        </details>
      );
    }
    case "diff":
      return (
        <details>
          <summary className="cursor-pointer list-none font-mono text-xs text-ink-2">
            {c.path}
          </summary>
          <DiffBlock text={String(c.unified ?? "")} />
        </details>
      );
    case "cursor":
      return null;
    case "usage":
      return (
        <p className="num text-xs text-ink-2">
          {l.t("app-tr-usage", { input: c.input_tokens ?? 0, output: c.output_tokens ?? 0 })}
        </p>
      );
    case "stopped":
      return <p className="text-xs text-ink-2">{l.t("tui-agent-stopped")}</p>;
    default:
      return null;
  }
}

function Transcript() {
  const l = useT();
  const app = useApp();
  const id = useS((s) => s.agent);
  const agent = useS((s) => s.agents.find((a) => a.id === s.agent));
  const members = useS((s) => s.members);
  const agents = useS((s) => s.agents);
  const events = useS((s) => s.transcript);
  const load = useS((s) => s.load[Part.TRANSCRIPT]);
  const room = useS((s) => s.room);
  const color = useVoice(hueOf(members, agent?.member), agentSteps(agents).get(id ?? "") ?? 0);
  const ref = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  useLayoutEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [events.length]);
  useEffect(() => {
    stick.current = true;
  }, [id]);
  if (!id || !agent)
    return (
      <div className="flex-1 p-4">
        <p className="text-sm text-ink-2">{l.t("app-select-agent")}</p>
      </div>
    );
  // Tool calls arrive as several updates with one id; show each once, at its latest.
  const latest = new Map<string, AgentEvent>();
  for (const e of events) if (e.kind.t === "tool") latest.set(String((e.kind.c as any).id), e);
  const seen = new Set<string>();
  const shown = events.filter((e) => {
    if (e.kind.t !== "tool") return true;
    const tid = String((e.kind.c as any).id);
    if (seen.has(tid)) return false;
    seen.add(tid);
    return true;
  });
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex h-12 items-center gap-3 border-b border-line px-4">
        <VoiceName
          hue={hueOf(members, agent.member)}
          step={agentSteps(agents).get(agent.id) ?? 0}
          className="truncate text-sm"
        >
          {agent.label}
        </VoiceName>
        <StatusMark state={agent.state} />
        <span className="flex-1" />
        {agent.mine && agent.state !== "stopped" && (
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              await app.act("agent.stop", { agent: agent.id }, l.t("tui-agent-stopping"));
              app.queue(Part.AGENTS);
            }}
          >
            <Square size={12} strokeWidth={1.5} />
            {l.t("app-stop")}
          </Button>
        )}
      </header>
      <div
        ref={ref}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
        className="scroll-quiet min-h-0 flex-1 overflow-y-auto px-4 py-3"
      >
        {load === "loading" && events.length === 0 ? (
          <Rows n={6} />
        ) : (
          <ol
            aria-live="polite"
            className="voice-gutter flex flex-col gap-2"
            style={{ ["--voice" as string]: color }}
          >
            {shown.map((e) => (
              <li key={e.seq} className="grid grid-cols-[64px_1fr] gap-2">
                <time className="whitespace-nowrap num pt-0.5 text-xs text-ink-2">
                  {clock(l, e.at)}
                </time>
                <div className="min-w-0">
                  <Line e={e} latest={latest} />
                </div>
              </li>
            ))}
          </ol>
        )}
      </div>
      {agent.state !== "stopped" && (
        <Composer
          label={
            agent.mine
              ? l.t("tui-prompt-agent", { agent: agent.label })
              : l.t("app-send-inbox", { agent: agent.label })
          }
          placeholder={
            agent.mine
              ? l.t("tui-prompt-agent", { agent: agent.label })
              : l.t("app-send-inbox", { agent: agent.label })
          }
          onSend={async (text) => {
            if (agent.mine) await app.act("agent.prompt", { agent: agent.id, text });
            else
              await app.act("inbox.send", { room, agent: agent.id, text }, l.t("tui-sent-inbox"));
          }}
        />
      )}
    </div>
  );
}

function StartAgent({ onDone }: { onDone: () => void }) {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const cards = useS((s) => s.board.cards).filter((c) => c.column !== "done");
  const [workdir, setWorkdir] = useWorkdir();
  const [kinds, setKinds] = useState<string[]>(["claude", "codex", "gemini", "opencode", "local"]);
  const [kind, setKind] = useState("claude");
  const [task, setTask] = useState("");
  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!workdir) return;
    app.api
      .call<{ agents: { name: string }[] }>("agents.doctor", { cwd: workdir })
      .then((d) => setKinds(d.agents.map((a) => a.name)))
      .catch(() => {});
  }, [workdir, app]);
  return (
    <form
      className="flex max-w-xl flex-col gap-4 p-4"
      onSubmit={async (e) => {
        e.preventDefault();
        setBusy(true);
        const r = await app.act<{ id: string }>(
          "agent.start",
          { room, kind, cwd: workdir, task: task || null, prompt: prompt.trim() || null },
          l.t("tui-agent-started"),
        );
        setBusy(false);
        if (r?.id) {
          await app.refresh(Part.AGENTS);
          app.selectAgent(r.id);
          onDone();
        }
      }}
    >
      <h2 className="text-lg font-medium text-ink">{l.t("app-start-agent")}</h2>
      <div>
        <Label htmlFor="ag-kind">{l.t("app-agent-kind")}</Label>
        <Select id="ag-kind" value={kind} onChange={(e) => setKind(e.target.value)}>
          {kinds.map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </Select>
      </div>
      <div>
        <Label htmlFor="ag-dir">{l.t("app-workdir")}</Label>
        <Input
          id="ag-dir"
          className="font-mono"
          value={workdir}
          onChange={(e) => setWorkdir(e.target.value)}
          placeholder={l.t("app-workdir-placeholder")}
        />
      </div>
      <div>
        <Label htmlFor="ag-task">{l.t("app-agent-task")}</Label>
        <Select id="ag-task" value={task} onChange={(e) => setTask(e.target.value)}>
          <option value="">{l.t("app-agent-no-task")}</option>
          {cards.map((c) => (
            <option key={c.id} value={c.id}>
              {c.title}
            </option>
          ))}
        </Select>
      </div>
      <div>
        <Label htmlFor="ag-prompt">{l.t("app-agent-prompt")}</Label>
        <Textarea
          id="ag-prompt"
          rows={3}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
        />
      </div>
      <div className="flex gap-2">
        <Button type="submit" variant="primary" disabled={busy || !workdir}>
          <Play size={14} strokeWidth={1.5} />
          {l.t("app-start")}
        </Button>
        <Button variant="ghost" onClick={onDone}>
          {l.t("app-cancel")}
        </Button>
      </div>
    </form>
  );
}

export function AgentsView() {
  const l = useT();
  const app = useApp();
  const agents = useS((s) => s.agents);
  const members = useS((s) => s.members);
  const selected = useS((s) => s.agent);
  const load = useS((s) => s.load[Part.AGENTS]);
  const [starting, setStarting] = useState(false);
  const phone = usePlatform().kind === "phone";
  const steps = agentSteps(agents);
  const list = [...agents].sort(
    (a, b) =>
      Number(a.state === "stopped") - Number(b.state === "stopped") ||
      a.label.localeCompare(b.label),
  );
  const first = list.find((a) => a.state !== "stopped")?.id;
  useEffect(() => {
    // Follow mode by default: open the first live agent.
    if (!selected && first) app.selectAgent(first);
  }, [selected, first, app]);
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <aside
        aria-label={l.t("app-view-agents")}
        className="flex w-72 shrink-0 flex-col border-e border-line"
      >
        {!phone && (
          <div className="flex h-12 items-center px-3">
            <Button variant="secondary" onClick={() => setStarting(true)}>
              <Play size={14} strokeWidth={1.5} />
              {l.t("app-start-agent")}
            </Button>
          </div>
        )}
        <ul className="scroll-quiet min-h-0 flex-1 overflow-y-auto px-2 pb-2">
          {load === "loading" && agents.length === 0 && <Rows n={3} />}
          {list.map((a) => (
            <li key={a.id}>
              <button
                type="button"
                aria-current={selected === a.id ? "true" : undefined}
                onClick={() => {
                  setStarting(false);
                  app.selectAgent(a.id);
                }}
                className={clsx(
                  "flex h-11 w-full flex-col justify-center rounded-control px-2 text-start",
                  selected === a.id ? "bg-s3" : "hover:bg-s2",
                )}
              >
                <VoiceName
                  hue={hueOf(members, a.member)}
                  step={steps.get(a.id) ?? 0}
                  className="truncate text-sm"
                >
                  {a.label}
                </VoiceName>
                <StatusMark state={a.state} />
              </button>
            </li>
          ))}
        </ul>
        {agents.length === 0 && load !== "loading" && (
          <div className="p-3">
            <Empty>{l.t("app-empty-agents")}</Empty>
          </div>
        )}
      </aside>
      {starting ? <StartAgent onDone={() => setStarting(false)} /> : <Transcript />}
    </div>
  );
}
