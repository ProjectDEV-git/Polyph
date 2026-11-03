# Design

Durable visual decisions for every Polyph surface. Product truth lives in
`PRODUCT.md`. Surface-specific strategy (home page narrative, app layout
concepts) lives in surface briefs written when each surface is built.

Status: foundations set in M0 from the brief. Screen-level composition is
decided per surface in M6 (TUI), M7 (app), M8 (PWA, extension) and M10
(website), each going through Impeccable `shape`, then `critique`, `audit`
and `polish`.

## World

Calm, precise, dark-first, editorial. The interface is a quiet instrument
panel around loud content: transcripts, diffs, cards and people's words carry
the color; chrome is near-neutral. The polyphony idea shows up only as
*voices*: each person has a hue, their agents are lighter and darker steps of
it, and several voices can be seen working side by side. No music notes,
staves, piano keys or waveforms, ever.

Dials (from design-taste-frontend), set by the brief:

| Surface | Variance | Motion | Density |
|---|---|---|---|
| Website (marketing) | medium-high | medium | low |
| App (desktop, browser client, PWA, extension webviews) | low | low | high |
| Docs | low | low | medium |

## Typography

All three families are under the SIL Open Font License 1.1 (commercial
embedding allowed) and are self-hosted from `packages/ui/fonts/`. No runtime
calls to font CDNs in the app.

| Role | Family | Why |
|---|---|---|
| Display serif | **Newsreader** (Production Type, OFL), display optical size | Built for editorial headlines with an optical-size axis, so large sizes tighten naturally. Reads as considered rather than decorative, and is not one of the saturated "AI landing page" serifs. |
| UI sans | **IBM Plex Sans** with **IBM Plex Sans Thai** and **IBM Plex Sans JP** | Engineered, neutral, legible at small sizes, with real tabular figures. Its Thai and Japanese siblings share metrics and stroke character, so mixed-language UI (a Thai name beside an English file path) holds one rhythm. Explicitly not Inter. |
| Monospace | **IBM Plex Mono** | Matches the sans in proportion so inline code sits on the same line height; dotted zero keeps join codes unambiguous. |

Fallback stacks (CJK and Thai always covered):

```css
--font-serif: "Newsreader", "Noto Serif Thai", "Noto Serif JP", "Noto Serif SC", "Source Han Serif SC", Georgia, serif;
--font-sans: "IBM Plex Sans", "IBM Plex Sans Thai", "IBM Plex Sans JP", "Noto Sans SC", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif;
--font-mono: "IBM Plex Mono", "Noto Sans Mono", ui-monospace, "SF Mono", "Cascadia Mono", monospace;
```

Simplified Chinese uses Noto Sans SC (OFL), self-hosted as a subset for UI
strings, with platform fonts behind it.

Rules:

- The serif appears in the app **only** for room names and empty-state
  headlines. On the website it carries headlines and a few pull quotes.
- Display tracking tightens with size: -0.02em at 32px, -0.035em at 64px and
  above; never below -0.04em. Headlines use `text-wrap: balance`.
- Real typographic quotes and apostrophes in all copy.
- Numbers that align (usage, costs, counts, timestamps) use tabular figures.
- Monospace is for code, commands, paths, join codes and aligned numbers,
  never as a "technical" costume for prose.
- No italics for CJK text (`:lang(ja) em, :lang(zh) em { font-style: normal; font-weight: 600 }`).
- Thai text gets `line-height` at least 1.7 for body and 1.35 for headings so
  stacked tone marks are not clipped; containers never clip vertically.

App type scale (px): 12, 13 (default UI), 15, 18, 24, serif 28 / 40 for room
names and empty states. Website scale: 15, 17 (body), 22, 32, 48, 72, 96 max.

## Color

All tokens are OKLCH, defined on `:root` with light and dark sets. Dark is the
default. Neutrals are tinted slightly toward blue-violet (hue 265) so dark
surfaces read as ink rather than void.

### Dark (default)

| Token | Value | Use |
|---|---|---|
| `--bg` | `oklch(0.165 0.008 265)` | App and page background |
| `--surface-1` | `oklch(0.195 0.009 265)` | Panels, rail |
| `--surface-2` | `oklch(0.225 0.010 265)` | Cards, inputs |
| `--surface-3` | `oklch(0.265 0.011 265)` | Hover, selected |
| `--line` | `oklch(0.30 0.010 265)` | Hairlines between real regions |
| `--text` | `oklch(0.93 0.006 265)` | Primary text |
| `--text-2` | `oklch(0.74 0.010 265)` | Secondary text (≥4.5:1 on surfaces) |
| `--text-3` | `oklch(0.60 0.010 265)` | Tertiary, large text only |
| `--accent` | `oklch(0.80 0.090 205)` | Focus rings, primary action, selection |

