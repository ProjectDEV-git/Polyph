//! Drawing. Chrome stays near-neutral; voices color names and agent rows;
//! states always show a symbol and a word (docs/tui.md).

use polyph_i18n::{FluentValue, Localizer};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};
use serde_json::Value;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::model::{App, Focus, View, s};
use crate::theme::Status;

/// Narrower than this, the rail hides unless asked for.
pub const RAIL_MIN_WIDTH: u16 = 80;
const RAIL: u16 = 24;

/// Cuts `text` to `w` cells, ending in `…` when cut. Never splits a char,
/// and keeps zero-width marks (Thai vowels and tones) with their base.
pub fn fit(text: &str, w: usize) -> String {
    if text.width() <= w {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let cw = c.width().unwrap_or(0);
        if used + cw + 1 > w {
            break;
        }
        used += cw;
        out.push(c);
    }
    // Zero-width marks right after the cut belong to the last character.
    out.extend(text.chars().skip(out.chars().count()).take_while(|c| c.width() == Some(0)));
    out.push('…');
    out
}

/// Wraps `text` to lines of at most `w` cells, breaking at spaces when it can.
pub fn wrap(text: &str, w: usize) -> Vec<String> {
    let w = w.max(4);
    let mut lines = vec![];
    for para in text.split('\n') {
        let mut cur = String::new();
        let mut cur_w = 0;
        for word in para.split_inclusive(' ') {
            let ww = word.width();
            // A trailing space may hang past the edge; the word itself must fit.
            if cur_w + word.trim_end().width() <= w {
                cur.push_str(word);
                cur_w += ww;
                continue;
            }
            if !cur.is_empty() {
                lines.push(cur.trim_end().to_string());
                cur = String::new();
                cur_w = 0;
            }
            // A word longer than the line: split by width (CJK has no spaces).
            for c in word.chars() {
                let cw = c.width().unwrap_or(0);
                if cur_w + cw > w && cw > 0 {
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0;
                }
                cur.push(c);
                cur_w += cw;
            }
        }
        lines.push(cur.trim_end().to_string());
    }
    lines
}

pub fn t_args(l: &Localizer, key: &str, args: &Value) -> String {
    let owned: Vec<(String, FluentValue<'static>)> = args
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| {
                    let fv: FluentValue<'static> = match v {
                        Value::Number(n) => FluentValue::from(n.as_f64().unwrap_or(0.0)),
                        Value::String(x) => FluentValue::from(x.clone()),
                        Value::Bool(b) => FluentValue::from(b.to_string()),
                        Value::Null => FluentValue::from(""),
                        other => FluentValue::from(other.to_string()),
                    };
                    (k.clone(), fv)
                })
                .collect()
        })
        .unwrap_or_default();
    let refs: Vec<(&str, FluentValue<'_>)> = owned.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    l.t_args(key, &refs)
}

fn tr(l: &Localizer, key: &str, args: Value) -> String {
    t_args(l, key, &args)
}

fn clock(app: &App, at: i64) -> String {
    let t = (at + app.utc_offset).rem_euclid(86_400);
    format!("{:02}:{:02}", t / 3600, (t % 3600) / 60)
}

fn ago(app: &App, at: i64) -> String {
    let secs = (polyph_core_now() - at).max(0);
    let l = &app.l;
    if secs < 60 {
        l.t("tui-ago-now")
    } else if secs < 3600 {
        tr(l, "tui-ago-minutes", serde_json::json!({ "n": secs / 60 }))
    } else if secs < 86_400 {
        tr(l, "tui-ago-hours", serde_json::json!({ "n": secs / 3600 }))
    } else {
        tr(l, "tui-ago-days", serde_json::json!({ "n": secs / 86_400 }))
    }
}

fn polyph_core_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The last two parts of a path (`…/payments/api`), so the command keeps the room.
fn short_path(p: &str) -> String {
    let parts: Vec<&str> = p.trim_end_matches('/').rsplit('/').take(3).collect();
    if parts.len() < 3 {
        return p.to_string();
    }
    format!("…/{}/{}", parts[1], parts[0])
}

