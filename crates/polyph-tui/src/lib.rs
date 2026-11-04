//! Polyph's terminal UI: the whole room on one screen, for terminals and SSH
//! sessions. A client of the local daemon like every other surface. Design
//! brief: `docs/tui.md`.

pub mod conn;
pub mod keys;
pub mod model;
pub mod run;
pub mod theme;
pub mod ui;

pub use model::App;
pub use run::run;
