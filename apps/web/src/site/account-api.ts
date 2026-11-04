// Calls to the account service (cloud/). Errors carry the service's code so
// the page can show its own text for each.
import { CLOUD_API } from "./i18n";

export type Account = { id: string; email: string | null; github_login: string | null };
export type Device = { device: string; label: string; linked_at: number };
export type Team = { id: string; name: string; role: string };
export type Me = {
  account: Account;
  plan: string;
  limits: { retention_days: number | null };
  devices: Device[];
  teams: Team[];
  subscriptions: { plan: string; status: string }[];
  billing: boolean;
};
export type Plan = { id: string; purchasable: boolean; price: string | null };
export type Member = {
  account: string;
  email: string | null;
  github_login: string | null;
  role: string;
};
export type GithubStart = {
  pending: string;
  user_code: string;
  verification_uri: string;
  interval: number;
};
export type Task = () => Promise<void>;
export type Run = (f: Task) => Promise<void>;
/** Saves a new session and loads the account. */
export type OnToken = (token: string) => Promise<void>;

export class ApiFail extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

const TOKEN_KEY = "polyph-account-token";

export function readToken(): string | null {
  try {
    return sessionStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function writeToken(t: string | null): void {
  try {
    if (t) sessionStorage.setItem(TOKEN_KEY, t);
    else sessionStorage.removeItem(TOKEN_KEY);
  } catch {
    // Without storage the session lasts until the page closes.
  }
}

async function call<T>(
  method: string,
  path: string,
  token: string | null,
  body?: unknown,
): Promise<T> {
  let r: Response;
  try {
    r = await fetch(`${CLOUD_API}${path}`, {
      method,
      headers: {
        ...(body !== undefined ? { "content-type": "application/json" } : {}),
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  } catch {
    throw new ApiFail("unreachable", "unreachable");
  }
  const v = (await r.json().catch(() => ({}))) as { error?: string; message?: string };
  if (!r.ok) throw new ApiFail(v.error ?? "failed", v.message ?? String(r.status));
  return v as T;
}

export const cloud = {
  me: (t: string) => call<Me>("GET", "/v1/account", t),
  plans: () => call<{ plans: Plan[] }>("GET", "/v1/plans", null).then((v) => v.plans),
  emailStart: (email: string, lang: string) =>
    call<{ pending: string }>("POST", "/v1/auth/email/start", null, { email, lang }),
  emailFinish: (pending: string, code: string) =>
    call<{ token: string }>("POST", "/v1/auth/email/finish", null, { pending, code }),
  githubStart: (lang: string) => call<GithubStart>("POST", "/v1/auth/github/start", null, { lang }),
  githubPoll: (pending: string) =>
    call<{ status?: string; token?: string }>("POST", "/v1/auth/github/poll", null, { pending }),
  logout: (t: string) => call<unknown>("POST", "/v1/auth/logout", t, {}),
  unlink: (t: string, device: string) => call<unknown>("DELETE", `/v1/devices/${device}`, t),
  team: (t: string, id: string) => call<{ members: Member[] }>("GET", `/v1/teams/${id}`, t),
  createTeam: (t: string, name: string) => call<unknown>("POST", "/v1/teams", t, { name }),
  invite: (t: string, team: string, email: string) =>
    call<unknown>("POST", `/v1/teams/${team}/invites`, t, { email }),
  removeMember: (t: string, team: string, account: string) =>
    call<unknown>("DELETE", `/v1/teams/${team}/members/${account}`, t),
  acceptInvite: (t: string, token: string) =>
    call<{ team: { name: string } }>("POST", "/v1/invites/accept", t, { token }),
  checkout: (t: string, plan: string, team?: string) =>
    call<{ url: string }>("POST", "/v1/billing/checkout", t, { plan, team }),
  portal: (t: string) => call<{ url: string }>("POST", "/v1/billing/portal", t, {}),
  deleteAccount: (t: string) => call<unknown>("DELETE", "/v1/account", t),
};
