"use client";
// The phone PWA: links this phone to you in a room (or joins as someone new),
// then runs the shared app on a room member inside the page. No daemon.
import {
  Button,
  Input,
  Label,
  Localizer,
  Mark,
  PolyphApp,
  pickLang,
  type Platform,
} from "@polyph/ui";
import { MemberClient, memberStorage } from "@polyph/ui/member";
import { useEffect, useMemo, useState } from "react";

const DEFAULT_RELAY = "wss://relay.polyph.dev/ws";

type Invite = { code: string; member: string | null; relay: string };
type Phase =
  | { t: "loading" }
  | { t: "setup"; invite: Invite | null; error?: string; busy?: "connecting" | "waiting" }
  | { t: "ready"; client: MemberClient };

/** `#link=CODE&member=HEX&relay=URL` from the QR code; dropped from the address bar at once. */
function readInvite(): Invite | null {
  const p = new URLSearchParams(window.location.hash.slice(1));
  const code = p.get("link") ?? p.get("join");
  if (!code) return null;
  history.replaceState(null, "", window.location.pathname);
  return { code, member: p.get("member"), relay: p.get("relay") ?? DEFAULT_RELAY };
}

function notify(title: string, body: string) {
  if (typeof Notification === "undefined" || Notification.permission !== "granted") return;
  // Through the service worker when there is one: it works with the app in the background.
  navigator.serviceWorker?.ready
    .then((r) => r.showNotification(title, { body, icon: "/icon-192.png", tag: "polyph" }))
    .catch(() => new Notification(title, { body }));
}

export function Phone() {
  const l = useMemo(() => new Localizer(pickLang([...navigator.languages])), []);
  const [phase, setPhase] = useState<Phase>({ t: "loading" });
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const [relay, setRelay] = useState(DEFAULT_RELAY);

  useEffect(() => {
    navigator.serviceWorker?.register("/sw.js", { scope: "/m/" }).catch(() => {});
    const invite = readInvite();
    (async () => {
      const saved = await MemberClient.restore().catch(() => null);
      if (saved) return setPhase({ t: "ready", client: saved });
      const profile = await memberStorage.profile().catch(() => null);
      if (profile) setName(profile.name);
      if (invite) {
        setCode(invite.code);
        setRelay(invite.relay);
      }
      setPhase({ t: "setup", invite });
    })();
  }, []);

  const platform: Platform = useMemo(
    () => ({
      kind: "phone",
      notify,
      openExternal: (url) => window.open(url, "_blank", "noopener"),
      leave: () => {
        void memberStorage.forget().then(() => window.location.reload());
      },
    }),
    [],
  );

  if (phase.t === "ready") return <PolyphApp client={phase.client} platform={platform} />;

  const invite = phase.t === "setup" ? phase.invite : null;
  const linking = !!invite?.member;
  const submit = async () => {
    if (phase.t !== "setup" || !name.trim() || !code.trim()) return;
    if (typeof Notification !== "undefined" && Notification.permission === "default") {
      void Notification.requestPermission();
    }
    setPhase({ ...phase, busy: "connecting", error: undefined });
    try {
      const client = await MemberClient.fresh(name.trim(), relay.trim());
      client.onState = (s) => {
        if (s === "open") setPhase((p) => (p.t === "setup" ? { ...p, busy: "waiting" } : p));
      };
      client.start();
      await client.call("join", { code: code.trim(), member: invite?.member ?? null });
      setPhase({ t: "ready", client });
    } catch (e) {
      setPhase({ t: "setup", invite, error: e instanceof Error ? e.message : String(e) });
    }
  };

  return (
    <main className="flex min-h-dvh flex-col bg-bg px-5 pb-8 pt-10">
      <div className="mx-auto flex w-full max-w-md flex-col gap-6">
        <div className="flex items-center gap-2 text-ink">
          <Mark size={22} className="text-accent" />
          <span className="text-sm font-semibold">{l.t("app-name")}</span>
        </div>
        {phase.t === "loading" ? (
          <p role="status" className="text-sm text-ink-2">
            {l.t("app-connecting-phone")}
          </p>
        ) : (
          <form
            className="flex flex-col gap-5"
            onSubmit={(e) => {
              e.preventDefault();
              void submit();
            }}
          >
            <div>
              <h1 className="font-serif text-[28px] leading-[34px] text-ink">
                {linking ? l.t("app-phone-link-title") : l.t("app-phone-join-title")}
              </h1>
              <p className="mt-2 text-base text-ink-2">
                {linking ? l.t("app-phone-link-body") : l.t("app-phone-join-body")}
              </p>
            </div>
            <div>
              <Label htmlFor="ph-name">{l.t("app-settings-name")}</Label>
              <Input
                id="ph-name"
                className="h-11 text-base"
                value={name}
                autoComplete="name"
                onChange={(e) => setName(e.target.value)}
              />
            </div>
            {!linking && (
              <>
                <div>
                  <Label htmlFor="ph-code">{l.t("app-join-code")}</Label>
                  <Input
                    id="ph-code"
                    className="h-11 font-mono text-base uppercase tracking-wide"
                    value={code}
                    autoComplete="off"
                    spellCheck={false}
                    onChange={(e) => setCode(e.target.value)}
                  />
                </div>
                <div>
                  <Label htmlFor="ph-relay">{l.t("app-settings-relay")}</Label>
                  <Input
                    id="ph-relay"
                    className="h-11 font-mono text-sm"
                    value={relay}
                    onChange={(e) => setRelay(e.target.value)}
                  />
                </div>
              </>
            )}
            <Button
              type="submit"
              variant="primary"
              className="h-11 text-base"
              disabled={!!phase.busy || !name.trim() || !code.trim()}
            >
              {linking ? l.t("app-phone-link") : l.t("app-join")}
            </Button>
            {phase.busy && (
              <p role="status" aria-live="polite" className="text-sm text-ink-2">
                {phase.busy === "waiting" ? l.t("app-phone-waiting") : l.t("app-connecting-phone")}
              </p>
            )}
            {phase.error && (
              <p role="alert" className="text-sm text-error">
                {phase.error}
              </p>
            )}
            <p className="text-xs text-ink-2">{l.t("app-phone-privacy")}</p>
          </form>
        )}
      </div>
    </main>
  );
}
