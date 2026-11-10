//! The whole generator, from a dialog choice to a map.
//!
//! **Binary** — world generate charts builds eight
//! charts on one RNG stream seeded with the map number. Each chart's draws
//! shift all later ones.
//!
//! ```text
//! rng = map number
//!   force      the blocks a settlement and a scenario take
//!   unit       what stands inside each of those blocks
//!   region     the zones, grown as blobs
//!   terrain    a terrain pattern per cell
//!   surface    that pattern's classes, and the Dirt between biomes
//!   landscape  which of the zone's scatter entries a cell drew
//!   mobject    the scatter objects themselves
//!   fractal    the heightfield, re-seeded once per height profile
//! ```

use std::collections::BTreeMap;

use crate::base::fixed::Fixed;
use crate::data::fourcc::FourCc;
use crate::data::rgs::Map;
use crate::data::rgs::chart::{Chart as Patterns, Layer, Pattern};
use crate::sim::freestyle::Choice;
use crate::sim::mapgen::recipe::Land;
use crate::sim::scene::SIDES;

use crate::sim::mapgen::field::Field;
use crate::sim::mapgen::pick::Distribution;
use crate::sim::mapgen::rng::Rng;
use crate::sim::mapgen::surface::Ground;
use crate::sim::mapgen::{
    Zone, generate_from, landscape, mobject, paint, region, setup, surface, terrain,
};

/// A generated map: every chart, and the ground under them.
pub struct World {
    /// Where each placed settlement and scenario unit stands.
    pub units: crate::sim::mapgen::unit::Units,
    pub region: region::Chart,
    /// The `DTPT` class per cell, Dirt between biomes included.
    ///
    /// Lossy against [`World::cells`] (two classes per cell). Downstream
    /// charts read one class per cell.
    pub ground: surface::Chart,
    /// The painted cells, pair and blend case, as a save stores them. See
    /// `crate::sim::mapgen::paint`.
    pub cells: Vec<crate::sim::paint::Cell>,
    /// Which entry of its zone's scatter each cell drew.
    pub landscape: landscape::Chart,
    /// The scatter objects and their cells.
    pub mobjects: mobject::Chart,
    pub field: Field,
    /// Draws spent by the seven charts before the fractal.
    pub draws: u32,
    /// [`World::draws`] by chart.
    pub spend: Spend,
    /// Stream state when the fractal starts. Kept for tests that regenerate
    /// from other states.
    pub state: i32,
    /// Stream after the fractal: the last profile's pass, since it re-seeds
    /// per profile. Later draws start here; see [`crate::sim::mapgen::animate`].
    pub after: Rng,
}

/// What each chart before the fractal spent, in draws.
///
/// Sums to [`World::draws`]. Six fields: [`Spend::setup`] covers force and
/// unit, both run by [`setup::before_the_region_chart`].
#[derive(Debug, Default, Clone, Copy)]
pub struct Spend {
    /// Force and unit charts.
    pub setup: u32,
    pub region: u32,
    pub terrain: u32,
    pub surface: u32,
    pub landscape: u32,
    pub mobject: u32,
}

/// Why a map is not generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The quest lacks the chosen land.
    Land(FourCc),
    /// A side not in [`SIDES`]: the fractal halves it down to one cell,
    /// so it must be a power of two, and the charts are made for these.
    Side(u32),
    /// A terrain pattern whose blotch starts past the eight neighbours
    /// it can take (modded data): the blotch would never end.
    Spread(FourCc, u32),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Land(land) => write!(f, "the quest does not hold land {land}"),
            Refusal::Side(side) => write!(f, "a map side is one of {SIDES:?}, not {side}"),
            Refusal::Spread(code, spread) => {
                write!(
                    f,
                    "terrain pattern {code}'s blotch spread {spread} is too wide"
                )
            }
        }
    }
}

/// `side` if the generator makes maps of it.
///
/// # Errors
///
/// A side not in [`SIDES`].
pub fn sided(side: u32) -> Result<u32, Refusal> {
    if SIDES.contains(&side) {
        Ok(side)
    } else {
        Err(Refusal::Side(side))
    }
}

/// Generate a map.
///
/// # Errors
///
/// A land the quest lacks, a side not in [`SIDES`], or a terrain pattern
/// too wide ([`Refusal`]).
pub fn build(map: &Map<'_>, patterns: &Patterns<'_>, choice: &Choice) -> Result<World, Refusal> {
    build_over(map, patterns, choice, &Over::default())
}

/// Optional inputs beyond the dialog's picks.
#[derive(Default)]
pub struct Over<'a> {
    /// An object's footprint in cells, from its art
    /// ([`mobject::Scatter::footprint`]). Default: one cell.
    pub footprint: Option<&'a dyn Fn(FourCc) -> mobject::Reach>,
    /// Whether the scatter chart logs its walk ([`mobject::Chart::walked`]).
    /// Read-only.
    pub trace: mobject::Trace,
}

