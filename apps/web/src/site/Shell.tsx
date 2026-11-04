// The frame every site page shares: the name band, the link row, the page,
// and the footer of tiles. Rendered at build time in one language.
import type { ReactNode } from "react";
import { Mark } from "@polyph/ui/mark";
import { Arrow } from "./icons";
import { LANG_NAMES, SITE_LANGS, SOURCE_URL, href, strings, type Lang } from "./i18n";
import { Motion } from "./Motion";
import { ThemeToggle } from "./ThemeToggle";
import "./site.css";

export type PageKey =
  "home" | "pricing" | "download" | "security" | "privacy" | "terms" | "account";

const PATHS: Record<PageKey, string> = {
  home: "/",
  pricing: "/pricing/",
  download: "/download/",
  security: "/security/",
  privacy: "/privacy/",
  terms: "/terms/",
  account: "/account/",
};

export function pathOf(page: PageKey): string {
  return PATHS[page];
}

export function Shell({
  lang,
  page,
  children,
}: {
  lang: Lang;
  page: PageKey;
  children: ReactNode;
}) {
  const l = strings(lang);
  const links: [PageKey, string][] = [
    ["pricing", "site-nav-pricing"],
    ["download", "site-nav-download"],
    ["security", "site-nav-security"],
    ["account", "site-nav-account"],
  ];
  return (
    <div className="site" lang={lang}>
      <a className="skip" href="#content">
        {l.t("site-skip")}
      </a>
      <header className="board">
        <a
          className={`tile band${page === "home" ? "" : " small"}`}
          href={href(lang, "/")}
          aria-label={l.t("site-nav-home")}
        >
          <span className="name" aria-hidden>
            <Mark size={24} />
            Polyph{/* i18n-allow (product name) */}
          </span>
        </a>
        <nav className="links" aria-label={l.t("site-nav-label")}>
          {links.map(([key, label]) => (
            <a
              key={key}
              className="tile"
              href={href(lang, PATHS[key])}
              aria-current={page === key ? "page" : undefined}
            >
              {l.t(label)}
            </a>
          ))}
        </nav>
      </header>
      <main id="content">{children}</main>
      <Footer lang={lang} page={page} />
      <Motion />
    </div>
  );
}

function Footer({ lang, page }: { lang: Lang; page: PageKey }) {
  const l = strings(lang);
  const nav: [PageKey, string][] = [
    ["pricing", "site-nav-pricing"],
    ["download", "site-nav-download"],
    ["security", "site-nav-security"],
    ["account", "site-nav-account"],
    ["privacy", "site-nav-privacy"],
    ["terms", "site-nav-terms"],
  ];
  return (
    <footer className="board foot-board" style={{ marginTop: "var(--gap)" }}>
      <div className="tile span-4 statement-t">
        <p>{l.t("site-footer-statement")}</p>
        <p className="small soft">{l.t("site-footer-legal")}</p>
      </div>
      <nav className="tile span-3 t2" aria-label={l.t("site-nav-label")}>
        <ul>
          {nav.map(([key, label]) => (
            <li key={key}>
              <a href={href(lang, PATHS[key])}>{l.t(label)}</a>
            </li>
          ))}
          <li>
            <a href={SOURCE_URL}>{l.t("site-nav-source")}</a>
          </li>
        </ul>
      </nav>
      <div className="tile span-2">
        <p className="small soft" id="lang-label">
          {l.t("site-lang-label")}
        </p>
        <ul aria-labelledby="lang-label">
          {SITE_LANGS.map((code) => (
            <li key={code} lang={code}>
              <a
                href={href(code, PATHS[page])}
                aria-current={code === lang ? "true" : undefined}
                hrefLang={code}
              >
                {LANG_NAMES[code]}
              </a>
            </li>
          ))}
        </ul>
        <ThemeToggle
          labels={{
            group: l.t("site-theme-label"),
            dark: l.t("site-theme-dark"),
            light: l.t("site-theme-light"),
          }}
        />
      </div>
      <a className="tile span-3 t3 statement-t" href={SOURCE_URL}>
        <p className="small soft">{l.t("site-nav-source")}</p>
        <p style={{ fontSize: 20, fontFamily: "var(--font-sans)" }}>
          github.com/ProjectDEV-git/Polyph{/* i18n-allow (URL) */}
        </p>
      </a>
      <div className="tile span-8 ink wordmark" aria-hidden>
        <Mark size={24} />
        Polyph{/* i18n-allow (product name) */}
      </div>
      <a className="tile span-4 accent cta" href={href(lang, PATHS.download)}>
        {l.t("site-footer-cta")}
        <Arrow />
      </a>
    </footer>
  );
}
