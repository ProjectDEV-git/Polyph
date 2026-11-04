//! The live loop: keys, daemon events and finished actions in, redraws out.
//! Refreshes are coalesced; redraws happen on change, at most 30 a second;
//! the spinner ticks every 100 ms only while something animates.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use futures_util::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::conn::Backend;
use crate::keys::{Action, Outcome, on_key, perform};
use crate::model::{App, Parts};
use crate::ui::draw;

const FRAME: Duration = Duration::from_millis(33);
const TICK: Duration = Duration::from_millis(100);

fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(std::io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
}

/// Runs the TUI until the person quits. The daemon keeps running.
pub async fn run(
    mut app: App,
    backend: Arc<dyn Backend>,
    mut events: mpsc::UnboundedReceiver<Value>,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    // Leave the terminal usable even if drawing panics.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        prev(info);
    }));
    let r = run_inner(&mut app, backend, &mut events).await;
    restore();
    r
}

async fn run_inner(
    app: &mut App,
    backend: Arc<dyn Backend>,
    events: &mut mpsc::UnboundedReceiver<Value>,
) -> anyhow::Result<()> {
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    app.width = term.size()?.width;
    app.refresh(backend.as_ref(), Parts::ALL).await;
    let mut keys = EventStream::new();
    let (done_tx, mut done_rx) = mpsc::unbounded_channel::<Outcome>();
    // Keystrokes for a terminal go through one queue, in order.
    let (keys_tx, mut keys_rx) = mpsc::unbounded_channel::<Action>();
    {
        let (b, tx) = (backend.clone(), done_tx.clone());
        tokio::spawn(async move {
            let mut next: Option<Action> = None;
            loop {
                let a = match next.take() {
                    Some(a) => a,
                    None => match keys_rx.recv().await {
                        Some(a) => a,
                        None => return,
                    },
                };
                let Action::TermInput { room, term, mut bytes } = a else { continue };
                // Whatever queued up meanwhile for the same terminal goes in one write.
                while let Ok(more) = keys_rx.try_recv() {
                    match more {
                        Action::TermInput { term: t2, bytes: b2, .. } if t2 == term => bytes.extend(b2),
                        other => {
                            next = Some(other);
                            break;
                        }
                    }
                }
                let o = perform(b.clone(), Action::TermInput { room, term, bytes }).await;
                if o.message.is_some() {
                    let _ = tx.send(o);
                }
            }
        });
    }
    let mut tick = tokio::time::interval(TICK);
    let mut pending = Parts::default();
    let mut dirty = true;
    let mut last_draw = Instant::now() - FRAME;
    loop {
        if dirty && last_draw.elapsed() >= FRAME {
            term.draw(|f| draw(f, app))?;
            last_draw = Instant::now();
            dirty = false;
        }
        if app.quit {
            return Ok(());
        }
        tokio::select! {
            k = keys.next() => match k {
                Some(Ok(Event::Key(k))) if k.kind != KeyEventKind::Release => {
                    for a in on_key(app, k) {
                        if matches!(a, Action::TermInput { .. }) {
                            let _ = keys_tx.send(a);
                            continue;
                        }
                        let (b, tx) = (backend.clone(), done_tx.clone());
                        tokio::spawn(async move {
                            let _ = tx.send(perform(b, a).await);
                        });
                    }
                    dirty = true;
                }
                Some(Ok(Event::Resize(w, _))) => {
                    app.width = w;
                    dirty = true;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return Ok(()),
            },
            ev = events.recv() => match ev {
                Some(ev) => {
                    app.apply_event(&ev);
                    pending |= app.parts_for(&ev);
                    dirty = true;
                }
                None => {
                    app.say(app.l.t("tui-lost-daemon"));
                    term.draw(|f| draw(f, app))?;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    return Ok(());
                }
            },
            o = done_rx.recv() => {
                if let Some(o) = o {
                    app.apply_outcome(&o);
                    pending |= o.parts;
                    dirty = true;
                }
            }
            _ = tick.tick() => {
                if pending != Parts::default() {
                    let p = std::mem::take(&mut pending);
                    app.refresh(backend.as_ref(), p).await;
                    dirty = true;
                }
                if app.animate() {
                    dirty = true;
                }
            }
        }
    }
}
