//! The custom-game dialog's picks resolved ([`Picks`] to [`Choice`]), and
//! the art a generated map is made by.
//!
//! ```text
//!  picks, else DEFAULTS ──> chosen ──> Choice
//!  catalogue ──> Art: footprints, slot counts, variants, descriptors
//! ```

use std::collections::BTreeMap;

use crate::content::terrain::Terrain;
use crate::content::terrain::grid::Cell;
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::data::fourcc::FourCc;
use crate::data::rgs::Map;
use crate::sim::footprint;
use crate::sim::freestyle::{self, Choice, Picks};
use crate::sim::setup;
use crate::sim::world::descriptors::Descriptors;

use crate::sim::mapgen::mobject;

/// The picks a game with no template takes where none is given: the start
/// trace's (`freestyle_seed1_S_hd`), Wizard Hold (Beginner), Ruins of
/// Esterhausen - S, no Wandering Monsters, K'Hal-Kratis, Map Number 1. Not
/// the dialog's own first choices, which are not read.
pub const DEFAULTS: (FourCc, FourCc, FourCc, FourCc, i32) = (
    FourCc::new(b"Wizb"),
    FourCc::new(b"Ruib"),
    FourCc::new(b"NONE"),
    FourCc::new(b"KxHa"),
    1,
);

/// The least Map Number: the dialog raises a lower one to it, and the
/// map is drawn from that ([`crate::sim::freestyle::Setup::SEED_MIN`]).
pub const MAP_NUMBER_MIN: i32 = 1;

/// The picks with no template: [`DEFAULTS`] where not given, the Map
/// Number at least [`MAP_NUMBER_MIN`], a Random
/// pick drawn for the picks' players ([`freestyle::random`]), the side
/// the scenario's record gives (freestyle setup; see
/// `crate::data::rgs::map::UnitPattern::side`).
///
/// # Errors
///
/// A scenario the quest does not list, a Random pick with nothing to
/// draw, or a side the generator does not make ([`crate::sim::mapgen::world::sided`]).
pub fn chosen(picks: &Picks, map: &Map<'_>) -> Result<Choice, String> {
    let (settlement, scenario, monsters, land, seed) = DEFAULTS;
    let asked = [
        picks.land.unwrap_or(land),
        picks.settlement.unwrap_or(settlement),
        picks.scenario.unwrap_or(scenario),
        picks.monsters.unwrap_or(monsters),
    ];
    let seed = picks.seed.unwrap_or(seed).max(MAP_NUMBER_MIN);
    let [land, settlement, scenario, monsters] =
        freestyle::random(map, asked, seed, picks.players())
            .map_err(|code| format!("a Random {code} pick with nothing to draw"))?;
    let side = match picks.side {
        Some(side) => side,
        None => map.units[1]
            .iter()
            .find(|u| u.faction == scenario)
            .map(crate::data::rgs::map::UnitPattern::side)
            .ok_or_else(|| format!("the quest lists no scenario {scenario}"))?,
    };
    let side = crate::sim::mapgen::world::sided(side).map_err(|e| e.to_string())?;

    Ok(Choice {
        land,
        settlement,
        scenario,
        monsters,
        seed,
        side,
    })
}

/// A four-character code from text.
///
/// # Errors
///
/// Text that is not four bytes.
pub fn code(text: &str) -> Result<FourCc, String> {
    <[u8; 4]>::try_from(text.as_bytes())
        .map(|tag| FourCc::new(&tag))
        .map_err(|_| format!("{text} is not a four-character code"))
}

