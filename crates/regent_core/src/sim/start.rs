//! The start: what stands on a new map before the first frame, and where.
//!
//! **Binary** — world materialise turns the charts
//! into agents, all on the charts' RNG stream:
//!
//! ```text
//!  bare placements             off-map lairs, one draw each      Stage::OffMap
//!  landscape place             scatter, x outer, one draw each   Stood::slots
//!  unit chart place:
//!    shuffled_point_list(w, h)          w * h draws: the order
//!    per unit chart cell holding a unit:
//!      at = cell << 5                   32nds, a whole cell
//!      kind 0 or 3: mobject_jitter      two draws, x then y
//!      create, owner from the block     a Rec
//!      animate                          one draw
//!      place (margin 8, flags 1)        the place search below
//! ```
//!
//! [`stand`] runs all three; [`raised`] hands the result to
//! [`World::fresh`](crate::sim::world::World::fresh).
//!
//! The descriptor's kind picks the record class `GS_Level`
//! makes: a character (3) a `VehicleRec`, unit kind 1;
//! a general unit (0) a `TerrainFeatureRec`, unit kind 2,
//! as scatter is; anything else a `BuildingRec`, kind 0.
//!
//! # The place search
//!
//! A building floods from its cell to
//! the nearest half cell where its clearing ring is clear
//! (place search, flags `0x1800`). Scatter lets it through
//! (blocker default's `0x1000`), so only units made before it block:
//! the order decides. A ring cell off the map blocks. A failed search leaves
//! it on its cell.
//!
//! A mobile searches with its own pad
//! and flags `0x800`: scatter blocks it too. A terrain feature does not
//! search: it takes `GSUnit`'s placing, the clearing
//! alone, which clears the scatter about it. It blocks as scatter does,
//! and a later ring clears it. **Game**: the gamble probe's three ruined
//! keeps clear four scatter objects; the giant rat placed after clears
//! the third keep, so its start save holds two.
//!
//! Neither search draws, nor does levelling or clearing; a cleared object
//! is deleted after the pass, so it still blocks a mobile.
//!
//! # The owner
//!
//! **Binary**: a unit chart cell's second word is its force block's, which
//! Force owner of gives as the player slot listing the
//! block's template placement index, else 7.
//! A freestyle game's lists are freestyle setup's
//! ([`freestyle::seats`]), which `owner` reads; alone, they agree with the
//! quest's slot items on every shipped pair. A quest's `owner` reads its
//! slot items (**Inferred**: a quest's lists are its slot items).
//! `AVk1`-`AVk3` get -1.
//!
//! # The slot
//!
//! **Binary**: agent animate's draw is `GSUnit`'s, bounded by
//! the slots of the set the unit is made in ([`Body::slots`]), gated only
//! by unit animation absent. A code with no body
//! still spends its draw. **Corpus**: that set is
//! [`Unit::initial`](crate::content::unit::Unit::initial); each start
//! building's and terrain feature's slot is the draw (**Measured**).

use crate::base::digest;

use crate::base::rng;
use crate::data::fourcc::FourCc;
use crate::data::rgs::Map;
use crate::data::rgs::map::{OFF_MAP, Spawner};
use crate::sim::clearing;
use crate::sim::freestyle::{self, Forces, Play, lair_key};
use crate::sim::placement::{self, Edge};
use crate::sim::units::{PER_CELL, PER_HALF};
use crate::sim::world::{
    bodies::{Bodies, Body, Class, Placing},
    new_map::{LairEntry, Made, OnGrid, Raised, Settings, Sited, Spawns},
};

/// The slot a unit goes to when its block names none: 7
/// in unit chart place, the same in force chart build.
pub const NEUTRAL: i32 = 7;

/// A resource's owner: is avk in unit chart place.
const NO_OWNER: i32 = -1;

/// Mobject jitter: the box a jittered unit lands in,
/// 32nds, and its half width.
const JITTER: i32 = 100;
const JITTER_HALF: i32 = 50;

