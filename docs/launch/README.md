# Launch drafts

Drafts for the Polyph public beta. Written by the launch writer; nothing here
has been posted.

## Posting is human-only

Kao posts. No agent posts from any public account, submits to Hacker News,
Reddit or X, or sends anything to press. Agents only write and revise drafts
in this folder.

## Files

| File | What it is |
|---|---|
| `demo-script.md` | 60 to 90 second demo video: shot list, voiceover, and the recording setup (needs a human) |
| `show-hn.md` | Show HN title, text and a first comment |
| `x-thread.md` | Thread for X |
| `reddit-programming.md` | Post for r/programming |
| `reddit-rust.md` | Post for r/rust |
| `reddit-localllama.md` | Post for r/LocalLLaMA (bring your own model) |
| `positioning.md` | Polyph against Amoeba and Murmell, with sources and dates, plus beta limits |
| `blurb.md` | The canonical short Polyph description, for listings, posts and press |
| `website-copy.md` | Proposed changes to `apps/web/src/site` and `locales/en/site.ftl` for the beta |

## Rules every draft follows

- Voice from `PRODUCT.md`: direct, specific, written for engineers. No hype
  words, no exclamation marks, no emoji.
- No customers, testimonials, benchmarks or press, because there are none.
  The only number quoted is the relay memory figure measured in
  `docs/self-hosting.md` (71 MB peak for 50 rooms of 5 people at 2 messages
  per second per member; 118 MB in the worst-case run).
- Every post links the beta page
  (https://github.com/ProjectDEV-git/Polyph/blob/Main/docs/beta.md), the site
  (https://projectdev-git.github.io/Polyph) and the releases page
  (https://github.com/ProjectDEV-git/Polyph/releases), and states the beta
  limits that matter to that audience.
- Nothing in `STUBS.md` is described as done.
- Competitor statements are limited to what their public pages say, with the
  date checked (see `positioning.md`).

## Before anyone posts

1. Re-read each draft against `STUBS.md` and `docs/beta.md`; both change.
2. Check that the release tag named in the post exists and that the install
   line works on a clean machine.
3. Replace the video placeholder once the demo is recorded.
4. Re-check every claim in `positioning.md` on the day of posting.
5. Be present for the first hours after posting to answer questions. Answers
   follow the same rules: no invented numbers, no promises about dates.