/// Per-unit art data: scatter footprint, slot counts, and what the
/// library's world measures a unit by.
#[derive(Default)]
pub struct Art {
    pub reaches: BTreeMap<FourCc, mobject::Reach>,
    /// Slot count by descriptor and set id (low 16 bits, as in
    /// `crate::sim::footprint`). The RNG bound for `GSUnit`;
    /// see [`crate::sim::mapgen::animate`].
    pub slots: BTreeMap<(FourCc, u32), u16>,
    /// The set a new unit is made in ([`Unit::initial`]), whole as its
    /// last layer's record names it ([`setup::recorded`]), and its slot
    /// count.
    pub initial: BTreeMap<FourCc, (u32, u16)>,
    /// The `variant`s a cell may take, by pair and case: a uniform pair's
    /// plain grid columns, a blend case's variant indices. From the widest
    /// `DTRN`'s tileset, as the viewer takes it.
    pub variants: BTreeMap<(u8, u8), Vec<u16>>,
    /// What the world reads of each descriptor: pad rect, sight, ring,
    /// Dirt (`crate::sim::setup::descriptors`).
    pub descriptors: Descriptors,
}

impl Art {
    /// Every unit's, from `catalogue`.
    #[must_use]
    pub fn of(catalogue: &Catalogue<'_>) -> Art {
        let mut art = Art::default();

        for (at, record) in catalogue.tagged(b"DUNT") {
            let Ok(unit) = Unit::at(catalogue, at) else {
                continue;
            };

            if let Ok(Some(rect)) = footprint::of_unit(catalogue, &unit) {
                art.reaches.insert(record.id, mobject::reach(rect));
            }

            for set in &unit.animations {
                let count = u16::try_from(set.poses.len()).unwrap_or(u16::MAX);

                art.slots.insert(
                    (record.id, u32::from(crate::data::imag::set_number(set.id))),
                    count,
                );
            }

            if let (Some(first), Some(set)) = (unit.initial(), setup::recorded(&unit)) {
                let count = u16::try_from(first.poses.len()).unwrap_or(u16::MAX);

                art.initial.insert(record.id, (set, count));
            }
        }

        art.variants = variants(catalogue);
        art.descriptors = setup::descriptors(catalogue);
        art
    }

    /// A cell with a variant its pair and case have
    /// ([`crate::sim::mapgen::dressing::spread`]).
    ///
    /// A uniform cell's `variant` is a grid column, not an index into the
    /// plain list: Dirt's plain columns skip the group at 16. A group
    /// column is never picked, since one cell cannot draw a 2 × 2 picture.
    #[must_use]
    pub fn dress(&self, cell: Cell, at: (u32, u32)) -> Cell {
        let columns = self.variants.get(&(cell.pair, cell.case));
        let count = columns.map_or(1, |c| u16::try_from(c.len()).unwrap_or(u16::MAX));
        let picked = crate::sim::mapgen::dressing::spread(cell, count, at);
        let column = columns.and_then(|c| c.get(usize::from(picked.variant)));

        Cell {
            variant: column.copied().unwrap_or(0),
            ..picked
        }
    }
}

/// The variants per pair and case in the widest tileset: each body's
/// plain columns, each blend case's indices.
fn variants(catalogue: &Catalogue<'_>) -> BTreeMap<(u8, u8), Vec<u16>> {
    let terrain = catalogue
        .tagged(b"DTRN")
        .filter_map(|(at, _)| Terrain::at(catalogue, at).ok())
        .max_by_key(|t| t.classes.len());
    let Some(tileset) = terrain.and_then(|t| t.tileset) else {
        return BTreeMap::new();
    };
    let upto = |n: usize| (0..u16::try_from(n).unwrap_or(u16::MAX)).collect::<Vec<_>>();

    // A body's plain columns: every column but its groups.
    let bodies = tileset.bodies.iter().map(|b| {
        let plain = (0..b.frames)
            .filter(|c| b.groups.iter().all(|(g, _)| *g != usize::from(*c)))
            .collect();

        ((b.letter << 4 | b.letter, 0), plain)
    });
    let blends = tileset.blends.iter().flat_map(move |b| {
        b.cases.iter().enumerate().map(move |(case, c)| {
            let case = u8::try_from(case).unwrap_or(u8::MAX);

            ((b.from << 4 | b.to, case), upto(c.variants.len()))
        })
    });

    bodies.chain(blends).collect()
}