/// What an approval will do, exactly (PRODUCT.md principle 2).
pub fn describe_approval(l: &Localizer, a: &Value) -> String {
    let agent = a["agent"]["label"].as_str().unwrap_or_default();
    let k = &a["kind"];
    let c = &k["c"];
    let args = |extra: Value| {
        let mut v = extra;
        v["agent"] = Value::from(agent);
        v
    };
    match k["t"].as_str().unwrap_or_default() {
        "command" => tr(
            l,
            "tui-approve-command",
            args(serde_json::json!({ "command": c["command"], "cwd": short_path(s(c, "cwd")) })),
        ),
        "edit" => {
            let files: Vec<&str> = c["files"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).collect();
            tr(l, "tui-approve-edit", args(serde_json::json!({ "files": files.join(", ") })))
        }
        "network" => tr(l, "tui-approve-network", args(serde_json::json!({ "host": c["host"], "port": c["port"] }))),
        "outside_worktree" => tr(l, "tui-approve-outside", args(serde_json::json!({ "path": c["path"] }))),
        "push" => tr(l, "tui-approve-push", args(serde_json::json!({ "remote": c["remote"], "branch": c["branch"] }))),
        "merge" => tr(l, "tui-approve-merge", args(serde_json::json!({ "branch": c["branch"], "into": c["into"] }))),
        "terminal_input" => tr(l, "tui-approve-terminal", args(serde_json::json!({}))),
        "host_command" => tr(l, "tui-approve-command", args(serde_json::json!({ "command": c["command"], "cwd": "" }))),
        "steer" => tr(l, "tui-approve-steer", args(serde_json::json!({ "text": c["text"] }))),
        _ => tr(l, "tui-approve-other", args(serde_json::json!({ "title": c["title"], "detail": c["detail"] }))),
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::default().style(app.theme.bg()), area);
    let mine = app.my_approvals();
    let stack_h =
        if mine.is_empty() || matches!(app.focus, Focus::Attached(_)) { 0 } else { (mine.len().min(3) as u16) + 2 };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(stack_h),
        Constraint::Length(1),
    ])
    .split(area);
    top_bar(f, app, rows[0]);
    tabs(f, app, rows[1]);
    let show_rail = if area.width >= RAIL_MIN_WIDTH { app.rail } else { app.rail_narrow && area.width >= 50 };
    let body = if show_rail {
        let cols = Layout::horizontal([Constraint::Length(RAIL), Constraint::Min(20)]).split(rows[2]);
        rail(f, app, cols[0]);
        cols[1]
    } else {
        rows[2]
    };
    match app.view {
        View::Chat => chat(f, app, body),
        View::Board => board(f, app, body),
        View::Agents => agents(f, app, body),
        View::Activity => activity(f, app, body),
        View::Merges => merges(f, app, body),
        View::Terminals => terminals(f, app, body),
    }
    if stack_h > 0 {
        approvals(f, app, rows[3], &mine);
    }
    input(f, app, rows[4]);
    if app.help {
        help(f, app, area);
    }
}

fn top_bar(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let mut left = vec![Span::raw(" ")];
    match app.room_info() {
        Some(room) => {
            let name = if s(room, "name").is_empty() { l.t("tui-quick-session") } else { s(room, "name").to_string() };
            left.push(Span::styled(name, th.text().add_modifier(Modifier::BOLD)));
        }
        None => left.push(Span::styled(l.t("tui-no-room"), th.text2())),
    }
    let mut right: Vec<Span> = vec![];
    if let Some(room) = app.room_info() {
        let (st, key) = if room["connected"] == true {
            (Status::Running, "tui-connected")
        } else {
            (Status::Blocked, "tui-offline")
        };
        right.push(Span::styled(if st == Status::Running { "● " } else { "■ " }, th.status(st)));
        right.push(Span::styled(l.t(key), th.text2()));
        right.push(Span::styled(
            format!("  {}", tr(l, "tui-online", serde_json::json!({ "n": room["members_online"] }))),
            th.text2(),
        ));
    }
    let n = app.my_approvals().len();
    if n > 0 {
        right.push(Span::styled("  ! ", th.status(Status::Waiting)));
        right.push(Span::styled(tr(l, "tui-approvals", serde_json::json!({ "n": n })), th.status(Status::Waiting)));
    }
    right.push(Span::raw(" "));
    let rw: usize = right.iter().map(|s| s.content.width()).sum();
    let lw = (r.width as usize).saturating_sub(rw);
    let left_line = Line::from(left);
    let left_text: String = left_line.spans.iter().map(|s| s.content.to_string()).collect();
    let left_fit = fit(&left_text, lw);
    let mut spans = vec![Span::styled(left_fit.clone(), th.text().add_modifier(Modifier::BOLD))];
    spans.push(Span::raw(" ".repeat(lw.saturating_sub(left_fit.width()))));
    spans.extend(right);
    f.render_widget(Paragraph::new(Line::from(spans)), r);
}

