//! A room member that runs in the browser (the phone PWA): `member` is the
//! sans-I/O engine, `wasm` its JavaScript bindings.

pub mod member;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use member::{Member, Output};
