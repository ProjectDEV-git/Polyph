// Fluent strings from locales/*.ftl, the same files the CLI and TUI use.
import { createContext, useContext, type ReactNode } from "react";
import { Localizer } from "./localizer";

export { LANGS, Localizer, pickLang, type Args, type Lang } from "./localizer";

const Ctx = createContext<Localizer>(new Localizer("en"));

export function I18nProvider({ l, children }: { l: Localizer; children: ReactNode }) {
  return <Ctx.Provider value={l}>{children}</Ctx.Provider>;
}

export function useT(): Localizer {
  return useContext(Ctx);
}
