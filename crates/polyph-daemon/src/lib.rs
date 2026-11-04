//! The Polyph daemon. Runs on each person's machine, owns rooms and their
//! relay connections, and serves the local API used by the CLI, TUI, desktop
//! app, VS Code extension and status pill.

pub mod agents;
pub mod api;
pub mod boards;
pub mod clock;
pub mod config;
pub mod coord;
pub mod daemon;
#[cfg(target_os = "linux")]
pub mod dbus;
pub mod events;
pub mod export;
pub mod feed;
pub mod github;
pub mod integrations;
pub mod lan;
pub mod linear;
pub mod mirror;
pub mod notify;
pub mod paths;
pub mod plan;
pub mod provider;
pub mod runtime;
pub mod secrets;
pub mod status;
pub mod tasks;
pub mod terms;
pub mod watch;
pub mod web;

pub use clock::Clock;
pub use daemon::{Daemon, DaemonOptions};
pub use paths::Paths;
