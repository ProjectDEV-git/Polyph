// Per-device preferences: theme and language, kept in local storage.
import { useCallback, useSyncExternalStore } from "react";

export interface Prefs {
  theme: "system" | "dark" | "light";
  /** "" follows the browser's languages. */
  lang: string;
}

const KEY = "polyph.prefs";
// Dark is the default (PRODUCT.md); "system" follows the OS.
const DEFAULTS: Prefs = { theme: "dark", lang: "" };
const subs = new Set<() => void>();
let cache: Prefs | null = null;

function read(): Prefs {
  if (cache) return cache;
  try {
    cache = { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    cache = DEFAULTS;
  }
  return cache!;
}

export function usePrefs(): [Prefs, (p: Partial<Prefs>) => void] {
  const v = useSyncExternalStore(
    (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    read,
    () => DEFAULTS,
  );
  const set = useCallback((p: Partial<Prefs>) => {
    cache = { ...read(), ...p };
    try {
      localStorage.setItem(KEY, JSON.stringify(cache));
    } catch {}
    for (const f of subs) f();
  }, []);
  return [v, set];
}
