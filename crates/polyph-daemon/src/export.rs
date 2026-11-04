//! Export of a room's board and threads to Markdown, offered before a quick
//! session is deleted. Also one-line titles for approvals in status feeds.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use polyph_core::RoomCore;
use polyph_proto::crdt::Board;
use polyph_proto::messages::{ApprovalKind, DocName};

/// Writes `<dir>/.polyph/export/<YYYY-MM-DD>.md` and returns its path.
pub fn write(core: &mut RoomCore, dir: &Path, now: i64) -> anyhow::Result<PathBuf> {
    let date = time::OffsetDateTime::from_unix_timestamp(now)
        .map(|t| format!("{:04}-{:02}-{:02}", t.year(), u8::from(t.month()), t.day()))
        .unwrap_or_else(|_| "export".into());
    let out_dir = dir.join(".polyph").join("export");
    std::fs::create_dir_all(&out_dir)?;
    let mut path = out_dir.join(format!("{date}.md"));
    let mut n = 2;
    while path.exists() {
        path = out_dir.join(format!("{date}-{n}.md"));
        n += 1;
    }
    std::fs::write(&path, render(core))?;
    Ok(path)
}

pub fn render(core: &mut RoomCore) -> String {
    let names = crate::daemon::member_names(core);
    let mut s = String::new();
    let title = if core.name.is_empty() { "Quick session".to_string() } else { core.name.clone() };
    let _ = writeln!(s, "# {title}\n");
    let board = {
        let doc = core.doc(&DocName::Board);
        let b = Board::new(doc);
        (b.columns(), b.all_cards())
    };
    let _ = writeln!(s, "## Board\n");
    for (id, col_title) in &board.0 {
        let cards: Vec<_> = board.1.iter().filter(|c| &c.column == id).collect();
        if cards.is_empty() {
            continue;
        }
        let _ = writeln!(s, "### {col_title}\n");
        for c in cards {
            let _ = writeln!(s, "- **{}**", c.title);
            if !c.description.is_empty() {
                let _ = writeln!(s, "  {}", c.description.replace('\n', "\n  "));
            }
            for a in &c.acceptance {
                let _ = writeln!(s, "  - [ ] {a}");
            }
        }
        let _ = writeln!(s);
    }
    let chat = core.chat();
    if !chat.is_empty() {
        let _ = writeln!(s, "## Chat\n");
        for e in chat {
            let who = e
                .agent
                .clone()
                .unwrap_or_else(|| names.get(&e.author).cloned().unwrap_or_else(|| e.author.chars().take(8).collect()));
            let _ = writeln!(s, "- {who}: {}", e.text.replace('\n', "\n  "));
        }
    }
    s
}

/// A short, content-free title for status feeds (pill, Nublet).
pub fn approval_title(k: &ApprovalKind) -> String {
    polyph_core::room::approval_title(k)
}
