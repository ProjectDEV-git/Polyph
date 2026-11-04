//! Milestone 6: the terminal UI against real daemons, drawn into ratatui's
//! test buffer: every view, every language, with and without color, wide and
//! narrow; key flows; live updates.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use polyph_i18n::Localizer;
use polyph_testkit::harness::{AgentBins, Harness};
use polyph_testkit::tui::{InProcess, events};
use polyph_tui::App;
use polyph_tui::conn::Backend;
use polyph_tui::keys::{on_key, perform};
use polyph_tui::model::{Parts, View};
use polyph_tui::theme::{Depth, Mode, Theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use serde_json::{Value, json};

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

async fn call(h: &Harness, i: usize, m: &str, p: Value) -> Value {
    h.nodes[i].call(m, p).await.unwrap()
}

/// Ann (owner) and Bo in "payments": chat, a thread, cards, an agent waiting
/// for Ann's approval, and Ann's shared terminal.
async fn room() -> (Harness, String, tempfile::TempDir, tokio::task::JoinHandle<()>) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let (a, b) = (h.add("Ann").await.unwrap(), h.add("Bo").await.unwrap());
    let r = call(&h, a, "room.create", json!({ "name": "payments" })).await;
    let rid = r["id"].as_str().unwrap().to_string();
    h.until("connected", || async { call(&h, a, "room.get", json!({ "room": rid })).await["connected"] == true })
        .await
        .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();
    call(&h, a, "chat.send", json!({ "room": rid, "text": "Shall we split billing?" })).await;
    h.clock.advance(1);
    call(&h, b, "chat.send", json!({ "room": rid, "text": "Yes, I take invoices." })).await;
    call(&h, a, "thread.create", json!({ "room": rid, "title": "Which database?", "text": "Postgres?" })).await;
    for (t, col) in [
        ("Add refunds", "ready"),
        ("Add invoices", "in_progress"),
        ("ระบบชำระเงิน", "backlog"),
        ("請求書の分割", "backlog"),
    ] {
        call(&h, a, "board.add", json!({ "room": rid, "title": t, "column": col })).await;
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("polyph.toml"), "").unwrap();
    std::fs::write(
        dir.path().join(".fake-script.json"),
        json!({ "turns": [[ { "message": "Checking the release" }, { "permission": { "kind": "execute", "title": "Run cargo publish", "command": "cargo publish" } } ]] }).to_string(),
    )
    .unwrap();
    let ag = call(&h, a, "agent.start", json!({ "room": rid, "kind": "fake", "cwd": dir.path(), "preset": "default" }))
        .await;
    let id = ag["id"].as_str().unwrap().to_string();
    let d = h.nodes[a].daemon.clone();
    // The prompt waits for Ann's approval; it finishes when she answers.
    let turn = tokio::spawn(async move {
        let _ = d.call("agent.prompt", json!({ "agent": id, "text": "publish" })).await;
    });
    h.until("approval pending", || async {
        !call(&h, a, "approvals.list", json!({ "room": rid })).await.as_array().unwrap().is_empty()
    })
    .await
    .unwrap();
    #[cfg(unix)]
    call(
        &h,
        a,
        "term.open",
        json!({ "room": rid, "cwd": dir.path(), "command": ["sh"], "title": "Ann's shell", "cols": 60, "rows": 8 }),
    )
    .await;
    (h, rid, dir, turn)
}

async fn app_for(h: &Harness, i: usize, lang: &str, depth: Depth) -> (App, Arc<dyn Backend>) {
    let b: Arc<dyn Backend> = Arc::new(InProcess(h.nodes[i].daemon.clone()));
    let mut app = App::new(Localizer::new(lang), Theme::new(depth, Mode::Dark));
    app.refresh(b.as_ref(), Parts::ALL).await;
    (app, b)
}

/// Draws the app and returns the screen as text plus the buffer.
fn screen(app: &mut App, w: u16, h: u16) -> (String, ratatui::buffer::Buffer) {
    app.width = w;
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| polyph_tui::ui::draw(f, app)).unwrap();
    let buf = t.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            let sym = buf[(x, y)].symbol();
            text.push_str(sym);
            // A wide character's second cell is blank: skip it.
            x += unicode_width::UnicodeWidthStr::width(sym).max(1) as u16;
        }
        text.push('\n');
    }
    (text, buf)
}

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), if c.is_ascii_uppercase() { KeyModifiers::SHIFT } else { KeyModifiers::NONE })
}

