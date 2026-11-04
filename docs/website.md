# The website

The surface brief for the public website (`apps/web`, milestone 10), written
the way `docs/tui.md` and `docs/app.md` were. Product truth stays in
`PRODUCT.md`; visual foundations in `DESIGN.md`.

## What it must do

A visitor learns in one screen what Polyph is (several people's coding
agents in one end-to-end encrypted room) and can download it. Further down:
how work flows, the parts, every surface, plans, questions. Inner pages:
pricing, download (build from source until M11 ships installers), security
(what the relay can see), privacy and terms (drafts, marked as such), and the
account page.

## Direction

Art direction came from the design-taste-frontend skill: direction
**"The well-kept board"** (`dense-tiles`, seed 406490173), sector dev-tool,
with palette and type kept from `DESIGN.md`.

- **Idea:** the page is a board of tiles, like the room's own board, and every
  tile that shows work is signed by a *voice*: a person's hue, or a lighter or
  darker step of it for their agents. The voices are computed with the app's
  own code (`packages/ui/src/voice.ts`).
- **The live thing:** "Set up a room" on the home page. Pick the number of
  people and agents, turn claims, approvals, hosted mode and the light theme
  on or off, and the room fragment beside it changes at once, with voices
  assigned exactly as a real room assigns them.
- **Pictures** are Polyph's own interface rebuilt in HTML at real size
  (`apps/web/src/site/fragments.tsx`, each root marked `data-ui`): the
  transcript, the status pill, an approval stating the exact command, files
  and hosts, a join knock, the board with a claim, a merge that kept both
  intents, and real CLI output from `locales/*/account.ftl`. No stock
  photographs; the room, names (Ann, Bo, Cy…), files and join code
  `A1B2-CDEF` are a sample.
- **Motion:** the marquee ("Independent voices, one room"), staggered
  entrances for each section's parts, a slow drift on the hero room, Lenis
  smooth scrolling, and cards that fold into spines as you scroll through the
  surfaces. GSAP, ScrollTrigger and Lenis load only on site pages. Under
  `prefers-reduced-motion` Lenis and the marquee stay off and entrances become
  short fades; content is visible without script.

Where the dealt sheet and `DESIGN.md` disagreed, `DESIGN.md` won:

- Voices are never large fills, so tiles use the surface steps and the accent;
  voices appear as gutters, dots, card edges and names.
- Three faces (Newsreader, IBM Plex Sans, IBM Plex Mono), with mono only in
  real code (`<pre>`, `<code>`).
- The name band is the wordmark; the `h1` is the statement under the room
  ("Your team's coding agents, in one room."), at statement size.

## Structure

- English at `/`, the other languages under `/th/`, `/ja/`, `/zh-Hans/`
  (`app/[lang]/…`). Every word comes from `locales/*/site.ftl`.
- `apps/web/src/site/`: `Shell` (name band, links, footer of tiles), `Home`,
  `Pages` (pricing, download, security, legal, account shell), `LiveRoom`,
  `Spines`, `PlanPrice`, `Account` and `account-api.ts`, `Motion`,
  `ThemeToggle`, `fragments`, `voices`, `site.css`.
- Prices come from the account service (`GET /v1/plans`); until a price is
  set there the plan says "Pricing coming soon". Plan limits shown are the
  ones the brief fixes (7 days Free, 1 year Pro and Team, one spectator link
  per room on Free).
- The account service URL is `NEXT_PUBLIC_POLYPH_CLOUD` at build time
  (default `https://api.polyph.dev`, a placeholder like the relay's domain).
- `apps/web/vercel.json` sends the site's CSP (only the account service may be
  called) and the phone app's own CSP for `/phone/`.
- A person's own daemon serves the same export for the browser client; its
  `/` redirects to `/app/`.

## Checks and the last critique

`inspect.mjs` (1440, 768, 390 px) on the production build: **0 FAIL** on the
home page in English, Thai and Japanese, and on pricing, download and
security. Warnings kept, with reasons:

- *three typefaces* — fixed by `DESIGN.md`;
- *no photographs or footage* — the subject is software and its pictures are
  its interface, per the direction sheet;
- *font sizes* — the extra small sizes are inside interface fragments, which
  keep the app's own scale;
- *small h1* — the large type on the first screen is the wordmark;
- *bare-name title* in Thai, Japanese and Chinese — the heuristic counts words
  by spaces; those titles are full sentences;
- *long pin* on the surfaces deck — two screens of scroll for seven cards.

Privacy and terms fail only the picture floor: they are legal drafts and stay
text.

Critique of the last round (`references/critique.md` shape, condensed):
first screen says what this is and offers Download within five seconds; the
board composition holds at all three widths and reorders on phones (title
first); the live room carries the idea. What still falls short: the "how it
works" fragments sit small inside tall tiles at 1440 px; the close-up tiles
crop the approval and claim harder than ideal on tablets; there is no real
photograph of the subject's world. A designer pass with real screenshots of
the desktop app (after M11 packaging) would lift the middle of the page.