/// What the eight charts leave the start: world generate charts's
/// unit and landscape charts, and the stream after them.
pub struct Charts<'a> {
    /// Cells a side.
    pub side: u32,
    /// Every scatter object in landscape place's walk, x outer and
    /// y inner: code and cell.
    pub scatter: &'a [(FourCc, (i32, i32))],
    /// The unit chart's unit on a cell; `None` where it holds none.
    pub units: &'a dyn Fn((i32, i32)) -> Option<Charted>,
    /// The charts' stream state after the fractal.
    pub after: u32,
}

/// A unit chart cell's unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Charted {
    pub code: FourCc,
    /// The template placement whose force block holds the cell: the
    /// owner's source.
    pub source: Option<usize>,
    /// Its lair key ([`lair_key`]), chart word `0x34`.
    pub key: Option<u32>,
}

/// Whose world the start stands up: what world materialise finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    /// The custom game's: freestyle setup's template and its lairs,
    /// and who plays, which owns each placement ([`freestyle::seats`]).
    Freestyle(Forces, Play),
    /// A quest's, as quest file read left it: no template at,
    /// so no off-map lair (bare placement materialise
    /// walks it); the lairs the `.q`'s own spawners, which it reads into.
    /// **Corpus**: no Day of Reckoning save holds
    /// its `Anim` placement's off-map den.
    Quest,
}

/// The record class a chart unit is made as: `GS_Level`'s kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rec {
    /// Kind 0, `BuildingRec`.
    Building,
    /// Kind 1, `VehicleRec`.
    Mobile,
    /// Kind 2, `TerrainFeatureRec`: scatter to the grid and the clearing.
    Feature,
}

impl Rec {
    /// The class `body`'s create makes.
    #[must_use]
    pub const fn of(body: &Body) -> Rec {
        match body.class {
            Class::Character => Rec::Mobile,
            Class::General => Rec::Feature,
            _ => Rec::Building,
        }
    }
}

/// Which pass made a unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// bare placement materialise: an off-map lair.
    OffMap,
    /// unit chart place.
    Chart,
}

/// How the place search ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// A clear half cell, maybe its own.
    Clear,
    /// None: it stays where it was made, unlevelled.
    Stuck,
    /// Not searched: an off-map lair, a terrain feature.
    Unsearched,
}

/// One unit as the start stands it up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub code: FourCc,
    /// Player slot; -1 for a resource.
    pub owner: i32,
    /// Where it stands, 32nds of a cell.
    pub at: (i32, i32),
    /// The chart cell it was made on; an off-map lair's edge position.
    pub cell: (i32, i32),
    pub stage: Stage,
    pub found: Found,
    pub rec: Rec,
    /// The slot `GSUnit` drew: a building's or feature's picture, a
    /// mobile's direction word (**Inferred** for a mobile: no save holds
    /// one unmoved).
    pub slot: u16,
    /// A lair's settings: the template's spawner under its key
    /// ([`freestyle::lairs`]).
    pub lair: Option<Settings>,
}

/// What the start stood up.
pub struct Stood {
    /// Every unit, in creation order: off-map lairs, then the unit chart
    /// in its shuffled order. Creation order is handle and agent order.
    pub starts: Vec<Start>,
    /// Each of [`Charts::scatter`]'s art slots, in its order.
    pub slots: Vec<u16>,
}

/// Who blocks a search: blocker default under its flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blockers {
    /// `0x1800`, a building: scatter passes.
    Units,
    /// `0x800`, a mobile: everything on a shared layer.
    All,
}

/// The collision grid: per half cell, the layers scatter and units take.
#[derive(Default)]
struct Occupancy {
    scatter: digest::Map<(i32, i32), u32>,
    units: digest::Map<(i32, i32), u32>,
}

impl Occupancy {
    fn blocks(&self, at: (i32, i32), layer: u32, who: Blockers) -> bool {
        let taken =
            |map: &digest::Map<(i32, i32), u32>| map.get(&at).is_some_and(|l| l & layer != 0);

        taken(&self.units) || (who == Blockers::All && taken(&self.scatter))
    }

