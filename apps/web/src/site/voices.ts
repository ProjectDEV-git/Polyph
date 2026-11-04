// Voice colors for the site's sample room, computed with the same code the
// app uses (packages/ui/src/voice.ts), so the page shows real assignments.
import type { CSSProperties } from "react";
import { assignHue, voiceCss } from "@polyph/ui/voice";

export type Voice = { name: string; hue: number };

/** Hues for people in join order, as a room assigns them. */
export function team(names: string[]): Voice[] {
  const used: number[] = [];
  return names.map((name) => {
    const hue = assignHue(used);
    used.push(hue);
    return { name, hue };
  });
}

/** Style for an element with class "v": its person's voice (step null) or an agent's step. */
export function voiceStyle(hue: number, step: number | null = null): CSSProperties {
  return {
    "--vd": voiceCss(hue, step, "dark"),
    "--vl": voiceCss(hue, step, "light"),
  } as CSSProperties;
}

/** Sample names for the demo room. They are examples, not people. */
export const SAMPLE_NAMES = ["Ann", "Bo", "Cy", "Dee", "Eli", "Fay", "Gus", "Hana", "Ian", "Jo"]; // i18n-allow (sample names)
