//! The Polyph daemon. Runs on each person's machine, owns rooms and their
//! relay connections, and serves the local API used by the CLI, TUI, desktop
//! app, VS Code extension and status pill.

pub mod agents;
pub mod api;
pub mod clock;
pub mod config;
pub mod coord;
pub mod daemon;
pub mod events;
pub mod export;
pub mod lan;
pub mod paths;
pub mod runtime;
pub mod secrets;
pub mod status;

pub use clock::Clock;
pub use daemon::{Daemon, DaemonOptions};
pub use paths::Paths;
