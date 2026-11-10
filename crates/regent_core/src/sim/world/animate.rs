//! A building's looping `Animate`s: the order that keeps a building in
//! ORDR while its set plays in frames. **Binary** unless marked.
//!
//! ```text
//!  the building's damage set (unit, forced)
//!    unforced  GSUnit, every write; the new game's pass over
//!              BuildingList; a placed building's creation;
//!              the upgrade
//!    forced    GSUnit: an InitAction's end, a unit entering
//!              or leaving (GE_OrderProcessor)
//!    └─ the building's set play: nothing while track 1 holds
//!       an Animate showing the set
//!       └─ the unit's set play: track 1's Animates off, then
//!          an Animate (GE_OrderProcessor, flags 0x50, loops for ever)
//!          per part keyed 0 whose slot has more than one frame
//!  a stop (GenUnit) takes every Animate off
//! ```
//!
//! **Game**: every save's buildings in ORDR are those whose set has a
//! layer of more frames than one, or that hold a timed order.

use std::collections::BTreeMap;

use super::{Listed, World, new_map::OnGrid};
use crate::data::fourcc::FourCc;
use crate::data::imag::{Image, SET_BASE};

/// The bound of a start frame's draw: the state steps the same for any,
/// and the frame is not kept.
const START_FRAME_BOUND: usize = 2;

/// Each set's layers whose parts take an `Animate`, by base id: each
/// layer's frames per slot.
pub type Animated = BTreeMap<u32, Vec<Vec<u16>>>;

/// The layers of `image`'s sets that give parts (`UnitIMG`: layer
/// flag bits 8 and 9) keyed 0 ([`crate::data::imag::Set::part_key`]),
/// in the order set layer at finds them.
#[must_use]
pub fn animated(image: &Image<'_>) -> Animated {
    let mut out = Animated::new();

    for set in &image.sets {
        let parts = super::plume::PART_BITS
            .iter()
            .any(|b| set.layer_flags() & b != 0);

        if parts && set.part_key() == 0 {
            out.entry(set.id & SET_BASE)
                .or_default()
                .push(set.slots.iter().map(|s| s.frames).collect());
        }
    }

    out
}

impl World {
    /// Whether set `set` gives piece `id` an `Animate`: a layer's slot
    /// at its facing has more than one frame.
    fn loops_on(&self, id: u32, set: u32) -> bool {
        let facing = u16::try_from(self.facing_word(id)).unwrap_or_default();
        let body = self.body_of(id);

        body.and_then(|b| Some((b.facing, b.animated.get(&set)?)))
            .is_some_and(|(turn, layers)| {
                layers
                    .iter()
                    .any(|f| f.get(turn.slot(facing, f.len())).is_some_and(|n| *n > 1))
            })
    }

    /// Unit play set's `Animate`s for building `id` playing
    /// `set`: the old off, a new one where the set loops, joining the
    /// back of ORDR.
    pub(super) fn play_loop(&mut self, id: u32, set: u32) {
        if let Some(u) = self.units.get_mut(id) {
            u.looping = false;
        }

        if self.loops_on(id, set) {
            self.units.entry(id).looping = true;

            self.dispatches.list(Listed::Unit(id));
        }
    }

    /// Whether building `id`'s track 1 holds an `Animate` of `set`.
    pub(super) fn shows(&self, id: u32, set: u32) -> bool {
        self.units
            .get(id)
            .is_some_and(|u| u.looping && u.played == Some(set))
    }

    /// Whether a `code` unit made in its first set (`Unit::initial`)
    /// plays frames at `slot`: a layer's slot of more frames than one.
    pub(super) fn plays_frames(&self, code: FourCc, slot: u16) -> bool {
        self.descriptors
            .body(code)
            .and_then(|b| b.animated.get(&(b.made_in? & SET_BASE)))
            .is_some_and(|layers| {
                layers
                    .iter()
                    .any(|f| f.get(usize::from(slot)).is_some_and(|n| *n > 1))
            })
    }

    /// New game setup's pass over `TerrainFeatureList`, unit list
    /// 2, after `Palace_Pre_Birth`: unit play set
    /// of set 0 on each scatter object and terrain feature, and where it
    /// pushed an `Animate` a world draw for a start frame. So one draw per
    /// `Board::scenery` piece no ring cleared; the frame is not kept.
    /// **Game**: the Liche Queen's keep picks the log's skeleton only
    /// after its 49 (**Measured**).
    ///
    /// `standing` is the scatter as the setup began: a ring the setup's
    /// spawns clear only queues its deletes, drained in
    /// frame 1 (`GS_Level`), so the pass still finds and draws
    /// for them. **Game**: The Barren Waste's chests clear two looping
    /// pieces, ten draws, not eight, to its 0-day save.
    pub fn start_frames(&mut self, standing: &[(FourCc, (i32, i32))]) {
        let mut counts: BTreeMap<(FourCc, (i32, i32)), usize> = BTreeMap::new();

        for key in standing {
            *counts.entry(*key).or_default() += 1;
        }

        let drawn = self
            .board
            .scenery()
            .iter()
            .filter(|(code, at, grid)| match grid {
                OnGrid::Off => true,
                OnGrid::On => match counts.get_mut(&(*code, *at)) {
                    Some(n) if *n > 0 => {
                        *n -= 1;
                        true
                    }
                    _ => false,
                },
            })
            .count();

        for _ in 0..drawn {
            self.slot_draw(START_FRAME_BOUND);
        }
    }

    /// New game setup's pass over `BuildingList`, after
    /// `Palace_Pre_Birth`: each building's set, unforced, in the order
    /// made.
    pub fn start_sets(&mut self) {
        let mut made: Vec<(u32, u32)> = self
            .pieces
            .iter()
            .filter(|(_, p)| self.descriptors.class(p.code) == Some(super::bodies::Class::Building))
            .map(|(id, p)| (p.handle, *id))
            .collect();

        made.sort_unstable();

        for (_, id) in made {
            self.restyle(id, super::plume::Force::Unforced);
        }
    }
}