/// [`build`], with [`Over`] inputs.
///
/// # Errors
///
/// As [`build`].
pub fn build_over(
    map: &Map<'_>,
    patterns: &Patterns<'_>,
    choice: &Choice,
    over: &Over<'_>,
) -> Result<World, Refusal> {
    let land = Land::of(map, choice.land).ok_or(Refusal::Land(choice.land))?;
    let side = sided(choice.side)?;
    let mut rng = Rng::new(choice.seed);
    let mut spend = Spend::default();
    let mut mark = 0;

    // Draws since the mark; advances the mark.
    let since = |rng: &Rng, mark: &mut u32| {
        let cost = rng.draws - *mark;

        *mark = rng.draws;
        cost
    };

    // One and two: force and unit.
    let (force, units) = setup::before_the_region_chart(map, &choice.picks(), side, &mut rng);

    spend.setup = since(&rng, &mut mark);

    // Three: zones, seeded then grown.
    let region = regions(map, &land, &force, side, &mut rng);

    spend.region = since(&rng, &mut mark);

    // Four and five: the ground, two picks deep.
    let terrain = terrain::Chart::build(&region, &surfaces(map, patterns), &mut rng);

    spend.terrain = since(&rng, &mut mark);

    let chosen = surface::Chart::build(&terrain, &grounds(patterns), &mut rng)?;

    spend.surface = since(&rng, &mut mark);

    // The engine's paint, making Dirt between biomes. See `crate::sim::mapgen::paint`.
    let cells = paint::lay(&chosen);
    let ground = paint::classes(&cells, &chosen);

    // Six and seven: scatter entries, then objects.
    let scatters: BTreeMap<FourCc, Distribution> = scatter_patterns(map, patterns)
        .map(|(code, p)| (code, table(p)))
        .collect();
    let landscape = landscape::Chart::build(&region, &scatters, &mut rng);

    spend.landscape = since(&rng, &mut mark);

    let clumps: BTreeMap<FourCc, Vec<mobject::Clump>> = scatter_patterns(map, patterns)
        .map(|(code, p)| (code, clumps_of(p)))
        .collect();
    let objects: BTreeMap<FourCc, mobject::Table> = patterns
        .layer(Layer::Object)
        .iter()
        .map(|p| (p.code, objects_of(p)))
        .collect();
    let mobjects = mobject::Chart::build(
        &region,
        &landscape,
        &mobject::Scatter {
            zones: &clumps,
            objects: &objects,
            footprint: over.footprint.unwrap_or(&|_| mobject::POINT),
            trace: over.trace,
        },
        mobject::limit(side),
        &mut rng,
    );

    spend.mobject = since(&rng, &mut mark);

    // Eight: the heightfield, re-seeded per profile from the current state.
    let (field, after) = field_of(map, &land, patterns, &region, side, rng.state);

    Ok(World {
        units,
        region,
        ground,
        cells,
        landscape,
        mobjects,
        field,
        draws: rng.draws,
        spend,
        state: rng.state,
        after,
    })
}

/// The region chart: seed zones, stamp placed blocks, grow until full.
///
/// # Panics
///
/// If a stamped block lies off the five by five grid.
pub fn regions(
    map: &Map<'_>,
    land: &Land<'_>,
    force: &crate::sim::mapgen::force::Force,
    side: u32,
    rng: &mut Rng,
) -> region::Chart {
    let mut zones = Distribution::new(land.zones.iter().map(|z| (z.code, z.weight)).collect());
    let mut region = region::Chart::new(side);

    region.seed(
        &land
            .zones
            .iter()
            .map(|z| (z.code, z.seeds))
            .collect::<Vec<_>>(),
        rng,
    );

    for (block, code) in setup::stamps(map, force) {
        let at = (
            i32::try_from(block.0).expect("a block of the five by five grid"),
            i32::try_from(block.1).expect("a block of the five by five grid"),
        );

        region.stamp_block(at, code, &mut zones);
    }

    let order: Vec<(i32, i32)> = crate::sim::mapgen::shuffled_points(rng, side, side)
        .into_iter()
        .map(|(x, y)| (x.cast_signed(), y.cast_signed()))
        .collect();

    region.grow_all(&mut zones, &order, rng);
    region
}

/// Every zone's surface pattern, by region code.
///
/// The quest's zones, not the land's: zone of looks up
/// all 108 the quest defines. A stamped block can put a code the land does not
/// name (`Icex` stamps K'Hal's `$Dta`); those cells must still draw.
fn surfaces(map: &Map<'_>, patterns: &Patterns<'_>) -> BTreeMap<FourCc, Distribution> {
    map.zones
        .iter()
        .filter_map(|z| {
            patterns
                .find_in(Layer::Surface, z.surface)
                .map(|p| (z.code, table(p)))
        })
        .collect()
}

