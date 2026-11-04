// Claims as editor decorations: which files other voices hold, and why.
import { voiceCss, type Theme } from "@polyph/ui/voice";

export interface Claim {
  id: string;
  target: { path: string; symbol?: string | null };
  owner: { id: string; member: string; device: string; kind: string; label: string };
  reason: string;
  task?: string | null;
  expires_at: number;
}

export interface Member {
  member: string;
  name: string;
  hue: number;
  me: boolean;
}

export interface FileClaim {
  /** Repository-relative path (a directory claim ends with `/`). */
  path: string;
  label: string;
  reason: string;
  symbol: string | null;
  color: string;
  mine: boolean;
}

/** Other people's claims (and the person's own, marked), with voice colors. */
export function fileClaims(claims: Claim[], members: Member[], theme: Theme): FileClaim[] {
  const me = members.find((m) => m.me)?.member;
  const steps = new Map<string, number>();
  const agentStep = (owner: Claim["owner"]) => {
    const key = `${owner.member}/${owner.id}`;
    if (!steps.has(key))
      steps.set(key, [...steps.keys()].filter((k) => k.startsWith(`${owner.member}/`)).length);
    return steps.get(key) ?? 0;
  };
  return claims.map((c) => ({
    path: c.target.path,
    label: c.owner.label,
    reason: c.reason,
    symbol: c.target.symbol ?? null,
    color: voiceCss(
      members.find((m) => m.member === c.owner.member)?.hue,
      agentStep(c.owner),
      theme,
    ),
    mine: c.owner.member === me,
  }));
}

/** The claim covering a repository-relative file path, if any. */
export function claimFor(claims: FileClaim[], rel: string): FileClaim | undefined {
  const p = rel.split("\\").join("/");
  return claims.find((c) => (c.path.endsWith("/") ? p.startsWith(c.path) : p === c.path));
}
