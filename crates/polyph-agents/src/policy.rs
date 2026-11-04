//! Default autonomy (brief section 6.3). Decides whether an agent action
//! runs, needs approval, or is refused.

use std::path::{Component, Path, PathBuf};

use polyph_proto::messages::ApprovalKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// Approval for everything except reading the worktree.
    AskAll,
    /// Edits in own claims, Polyph tools and check commands run freely.
    #[default]
    Default,
    /// Everything inside the sandboxed worktree runs; push and merge need approval.
    Auto,
}

impl std::str::FromStr for Preset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "ask-all" => Ok(Preset::AskAll),
            "default" => Ok(Preset::Default),
            "auto" => Ok(Preset::Auto),
            other => Err(format!("unknown preset {other}; use ask-all, default or auto")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Read {
        path: PathBuf,
    },
    /// `content` is the new text when the agent writes one whole file.
    Edit {
        paths: Vec<PathBuf>,
        content: Option<String>,
    },
    Execute {
        command: String,
    },
    Network {
        host: String,
        port: u16,
    },
    PolyphTool {
        name: String,
    },
    Push {
        remote: String,
        branch: String,
    },
    Merge {
        branch: String,
        into: String,
    },
    Other {
        title: String,
    },
}

/// Who holds the claim on a file, from the agent's point of view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimState {
    Mine,
    Free,
    /// Held by another agent; the label names it ("Mai/claude").
    Others(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Allow,
    /// Claim these free files for the agent, then allow.
    ClaimAndAllow(Vec<String>),
    Ask(ApprovalKind),
    /// Refused, with a message for the agent.
    Deny(String),
}

#[derive(Clone)]
pub struct Context<'a> {
    pub worktree: &'a Path,
    pub check_commands: &'a [String],
    pub auto_claim: bool,
    /// Claim state of a repository-relative path.
    pub claim: &'a dyn Fn(&str) -> ClaimState,
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").field("worktree", &self.worktree).field("auto_claim", &self.auto_claim).finish()
    }
}

