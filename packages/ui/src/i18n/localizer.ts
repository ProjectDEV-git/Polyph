// Fluent strings from locales/*.ftl, the same files the CLI and TUI use.
// No React here: the VS Code extension host uses it too.
import { FluentBundle, FluentResource, type FluentVariable } from "@fluent/bundle";
import { negotiateLanguages } from "@fluent/langneg";
import { LOCALES } from "./locales.gen";

export const LANGS = ["en", "th", "ja", "zh-Hans"] as const;
export type Lang = (typeof LANGS)[number];

export type Args = Record<string, FluentVariable | null | undefined>;

export class Localizer {
  readonly lang: Lang;
  private bundles: FluentBundle[];

  constructor(lang: Lang) {
    this.lang = lang;
    const chain = lang === "en" ? ["en"] : [lang, "en"];
    this.bundles = chain.map((l) => {
      // Isolation marks would leak into aria labels and tests; layout handles bidi.
      const b = new FluentBundle(l, { useIsolating: false });
      b.addResource(new FluentResource(LOCALES[l] ?? ""));
      return b;
    });
  }

  has(id: string): boolean {
    return this.bundles.some((b) => b.hasMessage(id));
  }

  t(id: string, args?: Args): string {
    for (const b of this.bundles) {
      const m = b.getMessage(id);
      if (m?.value) {
        const clean: Record<string, FluentVariable> = {};
        for (const [k, v] of Object.entries(args ?? {})) clean[k] = v ?? "";
        return b.formatPattern(m.value, clean);
      }
    }
    return id;
  }
}

/** The person's language from a setting or the browser's list. */
export function pickLang(wanted: readonly string[]): Lang {
  const got = negotiateLanguages([...wanted], [...LANGS], {
    defaultLocale: "en",
    strategy: "lookup",
  });
  const l = got[0];
  if (l && (LANGS as readonly string[]).includes(l)) return l as Lang;
  // zh, zh-CN, zh-SG all read Simplified Chinese.
  if (wanted.some((w) => /^zh(-(hans|cn|sg))?$/i.test(w))) return "zh-Hans";
  return "en";
}