fn tabs(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let mut spans = vec![Span::raw(" ")];
    for (i, v) in View::ALL.iter().enumerate() {
        let label = format!("{} {}", i + 1, app.l.t(v.key()));
        let st =
            if *v == app.view { th.accent().add_modifier(Modifier::BOLD | Modifier::UNDERLINED) } else { th.text2() };
        spans.push(Span::styled(label, st));
        spans.push(Span::raw("  "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), r);
}

fn rail(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let w = r.width.saturating_sub(3) as usize;
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(l.t("tui-rooms"), th.heading()))];
    for room in &app.rooms {
        let cur = Some(s(room, "id")) == app.room.as_deref();
        let name = if s(room, "name").is_empty() { l.t("tui-quick-session") } else { s(room, "name").to_string() };
        let mark = if cur { "› " } else { "  " };
        lines
            .push(Line::from(Span::styled(fit(&format!("{mark}{name}"), w), if cur { th.text() } else { th.text2() })));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(l.t("tui-people"), th.heading())));
    for m in &app.members {
        let hue = m["hue"].as_u64().map(|h| h as u16).unwrap_or(u16::MAX);
        let online = m["online"] == true;
        let dot = if online { "● " } else { "○ " };
        let mut name = s(m, "name").to_string();
        if m["me"] == true {
            name = tr(l, "tui-you", serde_json::json!({ "name": name }));
        }
        let name_style = if online { th.voice(hue, None) } else { th.text2() };
        lines.push(Line::from(vec![
            Span::styled(dot, th.voice(hue, None)),
            Span::styled(fit(&name, w.saturating_sub(2)), name_style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(l.t("tui-agents"), th.heading())));
    if app.agents.is_empty() {
        lines.push(Line::from(Span::styled(fit(&l.t("tui-no-agents"), w), th.text2())));
    }
    for a in &app.agents {
        let st = Status::from_state(s(a, "state"));
        let (hue, step) = app.agent_voice(s(a, "label"));
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", st.symbol(app.tick)), th.status(st)),
            Span::styled(fit(s(a, "label"), w.saturating_sub(2)), th.voice(hue, Some(step))),
        ]));
        lines.push(Line::from(Span::styled(format!("  {}", l.t(st.key())), th.status(st))));
    }
    let block = Block::default().borders(Borders::RIGHT).border_style(th.line()).padding(Padding::left(1));
    f.render_widget(Paragraph::new(lines).block(block), r);
}

fn name_style(app: &App, m: &Value) -> Style {
    match m["agent"].as_str() {
        Some(label) => {
            let (h, st) = app.agent_voice(label);
            app.theme.voice(h, Some(st))
        }
        None => app.theme.voice(app.hue_of(s(m, "author")), None),
    }
}

