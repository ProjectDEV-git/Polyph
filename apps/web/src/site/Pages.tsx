// The site's inner pages: pricing, download, security and the legal drafts.
import { Approval, Board, Claim, Knock, Transcript } from "./fragments";
import { SAMPLE_NAMES, team } from "./voices";
import {
  PAID_RETENTION_DAYS,
  RELAY_PEAK_MB,
  SAMPLE_CODE,
  SOURCE_URL,
  href,
  strings,
  type Lang,
} from "./i18n";
import { Plans } from "./Home";
import { AccountPage } from "./Account";
import { Shell } from "./Shell";

function Title({
  lang,
  title,
  sub,
  side,
  draft,
}: {
  lang: Lang;
  title: string;
  sub?: string;
  side?: React.ReactNode;
  draft?: boolean;
}) {
  const l = strings(lang);
  return (
    <section
      className="page-title"
      aria-labelledby="page-title"
      style={{ marginTop: "var(--gap)" }}
    >
      <div className={`tile ${draft ? "t3" : "t2"} reveal`}>
        <h1 id="page-title">{title}</h1>
        {sub && <p>{sub}</p>}
      </div>
      <div
        className={`tile side ${draft ? "draft" : "t3"} reveal`}
        style={{ display: "flex", alignItems: "flex-end" }}
      >
        {draft ? <p>{l.t("site-legal-draft")}</p> : side}
      </div>
    </section>
  );
}

export function PricingPage({ lang }: { lang: Lang }) {
  const l = strings(lang);
  const who = "ann@example.com"; // i18n-allow (sample address)
  const server = "https://api.polyph.dev"; // i18n-allow (URL)
  return (
    <Shell lang={lang} page="pricing">
      <Plans lang={lang} asPage />
      <section
        className="board"
        aria-label={l.t("site-nav-account")}
        style={{ marginTop: "var(--gap)" }}
      >
        <div className="tile span-6 t2 reveal" data-ui style={{ minHeight: 220 }}>
          <Out
            cmd="polyph login --email ann@example.com"
            lines={[
              l.t("login-code-prompt", { email: who }) + " 40218377",
              l.t("login-done", { who, plan: l.t("cloud-plan-pro") }),
              l.t("login-device-linked"),
            ]}
          />
        </div>
        <div className="tile span-6 t3 reveal" data-ui style={{ minHeight: 220 }}>
          <Out
            cmd="polyph cloud status"
            lines={[
              l.t("cloud-account", { who, server }),
              l.t("cloud-plan", { plan: l.t("cloud-plan-pro") }),
              l.t("cloud-limits", {
                members: l.t("cloud-unlimited"),
                days: PAID_RETENTION_DAYS,
                links: l.t("cloud-unlimited"),
              }),
              l.t("cloud-devices", { count: 2 }),
              l.t("cloud-this-linked"),
            ]}
          />
        </div>
        <div className="tile span-7 t3 reveal" data-ui style={{ minHeight: 220 }}>
          <Out cmd="polyph relay serve --bind 0.0.0.0:8787" lines={[]} />
        </div>
        <div className="tile span-5 t2 reveal" data-ui style={{ minHeight: 220 }}>
          <Out cmd="polyph cloud status" lines={[l.t("cloud-signed-out", { server })]} />
        </div>
      </section>
    </Shell>
  );
}

function Out({ cmd, lines }: { cmd: string; lines: string[] }) {
  return (
    <pre className="term">
      <span className="prompt">$ </span>
      {cmd}
      {lines.map((line, i) => (
        <span key={i} className="out">
          {"\n"}
          {line}
        </span>
      ))}
    </pre>
  );
}

function Cmd({ lines }: { lines: string[] }) {
  return (
    <pre className="term" data-ui>
      {lines.map((line, i) => (
        <span key={i}>
          <span className="prompt">$ </span>
          {line}
          {"\n"}
        </span>
      ))}
    </pre>
  );
}

