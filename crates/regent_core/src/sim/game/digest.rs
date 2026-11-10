//! A game's state as one word, for comparing runs and peers.
//!
//! ```text
//!  digest       frames, GPL RNG, World::digest       cheap, every frame
//!  deep_digest  the snapshot's VM, world and game    costly, the lot
//!               but the local parts
//!  deep_fold    the fold alone, of a snapshot taken  any thread's
//! ```
//!
//! Left out of both, as one machine's own: inputs still to come, and the
//! script error count.

use std::hash::Hasher;

use super::Game;
use crate::base::digest::{Fold, digest};
use crate::save::{self as m, Refused};

impl Game<'_> {
    /// Frames run, the GPL RNG and [`crate::sim::world::World::digest`]:
    /// cheap enough for every frame.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut fold = Fold::new(0);

        fold.write_u32(self.frames);
        fold.write_u32(self.vm.rng);
        fold.write_u64(self.world().digest());
        fold.finish()
    }
}

impl Game<'_> {
    /// The whole snapshot ([`Game::snapshot`]) folded, but for the local
    /// parts (module doc): VM, world, frames, queues, attribute
    /// objects and seed.
    ///
    /// # Errors
    ///
    /// As [`Game::snapshot`]: a game mid-run.
    pub fn deep_digest(&self) -> Result<u64, Refused> {
        self.snapshot().map(|s| deep_fold(&s))
    }
}

/// `snapshot` folded as [`Game::deep_digest`] folds it: all but the
/// local parts. Apart from the snapshot, so another thread may fold it.
#[must_use]
pub fn deep_fold(snapshot: &m::Snapshot) -> u64 {
    let m::Snapshot {
        version,
        vm,
        world,
        game,
    } = snapshot;
    let m::Game {
        frames,
        queues,
        inputs: _,
        host,
    } = game;
    let m::Host {
        attributes,
        seed,
        reports: _,
    } = host;

    digest(&(version, vm, world, frames, queues, attributes, seed))
}