### Light

| Token | Value |
|---|---|
| `--bg` | `oklch(0.985 0.003 265)` |
| `--surface-1` | `oklch(0.965 0.004 265)` |
| `--surface-2` | `oklch(1 0 0)` |
| `--surface-3` | `oklch(0.94 0.006 265)` |
| `--line` | `oklch(0.89 0.006 265)` |
| `--text` | `oklch(0.22 0.012 265)` |
| `--text-2` | `oklch(0.45 0.012 265)` |
| `--text-3` | `oklch(0.56 0.010 265)` |
| `--accent` | `oklch(0.52 0.110 215)` |

The light theme is designed, not inverted: surfaces lift with white rather
than darken, and voices use their darker steps for text.

### Status

Always paired with an icon and a label. Hues are reserved: voice hues avoid them.

| State | Hue | Dark | Light | Icon |
|---|---|---|---|---|
| Running | 150 | `oklch(0.78 0.14 150)` | `oklch(0.52 0.13 150)` | animated dot (spinner in TUI) |
| Waiting for approval | 80 | `oklch(0.83 0.13 80)` | `oklch(0.58 0.12 75)` | hand / raised marker |
| Blocked | 50 | `oklch(0.75 0.14 50)` | `oklch(0.56 0.15 45)` | barrier |
| Error | 25 | `oklch(0.70 0.17 25)` | `oklch(0.53 0.19 25)` | octagon |
| Done | — | `--text-2` | `--text-2` | check |

### Voices

Every member gets a hue; their agents use lighter and darker steps of it.

- Hues are assigned in join order from a sequence spread evenly around the
  wheel by golden-angle stepping (137.5°), starting at 255°, skipping any hue
  within 18° of a status hue (25, 50, 80, 150) and within 12° of a hue already
  used in the room. Implemented identically in `polyph-core::voice` (Rust) and
  `packages/ui/src/voice.ts`.
- Person step: L 0.76 / C 0.12 in dark, L 0.50 / C 0.13 in light.
- Agent steps: L ±0.08 from the person step (first agent lighter, second
  darker, third lighter again at ±0.14), same hue.
- Every voice color must reach 4.5:1 against `--surface-2` when used as text;
  the generator clamps L until it does. Tested in both themes.
- Voices appear as: a 3px gutter on transcript and terminal lines, the dot in
  presence and the pill, the outline of agent-made whiteboard shapes, claim
  decorations in VS Code, and the name text itself. Never as large fills.

## Space, shape, depth

- 4px base grid. App density is high: 28px rows, 8/12px paddings, 32px controls.
- Radius: 6px controls, 8px cards and panels, 12px dialogs. Nothing pill-shaped
  except status chips and the presence dot.
- Borders only between real regions (rail, center, right panel). Cards
  separate by surface step, not outline.
- Shadows only for things that float above the page (popovers, the approval
  stack, the command palette): `0 8px 24px oklch(0 0 0 / 0.35), 0 1px 0 oklch(1 0 0 / 0.04) inset` in dark.

## Motion

Follows Emil Kowalski's rules. Motion explains a change; it never decorates.

- App: 120–180ms, `cubic-bezier(0.2, 0, 0, 1)` (ease-out) for entering,
  `cubic-bezier(0.4, 0, 1, 1)` 100ms for leaving. Only these move: a card
  changing column, a panel opening, an approval arriving in the stack, a voice
  joining (dot fades and scales from 0.6). Everything is interruptible.
- Never animate: text content of transcripts, typing, list re-sorts larger
  than a few items, anything on keyboard navigation repeat.
- Website: GSAP timelines and ScrollTrigger reveals, medium intensity. Lenis on
  marketing pages only.
- `prefers-reduced-motion`: Lenis off, all transforms replaced by 120ms
  opacity fades or nothing.
- TUI: a spinner per running agent and a 600ms highlight on a moved card; no
  other motion; redraw on change with a capped frame rate.

## Iconography and marks

- Icons: Lucide (ISC), 1.5px stroke at 16px, one weight throughout.
- Logo: three parallel strokes of slightly different length and offset that
  converge toward one baseline — independent lines moving together. Drawn as
  SVG on a 24-unit grid; monochrome template variant for the macOS menu bar
  and tray. Never paired with musical imagery.

## Copy

Direct, specific, for engineers. Name the action on controls ("Approve once",
"Deny with note"), name the problem and the fix in errors. No hype words, no
exclamation marks, no emoji. No eyebrow labels above headings.