export function DownloadPage({ lang }: { lang: Lang }) {
  const l = strings(lang);
  const raw = "https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts"; // i18n-allow (URL)
  const install: [string, string[]][] = [
    ["site-dl-step-install", [`curl -fsSL ${raw}/install.sh | sh`]],
    ["site-dl-step-install-win", [`irm ${raw}/install.ps1 | iex`]],
    ["site-dl-step-host", ["cd ~/code/payments", "polyph host --name payments"]],
    ["site-dl-step-join", [`polyph join ${SAMPLE_CODE}`]],
  ];
  return (
    <Shell lang={lang} page="download">
      <Title
        lang={lang}
        title={l.t("site-dl-title")}
        sub={l.t("site-dl-sub")}
        side={
          <a href={`${SOURCE_URL}/releases`} style={{ fontWeight: 600 }}>
            {l.t("site-dl-releases")}
          </a>
        }
      />
      <section
        className="board"
        aria-label={l.t("site-dl-title")}
        style={{ marginTop: "var(--gap)" }}
      >
        {install.map(([key, lines], i) => (
          <div
            key={key}
            className={`tile span-6 ${i % 2 ? "t3" : "t2"} reveal`}
            data-ui
            style={{ display: "grid", gap: 14, alignContent: "start", minHeight: 220 }}
          >
            <h2>{l.t(key)}</h2>
            <Cmd lines={lines} />
          </div>
        ))}
        <div className="tile span-4 t3 reveal">
          <h2>{l.t("site-dl-apps-title")}</h2>
          <p>{l.t("site-dl-apps")}</p>
        </div>
        <div className="tile span-4 reveal">
          <h2>{l.t("site-dl-immutable-title")}</h2>
          <p>{l.t("site-dl-immutable")}</p>
        </div>
        <div
          className="tile span-4 t2 reveal"
          style={{ display: "grid", gap: 14, alignContent: "start" }}
        >
          <h2>{l.t("site-dl-relay-title")}</h2>
          <p>{l.t("site-dl-relay")}</p>
          <Cmd
            lines={["cd deploy/relay", "POLYPH_DOMAIN=relay.example.com docker compose up -d"]}
          />
        </div>
        <div
          className="tile span-6 t3 reveal"
          data-ui
          style={{ display: "grid", gap: 14, alignContent: "start", minHeight: 220 }}
        >
          <h2>{l.t("site-dl-source-title")}</h2>
          <Cmd
            lines={[
              `git clone ${SOURCE_URL}.git`,
              "cd Polyph",
              "cargo install --locked --path crates/polyph-cli",
            ]}
          />
        </div>
        <div className="tile span-6 t2 reveal prose">
          <h2>{l.t("site-dl-need")}</h2>
          <ul>
            <li>{l.t("site-dl-need-rust")}</li>
            <li>{l.t("site-dl-need-git")}</li>
            <li>{l.t("site-dl-need-agent")}</li>
          </ul>
        </div>
      </section>
    </Shell>
  );
}

