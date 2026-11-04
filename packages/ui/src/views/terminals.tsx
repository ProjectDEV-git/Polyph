// Shared terminals: watch any, type in yours or one whose keyboard you hold.
import { Terminal as XTerm } from "@xterm/xterm";
import clsx from "clsx";
import { Keyboard, Plus, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Terminal } from "../api/types";
import { Button, Empty, IconButton, Rows } from "../components/ui";
import { VoiceName, hueOf, useTheme } from "../components/voice";
import { useApp, useS } from "../ctx";
import { useT } from "../i18n";
import { Part } from "../store";
import { LoadError } from "./common";
import { useWorkdir } from "./workdir";

/** A CSS color (OKLCH tokens included) as #rrggbb, for xterm. */
function hex(css: string): string {
  const c = document.createElement("canvas");
  c.width = c.height = 1;
  const x = c.getContext("2d");
  if (!x) return "#000000";
  x.fillStyle = css;
  x.fillRect(0, 0, 1, 1);
  const [r, g, b] = x.getImageData(0, 0, 1, 1).data;
  return `#${[r, g, b].map((v) => (v ?? 0).toString(16).padStart(2, "0")).join("")}`;
}

function token(name: string): string {
  return hex(getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "black");
}

function toB64(s: string): string {
  const bytes = new TextEncoder().encode(s);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

function fromB64(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function TermScreen({ t, canType }: { t: Terminal; canType: boolean }) {
  const app = useApp();
  const room = useS((s) => s.room);
  const theme = useTheme();
  const host = useRef<HTMLDivElement>(null);
  const typing = useRef(canType);
  typing.current = canType;
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const term = new XTerm({
      cols: t.cols,
      rows: t.rows,
      fontFamily:
        getComputedStyle(document.documentElement).getPropertyValue("--font-mono") || "monospace",
      fontSize: 13,
      cursorBlink: false,
      disableStdin: false,
      theme: {
        background: token("--bg"),
        foreground: token("--text"),
        cursor: token("--accent"),
        selectionBackground: token("--surface-3"),
      },
    });
    term.open(el);
    let alive = true;
    const queue: Uint8Array[] = [];
    let ready = false;
    const off = app.onTerminal(t.id, (bytes) => (ready ? term.write(bytes) : queue.push(bytes)));
    app.api
      .call<{ data: string }>("term.read", { room, term: t.id })
      .then((r) => {
        if (!alive) return;
        term.write(fromB64(r.data ?? ""));
        for (const b of queue) term.write(b);
        ready = true;
      })
      .catch(() => {
        ready = true;
      });
    const input = term.onData((d) => {
      if (!typing.current) return;
      app.api
        .call("term.input", { room, term: t.id, data: toB64(d) })
        .catch((e) => app.say(String(e.message ?? e)));
    });
    return () => {
      alive = false;
      off();
      input.dispose();
      term.dispose();
    };
  }, [t.id, t.cols, t.rows, room, app, theme]);
  return (
    <div
      ref={host}
      className="scroll-quiet overflow-auto rounded-card bg-bg p-2"
      data-testid="xterm"
    />
  );
}

export function TerminalsView() {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const terms = useS((s) => s.terms).filter((t) => !t.closed);
  const me = useS((s) => s.me);
  const members = useS((s) => s.members);
  const load = useS((s) => s.load[Part.TERMS]);
  const [workdir] = useWorkdir();
  const [sel, setSel] = useState<string | null>(null);
  const t = terms.find((x) => x.id === sel) ?? terms[0];
  const owner = !!t && !!me && t.owner === me.member && t.device === me.device;
  const holder = !!t && !!me && t.writer === me.member;
  const canType = owner ? !t?.writer || t.writer === me?.member : holder;
  const writer = t?.writer ? members.find((m) => m.member === t.writer) : undefined;
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <aside
        aria-label={l.t("app-view-terminals")}
        className="flex w-60 shrink-0 flex-col border-e border-line"
      >
        <div className="flex h-12 items-center px-3">
          <Button
            onClick={async () => {
              const r = await app.act<{ id: string }>("term.open", { room, cwd: workdir || null });
              app.queue(Part.TERMS);
              if (r?.id) setSel(r.id);
            }}
          >
            <Plus size={14} strokeWidth={1.5} />
            {l.t("app-open-terminal")}
          </Button>
        </div>
        <ul className="px-2">
          {load === "loading" && terms.length === 0 && <Rows n={2} />}
          {terms.map((x) => {
            const m = members.find((mm) => mm.member === x.owner);
            return (
              <li key={x.id}>
                <button
                  type="button"
                  onClick={() => setSel(x.id)}
                  aria-current={t?.id === x.id ? "true" : undefined}
                  className={clsx(
                    "flex h-11 w-full flex-col justify-center rounded-control px-2 text-start",
                    t?.id === x.id ? "bg-s3" : "hover:bg-s2",
                  )}
                >
                  <span className="truncate font-mono text-sm text-ink">{x.title}</span>
                  <VoiceName hue={m?.hue} className="text-xs font-normal">
                    {m?.name ?? ""}
                  </VoiceName>
                </button>
              </li>
            );
          })}
        </ul>
      </aside>
      <div className="flex min-w-0 flex-1 flex-col gap-2 px-4 py-3">
        {load === "error" && terms.length === 0 ? (
          <LoadError part={Part.TERMS} />
        ) : !t ? (
          <Empty title={l.t("app-empty-terminals-title")}>{l.t("app-empty-terminals")}</Empty>
        ) : (
          <>
            <div className="flex h-8 items-center gap-3">
              <h2 className="truncate font-mono text-sm text-ink">{t.title}</h2>
              <span className="text-xs text-ink-2">
                {writer
                  ? l.t("tui-term-writer", { name: writer.name })
                  : canType
                    ? l.t("app-term-you-type")
                    : l.t("app-term-watching")}
              </span>
              <span className="flex-1" />
              {!owner && !holder && (
                <Button
                  size="sm"
                  onClick={() => app.act("term.ask", { room, term: t.id }, l.t("tui-term-asked"))}
                >
                  <Keyboard size={14} strokeWidth={1.5} />
                  {l.t("app-term-ask")}
                </Button>
              )}
              {holder && !owner && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => app.act("term.release", { room, term: t.id })}
                >
                  {l.t("app-term-release")}
                </Button>
              )}
              {owner && t.writer && t.writer !== me?.member && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => app.act("term.revoke", { room, term: t.id })}
                >
                  {l.t("app-term-revoke")}
                </Button>
              )}
              {owner && (
                <IconButton
                  label={l.t("app-term-close")}
                  onClick={async () => {
                    await app.act("term.close", { room, term: t.id });
                    app.queue(Part.TERMS);
                  }}
                >
                  <X size={16} strokeWidth={1.5} />
                </IconButton>
              )}
            </div>
            <TermScreen t={t} canType={canType} />
          </>
        )}
      </div>
    </div>
  );
}