    fn put(map: &mut digest::Map<(i32, i32), u32>, body: &Body, half: (i32, i32)) {
        if body.grid() == OnGrid::On {
            for (dx, dy) in &body.cells {
                *map.entry((half.0 + dx, half.1 + dy)).or_default() |= body.layer;
            }
        }
    }
}

/// Every unit the start stands up on `charts`, and each scatter
/// object's slot, `recipe` naming the template and lairs.
///
/// A code `bodies` lacks is not made, as the engine's create fails
/// without a descriptor.
///
/// # Panics
///
/// If the map side exceeds `i32`.
#[must_use]
pub fn stand(charts: &Charts<'_>, map: &Map<'_>, recipe: Recipe, bodies: &Bodies) -> Stood {
    let mut rng = charts.after;
    let lairs = match recipe {
        Recipe::Freestyle(forces, _) => freestyle::lairs(map, &forces),
        Recipe::Quest => map.spawners.iter().map(|(k, s)| (*k, s)).collect(),
    };
    let settings = |key: Option<u32>| key.and_then(|k| lairs.get(&k)).map(|s| lair(s));
    let mut out = match recipe {
        Recipe::Freestyle(forces, _) => off_map(map, &forces, bodies, &settings),
        Recipe::Quest => Vec::new(),
    };
    let lists = match recipe {
        Recipe::Freestyle(forces, play) => Lists::Freestyle(freestyle::seats(map, &forces, play)),
        Recipe::Quest => Lists::Quest,
    };

    // Bare placement materialise draws first, a slot each.
    for lair in &mut out {
        lair.slot = bodies.get(&lair.code).map_or(0, |b| draw(b, &mut rng));
    }

    // Landscape place: a draw per scatter object.
    let slots = charts
        .scatter
        .iter()
        .map(|(code, _)| draw_over(bodies.get(code).map_or(0, |b| b.slots), &mut rng))
        .collect();

    let side = i32::try_from(charts.side).expect("a map side");
    let grid = (2 * side, 2 * side);
    let mut occupancy = Occupancy::default();

    for (code, cell) in charts.scatter {
        if let Some(b) = bodies.get(code) {
            Occupancy::put(&mut occupancy.scatter, b, (2 * cell.0, 2 * cell.1));
        }
    }

    for (px, py) in shuffled_points(&mut rng, charts.side, charts.side) {
        let cell = (px.cast_signed(), py.cast_signed());
        let Some(charted) = (charts.units)(cell) else {
            continue;
        };
        let Some(b) = bodies.get(&charted.code) else {
            continue;
        };
        let rec = Rec::of(b);

        // A whole cell in 32nds: `<< 5` in unit chart place.
        let made = (cell.0 * PER_CELL, cell.1 * PER_CELL);
        let made = match rec {
            Rec::Building => made,
            Rec::Mobile | Rec::Feature => jitter(made, side, &mut rng),
        };

        let slot = draw(b, &mut rng);

        let (at, found) = search(b, made, grid, &occupancy);
        let half = (at.0 / PER_HALF, at.1 / PER_HALF);

        match rec {
            Rec::Feature => Occupancy::put(&mut occupancy.scatter, b, half),
            Rec::Building | Rec::Mobile => Occupancy::put(&mut occupancy.units, b, half),
        }

        out.push(Start {
            code: charted.code,
            owner: owner(map, &lists, charted.source, charted.code),
            at,
            cell,
            stage: Stage::Chart,
            found,
            rec,
            slot,
            lair: settings(charted.key),
        });
    }

    Stood { starts: out, slots }
}

/// A chart spawner as a lair's settings.
fn lair(spawner: &Spawner<'_>) -> Settings {
    let mut entries = [LairEntry {
        code: FourCc::new(b"NONE"),
        weight: 0,
        force: 0,
    }; 4];

    for (slot, item) in entries.iter_mut().zip(&spawner.team.items) {
        *slot = LairEntry {
            code: item.id,
            weight: item.level,
            force: item.resource.unwrap_or(0),
        };
    }

    Settings {
        words: spawner.words,
        extra: spawner.extra.unwrap_or(0),
        entries,
        special: spawner.names.clone(),
    }
}

