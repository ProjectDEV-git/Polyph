// The app root shared by the desktop shell and the browser client.
import { useEffect, useMemo, useState } from "react";
import { DaemonClient, type AppClient } from "./api/client";
import { ThemeCtx } from "./components/voice";
import { PlatformCtx, StoreCtx } from "./ctx";
import { I18nProvider, Localizer, pickLang } from "./i18n";
import type { Platform } from "./platform";
import { usePrefs } from "./prefs";
import { Store } from "./store";
import type { Theme } from "./voice";
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
  /** `ws://127.0.0.1:<port>/ws?token=…` */
  url?: string;
  /** A ready connection instead of `url` (the phone engine). */
  client?: AppClient;
  platform: Platform;
  mode?: "app" | "pill";
  onOpenApp?: () => void;
}

export function PolyphApp({
  url,
  client: given,
  platform,
  mode = "app",
  onOpenApp,
}: PolyphAppProps) {
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
    const client: AppClient = given ?? new DaemonClient(url ?? "");
    const s = new Store(client);
    client.onState = (c) => s.setConn(c);
    client.onEvent = (p) => s.handleEvent(p);
    client.onStatus = (p) => s.setStatus(p);
    return s;
  }, [url, given]);
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
