import type { Localizer } from "../i18n";

/** A board column's localized name, falling back to the daemon's title. */
export function columnName(l: Localizer, id: string, fallback = id): string {
  const key = `column-${id.replace(/_/g, "-")}`;
  return l.has(key) ? l.t(key) : fallback;
}

/** Activity arguments with column ids replaced by their localized names. */
export function localizeColumns(
  l: Localizer,
  args: Record<string, unknown>,
): Record<string, unknown> {
  const out = { ...args };
  for (const k of ["column", "from", "to"]) {
    const v = out[k];
    if (typeof v === "string") out[k] = columnName(l, v, v);
  }
  return out;
}
