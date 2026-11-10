//! The game run: made, stepped on a thread of its own, recorded, and
//! what the drawing sees of it.

pub(crate) mod begin;
pub(crate) mod bot;
pub(crate) mod clock;
pub(crate) mod drive;
pub(crate) mod load;
pub(crate) mod record;
pub(crate) mod scope;
pub(crate) mod seen;
pub(crate) mod setup;
pub(crate) mod sim;
pub(crate) mod speed;
#[cfg(test)]
mod test_game;
pub(crate) mod thread;
pub(crate) mod tune;
