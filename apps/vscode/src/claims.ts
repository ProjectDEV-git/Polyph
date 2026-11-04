// Claims as the editor shows them: which claims cover a file, and how to
// describe them. Pure functions; the VS Code parts are in extension.ts.

export interface Claim {
  id: string;
  target: { path: string; symbol: string | null };
  owner: { id: string; member: string; label: string; kind: string; device: string };
  reason: string;
  expires_at: number;
}

/** Claims covering `rel` (a repository-relative path with `/`), most specific first. */
export function claimsFor(claims: readonly Claim[], rel: string): Claim[] {
  return claims
    .filter((c) => {
      const p = c.target.path;
      if (p === rel) return true;
      // A trailing `/` claims a directory and everything under it.
      return p.endsWith("/") && rel.startsWith(p);
    })
    .sort((a, b) => b.target.path.length - a.target.path.length);
}

/** Paths that have a claim somewhere beneath them (for folder badges). */
export function claimedDirs(claims: readonly Claim[]): Set<string> {
  const out = new Set<string>();
  for (const c of claims) {
    const parts = c.target.path.replace(/\/$/, "").split("/");
    for (let i = 1; i < parts.length; i++) out.add(parts.slice(0, i).join("/"));
  }
  return out;
}

/** Two letters for a file badge: the owner's initial and the agent kind's. */
export function badge(c: Claim): string {
  const [who = "", kind = ""] = c.owner.label.split("/");
  return ((who[0] ?? "") + (kind[0] ?? "")).toUpperCase() || "•";
}

/** Converts an absolute path to a repository-relative one, or null when outside. */
export function relative(root: string, abs: string): string | null {
  const norm = (s: string) => s.replace(/\\/g, "/").replace(/\/+$/, "");
  const r = norm(root);
  const a = norm(abs);
  if (a === r) return "";
  return a.startsWith(`${r}/`) ? a.slice(r.length + 1) : null;
}
