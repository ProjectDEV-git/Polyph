// The app frame: rail, title row with the room's one finding, the current
// view, and the "Needs you" column.
import clsx from "clsx";
import { Command, Hand, Menu } from "lucide-react";
import { Dialog } from "radix-ui";
import { useEffect, useState } from "react";
import { Mark } from "../components/mark";
import { Button, IconButton, Kbd } from "../components/ui";
import { useApp, usePlatform, useS } from "../ctx";
import { useT } from "../i18n";
import { ActivityView } from "./activity";
import { AgentsView } from "./agents";
import { ApprovalStack, describeApproval } from "./approvals";
import { BoardView } from "./board";
import { ChatView } from "./chat";
import { MergesView } from "./merges";
import { Palette } from "./palette";
import { Rail } from "./rail";
import { SettingsView } from "./settings";
import { StartView } from "./start";
import { TerminalsView } from "./terminals";
import { WhiteboardsView } from "./whiteboards";

const isMac = () => typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

function Finding() {
  const l = useT();
  const app = useApp();
  useS((s) => s.approvals);
  const agents = useS((s) => s.agents);
  const knocks = useS((s) => s.knocks);
  const mine = app.myApprovals();
  const running = agents.filter((a) => a.state === "running" || a.state === "starting").length;
  let text: string;
  if (mine[0])
    text =
      mine.length > 1
        ? l.t("app-finding-more", { first: describeApproval(l, mine[0]), n: mine.length - 1 })
        : describeApproval(l, mine[0]);
  else if (knocks[0])
    text = l.t(knocks[0].link ? "app-knock-link" : "app-knock", { name: knocks[0].display_name });
  else if (running) text = l.t("app-finding-running", { n: running });
  else text = l.t("app-finding-quiet");
  return (
    <p
      className={clsx(
        "truncate text-sm",
        mine.length || knocks.length ? "text-waiting" : "text-ink-2",
      )}
      data-testid="finding"
    >
      {text}
    </p>
  );
}

function Current() {
  const view = useS((s) => s.view);
  switch (view) {
    case "board":
      return <BoardView />;
    case "agents":
      return <AgentsView />;
    case "activity":
      return <ActivityView />;
    case "merges":
      return <MergesView />;
    case "terminals":
      return <TerminalsView />;
    case "whiteboards":
      return <WhiteboardsView />;
    case "settings":
      return <SettingsView />;
    default:
      return <ChatView />;
  }
}

function Notice() {
  const notice = useS((s) => s.notice);
  const app = useApp();
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => app.set({ notice: null }), 5000);
    return () => clearTimeout(t);
  }, [notice, app]);
  return (
    <div
      aria-live="polite"
      className="pointer-events-none fixed bottom-4 start-1/2 z-50 -translate-x-1/2 rtl:translate-x-1/2"
    >
      {notice && (
        <p className="rounded-card bg-s3 px-3 py-2 text-sm text-ink shadow-float">{notice.text}</p>
      )}
    </div>
  );
}

function Disconnected() {
  const l = useT();
  const app = useApp();
  const conn = useS((s) => s.conn);
  const ever = useS((s) => s.me !== null);
  const phone = usePlatform().kind === "phone";
  if (conn === "open") return null;
  return (
    <div
      role="status"
      className="flex items-center gap-3 border-b border-line bg-s1 px-4 py-2 text-sm text-ink-2"
    >
      <span>
        {conn === "connecting"
          ? l.t("app-connecting")
          : ever
            ? l.t("app-reconnecting")
            : l.t("app-unreachable")}
      </span>
      {conn === "closed" && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => (app.api as { retryNow?: () => void }).retryNow?.()}
        >
          {l.t("app-retry")}
        </Button>
      )}
    </div>
  );
}

export function Shell() {
  const l = useT();
  const room = useS((s) => s.rooms.find((r) => r.id === s.room));
  const roomId = useS((s) => s.room);
  const view = useS((s) => s.view);
  const app = useApp();
  useS((s) => s.approvals);
  const waiting = app.myApprovals().length + useS((s) => s.knocks.length);
  const [menu, setMenu] = useState(false);
  const [needs, setNeeds] = useState(false);
  const online = useS((s) => s.members.filter((m) => m.online).length);
  return (
    <div className="flex h-dvh min-h-0 flex-col bg-bg text-ink md:flex-row">
      {/* Phone and narrow windows: the rail folds into a sheet. */}
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line px-3 md:hidden">
        <Dialog.Root open={menu} onOpenChange={setMenu}>
          <Dialog.Trigger asChild>
            <IconButton label={l.t("app-menu")}>
              <Menu size={18} strokeWidth={1.5} />
            </IconButton>
          </Dialog.Trigger>
          <Dialog.Portal>
            <Dialog.Overlay className="fixed inset-0 z-40 bg-[oklch(0_0_0/0.35)]" />
            <Dialog.Content className="fixed inset-y-0 start-0 z-50 w-[min(300px,85vw)] bg-s1">
              <Dialog.Title className="sr-only">{l.t("app-menu")}</Dialog.Title>
              <Rail onPick={() => setMenu(false)} />
            </Dialog.Content>
          </Dialog.Portal>
        </Dialog.Root>
        <Mark size={18} className="text-accent" />
        <span className="truncate text-sm font-semibold">{l.t("app-name")}</span>
      </header>
      <aside className="hidden w-62 shrink-0 border-e border-line bg-bg md:block">
        <Rail />
      </aside>
      <main className="flex min-h-0 min-w-0 flex-1 flex-col">
        <Disconnected />
        {!roomId ? (
          <StartView />
        ) : (
          <>
            <div className="flex shrink-0 items-start gap-3 border-b border-line px-4 py-3">
              <div className="min-w-0 flex-1">
                <h1 className="truncate font-serif text-2xl tracking-[-0.01em] text-ink">
                  {view === "settings"
                    ? l.t("app-view-settings")
                    : room?.name || l.t("tui-quick-session")}
                </h1>
                <Finding />
              </div>
              <span className="hidden pt-1.5 text-xs text-ink-2 lg:inline">
                {l.t("app-online-count", { n: online })}
              </span>
              <span
                className="hidden items-center gap-1 pt-1 text-xs text-ink-2 lg:inline-flex"
                title={l.t("app-palette")}
              >
                {isMac() ? (
                  <Command size={12} strokeWidth={1.5} aria-hidden />
                ) : (
                  <Kbd>Ctrl{/* i18n-allow (key name) */}</Kbd>
                )}
                <Kbd>K</Kbd>
              </span>
              {waiting > 0 && (
                <Button
                  variant="secondary"
                  className="xl:hidden"
                  onClick={() => setNeeds((v) => !v)}
                  aria-expanded={needs}
                >
                  <Hand size={14} strokeWidth={1.5} className="text-waiting" />
                  <span className="num">{l.t("tui-approvals", { n: waiting })}</span>
                </Button>
              )}
            </div>
            {needs && (
              <div className="border-b border-line px-4 py-3 xl:hidden">
                <ApprovalStack />
              </div>
            )}
            <div className="flex min-h-0 min-w-0 flex-1">
              <Current />
            </div>
          </>
        )}
      </main>
      {/* The column appears only while something waits; the title row says so otherwise. */}
      {roomId && waiting > 0 && (
        <aside
          aria-label={l.t("app-needs-you")}
          className="scroll-quiet hidden w-80 shrink-0 overflow-y-auto border-s border-line px-4 py-3 xl:block"
        >
          <ApprovalStack />
        </aside>
      )}
      <Palette />
      <Notice />
    </div>
  );
}
