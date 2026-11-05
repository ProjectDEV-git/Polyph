// Strings and links for the website. Pages render at build time, one copy
// per language: English at the root, the others under /th/, /ja/, /zh-Hans/.
import { LANGS, Localizer } from "@polyph/ui/core";

export type Lang = (typeof LANGS)[number];
export const SITE_LANGS: readonly Lang[] = LANGS;
/** Languages with their own path prefix (English lives at the root). */
export const PREFIXED: Lang[] = LANGS.filter((l) => l !== "en");

/** Each language's name, written in that language. */
export const LANG_NAMES: Record<Lang, string> = {
  en: "English", // i18n-allow (language names are written in their own language)
  th: "ไทย",
  ja: "日本語",
  "zh-Hans": "简体中文",
};

const cache = new Map<Lang, Localizer>();
export function strings(lang: Lang): Localizer {
  let l = cache.get(lang);
  if (!l) {
    l = new Localizer(lang);
    cache.set(lang, l);
  }
  return l;
}

/** A site path in a language: href("th", "/pricing/") is "/th/pricing/". */
export function href(lang: Lang, path: string): string {
  return lang === "en" ? path : `/${lang}${path}`;
}

export function asLang(s: string): Lang {
  return (LANGS as readonly string[]).includes(s) ? (s as Lang) : "en";
}

/** Measured in the relay load test (docs/self-hosting.md): peak RSS, normal load. */
export const RELAY_PEAK_MB = 71;
/** The plan limits the brief fixes (cloud/src/plans.rs defaults). */
export const FREE_RETENTION_DAYS = 7;
export const PAID_RETENTION_DAYS = 365;
export const FREE_SPECTATOR_LINKS = 1;
/** The hosted account service; set at build time. */
export const CLOUD_API = process.env.NEXT_PUBLIC_POLYPH_CLOUD ?? "https://api.polyph.dev";
export const SOURCE_URL = "https://github.com/ProjectDEV-git/Polyph";
/** A join code for pictures of the interface (not a real room). */
export const SAMPLE_CODE = "A1B2-CDEF"; // i18n-allow (sample join code)
