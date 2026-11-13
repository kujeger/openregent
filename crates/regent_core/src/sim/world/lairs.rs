//! Lairs: what a spawner picks, an off-map lair's
//! edge, and what it holds back. **Binary** unless marked.
//!
//! ```text
//!  SpawnUnit from a lair   lair_data(unit handle): with entries, one
//!                          picked on a world draw (distribution pick);
//!                          an off-map lair's unit enters on its edge
//!                          (spawn edge, a GPL draw)
//!  UnitIsOffmapLair(agent) unit list 7: a piece at an OFF_MAP edge code
//!  FillSpecialLairList(agent, list)
//!    no unit       throw "FillSpecialLairList() unit is null"
//!    lair_data(unit handle), a default made on a miss
//!    list          emptied, then each code of the data's special array
//!                  with a descriptor: its HEAD name, a string, appended
//!    returns       the array's count, codes without a descriptor too
//! ```
//!
//! The unit chart place fills the special array from the chart
//! spawner's codes ([`super::new_map::Settings::special`]); no other code writes
//! it. `mx_Building_Deaths` spawns them as a lair falls.

use super::{Answer, Found, World, agent_ref, new_map::LairEntry, put_bool, put_int};
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::{Accessor, OpError, Value};
use crate::gpl::vm::{Fault, Handle, Vm};
use std::collections::BTreeMap;

/// Off-map lair x positions, 32nds of a cell: spawn edge draws a
/// point on an edge, which the world does not model.
pub(super) const OFF_MAP: std::ops::RangeInclusive<i32> = -1003..=-1000;

/// A lair entry or pick that names no unit.
pub(super) const NONE: FourCc = FourCc::new(b"NONE");

/// Distribution pick over a lair's entries: the first whose running
/// weight reaches the draw; off the end, the first.
pub(super) fn pick(entries: &[LairEntry], drawn: i32) -> Option<&LairEntry> {
    let mut seen = 0_i64;

    entries
        .iter()
        .find(|e| {
            seen += i64::from(e.weight);
            i64::from(drawn) <= seen
        })
        .or_else(|| entries.first())
}

/// A lair's weights summed, the pick's bound.
pub(super) fn total(entries: &[LairEntry]) -> i32 {
    entries
        .iter()
        .map(|e| e.weight.cast_signed())
        .fold(0, i32::wrapping_add)
}

/// The `APV:2` (Artifice) a picked mobile is made with (gpl spawn unit, after
/// the list's pairs): the entry's force, where not 0. Not for a building.
pub(super) fn artifice(entry: &LairEntry) -> Option<(i32, i32)> {
    (entry.force != 0).then(|| (Key::new(*b"APV", 2).packed(), entry.force.cast_signed()))
}

impl World {
    /// The units spawner `handle`'s lair data names: its entries'
    /// codes, then its special codes, `NONE` left out. Empty where it has
    /// no data: then its script names what it spawns.
    #[must_use]
    pub fn lair_units(&self, handle: u32) -> Vec<FourCc> {
        let picked = self.lairs.entries(handle).into_iter().flatten();

        picked
            .map(|e| e.code)
            .chain(self.lairs.special(handle).iter().copied())
            .filter(|code| *code != NONE)
            .collect()
    }

    /// `FillSpecialLairList(agent, list)`: the lair's special codes as
    /// descriptor names into `list`, their count returned.
    pub(super) fn special_lair(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let handle = match self.found(id) {
            Found::Piece(p) => Some(p.handle),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw("FillSpecialLairList() unit is null"))?,
        };
        let list = vm.arg(args, 2)?;

        if !matches!(vm.object(list), Value::List(_)) {
            Err(OpError::BadConversion(Accessor::ListRef))?;
        }

