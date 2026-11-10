//! The Temple to Krolm's Rage of Krolm. **Binary** unless marked.
//!
//! ```text
//!  panel AP24, control 8009 RAGE OF KROLM (hot key K), price 4484,
//!    bar 8201, text 8826
//!    the panel state: APB:2 0 hides all four; the
//!      palace's APB:7 set: 8201 a bar of now - the palace's APB:52 over
//!      #Rage_Krolm_Duration (#Deathmatch_.. at victory index 3), 8826
//!      shown, 8009 and 4484 hidden; else 8009 and 4484 (red under the
//!      treasury), 8009 enabled while the owner has a hero
//!      (owner hero count)
//!         #RageOfKrolmCost in the treasury: PlayerResourceMessage
//!      spend, GUILD_ACTION 1 (the temple's handle, its point), 8009
//!      disabled at once; else GMTX 36 "Can't afford to Rage."
//!  decode guild action apply, case 1: the temple
//!    with an agent, its owner's palace with APB:7 0: the palace's APB:52
//!    the clock, then DoRageOfKrolm(temple)
//! ```
//!
//! `DoRageOfKrolm` is the effect: each living hero of the owner gets a
//! `Rage_Icon` effector of the duration, +10 strength, hand to hand,
//! dodge and parry (magical), +10 hit points and maximum, a speed trail;
//! the palace's `APB:7` its hero count. Each icon's end runs `Rage_end`,
//! which undoes it and takes 1 off `APB:7`. A refused press keeps its
//! price (**Inferred**: no message gives it back).

use crate::data::attrib::{CURRENT_STAGE_BUILT, Key};
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::sim::world::{World, offer::Button};

/// What case 1 dispatches: the string `"DoRageOfKrolm"`.
pub const RAGE: &str = "DoRageOfKrolm";

/// The Rage's price, read once into rage cost.
pub const RAGE_COST: &str = "#RageOfKrolmCost";

/// The bar's length, read once.
pub const RAGE_DURATION: &str = "#Rage_Krolm_Duration";

/// The bar's length in a deathmatch.
pub const DEATHMATCH_RAGE_DURATION: &str = "#Deathmatch_Rage_Krolm_Duration";

/// The victory index whose game is a deathmatch (a compare with 3,
/// rage panel state).
pub const DEATHMATCH: i32 = 3;

/// The temple's code prefix: `ABS1` `Temple_Krolm`.
pub const KROLM_TEMPLE: [u8; 3] = *b"ABS";

/// `GMTX` 36, `Can't afford to Rage.`: the press's line.
pub const NO_GOLD_RAGE: usize = 36;

/// The palace's `APB:7` `RageOfKrolmCount` and `APB:52` `RageBeginTime`.
pub(in crate::sim::world) const RAGE_COUNT: Key = Key::new(*b"APB", 7);
const RAGE_BEGAN: Key = Key::new(*b"APB", 52);

/// The Rage controls as rage panel state sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rage {
    /// 8009 and its price 4484 shown.
    Ready(Button<()>),
    /// 8201 and 8826 shown: `elapsed` of `duration` game ms.
    Running { elapsed: i32, duration: i32 },
}

impl World {
    /// The Rage controls of `owner`'s temple `id`: `None` before its stage
    /// is built, for another's piece or any other building.
    #[must_use]
    pub fn rage(&self, program: &Program, owner: u32, id: u32) -> Option<Rage> {
        let piece = self.pieces.owned(id, owner, &KROLM_TEMPLE)?;
        let built = piece.get(CURRENT_STAGE_BUILT).is_some_and(|v| v != 0);
        let palace = self.player_palace(owner);
        let word = |k| palace.map_or(0, |p| self.word_of(p, k));
        let duration = if self.level.victory == DEATHMATCH {
            DEATHMATCH_RAGE_DURATION
        } else {
            RAGE_DURATION
        };

        match (built, word(RAGE_COUNT)) {
            (false, _) => None,
            (true, 0) => Some(Rage::Ready(Button {
                what: (),
                price: constant(program, RAGE_COST),
                ready: self.hero_count(owner) > 0,
            })),
            (true, _) => Some(Rage::Running {
                elapsed: self.now.cast_signed().wrapping_sub(word(RAGE_BEGAN)),
                duration: constant(program, duration),
            }),
        }
    }

