// The board: six columns, cards moved by drag or by keyboard.
import clsx from "clsx";
import { GitBranch, GitPullRequest, Plus, Sparkles } from "lucide-react";
import { LayoutGroup, motion, useReducedMotion } from "motion/react";
import { useState } from "react";
import type { Card } from "../api/types";
import { Button, Input, Label, Rows, Select } from "../components/ui";
import { VoiceName, agentSteps, hueOf } from "../components/voice";
import { useApp, usePlatform, useS } from "../ctx";
import { useT } from "../i18n";
import { Part } from "../store";
import { LoadError } from "./common";
import { columnName } from "./names";
import { useWorkdir } from "./workdir";

function Assignee({ id }: { id: string | null }) {
  const l = useT();
  const members = useS((s) => s.members);
  const agents = useS((s) => s.agents);
  if (!id) return <span className="text-xs text-ink-2">{l.t("app-unassigned")}</span>;
  const m = members.find((x) => x.member === id);
  if (m)
    return (
      <VoiceName hue={m.hue} className="text-xs">
        {m.name}
      </VoiceName>
    );
  const a = agents.find((x) => x.id === id || x.label === id);
  if (a)
    return (
      <VoiceName
        hue={hueOf(members, a.member)}
        step={agentSteps(agents).get(a.id) ?? 0}
        className="text-xs"
      >
        {a.label}
      </VoiceName>
    );
  return <span className="font-mono text-xs text-ink-2">{id.slice(0, 8)}</span>;
}

function prNumber(pr: Card["pr"]): number | null {
  if (typeof pr === "number") return pr;
  if (pr && typeof pr === "object" && typeof pr.number === "number") return pr.number;
  return null;
}

function CardItem({ c }: { c: Card }) {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const columns = useS((s) => s.board.columns);
  const reduce = useReducedMotion();
  const pr = prNumber(c.pr);
  return (
    <motion.li
      layout={!reduce}
      layoutId={reduce ? undefined : c.id}
      transition={{ duration: 0.18, ease: [0.2, 0, 0, 1] }}
      draggable
      onDragStart={(e) => {
        (e as unknown as DragEvent).dataTransfer?.setData("text/x-polyph-card", c.id);
      }}
      className={clsx(
        "group cursor-grab rounded-card bg-s2 px-3 py-2 active:cursor-grabbing",
        c.draft && "border border-dashed border-line bg-transparent",
      )}
      data-testid="card"
    >
      <p className="text-sm text-ink">{c.title}</p>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1">
        <Assignee id={c.assignee ?? c.suggested_assignee} />
        {c.draft && (
          <span className="rounded-full border border-line px-1.5 text-xs text-ink-2">
            {l.t("app-card-draft")}
          </span>
        )}
        {c.branch && (
          <span className="inline-flex min-w-0 items-center gap-1 text-xs text-ink-2">
            <GitBranch size={12} strokeWidth={1.5} />
            <span className="truncate font-mono">{c.branch}</span>
          </span>
        )}
        {pr !== null && (
          <span className="num inline-flex items-center gap-1 text-xs text-ink-2">
            <GitPullRequest size={12} strokeWidth={1.5} />
            {l.t("app-card-pr", { n: pr })}
          </span>
        )}
      </div>
      <label className="sr-only" htmlFor={`move-${c.id}`}>
        {l.t("app-move-card", { title: c.title })}
      </label>
      <Select
        id={`move-${c.id}`}
        value={c.column}
        className="mt-2 hidden h-7 text-xs group-focus-within:block group-hover:block"
        onChange={async (e) => {
          await app.act("board.move", { room, card: c.id, column: e.target.value });
          app.queue(Part.BOARD);
        }}
      >
        {columns.map((col) => (
          <option key={col.id} value={col.id}>
            {l.t("app-move-to", { column: columnName(l, col.id, col.title) })}
          </option>
        ))}
      </Select>
    </motion.li>
  );
}