fn chat(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let r = inner(r);
    let (list_area, area) = if app.show_threads {
        let c = Layout::horizontal([Constraint::Length(28), Constraint::Min(20)]).split(r);
        (Some(c[0]), c[1])
    } else {
        (None, r)
    };
    if let Some(la) = list_area {
        let mut lines = vec![Line::from(Span::styled(l.t("tui-threads"), th.heading()))];
        let main = Line::from(Span::styled(
            fit(&l.t("tui-main-chat"), la.width as usize - 1),
            if app.open_thread.is_none() { th.selected() } else { th.text2() },
        ));
        lines.push(main);
        for (i, t) in app.threads.iter().enumerate() {
            let id = s(&t["thread"], "id");
            let open = app.open_thread.as_deref() == Some(id);
            let sel = app.sel(View::Chat) == i + 1;
            let text = format!("{} ({})", s(&t["thread"], "title"), t["posts"].as_u64().unwrap_or(0));
            let st = if open || sel { th.selected() } else { th.text() };
            lines.push(Line::from(Span::styled(fit(&text, la.width as usize - 1), st)));
        }
        f.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::RIGHT).border_style(th.line())),
            la,
        );
    }
    let msgs: Vec<&Value> = app.chat.iter().filter(|m| m["thread"].as_str() == app.open_thread.as_deref()).collect();
    let mut lines: Vec<Line> = vec![];
    if let Some(tid) = &app.open_thread {
        let title = app
            .threads
            .iter()
            .find(|t| s(&t["thread"], "id") == tid)
            .map(|t| s(&t["thread"], "title").to_string())
            .unwrap_or_default();
        lines.push(Line::from(Span::styled(title, th.text().add_modifier(Modifier::BOLD))));
    }
    if msgs.is_empty() {
        let key = if app.room.is_none() { "tui-empty-no-room" } else { "tui-empty-chat" };
        lines.push(Line::from(Span::styled(l.t(key), th.text2())));
    }
    let width = area.width as usize;
    let who_of = |m: &Value| m["agent"].as_str().map(str::to_string).unwrap_or_else(|| s(m, "name").to_string());
    // Names share one column so messages line up (long names are cut).
    let name_w = msgs.iter().map(|m| who_of(m).width()).max().unwrap_or(0).min(16);
    for m in &msgs {
        let who = fit(&who_of(m), name_w);
        let who = format!("{who}{}", " ".repeat(name_w.saturating_sub(who.width())));
        let head = format!("{}  ", clock(app, m["at"].as_i64().unwrap_or(0)));
        let who_w = who.width() + 2;
        let body_w = width.saturating_sub(head.width() + who_w).max(10);
        let wrapped = wrap(s(m, "text"), body_w);
        for (i, part) in wrapped.into_iter().enumerate() {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled(head.clone(), th.text2()),
                    Span::styled(format!("{who}  "), name_style(app, m)),
                    Span::styled(part, th.text()),
                ]));
            } else {
                lines
                    .push(Line::from(vec![Span::raw(" ".repeat(head.width() + who_w)), Span::styled(part, th.text())]));
            }
        }
    }
    let skip = lines.len().saturating_sub(area.height as usize);
    f.render_widget(Paragraph::new(lines.into_iter().skip(skip).collect::<Vec<_>>()), area);
}

fn inner(r: Rect) -> Rect {
    Rect { x: r.x + 1, y: r.y, width: r.width.saturating_sub(1), height: r.height }
}

