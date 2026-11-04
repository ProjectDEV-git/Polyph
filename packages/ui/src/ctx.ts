import { createContext, useContext } from "react";
import type { Platform } from "./platform";
import { type State, type Store, useStore } from "./store";

export const StoreCtx = createContext<Store | null>(null);
export const PlatformCtx = createContext<Platform>({ kind: "web" });

export function useApp(): Store {
  const s = useContext(StoreCtx);
  if (!s) throw new Error("no store");
  return s;
}

export function useS<T>(sel: (s: State) => T): T {
  return useStore(useApp(), sel);
}

export const usePlatform = () => useContext(PlatformCtx);
