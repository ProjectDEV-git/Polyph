// Whiteboards: Excalidraw, synced element by element through the daemon.
import clsx from "clsx";
import { Plus } from "lucide-react";
import { Suspense, lazy, useEffect, useRef, useState } from "react";
import { Button, Empty, Input, Rows } from "../components/ui";
import { useTheme } from "../components/voice";
import { useApp, useS } from "../ctx";
import { useT } from "../i18n";
import { Part } from "../store";
import { LoadError } from "./common";

// Fonts come from the app's own files (copied at build), never a CDN. Set
// before Excalidraw's module runs: it builds its font URLs on load.
if (typeof window !== "undefined") {
  // A host whose files are not at the page's origin (the VS Code webview)
  // sets POLYPH_ASSET_BASE first.
  const w = window as unknown as { EXCALIDRAW_ASSET_PATH: string; POLYPH_ASSET_BASE?: string };
  w.EXCALIDRAW_ASSET_PATH = `${w.POLYPH_ASSET_BASE ?? `${window.location.origin}/`}excalidraw/`;
}

const Excalidraw = lazy(async () => {
  const m = await import("@excalidraw/excalidraw");
  return { default: m.Excalidraw };
});

type El = { id: string; version: number; [k: string]: unknown };

function Board({ id }: { id: string }) {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const theme = useTheme();
  const api = useRef<any>(null);
  const versions = useRef(new Map<string, number>());
  const [initial, setInitial] = useState<El[] | null>(null);
  const [failed, setFailed] = useState(false);
  const pushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const pull = async (first: boolean) => {
    try {
      const r = await app.api.call<{ elements: El[] }>("whiteboard.get", { room, board: id });
      const els = r.elements ?? [];
      for (const e of els) versions.current.set(e.id, e.version);
      if (first) setInitial(els);
      else api.current?.updateScene({ elements: els });
    } catch {
      setFailed(true);
    }
  };

  useEffect(() => {
    versions.current = new Map();
    setInitial(null);
    setFailed(false);
    void pull(true);
    return app.onWhiteboard((doc) => {
      if (doc === id) void pull(false);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, room]);

  if (failed) return <LoadError part={Part.WHITEBOARDS} />;
  if (!initial) return <Rows n={3} />;
  return (
    <div
      className="min-h-0 flex-1 overflow-hidden rounded-card border border-line"
      data-testid="whiteboard"
    >
      <Suspense
        fallback={<p className="p-3 text-sm text-ink-2">{l.t("app-whiteboard-loading")}</p>}
      >
        <Excalidraw
          excalidrawAPI={(a: unknown) => (api.current = a)}
          initialData={{
            elements: initial as any,
            appState: { viewBackgroundColor: "transparent" },
          }}
          theme={theme}
          langCode={
            l.lang === "zh-Hans"
              ? "zh-CN"
              : l.lang === "ja"
                ? "ja-JP"
                : l.lang === "th"
                  ? "th-TH"
                  : "en"
          }
          onChange={(els: readonly El[]) => {
            // Send only what changed since the daemon last told us.
            const changed = els.filter((e) => versions.current.get(e.id) !== e.version);
            if (!changed.length) return;
            if (pushTimer.current) clearTimeout(pushTimer.current);
            pushTimer.current = setTimeout(() => {
              const now = (api.current?.getSceneElementsIncludingDeleted?.() ?? els) as El[];
              const send = now.filter((e) => versions.current.get(e.id) !== e.version);
              for (const e of send) versions.current.set(e.id, e.version);
              if (send.length)
                app.api
                  .call("whiteboard.put", { room, board: id, elements: send })
                  .catch((e) => app.say(String(e.message ?? e)));
            }, 250);
          }}
        />
      </Suspense>
    </div>
  );
}

export function WhiteboardsView() {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const boards = useS((s) => s.whiteboards);
  const load = useS((s) => s.load[Part.WHITEBOARDS]);
  const [sel, setSel] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const b = boards.find((x) => x.id === sel) ?? boards[0];
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 px-4 py-3">
      <div className="flex flex-wrap items-center gap-1">
        {boards.map((x) => (
          <button
            key={x.id}
            type="button"
            aria-pressed={b?.id === x.id}
            onClick={() => setSel(x.id)}
            className={clsx(
              "h-8 rounded-control px-3 text-sm",
              b?.id === x.id ? "bg-s3 text-ink" : "text-ink-2 hover:bg-s2",
            )}
          >
            {x.name}
          </button>
        ))}
        {adding ? (
          <form
            className="w-56"
            onSubmit={async (e) => {
              e.preventDefault();
              if (!name.trim()) return;
              const r = await app.act<{ id: string }>("whiteboard.create", {
                room,
                name: name.trim(),
              });
              setName("");
              setAdding(false);
              await app.refresh(Part.WHITEBOARDS);
              if (r?.id) setSel(r.id);
            }}
          >
            <Input
              autoFocus
              value={name}
              aria-label={l.t("app-whiteboard-name")}
              placeholder={l.t("app-whiteboard-name")}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setAdding(false)}
            />
          </form>
        ) : (
          <Button variant="ghost" onClick={() => setAdding(true)}>
            <Plus size={14} strokeWidth={1.5} />
            {l.t("app-new-whiteboard")}
          </Button>
        )}
      </div>
      {load === "error" && boards.length === 0 ? (
        <LoadError part={Part.WHITEBOARDS} />
      ) : load === "loading" && boards.length === 0 ? (
        <Rows n={2} />
      ) : !b ? (
        <Empty title={l.t("app-empty-whiteboards-title")}>{l.t("app-empty-whiteboards")}</Empty>
      ) : (
        <Board key={b.id} id={b.id} />
      )}
    </div>
  );
}
