// The command palette (Ctrl/Cmd K): every view, room and common action.
import clsx from "clsx";
import { Dialog } from "radix-ui";
import { useEffect, useMemo, useState } from "react";
import { useApp, useS } from "../ctx";
import { useT } from "../i18n";
import { usePrefs } from "../prefs";
import { VIEWS } from "../store";

type Cmd = { id: string; label: string; hint?: string; run: () => void };

export function Palette() {
  const l = useT();
  const app = useApp();
  const rooms = useS((s) => s.rooms);
  const agents = useS((s) => s.agents);
  const [prefs, setPrefs] = usePrefs();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, []);
  const cmds = useMemo<Cmd[]>(() => {
    const out: Cmd[] = VIEWS.map((v) => ({
      id: `view-${v}`,
      label: l.t(`app-view-${v}`),
      hint: l.t("app-palette-view"),
      run: () => app.set({ view: v }),
    }));
    for (const r of rooms)
      out.push({
        id: `room-${r.id}`,
        label: r.name || l.t("tui-quick-session"),
        hint: l.t("app-palette-room"),
        run: () => app.selectRoom(r.id),
      });
    for (const a of agents.filter((x) => x.state !== "stopped"))
      out.push({
        id: `agent-${a.id}`,
        label: a.label,
        hint: l.t("app-palette-agent"),
        run: () => app.selectAgent(a.id),
      });
    out.push({ id: "host", label: l.t("app-host-or-join"), run: () => app.set({ room: null }) });
    out.push({
      id: "theme",
      label: l.t("app-palette-theme"),
      run: () => setPrefs({ theme: prefs.theme === "light" ? "dark" : "light" }),
    });
    return out;
  }, [l, app, rooms, agents, prefs.theme, setPrefs]);
  const needle = q.trim().toLowerCase();
  const list = needle ? cmds.filter((c) => c.label.toLowerCase().includes(needle)) : cmds;
  const run = (c: Cmd | undefined) => {
    if (!c) return;
    setOpen(false);
    setQ("");
    c.run();
  };
  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-[oklch(0_0_0/0.35)]" />
        <Dialog.Content className="fixed start-1/2 top-[15vh] z-50 w-[min(560px,calc(100vw-32px))] -translate-x-1/2 rounded-dialog bg-s2 shadow-float rtl:translate-x-1/2">
          <Dialog.Title className="sr-only">{l.t("app-palette")}</Dialog.Title>
          <input
            autoFocus
            value={q}
            aria-label={l.t("app-palette")}
            placeholder={l.t("app-palette-placeholder")}
            onChange={(e) => {
              setQ(e.target.value);
              setSel(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setSel((s) => Math.min(s + 1, list.length - 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSel((s) => Math.max(s - 1, 0));
              } else if (e.key === "Enter") {
                e.preventDefault();
                run(list[sel]);
              }
            }}
            className="h-11 w-full border-b border-line bg-transparent px-4 text-base text-ink placeholder:text-ink-2 focus:outline-none"
          />
          <ul
            role="listbox"
            aria-label={l.t("app-palette")}
            className="scroll-quiet max-h-80 overflow-y-auto p-1"
          >
            {list.map((c, i) => (
              <li
                key={c.id}
                role="option"
                aria-selected={i === sel}
                onMouseEnter={() => setSel(i)}
                onClick={() => run(c)}
                className={clsx(
                  "flex h-8 cursor-pointer items-center justify-between rounded-control px-3 text-sm",
                  i === sel ? "bg-s3 text-ink" : "text-ink",
                )}
              >
                <span className="truncate">{c.label}</span>
                {c.hint && <span className="text-xs text-ink-2">{c.hint}</span>}
              </li>
            ))}
            {list.length === 0 && (
              <li className="px-3 py-2 text-sm text-ink-2">{l.t("app-palette-none")}</li>
            )}
          </ul>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
