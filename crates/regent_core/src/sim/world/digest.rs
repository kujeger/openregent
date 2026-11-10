//! A cheap digest of the world's play state: two runs, or two peers, that
//! agree on it are very likely in one state.
//!
//! ```text
//!  digest = fold(clock, world RNG, Σ piece, each player's gold)
//!  piece  = fold(agent, owner, handle, code, attributes, spells, place)
//! ```
//!
//! The pieces are summed, not folded in turn, so no map order can enter.
//! `Game::deep_digest` covers the rest of the state.

use std::hash::{Hash, Hasher};

use super::{Piece, World};
use crate::base::digest::Fold;

/// A treasury the world does not know.
const NO_GOLD: i32 = i32::MIN;

impl World {
    /// The clock, the world RNG, every piece, and each player's gold,
    /// folded into one word. Same on every machine for the same state.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut fold = Fold::new(0);

        fold.write_u32(self.now);
        fold.write_u32(self.rngs.state());

        let pieces = self.pieces.all().fold(0u64, |sum, (id, p)| {
            sum.wrapping_add(self.piece_digest(*id, p))
        });

        fold.write_u64(pieces);

        for &slot in self.players.slots() {
            fold.write_u32(slot);
            fold.write_i32(self.players.gold(slot).unwrap_or(NO_GOLD));
        }

        fold.finish()
    }

    /// Piece `id`'s own word: what it is, whose, its attributes and
    /// spells, and where it is now.
    fn piece_digest(&self, id: u32, piece: &Piece) -> u64 {
        let mut fold = Fold::new(0);

        fold.write_u32(id);
        fold.write_u32(piece.owner);
        fold.write_u32(piece.handle);
        piece.code.hash(&mut fold);
        piece.attributes.hash(&mut fold);

        for charge in piece.spells.iter().flatten() {
            charge.code.hash(&mut fold);
            fold.write_u32(charge.since);
            fold.write_u32(charge.timeout);
        }

        self.unit_pos(id).hash(&mut fold);
        fold.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pieces count whatever order they went in; a changed attribute or
    /// RNG changes the digest.
    #[test]
    fn a_digest_follows_the_state_not_the_order() {
        let (mut a, mut b) = (World::default(), World::default());

        a.add_bare_piece(1);
        a.add_bare_piece(2);
        b.add_bare_piece(2);
        b.add_bare_piece(1);

        assert_eq!(a.digest(), b.digest());

        let before = a.digest();

        if let Some(p) = a.pieces.get_mut(1) {
            p.attributes.insert(7, 1);
        }

        assert_ne!(a.digest(), before);

        b.rngs.seed(5);

        assert_ne!(b.digest(), World::default().digest());
    }
}
