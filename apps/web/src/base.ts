// Where the site is served from: "" at a domain's root (Vercel, or a
// person's own daemon), "/Polyph" on GitHub Pages. Set at build time with
// POLYPH_BASE_PATH (next.config.mjs); Next.js adds it to its own assets, and
// these helpers add it to the plain links and files the site names itself.
export const BASE = process.env.NEXT_PUBLIC_BASE_PATH ?? "";

/** A site path with the base in front: withBase("/phone/"). */
export function withBase(path: string): string {
  return `${BASE}${path}`;
}

/** The whiteboard library loads its fonts from the site; tell it where. */
export function setAssetBase(): void {
  if (BASE && typeof window !== "undefined") {
    (window as unknown as { POLYPH_ASSET_BASE?: string }).POLYPH_ASSET_BASE =
      `${window.location.origin}${BASE}/`;
  }
}
