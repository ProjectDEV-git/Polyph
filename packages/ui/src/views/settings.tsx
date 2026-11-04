// Settings: this device, appearance, the room, integrations and usage.
import { useEffect, useState, type ReactNode } from "react";
import { Button, Input, Label, Select } from "../components/ui";
import { useApp, useS } from "../ctx";
import { LANGS, useT } from "../i18n";
import { usePrefs } from "../prefs";
import { Part } from "../store";
import { useWorkdir } from "./workdir";

function Section({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <section className="grid gap-4 border-b border-line py-6 md:grid-cols-[240px_minmax(0,560px)]">
      <div>
        <h2 className="text-sm font-medium text-ink">{title}</h2>
        {hint && <p className="mt-1 text-sm text-ink-2">{hint}</p>}
      </div>
      <div className="flex flex-col gap-4">{children}</div>
    </section>
  );
}

function Row({ k, v, mono }: { k: string; v: ReactNode; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-line py-1.5 last:border-0">
      <dt className="text-sm text-ink-2">{k}</dt>
      <dd className={mono ? "truncate font-mono text-sm text-ink" : "text-sm text-ink"}>{v}</dd>
    </div>
  );
}

type Ledger = {
  input_tokens: number;
  output_tokens: number;
  cost: number;
  turns: number;
  subscription: boolean;
};

