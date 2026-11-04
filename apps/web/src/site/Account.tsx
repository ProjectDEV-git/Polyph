"use client";
// The account page: sign in by email code or GitHub, see the plan, linked
// devices and teams, invite teammates, and open billing. Talks to the
// account service (cloud/); nothing here touches room content.
import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { Localizer } from "@polyph/ui/core";
import {
  ApiFail,
  cloud,
  readToken,
  writeToken,
  type GithubStart,
  type Me,
  type Member,
  type Plan,
  type OnToken,
  type Run,
  type Task,
} from "./account-api";
import type { Lang } from "./i18n";

export function AccountPage({ lang }: { lang: Lang }) {
  const l = useMemo(() => new Localizer(lang), [lang]);
  const [token, setToken] = useState(null as string | null);
  const [me, setMe] = useState(null as Me | null);
  const [plans, setPlans] = useState([] as Plan[]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(null as string | null);
  const [note, setNote] = useState(null as string | null);
  const [invite, setInvite] = useState(null as string | null);

  const explain = useCallback(
    (e: unknown) => {
      const code = e instanceof ApiFail ? e.code : "failed";
      if (code === "unreachable") return l.t("site-acct-unreachable");
      const key = `site-acct-err-${code}`;
      return l.has(key)
        ? l.t(key)
        : l.t("site-acct-error", { detail: e instanceof Error ? e.message : String(e) });
    },
    [l],
  );

  const run = useCallback(
    async (f: Task) => {
      setBusy(true);
      setError(null);
      try {
        await f();
      } catch (e) {
        if (e instanceof ApiFail && e.code === "unauthorized") {
          writeToken(null);
          setToken(null);
          setMe(null);
        }
        setError(explain(e));
      } finally {
        setBusy(false);
      }
    },
    [explain],
  );

  const refresh = useCallback(async (t: string) => {
    const [m, p] = await Promise.all([cloud.me(t), cloud.plans()]);
    setMe(m);
    setPlans(p);
  }, []);

  useEffect(() => {
    const q = new URLSearchParams(window.location.search);
    const inv = q.get("invite");
    if (inv) setInvite(inv);
    if (q.get("billing") === "done") setNote(l.t("site-acct-billing-done"));
    if (inv || q.has("billing")) window.history.replaceState(null, "", window.location.pathname);
    const t = readToken();
    if (t) {
      setToken(t);
      void run(() => refresh(t));
    }
  }, [l, refresh, run]);

  const signedIn = (t: string) => {
    writeToken(t);
    setToken(t);
    return refresh(t);
  };

  if (!token || !me) {
    return (
      <SignIn
        l={l}
        lang={lang}
        busy={busy}
        error={error}
        run={run}
        onToken={signedIn}
        inviteNote={invite ? l.t("site-acct-invite-accept") : null}
      />
    );
  }

  const who = me.account.email ?? me.account.github_login ?? "";
  const purchasable = (id: string) => plans.some((p) => p.id === id && p.purchasable);
  const go = (url: string) => {
    window.location.assign(url);
  };
  return (
    <section
      className="board"
      aria-label={l.t("site-acct-title")}
      style={{ marginTop: "var(--gap)" }}
    >
      <div
        className="tile span-12 t2"
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: 12,
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <p>
          <b>{who}</b>
        </p>
        <button
          type="button"
          className="btn quiet"
          disabled={busy}
          onClick={() =>
            run(async () => {
              await cloud.logout(token).catch(() => undefined);
              writeToken(null);
              setToken(null);
              setMe(null);
            })
          }
        >
          {l.t("site-acct-signout")}
        </button>
      </div>
      {(error || note) && (
        <div className="span-12" role="status" aria-live="polite">
          {error && <p className="notice error">{error}</p>}
          {note && <p className="notice">{note}</p>}
        </div>
      )}
      {invite && (
        <div
          className="tile span-12 accent"
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 12,
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <p>{l.t("site-acct-invite-accept")}</p>
          <button
            type="button"
            className="btn quiet"
            disabled={busy}
            onClick={() =>
              run(async () => {
                const r = await cloud.acceptInvite(token, invite);
                setInvite(null);
                setNote(l.t("site-acct-joined", { team: r.team.name }));
                await refresh(token);
              })
            }
          >
            {l.t("site-acct-invite-join")}
          </button>
        </div>
      )}
      <div className="tile span-4 t3" style={{ display: "grid", gap: 10, alignContent: "start" }}>
        <h2>{l.t("site-acct-plan")}</h2>
        <p className="statement">{l.t(`site-plan-${me.plan}`)}</p>
        <p className="soft">
          {me.limits.retention_days != null
            ? l.t("site-acct-limits", { days: me.limits.retention_days })
            : l.t("site-acct-limits-none")}
        </p>
      </div>
      <div className="tile span-8" style={{ display: "grid", gap: 12, alignContent: "start" }}>
        <h2>{l.t("site-acct-devices")}</h2>
        {me.devices.length === 0 ? (
          <p className="soft">{l.t("site-acct-devices-none")}</p>
        ) : (
          <ul className="rows">
            {me.devices.map((d) => (
              <li key={d.device}>
                <span>
                  <b>{d.label || l.t("site-acct-device-unnamed")}</b>{" "}
                  <code className="soft">{d.device.slice(0, 16)}</code>
                </span>
                <span style={{ display: "flex", gap: 12, alignItems: "center" }}>
                  <span className="soft small">
                    {l.t("site-acct-linked", {
                      date: new Date(d.linked_at * 1000).toLocaleDateString(lang),
                    })}
                  </span>
                  <button
                    type="button"
                    className="btn quiet"
                    disabled={busy}
                    onClick={() =>
                      run(async () => {
                        await cloud.unlink(token, d.device);
                        await refresh(token);
                      })
                    }
                  >
                    {l.t("site-acct-unlink")}
                  </button>
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>
      <Teams
        l={l}
        me={me}
        token={token}
        busy={busy}
        run={run}
        refresh={() => refresh(token)}
        setNote={setNote}
      />
      <div className="tile span-6 t3" style={{ display: "grid", gap: 12, alignContent: "start" }}>
        <h2>{l.t("site-acct-billing")}</h2>
        {!me.billing ? (
          <p className="soft">{l.t("site-acct-billing-off")}</p>
        ) : (
          <div className="form-row">
            {me.plan === "free" && purchasable("pro") && (
              <button
                type="button"
                className="btn"
                disabled={busy}
                onClick={() => run(async () => go((await cloud.checkout(token, "pro")).url))}
              >
                {l.t("site-acct-upgrade", { plan: l.t("site-plan-pro") })}
              </button>
            )}
            {purchasable("team") &&
              me.teams
                .filter((t) => t.role === "owner")
                .map((t) => (
                  <button
                    key={t.id}
                    type="button"
                    className="btn"
                    disabled={busy}
                    onClick={() =>
                      run(async () => go((await cloud.checkout(token, "team", t.id)).url))
                    }
                  >
                    {l.t("site-acct-team-upgrade", { team: t.name })}
                  </button>
                ))}
            {me.subscriptions.length > 0 && (
              <button
                type="button"
                className="btn quiet"
                disabled={busy}
                onClick={() => run(async () => go((await cloud.portal(token)).url))}
              >
                {l.t("site-acct-manage")}
              </button>
            )}
          </div>
        )}
      </div>
      <div className="tile span-6" style={{ display: "flex", alignItems: "flex-end" }}>
        <button
          type="button"
          className="btn quiet"
          disabled={busy}
          onClick={() => {
            if (!window.confirm(l.t("site-acct-delete-confirm"))) return;
            void run(async () => {
              await cloud.deleteAccount(token);
              writeToken(null);
              setToken(null);
              setMe(null);
            });
          }}
        >
          {l.t("site-acct-delete")}
        </button>
      </div>
    </section>
  );
}

function SignIn(props: {
  l: Localizer;
  lang: Lang;
  busy: boolean;
  error: string | null;
  run: Run;
  onToken: OnToken;
  inviteNote: string | null;
}) {
  const { l, lang, busy, error, run, onToken, inviteNote } = props;
  const [email, setEmail] = useState("");
  const [pending, setPending] = useState(null as string | null);
  const [code, setCode] = useState("");
  const [gh, setGh] = useState(null as GithubStart | null);

  useEffect(() => {
    if (!gh) return;
    let stop = false;
    const tick = async () => {
      while (!stop) {
        await new Promise((r) => setTimeout(r, Math.max(1, gh.interval) * 1000));
        if (stop) return;
        try {
          const v = await cloud.githubPoll(gh.pending);
          if (v.token) {
            setGh(null);
            await run(() => onToken(v.token as string));
            return;
          }
        } catch (e) {
          setGh(null);
          await run(() => Promise.reject(e));
          return;
        }
      }
    };
    void tick();
    return () => {
      stop = true;
    };
  }, [gh, onToken, run]);

  const start = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      const v = await cloud.emailStart(email, lang);
      setPending(v.pending);
    });
  };
  const finish = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      const v = await cloud.emailFinish(pending ?? "", code);
      await onToken(v.token);
    });
  };
  const github = () =>
    run(async () => {
      setGh(await cloud.githubStart(lang));
    });

  return (
    <section className="board" aria-labelledby="signin-title" style={{ marginTop: "var(--gap)" }}>
      <div className="tile span-7 t2" style={{ display: "grid", gap: 16, alignContent: "start" }}>
        <h2 id="signin-title">{l.t("site-acct-signin")}</h2>
        {inviteNote && <p className="notice">{inviteNote}</p>}
        {!pending ? (
          <form onSubmit={start} className="form-row">
            <label className="field">
              {l.t("site-acct-email")}
              <input
                type="email"
                required
                autoComplete="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
              />
            </label>
            <button className="btn" disabled={busy}>
              {busy ? l.t("site-acct-working") : l.t("site-acct-send")}
            </button>
          </form>
        ) : (
          <form onSubmit={finish} style={{ display: "grid", gap: 12 }}>
            <p id="code-help">{l.t("site-acct-code-sent", { email })}</p>
            <div className="form-row">
              <label className="field">
                {l.t("site-acct-code")}
                <input
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  required
                  value={code}
                  aria-describedby="code-help"
                  onChange={(e) => setCode(e.target.value)}
                />
              </label>
              <button className="btn" disabled={busy}>
                {busy ? l.t("site-acct-working") : l.t("site-acct-verify")}
              </button>
            </div>
          </form>
        )}
        {error && (
          <p className="notice error" role="alert">
            {error}
          </p>
        )}
      </div>
      <div className="tile span-5 t3" style={{ display: "grid", gap: 16, alignContent: "start" }}>
        <p className="soft">{l.t("site-acct-or")}</p>
        {gh ? (
          <p role="status">
            {l.t("site-acct-github-open", { url: gh.verification_uri, code: gh.user_code })}{" "}
            <a href={gh.verification_uri} target="_blank" rel="noreferrer">
              {gh.verification_uri}
            </a>
          </p>
        ) : (
          <button
            type="button"
            className="btn quiet"
            disabled={busy}
            onClick={() => void github()}
            style={{ justifySelf: "start" }}
          >
            {l.t("site-acct-github")}
          </button>
        )}
      </div>
    </section>
  );
}

