// The app root shared by the desktop shell and the browser client.
import { useEffect, useMemo, useState } from "react";
import { type AppClient, DaemonClient } from "./api/client";
import { ThemeCtx } from "./components/voice";
import { PlatformCtx, StoreCtx } from "./ctx";
import { I18nProvider, Localizer, pickLang } from "./i18n";
import type { Platform } from "./platform";
import { usePrefs } from "./prefs";
import { Store } from "./store";
import type { Theme } from "./voice";
import { describeApproval } from "./views/approvals";
import { Pill } from "./views/pill";
import { Shell } from "./views/shell";

function useSystemTheme(): Theme {
  const q =
    typeof window !== "undefined"
      ? window.matchMedia?.("(prefers-color-scheme: light)")
      : undefined;
  const [light, setLight] = useState(!!q?.matches);
  useEffect(() => {
    if (!q) return;
    const on = () => setLight(q.matches);
    q.addEventListener("change", on);
    return () => q.removeEventListener("change", on);
  }, [q]);
  return light ? "light" : "dark";
}

export interface PolyphAppProps {
  /** `ws://127.0.0.1:<port>/ws?token=…` (the daemon), or pass `client`. */
  url?: string;
  /** Another client, such as the phone's own room member. */
  client?: AppClient;
  platform: Platform;
  mode?: "app" | "pill";
  onOpenApp?: () => void;
}

export function PolyphApp({ url, client, platform, mode = "app", onOpenApp }: PolyphAppProps) {
  const [prefs] = usePrefs();
  const system = useSystemTheme();
  const theme: Theme = prefs.theme === "system" ? system : prefs.theme;
  const lang = pickLang(
    prefs.lang
      ? [prefs.lang]
      : typeof navigator !== "undefined"
        ? [...navigator.languages]
        : ["en"],
  );
  const l = useMemo(() => new Localizer(lang), [lang]);
  const store = useMemo(() => {
    const c = client ?? new DaemonClient(url ?? "");
    const s = new Store(c);
    c.onState = (st) => s.setConn(st);
    c.onEvent = (p) => s.handleEvent(p);
    c.onStatus = (p) => s.setStatus(p);
    return s;
  }, [url, client]);
  useEffect(() => {
    const c = store.api as AppClient;
    c.start();
    return () => c.stop();
  }, [store]);
  useEffect(() => {
    store.onNotification = (text) => {
      if (platform.notify && (typeof document === "undefined" || !document.hasFocus()))
        platform.notify(l.t("app-name"), text);
    };
  }, [store, platform, l]);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.lang = lang;
  }, [theme, lang]);
  return (
    <PlatformCtx.Provider value={platform}>
      <StoreCtx.Provider value={store}>
        <I18nProvider l={l}>
          <ThemeCtx.Provider value={theme}>
            {mode === "pill" ? <Pill onOpenApp={onOpenApp} /> : <Shell />}
          </ThemeCtx.Provider>
        </I18nProvider>
      </StoreCtx.Provider>
    </PlatformCtx.Provider>
  );
}
