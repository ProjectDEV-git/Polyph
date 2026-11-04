// No room yet: host one, or join with a code.
import { useState } from "react";
import { Button, Input, Label } from "../components/ui";
import { useApp, usePlatform, useS } from "../ctx";
import { useT } from "../i18n";
import { ALL } from "../store";

/** The shape of a join code, shown as the field's example. */
const CODE_FORMAT = "A1B2-CDEF";

export function StartView() {
  const l = useT();
  const app = useApp();
  const rooms = useS((s) => s.rooms);
  const platform = usePlatform();
  // A phone joins rooms; hosting needs a computer running Polyph.
  const phone = platform.kind === "phone";
  const [name, setName] = useState("");
  const [me, setMe] = useState("");
  const [code, setCode] = useState(platform.joinCode ?? "");
  const [joining, setJoining] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <div className="scroll-quiet flex min-h-0 flex-1 flex-col gap-8 overflow-y-auto px-6 py-8">
      <div>
        <h1 className="font-serif text-[40px] leading-[46px] tracking-[-0.02em] text-ink">
          {l.t("app-no-room-title")}
        </h1>
        <p className="mt-2 max-w-prose text-base text-ink-2">
          {l.t(phone ? "app-phone-no-room-body" : "app-no-room-body")}
        </p>
      </div>
      <div className="grid max-w-3xl gap-6 md:grid-cols-2">
        {!phone && (
          <form
            className="flex flex-col gap-3 rounded-card bg-s1 p-4"
            onSubmit={async (e) => {
              e.preventDefault();
              setError(null);
              const r = await app.act<{ id: string }>("room.create", {
                name: name.trim() || l.t("app-default-room-name"),
                quick: false,
              });
              if (r?.id) {
                await app.refresh(ALL);
                app.selectRoom(r.id);
              }
            }}
          >
            <h2 className="text-sm font-medium text-ink">{l.t("app-host-room")}</h2>
            <div>
              <Label htmlFor="host-name">{l.t("app-room-name")}</Label>
              <Input id="host-name" value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <Button type="submit" variant="primary" className="self-start">
              {l.t("app-host")}
            </Button>
          </form>
        )}
        <form
          className="flex flex-col gap-3 rounded-card bg-s1 p-4"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!code.trim()) return;
            setJoining(true);
            setError(null);
            try {
              const r = await app.api.call<{ room?: string; id?: string }>("join", {
                code: code.trim(),
                ...(phone && me.trim() ? { name: me.trim() } : {}),
              });
              await app.refresh(ALL);
              const id = r.room ?? r.id;
              if (id) app.selectRoom(id);
              app.say(l.t("tui-joined"));
            } catch (err) {
              setError(err instanceof Error ? err.message : String(err));
            }
            setJoining(false);
          }}
        >
          <h2 className="text-sm font-medium text-ink">{l.t("app-join-room")}</h2>
          {phone && (
            <div>
              <Label htmlFor="join-name">{l.t("app-settings-name")}</Label>
              <Input
                id="join-name"
                value={me}
                onChange={(e) => setMe(e.target.value)}
                autoComplete="name"
                required
              />
            </div>
          )}
          <div>
            <Label htmlFor="join-code">{l.t("app-join-code")}</Label>
            <Input
              id="join-code"
              className="font-mono uppercase tracking-wide"
              placeholder={CODE_FORMAT}
              value={code}
              onChange={(e) => setCode(e.target.value)}
              autoComplete="off"
              spellCheck={false}
            />
          </div>
          <Button type="submit" variant="secondary" className="self-start" disabled={joining}>
            {l.t("app-join")}
          </Button>
          {joining && (
            <p className="text-sm text-ink-2" aria-live="polite">
              {l.t("app-join-waiting")}
            </p>
          )}
        </form>
      </div>
      {error && (
        <p role="alert" className="text-sm text-error">
          {error}
        </p>
      )}
      {rooms.length > 0 && (
        <section>
          <h2 className="mb-2 text-sm font-medium text-ink-2">{l.t("app-your-rooms")}</h2>
          <ul className="flex max-w-3xl flex-col">
            {rooms.map((r) => (
              <li key={r.id}>
                <button
                  type="button"
                  onClick={() => app.selectRoom(r.id)}
                  className="flex h-10 w-full items-center justify-between rounded-control px-2 hover:bg-s2"
                >
                  <span className="font-serif text-lg text-ink">
                    {r.name || l.t("tui-quick-session")}
                  </span>
                  <span className="num text-xs text-ink-2">
                    {l.t("tui-online", { n: r.members_online })}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