        match handle {
            Some(handle) => {
                let codes = self.lairs.special(handle).to_vec();
                let names = codes
                    .iter()
                    .filter_map(|c| self.descriptors.body(*c))
                    .map(|b| Value::String(b.name.clone()));

                if let Value::List(out) = vm.object_mut(list) {
                    out.clear();
                    out.extend(names);
                }

                let ret = vm.arg(args, 0)?;
                let count = i32::try_from(codes.len()).unwrap_or(i32::MAX);

                put_int(vm, ret, count).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `UnitIsOffmapLair(agent)`, gpl unit is offmap lair: false for an agent id below
    /// 0; throws with no unit; else whether unit list 7, `OffMapLairList`,
    /// holds it: a piece standing at an [`OFF_MAP`] edge code.
    pub(super) fn is_offmap_lair(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let listed = if id.cast_signed() < 0 {
            Some(false)
        } else {
            match self.found(id) {
                Found::Piece(_) => {
                    Some(self.position(id).is_some_and(|(x, _)| OFF_MAP.contains(&x)))
                }
                Found::Unsure => None,
                Found::Gone => Err(Fault::throw("UnitIsOffmapLair() unit is null"))?,
            }
        };

        match listed {
            Some(answer) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, answer).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// Spawn edge: a point on the edge off-map lair
    /// `x` names, 32nds: `-1000` north `(r(w - 1), 0)`, `-1001` east
    /// `(w - 1, r(h - 1))`, `-1002` south `(r(w - 1), h - 1)`, `-1003`
    /// west `(0, r(h - 1))`, each `r` a GPL draw; `w` and `h` the map in
    /// 32nds (**Inferred**: the level's map size turned 16.16,
    /// then `>> 11`).
    pub(super) const fn edge_point(&mut self, vm: &mut Vm<'_>, x: i32) -> (i32, i32) {
        let (w, h) = self.board.extent();

        match x {
            -1000 => (self.native_draw(vm, w - 1), 0),
            -1001 => (w - 1, self.native_draw(vm, h - 1)),
            -1002 => (self.native_draw(vm, w - 1), h - 1),
            _ => (0, self.native_draw(vm, h - 1)),
        }
    }
}

/// Each spawner's lair data, by handle: what `SpawnUnit` picks from, and
/// what `FillSpecialLairList` names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Lairs {
    /// Spawners whose lair data has entries: what `SpawnUnit` picks from,
    /// with weights, on a draw of the world RNG. The
    /// new game's `GSGL`, or the chart's lairs; any other spawns the
    /// script's name.
    picking: BTreeMap<u32, Vec<LairEntry>>,
    /// Each spawner's lair data: what `FillSpecialLairList`
    /// names ([`super::new_map::Settings::special`]).
    special: BTreeMap<u32, Vec<FourCc>>,
}

impl Lairs {
    /// What spawner `handle` picks from, where its lair data has entries.
    pub(super) fn entries(&self, handle: u32) -> Option<&Vec<LairEntry>> {
        self.picking.get(&handle)
    }

    /// Spawner `handle`'s special codes.
    pub(super) fn special(&self, handle: u32) -> &[FourCc] {
        self.special.get(&handle).map_or(&[], Vec::as_slice)
    }

    /// A chart lair's data: what it picks from, and holds back.
    pub(super) fn set(
        &mut self,
        handle: u32,
        entries: Vec<LairEntry>,
        special: Option<Vec<FourCc>>,
    ) {
        self.picking.insert(handle, entries);

        if let Some(special) = special {
            self.special.insert(handle, special);
        }
    }

    /// Every spawner's entries, by handle.
    pub(super) const fn picking(&self) -> &BTreeMap<u32, Vec<LairEntry>> {
        &self.picking
    }

    /// Every spawner's special codes, by handle.
    pub(super) const fn specials(&self) -> &BTreeMap<u32, Vec<FourCc>> {
        &self.special
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Lairs {
        type Model = m::Lairs;

        fn save(&self) -> m::Lairs {
            m::Lairs {
                picking: self.picking.save(),
                special: self.special.save(),
            }
        }

        fn load(model: m::Lairs) -> Result<Lairs, Refused> {
            Ok(Lairs {
                picking: BTreeMap::load(model.picking)?,
                special: BTreeMap::load(model.special)?,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Undead lairs' entries carry force 1, `#force_raider`: a unit
    /// their pick spawns is born with `APV:2` 1, which `Skeleton_Birth`
    /// reads to make it a raider. A force of 0 writes nothing.
    #[test]
    fn a_picked_entry_gives_its_force_as_artifice() {
        let skeleton = FourCc(*b"BVZ1");
        let entries = [
            LairEntry {
                code: skeleton,
                weight: 20,
                force: 1,
            },
            LairEntry {
                code: NONE,
                weight: 0,
                force: 0,
            },
        ];
        let picked = pick(&entries, 5).expect("an entry");

        assert_eq!(picked.code, skeleton);
        assert_eq!(artifice(picked), Some((Key::new(*b"APV", 2).packed(), 1)));
        assert_eq!(artifice(&entries[1]), None);
        assert_eq!(total(&entries), 20);
    }
}
