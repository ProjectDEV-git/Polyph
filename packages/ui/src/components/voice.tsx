// Voices: a person's hue, their agents as lighter and darker steps of it.
import clsx from "clsx";
import { createContext, useContext, type ReactNode } from "react";
import type { Agent, Member } from "../api/types";
import { voiceCss, type Step, type Theme } from "../voice";

export const ThemeCtx = createContext<Theme>("dark");
export const useTheme = () => useContext(ThemeCtx);

export function useVoice(hue: number | undefined | null, step: Step = null): string {
  return voiceCss(hue, step, useTheme());
}

export function VoiceDot({
  hue,
  step = null,
  online = true,
  size = 8,
}: {
  hue?: number | null;
  step?: Step;
  online?: boolean;
  size?: number;
}) {
  const c = useVoice(hue, step);
  return (
    <span
      aria-hidden
      className="inline-block shrink-0 rounded-full"
      style={{
        width: size,
        height: size,
        background: online ? c : "transparent",
        boxShadow: online ? undefined : `inset 0 0 0 1.5px ${c}`,
      }}
    />
  );
}

export function VoiceName({
  hue,
  step = null,
  children,
  className,
}: {
  hue?: number | null;
  step?: Step;
  children: ReactNode;
  className?: string;
}) {
  const c = useVoice(hue, step);
  return (
    <span className={clsx("font-medium", className)} style={{ color: c }}>
      {children}
    </span>
  );
}

/** The step of each agent: its index among its owner's agents, in list order. */
export function agentSteps(agents: Agent[]): Map<string, number> {
  const per = new Map<string, number>();
  const out = new Map<string, number>();
  for (const a of agents) {
    const n = per.get(a.member) ?? 0;
    out.set(a.id, n);
    per.set(a.member, n + 1);
  }
  return out;
}

export function hueOf(members: Member[], member: string | null | undefined): number | undefined {
  return members.find((m) => m.member === member)?.hue;
}
