"use client";
// Signs this tab in with the one-time link from `polyph web`, then runs the
// shared app against the daemon that served the page.
import { Localizer, Mark, PolyphApp, pickLang, type Platform } from "@polyph/ui";
import { useEffect, useMemo, useState } from "react";

const KEY = "polyph.session";

type Phase = { t: "signing-in" } | { t: "ready"; token: string } | { t: "expired" } | { t: "none" };

function readSession(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

async function check(token: string): Promise<boolean> {
  const r = await fetch("/rpc", {
    method: "POST",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "ping" }),
  }).catch(() => null);
  return !!r && r.status !== 401;
}

const platform: Platform = {
  kind: "web",
  openExternal: (url) => window.open(url, "_blank", "noopener"),
  notify: (title, body) => {
    if (typeof Notification !== "undefined" && Notification.permission === "granted")
      new Notification(title, { body });
  },
};

export function BrowserClient({ mode }: { mode: "app" | "pill" }) {
  const [phase, setPhase] = useState<Phase>({ t: "signing-in" });
  const l = useMemo(() => new Localizer(pickLang([...navigator.languages])), []);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const code = new URLSearchParams(window.location.hash.slice(1)).get("code");
      if (code) {
        // Drop the code from the address bar and history at once.
        history.replaceState(null, "", window.location.pathname);
        const r = await fetch("/web/session", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ code }),
        }).catch(() => null);
        if (r?.ok) {
          const { token } = (await r.json()) as { token: string };
          try {
            localStorage.setItem(KEY, token);
          } catch {}
          if (!cancelled) setPhase({ t: "ready", token });
          return;
        }
        if (!cancelled) setPhase({ t: "expired" });
        return;
      }
      const saved = readSession();
      if (saved && (await check(saved))) {
        if (!cancelled) setPhase({ t: "ready", token: saved });
        return;
      }
      if (!cancelled) setPhase({ t: "none" });
    })();
    return () => {
      cancelled = true;
    };
  }, []);
  if (phase.t === "ready") {
    const url = `ws://${window.location.host}/ws?token=${encodeURIComponent(phase.token)}`;
    return (
      <PolyphApp
        url={url}
        platform={platform}
        mode={mode}
        onOpenApp={() => window.open("../app/", "_blank")}
      />
    );
  }
  const text =
    phase.t === "signing-in"
      ? l.t("app-signing-in")
      : phase.t === "expired"
        ? l.t("app-link-expired")
        : l.t("app-no-session");
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
