"use client";
// Dark is the default; the choice is remembered on this browser only.
import { useEffect, useState } from "react";

type Theme = "dark" | "light";

function saved(): Theme | null {
  try {
    const t = localStorage.getItem("polyph-site-theme");
    return t === "light" || t === "dark" ? t : null;
  } catch {
    return null;
  }
}

export function ThemeToggle({
  labels,
}: {
  labels: { group: string; dark: string; light: string };
}) {
  const [theme, setTheme] = useState<Theme>("dark");
  useEffect(() => {
    const t = saved();
    if (t) setTheme(t);
  }, []);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  const pick = (t: Theme) => {
    setTheme(t);
    try {
      localStorage.setItem("polyph-site-theme", t);
    } catch {
      // Private windows may refuse storage; the choice still applies now.
    }
  };
  return (
    <div className="theme-toggle" role="group" aria-label={labels.group}>
      <button type="button" aria-pressed={theme === "dark"} onClick={() => pick("dark")}>
        {labels.dark}
      </button>
      <button type="button" aria-pressed={theme === "light"} onClick={() => pick("light")}>
        {labels.light}
      </button>
    </div>
  );
}
