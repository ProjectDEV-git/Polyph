//! DESIGN.md's tokens for a terminal: true color when the terminal says it
//! has it, the nearest of the 256 xterm colors otherwise, and none at all with
//! `NO_COLOR` or `TERM=dumb`. Without color, selection is reverse video and
//! headings are bold; symbols and words carry every state.

use polyph_core::voice::{self, Step};
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    TrueColor,
    Ansi256,
    None,
}

impl Depth {
    /// From the environment, as terminals advertise it.
    pub fn detect() -> Depth {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let term = std::env::var("TERM").unwrap_or_default();
        if no_color || term == "dumb" {
            return Depth::None;
        }
        let ct = std::env::var("COLORTERM").unwrap_or_default().to_lowercase();
        if ct == "truecolor" || ct == "24bit" { Depth::TrueColor } else { Depth::Ansi256 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

/// A status, with the symbol and the color DESIGN.md gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Running,
    Waiting,
    Blocked,
    Error,
    Done,
    Idle,
    Stopped,
}

impl Status {
    pub fn from_state(s: &str) -> Status {
        match s {
            "running" | "starting" => Status::Running,
            "waiting_approval" => Status::Waiting,
            "blocked" => Status::Blocked,
            "error" => Status::Error,
            "done" => Status::Done,
            "stopped" => Status::Stopped,
            _ => Status::Idle,
        }
    }

    /// One cell wide in every terminal font we target.
    pub fn symbol(self, tick: usize) -> &'static str {
        const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        match self {
            Status::Running => SPIN[tick % SPIN.len()],
            Status::Waiting => "!",
            Status::Blocked => "■",
            Status::Error => "✕",
            Status::Done => "✓",
            Status::Idle => "○",
            Status::Stopped => "·",
        }
    }

    /// Translation key for the word shown beside the symbol.
    pub fn key(self) -> &'static str {
        match self {
            Status::Running => "tui-state-running",
            Status::Waiting => "tui-state-waiting",
            Status::Blocked => "tui-state-blocked",
            Status::Error => "tui-state-error",
            Status::Done => "tui-state-done",
            Status::Idle => "tui-state-idle",
            Status::Stopped => "tui-state-stopped",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub depth: Depth,
    pub mode: Mode,
}

/// An sRGB color to the 256-color palette: the 6x6x6 cube or the gray
/// ramp, whichever is closer.
pub fn nearest_256(c: (u8, u8, u8)) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let dist = |a: (u8, u8, u8)| {
        let d = |x: u8, y: u8| (i32::from(x) - i32::from(y)).pow(2);
        d(a.0, c.0) + d(a.1, c.1) + d(a.2, c.2)
    };
    let level = |v: u8| (0..6).min_by_key(|&i| (i32::from(LEVELS[i]) - i32::from(v)).abs()).unwrap_or(0);
    let (ri, gi, bi) = (level(c.0), level(c.1), level(c.2));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let avg = (i32::from(c.0) + i32::from(c.1) + i32::from(c.2)) / 3;
    let gray_i = ((avg - 8).max(0) / 10).min(23) as u8;
    let gv = 8 + gray_i * 10;
    if dist((gv, gv, gv)) < dist(cube) { 232 + gray_i } else { 16 + 36 * ri as u8 + 6 * gi as u8 + bi as u8 }
}

fn rgb_of(lch: (f64, f64, f64)) -> (u8, u8, u8) {
    let (r, g, b) = voice::srgb_from_oklch(lch);
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    (q(r), q(g), q(b))
}

impl Theme {
    pub fn new(depth: Depth, mode: Mode) -> Self {
        Self { depth, mode }
    }

    fn color(&self, rgb: (u8, u8, u8)) -> Option<Color> {
        match self.depth {
            Depth::TrueColor => Some(Color::Rgb(rgb.0, rgb.1, rgb.2)),
            Depth::Ansi256 => Some(Color::Indexed(nearest_256(rgb))),
            Depth::None => None,
        }
    }

    fn token(&self, dark: (f64, f64, f64), light: (f64, f64, f64)) -> Option<Color> {
        self.color(rgb_of(if self.mode == Mode::Dark { dark } else { light }))
    }

    fn fg(&self, c: Option<Color>) -> Style {
        match c {
            Some(c) => Style::default().fg(c),
            None => Style::default(),
        }
    }

