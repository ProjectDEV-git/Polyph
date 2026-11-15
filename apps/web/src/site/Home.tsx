// The home page: one board of tiles, read in the order of its argument —
// what Polyph is, a room you can set up, how work flows, the parts, every
// surface, the plans, questions.
import { Mark } from "@polyph/ui/mark";
import {
  Approval,
  Board,
  Claim,
  HostTerminal,
  Knock,
  Merge,
  Pill,
  Transcript,
} from "./fragments";
import {
  FREE_RETENTION_DAYS,
  FREE_SPECTATOR_LINKS,
  LANG_NAMES,
  PAID_RETENTION_DAYS,
  RELAY_PEAK_MB,
  SAMPLE_CODE,
  href,
  strings,
  type Lang,
} from "./i18n";
import { LiveRoom } from "./LiveRoom";
import { PlanPrice } from "./PlanPrice";
import { Shell } from "./Shell";
import { Spines } from "./Spines";
import { SAMPLE_NAMES, team } from "./voices";

export function Home({ lang }: { lang: Lang }) {
  const l = strings(lang);
  const room = team(SAMPLE_NAMES.slice(0, 3));
  const [ann, bo, cy] = room;
  if (!ann || !bo || !cy) return null;
  return (
    <Shell lang={lang} page="home">
      <Hero lang={lang} />
      <section className="board" aria-labelledby="show-title" style={{ marginTop: "var(--gap)" }}>
        <div className="tile span-4 section-head t2">
          <h2 id="show-title" className="reveal">
            {l.t("site-show-title")}
          </h2>
          <div className="reveal" style={{ display: "grid", gap: 16 }}>
            <p>{l.t("site-show-sub")}</p>
            <p className="soft small">{l.t("site-show-next")}</p>
          </div>
        </div>
        <div className="span-8">
          <LiveRoom lang={lang} howHref="#how-title" />
        </div>
      </section>

      <section aria-labelledby="how-title" style={{ marginTop: "var(--gap)", padding: "56px 0 0" }}>
        <div className="grow">
          <h2 id="how-title" className="head grow-head reveal">
            {l.t("site-how-title")}
          </h2>
          <div className="stage s1 reveal">
            <div className="tile t2" data-ui>
              <HostTerminal />
            </div>
            <h3>{l.t("site-how-host")}</h3>
            <p className="soft">{l.t("site-how-host-text")}</p>
          </div>
          <div className="stage s2 reveal">
            <div className="tile t3" data-ui>
              <Knock l={l} who={bo} />
            </div>
            <h3>{l.t("site-how-join")}</h3>
            <p className="soft">{l.t("site-how-join-text")}</p>
          </div>
          <div className="stage s3 reveal">
            <div className="tile t2" data-ui>
              <Board l={l} team={room} />
            </div>
            <h3>{l.t("site-how-plan")}</h3>
            <p className="soft">{l.t("site-how-plan-text")}</p>
          </div>
          <div className="stage s4 reveal">
            <div className="tile t3" data-ui>
              <Merge l={l} team={room} />
            </div>
            <h3>{l.t("site-how-merge")}</h3>
            <p className="soft">{l.t("site-how-merge-text")}</p>
          </div>
        </div>
      </section>

      <section className="mosaic" aria-labelledby="parts-title" style={{ marginTop: "var(--gap)" }}>
        <div className="tile w6h2 heading-tile accent reveal">
          <h2 id="parts-title" className="head">
            {l.t("site-parts-title")}
          </h2>
        </div>
        <div className="tile w3h4 t2 reveal">
          <div className="zoom z2" style={{ top: 28, left: 28 }}>
            <Claim l={l} who={bo} path="src/pay/webhook.ts" />
          </div>
          <Caption name={l.t("site-part-editing")} text={l.t("site-part-editing-text")} />
        </div>
        <div className="tile w3h4 t3 reveal">
          <div className="zoom z2" style={{ top: 24, left: 24, width: "160%" }}>
            <Approval l={l} who={ann} />
          </div>
          <Caption name={l.t("site-part-safety")} text={l.t("site-part-safety-text")} />
        </div>
        <div className="tile w6h2 reveal">
          <div className="zoom" style={{ top: 20, left: 24 }} data-ui>
            <span className="big-figure">
              <code>{SAMPLE_CODE}</code>
            </span>
          </div>
          <Caption name={l.t("site-part-server")} text={l.t("site-part-server-text")} />
        </div>
        <div className="tile w3h2 t2 reveal">
          <div className="zoom" style={{ top: 18, left: 24 }} data-ui>
            <span className="big-figure">{l.t("site-fig-tiers")}</span>
          </div>
          <Caption name={l.t("site-part-router")} text={l.t("site-part-router-text")} />
        </div>
        <div className="tile w3h2 t3 reveal">
          <div className="zoom" style={{ top: 18, left: 24 }} data-ui>
            <span className="big-figure">{l.t("site-fig-models")}</span>
          </div>
          <Caption name={l.t("site-part-models")} text={l.t("site-part-models-text")} />
        </div>
        <div className="tile w6h4 t2 reveal">
          <div className="zoom z2" style={{ top: 28, left: 28, width: "120%" }}>
            <Board l={l} team={room} />
          </div>
          <Caption name={l.t("site-part-context")} text={l.t("site-part-context-text")} />
        </div>
        <div className="tile w6h2 reveal">
          <ul className="zoom langs" style={{ top: 20, left: 24 }} data-ui>
            {(["th", "ja", "zh-Hans"] as const).map((code) => (
              <li key={code} className="big-figure" lang={code}>
                {LANG_NAMES[code]}
              </li>
            ))}
          </ul>
          <Caption name={l.t("site-part-langs")} text={l.t("site-part-langs-text")} />
        </div>
      </section>

      <div style={{ marginTop: "var(--gap)" }}>
        <Spines
          title={l.t("site-surfaces-title")}
          count={l.t("site-surfaces-count", { count: 7 })}
          cards={(
            [
              ["cli", "site-surface-everywhere"],
              ["tui", "site-surface-everywhere"],
              ["desktop", "site-surface-everywhere"],
              ["browser", "site-surface-everywhere"],
              ["phone", "site-surface-phones"],
              ["vscode", "site-surface-editor"],
              ["pill", "site-surface-everywhere"],
            ] as const
          ).map(([key, where]) => ({
            key,
            title: l.t(`site-surface-${key}`),
            text: l.t(`site-surface-${key}-text`),
            where: l.t(where),
            link: href(lang, "/download/"),
            linkLabel: l.t("site-surface-open"),
          }))}
        />
      </div>

      <div className="marquee" aria-hidden>
        <div className="track">
          {[0, 1, 2, 3].map((i) => (
            <span key={i}>
              {l.t("site-marquee")}
              <Mark size={24} />
            </span>
          ))}
        </div>
      </div>

      <Plans lang={lang} />
      <Faq lang={lang} />
    </Shell>
  );
}

