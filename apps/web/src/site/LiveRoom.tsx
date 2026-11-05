"use client";
// The live room: choose a team and see each person's voice and their agents'
// steps, assigned by the app's own voice code, with claims and approvals.
import { useMemo, useState } from "react";
import { Localizer } from "@polyph/ui/core";
import { Approval, Chip, Claim, Line } from "./fragments";
import type { Lang } from "./i18n";
import { SAMPLE_NAMES, team as makeTeam, voiceStyle } from "./voices";

const FILES = ["src/pay/refund.ts", "src/pay/webhook.ts", "src/api/routes.ts", "src/db/ledger.ts"]; // i18n-allow (file paths)

function Stepper(props: {
  id: string;
  value: number;
  min: number;
  max: number;
  set: (n: number) => void;
  fewer: string;
  more: string;
}) {
  const { id, value, min, max, set, fewer, more } = props;
  return (
    <span className="stepper">
      <button
        type="button"
        className="btn-sq"
        aria-label={fewer}
        aria-controls={id}
        disabled={value <= min}
        onClick={() => set(value - 1)}
      >
        −{/* i18n-allow (minus sign) */}
      </button>
      <output id={id} aria-live="polite">
        {value}
      </output>
      <button
        type="button"
        className="btn-sq"
        aria-label={more}
        aria-controls={id}
        disabled={value >= max}
        onClick={() => set(value + 1)}
      >
        +
      </button>
    </span>
  );
}

export function LiveRoom({ lang, howHref }: { lang: Lang; howHref: string }) {
  const l = useMemo(() => new Localizer(lang), [lang]);
  const [people, setPeople] = useState(3);
  const [agents, setAgents] = useState(1);
  const [claims, setClaims] = useState(true);
  const [approvals, setApprovals] = useState(true);
  const [light, setLight] = useState(false);
  const [hosted, setHosted] = useState(false);
  const team = useMemo(() => makeTeam(SAMPLE_NAMES.slice(0, people)), [people]);
  const agentName = (name: string, i: number) =>
    l.t("site-ui-agent-of", { name }) + (agents > 1 ? ` ${i + 1}` : "");
  const [a, b, c] = team;
  const third = c ?? a;
  return (
    <div className="live reveal">
      <div
        className="demo"
        data-theme={light ? "light" : "dark"}
        style={{ background: "var(--surface-2)", color: "var(--text)" }}
      >
        <div data-ui>
          <p className="small soft" id="roster-head">
            {l.t("site-show-roster")}
          </p>
          <ul
            className="roster"
            aria-labelledby="roster-head"
            style={{ listStyle: "none", margin: "8px 0 0", padding: 0 }}
          >
            {team.map((v) => (
              <li key={v.name} style={{ display: "contents" }}>
                <span className="v" style={voiceStyle(v.hue)}>
                  <span className="voice-dot" aria-hidden />
                  <b>{v.name}</b>
                </span>
                {Array.from({ length: agents }, (_, i) => (
                  <span key={i} className="agent v" style={voiceStyle(v.hue, i)}>
                    <span className="voice-dot" aria-hidden />
                    {agentName(v.name, i)}
                  </span>
                ))}
              </li>
            ))}
          </ul>
        </div>
        <div data-ui className="ui-lines">
          <Chip kind="running">
            {hosted ? l.t("site-show-mode-hosted") : l.t("site-show-mode-distributed")}
          </Chip>
          {agents > 0 &&
            (claims ? (
              <div className="ui-btns">
                {team.slice(0, Math.min(people, FILES.length)).map((v, i) => (
                  <Claim key={v.name} l={l} who={v} path={FILES[i] ?? ""} />
                ))}
              </div>
            ) : (
              <p className="notice small" style={{ color: "var(--blocked)" }}>
                {l.t("site-show-claims-off")}
              </p>
            ))}
        </div>
        <div data-ui className="ui-lines" style={{ alignContent: "end" }}>
          {a && b && agents > 0 && (
            <>
              <Line who={agentName(a.name, 0)} hue={a.hue} step={0}>
                {l.t("site-ui-line-1")}
              </Line>
              {claims && (
                <Line who={agentName(b.name, 0)} hue={b.hue} step={0}>
                  {l.t("site-ui-line-2", { name: a.name })}
                </Line>
              )}
            </>
          )}
          {third && (
            <Line who={third.name} hue={third.hue} step={null}>
              {l.t("site-ui-line-4")}
            </Line>
          )}
          {approvals && agents > 0 && a && <Approval l={l} who={a} />}
        </div>
      </div>
      <fieldset className="controls" style={{ border: 0, margin: 0 }}>
        <legend className="small soft" style={{ padding: "8px 0" }}>
          {l.t("site-show-controls")}
        </legend>
        <div className="control">
          <span id="ctl-people">{l.t("site-show-people")}</span>
          <Stepper
            id="people-n"
            value={people}
            min={2}
            max={10}
            set={setPeople}
            fewer={`${l.t("site-show-people")}: ${l.t("site-show-fewer")}`}
            more={`${l.t("site-show-people")}: ${l.t("site-show-more")}`}
          />
        </div>
        <div className="control">
          <span>{l.t("site-show-agents")}</span>
          <Stepper
            id="agents-n"
            value={agents}
            min={0}
            max={3}
            set={setAgents}
            fewer={`${l.t("site-show-agents")}: ${l.t("site-show-fewer")}`}
            more={`${l.t("site-show-agents")}: ${l.t("site-show-more")}`}
          />
        </div>
        {(
          [
            ["claims", claims, setClaims, "site-show-claims"],
            ["approvals", approvals, setApprovals, "site-show-approvals"],
            ["hosted", hosted, setHosted, "site-show-mode"],
            ["light", light, setLight, "site-show-theme"],
          ] as const
        ).map(([id, on, set, label]) => (
          <label key={id} className="control" htmlFor={`ctl-${id}`}>
            <span>{l.t(label)}</span>
            <input
              id={`ctl-${id}`}
              type="checkbox"
              role="switch"
              className="switch"
              checked={on}
              onChange={(e) => set(e.target.checked)}
            />
          </label>
        ))}
        <a href={howHref}>{l.t("site-show-link")}</a>
      </fieldset>
    </div>
  );
}
