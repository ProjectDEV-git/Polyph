// Sentences for approvals, from the same strings as the TUI. No React: the
// VS Code extension host and phone notifications use it too.
import type { Approval } from "./api/types";
import type { Localizer } from "./i18n/localizer";

function shortPath(p: string): string {
  const parts = p.split("/").filter(Boolean);
  return parts.length > 3 ? `…/${parts.slice(-3).join("/")}` : p;
}

/** The sentence for an approval, from the same strings as the TUI. */
export function describeApproval(l: Localizer, a: Approval): string {
  const c = (a.kind.c ?? {}) as Record<string, any>;
  const agent = a.agent.label;
  switch (a.kind.t) {
    case "command":
      return l.t("tui-approve-command", {
        agent,
        command: c.command,
        cwd: shortPath(String(c.cwd ?? "")),
      });
    case "host_command":
      return l.t("tui-approve-command", { agent, command: c.command, cwd: "" });
    case "edit":
      return l.t("tui-approve-edit", { agent, files: (c.files ?? []).join(", ") });
    case "network":
      return l.t("tui-approve-network", { agent, host: c.host, port: c.port });
    case "outside_worktree":
      return l.t("tui-approve-outside", { agent, path: c.path });
    case "push":
      return l.t("tui-approve-push", { agent, remote: c.remote, branch: c.branch });
    case "merge":
      return l.t("tui-approve-merge", { agent, branch: c.branch, into: c.into });
    case "terminal_input":
      return l.t("tui-approve-terminal", { agent });
    case "steer":
      return l.t("tui-approve-steer", { agent, text: c.text });
    default:
      return l.t("tui-approve-other", { agent, title: c.title ?? "", detail: c.detail ?? "" });
  }
}