function Caption({ name, text }: { name: string; text: string }) {
  return (
    <p className="caption">
      <b>{name}</b>
      <span>{text}</span>
    </p>
  );
}

function Hero({ lang }: { lang: Lang }) {
  const l = strings(lang);
  const room = team(SAMPLE_NAMES.slice(0, 3));
  return (
    <section className="hero" aria-labelledby="hero-title" style={{ marginTop: "var(--gap)" }}>
      <div className="left">
        <div className="tile t2" style={{ display: "flex", alignItems: "flex-start" }}>
          <Pill l={l} team={room} />
        </div>
        <div className="tile">
          <p className="small soft" id="facts-head" style={{ marginBottom: 10 }}>
            {l.t("site-hero-facts-label")}
          </p>
          <ul className="facts" aria-labelledby="facts-head">
            <li>{l.t("site-fact-platforms")}</li>
            <li>{l.t("site-fact-e2e")}</li>
            <li>{l.t("site-fact-selfhost")}</li>
            <li>{l.t("site-fact-agents")}</li>
            <li>{l.t("site-fact-langs")}</li>
          </ul>
        </div>
      </div>
      <div className="centre">
        <div className="tile t2 room-tile" data-ui>
          <div data-drift>
            <Transcript l={l} team={room} />
          </div>
          <div className="room-side">
            <Board l={l} team={room} />
            <Approval l={l} who={room[0] ?? { name: "", hue: 0 }} floating />
          </div>
        </div>
        <div className="tile t3 statement-tile">
          <h1 id="hero-title">{l.t("site-hero-title")}</h1>
          <p>{l.t("site-hero-sub")}</p>
        </div>
      </div>
      <div className="right">
        <div className="tile t1">{l.t("site-rail-agents")}</div>
        <div className="tile t2">{l.t("site-rail-code")}</div>
        <div className="tile t3">{l.t("site-rail-lan")}</div>
        <div className="tile t4">{l.t("site-rail-open")}</div>
        <a className="tile accent" href={href(lang, "/download/")}>
          {l.t("site-rail-action")}
        </a>
      </div>
    </section>
  );
}

