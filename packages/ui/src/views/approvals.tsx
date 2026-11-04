// The approval stack: each card says exactly what will happen (PRODUCT.md
// principle 2) and offers Approve once, Approve for this task, Deny with note.
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useState } from "react";
import type { Approval, Knock } from "../api/types";
import { Button, Textarea } from "../components/ui";
import { VoiceName, hueOf } from "../components/voice";
import { StatusIcon } from "../components/status";
import { useApp, useS } from "../ctx";
import { type Localizer, useT } from "../i18n";
import { ago } from "../time";

function shortPath(p: string): string {
  const parts = p.split("/").filter(Boolean);
  return parts.length > 3 ? `…/${parts.slice(-3).join("/")}` : p;
}

/** The sentence for an approval, from the same strings as the TUI. */
export function describeApproval(l: Localizer, a: Approval): string {
  const c = (a.kind.c ?? {}) as Record<string, any>;
  const agent = a.agent.label;
  switch (a.kind.t) {
    case "command":
      return l.t("tui-approve-command", {
        agent,
        command: c.command,
        cwd: shortPath(String(c.cwd ?? "")),
      });
    case "host_command":
      return l.t("tui-approve-command", { agent, command: c.command, cwd: "" });
    case "edit":
      return l.t("tui-approve-edit", { agent, files: (c.files ?? []).join(", ") });
    case "network":
      return l.t("tui-approve-network", { agent, host: c.host, port: c.port });
    case "outside_worktree":
      return l.t("tui-approve-outside", { agent, path: c.path });
    case "push":
      return l.t("tui-approve-push", { agent, remote: c.remote, branch: c.branch });
    case "merge":
      return l.t("tui-approve-merge", { agent, branch: c.branch, into: c.into });
    case "terminal_input":
      return l.t("tui-approve-terminal", { agent });
    case "steer":
      return l.t("tui-approve-steer", { agent, text: c.text });
    default:
      return l.t("tui-approve-other", { agent, title: c.title ?? "", detail: c.detail ?? "" });
  }
}

/** The exact thing approved, shown in mono under the sentence. */
function detail(a: Approval): string | null {
  const c = (a.kind.c ?? {}) as Record<string, any>;
  if (a.kind.t === "command" || a.kind.t === "host_command") return String(c.command ?? "");
  if (a.kind.t === "edit") return (c.files ?? []).join("\n");
  if (a.kind.t === "network") return `${c.host}:${c.port}`;
  if (a.kind.t === "outside_worktree") return String(c.path ?? "");
  return null;
}

export function ApprovalCard({ a, compact = false }: { a: Approval; compact?: boolean }) {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const members = useS((s) => s.members);
  const [denying, setDenying] = useState(false);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const d = detail(a);
  const answer = async (
    method: "approve" | "deny",
    extra: Record<string, unknown>,
    done: string,
  ) => {
    setBusy(true);
    await app.act(method, { room, id: a.id, ...extra }, l.t(done));
    setBusy(false);
    app.queue(32);
  };
  return (
    <article
      aria-label={describeApproval(l, a)}
      className="rounded-card bg-s2 p-3 shadow-float"
      data-testid="approval"
    >
      <div className="flex items-start gap-2">
        <span className="mt-0.5">
          <StatusIcon s="waiting" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-sm text-ink">
            <VoiceName hue={hueOf(members, a.agent.member)} step={0}>
              {a.agent.label}
            </VoiceName>
          </p>
          <p className="mt-0.5 text-sm text-ink-2">{describeApproval(l, a)}</p>
          {d && !compact && (
            <pre className="scroll-quiet mt-2 max-h-28 overflow-auto rounded-control bg-bg px-2 py-1.5 font-mono text-xs text-ink">
              {d}
            </pre>
          )}
          <p className="mt-1.5 text-xs text-ink-2">{ago(l, a.created_at)}</p>
        </div>
      </div>
      {denying ? (
        <form
          className="mt-3 flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void answer("deny", { note: note.trim() || null }, "tui-denied");
          }}
        >
          <Textarea
            autoFocus
            rows={2}
            value={note}
            aria-label={l.t("tui-prompt-deny")}
            placeholder={l.t("tui-prompt-deny")}
            onChange={(e) => setNote(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && setDenying(false)}
          />
          <div className="flex gap-2">
            <Button type="submit" variant="danger" size="sm" disabled={busy}>
              {l.t("app-deny-send")}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setDenying(false)}>
              {l.t("app-cancel")}
            </Button>
          </div>
        </form>
      ) : (
        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            variant="primary"
            size="sm"
            disabled={busy}
            onClick={() => void answer("approve", { task: false }, "tui-approved")}
          >
            {l.t("app-approve-once")}
          </Button>
          {a.task && !compact && (
            <Button
              size="sm"
              disabled={busy}
              onClick={() => void answer("approve", { task: true }, "tui-approved")}
            >
              {l.t("app-approve-task")}
            </Button>
          )}
          <Button variant="danger" size="sm" disabled={busy} onClick={() => setDenying(true)}>
            {l.t("app-deny")}
          </Button>
        </div>
      )}
    </article>
  );
}

function KnockCard({ k }: { k: Knock }) {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const decide = async (approve: boolean) => {
    await app.act(
      "knock.decide",
      { room, knock: k.id, approve },
      l.t(approve ? "app-knock-let-in" : "tui-denied"),
    );
    app.queue(32 | 2);
  };
  return (
    <article className="rounded-card bg-s2 p-3 shadow-float" data-testid="knock">
      <p className="text-sm text-ink">{l.t("app-knock", { name: k.display_name })}</p>
      <p className="mt-1.5 text-xs text-ink-2">{ago(l, k.at)}</p>
      <div className="mt-3 flex gap-2">
        <Button variant="primary" size="sm" onClick={() => void decide(true)}>
          {l.t("app-knock-approve")}
        </Button>
        <Button variant="danger" size="sm" onClick={() => void decide(false)}>
          {l.t("app-deny")}
        </Button>
      </div>
    </article>
  );
}

export function ApprovalStack() {
  const l = useT();
  const app = useApp();
  useS((s) => s.approvals);
  useS((s) => s.me);
  const mine = app.myApprovals();
  const knocks = useS((s) => s.knocks);
  const reduce = useReducedMotion();
  return (
    <section aria-labelledby="needs-you" className="flex flex-col gap-2">
      <h2 id="needs-you" className="flex h-8 items-center gap-2 text-sm font-medium text-ink-2">
        {l.t("app-needs-you")}
        {mine.length + knocks.length > 0 && (
          <span className="num text-waiting">{mine.length + knocks.length}</span>
        )}
      </h2>
      {mine.length + knocks.length === 0 && (
        <p className="text-sm text-ink-2">{l.t("app-nothing-needs-you")}</p>
      )}
      <div aria-live="polite" className="flex flex-col gap-2">
        {knocks.map((k) => (
          <KnockCard key={k.id} k={k} />
        ))}
        <AnimatePresence initial={false}>
          {mine.map((a) => (
            <motion.div
              key={a.id}
              layout={!reduce}
              initial={reduce ? { opacity: 0 } : { opacity: 0, y: -6 }}
              animate={{
                opacity: 1,
                y: 0,
                transition: { duration: reduce ? 0.12 : 0.18, ease: [0.2, 0, 0, 1] },
              }}
              exit={{ opacity: 0, transition: { duration: 0.1, ease: [0.4, 0, 1, 1] } }}
            >
              <ApprovalCard a={a} />
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </section>
  );
}
