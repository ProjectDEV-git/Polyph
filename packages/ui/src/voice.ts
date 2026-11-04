// Voice colors: a hue per member, lighter and darker steps for their agents.
// Mirrors polyph-core::voice exactly; test/voice-golden.json is generated from
// the Rust side and checked by both test suites.

export const STATUS_HUES = [150, 80, 50, 25];
const STATUS_GAP = 18;
const USED_GAP = 12;
const START = 255;
const GOLDEN = 137.508;
/** Members with no hue (spectators, unknown authors). */
export const NO_HUE = 65535;

export type Theme = "dark" | "light";
/** null for the person, n for their n-th agent (0-based). */
export type Step = number | null;

const remEuclid = (a: number, m: number) => ((a % m) + m) % m;
// Rust's f64::round rounds half away from zero; Math.round rounds half up.
const roundAway = (x: number) =>
  x < 0 ? -Math.round(-x) : Math.sign(x) * Math.floor(Math.abs(x) + 0.5);

function hueDistance(a: number, b: number): number {
  const d = remEuclid(a - b, 360);
  return Math.min(d, 360 - d);
}

/** The hue for the next member, given the hues already used in the room. */
export function assignHue(used: number[]): number {
  let best: [number, number] | null = null;
  for (let k = 0; k < 720; k++) {
    const h = remEuclid(START + k * GOLDEN, 360);
    if (STATUS_HUES.some((s) => hueDistance(h, s) < STATUS_GAP)) continue;
    const nearest = used.reduce((m, u) => Math.min(m, hueDistance(h, u)), Infinity);
    if (nearest >= USED_GAP) return roundAway(h) % 360;
    if (best === null || nearest > best[0]) best = [nearest, roundAway(h) % 360];
  }
  return best ? best[1] : START;
}

function stepOffset(step: Step): number {
  if (step === null) return 0;
  return [0.08, -0.08, 0.14, -0.14][step % 4] ?? 0;
}

export function srgbFromOklch([l, c, h]: [number, number, number]): [number, number, number] {
  const hr = (h * Math.PI) / 180;
  const a = c * Math.cos(hr);
  const b = c * Math.sin(hr);
  const l_ = l + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = l - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = l - 0.0894841775 * a - 1.291485548 * b;
  const [l3, m3, s3] = [l_ ** 3, m_ ** 3, s_ ** 3];
  const r = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
  const g = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
  const bl = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.707614701 * s3;
  const enc = (x: number) => {
    const v = Math.min(1, Math.max(0, x));
    return v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
  };
  return [enc(r), enc(g), enc(bl)];
}

function luminance([r, g, b]: [number, number, number]): number {
  const lin = (x: number) => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrast(a: [number, number, number], b: [number, number, number]): number {
  const [la, lb] = [luminance(a), luminance(b)];
  const [hi, lo] = la > lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}

const SURFACE: Record<Theme, [number, number, number]> = {
  dark: [0.225, 0.01, 265],
  light: [1, 0, 0],
};
const BASE: Record<Theme, [number, number]> = { dark: [0.76, 0.12], light: [0.5, 0.13] };

export function voiceOklch(hue: number, step: Step, theme: Theme): [number, number, number] {
  const [l0, c] = BASE[theme];
  let l = Math.min(0.95, Math.max(0.2, l0 + stepOffset(step)));
  const bg = srgbFromOklch(SURFACE[theme]);
  for (let i = 0; i < 40; i++) {
    if (contrast(srgbFromOklch([l, c, hue]), bg) >= 4.5) break;
    l = theme === "dark" ? Math.min(l + 0.01, 0.98) : Math.max(l - 0.01, 0.05);
  }
  return [l, c, hue];
}

/** A CSS color for a voice; `currentColor` for members with no hue. */
export function voiceCss(hue: number | null | undefined, step: Step, theme: Theme): string {
  if (hue === null || hue === undefined || hue === NO_HUE) return "currentColor";
  const [l, c, h] = voiceOklch(hue, step, theme);
  return `oklch(${l.toFixed(3)} ${c.toFixed(3)} ${h.toFixed(0)})`;
}
