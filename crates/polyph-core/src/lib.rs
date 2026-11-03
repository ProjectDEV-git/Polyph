//! Polyph's room engine. No I/O: the daemon feeds it envelopes, ticks and
//! local actions, and carries out the effects it returns.

pub mod claims;
pub mod docs;
pub mod election;
pub mod room;
pub mod voice;

pub use room::{Effect, RoomCore, RoomError, RoomEvent, SavedRoom};
