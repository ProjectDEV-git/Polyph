# r/LocalLLaMA draft

Draft. Posting is human-only (Kao). Check the subreddit's rules on project
posts. Before posting, someone must run the local agent against a real Ollama
model (see "Before posting" at the end); the smoke tests for Ollama exist but
have not been run (`STUBS.md`).

## Title

Polyph: share a room of coding agents between teammates, including a built-in agent for OpenAI-compatible endpoints like Ollama (beta, with limits)

## Body

Polyph is a tool for teams that run coding agents on several machines: agents
claim files before editing so they do not collide, a planner splits a goal
into cards, and everything is end-to-end encrypted. It works with Claude
Code, Codex, OpenCode and Gemini CLI, and it also ships a small agent for
local models, `polyph-localagent`.

How the local agent works:

- It is an ACP agent that talks to any OpenAI-compatible
  `/chat/completions` endpoint. The default is Ollama at
  `http://127.0.0.1:11434/v1` with the model `qwen2.5-coder:7b`.
- You configure it with environment variables: `POLYPH_LOCAL_ENDPOINT`,
  `POLYPH_LOCAL_MODEL`, `POLYPH_LOCAL_API_KEY` (optional),
  `POLYPH_LOCAL_CONTEXT` (tokens, default 32768) and `POLYPH_LOCAL_TOOLS`
  (`native` or `text`). It also reads `POLYPH_LOCAL_MAX_STEPS` (default 30).
- It uses the endpoint's native tool calls. If the endpoint rejects them, it
  switches to a text protocol, which is meant for models without tool
  support.
- Start it in a room with `polyph agent start local --task <card>`.
- Your prompts and code go from your machine to the endpoint you chose.
  Polyph does not proxy model calls and sends no telemetry. If the endpoint
  is on localhost, nothing about the model call leaves your computer. The
  room itself still goes through a relay (ours, yours, or a LAN relay with no
  internet).
- Local agents are not metered, so usage shows tokens only.

What it cannot do yet, plainly:

- It gets no Polyph MCP tools. It does not read the `mcpServers` list it is
  given, so it cannot call `ask_agent`, take notes, or ask for the next
  task. It still gets claims, because claims are taken when it writes files
  through ACP.
- It has been tested against a stand-in model server in our test suite. The
  smoke test against a real Ollama model exists and has not been run yet.
  Whether a 7B model produces good edits is your question to answer, and I
  have no benchmark to offer.
- Quality of tool use depends on the model. Small models may be better with
  `POLYPH_LOCAL_TOOLS=text` or may not work.

If you want a team of friends or a hackathon group to share one repo where
some people run Claude Code and others run local models, the room gives them
claims, a shared board and a reviewer, with each person using their own
hardware and keys. A LAN mode (`polyph host --lan`) works with no internet at
all, which fits an offline setup. Phones cannot join LAN rooms.

Install:

    curl -fsSL https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.sh | sh
    cargo install polyph-localagent   # the agent's install hint

Other beta limits: Linux x86_64 is the tested platform; macOS and Windows
are experimental; late joiners do not see earlier agent transcripts.

Beta page with every known limit: https://github.com/ProjectDEV-git/Polyph/blob/Main/docs/beta.md
Site: https://projectdev-git.github.io/Polyph
Releases: https://github.com/ProjectDEV-git/Polyph/releases

I would like to know which local models and endpoints you try it with, and
where the text-protocol fallback fails.

## Before posting

- Run the local agent against a real Ollama model, in a room, with the
  commands above, and fix any step in this draft that does not match.
- Confirm how to pass the `POLYPH_LOCAL_*` variables to an agent started by
  the daemon (the registry takes `env` per agent under `[agents.<name>]` in
  `polyph.toml`; the draft does not give a snippet because that was not
  tested), and add the exact snippet once checked.
- Confirm that `cargo install polyph-localagent` works, or whether the
  release archive already includes the binary. The hint comes from
  `crates/polyph-agents/src/registry.rs`.
