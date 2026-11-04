// The folder this person works in (a git repository): agents, plans and
// terminals start there. Remembered per device in local storage.
import { useCallback, useSyncExternalStore } from "react";

const KEY = "polyph.workdir";
const subs = new Set<() => void>();

function read(): string {
  try {
    return localStorage.getItem(KEY) ?? "";
  } catch {
    return "";
  }
}

export function useWorkdir(): [string, (v: string) => void] {
  const v = useSyncExternalStore(
    (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    read,
    () => "",
  );
  const set = useCallback((next: string) => {
    try {
      localStorage.setItem(KEY, next);
    } catch {}
    for (const f of subs) f();
  }, []);
  return [v, set];
}
