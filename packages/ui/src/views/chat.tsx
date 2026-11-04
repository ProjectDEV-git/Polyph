// Room chat and threads.
import clsx from "clsx";
import { ArrowLeft, Plus } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ChatMessage } from "../api/types";
import { Button, Empty, IconButton, Input, Rows, SectionTitle, Textarea } from "../components/ui";
import { VoiceName, hueOf } from "../components/voice";
import { useApp, useS } from "../ctx";
import { useT } from "../i18n";
import { Part } from "../store";
import { ago, clock } from "../time";
import { LoadError } from "./common";

export function Composer({
  placeholder,
  onSend,
  label,
}: {
  placeholder: string;
  label: string;
  onSend: (text: string) => Promise<unknown> | void; // i18n-allow (type, not text)
}) {
  const l = useT();
  const [text, setText] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);
  const send = async () => {
    const t = text.trim();
    if (!t) return;
    setText("");
    await onSend(t);
    ref.current?.focus();
  };
  return (
    <form
      className="flex items-end gap-2 border-t border-line px-4 py-3"
      onSubmit={(e) => {
        e.preventDefault();
        void send();
      }}
    >
      <Textarea
        ref={ref}
        rows={1}
        value={text}
        aria-label={label}
        placeholder={placeholder}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            void send();
          }
        }}
        className="max-h-40"
        data-testid="composer"
      />
      <Button type="submit" variant="primary" disabled={!text.trim()}>
        {l.t("app-send")}
      </Button>
    </form>
  );
}

function Message({ m, first }: { m: ChatMessage; first: boolean }) {
  const l = useT();
  const members = useS((s) => s.members);
  const hue = hueOf(members, m.author);
  return (
    <li className={clsx("group grid grid-cols-[64px_1fr] gap-x-2 px-4", first ? "pt-2" : "pt-0.5")}>
      <time
        className="whitespace-nowrap num pt-px text-xs leading-5 text-ink-2"
        dateTime={new Date(m.at * 1000).toISOString()}
        title={ago(l, m.at)}
      >
        {first ? clock(l, m.at) : ""}
      </time>
      <div className="min-w-0">
        {first && (
          <p className="text-sm">
            <VoiceName hue={hue} step={m.agent ? 0 : null}>
              {m.agent ?? m.name}
            </VoiceName>
          </p>
        )}
        <p className="whitespace-pre-wrap break-words text-sm text-ink">{m.text}</p>
      </div>
    </li>
  );
}

