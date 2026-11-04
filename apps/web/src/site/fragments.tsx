// Pieces of Polyph's own interface, built in HTML at real size for the
// website. Each root carries data-ui. The room, names and files are a
// sample; the voice colors are computed the way the app computes them.
import type { Localizer } from "@polyph/ui/core";
import { SAMPLE_CODE } from "./i18n";
import { StatusIcon } from "./icons";
import { type Voice, voiceStyle } from "./voices";

type L = { l: Localizer };

export function Chip({
  kind,
  children,
}: {
  kind: "running" | "waiting" | "blocked" | "done";
  children: string;
}) {
  return (
    <span className={`chip ${kind}`}>
      <StatusIcon kind={kind} />
      {children}
    </span>
  );
}

export function Line({
  who,
  hue,
  step,
  children,
}: {
  who: string;
  hue: number;
  step: number | null;
  children: React.ReactNode;
}) {
  return (
    <div className="ui-line v" style={voiceStyle(hue, step)}>
      <span className="gutter" aria-hidden />
      <div>
        <div className="who">{who}</div>
        <div>{children}</div>
      </div>
    </div>
  );
}

/** A room's transcript: people and agents speaking side by side. */
export function Transcript({ l, team }: L & { team: Voice[] }) {
  const [a, b, c] = team;
  if (!a || !b || !c) return null;
  const agent = (v: Voice) => l.t("site-ui-agent-of", { name: v.name });
  return (
    <div data-ui className="ui-lines">
      <Line who={agent(a)} hue={a.hue} step={0}>
        {l.t("site-ui-line-1")}
      </Line>
      <Line who={agent(b)} hue={b.hue} step={0}>
        {l.t("site-ui-line-2", { name: a.name })}
      </Line>
      <Line who={c.name} hue={c.hue} step={null}>
        {l.t("site-ui-line-4")}
      </Line>
      <Line who={agent(a)} hue={a.hue} step={0}>
        {l.t("site-ui-line-3")}
      </Line>
      <Line who={agent(c)} hue={c.hue} step={0}>
        {l.t("site-ui-line-5")}
      </Line>
    </div>
  );
}

/** The always-on-top status pill. */
export function Pill({ l, team }: L & { team: Voice[] }) {
  return (
    <div data-ui className="pill-ui">
      <div className="row">
        {team.slice(0, 3).map((v) => (
          <span key={v.name} className="voice-dot v" style={voiceStyle(v.hue)} aria-hidden />
        ))}
        <b>{l.t("site-ui-room")}</b>
      </div>
      <Chip kind="running">{l.t("site-ui-pill-running", { count: 2 })}</Chip>
      <Chip kind="waiting">{l.t("site-ui-pill-waiting", { count: 1 })}</Chip>
    </div>
  );
}

/** An approval that states exactly what will happen. */
export function Approval({ l, who, floating }: L & { who: Voice; floating?: boolean }) {
  return (
    <div data-ui className={`ui-card${floating ? " floating" : ""}`}>
      <b className="v" style={{ ...voiceStyle(who.hue, 0), color: "var(--voice)" }}>
        {l.t("site-ui-approve-title", { name: who.name })}
      </b>
      <dl className="ui-kv">
        <dt>{l.t("site-ui-approve-cmd")}</dt>
        <dd>
          <code>{"cargo test -p payments" /* i18n-allow (command) */}</code>
        </dd>
        <dt>{l.t("site-ui-approve-files")}</dt>
        <dd>
          <code>{"src/pay/refund.ts" /* i18n-allow (path) */}</code>
        </dd>
        <dt>{l.t("site-ui-approve-hosts")}</dt>
        <dd>{l.t("site-ui-approve-none")}</dd>
      </dl>
      <div className="ui-btns">
        <span className="ui-btn primary">{l.t("site-ui-approve-once")}</span>
        <span className="ui-btn">{l.t("site-ui-approve-task")}</span>
        <span className="ui-btn">{l.t("site-ui-deny")}</span>
      </div>
    </div>
  );
}

/** `polyph host` in a terminal. */
export function HostTerminal() {
  return (
    <pre data-ui className="term">
      <span className="prompt">$ </span>polyph host --name payments{"\n"}
      <span className="out">{SAMPLE_CODE}</span>
    </pre>
  );
}

/** Someone knocking to join. */
export function Knock({ l, who }: L & { who: Voice }) {
  return (
    <div data-ui className="ui-card">
      <span className="roster">
        <span className="v" style={voiceStyle(who.hue)}>
          <span className="voice-dot" aria-hidden />
          {l.t("site-ui-knock", { name: who.name })}
        </span>
      </span>
      <div className="ui-btns">
        <span className="ui-btn primary">{l.t("site-ui-approve")}</span>
      </div>
    </div>
  );
}

/** The board with cards in their owners' voices, and a claim. */
export function Board({ l, team }: L & { team: Voice[] }) {
  const [a, b, c] = team;
  if (!a || !b || !c) return null;
  return (
    <div data-ui className="ui-lines">
      <div className="kanban">
        <div className="col">
          <b>{l.t("site-ui-col-todo")}</b>
          <div className="card v" style={voiceStyle(c.hue)}>
            {l.t("site-ui-card-3")}
          </div>
        </div>
        <div className="col">
          <b>{l.t("site-ui-col-doing")}</b>
          <div className="card v" style={voiceStyle(a.hue)}>
            {l.t("site-ui-card-1")}
          </div>
          <div className="card v" style={voiceStyle(b.hue)}>
            {l.t("site-ui-card-2")}
          </div>
        </div>
        <div className="col">
          <b>{l.t("site-ui-col-review")}</b>
        </div>
      </div>
      <Claim l={l} who={a} path="src/pay/refund.ts" />
    </div>
  );
}

export function Claim({ l, who, path }: L & { who: Voice; path: string }) {
  return (
    <span className="chip v" style={voiceStyle(who.hue, 0)}>
      <span className="voice-dot" aria-hidden />
      <code>{path}</code>
      <span className="soft">
        {l.t("site-ui-claimed-by", { name: l.t("site-ui-agent-of", { name: who.name }) })}
      </span>
    </span>
  );
}

/** A merge that kept both tasks' intent. */
export function Merge({ l, team }: L & { team: Voice[] }) {
  const [a, b] = team;
  if (!a || !b) return null;
  return (
    <div data-ui className="ui-lines">
      <div className="ui-btns">
        <Chip kind="done">{l.t("site-ui-merged")}</Chip>
        <span className="chip">{l.t("site-ui-kept-both")}</span>
      </div>
      <pre className="term">
        <span className="v" style={{ ...voiceStyle(a.hue, 0), color: "var(--voice)" }}>
          {"+  for attempt in retries(3, on=Timeout):\n"}
        </span>
        <span className="v" style={{ ...voiceStyle(b.hue, 0), color: "var(--voice)" }}>
          {"+      key = idempotency_key(refund)\n"}
        </span>
        {"       charge.refund(refund, key=key)"}
      </pre>
    </div>
  );
}

export function StatusSet({ l }: L) {
  return (
    <div data-ui className="ui-lines">
      <Chip kind="running">{l.t("site-ui-status-running")}</Chip>
      <Chip kind="waiting">{l.t("site-ui-status-waiting")}</Chip>
      <Chip kind="blocked">{l.t("site-ui-status-blocked")}</Chip>
      <Chip kind="done">{l.t("site-ui-status-done")}</Chip>
    </div>
  );
}