export function Plans({ lang, asPage }: { lang: Lang; asPage?: boolean }) {
  const l = strings(lang);
  const Heading = asPage ? "h1" : "h2";
  const plans = [
    {
      id: "free",
      lines: [
        l.t("site-plan-free-1"),
        l.t("site-plan-free-2", { days: FREE_RETENTION_DAYS }),
        l.t("site-plan-free-3", { links: FREE_SPECTATOR_LINKS }),
        l.t("site-plan-free-4"),
      ],
      action: l.t("site-plan-free-action"),
      to: href(lang, "/download/"),
    },
    {
      id: "pro",
      lines: [
        l.t("site-plan-pro-1"),
        l.t("site-plan-pro-2", { days: PAID_RETENTION_DAYS }),
        l.t("site-plan-pro-3"),
      ],
      action: l.t("site-plan-paid-action"),
      to: href(lang, "/account/"),
    },
    {
      id: "team",
      lines: [
        l.t("site-plan-team-1"),
        l.t("site-plan-team-2", { days: PAID_RETENTION_DAYS }),
        l.t("site-plan-team-3"),
        l.t("site-plan-team-4"),
      ],
      action: l.t("site-plan-paid-action"),
      to: href(lang, "/account/"),
    },
  ];
  return (
    <section className="plans" aria-labelledby="plans-title" style={{ marginTop: "var(--gap)" }}>
      <div className="intro reveal">
        <Heading id="plans-title">{l.t(asPage ? "site-pricing-h1" : "site-pricing-title")}</Heading>
        <p>{l.t("site-pricing-sub")}</p>
      </div>
      {plans.map((p) => (
        <article key={p.id} className="plan reveal" aria-labelledby={`plan-${p.id}`}>
          <div className="body">
            <h3 id={`plan-${p.id}`}>{l.t(`site-plan-${p.id}`)}</h3>
            {p.id !== "free" && <PlanPrice plan={p.id} soon={l.t("site-price-soon")} />}
            <ul>
              {p.lines.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </div>
          <a className="strip" href={p.to}>
            {p.action}
          </a>
        </article>
      ))}
    </section>
  );
}

function Faq({ lang }: { lang: Lang }) {
  const l = strings(lang);
  return (
    <section className="faq" aria-labelledby="faq-title" style={{ marginTop: "var(--gap)" }}>
      <div className="faq-head">
        <h2 id="faq-title" className="reveal">
          {l.t("site-faq-title")}
        </h2>
      </div>
      <div>
        {[1, 2, 3, 4, 5].map((i) => (
          <div key={i} className="qa">
            <h3>{l.t(`site-faq-q${i}`)}</h3>
            <p>{l.t(`site-faq-a${i}`, { mb: RELAY_PEAK_MB })}</p>
          </div>
        ))}
      </div>
    </section>
  );
}
