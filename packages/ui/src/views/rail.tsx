// The rail: product mark, room switcher, views, the voice stack, settings.
import clsx from "clsx";
import {
  Activity,
  Bot,
  ChevronsUpDown,
  Columns3,
  GitMerge,
  MessageSquare,
  PenTool,
  Plus,
  Settings,
  SquareTerminal,
} from "lucide-react";
import { DropdownMenu } from "radix-ui";
import type { ReactNode } from "react";
import { Mark } from "../components/mark";
import { StatusMark } from "../components/status";
import { VoiceDot, VoiceName, agentSteps } from "../components/voice";
import { useApp, useS } from "../ctx";
import { useT } from "../i18n";
import type { View } from "../store";

const NAV: { view: View; icon: ReactNode }[] = [
  { view: "chat", icon: <MessageSquare size={16} strokeWidth={1.5} /> },
  { view: "board", icon: <Columns3 size={16} strokeWidth={1.5} /> },
  { view: "agents", icon: <Bot size={16} strokeWidth={1.5} /> },
  { view: "activity", icon: <Activity size={16} strokeWidth={1.5} /> },
  { view: "merges", icon: <GitMerge size={16} strokeWidth={1.5} /> },
  { view: "terminals", icon: <SquareTerminal size={16} strokeWidth={1.5} /> },
  { view: "whiteboards", icon: <PenTool size={16} strokeWidth={1.5} /> },
];

function NavItem({
  view,
  icon,
  count,
  onPick,
}: {
  view: View;
  icon: ReactNode;
  count?: number;
  onPick?: () => void;
}) {
  const l = useT();
  const app = useApp();
  const active = useS((s) => s.view === view);
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      onClick={() => {
        app.set({ view });
        onPick?.();
      }}
      className={clsx(
        "flex h-8 w-full items-center gap-2.5 rounded-control px-2 text-sm",
        active ? "bg-s3 text-ink" : "text-ink-2 hover:bg-s2 hover:text-ink",
      )}
    >
      <span className="text-ink-2">{icon}</span>
      <span className="flex-1 truncate text-start">{l.t(`app-view-${view}`)}</span>
      {count ? <span className="num text-xs text-ink-2">{count}</span> : null}
    </button>
  );
}

export function RoomSwitcher() {
  const l = useT();
  const app = useApp();
  const rooms = useS((s) => s.rooms);
  const room = useS((s) => s.rooms.find((r) => r.id === s.room));
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          aria-label={l.t("app-switch-room")}
          className="flex h-10 w-full items-center gap-2 rounded-control px-2 text-start hover:bg-s2"
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate font-serif text-lg text-ink">
              {room ? room.name || l.t("tui-quick-session") : l.t("tui-no-room")}
            </span>
          </span>
          <ChevronsUpDown size={14} strokeWidth={1.5} className="text-ink-2" />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="start"
          sideOffset={4}
          className="z-50 min-w-56 rounded-card bg-s2 p-1 text-sm text-ink shadow-float"
        >
          {rooms.map((r) => (
            <DropdownMenu.Item
              key={r.id}
              onSelect={() => app.selectRoom(r.id)}
              className="flex h-8 cursor-pointer items-center gap-2 rounded-control px-2 outline-none data-[highlighted]:bg-s3"
            >
              <span className="flex-1 truncate">{r.name || l.t("tui-quick-session")}</span>
              <span className="num text-xs text-ink-2">
                {l.t("tui-online", { n: r.members_online })}
              </span>
            </DropdownMenu.Item>
          ))}
          <DropdownMenu.Separator className="my-1 h-px bg-line" />
          <DropdownMenu.Item
            onSelect={() => app.set({ room: null })}
            className="flex h-8 cursor-pointer items-center gap-2 rounded-control px-2 text-ink-2 outline-none data-[highlighted]:bg-s3"
          >
            <Plus size={14} strokeWidth={1.5} />
            {l.t("app-host-or-join")}
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

/** People with their agents nested under them, each in its voice. */
export function VoiceStack({ onPick }: { onPick?: () => void }) {
  const l = useT();
  const app = useApp();
  const members = useS((s) => s.members);
  const agents = useS((s) => s.agents);
  const selected = useS((s) => s.agent);
  const steps = agentSteps(agents);
  const live = agents.filter((a) => a.state !== "stopped");
  const sorted = [...members].sort(
    (a, b) =>
      Number(b.me) - Number(a.me) ||
      Number(b.online) - Number(a.online) ||
      a.name.localeCompare(b.name),
  );
  return (
    <section aria-labelledby="rail-people" className="flex flex-col">
      <h2 id="rail-people" className="flex h-8 items-center px-2 text-xs text-ink-2">
        {l.t("app-people")}
      </h2>
      <ul className="flex flex-col">
        {sorted.map((m) => (
          <li key={m.member}>
            <div className="flex h-7 items-center gap-2 px-2 text-sm">
              <VoiceDot hue={m.hue} online={m.online} />
              <VoiceName hue={m.hue} className="truncate">
                {m.me ? l.t("tui-you", { name: m.name }) : m.name}
              </VoiceName>
              {!m.online && <span className="text-xs text-ink-2">{l.t("tui-offline")}</span>}
            </div>
            <ul>
              {live
                .filter((a) => a.member === m.member)
                .map((a) => (
                  <li key={a.id}>
                    <button
                      type="button"
                      onClick={() => {
                        app.selectAgent(a.id);
                        onPick?.();
                      }}
                      aria-current={selected === a.id ? "true" : undefined}
                      className={clsx(
                        "flex h-7 w-full items-center gap-2 rounded-control ps-6 pe-2 text-start text-sm",
                        selected === a.id ? "bg-s3" : "hover:bg-s2",
                      )}
                    >
                      <VoiceName
                        hue={m.hue}
                        step={steps.get(a.id) ?? 0}
                        className="min-w-0 flex-1 truncate font-normal"
                      >
                        {a.label.split("/").slice(1).join("/") || a.label}
                      </VoiceName>
                      <StatusMark state={a.state} compact />
                    </button>
                  </li>
                ))}
            </ul>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function Rail({ onPick }: { onPick?: () => void }) {
  const l = useT();
  const merges = useS((s) => s.merges.length);
  const app = useApp();
  const settings = useS((s) => s.view === "settings");
  return (
    <nav aria-label={l.t("app-name")} className="flex h-full flex-col gap-3 px-3 py-3">
      <div className="flex h-8 items-center gap-2 px-2 text-ink">
        <Mark size={20} className="text-accent" />
        <span className="text-sm font-semibold tracking-tight">{l.t("app-name")}</span>
      </div>
      <RoomSwitcher />
      <div className="flex flex-col">
        {NAV.map((n) => (
          <NavItem
            key={n.view}
            view={n.view}
            icon={n.icon}
            count={n.view === "merges" ? merges : undefined}
            onPick={onPick}
          />
        ))}
      </div>
      <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto border-t border-line pt-2">
        <VoiceStack onPick={onPick} />
      </div>
      <button
        type="button"
        aria-current={settings ? "page" : undefined}
        onClick={() => {
          app.set({ view: "settings" });
          onPick?.();
        }}
        className={clsx(
          "flex h-8 items-center gap-2.5 rounded-control px-2 text-sm",
          settings ? "bg-s3 text-ink" : "text-ink-2 hover:bg-s2 hover:text-ink",
        )}
      >
        <Settings size={16} strokeWidth={1.5} />
        {l.t("app-view-settings")}
      </button>
    </nav>
  );
}