function Messages({ list }: { list: ChatMessage[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  useLayoutEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [list.length]);
  return (
    <div
      ref={ref}
      onScroll={(e) => {
        const el = e.currentTarget;
        stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      }}
      className="scroll-quiet min-h-0 flex-1 overflow-y-auto pb-3"
    >
      <ol aria-live="polite" aria-relevant="additions" className="flex flex-col">
        {list.map((m, i) => {
          const prev = list[i - 1];
          const first =
            !prev || prev.author !== m.author || prev.agent !== m.agent || m.at - prev.at > 300;
          return <Message key={m.id} m={m} first={first} />;
        })}
      </ol>
    </div>
  );
}

function Threads() {
  const l = useT();
  const app = useApp();
  const threads = useS((s) => s.threads);
  const open = useS((s) => s.thread);
  const room = useS((s) => s.room);
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  return (
    <aside
      aria-labelledby="threads-h"
      className="flex w-60 shrink-0 flex-col border-s border-line px-3 py-2"
    >
      <SectionTitle
        aside={
          <IconButton label={l.t("app-new-thread")} onClick={() => setAdding((v) => !v)}>
            <Plus size={16} strokeWidth={1.5} />
          </IconButton>
        }
      >
        <span id="threads-h">{l.t("app-threads")}</span>
      </SectionTitle>
      {adding && (
        <form
          className="mb-2"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!title.trim()) return;
            const r = await app.act<{ id: string }>(
              "thread.create",
              { room, title: title.trim() },
              l.t("tui-thread-created"),
            );
            setTitle("");
            setAdding(false);
            app.queue(Part.CHAT);
            if (r?.id) app.set({ thread: r.id });
          }}
        >
          <Input
            autoFocus
            value={title}
            aria-label={l.t("app-thread-title")}
            placeholder={l.t("app-thread-title")}
            onChange={(e) => setTitle(e.target.value)}
          />
        </form>
      )}
      <ul className="flex flex-col">
        <li>
          <button
            type="button"
            onClick={() => app.set({ thread: null })}
            className={clsx(
              "flex h-8 w-full items-center rounded-control px-2 text-start text-sm",
              open === null ? "bg-s3 text-ink" : "text-ink-2 hover:bg-s2",
            )}
          >
            {l.t("tui-main-chat")}
          </button>
        </li>
        {threads.map((t) => (
          <li key={t.thread.id}>
            <button
              type="button"
              onClick={() => app.set({ thread: t.thread.id })}
              className={clsx(
                "flex h-10 w-full flex-col justify-center rounded-control px-2 text-start",
                open === t.thread.id ? "bg-s3" : "hover:bg-s2",
              )}
            >
              <span className="truncate text-sm text-ink">{t.thread.title}</span>
              <span className="num text-xs text-ink-2">
                {l.t("app-thread-posts", { n: t.posts })} · {ago(l, t.last_at)}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {threads.length === 0 && !adding && (
        <p className="px-2 pt-1 text-sm text-ink-2">{l.t("app-empty-threads")}</p>
      )}
    </aside>
  );
}

export function ChatView() {
  const l = useT();
  const app = useApp();
  const chat = useS((s) => s.chat);
  const room = useS((s) => s.room);
  const roomName = useS((s) => s.rooms.find((r) => r.id === s.room)?.name ?? "");
  const thread = useS((s) => s.thread);
  const threadTitle = useS(
    (s) => s.threads.find((t) => t.thread.id === s.thread)?.thread.title ?? "",
  );
  const load = useS((s) => s.load[Part.CHAT]);
  const list = chat.filter((m) => (m.thread ?? null) === thread);
  useEffect(() => {
    // Threads are fetched with posts only when open.
    if (thread) void app.refresh(Part.CHAT);
  }, [thread, app]);
  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        {thread && (
          <div className="flex h-10 items-center gap-2 border-b border-line px-4">
            <IconButton label={l.t("tui-main-chat")} onClick={() => app.set({ thread: null })}>
              <ArrowLeft size={16} strokeWidth={1.5} />
            </IconButton>
            <h2 className="truncate text-sm font-medium text-ink">{threadTitle}</h2>
          </div>
        )}
        {load === "loading" && list.length === 0 ? (
          <div className="flex-1 px-4 py-3">
            <Rows n={6} />
          </div>
        ) : load === "error" && list.length === 0 ? (
          <div className="flex-1 px-4 py-3">
            <LoadError part={Part.CHAT} />
          </div>
        ) : list.length === 0 ? (
          <div className="flex-1 px-4 py-3">
            <Empty title={l.t("app-empty-chat-title")}>{l.t("app-empty-chat")}</Empty>
          </div>
        ) : (
          <Messages list={list} />
        )}
        <Composer
          label={
            thread
              ? l.t("app-thread-placeholder", { thread: threadTitle })
              : l.t("app-chat-placeholder", { room: roomName })
          }
          placeholder={
            thread
              ? l.t("app-thread-placeholder", { thread: threadTitle })
              : l.t("app-chat-placeholder", { room: roomName })
          }
          onSend={async (text) => {
            await app.act("chat.send", { room, text, thread });
            app.queue(Part.CHAT);
          }}
        />
      </div>
      <div className="hidden xl:flex">
        <Threads />
      </div>
    </div>
  );
}