fn board(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let r = inner(r);
    if app.columns.is_empty() {
        f.render_widget(Paragraph::new(Span::styled(l.t("tui-empty-board"), th.text2())), r);
        return;
    }
    const COL_MIN: u16 = 22;
    let visible = ((r.width / COL_MIN) as usize).clamp(1, app.columns.len());
    let first = app.board_col.saturating_sub(visible - 1).min(app.columns.len() - visible);
    let cols = Layout::horizontal(vec![Constraint::Ratio(1, visible as u32); visible]).split(r);
    for (slot, ci) in (first..first + visible).enumerate() {
        let id = &app.columns[ci].0;
        let area = cols[slot];
        let w = area.width.saturating_sub(3) as usize;
        let cards = app.column_cards(id);
        let title = l.t(&format!("column-{}", id.replace('_', "-")));
        let title = if title.starts_with("column-") { app.columns[ci].1.clone() } else { title };
        let head_style = if ci == app.board_col { th.accent().add_modifier(Modifier::BOLD) } else { th.heading() };
        let mut lines = vec![Line::from(Span::styled(fit(&format!("{title} {}", cards.len()), w), head_style))];
        for (i, c) in cards.iter().enumerate() {
            let selected = ci == app.board_col && i == app.sel(View::Board);
            let flashing = app.flash.contains_key(s(c, "id"));
            let st = if flashing {
                th.flash()
            } else if selected {
                th.selected()
            } else {
                th.text()
            };
            lines.push(Line::from(Span::styled(fit(s(c, "title"), w), st)));
            let mut meta: Vec<Span> = vec![];
            if let Some(a) = c["assignee"].as_str() {
                let (h, stp) = app.agent_voice(a);
                meta.push(Span::styled(
                    fit(a, w.saturating_sub(2)),
                    if h == u16::MAX { th.text2() } else { th.voice(h, Some(stp)) },
                ));
            }
            if c["pr"].is_string() {
                meta.push(Span::styled(format!(" {}", l.t("tui-card-pr")), th.text2()));
            }
            lines.push(Line::from(meta));
        }
        let block = Block::default()
            .borders(if slot + 1 < visible { Borders::RIGHT } else { Borders::NONE })
            .border_style(th.line())
            .padding(Padding::horizontal(1));
        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}

fn agents(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let r = inner(r);
    if app.agents.is_empty() {
        f.render_widget(Paragraph::new(Span::styled(l.t("tui-empty-agents"), th.text2())), r);
        return;
    }
    let cols = Layout::horizontal([Constraint::Length(30), Constraint::Min(20)]).split(r);
    let mut list = vec![];
    for (i, a) in app.agents.iter().enumerate() {
        let st = Status::from_state(s(a, "state"));
        let (h, step) = app.agent_voice(s(a, "label"));
        // Two lines, like the rail: the state word is never cut.
        let name = Line::from(vec![
            Span::styled(format!("{} ", st.symbol(app.tick)), th.status(st)),
            Span::styled(fit(s(a, "label"), 26), th.voice(h, Some(step))),
        ]);
        let state = Line::from(Span::styled(fit(&format!("  {}", l.t(st.key())), 28), th.status(st)));
        if i == app.sel(View::Agents) {
            list.push(name.style(th.selected()));
            list.push(state.style(th.selected()));
        } else {
            list.push(name);
            list.push(state);
        }
    }
    f.render_widget(
        Paragraph::new(list).block(Block::default().borders(Borders::RIGHT).border_style(th.line())),
        cols[0],
    );
    let area = inner(cols[1]);
    let w = area.width as usize;
    let mut lines: Vec<Line> = vec![];
    // A tool call is reported as it starts and as it ends: show it once, at
    // its first place, in its latest state.
    let mut last_tool: std::collections::HashMap<&str, &Value> = Default::default();
    for e in &app.transcript {
        if e["kind"]["t"] == "tool" {
            last_tool.insert(e["kind"]["c"]["id"].as_str().unwrap_or_default(), &e["kind"]["c"]);
        }
    }
    let mut shown_tools: std::collections::HashSet<&str> = Default::default();
    for e in &app.transcript {
        let k = &e["kind"];
        let c = &k["c"];
        match k["t"].as_str().unwrap_or_default() {
            "prompt" => {
                for (i, part) in wrap(s(c, "text"), w.saturating_sub(2)).into_iter().enumerate() {
                    lines.push(Line::from(Span::styled(
                        format!("{}{part}", if i == 0 { "> " } else { "  " }),
                        th.accent(),
                    )));
                }
            }
            "message" => {
                for part in wrap(s(c, "text").trim_end(), w) {
                    lines.push(Line::from(Span::styled(part, th.text())));
                }
            }
            "tool" => {
                let id = c["id"].as_str().unwrap_or_default();
                if !shown_tools.insert(id) {
                    continue;
                }
                let c = last_tool.get(id).copied().unwrap_or(c);
                let st = match s(c, "status") {
                    "failed" => Status::Error,
                    "completed" => Status::Done,
                    _ => Status::Running,
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", st.symbol(app.tick)), th.status(st)),
                    Span::styled(fit(s(c, "title"), w.saturating_sub(2)), th.text2()),
                ]));
            }
            "stopped" => lines.push(Line::from(Span::styled(l.t("tui-agent-stopped"), th.text2()))),
            _ => {}
        }
    }
    // Follow mode: the newest lines stay in view.
    let skip = lines.len().saturating_sub(area.height as usize);
    f.render_widget(Paragraph::new(lines.into_iter().skip(skip).collect::<Vec<_>>()), area);
}

fn who_of(app: &App, e: &Value) -> (String, Style) {
    match e["agent"].as_str() {
        Some(a) => {
            let (h, st) = app.agent_voice(a);
            (a.to_string(), app.theme.voice(h, Some(st)))
        }
        None => {
            let m = app.members.iter().find(|m| m["member"] == e["actor"]);
            let name = m.map(|m| s(m, "name").to_string()).unwrap_or_default();
            (name, app.theme.voice(app.hue_of(s(e, "actor")), None))
        }
    }
}

/// Column ids in activity arguments become the localized column names.
fn localize_columns(l: &Localizer, args: &Value) -> Value {
    let mut a = args.clone();
    for k in ["column", "from", "to"] {
        if let Some(id) = a.get(k).and_then(|v| v.as_str()) {
            let key = format!("column-{}", id.replace('_', "-"));
            if l.has(&key) {
                a[k] = Value::from(l.t(&key));
            }
        }
    }
    a
}

