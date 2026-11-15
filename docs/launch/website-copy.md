# Website copy proposals for the beta

Draft. These are proposed changes; nothing here has been applied to
`apps/web/src/site` or `locales/en/site.ftl`, with two exceptions that landed
with the core-features rewrite: `site-fact-platforms` now states the beta's
platforms honestly, and the `site-part-*` close-ups became the six
core-features pillars (new keys `site-part-{editing,safety,server,router,
models,context}` and `site-fig-{tiers,models}`; the proposals below that name
the old keys are superseded for those two areas). A developer applies the
rest, adds the same keys to the Thai, Japanese and Simplified Chinese locale
files (every key needs all four; translations need a human reviewer), and
runs the site's tests.

Links used:

- Beta page: https://github.com/ProjectDEV-git/Polyph/blob/Main/docs/beta.md
- Releases: https://github.com/ProjectDEV-git/Polyph/releases
- Site: https://projectdev-git.github.io/Polyph

## Beta label

Add a quiet label near the logo in the header (`Shell.tsx`) and above the
hero title (`Home.tsx`). It needs an icon or text, not color alone, and no
hype styling.

```
site-beta-label = Public beta
site-beta-note = Free while in beta. Some things do not work yet.
site-beta-link = Known limits
```

`site-beta-link` points to the beta page. Change `site-title` or
`site-description` only if the owner wants "beta" in the page title:

```
site-title = Polyph (public beta): your team's coding agents in one room
```

## Home hero

Keep `site-hero-title` and `site-hero-sub`. Add one line under the facts:

```
site-hero-beta = Public beta. Linux is the tested platform; macOS and Windows are experimental. Read the known limits before you rely on it.
```

Change `site-fact-platforms`, which currently says "Linux, macOS and Windows,
including Bazzite and Fedora Atomic" and so claims more than the beta
supports:

```
site-fact-platforms = Linux (including Bazzite and Fedora Atomic); macOS and Windows are experimental
```

The rail line `site-rail-action = Download Polyph` can stay.

## Download page

`site-dl-sub` currently says one command installs the CLI, daemon and relay.
Keep it. Add a platform table section (or list) after the install blocks.

```
site-dl-platforms-title = Platforms in the beta
site-dl-platform-linux = Linux x86_64, including Bazzite and Fedora Atomic: supported. Linux arm64: command line, .deb and .rpm.
site-dl-platform-macos = macOS, Intel and Apple silicon: experimental. Built and smoke-tested in CI only. The app is not signed; open it from the context menu.
site-dl-platform-windows = Windows x64: experimental. Built and smoke-tested in CI only. Not signed, so SmartScreen warns on first run.
site-dl-platform-none = Alpine (musl) and Windows on ARM: no prebuilt downloads. Build from source.
site-dl-limits = Read the known limits
```

Download links. The page already links all downloads to the releases page via
`site-dl-releases`. Proposed text and per-file links, once the release tag is
final, all under https://github.com/ProjectDEV-git/Polyph/releases:

```
site-dl-releases = All downloads on GitHub (releases page)
site-dl-app-linux = Desktop app for Linux: .AppImage, .deb, .rpm
site-dl-app-macos = Desktop app for macOS: .dmg (experimental)
site-dl-app-windows = Desktop app for Windows: .msi or .exe (experimental)
site-dl-vsix = VS Code extension: .vsix (not in the Marketplace yet)
```

Do not hard-code file names. They contain the version, so link to the
releases page unless the build can read the latest tag. Each archive is
checked against the release's `SHA256SUMS` by the install scripts; say so
in `site-dl-sub` only if it is still true on release day.

`site-dl-apps` currently lists `.dmg, .msi, .deb, .rpm, .AppImage` as one
group. Replace with:

```
site-dl-apps = The desktop app carries its own polyph. Linux has .AppImage, .deb and .rpm (arm64: .deb and .rpm). macOS (.dmg) and Windows (.msi, .exe) are experimental. The VS Code extension is a .vsix on the releases page and is not in the Marketplace yet.
```

`site-dl-immutable` mentions Bazzite and Fedora Atomic. Keep it. The hand
install of the AppImage and `.rpm` on those systems has not been recorded
(`STUBS.md`, Milestone 11 gaps), so do not add "tested on Bazzite" unless
that has been done and the entry has been removed from `STUBS.md`.

## Known limits link

Add a "Known limits" link to the footer (`site-nav-*` group) and to the
download page, pointing at the beta page:

```
site-nav-limits = Known limits
```

The beta page lives in the repository. If it should be a site page later,
that is a separate piece of work. Do not paste the limits into the site by
hand; they change.

## Pricing page

The pricing page says "Pricing coming soon" and lists plan features. During
the beta, billing is off and plan limits are not enforced. Add one line
under `site-pricing-sub`:

```
site-pricing-beta = During the beta everything is free and plan limits are not enforced. Prices are not set.
```

The Free plan line `site-plan-free-3 = { $links } spectator link per room`
describes something that does not exist in the beta (spectator links are not
built; `STUBS.md`, `docs/beta.md`). Remove it or change it to a statement
that spectator links are planned. Suggested replacement for the plan list
item: delete the line and renumber.

## FAQ

`site-faq-a2` says an account adds longer room storage and team billing.
Add that accounts are not part of the beta:

```
site-faq-a2 = No. Polyph works without one, on the project's relay, your own relay, or a LAN with no internet. Accounts and billing are not part of the beta.
```

Add one question:

```
site-faq-q6 = What does not work yet?
site-faq-a6 = Spectator links, push notifications to a closed phone app, accounts and billing are not in the beta. macOS and Windows are experimental. The known limits page lists the rest.
```

`site-faq-a3` lists agents. Add the local agent limit:

```
site-faq-a3 = Claude Code, Codex (through the codex-acp adapter), OpenCode, Gemini CLI, any agent that speaks ACP, and local models through an OpenAI-compatible endpoint. The built-in local agent does not get Polyph's tools yet. You bring your own subscriptions and keys; Polyph never proxies model calls.
```

## Security page

Add the sentence from the beta page that the beta relay runs on Fly.io,
which also sees connection metadata, and that GitHub Pages serves the site
and sees page visits:

```
site-sec-hosting = The beta relay runs on Fly.io, which also sees connection metadata. This site is served by GitHub Pages, which sees page visits. Polyph sends no telemetry.
```

## Footer

`site-footer-legal = © 2026 Polyph` can stay. The privacy and terms pages
stay marked as drafts; do not change their text (legal text is out of scope
for the writer and needs Kao).

## Screenshots needed

`STUBS.md` ("Website pictures") says the home page pictures are interface
fragments built in code (`fragments.tsx`, strings under "Interface fragments"
in `site.ftl`), and that no screenshots of the real desktop app exist yet.
Capturing them is pending and needs a human with the packaged app.

Needed to replace the fragments, each in dark and light themes, at 2x:

1. Room overview in the desktop app with three people and their agents
   visible, claims shown on files.
2. An approval card with command, files and hosts.
3. The board with cards in To do, Doing and Review.
4. The merge view showing a conflict resolved by keeping both tasks' intent
   (needs the demo repository from `demo-script.md`).
5. The status pill with the first approval answerable in place.
6. The phone app showing an approval.
7. The terminal UI.
8. The VS Code sidebar with claims on open files.

Capture rules: real data from a real room, no private paths or tokens, no
retouching of content, English locale first. Until they exist, keep the
fragments and do not caption them as screenshots. If the alt text says
"screenshot", change it.
