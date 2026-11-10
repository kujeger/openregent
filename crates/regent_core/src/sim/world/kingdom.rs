//! The palace's rosters (`AP85`, `APc1`, `APc2`) and the henchmen its
//! level supports. **Binary** unless marked.
//!
//! ```text
//!  roster     panel  controller                  rows (each)
//!  heroes     AP85   Top5Controller              unit list 1: the palace
//!                                                owner's heroes, none held
//!                                                by a Mausoleum (ABp)
//!  buildings  APc1   KingdomBuildingsController  unit list 0: the owner's
//!                                                with an agent (AE:0 not
//!                                                0, so no ruin), not
//!                                                Sewers (ABN1) or a
//!                                                Graveyard (BBJ1)
//!  henchmen   APc2   HenchmenListController      unit list 1: the owner's
//!                                                henchmen
//!
//!  sort   a sorted insert into the unit list, by the
//!         row's mode: heroes, buildings and henchmen each its own
//!         (heroes: hero list sort)
//!    1 name    the panel name, flags 0, case kept, ascending
//!    2 level   APV:11, highest first
//!    3 type    a building's guild flag first,
//!              then the code as a little-endian word, ascending
//!    0         an uninitialised byte: no order
//!
//!  PalaceController (palace henchmen fill): controls 14, 13, 12 the answers of
//!    GetSupportedHenchmen(palace, 0, 1, 2), an interface dispatch:
//!    max_peasants; max_tax_collectors + the owner's fountains; Max_guards
//! ```

use crate::sim::world::kind;
use std::cmp::Ordering;

use super::{AGENT_ID, World};
use crate::data::fourcc::FourCc;

/// The GPL function the palace's counts ask: palace henchmen fill pushes it.
pub const SUPPORTED_HENCHMEN: &str = "GetSupportedHenchmen";

/// `GetSupportedHenchmen`'s index, and the palace control its answer
/// fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Henchmen {
    /// `max_peasants`, control 14.
    Peasants,
    /// `max_tax_collectors` and one a fountain, control 13.
    TaxCollectors,
    /// `Max_guards`, control 12.
    Guards,
}

impl Henchmen {
    /// Every index, in the dispatch's order.
    pub const ALL: [Henchmen; 3] = [
        Henchmen::Peasants,
        Henchmen::TaxCollectors,
        Henchmen::Guards,
    ];

    /// The function's index argument.
    #[must_use]
    pub const fn index(self) -> i32 {
        self as i32
    }

    /// The `AP39` text box the answer goes to.
    #[must_use]
    pub const fn control(self) -> i32 {
        match self {
            Henchmen::Peasants => 14,
            Henchmen::TaxCollectors => 13,
            Henchmen::Guards => 12,
        }
    }
}

/// Which roster a page lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Roster {
    #[default]
    Heroes,
    Buildings,
    Henchmen,
}

/// A roster's sort, the radios 5005 to 5007.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sort {
    /// Mode 0, the start: no order. **Inferred**: the engine's insert
    /// under an uninitialised answer; the list's order is kept.
    #[default]
    Unset,
    Name,
    Level,
    Type,
}

/// What a row sorts by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sorted {
    pub id: u32,
    /// The panel name's.
    pub name: String,
    /// `APV:11`.
    pub level: i32,
    pub code: FourCc,
    /// A building whose descriptor has bit 1: a guild.
    pub guild: bool,
}

/// Sewers and the Graveyard, which the buildings roster skips.
const UNLISTED: [[u8; 4]; 2] = [*b"ABN1", *b"BBJ1"];

/// The Mausoleum's code prefix: a hero it holds is off the roster.
const MAUSOLEUM: [u8; 3] = *b"ABp";

/// Unit list less: whether `a` goes before `b` under `sort`.
#[must_use]
pub fn before(sort: Sort, a: &Sorted, b: &Sorted) -> bool {
    match sort {
        Sort::Name => a.name.as_bytes() < b.name.as_bytes(),
        Sort::Level => a.level > b.level,
        Sort::Type => match (a.guild, b.guild) {
            (true, false) => true,
            (false, true) => false,
            _ => a.code.word() < b.code.word(),
        },
        Sort::Unset => false,
    }
}

/// `rows` in `sort`'s order; ties and [`Sort::Unset`] keep theirs
/// (**Inferred**).
pub fn sort(rows: &mut [Sorted], sort: Sort) {
    rows.sort_by(|a, b| {
        if before(sort, a, b) {
            Ordering::Less
        } else if before(sort, b, a) {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });
}

impl World {
    /// The agents palace `palace`'s roster lists, in the unit list's order
    /// (**Inferred**: agent order), before the sort: each controller's
    /// method.
    #[must_use]
    pub fn roster_of(&self, palace: u32, roster: Roster) -> Vec<u32> {
        let owner = self.pieces.owner(palace);
        let kind = |id: u32| self.known(id).map(|p| p.kind);
        let interred = |id: u32| {
            self.known(id)
                .filter(|p| p.holder != 0)
                .and_then(|p| self.holder(p.holder))
                .and_then(|h| self.pieces.get(h))
                .is_some_and(|h| h.code.0[..3] == MAUSOLEUM)
        };

        let mut listed: Vec<u32> = self
            .pieces
            .iter()
            .filter(|(_, p)| Some(p.owner) == owner)
            .filter(|(id, p)| match roster {
                Roster::Buildings => {
                    kind(**id) == Some(kind::Kind::BUILDING)
                        && p.get(AGENT_ID).is_some_and(|a| a != 0)
                        && !UNLISTED.contains(&p.code.0)
                }
                Roster::Heroes => {
                    kind(**id) == Some(kind::Kind::VEHICLE) && self.is_hero(**id) && !interred(**id)
                }
                Roster::Henchmen => kind(**id) == Some(kind::Kind::VEHICLE) && self.henchman(**id),
            })
            .map(|(id, _)| *id)
            .collect();

        listed.sort_unstable();
        listed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u32, name: &str, level: i32, code: [u8; 4]) -> Sorted {
        Sorted {
            id,
            name: name.to_owned(),
            level,
            code: FourCc(code),
            guild: false,
        }
    }

    fn sorted(rows: &[Sorted], by: Sort) -> Vec<u32> {
        let mut rows = rows.to_vec();

        sort(&mut rows, by);
        rows.iter().map(|r| r.id).collect()
    }

    #[test]
    fn a_roster_sorts_by_name_level_or_type_as_the_less_test_says() {
        let rows = [
            row(1, "Palace", 0, *b"ABJ1"),
            Sorted {
                guild: true,
                ..row(2, "Warriors Guild", 2, *b"ABH1")
            },
            row(3, "Blacksmith Level 2", 1, *b"ABC2"),
            row(4, "blacksmith", 5, *b"ABC1"),
        ];

        // Case kept: capitals before small letters.
        assert_eq!(sorted(&rows, Sort::Name), [3, 1, 2, 4]);
        assert_eq!(sorted(&rows, Sort::Level), [4, 2, 3, 1]);
        // The guild first, then the codes as words, the last letter
        // highest: ABC1 < ABJ1 < ABC2.
        assert_eq!(sorted(&rows, Sort::Type), [2, 4, 1, 3]);
        assert_eq!(sorted(&rows, Sort::Unset), [1, 2, 3, 4]);
    }

    #[test]
    fn the_palace_counts_go_to_the_controls_the_dispatch_fills() {
        let filled: Vec<(i32, i32)> = Henchmen::ALL
            .iter()
            .map(|h| (h.index(), h.control()))
            .collect();

        assert_eq!(filled, [(0, 14), (1, 13), (2, 12)]);
    }
}