function Teams(props: {
  l: Localizer;
  me: Me;
  token: string;
  busy: boolean;
  run: Run;
  refresh: Task;
  setNote: (s: string | null) => void;
}) {
  const { l, me, token, busy, run, refresh, setNote } = props;
  const [name, setName] = useState("");
  const [members, setMembers] = useState({} as Record<string, Member[]>);
  const [emails, setEmails] = useState({} as Record<string, string>);

  useEffect(() => {
    let live = true;
    void Promise.all(
      me.teams.map((t) =>
        cloud
          .team(token, t.id)
          .then((v) => [t.id, v.members] as const)
          .catch(() => [t.id, []] as const),
      ),
    ).then((rows) => {
      if (live) setMembers(Object.fromEntries(rows));
    });
    return () => {
      live = false;
    };
  }, [me.teams, token]);

  return (
    <div className="tile span-12 t2" style={{ display: "grid", gap: 16, alignContent: "start" }}>
      <h2>{l.t("site-acct-teams")}</h2>
      {me.teams.length === 0 && <p className="soft">{l.t("site-acct-teams-none")}</p>}
      {me.teams.map((t) => (
        <div key={t.id} style={{ display: "grid", gap: 10 }}>
          <p>
            <b>{t.name}</b> <span className="soft">{l.t(`site-acct-role-${t.role}`)}</span>
          </p>
          <ul className="rows">
            {(members[t.id] ?? []).map((m) => (
              <li key={m.account}>
                <span>{m.email ?? m.github_login}</span>
                <span className="soft small">{l.t(`site-acct-role-${m.role}`)}</span>
                {m.account === me.account.id && m.role !== "owner" && (
                  <button
                    type="button"
                    className="btn quiet"
                    disabled={busy}
                    onClick={() =>
                      run(async () => {
                        await cloud.removeMember(token, t.id, m.account);
                        await refresh();
                      })
                    }
                  >
                    {l.t("site-acct-leave")}
                  </button>
                )}
              </li>
            ))}
          </ul>
          {t.role === "owner" && (
            <form
              className="form-row"
              onSubmit={(e) => {
                e.preventDefault();
                const email = emails[t.id] ?? "";
                void run(async () => {
                  await cloud.invite(token, t.id, email);
                  setEmails({ ...emails, [t.id]: "" });
                  setNote(l.t("site-acct-invited", { email }));
                });
              }}
            >
              <label className="field">
                {l.t("site-acct-invite-email")}
                <input
                  type="email"
                  required
                  value={emails[t.id] ?? ""}
                  onChange={(e) => setEmails({ ...emails, [t.id]: e.target.value })}
                />
              </label>
              <button className="btn quiet" disabled={busy}>
                {l.t("site-acct-invite")}
              </button>
            </form>
          )}
        </div>
      ))}
      <form
        className="form-row"
        onSubmit={(e) => {
          e.preventDefault();
          void run(async () => {
            await cloud.createTeam(token, name);
            setName("");
            await refresh();
          });
        }}
      >
        <label className="field">
          {l.t("site-acct-team-name")}
          <input required maxLength={80} value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <button className="btn" disabled={busy}>
          {l.t("site-acct-team-create")}
        </button>
      </form>
    </div>
  );
}
