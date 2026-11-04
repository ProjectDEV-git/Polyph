// The desktop shell's side of the app: asks Tauri for the daemon's port and
// token (the shell starts the daemon if needed), then runs the shared app.
import { Localizer, Mark, PolyphApp, pickLang, type Platform } from "@polyph/ui";
import { invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { useCallback, useEffect, useMemo, useState } from "react";

type Conn = { port: number; token: string };

const platform: Platform = {
  kind: "desktop",
  notify: (title, body) => {
    void (async () => {
      let ok = await isPermissionGranted();
      if (!ok) ok = (await requestPermission()) === "granted";
      if (ok) sendNotification({ title, body });
    })();
  },
  openExternal: (url) => void invoke("open_url", { url }),
  togglePill: () => void invoke("toggle_pill"),
};

export function Desktop({ mode }: { mode: "app" | "pill" }) {
  const [conn, setConn] = useState<Conn | null>(null);
  const [failed, setFailed] = useState(false);
  const l = useMemo(() => new Localizer(pickLang([...navigator.languages])), []);
  const connect = useCallback(() => {
    setFailed(false);
    invoke<Conn>("connection").then(setConn, () => setFailed(true));
  }, []);
  useEffect(connect, [connect]);
  if (conn)
    return (
      <PolyphApp
        url={`ws://127.0.0.1:${conn.port}/ws?token=${encodeURIComponent(conn.token)}`}
        platform={platform}
        mode={mode}
        onOpenApp={() => void invoke("show_main")}
      />
    );
  if (mode === "pill") return null;
  return (
    <main className="flex min-h-dvh items-center justify-center bg-bg px-6">
      <div className="flex max-w-md flex-col items-start gap-3">
        <Mark size={28} className="text-accent" />
        <h1 className="font-serif text-2xl text-ink">{l.t("app-name")}</h1>
        <p role="status" className="text-sm text-ink-2">
          {failed ? l.t("app-unreachable") : l.t("app-connecting")}
        </p>
        {failed && (
          <button
            type="button"
            onClick={connect}
            className="h-8 rounded-control border border-line bg-s2 px-3 text-sm text-ink hover:bg-s3"
          >
            {l.t("app-retry")}
          </button>
        )}
      </div>
    </main>
  );
}