fn activity(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let r = inner(r);
    let (feed, replay) = match &app.replay {
        Some(_) => {
            let c = Layout::vertical([Constraint::Percentage(45), Constraint::Min(4)]).split(r);
            (c[0], Some(c[1]))
        }
        None => (r, None),
    };
    let w = feed.width as usize;
    let mut lines = vec![];
    if app.activity.is_empty() {
        lines.push(Line::from(Span::styled(l.t("tui-empty-activity"), th.text2())));
    }
    let sel = app.sel(View::Activity);
    let whens: Vec<String> = app.activity.iter().map(|e| ago(app, e["at"].as_i64().unwrap_or(0))).collect();
    let when_w = whens.iter().map(|w| w.width()).max().unwrap_or(0);
    for (i, e) in app.activity.iter().enumerate() {
        let (who, st) = who_of(app, e);
        let text = t_args(l, s(e, "summary_key"), &localize_columns(l, &e["args"]));
        let when = format!("{}{}  ", " ".repeat(when_w - whens[i].width()), whens[i]);
        let line = Line::from(vec![
            Span::styled(when.clone(), th.text2()),
            Span::styled(format!("{who} "), st),
            Span::styled(fit(&text, w.saturating_sub(when.width() + who.width() + 1)), th.text()),
        ]);
        lines.push(if i == sel && app.replay.is_some() { line.style(th.selected()) } else { line });
    }
    let h = feed.height as usize;
    let skip = if app.replay.is_some() { sel.saturating_sub(h / 2) } else { lines.len().saturating_sub(h) };
    f.render_widget(Paragraph::new(lines.into_iter().skip(skip).collect::<Vec<_>>()), feed);
    if let (Some(area), Some((at, board))) = (replay, &app.replay) {
        let mut out = vec![Line::from(Span::styled(
            tr(l, "tui-replay-at", serde_json::json!({ "time": clock(app, *at) })),
            th.accent().add_modifier(Modifier::BOLD),
        ))];
        for (id, _) in &app.columns {
            let cards: Vec<String> = board["columns"][id.as_str()]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c.get(1).and_then(|t| t.as_str()).map(str::to_string))
                .collect();
            if cards.is_empty() {
                continue;
            }
            let title = l.t(&format!("column-{}", id.replace('_', "-")));
            out.push(Line::from(vec![
                Span::styled(format!("{title}: "), th.heading()),
                Span::styled(fit(&cards.join(" · "), area.width as usize - title.width() - 2), th.text()),
            ]));
        }
        f.render_widget(
            Paragraph::new(out).block(Block::default().borders(Borders::TOP).border_style(th.line())),
            area,
        );
    }
}

