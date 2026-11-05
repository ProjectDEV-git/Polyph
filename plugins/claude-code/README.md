# Polyph for Claude Code

This plugin puts Claude Code in your Polyph room:

- **Polyph's tools** (`polyph mcp --client claude-code`): claim files, see the
  team, message other agents, read and pin notes, `get_context` for the
  project brief and your task, `search`, `recent_work` and `next_task`.
- **A hook before every edit** (`Edit`, `MultiEdit`, `Write`, `NotebookEdit`):
  when another agent in the room holds the file, the edit is stopped and
  Claude is told who holds it and why, so it can ask them with `ask_agent`.
  Free files are claimed for this session, so other agents are stopped in
  turn.

## Install

You need `polyph` on your `PATH` (see the main README), and the folder you
work in linked to a room (`polyph start` there, or `polyph join <code>`).

In Claude Code:

```
/plugin marketplace add ProjectDEV-git/Polyph
/plugin install polyph@polyph
```

The plugin brings Polyph's MCP server with it, so you do not also need
`polyph connect claude-code` (which skips Claude Code when the plugin is
enabled). If you connected it before installing the plugin, run
`polyph disconnect claude-code` to avoid having Polyph twice.

## How the hook decides

The hook asks your daemon (`polyph hook pre-edit`, daemon method
`claims.check`). It blocks only when another agent holds the file or a
directory containing it. Claims held by your own Claude Code sessions in
the same folder never block. When Polyph cannot answer (the daemon is not
running, or the folder is not linked to a room), the edit goes ahead.