    pub fn bg(&self) -> Style {
        match self.token((0.165, 0.008, 265.0), (0.985, 0.003, 265.0)) {
            Some(c) => Style::default()
                .bg(c)
                .fg(self.token((0.93, 0.006, 265.0), (0.22, 0.012, 265.0)).unwrap_or(Color::Reset)),
            None => Style::default(),
        }
    }

    pub fn text(&self) -> Style {
        self.fg(self.token((0.93, 0.006, 265.0), (0.22, 0.012, 265.0)))
    }

    pub fn text2(&self) -> Style {
        self.fg(self.token((0.74, 0.010, 265.0), (0.45, 0.012, 265.0)))
    }

    pub fn line(&self) -> Style {
        self.fg(self.token((0.30, 0.010, 265.0), (0.89, 0.006, 265.0)))
    }

    pub fn accent(&self) -> Style {
        self.fg(self.token((0.80, 0.090, 205.0), (0.52, 0.110, 215.0)))
    }

    /// Section headings: secondary text, bold.
    pub fn heading(&self) -> Style {
        self.text2().add_modifier(Modifier::BOLD)
    }

    /// The selected row: a raised surface, or reverse video without color.
    pub fn selected(&self) -> Style {
        match self.token((0.265, 0.011, 265.0), (0.94, 0.006, 265.0)) {
            Some(c) => self.text().bg(c),
            None => Style::default().add_modifier(Modifier::REVERSED),
        }
    }

    /// The 600 ms highlight on a card that moved.
    pub fn flash(&self) -> Style {
        match self.token((0.80, 0.090, 205.0), (0.52, 0.110, 215.0)) {
            Some(c) => Style::default()
                .fg(self.token((0.165, 0.008, 265.0), (0.985, 0.003, 265.0)).unwrap_or(Color::Reset))
                .bg(c),
            None => Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        }
    }

    pub fn status(&self, s: Status) -> Style {
        let c = match s {
            Status::Running => self.token((0.78, 0.14, 150.0), (0.52, 0.13, 150.0)),
            Status::Waiting => self.token((0.83, 0.13, 80.0), (0.58, 0.12, 75.0)),
            Status::Blocked => self.token((0.75, 0.14, 50.0), (0.56, 0.15, 45.0)),
            Status::Error => self.token((0.70, 0.17, 25.0), (0.53, 0.19, 25.0)),
            Status::Done | Status::Idle | Status::Stopped => self.token((0.74, 0.010, 265.0), (0.45, 0.012, 265.0)),
        };
        let st = self.fg(c);
        if self.depth == Depth::None && s == Status::Waiting { st.add_modifier(Modifier::BOLD) } else { st }
    }

    /// A voice: a person (`agent` None) or their n-th agent.
    pub fn voice(&self, hue: u16, agent: Option<u8>) -> Style {
        if hue == u16::MAX {
            return self.text();
        }
        let mode = if self.mode == Mode::Dark { voice::Theme::Dark } else { voice::Theme::Light };
        let step = match agent {
            None => Step::Person,
            Some(n) => Step::Agent(n),
        };
        self.fg(self.color(voice::voice_rgb(hue, step, mode)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_mapping() {
        assert_eq!(nearest_256((255, 0, 0)), 196);
        assert_eq!(nearest_256((0, 0, 0)), 16);
        assert_eq!(nearest_256((128, 128, 128)), 244);
        let t = Theme::new(Depth::None, Mode::Dark);
        assert_eq!(t.voice(255, None), Style::default());
        assert!(t.selected().add_modifier.contains(Modifier::REVERSED));
        let t = Theme::new(Depth::TrueColor, Mode::Dark);
        assert!(matches!(t.voice(255, Some(0)).fg, Some(Color::Rgb(..))));
        let t = Theme::new(Depth::Ansi256, Mode::Light);
        assert!(matches!(t.status(Status::Running).fg, Some(Color::Indexed(_))));
    }

    #[test]
    fn every_status_symbol_is_one_cell() {
        use unicode_width::UnicodeWidthStr;
        for s in [
            Status::Running,
            Status::Waiting,
            Status::Blocked,
            Status::Error,
            Status::Done,
            Status::Idle,
            Status::Stopped,
        ] {
            for t in 0..10 {
                assert_eq!(s.symbol(t).width(), 1, "{s:?}");
            }
        }
    }
}
