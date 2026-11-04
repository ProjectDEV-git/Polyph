import golden from "./voice-golden.json";
import { assignHue, voiceCss } from "../src/voice";

describe("voice colors match polyph-core::voice", () => {
  it("assigns the same hues in join order", () => {
    const used: number[] = [];
    for (const want of golden.hues) {
      const h = assignHue(used);
      expect(h).toBe(want);
      used.push(h);
    }
  });

  it("produces the same CSS for every hue, step and theme", () => {
    for (const [hue, step, theme, css] of golden.colors as [
      number,
      number | null,
      "dark" | "light",
      string,
    ][]) {
      expect(voiceCss(hue, step, theme)).toBe(css);
    }
  });
});
