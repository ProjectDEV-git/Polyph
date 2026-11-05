//! The Polyph daemon. Runs on each person's machine, owns rooms and their
//! relay connections, and serves the local API used by the CLI, TUI, desktop
//! app, VS Code extension and status pill.

pub mod agents;
pub mod api;
pub mod areas;
pub mod boards;
pub mod clients;
pub mod clock;
pub mod cloud;
pub mod config;
pub mod connect_keys;
pub mod context;
pub mod coord;
pub mod daemon;
#[cfg(target_os = "linux")]
pub mod dbus;
pub mod events;
pub mod export;
pub mod external;
pub mod feed;
pub mod folders;
pub mod github;
pub mod hosts;
pub mod integrations;
pub mod lan;
pub mod linear;
pub mod mirror;
pub mod notify;
pub mod paths;
pub mod phone;
pub mod pickup;
pub mod plan;
pub mod plugins;
pub mod runtime;
pub mod secrets;
pub mod setup;
pub mod status;
pub mod sync;
pub mod tasks;
pub mod terms;
pub mod watch;
pub mod web;

pub use clock::Clock;
pub use daemon::{Daemon, DaemonOptions};
pub use paths::Paths;
