//! Polyph's extension points, as plain data and files (no I/O beyond the
//! file system, no daemon types):
//!
//! - [`manifest`]: plugins, folders with a `polyph-plugin.toml` that give
//!   agents an MCP tool server and run hooks on room events. The daemon
//!   runs both inside the agent sandbox, with only the network hosts the
//!   manifest declares.
//! - [`store`]: plugins installed for one person on one device.
//! - [`skills`]: instruction folders (`<name>/SKILL.md`) that agents find
//!   through Polyph's MCP tools; shared with the repository in
//!   `.polyph/skills/`, or personal in the Polyph home.
//! - [`templates`]: starter `polyph.toml` and skills for a repository.

pub mod manifest;
pub mod skills;
pub mod store;
pub mod templates;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Exists(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Lower-case letters, digits and dashes, 1 to 40 characters, starting with a letter.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
