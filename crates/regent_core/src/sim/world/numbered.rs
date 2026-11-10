//! Numbered names, "Wizards Tower #2": the engine's, not the scripts'.
//! **Binary** unless marked.
//!
//! ```text
//!  a player's statistics, saved in GSGL (gsgl::Record):
//!    bought         key -> count  as placed (stat built); a recruit's
//!                                 decode (stat hired); a henchman's
//!                                 creation (stat henchman), before
//!                                 AdjustPlayerData's spend on a
//!                                 hero or building type
//!    gained         key -> count  the unit chart place, before the
//!                                 unit is made; SetUnitPlayerNumber
//!  key   the stat key: AVj2 Peasant_female as AVj1 Peasant; a building
//!        its upgrade root
//!       no name yet, no NameGenType
//!    └─ Flags NumberedName only: GMTX 184 "%s #%d", the owner's
//!         bought + gained under the key; the unit keeps it as its
//!         own name, SECD's
//!         BuildingRec: the root's HEAD title and key
//!         VehicleRec: its own, "Tax Collector #1"
//! ```
//!
//! So a number counts every unit of the key the owner ever placed, hired or
//! got, itself included; none is given back. An upgrade or
//! `ChangeUnitType` keeps the name (the rename skips a named unit). A hero draws a name instead.

use std::collections::BTreeMap;

use super::hire::is_subtype;
use super::{Agent, World, bodies::Class};
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::data::desc::{BuildingFlag, CharacterFlag, UnitKind};
use crate::data::fourcc::FourCc;
use crate::gpl::load::Program;

/// The `subtype` unit class of classes as a hero, 1.
const HERO: &str = "hero";

/// How far upgrade root walks `UpgradeFrom`.
const UPGRADE_STEPS: usize = 2;

/// `Peasant_female`, which stat key counts as a [`PEASANT`].
const PEASANT_FEMALE: FourCc = FourCc(*b"AVj2");

/// `Peasant`.
const PEASANT: FourCc = FourCc(*b"AVj1");

/// Which of a player's two counts a making adds to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tally {
    /// Placed or hired.
    Bought,
    /// From the map's chart, or given.
    Gained,
}

/// Every player's two counts by key, as `GSGL` saves them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tallies(pub(super) BTreeMap<u32, [BTreeMap<FourCc, i32>; 2]>);

/// Upgrade root: the code `code`'s `UpgradeFrom` chain starts at,
/// at most `UPGRADE_STEPS` up.
#[must_use]
pub fn upgrade_root(catalogue: &Catalogue<'_>, code: FourCc) -> FourCc {
    // The descriptor alone: the art is no part of the chain.
    let from = |at: FourCc| {
        Unit::descriptor(catalogue, at)?
            .ok()?
            .building()?
            .upgrade_from()
    };

    // A root stays one: the walk stops where no link goes on.
    (0..UPGRADE_STEPS)
        .try_fold(code, |at, _| from(at).ok_or(at))
        .unwrap_or_else(|root| root)
}

/// Stat key: the key a count of `code` is kept under.
#[must_use]
pub fn tally_key(catalogue: &Catalogue<'_>, code: FourCc) -> FourCc {
    let building = Unit::descriptor(catalogue, code)
        .and_then(Result::ok)
        .is_some_and(|d| d.kind() == UnitKind::Building);

    match code {
        PEASANT_FEMALE => PEASANT,
        _ if building => upgrade_root(catalogue, code),
        _ => code,
    }
}

/// The key method numbers `unit` by: a building's root's,
/// a character's own; `None`
/// without `NumberedName`.
#[must_use]
pub fn numbered_key(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> Option<FourCc> {
    let code = unit.desc.head.id;
    let building = unit
        .desc
        .building()
        .filter(|b| b.has(BuildingFlag::NumberedName))
        .map(|_| upgrade_root(catalogue, code));
    let character = unit
        .desc
        .character()
        .filter(|c| c.has(CharacterFlag::NumberedName))
        .map(|_| code);

    building.or(character).map(|k| tally_key(catalogue, k))
}

impl World {
    /// One more of piece `code` for `owner`, under its body's key.
    pub(super) fn tally(&mut self, owner: u32, code: FourCc, how: Tally) {
        if let Some(key) = self.descriptors.body(code).and_then(|b| b.tally) {
            self.players.tally(owner, key, how);
        }
    }

    /// A converted piece of `code` counted, gpl set unit player number: the engine
    /// counts it for its machine's human, so here for every
    /// human seat ([`World::humans`]) and none differs by machine.
    pub(super) fn tally_converted(&mut self, code: FourCc) {
        for seat in self.humans() {
            self.tally(seat, code, Tally::Gained);
        }
    }

    /// `AdjustPlayerData`'s count of the descriptor named `ty`: bought,
    /// where a hero or a building, and
    /// a statistic of `price`.
    pub(super) fn count_spend(&mut self, program: &Program, owner: u32, (ty, price): (&str, i32)) {
        let found = self
            .descriptors
            .body_named(ty)
            .map(|(code, b)| (code, b.class));
        let hero = || Agent::made(program, ty).is_ok_and(|a| is_subtype(&a, HERO));

        match found {
            Some((code, _)) if hero() => {
                self.tally(owner, code, Tally::Bought);
                self.count_hired(owner, price);
            }
            Some((code, Class::Building)) => {
                self.tally(owner, code, Tally::Bought);
                self.count_built(owner, price);
            }
            _ => {}
        }
    }

    /// method on piece `id`: its owner's count of its key as its number,
    /// where its descriptor is `NumberedName`.
    pub(super) fn number(&mut self, id: u32) {
        let counted = self.pieces.get(id).and_then(|p| {
            let key = self.descriptors.body(p.code)?.numbered?;

            Some(self.players.total(p.owner, key))
        });

        if let Some(n) = counted {
            self.units.entry(id).number = Some(n);
        }
    }

    /// Agent `id`'s number, where it has a numbered name.
    #[must_use]
    pub fn numbered(&self, id: u32) -> Option<i32> {
        self.units.get(id).and_then(|u| u.number)
    }

    /// Every player's counts.
    #[must_use]
    pub fn tallies(&self) -> Tallies {
        self.players.tallies()
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Tallies {
        type Model = BTreeMap<u32, [BTreeMap<m::Code, i32>; 2]>;

        fn save(&self) -> Self::Model {
            self.0.save()
        }

        fn load(model: Self::Model) -> Result<Tallies, Refused> {
            BTreeMap::load(model).map(Tallies)
        }
    }
}
