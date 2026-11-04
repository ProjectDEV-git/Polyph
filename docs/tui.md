# Terminal UI brief

`polyph` with no arguments opens the terminal UI: the whole room in one
screen, for people who live in a terminal, work over SSH, or keep Polyph in a
split beside their editor. It is a client of the local daemon like every other
surface (PRODUCT.md principle 4) and shows the same state.

## Who and when

An engineer running agents in a room for hours, glancing at it between edits.
They need, at a glance: which voices are working, what needs them (an
approval), what moved on the board, and what was said. Then they act with a
few keys and go back to their editor. Dense and quiet beats spacious.

## Layout

```
 payments                         ● connected  3 online  ! 1 approval     ← top bar
 ROOMS          │ Chat  Board  Agents  Activity  Merges  Terminals         ← view tabs
 › payments     │                                                         
   demo         │ 10:41  Ann    shall we split billing?                   
 PEOPLE         │ 10:42  Bo     yes, I take invoices                      
 ● Ann (you)    │ 10:42  Bo/claude  claimed src/invoice.rs                
 ● Bo           │                                                         
 AGENTS         │                                                         
 ⠹ Ann/claude   │                                                         
   running      │                                                         
 ! Bo/codex     │                                                         
   waiting      │                                                         
├───────────────┴──────────────────────────────────────────────────────── 
│ ! Ann/claude wants to run: cargo publish                                ← approvals stack
│   a approve once   t approve for task   d deny   n next                 
 > message the room                                       ? keys          ← input
```

- **Rail** (left, 24 columns): rooms, people, agents. Hidden below 80
  columns (`r` shows it).
- **Center**: one view at a time, chosen by `1`–`6` or Tab: Chat (with
  threads), Board, Agents (follow a transcript), Activity (with replay),
  Merges, Terminals (attach to a shared terminal).
- **Approvals stack**: appears above the input only when something waits for
  this person, and states exactly what will happen (the command, the files,
  the host), per principle 2. It never covers the work.
- **Input**: chat by default; prompts an agent in the Agents view; `:` opens
  the command line (`:join CODE`, `:thread NAME`, `:card TITLE`, `:plan
  GOAL`, `:term`, `:quit`).

## Color and type

- Tokens from DESIGN.md converted from OKLCH to the terminal: true color when
  `COLORTERM` says so, the nearest of 256 colors otherwise, none with
  `NO_COLOR` or `TERM=dumb`. Dark by default; light when the person's theme is
  light.
- Chrome is near-neutral (`--text-2` labels, `--line` rules); voices color
  names, agent rows and the transcript gutter, never fills.
- Status is never color alone: each state has a symbol and a word.
  Running `⠋` spinner, waiting `!`, blocked `■`, error `✕`, done `✓`,
  idle `○`, stopped `·`.
- Without color: selection is reverse video, headings bold; everything else
  still reads, because symbols and labels carry the meaning.
- Widths come from `unicode-width`, so Thai combining marks and CJK double
  widths line up; text is never cut inside a grapheme.

## Motion

DESIGN.md: a spinner per running agent, a 600 ms highlight on a card that
moved, nothing else. Redraws happen on change, capped at 30 per second; the
spinner ticks every 100 ms only while an agent runs.

## Keys

| Key | Action |
|---|---|
| `1`–`6`, Tab, Shift-Tab | switch view |
| `j`/`k` or arrows | move selection |
| `h`/`l` | Board: column; Activity: replay step |
| `H`/`L` | Board: move the selected card |
| Enter | open (thread, agent transcript, terminal attach) |
| `i` | focus the input |
| Esc | leave input or detach, close overlays |
| `a` / `t` / `d` / `n` | approve once / approve for the task / deny / next approval |
| `[` / `]` | room before / after |
| `r` | show or hide the rail |
| `s` | Agents: stop the selected agent (yours) |
| `x` | Terminals: close the selected terminal (yours) |
| `T` | Chat: show threads |
| `?` | keys |
| `q`, Ctrl-C | quit (the daemon keeps running) |

Terminal attach forwards keystrokes when you own the terminal or hold its
keyboard; `Ctrl-]` detaches; `a` asks the owner for the keyboard.

## Copy

Every visible string comes from `locales/*/tui.ftl`. Labels name the action
("approve once", "deny"), empty states say what to do next ("No cards yet.
Add one with :card TITLE").