export function SecurityPage({ lang }: { lang: Lang }) {
  const l = strings(lang);
  const room = team(SAMPLE_NAMES.slice(0, 3));
  return (
    <Shell lang={lang} page="security">
      <Title
        lang={lang}
        title={l.t("site-sec-title")}
        sub={l.t("site-sec-sub")}
        side={
          <div style={{ display: "grid", gap: 8 }}>
            <h2 style={{ fontFamily: "var(--font-sans)", fontSize: 17, fontWeight: 600 }}>
              {l.t("site-sec-content")}
            </h2>
            <p className="small">{l.t("site-sec-content-text")}</p>
          </div>
        }
      />
      <section
        className="board"
        aria-label={l.t("site-sec-title")}
        style={{ marginTop: "var(--gap)" }}
      >
        <div className="tile span-7 t2 reveal prose">
          <h2>{l.t("site-sec-seen")}</h2>
          <ul>
            {[1, 2, 3, 4, 5].map((i) => (
              <li key={i}>{l.t(`site-sec-seen-${i}`)}</li>
            ))}
          </ul>
        </div>
        <div className="tile span-5 t3 reveal prose">
          <h2>{l.t("site-sec-cannot")}</h2>
          <ul>
            {[1, 2, 3].map((i) => (
              <li key={i}>{l.t(`site-sec-cannot-${i}`)}</li>
            ))}
          </ul>
        </div>
        <div className="tile span-5 reveal prose">
          <h2>{l.t("site-sec-keys")}</h2>
          <p>{l.t("site-sec-keys-text")}</p>
        </div>
        <div className="tile span-4 t2 reveal prose">
          <h2>{l.t("site-dl-relay-title")}</h2>
          <p>{l.t("site-faq-a5", { mb: RELAY_PEAK_MB })}</p>
        </div>
        <div className="tile span-6 t3 reveal" data-ui style={{ minHeight: 220 }}>
          <Transcript l={l} team={room} />
        </div>
        <div
          className="tile span-6 t2 reveal"
          data-ui
          style={{ display: "grid", gap: 12, alignContent: "start" }}
        >
          <Approval l={l} who={room[0] ?? { name: "", hue: 0 }} />
          <Claim l={l} who={room[1] ?? { name: "", hue: 0 }} path="src/pay/webhook.ts" />
          <Knock l={l} who={room[2] ?? { name: "", hue: 0 }} />
        </div>
        <div className="tile span-12 t2 reveal" data-ui style={{ minHeight: 220 }}>
          <Board l={l} team={room} />
        </div>
        <a
          className="tile span-3 accent reveal"
          href={`${SOURCE_URL}/blob/Main/docs/security.md`}
          style={{ display: "flex", alignItems: "flex-end", fontWeight: 600 }}
        >
          {l.t("site-sec-docs")}
        </a>
      </section>
    </Shell>
  );
}

function Legal({
  lang,
  page,
  title,
  intro,
  parts,
}: {
  lang: Lang;
  page: "privacy" | "terms";
  title: string;
  intro: string;
  parts: [string, string][];
}) {
  const l = strings(lang);
  return (
    <Shell lang={lang} page={page}>
      <Title lang={lang} title={title} sub={intro} draft />
      <section className="board" aria-label={title} style={{ marginTop: "var(--gap)" }}>
        {parts.map(([h, body], i) => (
          <div key={h} className={`tile span-6 ${i % 2 ? "t3" : "t2"} reveal prose`}>
            <h2 className="reveal">{l.t(h)}</h2>
            <p className="reveal">{l.t(body, { days: PAID_RETENTION_DAYS })}</p>
          </div>
        ))}
        <a
          className="tile span-12 reveal"
          href={href(lang, page === "privacy" ? "/terms/" : "/privacy/")}
        >
          {l.t(page === "privacy" ? "site-nav-terms" : "site-nav-privacy")}
        </a>
      </section>
    </Shell>
  );
}

export function PrivacyPage({ lang }: { lang: Lang }) {
  const l = strings(lang);
  return (
    <Legal
      lang={lang}
      page="privacy"
      title={l.t("site-privacy-h1")}
      intro={l.t("site-privacy-intro")}
      parts={[
        ["site-privacy-h-content", "site-privacy-content"],
        ["site-privacy-h-relay", "site-privacy-relay"],
        ["site-privacy-h-account", "site-privacy-account"],
        ["site-privacy-h-delete", "site-privacy-delete"],
      ]}
    />
  );
}

export function TermsPage({ lang }: { lang: Lang }) {
  const l = strings(lang);
  return (
    <Legal
      lang={lang}
      page="terms"
      title={l.t("site-terms-h1")}
      intro={l.t("site-terms-intro")}
      parts={[
        ["site-terms-h-use", "site-terms-use"],
        ["site-terms-h-content", "site-terms-content"],
        ["site-terms-h-plans", "site-terms-plans"],
        ["site-terms-h-changes", "site-terms-changes"],
      ]}
    />
  );
}

export function AccountShell({ lang }: { lang: Lang }) {
  const l = strings(lang);
  return (
    <Shell lang={lang} page="account">
      <Title
        lang={lang}
        title={l.t("site-acct-h1")}
        sub={l.t("site-acct-sub")}
        side={<p className="small">{l.t("site-sec-seen-5")}</p>}
      />
      <AccountPage lang={lang} />
    </Shell>
  );
}
