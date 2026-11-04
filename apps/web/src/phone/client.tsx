"use client";
// The phone app: a room member that runs entirely in this browser. The room
// engine (wasm) talks to the relay directly; nothing goes through a daemon.
// A pairing link from a computer (`polyph phone`, or Settings > Pair a
// phone) carries a join code, the person's member id and the relay in the
// address fragment, which never reaches the server that hosts this page.
import {
  Localizer,
  Mark,
  PhoneClient,
  PolyphApp,
  describeApproval,
  pickLang,
  type Approval,
  type Platform,
} from "@polyph/ui";
import { useEffect, useMemo, useState } from "react";
import { loadEngine, supported } from "./engine";
import { Vault } from "./storage";

/** The relay a phone uses when no pairing link names one. */
const DEFAULT_RELAY = process.env.NEXT_PUBLIC_POLYPH_RELAY ?? "wss://relay.polyph.dev/ws";

/** Views that work without a daemon. */
const VIEWS = ["chat", "board", "agents", "activity", "whiteboards"] as const;

export interface PairingLink {
  join?: string;
  member?: string;
  relay?: string;
}

export function parseLink(hash: string): PairingLink | null {
  const q = new URLSearchParams(hash.replace(/^#/, ""));
  const join = q.get("join") ?? undefined;
  const member = q.get("member") ?? undefined;
  const relay = q.get("relay") ?? undefined;
  if (!join && !relay) return null;
  if (member && !/^[0-9a-f]{32}$/i.test(member)) return null;
  if (relay && !/^wss?:\/\//.test(relay)) return null;
  return { join, member, relay };
}

type Phase =
  | { t: "loading" }
  | { t: "unsupported" }
  | { t: "failed"; error: string }
  | { t: "ready"; client: PhoneClient; joinCode?: string };

async function showNotification(title: string, body: string) {
  if (typeof Notification === "undefined" || Notification.permission !== "granted") return;
  // Android only shows notifications through the service worker.
  const reg = await navigator.serviceWorker?.getRegistration().catch(() => undefined);
  if (reg) await reg.showNotification(title, { body, icon: "icon-192.png", tag: body });
  else new Notification(title, { body });
}

async function start(l: Localizer): Promise<Phase> {
  if (!supported()) return { t: "unsupported" };
  const link = parseLink(window.location.hash);
  // Drop the code and member id from the address bar and history. Keep the
  // router's own history state, or Next.js reloads the page.
  if (window.location.hash) history.replaceState(history.state, "", window.location.pathname);
  const [m, vault] = await Promise.all([loadEngine(), Vault.open()]);
  let secret = await vault.get("secret");
  if (!secret) {
    secret = m.generateDeviceSecret();
    await vault.put("secret", secret);
  }
  const now = Math.floor(Date.now() / 1000);
  const saved = await vault.get("engine");
  let engine = null;
  if (saved) {
    try {
      engine = m.PhoneEngine.restore(secret, saved, now);
    } catch (e) {
      console.error("polyph: could not restore this phone's room", e);
      engine = null;
    }
  }
  // A new pairing link replaces a phone that has no room yet.
  if (!engine || (!engine.hasRoom && link)) {
    engine = new m.PhoneEngine(secret, link?.member, "", link?.relay ?? DEFAULT_RELAY);
    // Saved at once, so the pairing (member and relay) outlives a reload.
    await vault.put("engine", engine.save());
  }
  const roomName = () => {
    try {
      const out = engine.call("room.list", "{}", Math.floor(Date.now() / 1000));
      const v = out.result ? (JSON.parse(out.result) as { name: string }[]) : [];
      return v[0]?.name ?? "";
    } catch {
      return "";
    }
  };
  const client = new PhoneClient(engine, {
    save: (b) => vault.put("engine", b).catch((e) => console.error("polyph: saving failed", e)),
    describe: (ev, me) => {
      const e = ev.event ?? {};
      if (e.type !== "approval_pending") return null;
      const a = e.approval as Approval;
      if (!me || a?.agent?.member !== me) return null;
      return l.t("notify-approval", {
        agent: a.agent.label,
        room: roomName(),
        action: describeApproval(l, a),
      });
    },
  });
  return { t: "ready", client, joinCode: engine.hasRoom ? undefined : link?.join };
}

/** Starts once per page load: the app may mount twice (React's strict mode,
 * the router reacting to the cleared address bar), and the pairing link is
 * read only once. */
let starting: Promise<Phase> | null = null;

export function PhoneApp() {
  const l = useMemo(() => new Localizer(pickLang([...navigator.languages])), []);
  const [phase, setPhase] = useState<Phase>({ t: "loading" });
  useEffect(() => {
    let cancelled = false;
    starting ??= start(l);
    starting.then(
      (p) => !cancelled && setPhase(p),
      (e) =>
        !cancelled && setPhase({ t: "failed", error: e instanceof Error ? e.message : String(e) }),
    );
    if ("serviceWorker" in navigator)
      navigator.serviceWorker.register("sw.js", { scope: "./" }).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [l]);
  const platform = useMemo<Platform>(
    () => ({
      kind: "phone",
      views: VIEWS,
      joinCode: phase.t === "ready" ? phase.joinCode : undefined,
      openExternal: (url) => window.open(url, "_blank", "noopener"),
      notify: (title, body) => void showNotification(title, body),
    }),
    [phase],
  );
  if (phase.t === "ready") return <PolyphApp client={phase.client} platform={platform} />;
  const text =
    phase.t === "loading"
      ? l.t("app-phone-loading")
      : phase.t === "unsupported"
        ? l.t("app-phone-unsupported")
        : l.t("app-phone-failed", { error: phase.error });
  return (
    <main className="flex min-h-dvh items-center justify-center bg-bg px-6">
      <div className="flex max-w-md flex-col gap-3">
        <Mark size={28} className="text-accent" />
        <h1 className="font-serif text-2xl text-ink">{l.t("app-name")}</h1>
        <p role="status" className="text-sm text-ink-2">
          {text}
        </p>
      </div>
    </main>
  );
}