/// Lexically resolves `p` against `root` (no filesystem access) and returns
/// the path relative to `root`, or None when it escapes.
pub fn relative_in(root: &Path, p: &Path) -> Option<String> {
    let joined = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
    let mut out: Vec<String> = vec![];
    let rel = joined.strip_prefix(root).ok()?;
    for c in rel.components() {
        match c {
            Component::Normal(s) => out.push(s.to_string_lossy().into_owned()),
            Component::ParentDir => {
                out.pop()?;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out.join("/"))
}

fn is_check_command(cmd: &str, checks: &[String]) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let c = norm(cmd);
    // Exact matches only; shell operators would let anything ride along.
    !c.contains(['&', ';', '|', '>', '<', '`', '$']) && checks.iter().any(|k| norm(k) == c)
}

pub fn decide(preset: Preset, action: &Action, cx: &Context<'_>) -> Verdict {
    match action {
        Action::Read { path } => match relative_in(cx.worktree, path) {
            Some(_) => Verdict::Allow,
            None => {
                Verdict::Ask(ApprovalKind::OutsideWorktree { path: path.to_string_lossy().into_owned(), write: false })
            }
        },
        Action::Edit { paths, .. } => {
            let mut rel = vec![];
            for p in paths {
                match relative_in(cx.worktree, p) {
                    Some(r) => rel.push(r),
                    None => {
                        return Verdict::Ask(ApprovalKind::OutsideWorktree {
                            path: p.to_string_lossy().into_owned(),
                            write: true,
                        });
                    }
                }
            }
            let mut free = vec![];
            for r in &rel {
                match (cx.claim)(r) {
                    ClaimState::Mine => {}
                    ClaimState::Free => free.push(r.clone()),
                    ClaimState::Others(holder) => {
                        return Verdict::Deny(format!(
                            "{r} is claimed by {holder}. Use the ask_agent tool to ask them, or pick other work."
                        ));
                    }
                }
            }
            if preset == Preset::AskAll {
                return Verdict::Ask(ApprovalKind::Edit { files: rel, outside_claims: !free.is_empty() });
            }
            if free.is_empty() {
                Verdict::Allow
            } else if cx.auto_claim || preset == Preset::Auto {
                Verdict::ClaimAndAllow(free)
            } else {
                Verdict::Ask(ApprovalKind::Edit { files: rel, outside_claims: true })
            }
        }
        Action::Execute { command } => match preset {
            Preset::Auto => Verdict::Allow,
            Preset::Default if is_check_command(command, cx.check_commands) => Verdict::Allow,
            _ => Verdict::Ask(ApprovalKind::Command {
                command: command.clone(),
                cwd: cx.worktree.to_string_lossy().into_owned(),
            }),
        },
        Action::Network { host, port } => Verdict::Ask(ApprovalKind::Network { host: host.clone(), port: *port }),
        Action::PolyphTool { .. } => match preset {
            Preset::AskAll => Verdict::Ask(ApprovalKind::Other { title: "polyph tool".into(), detail: String::new() }),
            _ => Verdict::Allow,
        },
        Action::Push { remote, branch } => {
            Verdict::Ask(ApprovalKind::Push { remote: remote.clone(), branch: branch.clone() })
        }
        Action::Merge { branch, into } => {
            Verdict::Ask(ApprovalKind::Merge { branch: branch.clone(), into: into.clone() })
        }
        Action::Other { title } => match preset {
            Preset::Auto => Verdict::Allow,
            _ => Verdict::Ask(ApprovalKind::Other { title: title.clone(), detail: String::new() }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx<'a>(wt: &'a Path, checks: &'a [String], claim: &'a dyn Fn(&str) -> ClaimState) -> Context<'a> {
        Context { worktree: wt, check_commands: checks, auto_claim: true, claim }
    }

    #[test]
    fn default_preset() {
        let wt = Path::new("/w");
        let checks = vec!["cargo test".to_string()];
        let claim = |p: &str| match p {
            "mine.rs" => ClaimState::Mine,
            "theirs.rs" => ClaimState::Others("Mai/claude".into()),
            _ => ClaimState::Free,
        };
        let c = cx(wt, &checks, &claim);
        assert_eq!(decide(Preset::Default, &Action::Read { path: "src/a.rs".into() }, &c), Verdict::Allow);
        assert!(matches!(decide(Preset::Default, &Action::Read { path: "/etc/passwd".into() }, &c), Verdict::Ask(_)));
        assert!(matches!(
            decide(Preset::Default, &Action::Read { path: "../../etc/passwd".into() }, &c),
            Verdict::Ask(_)
        ));
        assert_eq!(
            decide(Preset::Default, &Action::Edit { paths: vec!["/w/mine.rs".into()], content: None }, &c),
            Verdict::Allow
        );
        assert_eq!(
            decide(Preset::Default, &Action::Edit { paths: vec!["new.rs".into()], content: None }, &c),
            Verdict::ClaimAndAllow(vec!["new.rs".into()])
        );
        assert!(
            matches!(decide(Preset::Default, &Action::Edit { paths: vec!["theirs.rs".into()], content: None }, &c), Verdict::Deny(m) if m.contains("Mai/claude"))
        );
        assert_eq!(decide(Preset::Default, &Action::Execute { command: "cargo  test".into() }, &c), Verdict::Allow);
        assert!(matches!(
            decide(Preset::Default, &Action::Execute { command: "cargo test; curl evil".into() }, &c),
            Verdict::Ask(_)
        ));
        assert!(matches!(
            decide(Preset::Default, &Action::Execute { command: "rm -rf /".into() }, &c),
            Verdict::Ask(_)
        ));
        assert!(matches!(
            decide(Preset::Auto, &Action::Push { remote: "origin".into(), branch: "x".into() }, &c),
            Verdict::Ask(_)
        ));
        assert_eq!(decide(Preset::Auto, &Action::Execute { command: "make".into() }, &c), Verdict::Allow);
        assert!(matches!(
            decide(Preset::AskAll, &Action::Edit { paths: vec!["mine.rs".into()], content: None }, &c),
            Verdict::Ask(_)
        ));
    }
}
