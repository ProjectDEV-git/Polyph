// React bindings for the localizer.
import { createContext, useContext, type ReactNode } from "react";
import { Localizer } from "./localizer";

export * from "./localizer";

const Ctx = createContext<Localizer>(new Localizer("en"));

export function I18nProvider({ l, children }: { l: Localizer; children: ReactNode }) {
  return <Ctx.Provider value={l}>{children}</Ctx.Provider>;
}

export function useT(): Localizer {
  return useContext(Ctx);
}