function Column({ id, title, cards }: { id: string; title: string; cards: Card[] }) {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const [over, setOver] = useState(false);
  const [adding, setAdding] = useState(false);
  const [text, setText] = useState("");
  return (
    <section
      aria-labelledby={`col-${id}`}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("text/x-polyph-card")) {
          e.preventDefault();
          setOver(true);
        }
      }}
      onDragLeave={() => setOver(false)}
      onDrop={async (e) => {
        setOver(false);
        const card = e.dataTransfer.getData("text/x-polyph-card");
        if (!card) return;
        e.preventDefault();
        await app.act("board.move", { room, card, column: id });
        app.queue(Part.BOARD);
      }}
      className={clsx(
        "flex min-w-56 flex-1 basis-0 flex-col rounded-card bg-s1 p-2",
        over && "outline outline-1 outline-accent",
      )}
    >
      <header className="flex h-8 items-center gap-2 px-1">
        <h2 id={`col-${id}`} className="flex-1 text-sm font-medium text-ink-2">
          {columnName(l, id, title)}
        </h2>
        <span className="num text-xs text-ink-2">{cards.length}</span>
        {id === "backlog" && (
          <button
            type="button"
            aria-label={l.t("app-add-card")}
            title={l.t("app-add-card")}
            onClick={() => setAdding((v) => !v)}
            className="inline-flex size-7 items-center justify-center rounded-control text-ink-2 hover:bg-s3 hover:text-ink"
          >
            <Plus size={16} strokeWidth={1.5} />
          </button>
        )}
      </header>
      {adding && (
        <form
          className="mb-2"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!text.trim()) return;
            await app.act(
              "board.add",
              { room, title: text.trim(), column: id },
              l.t("tui-card-added"),
            );
            setText("");
            setAdding(false);
            app.queue(Part.BOARD);
          }}
        >
          <Input
            autoFocus
            value={text}
            aria-label={l.t("app-card-title")}
            placeholder={l.t("app-card-title")}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && setAdding(false)}
          />
        </form>
      )}
      <ol className="scroll-quiet flex min-h-16 flex-col gap-1.5 overflow-y-auto">
        {cards.map((c) => (
          <CardItem key={c.id} c={c} />
        ))}
        {cards.length === 0 && (
          <li className="px-1 py-2 text-xs text-ink-2">{l.t("app-empty-column")}</li>
        )}
      </ol>
    </section>
  );
}

function PlanBar() {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const cards = useS((s) => s.board.cards);
  const [workdir] = useWorkdir();
  const [open, setOpen] = useState(false);
  const [goal, setGoal] = useState("");
  const [busy, setBusy] = useState(false);
  const plans = [...new Set(cards.filter((c) => c.draft && c.plan).map((c) => String(c.plan)))];
  // Planning runs an agent, which needs a computer running Polyph.
  if (usePlatform().kind === "phone") return null;
  return (
    <div className="flex flex-wrap items-end gap-2">
      {plans.map((p) => (
        <Button
          key={p}
          variant="primary"
          onClick={async () => {
            await app.act("plan.accept", { room, plan: p }, l.t("app-plan-accepted"));
            app.queue(Part.BOARD);
          }}
        >
          {l.t("app-plan-accept", { n: cards.filter((c) => c.plan === p && c.draft).length })}
        </Button>
      ))}
      {open ? (
        <form
          className="flex min-w-80 flex-1 items-end gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!goal.trim() || !workdir) return;
            setBusy(true);
            await app.act(
              "plan.create",
              { room, goal: goal.trim(), cwd: workdir },
              l.t("tui-planned"),
            );
            setBusy(false);
            setGoal("");
            setOpen(false);
            app.queue(Part.BOARD);
          }}
        >
          <div className="flex-1">
            <Label htmlFor="plan-goal">{l.t("app-plan-goal")}</Label>
            <Input
              id="plan-goal"
              autoFocus
              value={goal}
              placeholder={l.t("app-plan-placeholder")}
              onChange={(e) => setGoal(e.target.value)}
            />
            {!workdir && <p className="mt-1 text-xs text-blocked">{l.t("app-workdir-needed")}</p>}
          </div>
          <Button type="submit" variant="primary" disabled={busy || !workdir}>
            {l.t("app-plan")}
          </Button>
          <Button variant="ghost" onClick={() => setOpen(false)}>
            {l.t("app-cancel")}
          </Button>
        </form>
      ) : (
        <Button onClick={() => setOpen(true)}>
          <Sparkles size={14} strokeWidth={1.5} />
          {l.t("app-plan-goal")}
        </Button>
      )}
    </div>
  );
}

export function BoardView() {
  const board = useS((s) => s.board);
  const load = useS((s) => s.load[Part.BOARD]);
  if (load === "error" && board.columns.length === 0)
    return (
      <div className="p-4">
        <LoadError part={Part.BOARD} />
      </div>
    );
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 px-4 py-3">
      <PlanBar />
      {board.columns.length === 0 ? (
        <Rows n={4} />
      ) : (
        <LayoutGroup>
          <div className="scroll-quiet flex min-h-0 flex-1 gap-2 overflow-x-auto pb-2">
            {board.columns.map((col) => (
              <Column
                key={col.id}
                id={col.id}
                title={col.title}
                cards={board.cards
                  .filter((c) => c.column === col.id)
                  .sort((a, b) => a.order - b.order)}
              />
            ))}
          </div>
        </LayoutGroup>
      )}
    </div>
  );
}