export function SettingsView() {
  const l = useT();
  const app = useApp();
  const me = useS((s) => s.me);
  const room = useS((s) => s.rooms.find((r) => r.id === s.room));
  const [prefs, setPrefs] = usePrefs();
  const [workdir, setWorkdir] = useWorkdir();
  const [name, setName] = useState(me?.name ?? "");
  const [integrations, setIntegrations] = useState<Record<string, unknown> | null>(null);
  const [usage, setUsage] = useState<{
    ledger: { room: Ledger; by_member: Record<string, Ledger> };
  } | null>(null);
  const [invite, setInvite] = useState<string | null>(null);
  const members = useS((s) => s.members);
  useEffect(() => setName(me?.name ?? ""), [me?.name]);
  useEffect(() => {
    app.api.call<Record<string, unknown>>("integrations.status").then(setIntegrations, () => {});
    if (room) app.api.call<typeof usage>("usage.get", { room: room.id }).then(setUsage, () => {});
  }, [app, room]);
  const fmt = new Intl.NumberFormat(l.lang);
  const money = new Intl.NumberFormat(l.lang, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
  });
  return (
    <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto px-6">
      <Section title={l.t("app-settings-device")} hint={l.t("app-settings-device-hint")}>
        <form
          className="flex items-end gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!name.trim()) return;
            await app.act("config.set", { key: "name", value: name.trim() }, l.t("app-saved"));
            void app.refresh(Part.ME | Part.MEMBERS);
          }}
        >
          <div className="flex-1">
            <Label htmlFor="st-name">{l.t("app-settings-name")}</Label>
            <Input id="st-name" value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <Button type="submit">{l.t("app-save")}</Button>
        </form>
        <div>
          <Label htmlFor="st-workdir">{l.t("app-workdir")}</Label>
          <Input
            id="st-workdir"
            className="font-mono"
            value={workdir}
            placeholder={l.t("app-workdir-placeholder")}
            onChange={(e) => setWorkdir(e.target.value)}
          />
          <p className="mt-1 text-xs text-ink-2">{l.t("app-workdir-hint")}</p>
        </div>
        <dl>
          <Row k={l.t("app-settings-relay")} v={me?.relay ?? "—"} mono />
          <Row k={l.t("app-settings-keystore")} v={me?.keystore ?? "—"} />
          <Row k={l.t("app-settings-device-id")} v={me?.device.slice(0, 16) ?? "—"} mono />
        </dl>
      </Section>
      <Section title={l.t("app-settings-appearance")}>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <Label htmlFor="st-theme">{l.t("app-theme")}</Label>
            <Select
              id="st-theme"
              value={prefs.theme}
              onChange={(e) => setPrefs({ theme: e.target.value as typeof prefs.theme })}
            >
              <option value="system">{l.t("app-theme-system")}</option>
              <option value="dark">{l.t("app-theme-dark")}</option>
              <option value="light">{l.t("app-theme-light")}</option>
            </Select>
          </div>
          <div>
            <Label htmlFor="st-lang">{l.t("app-language")}</Label>
            <Select
              id="st-lang"
              value={prefs.lang}
              onChange={(e) => setPrefs({ lang: e.target.value })}
            >
              <option value="">{l.t("app-language-system")}</option>
              {LANGS.map((x) => (
                <option key={x} value={x}>
                  {l.t(`app-language-${x.toLowerCase()}`)}
                </option>
              ))}
            </Select>
          </div>
        </div>
      </Section>
      {room && (
        <Section title={l.t("app-settings-room")} hint={l.t("app-settings-room-hint")}>
          <div className="flex items-center gap-3">
            <Button
              onClick={async () => {
                const r = await app.act<{ code: string }>("room.invite", { room: room.id });
                if (r) setInvite(r.code);
              }}
            >
              {l.t("app-invite")}
            </Button>
            {invite && (
              <span className="font-mono text-lg tracking-wide text-ink" data-testid="invite-code">
                {invite}
              </span>
            )}
          </div>
          {invite && <p className="text-xs text-ink-2">{l.t("app-invite-hint")}</p>}
          <dl>
            <Row k={l.t("app-settings-role")} v={l.t(`app-role-${room.role}`)} />
            <Row
              k={l.t("app-settings-members")}
              v={<span className="num">{fmt.format(room.members)}</span>}
            />
          </dl>
        </Section>
      )}
      <Section
        title={l.t("app-settings-integrations")}
        hint={l.t("app-settings-integrations-hint")}
      >
        <dl>
          <Row
            k={l.t("app-github")}
            v={
              integrations?.github
                ? l.t("app-connected")
                : integrations
                  ? l.t("app-not-connected")
                  : "—"
            }
          />
          <Row
            k={l.t("app-linear")}
            v={
              integrations?.linear
                ? l.t("app-connected")
                : integrations
                  ? l.t("app-not-connected")
                  : "—"
            }
          />
        </dl>
      </Section>
      {usage && (
        <Section title={l.t("app-settings-usage")} hint={l.t("app-settings-usage-hint")}>
          <table className="w-full border-collapse text-sm">
            <thead>
              <tr className="text-xs text-ink-2">
                <th className="h-8 text-start font-normal">{l.t("app-usage-who")}</th>
                <th className="h-8 text-end font-normal">{l.t("app-usage-turns")}</th>
                <th className="h-8 text-end font-normal">{l.t("app-usage-tokens")}</th>
                <th className="h-8 text-end font-normal">{l.t("app-usage-cost")}</th>
              </tr>
            </thead>
            <tbody>
              {Object.entries(usage.ledger.by_member).map(([id, u]) => (
                <tr key={id} className="border-t border-line">
                  <td className="py-1.5">
                    {members.find((m) => m.member === id)?.name ?? id.slice(0, 8)}
                  </td>
                  <td className="num py-1.5 text-end">{fmt.format(u.turns)}</td>
                  <td className="num py-1.5 text-end">
                    {fmt.format(u.input_tokens + u.output_tokens)}
                  </td>
                  <td className="num py-1.5 text-end">
                    {u.subscription ? "—" : money.format(u.cost)}
                  </td>
                </tr>
              ))}
              <tr className="border-t border-line font-medium">
                <td className="py-1.5">{l.t("app-usage-room")}</td>
                <td className="num py-1.5 text-end">{fmt.format(usage.ledger.room.turns)}</td>
                <td className="num py-1.5 text-end">
                  {fmt.format(usage.ledger.room.input_tokens + usage.ledger.room.output_tokens)}
                </td>
                <td className="num py-1.5 text-end">{money.format(usage.ledger.room.cost)}</td>
              </tr>
            </tbody>
          </table>
        </Section>
      )}
    </div>
  );
}