    /// `owner`'s Temple to Krolm whose stage is built, the lowest id
    /// where several: a Rage pressed outside its panel (openregent's spell
    /// bar, an enhancement).
    #[must_use]
    pub fn rage_temple(&self, owner: u32) -> Option<u32> {
        self.pieces
            .iter()
            .filter(|(_, p)| p.owner == owner && p.code.0.starts_with(&KROLM_TEMPLE))
            .filter(|(_, p)| p.get(CURRENT_STAGE_BUILT).is_some_and(|v| v != 0))
            .map(|(id, _)| *id)
            .min()
    }

    /// Rage dispatch on temple `id`: whether it dispatches
    /// [`RAGE`]. With an agent and its owner's palace's `APB:7` 0, the
    /// palace's `APB:52` takes the clock first.
    pub fn rage_now(&mut self, id: u32) -> bool {
        let palace = self
            .agent(id)
            .and(self.pieces.get(id))
            .and_then(|p| self.player_palace(p.owner));
        let idle = palace.filter(|p| self.pieces.attribute(*p, RAGE_COUNT).is_none_or(|v| v == 0));

        if let Some(p) = idle {
            let now = self.now.cast_signed();

            self.write(p, RAGE_BEGAN.packed(), now);
        }

        idle.is_some()
    }

    /// Owner hero count: `owner`'s mobiles of unit list 1 whose
    /// agent's `subtype` is hero, lying dead too.
    fn hero_count(&self, owner: u32) -> usize {
        self.pieces
            .iter()
            .filter(|(id, p)| p.owner == owner && self.is_hero(**id))
            .count()
    }
}

/// An integer constant of the program's, 0 where none.
fn constant(program: &Program, name: &str) -> i32 {
    match program.constant(name) {
        Some(Literal::Int(v)) => v,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::Piece;

    const TEMPLE: u32 = 20;
    const PALACE: u32 = 4;

    fn world() -> World {
        let mut world = World::default();

        world.pieces.insert(TEMPLE, Piece::default());
        world.pieces.insert(
            PALACE,
            Piece {
                code: crate::data::fourcc::FourCc(*b"ABJ1"),
                handle: 9,
                ..Piece::default()
            },
        );
        world
            .agents
            .insert(TEMPLE, crate::sim::world::Agent::default());
        world
    }

    /// The bar's Rage needs a built temple of the owner's.
    #[test]
    fn the_rage_temple_is_the_owners_built_one() {
        let mut world = world();
        let temple = |owner, built| {
            let mut piece = Piece {
                code: crate::data::fourcc::FourCc(*b"ABS1"),
                owner,
                ..Piece::default()
            };

            piece.attributes.insert(CURRENT_STAGE_BUILT.packed(), built);
            piece
        };

        world.pieces.insert(30, temple(1, 1));
        world.pieces.insert(31, temple(0, 0));
        assert_eq!(world.rage_temple(0), None);

        world.pieces.insert(32, temple(0, 1));
        assert_eq!(world.rage_temple(0), Some(32));
        assert_eq!(world.rage_temple(1), Some(30));
    }

    /// A Rage with none running stamps the palace's `APB:52`; one
    /// running (`APB:7` set) dispatches nothing.
    #[test]
    fn a_rage_runs_only_while_none_does() {
        let mut world = world();

        world.now = 149_150;
        assert!(world.rage_now(TEMPLE));
        assert_eq!(world.pieces[&PALACE].get(RAGE_BEGAN), Some(149_150));

        world.write(PALACE, RAGE_COUNT.packed(), 3);
        assert!(!world.rage_now(TEMPLE));
    }
}