fn special(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Feeds keys and runs the resulting actions to completion.
async fn press(app: &mut App, b: &Arc<dyn Backend>, keys: Vec<KeyEvent>) {
    for k in keys {
        for a in on_key(app, k) {
            let o = perform(b.clone(), a).await;
            app.apply_outcome(&o);
            app.refresh(b.as_ref(), o.parts).await;
        }
    }
}

async fn type_text(app: &mut App, b: &Arc<dyn Backend>, s: &str) {
    press(app, b, s.chars().map(key).collect()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_view_in_every_language_with_and_without_color() {
    let (h, _rid, _dir, _turn) = room().await;
    for lang in ["en", "th", "ja", "zh-Hans"] {
        for depth in [Depth::TrueColor, Depth::Ansi256, Depth::None] {
            let (mut app, _) = app_for(&h, 0, lang, depth).await;
            let l = Localizer::new(lang);
            for (w, hgt) in [(120u16, 36u16), (70, 24), (40, 12)] {
                for v in View::ALL {
                    app.view = v;
                    let (text, buf) = screen(&mut app, w, hgt);
                    // The tab of each view is on screen in the person's language.
                    if w >= 120 {
                        assert!(text.contains(&l.t(v.key())), "{lang} {v:?}:\n{text}");
                    }
                    // Approvals are always stated, with what will happen.
                    if w >= 70 {
                        assert!(text.contains("cargo publish"), "{lang} {v:?} {w}:\n{text}");
                    }
                    if depth == Depth::None {
                        let colored = buf.content().iter().any(|c| c.fg != Color::Reset || c.bg != Color::Reset);
                        assert!(!colored, "NO_COLOR draws no colors ({lang} {v:?})");
                    }
                }
            }
            // The rail shows people and agents with a state word, wide only.
            app.view = View::Chat;
            let (text, _) = screen(&mut app, 120, 36);
            assert!(text.contains("Bo") && text.contains("Ann/fake"), "{text}");
            assert!(
                text.contains(&l.t("tui-state-waiting")),
                "{lang}: the state word is there, not only a color:\n{text}"
            );
            let (narrow, _) = screen(&mut app, 70, 24);
            assert!(!narrow.contains(&l.t("tui-rooms")), "the rail hides below 80 columns:\n{narrow}");
        }
    }
    // Board in Thai shows localized column names and keeps Thai and Japanese titles intact.
    let (mut app, _) = app_for(&h, 0, "th", Depth::TrueColor).await;
    app.view = View::Board;
    let (text, _) = screen(&mut app, 160, 20);
    assert!(text.contains(&Localizer::new("th").t("column-backlog")), "{text}");
    assert!(text.replace(' ', "").contains("ระบบชำระเงิน"), "{text}");
    // Wide characters take two cells: the buffer leaves the second one empty.
    assert!(text.replace(' ', "").contains("請求書の分割"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn keys_approve_chat_move_cards_and_type_in_a_terminal() {
    let (h, rid, _dir, turn) = room().await;
    let (mut app, b) = app_for(&h, 0, "en", Depth::TrueColor).await;

    // The approval stack states the command; `a` approves once.
    let (text, _) = screen(&mut app, 120, 36);
    assert!(text.contains("Ann/fake wants to run: cargo publish"), "{text}");
    press(&mut app, &b, vec![key('a')]).await;
    assert!(app.my_approvals().is_empty());
    turn.await.unwrap();
    let (text, _) = screen(&mut app, 120, 36);
    assert!(text.contains("Approved."), "{text}");

    // Chat: i, type, Enter.
    press(&mut app, &b, vec![key('i')]).await;
    type_text(&mut app, &b, "Approved the release").await;
    press(&mut app, &b, vec![special(KeyCode::Enter)]).await;
    let chat = call(&h, 1, "chat.list", json!({ "room": rid })).await;
    h.until("Bo has it", || async {
        call(&h, 1, "chat.list", json!({ "room": rid }))
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["text"] == "Approved the release")
    })
    .await
    .unwrap();
    drop(chat);

    // Threads: T shows them, j selects, Enter opens, i writes into it.
    press(&mut app, &b, vec![key('T'), key('j'), special(KeyCode::Enter), key('i')]).await;
    type_text(&mut app, &b, "SQLite first").await;
    press(&mut app, &b, vec![special(KeyCode::Enter)]).await;
    let (text, _) = screen(&mut app, 120, 36);
    assert!(text.contains("Which database?") && text.contains("SQLite first"), "{text}");
    assert!(!text.contains("Shall we split billing?"), "the thread shows only its posts:\n{text}");

    // Board: go to Ready, move "Add refunds" right with L.
    press(&mut app, &b, vec![key('2'), key('l'), key('L')]).await;
    let board = call(&h, 1, "board.get", json!({ "room": rid })).await;
    let refunds = board["cards"].as_array().unwrap().iter().find(|c| c["title"] == "Add refunds").unwrap().clone();
    h.until("move reaches Bo", || async {
        let b2 = call(&h, 1, "board.get", json!({ "room": rid })).await;
        b2["cards"].as_array().unwrap().iter().any(|c| c["title"] == "Add refunds" && c["column"] == "claimed")
    })
    .await
    .unwrap();
    drop(refunds);

    // `:card` adds a card; `:` with nonsense explains itself.
    press(&mut app, &b, vec![key(':')]).await;
    type_text(&mut app, &b, "card Write the changelog").await;
    press(&mut app, &b, vec![special(KeyCode::Enter)]).await;
    assert!(app.cards.iter().any(|c| c["title"] == "Write the changelog"));
    press(&mut app, &b, vec![key(':')]).await;
    type_text(&mut app, &b, "frobnicate").await;
    press(&mut app, &b, vec![special(KeyCode::Enter)]).await;
    assert!(app.message.as_ref().unwrap().0.contains("Unknown command"));

    // Terminal: attach, type a command, see its output, detach with Ctrl-].
    #[cfg(unix)]
    {
        press(&mut app, &b, vec![key('6'), special(KeyCode::Enter)]).await;
        type_text(&mut app, &b, "echo tui-''typed").await;
        press(&mut app, &b, vec![special(KeyCode::Enter)]).await;
        let mut events_rx = events(&h.nodes[0].daemon);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let (text, _) = screen(&mut app, 120, 36);
            if text.contains("tui-typed\n") || text.lines().any(|l| l.trim_end().ends_with("tui-typed")) {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "no output:\n{text}");
            if let Ok(Some(ev)) = tokio::time::timeout(std::time::Duration::from_millis(200), events_rx.recv()).await {
                app.apply_event(&ev);
            }
        }
        press(&mut app, &b, vec![KeyEvent::new(KeyCode::Char('5'), KeyModifiers::CONTROL)]).await;
        assert_eq!(app.focus, polyph_tui::model::Focus::Normal);

        // Bo can see it but not type without the keyboard.
        let (mut bo, bb) = app_for(&h, 1, "en", Depth::TrueColor).await;
        press(&mut bo, &bb, vec![key('6'), special(KeyCode::Enter), key('x')]).await;
        assert!(bo.message.as_ref().unwrap().0.contains("You do not have the keyboard"), "{:?}", bo.message);
        press(&mut bo, &bb, vec![key('a')]).await;
        h.until("Ann is asked", || async {
            call(&h, 0, "approvals.list", json!({ "room": rid }))
                .await
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["kind"]["t"] == "terminal_input")
        })
        .await
        .unwrap();
        app.refresh(b.as_ref(), Parts::APPROVALS).await;
        let (text, _) = screen(&mut app, 120, 36);
        assert!(text.contains("asks to type in your terminal"), "{text}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_updates_and_the_moved_card_highlight() {
    let (h, rid, _dir, _turn) = room().await;
    let (mut app, b) = app_for(&h, 0, "en", Depth::TrueColor).await;
    let mut rx = events(&h.nodes[0].daemon);
    app.view = View::Board;
    // Bo moves a card; Ann's screen follows and highlights it.
    let id = h
        .until_value("cards reach Bo", || async {
            let cards = call(&h, 1, "board.get", json!({ "room": rid })).await;
            cards["cards"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["title"] == "Add refunds")
                .map(|c| c["id"].as_str().unwrap().to_string())
        })
        .await
        .unwrap();
    call(&h, 1, "board.move", json!({ "room": rid, "card": id, "column": "review" })).await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut pending = Parts::default();
    while !app.cards.iter().any(|c| c["id"] == id.as_str() && c["column"] == "review") {
        assert!(tokio::time::Instant::now() < deadline);
        if let Ok(Some(ev)) = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await {
            app.apply_event(&ev);
            pending |= app.parts_for(&ev);
        }
        app.refresh(b.as_ref(), std::mem::take(&mut pending)).await;
    }
    assert!(app.flash.contains_key(&id), "the moved card is highlighted");
    app.board_col = 4;
    let (text, buf) = screen(&mut app, 160, 20);
    let line = text.lines().position(|l| l.contains("Add refunds")).unwrap() as u16;
    let x = (0..160u16).find(|x| buf[(*x, line)].symbol() == "A" && buf[(*x + 1, line)].symbol() == "d").unwrap();
    assert_ne!(buf[(x, line)].bg, Color::Reset, "the highlight has a background");
    // It fades after 600 ms.
    tokio::time::sleep(polyph_tui::model::FLASH + std::time::Duration::from_millis(50)).await;
    app.animate();
    assert!(app.flash.is_empty());

    // A chat message from Bo arrives live.
    app.view = View::Chat;
    call(&h, 1, "chat.send", json!({ "room": rid, "text": "live from Bo" })).await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let (text, _) = screen(&mut app, 120, 30);
        if text.contains("live from Bo") {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "{text}");
        if let Ok(Some(ev)) = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await {
            app.apply_event(&ev);
            let p = app.parts_for(&ev);
            app.refresh(b.as_ref(), p).await;
        }
    }
}