fn merges(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let r = inner(r);
    let w = r.width as usize;
    let mut lines = vec![];
    if app.merges.is_empty() {
        lines.push(Line::from(Span::styled(l.t("tui-empty-merges"), th.text2())));
    }
    for (i, e) in app.merges.iter().enumerate().rev() {
        let state = s(e, "state");
        let st = match state {
            "merged" => Status::Done,
            "failed" => Status::Error,
            "changes_requested" => Status::Blocked,
            "awaiting_approval" => Status::Waiting,
            _ => Status::Running,
        };
        let label = l.t(&format!("merge-state-{}", state.replace('_', "-")));
        let line = Line::from(vec![
            Span::styled(format!("{} ", st.symbol(app.tick)), th.status(st)),
            Span::styled(
                fit(&format!("{} → {}", s(e, "branch"), s(e, "into")), w.saturating_sub(label.width() + 4)),
                th.text(),
            ),
            Span::styled(format!("  {label}"), th.status(st)),
        ]);
        lines.push(if i == app.sel(View::Merges) { line.style(th.selected()) } else { line });
        if let Some(d) = e["detail"].as_str().filter(|d| !d.is_empty()) {
            for part in wrap(d, w.saturating_sub(2)).into_iter().take(3) {
                lines.push(Line::from(Span::styled(format!("  {part}"), th.text2())));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), r);
}

fn vt_color(c: vt100::Color, th: &crate::theme::Theme) -> Option<ratatui::style::Color> {
    use ratatui::style::Color;
    if th.depth == crate::theme::Depth::None {
        return None;
    }
    match c {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) => Some(Color::Indexed(i)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

fn terminals(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let r = inner(r);
    if app.terms.is_empty() {
        f.render_widget(Paragraph::new(Span::styled(l.t("tui-empty-terminals"), th.text2())), r);
        return;
    }
    let cols = Layout::horizontal([Constraint::Length(26), Constraint::Min(20)]).split(r);
    let mut list = vec![];
    for (i, t) in app.terms.iter().enumerate() {
        let st = if t["closed"] == true { Status::Stopped } else { Status::Idle };
        let owner = app.members.iter().find(|m| m["member"] == t["owner"]);
        let hue = owner.and_then(|m| m["hue"].as_u64()).map(|h| h as u16).unwrap_or(u16::MAX);
        let line = Line::from(vec![
            Span::styled(format!("{} ", st.symbol(0)), th.status(st)),
            Span::styled(fit(s(t, "title"), 22), th.voice(hue, None)),
        ]);
        list.push(if i == app.sel(View::Terminals) { line.style(th.selected()) } else { line });
        if let Some(w) = t["writer"].as_str() {
            let name =
                app.members.iter().find(|m| s(m, "member") == w).map(|m| s(m, "name").to_string()).unwrap_or_default();
            list.push(Line::from(Span::styled(
                fit(&format!("  {}", tr(l, "tui-term-writer", serde_json::json!({ "name": name }))), 24),
                th.text2(),
            )));
        }
    }
    f.render_widget(
        Paragraph::new(list).block(Block::default().borders(Borders::RIGHT).border_style(th.line())),
        cols[0],
    );
    let Some(t) = app.selected_term() else { return };
    let id = s(t, "id");
    let attached = app.focus == Focus::Attached(id.to_string());
    let area = cols[1];
    let title = if attached { l.t("tui-term-attached") } else { l.t("tui-term-preview") };
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(if attached { th.accent() } else { th.line() })
        .title(Span::styled(title, th.text2()));
    let inner_area = block.inner(area);
    f.render_widget(block, area);
    let Some(p) = app.screens.get(id) else { return };
    let screen = p.screen();
    let (rows, cols_n) = screen.size();
    let mut lines = vec![];
    for row in 0..rows.min(inner_area.height) {
        let mut spans = vec![];
        for col in 0..cols_n.min(inner_area.width) {
            let Some(cell) = screen.cell(row, col) else { continue };
            if cell.is_wide_continuation() {
                continue;
            }
            let mut st = Style::default();
            if let Some(c) = vt_color(cell.fgcolor(), th) {
                st = st.fg(c);
            }
            if let Some(c) = vt_color(cell.bgcolor(), th) {
                st = st.bg(c);
            }
            if cell.bold() {
                st = st.add_modifier(Modifier::BOLD);
            }
            if cell.inverse() {
                st = st.add_modifier(Modifier::REVERSED);
            }
            let text = if cell.has_contents() { cell.contents().to_string() } else { " ".to_string() };
            spans.push(Span::styled(text, st));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner_area);
}

fn approvals(f: &mut Frame, app: &App, r: Rect, mine: &[&Value]) {
    let th = &app.theme;
    let l = &app.l;
    let w = r.width.saturating_sub(4) as usize;
    let mut lines = vec![];
    for (i, a) in mine.iter().enumerate().take(3) {
        let mark = if i == app.approval_sel { "› " } else { "  " };
        let st =
            if i == app.approval_sel { th.status(Status::Waiting).add_modifier(Modifier::BOLD) } else { th.text() };
        lines.push(Line::from(vec![
            Span::styled(mark, th.status(Status::Waiting)),
            Span::styled(fit(&describe_approval(l, a), w), st),
        ]));
    }
    lines.push(Line::from(Span::styled(fit(&l.t("tui-approve-keys"), w + 2), th.text2())));
    let block = Block::default().borders(Borders::TOP).border_style(th.status(Status::Waiting)).title(Span::styled(
        format!(" ! {} ", tr(l, "tui-approvals", serde_json::json!({ "n": mine.len() }))),
        th.status(Status::Waiting),
    ));
    f.render_widget(Paragraph::new(lines).block(block), r);
}

fn input(f: &mut Frame, app: &App, r: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let w = r.width as usize;
    let (prompt, text, hint) = match &app.focus {
        Focus::Input => {
            let p = if app.view == View::Agents {
                tr(
                    l,
                    "tui-prompt-agent",
                    serde_json::json!({ "agent": app.selected_agent().map(|a| s(a, "label")).unwrap_or_default() }),
                )
            } else if app.open_thread.is_some() {
                l.t("tui-prompt-thread")
            } else {
                l.t("tui-prompt-chat")
            };
            (format!("{p} › "), app.input.clone(), l.t("tui-hint-input"))
        }
        Focus::Command => (":".to_string(), app.input.clone(), l.t("tui-hint-command")),
        Focus::DenyNote(_) => (format!("{} › ", l.t("tui-prompt-deny")), app.input.clone(), l.t("tui-hint-input")),
        Focus::Attached(_) => (String::new(), String::new(), l.t("tui-hint-attached")),
        Focus::Normal => {
            if let Some((m, _)) = &app.message {
                f.render_widget(Paragraph::new(Span::styled(fit(&format!(" {m}"), w), th.accent())), r);
                return;
            }
            (String::new(), String::new(), l.t("tui-hint-normal"))
        }
    };
    if prompt.is_empty() {
        f.render_widget(Paragraph::new(Span::styled(fit(&format!(" {hint}"), w), th.text2())), r);
        return;
    }
    // Show the end of long input.
    let room = w.saturating_sub(prompt.width() + 2);
    let shown: String = if text.width() > room {
        let mut acc = String::new();
        for c in text.chars().rev() {
            if acc.width() + c.width().unwrap_or(0) > room.saturating_sub(1) {
                break;
            }
            acc.insert(0, c);
        }
        format!("…{acc}")
    } else {
        text.clone()
    };
    let line =
        Line::from(vec![Span::styled(format!(" {prompt}"), th.accent()), Span::styled(shown.clone(), th.text())]);
    f.render_widget(Paragraph::new(line), r);
    let x = r.x + 1 + prompt.width() as u16 + shown.width() as u16;
    f.set_cursor_position((x.min(r.x + r.width.saturating_sub(1)), r.y));
}

fn help(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let l = &app.l;
    let keys = [
        ("1-6  Tab", "tui-help-views"),
        ("j k  ↑ ↓", "tui-help-move"),
        ("h l", "tui-help-columns"),
        ("H L", "tui-help-move-card"),
        ("Enter", "tui-help-open"),
        ("i", "tui-help-input"),
        (":", "tui-help-command"),
        ("a t d", "tui-help-approve"),
        ("n", "tui-help-next-approval"),
        ("s", "tui-help-stop"),
        ("x", "tui-help-close"),
        ("[ ]", "tui-help-rooms"),
        ("T", "tui-help-threads"),
        ("r", "tui-help-rail"),
        ("Esc", "tui-help-escape"),
        ("q", "tui-help-quit"),
    ];
    let w = 64.min(area.width.saturating_sub(4));
    let h = (keys.len() as u16 + 4).min(area.height.saturating_sub(2));
    let r = Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h };
    let mut lines = vec![];
    for (k, key) in keys {
        lines.push(Line::from(vec![
            Span::styled(format!(" {k:<10} "), th.accent()),
            Span::styled(fit(&l.t(key), w as usize - 14), th.text()),
        ]));
    }
    f.render_widget(Clear, r);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(th.line())
        .style(th.bg())
        .title(Span::styled(format!(" {} ", l.t("tui-help-title")), th.heading()));
    f.render_widget(Paragraph::new(lines).block(block), r);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_and_wrapping_respect_cell_widths() {
        assert_eq!(fit("hello", 10), "hello");
        assert_eq!(fit("hello world", 6), "hello…");
        // Japanese is two cells per character.
        let j = fit("日本語のテキスト", 7);
        assert!(j.width() <= 7, "{j}");
        // Thai marks stay with their consonant.
        let th = fit("สวัสดีครับ", 4);
        assert!(th.width() <= 4 && !th.starts_with('\u{0E31}'), "{th}");
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
        let w = wrap("日本語のテキストです", 6);
        assert!(w.iter().all(|l| l.width() <= 6), "{w:?}");
        assert_eq!(wrap("a\nb", 10), vec!["a", "b"]);
    }
}
