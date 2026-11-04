# Extending Polyph: templates, skills and plugins

## Templates

A template is a starting `polyph.toml` and skills for a repository.

```bash
polyph template list
polyph template apply startup      # in the repository; existing files are kept
```

Built-in templates: `startup`, `hackathon`, `open-source`, `agency`. Your own
are folders in `<polyph home>/templates/<name>/`, copied as they are; a
`TEMPLATE.md` in one gives its description (and is not copied).

## Skills

A skill is a folder with a `SKILL.md`: how this team does one kind of task.

```text
---
name: release
description: How we cut a release: version bump, changelog, tag.
---
1. …
```

Project skills live in `.polyph/skills/<name>/` and travel with the
repository, so every member's agents read the same ones. Personal skills live
in `<polyph home>/skills/<name>/`. A project skill wins over a personal one of
the same name. Agents find them with the Polyph MCP tools `list_skills` and
`read_skill`; people with `polyph skills list|show|add`.

## Plugins

A plugin is a folder with a `polyph-plugin.toml`. It can give agents tools
(an MCP stdio server) and run hooks on room events.

```toml
name = "release-notes"            # lower-case letters, digits, dashes
version = "0.1.0"
description = "Drafts release notes from merged cards."

[mcp]
command = "node"
args = ["server.js"]              # paths are relative to the plugin folder

[[hooks]]
on = ["merge_queue"]              # chat, approval_pending, approval_resolved,
command = "sh"                    # merge_queue, member_joined, member_removed,
args = ["hook.sh"]                # knock, claim_revoked, doc_changed
timeout_secs = 10                 # at most 60

[permissions]
hosts = ["api.example.com"]       # the only network the plugin may reach
```

```bash
polyph plugin add ./release-notes
polyph plugin list
polyph plugin remove release-notes
```

How they run:

- Plugins are installed per person, per device, in `<polyph home>/plugins/`.
  A repository cannot install one on a teammate's machine.
- **Tools**: each agent started after installing gets the plugin's MCP
  server in its session (named `plugin-<name>`). The agent starts it inside
  its own sandbox, from a copy of the plugin in the agent's private
  temporary directory (`POLYPH_PLUGIN_DIR`), and the plugin's hosts join that
  agent's network allowlist.
- **Hooks** run on the device that installed the plugin, in a fresh sandbox:
  a scratch copy of the plugin as the working directory, the event as JSON
  on stdin (`{"room", "event", "plugin"}`), only the manifest's hosts on the
  network, and the time limit. A hook can post to the room chat by printing
  a line `{"chat": "text"}` (at most five per run); the post is labeled
  `plugin/<name>`, and hooks never run on plugin posts.
- Nothing a plugin does bypasses claims, approvals or redaction: its tools
  act through the agent that calls them.
