// One entry per site page; app/ routes render these for each language.
import type { Metadata } from "next";
import { Home } from "./Home";
import { PREFIXED, SITE_LANGS, asLang, href, strings, type Lang } from "./i18n";
import {
  AccountShell,
  DownloadPage,
  PricingPage,
  PrivacyPage,
  SecurityPage,
  TermsPage,
} from "./Pages";
import { pathOf, type PageKey } from "./Shell";

const PAGES: Record<PageKey, { render: (lang: Lang) => React.ReactNode; title?: string }> = {
  home: { render: (lang) => <Home lang={lang} /> },
  pricing: { render: (lang) => <PricingPage lang={lang} />, title: "site-nav-pricing" },
  download: { render: (lang) => <DownloadPage lang={lang} />, title: "site-nav-download" },
  security: { render: (lang) => <SecurityPage lang={lang} />, title: "site-nav-security" },
  privacy: { render: (lang) => <PrivacyPage lang={lang} />, title: "site-privacy-title" },
  terms: { render: (lang) => <TermsPage lang={lang} />, title: "site-terms-title" },
  account: { render: (lang) => <AccountShell lang={lang} />, title: "site-acct-title" },
};

export function render(page: PageKey, lang: string) {
  return PAGES[page].render(asLang(lang));
}

export function meta(page: PageKey, langIn: string): Metadata {
  const lang = asLang(langIn);
  const l = strings(lang);
  const key = PAGES[page].title;
  const brand = "Polyph"; // i18n-allow (product name)
  return {
    title: key ? `${l.t(key)} · ${brand}` : l.t("site-title"),
    description: l.t("site-description"),
    alternates: {
      canonical: href(lang, pathOf(page)),
      languages: Object.fromEntries(SITE_LANGS.map((code) => [code, href(code, pathOf(page))])),
    },
  };
}

/** Static params for the prefixed languages. */
export function langParams() {
  return PREFIXED.map((lang) => ({ lang }));
}