/// Every zone's landscape pattern, by region code. As [`surfaces`].
fn scatter_patterns<'a>(
    map: &'a Map<'_>,
    patterns: &'a Patterns<'_>,
) -> impl Iterator<Item = (FourCc, &'a Pattern<'a>)> {
    map.zones.iter().filter_map(|z| {
        patterns
            .find_in(Layer::Landscape, z.landscape)
            .map(|p| (z.code, p))
    })
}

/// Every terrain pattern, with its blotch spread.
fn grounds(patterns: &Patterns<'_>) -> BTreeMap<FourCc, Ground> {
    patterns
        .layer(Layer::Terrain)
        .iter()
        .map(|p| {
            (
                p.code,
                Ground {
                    table: table(p),
                    spread: p.entries.first().map_or(0, |e| e.rest[0]),
                },
            )
        })
        .collect()
}

/// Run the fractal once per distinct height profile code the land names.
///
/// A pass per profile, not per zone: Ravenswood's zones carry `Subt`, `Moun`,
/// `Subt`, giving two passes.
///
/// # Pass list from the land, ownership from the quest
///
/// **Binary**. The pass list is built in gen heightfield from the land's zone
/// list (world, land zone list): each region code through
/// zone of, profile code, deduplicated first-wins. An
/// unresolved code adds no pass.
///
/// `crate::sim::mapgen::reset` resolves per cell in gen zone reset via zone of, in the
/// world's zone map (all 108 quest zones). So a stamped code the
/// land lacks still resolves: K'Hal's `$Dta` stamped onto East Cheswick.
///
/// Keyed on profile code, not `(scale, falloff)`: equal settings under two
/// codes are two passes. No shipped land has such a pair.
fn field_of(
    map: &Map<'_>,
    land: &Land<'_>,
    patterns: &Patterns<'_>,
    region: &region::Chart,
    side: u32,
    state: i32,
) -> (Field, Rng) {
    // zone of: every quest zone, by code.
    let profile_of: BTreeMap<FourCc, FourCc> = map
        .zones
        .iter()
        .map(|zone| (zone.code, zone.height))
        .collect();

    // Pass list: the land's zones, deduplicated by profile code.
    let mut distinct: Vec<FourCc> = Vec::new();

    for spec in &land.zones {
        if let Some(code) = profile_of.get(&spec.code)
            && !distinct.contains(code)
        {
            distinct.push(*code);
        }
    }

    let owns: Vec<Box<dyn Fn(u32, u32) -> bool + '_>> = distinct
        .iter()
        .map(|want| {
            let (want, profile_of) = (*want, &profile_of);

            Box::new(move |x: u32, y: u32| {
                region
                    .at((x.cast_signed(), y.cast_signed()))
                    .and_then(|code| profile_of.get(&code))
                    .is_some_and(|held| *held == want)
            }) as Box<dyn Fn(u32, u32) -> bool>
        })
        .collect();
    let profiles: Vec<Zone<'_>> = distinct
        .iter()
        .zip(&owns)
        .map(|(code, owns)| {
            let profile = patterns.profiles.iter().find(|p| p.code == *code);

            Zone {
                scale: profile.map_or(Zone::DEFAULT_SCALE, |p| {
                    Fixed::from_bits(p.words[2].cast_signed())
                }),
                falloff: profile.map_or(Zone::DEFAULT_FALLOFF, |p| {
                    Fixed::from_bits(p.words[1].cast_signed())
                }),
                owns: owns.as_ref(),
            }
        })
        .collect();

    generate_from(side, &profiles, state)
}

/// A chart pattern's entries as a weighted table.
fn table(pattern: &Pattern<'_>) -> Distribution {
    Distribution::new(pattern.entries.iter().map(|e| (e.id, e.weight)).collect())
}

/// A landscape pattern's entries as the mobject chart reads them.
fn clumps_of(pattern: &Pattern<'_>) -> Vec<mobject::Clump> {
    pattern
        .entries
        .iter()
        .map(|e| mobject::Clump {
            pattern: e.id,
            size: e.rest[0],
        })
        .collect()
}

/// An object pattern's entries and each one's stamp grid.
///
/// **Binary** — `rest[2]` is the byte entry byte reads and
/// mobject stamp switches on; see [`mobject::Stamp`].
fn objects_of(pattern: &Pattern<'_>) -> mobject::Table {
    mobject::Table {
        table: table(pattern),
        stamps: pattern
            .entries
            .iter()
            .map(|e| {
                if e.rest[2] == 0 {
                    mobject::Stamp::Map
                } else {
                    mobject::Stamp::Window
                }
            })
            .collect(),
    }
}
