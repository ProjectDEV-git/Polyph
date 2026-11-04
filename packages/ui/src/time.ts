import type { Localizer } from "./i18n";

/** "just now", "5 min ago", from unix seconds. */
export function ago(l: Localizer, at: number, now = Date.now() / 1000): string {
  const s = Math.max(0, now - at);
  if (s < 60) return l.t("tui-ago-now");
  if (s < 3600) return l.t("tui-ago-minutes", { n: Math.floor(s / 60) });
  if (s < 86400) return l.t("tui-ago-hours", { n: Math.floor(s / 3600) });
  return l.t("tui-ago-days", { n: Math.floor(s / 86400) });
}

/** Clock time in the person's locale, from unix seconds. */
export function clock(l: Localizer, at: number): string {
  return new Date(at * 1000).toLocaleTimeString(l.lang, { hour: "2-digit", minute: "2-digit" });
}

export function dateTime(l: Localizer, at: number): string {
  return new Date(at * 1000).toLocaleString(l.lang, { dateStyle: "medium", timeStyle: "short" });
}