/// Agent animate's slot draw: one draw over the
/// slots of the set the unit is made in.
fn draw(body: &Body, rng: &mut u32) -> u16 {
    draw_over(body.slots, rng)
}

/// A draw over `slots`; 0 under two, the stream still stepped.
fn draw_over(slots: u16, rng: &mut u32) -> u16 {
    u16::try_from(rng::draw(rng, i32::from(slots))).unwrap_or(0)
}

/// Bare placement materialise: per template
/// placement whose first cell is 1 to 8, each building of the block it
/// names, at its edge's `x` and `y` -1000, slot 7's, never searched.
fn off_map(
    map: &Map<'_>,
    forces: &Forces,
    bodies: &Bodies,
    settings: &dyn Fn(Option<u32>) -> Option<Settings>,
) -> Vec<Start> {
    let block = |code: FourCc| map.units[0].iter().find(|u| u.faction == code);

    freestyle::template(map, forces)
        .iter()
        .enumerate()
        .filter_map(|(i, placed)| Some((i, placed.edge()?, block(placed.id)?)))
        .flat_map(|(i, edge, record)| {
            record
                .placements
                .iter()
                .enumerate()
                .filter_map(move |(j, entry)| {
                    let b = bodies.get(&entry.id)?;
                    let at = (edge.lair_x(), OFF_MAP);

                    (b.class == Class::Building).then_some(Start {
                        code: entry.id,
                        owner: NEUTRAL,
                        at,
                        cell: at,
                        stage: Stage::OffMap,
                        found: Found::Unsearched,
                        rec: Rec::Building,
                        slot: 0,
                        lair: settings(Some(lair_key(i, j))),
                    })
                })
        })
        .collect()
}

/// Mobject jitter: a box of [`JITTER`] about the point, clamped
/// low at 0, high as the engine writes it (cells, not 32nds: the side less
/// `0x64`), then a draw on each axis.
fn jitter((x, y): (i32, i32), side: i32, rng: &mut u32) -> (i32, i32) {
    let low = |v: i32, slack: i32| {
        let v = v - JITTER_HALF;

        if v < 0 {
            0
        } else if v + slack > side * PER_CELL {
            side - JITTER
        } else {
            v
        }
    };
    let (x0, y0) = (low(x, JITTER), low(y, JITTER_HALF));
    let dx = rng::draw(rng, JITTER);
    let dy = rng::draw(rng, JITTER);

    (x0 + dx, y0 + dy)
}

/// The unit's own place search from `made`, 32nds; where it ends.
fn search(
    body: &Body,
    made: (i32, i32),
    grid: (i32, i32),
    occupancy: &Occupancy,
) -> ((i32, i32), Found) {
    let ring: Option<Vec<(i32, i32)>> = body.ring.as_ref().map(|pad| pad.halves().collect());
    let (cells, edge, who) = match (Rec::of(body), &ring) {
        (Rec::Feature, _) => return (made, Found::Unsearched),
        (Rec::Building, _) if body.placing == Placing::Left => return (made, Found::Unsearched),
        (Rec::Building, Some(ring)) => (ring.as_slice(), Edge::Mask, Blockers::Units),
        (Rec::Building, None) => (body.cells.as_slice(), body.edge, Blockers::Units),
        (Rec::Mobile, _) => (body.cells.as_slice(), body.edge, Blockers::All),
    };
    let taken = |at: (i32, i32)| occupancy.blocks(at, body.layer, who);

    match placement::search((cells, edge), grid, made, taken) {
        Some(at) => (at, Found::Clear),
        None => (made, Found::Stuck),
    }
}

/// The player slots' lists of template placements force owner of
/// reads.
enum Lists {
    /// freestyle setup's: each placement's slot, in template order.
    Freestyle(Vec<i32>),
    /// The quest's slot items.
    Quest,
}

