// The first-run checklist on the desktop: what is left before your agents
// work in a room with your team. Each step says what to do and, where the
// app can do it, offers the button; connecting agents shows the exact
// changes to their settings first, like `polyph connect`.
import clsx from "clsx";
import { Check, Circle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, Input } from "../components/ui";
import { useApp, usePlatform, useS } from "../ctx";
import { useT } from "../i18n";
import { usePrefs } from "../prefs";

export interface SetupClient {
  id: string;
  name: string;
  installed: boolean;
  connected: boolean;
  via_extension: boolean;
}

export interface SetupStatus {
  name: string | null;
  rooms: number;
  folders: { room: string; path: string }[];
  clients: SetupClient[];
  agents: number;
  members: number;
}

export interface ClientChange {
  client: string;
  name: string;
  file: string;
  before: string | null;
  after: string | null;
  drops_comments: boolean;
}

/** The steps, in order, with whether each is done. */
export function steps(s: SetupStatus) {
  return [
    { id: "name", done: !!s.name },
    { id: "room", done: s.rooms > 0 },
    { id: "folder", done: s.folders.length > 0 },
    { id: "agents", done: s.clients.some((c) => c.connected) },
    { id: "running", done: s.agents > 0 },
    { id: "team", done: s.members > 1 },
  ] as const;
}

function Diff({ change }: { change: ClientChange }) {
  const l = useT();
  const lines = [
    ...(change.before ?? "")
      .split("\n")
      .filter(Boolean)
      .map((x) => `- ${x}`),
    ...(change.after ?? "")
      .split("\n")
      .filter(Boolean)
      .map((x) => `+ ${x}`),
  ];
  return (
    <div className="flex flex-col gap-1">
      <p className="text-xs text-ink-2">
        {l.t("app-setup-change", { client: change.name, file: change.file })}
      </p>
      <pre className="scroll-quiet max-h-40 overflow-auto rounded-control bg-s2 p-2 font-mono text-xs text-ink">
        {lines.join("\n")}
      </pre>
      {change.drops_comments && <p className="text-xs text-waiting">{l.t("app-setup-comments")}</p>}
    </div>
  );
}

function ConnectAgents({ status, reload }: { status: SetupStatus; reload: () => void }) {
  const l = useT();
  const app = useApp();
  const [plan, setPlan] = useState<ClientChange[] | null>(null);
  const waiting = status.clients.filter((c) => c.installed && !c.connected);
  const review = async () => {
    const changes = await app.act<ClientChange[]>("clients.plan", {});
    setPlan(changes ?? []);
  };
  if (!waiting.length) return <p className="text-xs text-ink-2">{l.t("app-setup-agents-none")}</p>;
  if (!plan)
    return (
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-ink-2">
          {l.t("app-setup-agents-found", { names: waiting.map((c) => c.name).join(", ") })}
        </span>
        <Button size="sm" variant="secondary" onClick={review}>
          {l.t("app-setup-agents-review")}
        </Button>
      </div>
    );
  return (
    <div className="flex flex-col gap-2">
      {plan.map((c) => (
        <Diff key={c.client} change={c} />
      ))}
      <div className="flex gap-2">
        <Button
          size="sm"
          variant="primary"
          onClick={async () => {
            await app.act("clients.connect", {}, l.t("app-setup-agents-done"));
            setPlan(null);
            reload();
          }}
        >
          {l.t("app-setup-agents-write")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => setPlan(null)}>
          {l.t("app-cancel")}
        </Button>
      </div>
    </div>
  );
}

function NameStep({ reload }: { reload: () => void }) {
  const l = useT();
  const app = useApp();
  const [name, setName] = useState("");
  return (
    <form
      className="flex items-center gap-2"
      onSubmit={async (e) => {
        e.preventDefault();
        if (!name.trim()) return;
        await app.act("config.set", { key: "name", value: name.trim() });
        reload();
      }}
    >
      <Input
        aria-label={l.t("app-settings-name")}
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="h-7 max-w-48"
      />
      <Button size="sm" type="submit" variant="secondary">
        {l.t("app-save")}
      </Button>
    </form>
  );
}

function TeamStep() {
  const l = useT();
  const app = useApp();
  const room = useS((s) => s.room);
  const [code, setCode] = useState<string | null>(null);
  if (!room) return null;
  if (code)
    return (
      <p className="text-xs text-ink-2">
        {l.t("app-setup-team-code")}{" "}
        <code className="rounded bg-s2 px-1 font-mono text-ink">polyph join {code}</code>
      </p>
    );
  return (
    <Button
      size="sm"
      variant="secondary"
      onClick={async () => {
        const r = await app.act<{ code: string }>("room.invite", { room, reusable: true });
        if (r?.code) setCode(r.code);
      }}
    >
      {l.t("app-setup-team-invite")}
    </Button>
  );
}

export function Checklist() {
  const l = useT();
  const app = useApp();
  const platform = usePlatform();
  const [prefs, setPrefs] = usePrefs();
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const conn = useS((s) => s.conn);
  const rooms = useS((s) => s.rooms.length);
  const agents = useS((s) => s.agents.length);
  const members = useS((s) => s.members.length);
  const reload = useCallback(() => {
    app.api.call<SetupStatus>("setup.status", {}).then(setStatus, () => setStatus(null));
  }, [app]);
  useEffect(() => {
    if (conn === "open") reload();
  }, [conn, rooms, agents, members, reload]);
  if (platform.kind !== "desktop" || prefs.setupHidden || !status) return null;
  const list = steps(status);
  const done = list.filter((s) => s.done).length;
  if (done === list.length) return null;
  const next = list.find((s) => !s.done)?.id;
  return (
    <section
      aria-label={l.t("app-setup-title")}
      className="border-b border-line bg-s1 px-4 py-3"
      data-testid="checklist"
    >
      <div className="flex items-center gap-3">
        <h2 className="text-sm font-medium text-ink">{l.t("app-setup-title")}</h2>
        <span className="num text-xs text-ink-2">
          {l.t("app-setup-progress", { done, n: list.length })}
        </span>
        <Button
          size="sm"
          variant="ghost"
          className="ms-auto"
          onClick={() => setPrefs({ setupHidden: true })}
        >
          {l.t("app-setup-hide")}
        </Button>
      </div>
      <ol className="mt-2 flex flex-col gap-2">
        {list.map((s) => (
          <li key={s.id} className="flex gap-2">
            {s.done ? (
              <Check
                size={14}
                strokeWidth={2}
                className="mt-0.5 shrink-0 text-accent"
                aria-hidden
              />
            ) : (
              <Circle
                size={14}
                strokeWidth={1.5}
                className="mt-0.5 shrink-0 text-ink-3"
                aria-hidden
              />
            )}
            <div className="flex min-w-0 flex-col gap-1">
              <span className={clsx("text-sm", s.done ? "text-ink-2 line-through" : "text-ink")}>
                {l.t(`app-setup-${s.id}`)}
                <span className="sr-only">
                  {" "}
                  {s.done ? l.t("app-setup-step-done") : l.t("app-setup-step-todo")}
                </span>
              </span>
              {s.id === next && (
                <>
                  <span className="text-xs text-ink-2">{l.t(`app-setup-${s.id}-how`)}</span>
                  {s.id === "name" && <NameStep reload={reload} />}
                  {s.id === "agents" && <ConnectAgents status={status} reload={reload} />}
                  {s.id === "team" && <TeamStep />}
                </>
              )}
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}