/// A unit's owner: the slot listing its block's template placement, else
/// [`NEUTRAL`]; a resource has none.
fn owner(map: &Map<'_>, lists: &Lists, source: Option<usize>, code: FourCc) -> i32 {
    let listed = |index: usize| match lists {
        Lists::Freestyle(seats) => seats.get(index).copied(),
        Lists::Quest => map
            .slots
            .iter()
            .position(|slot| slot.items.iter().any(|i| usize::try_from(*i) == Ok(index)))
            .and_then(|slot| i32::try_from(slot).ok()),
    };

    if clearing::RESOURCES.contains(&code) {
        NO_OWNER
    } else {
        source.and_then(listed).unwrap_or(NEUTRAL)
    }
}

/// The start as the world takes it ([`World::fresh`]): each unit's
/// record class, owner, place and slot. A resource's owner -1 is
/// `u32::MAX`; a feature's grid is its body's.
///
/// [`World::fresh`]: crate::sim::world::World::fresh
#[must_use]
pub fn raised(starts: &[Start], bodies: &Bodies) -> Vec<Raised> {
    starts
        .iter()
        .map(|s| {
            let grid = bodies.get(&s.code).map_or(OnGrid::On, Body::grid);

            Raised {
                code: s.code,
                owner: s.owner.cast_unsigned(),
                at: s.at,
                made_at: match s.stage {
                    Stage::OffMap => s.cell,
                    Stage::Chart => (s.cell.0 * PER_CELL, s.cell.1 * PER_CELL),
                },
                direction: s.slot,
                spawns: Spawns::ByName,
                lair: s.lair.clone(),
                made: match s.rec {
                    Rec::Building => Made::Building,
                    Rec::Mobile => Made::Mobile,
                    Rec::Feature => Made::Feature(grid),
                },
                sited: match (s.stage, s.found) {
                    (Stage::OffMap, _) => Sited::OffMap,
                    (Stage::Chart, Found::Stuck) => Sited::Stuck,
                    (Stage::Chart, Found::Unsearched) if s.rec == Rec::Building => Sited::Left,
                    (Stage::Chart, Found::Clear | Found::Unsearched) => Sited::Searched,
                },
            }
        })
        .collect()
}

/// Every point of a `rows` by `columns` grid, shuffled on `rng`.
///
/// **Binary** — shuffled point list. Three of the
/// seven charts open with one, and the unit chart's placing:
///
/// ```text
/// for i in 0 .. a * b:
///     j = rng_next(a * b)
///     swap points[i], points[j]
/// ```
///
/// Exactly `a * b` draws, each bounded by `a * b`; not Fisher-Yates'
/// `i..n`, so the permutation differs. the decompiler splits one stack
/// slot into two variables, so it misreads as a swap of one slot;
/// shuffled point list zeroes, reads and increments it.
///
/// Point `i` starts at `(i % columns, i / columns)`. Which one the engine
/// calls width is unknown: every map is square.
///
/// # Panics
///
/// If `rows * columns` exceeds the engine's `i32` bound. The widest grid is
/// 512 by 512.
#[must_use]
pub fn shuffled_points(rng: &mut u32, rows: u32, columns: u32) -> Vec<(u32, u32)> {
    let total = rows * columns;
    let mut points: Vec<(u32, u32)> = (0..total).map(|i| (i % columns, i / columns)).collect();

    // The engine's bound is signed.
    let bound = i32::try_from(total).expect("a grid fits in the engine's i32");

    for i in 0..points.len() {
        let j = rng::draw(rng, bound);

        points.swap(
            i,
            usize::try_from(j).expect("a draw under a positive bound"),
        );
    }

    points
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A permutation: every point, once.
    #[test]
    fn a_shuffle_keeps_every_point_and_adds_none() {
        let mut rng = 7;
        let mut points = shuffled_points(&mut rng, 12, 12);

        points.sort_unstable();
        points.dedup();

        assert_eq!(points.len(), 144);
        assert_eq!(points.first(), Some(&(0, 0)));
        assert_eq!(points.last(), Some(&(11, 11)));
    }
}
