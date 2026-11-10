//! The mobject chart: the scatter objects, and the last draws before the
//! heightfield.
//!
//! **Binary** — mobject chart build and
//! Mobject place, the seventh chart; the only one a
//! save holds directly. After the fractal, landscape place
//! makes one `GRND` record per cell holding a code.
//!
//! ```text
//! shuffled_point_list(w, h)              -- w * h draws
//! for each point in that order:
//!     if placed >= limit - 200: stop
//!     six gates, none of which draws:
//!         the chart's own cell is still NONE
//!         the region code names a zone
//!         the zone names a landscape pattern
//!         the landscape chart's index names an entry of it
//!         the entry's code is neither NONE nor Empt
//!         that code names an object pattern
//!     one draw picks the object                       -- the object's kind
//!     the footprint has to fit, and then it is stamped
//!     while the clump holds fewer than the entry's own count:
//!         one draw for a direction and two for how far -- mobject step
//!         a step onto another zone places nothing
//!         otherwise one more draw for what goes there, and it is placed if
//!         the footprint fits
//! ```
//!
//! A clump try costs three draws, four if the step stays in the zone.
//!
//! # Two stamp grids; the entry picks one
//!
//! A placed object marks its cell and [`HALO`] cells around with `FOOT`.
//! The first gate reads that mark,
//! keeping clumps five cells apart.
//!
//! The grid is the stamp's fifth argument: the byte at the picked entry's
//!  (see [`Stamp`]). Preset to 1 in mobject place, so the default is the
//! window.
//!
//! [`Stamp::Window`] (common): stamped into a private 41 by 41 window about the
//! first point. Mobject fits reads the map, not the window, so a clump's
//! members do not block each other; mobject commit copies the window onto
//! `NONE` map cells afterwards. On one land 186 of 501 objects have a
//! neighbour one cell away, yet no two clumps are closer than five.
//!
//! # The footprint
//!
//! Mobject footprint gets a rect per code from the
//! `IsoFootprintRectManager` singleton, divides by 32 and adds it to the
//! point; mobject fits needs every cell free, map edge included.
//!
//! The lookup, `FootprintRectManager`,
//! finds the `IMAG` by code and calls `IsoFootprintRectManager`, ignoring
//! its result, over an uninitialised stack rect. The only object-layer code
//! with no `IMAG` is [`CLEAR`], which `kind` refuses first:
//! `every_code_the_scatter_can_pick_has_a_footprint_to_read`.
//!
//! `IsoFootprintRectManager` takes slot 0's rect via
//! Slot block on set 8, then `0xd0`, if `left < right` and
//! `top < bottom`; else it unions frame boxes on sets 8, `0xd0`, `0x40`. Set
//! `0x40`'s slot rect is never used, so a `Special`-only object is measured by
//! its pictures: `YG02`, `NGe1`, `NGl1` (3 of 79 scatterable codes). Top and
//! bottom are scaled by `32 / (32 sin(IsoCameraAngle))` before the divide by
//! 32. `crate::sim::footprint` carries the chain.
//!
//! A footprint off by one mis-sizes the halo, so `Chart::fits` takes or
//! spares the next clump's seed and the error walks forward. See [`reach`].
//!
//! # The cap stops the chart, not the shuffle
//!
//! On the measured 128-cell save the walk covers 5,265 of 16,384 points, then
//! breaks on `placed >= 550`; the chart's 22,092 draws end at that stamp. [`Chart::stamps`] is 550 at 149,453 draws, both exact.
//!
//! # Checked against the corpus
//!
//! With real rects:
//!
//! * 197,772 of 197,945 objects over 60 saves, same cell and code. The 173
//!   left are jittered records with fractional `GRND` positions.
//! * Objects a save holds that the chart does not place: 0 on every save.
//! * The save's own clump on all 16,229 cells whose seed a save pins.
//! * All 60 heightfields, so draw counts agree everywhere.
//!
//! Objects the chart places but a save lacks (19 to 181 a save, up to 66 cells
//! from a building) are lost after stamping:
//! **Measured**.
//!
//! # All of mobject place is read
//!
//! Mobject kind's extra gate (pattern code is `NONE`) cannot
//! fire: the loader drops such records
//! (`no_shipped_chart_pattern_carries_the_code_the_gate_refuses`).
//!
//! Read with `pd`; `pdg` cannot decompile mobject fits and mis-frames
//! its caller. Checked: `step` and its clamp, `Chart::fits` and its bounds,
//! `Chart::stamp`'s halo, window index and unconditional centre,
//! `Chart::commit`, the 256-try clump loop, the cap, [`reach`]'s
//! truncations. Only the footprint and fit sit between `kind`'s draw and
//! `Chart::stamp`.
//!
//! # The oracle over pinned entries
//!
//! A pinned entry with a one-object clump is stamped on the walk point itself.
//! `landscape_oracle` pins 16,229 such cells; trace the walk against them with
//! [`Trace`], [`Chart::walked`], [`Refused`]. All are answered.
//!
//! A clump placing no members runs 257 iterations: `1 + 257 * 4` = 1,029
//! draws. A draw-count offset in the hundreds is one to three clumps.

use std::collections::BTreeMap;

use crate::data::fourcc::FourCc;

use crate::sim::mapgen::pick::Distribution;
use crate::sim::mapgen::region::NONE;
use crate::sim::mapgen::rng::Rng;
use crate::sim::mapgen::shuffle::shuffled_points;
use crate::sim::mapgen::{landscape, region};

/// The mark a placed object leaves on the cells around it.
///
/// **Binary** — literal in mobject stamp, over the object's rect grown by
/// [`HALO`].
pub const FOOT: FourCc = FourCc::new(b"FOOT");

/// A landscape entry naming no objects.
///
/// **Binary** — compare in mobject place. Two of six entries in every
/// freestyle landscape pattern, carrying most of the weight.
pub const EMPTY: FourCc = FourCc::new(b"Empt");

/// An object-pattern entry that resolves to nothing.
///
/// **Binary** — compare inside mobject kind, after the pick, so the
/// draw is spent. Only the six `#DEMO_*` patterns have one.
pub const CLEAR: FourCc = FourCc::new(b"CLER");

/// Cells marked around a placed object's rect.
///
/// **Binary** — mobject stamp's four rect adjustments.
pub const HALO: i32 = 4;

/// A footprint's reach around its cell: left, up, right, down. From the art
/// ([`Scatter::footprint`]), or [`POINT`].
pub type Reach = (i32, i32, i32, i32);

/// The footprint of an object whose art says nothing: its own cell.
///
/// **Binary** — what a rejected rect gives (5,109 of HD's 10,024 slots).
pub const POINT: Reach = (0, 0, 0, 0);

/// A cell's screen depth over its width.
///
/// **Binary** — camera float: `sin(IsoCameraAngle)`. Top and bottom
/// are divided by it before all four are divided by 32. See
/// `crate::data::settings::ISO_CAMERA_SIN`.
const fn foreshortening() -> f64 {
    crate::data::settings::ISO_CAMERA_SIN
}

/// An art rect in pixels as a [`Reach`] in cells.
///
/// **Binary** — mobject footprint, over the rect
/// `crate::sim::footprint` reads: `left, top, right, bottom` in pixels
/// about the anchor. Left and top change sign.
///
/// Both steps truncate toward zero: `__ftol` in float to int after the
/// foreshortening, then a signed division by 32. `PG07`'s 26-pixel
/// rect scales to 31.74: reach 0 truncated, 1 rounded. Rounding costs 16
/// objects and 19 draws on a 501-object map.
#[must_use]
pub fn reach(rect: [i16; 4]) -> Reach {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a sprite is a few hundred pixels across, and the float-to-int truncates"
    )]
    let deep = |px: i16| (f64::from(px) / foreshortening()) as i32 / 32;
    let across = |px: i16| i32::from(px) / 32;

    (
        -across(rect[0]),
        -deep(rect[1]),
        across(rect[2]),
        deep(rect[3]),
    )
}

/// A clump's step limit.
///
/// **Binary** — a compare with `0x100` in mobject place.
const TRIES: u32 = 256;

/// How far short of the object limit the chart stops.
///
/// **Binary** — the limit less `0xc8` in mobject chart build, and again in the
/// clump loop.
const MARGIN: u32 = 200;

/// The clump window's side.
///
/// **Binary** — 41 by 41 `Array2D` in mobject chart build.
const WINDOW: i32 = 41;

/// The clump's first point in the window.
const CENTRE: i32 = 20;

/// Object limit by side in cells.
///
/// **Binary** — world generate charts sets 3,000;
/// mobject limit scales it by width. The chart stops
/// `MARGIN` short: 550 at 128 cells.
#[must_use]
pub const fn limit(side: u32) -> u32 {
    const BASE: u32 = 3_000;

    match side {
        128 => BASE / 4,
        512 => BASE * 4,
        1024 => BASE * 8,
        _ => BASE,
    }
}

/// One entry of a zone's landscape pattern: what to scatter, and how many.
///
/// **Binary** — distribution seeds reads the entry's
/// word, the first after the weight.
///
/// The landscape entry's, not the object pattern's: the `this` in
/// mobject place is the stack slot distribution at writes. Landscape entries hold 1 to 9, object entries 0.
/// `a_clumps_size_is_the_landscape_entrys_word_and_not_the_object_patterns`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clump {
    /// The object pattern this entry names.
    pub pattern: FourCc,
    /// Target clump size.
    pub size: u32,
}

/// Which grid a placed object and its halo are marked on.
///
/// **Binary** — mobject stamp's fifth argument: the byte
/// entry byte reads, `rest[2]` in the file.
///
/// * [`Stamp::Window`] (nonzero): marks the clump's window, only where
///   `NONE`; mobject commit copies it to the map afterwards.
/// * [`Stamp::Map`] (zero): marks the map at once, unconditionally. The halo
///   then blocks the rest of the clump (a step reaches four cells, the halo's
///   width), so it rarely grows but spends its tries. It can also overwrite
///   a placed object with `FOOT`.
///
/// About half the object entries are zero. Per entry, not per code:
/// `%sno_g-tree_02` names `NGa1` twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stamp {
    /// Straight onto the chart's grid.
    Map,
    /// Into the clump's window, committed when done.
    Window,
}

/// An object pattern: weighted table and each entry's stamp.
pub struct Table {
    pub table: Distribution,
    /// One per entry of `table`, in the same order.
    pub stamps: Vec<Stamp>,
}

/// One placed object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object {
    pub at: (i32, i32),
    pub code: FourCc,
}

/// Why a point of the shuffled walk placed nothing.
///
/// **Binary** — mobject place's gates in order. Only
/// [`Refused::Kind`] and [`Refused::Fits`] come after the draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The chart's own cell already holds an object or a halo's `FOOT`.
    Taken(FourCc),
    /// The cell's region code names no zone with a landscape pattern.
    Zone(FourCc),
    /// The landscape chart's index names no entry of that pattern.
    Index(usize),
    /// The entry it names is `NONE` or [`EMPTY`].
    Empty(FourCc),
    /// The entry's object pattern is in no chart layer.
    Pattern(FourCc),
    /// Picked [`CLEAR`] (one draw), or the table was empty (none).
    Kind,
    /// The picked object's footprint does not fit. One draw, already spent.
    Fits {
        /// What the pick named.
        code: FourCc,
        /// First non-free footprint cell, x outer, y inner.
        blocked: (i32, i32),
        /// What that cell held, or `None` off the map.
        holds: Option<FourCc>,
    },
    /// Off the map; the shuffle cannot produce it.
    OffMap,
}

/// One point of the shuffled walk, and what became of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Walk {
    pub at: (i32, i32),
    /// `None` where a clump grew there.
    pub refused: Option<Refused>,
}

/// Whether the chart logs its shuffled walk. Off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trace {
    #[default]
    Off,
    On,
}

/// One clump, as the chart grew it.
///
/// For locating a draw-count residual by clump. See
/// **Measured**.
#[derive(Debug, Clone)]
pub struct Grown {
    /// The walk point.
    pub seed: (i32, i32),
    /// The zone's landscape entry: which object pattern, and how many.
    pub entry: Clump,
    /// Which grid its first object stamped on.
    pub onto: Stamp,
    /// Steps taken.
    pub tries: u32,
    /// Draws spent, first pick included.
    pub draws: u32,
    /// Every cell it stamped, in order.
    pub stamps: Vec<Object>,
}

/// What the land scatters: zones' landscape entries and object patterns.
pub struct Scatter<'a> {
    /// By region code: the zone's landscape pattern, entry by entry.
    pub zones: &'a BTreeMap<FourCc, Vec<Clump>>,
    /// By object-pattern code: the objects it may put down.
    pub objects: &'a BTreeMap<FourCc, Table>,
    /// How far an object's footprint reaches around its cell.
    ///
    /// **Binary** — mobject footprint; see the module
    /// doc and [`reach`]. Rect: `crate::data::imag::Slot::footprint`.
    ///
    /// Without art, give [`POINT`], the engine's empty-rect answer.
    pub footprint: &'a dyn Fn(FourCc) -> Reach,
    /// Whether to keep [`Chart::walked`].
    pub trace: Trace,
}

/// A clump being grown.
struct Grow<'a> {
    table: &'a Table,
    footprint: &'a dyn Fn(FourCc) -> Reach,
    seed: (i32, i32),
    size: u32,
    /// Objects placed, this clump's first included.
    placed: u32,
    /// The chart's stop count, also checked in the clump loop.
    room: u32,
}

/// The chart's grid, and what it placed.
#[derive(Debug, Clone)]
pub struct Chart {
    side: i32,
    cells: Vec<FourCc>,
    objects: Vec<Object>,
    stamps: u32,
    grown: Vec<Grown>,
    /// Walk log, only under [`Trace::On`].
    walked: Vec<Walk>,
    /// The current clump's stamps.
    trail: Vec<Object>,
}

impl Chart {
    /// Run the chart over a finished region and landscape map.
    ///
    /// A code `scatter` lacks: no draw, nothing placed.
    ///
    /// # Panics
    ///
    /// If the region chart's side does not fit in a `u32`.
    #[must_use]
    pub fn build(
        region: &region::Chart,
        landscape: &landscape::Chart,
        scatter: &Scatter<'_>,
        cap: u32,
        rng: &mut Rng,
    ) -> Self {
        let side = region.side();
        let across = u32::try_from(side).expect("a side the region chart was built with");

        let mut chart = Chart {
            side,
            cells: vec![NONE; (across * across) as usize],
            objects: Vec::new(),
            stamps: 0,
            grown: Vec::new(),
            walked: Vec::new(),
            trail: Vec::new(),
        };

        // Reused window; mobject grid clear clears it
        // per clump.
        let mut window = vec![NONE; (WINDOW * WINDOW) as usize];
        let mut placed = 0;
        let room = cap.saturating_sub(MARGIN);

        for (x, y) in shuffled_points(rng, across, across) {
            if placed >= room {
                break;
            }

            let seed = (x.cast_signed(), y.cast_signed());

            let named = chart
                .entry(region, landscape, scatter.zones, seed)
                .and_then(|entry| {
                    scatter
                        .objects
                        .get(&entry.pattern)
                        .map(|table| (entry, table))
                        .ok_or(Refused::Pattern(entry.pattern))
                });
            let (entry, table) = match named {
                Ok(pair) => pair,
                Err(why) => {
                    chart.note(scatter.trace, seed, Some(why));
                    continue;
                }
            };

            let opened = rng.draws;
            let Some((code, onto)) = kind(table, rng) else {
                chart.note(scatter.trace, seed, Some(Refused::Kind));
                continue;
            };

            if let Some(blocked) = chart.blocker(seed, (scatter.footprint)(code)) {
                let holds = chart.at(blocked);

                chart.note(
                    scatter.trace,
                    seed,
                    Some(Refused::Fits {
                        code,
                        blocked,
                        holds,
                    }),
                );
                continue;
            }

            chart.note(scatter.trace, seed, None);

            window.fill(NONE);
            chart.trail.clear();
            chart.stamp(
                &mut window,
                seed,
                seed,
                code,
                (scatter.footprint)(code),
                onto,
            );
            placed += 1;

            let clump = Grow {
                table,
                footprint: scatter.footprint,
                seed,
                size: entry.size,
                placed,
                room,
            };

            let (added, tries) = chart.grow(region, scatter.zones, &clump, &mut window, rng);

            placed += added;
            chart.commit(&window, seed);
            chart.grown.push(Grown {
                seed,
                entry,
                onto,
                tries,
                draws: rng.draws - opened,
                stamps: chart.trail.clone(),
            });
        }

        // Drop objects the finished grid lost (covered by a halo, or not
        // committed); landscape place reads the grid.
        let cells = chart.cells.clone();

        chart
            .objects
            .retain(|object| cells[at_index(object.at.1 * side + object.at.0)] == object.code);

        chart.stamps = placed;

        chart
    }

    /// A point's landscape entry, or the pre-draw gate that refused it.
    fn entry(
        &self,
        region: &region::Chart,
        landscape: &landscape::Chart,
        zones: &BTreeMap<FourCc, Vec<Clump>>,
        at: (i32, i32),
    ) -> Result<Clump, Refused> {
        let held = self.at(at).ok_or(Refused::OffMap)?;

        if held == NONE {
            let code = region.at(at).ok_or(Refused::OffMap)?;
            let entries = zones.get(&code).ok_or(Refused::Zone(code))?;
            let index = landscape.at(at).ok_or(Refused::OffMap)?;
            let entry = *entries.get(index).ok_or(Refused::Index(index))?;

            if entry.pattern == NONE || entry.pattern == EMPTY {
                Err(Refused::Empty(entry.pattern))
            } else {
                Ok(entry)
            }
        } else {
            Err(Refused::Taken(held))
        }
    }

    /// Log a walk point.
    fn note(&mut self, trace: Trace, at: (i32, i32), refused: Option<Refused>) {
        if trace == Trace::On {
            self.walked.push(Walk { at, refused });
        }
    }

    /// Grow a clump: objects added, steps taken.
    fn grow(
        &mut self,
        region: &region::Chart,
        zones: &BTreeMap<FourCc, Vec<Clump>>,
        clump: &Grow<'_>,
        window: &mut [FourCc],
        rng: &mut Rng,
    ) -> (u32, u32) {
        let zone = region.at(clump.seed);
        let mut members = 1;
        let mut tries = 0;
        let mut added = 0;

        while members < clump.size {
            let step = step(clump.seed, self.side, rng);
            let onto = region.at(step);

            // Another zone: no pick, no draw.
            if onto.is_some_and(|code| zones.contains_key(&code) && onto == zone)
                && let Some((code, mark)) = kind(clump.table, rng)
                && self.fits(step, (clump.footprint)(code))
            {
                self.stamp(
                    window,
                    clump.seed,
                    step,
                    code,
                    (clump.footprint)(code),
                    mark,
                );
                members += 1;
                added += 1;
            }

            if tries >= TRIES || clump.placed + added >= clump.room {
                break;
            }

            tries += 1;
        }

        (added, tries)
    }

    /// Copy a finished clump's window onto the map.
    ///
    /// **Binary** — mobject commit: each non-`NONE` window cell onto a `NONE`
    /// map cell.
    fn commit(&mut self, window: &[FourCc], seed: (i32, i32)) {
        for i in 0..WINDOW {
            for j in 0..WINDOW {
                let mark = window[at_index(j * WINDOW + i)];
                let at = (seed.0 - CENTRE + i, seed.1 - CENTRE + j);

                if mark != NONE && self.at(at) == Some(NONE) {
                    self.cells[at_index(at.1 * self.side + at.0)] = mark;
                }
            }
        }
    }

    /// Mark an object and its halo on its entry's grid.
    ///
    /// **Binary** — mobject stamp. Rect grown by
    /// [`HALO`], clamped to the map. See [`Stamp`].
    fn stamp(
        &mut self,
        window: &mut [FourCc],
        seed: (i32, i32),
        at: (i32, i32),
        code: FourCc,
        reach: Reach,
        onto: Stamp,
    ) {
        let last = self.side - 1;
        let halo = ((at.0 - reach.0 - HALO).clamp(0, last)
            ..=(at.0 + reach.2 + HALO).clamp(0, last))
            .flat_map(|x| {
                ((at.1 - reach.1 - HALO).clamp(0, last)..=(at.1 + reach.3 + HALO).clamp(0, last))
                    .map(move |y| (x, y))
            });

        for cell in halo {
            match onto {
                // Unconditional: can cover a placed object.
                Stamp::Map => self.cells[at_index(cell.1 * self.side + cell.0)] = FOOT,
                Stamp::Window => {
                    let index = in_window(seed, cell);

                    if window[index] == NONE {
                        window[index] = FOOT;
                    }
                }
            }
        }

        match onto {
            Stamp::Map => self.cells[at_index(at.1 * self.side + at.0)] = code,
            Stamp::Window => window[in_window(seed, at)] = code,
        }

        // Clump members can share a cell (fits reads the map, the clump writes
        // the window; one save stamps `(1, 53)` three times). Last write wins.
        self.objects.retain(|object| object.at != at);
        self.objects.push(Object { at, code });
        self.trail.push(Object { at, code });
    }

    /// Whether an object's whole footprint is free ground.
    ///
    /// **Binary** — mobject fits.
    ///
    /// Occupancy only: no class, slope, zone or distance. `top < 0`,
    /// `left < 0`, `bottom >= height` or `right >= width` refuses;
    /// `left > right` accepts; then inclusive ranges, x outer, via
    /// map cell at, failing on the first non-`NONE`.
    ///
    /// Map grid, whole cells, not the window. Bounds at
    /// `this`/ equal the `Array2D` dims by
    /// construction in rgs chart ctor.
    fn fits(&self, at: (i32, i32), reach: Reach) -> bool {
        self.blocker(at, reach).is_none()
    }

    /// The first non-free footprint cell, in mobject fits's order.
    fn blocker(&self, at: (i32, i32), reach: Reach) -> Option<(i32, i32)> {
        (at.0 - reach.0..=at.0 + reach.2)
            .flat_map(|x| (at.1 - reach.1..=at.1 + reach.3).map(move |y| (x, y)))
            .find(|cell| self.at(*cell) != Some(NONE))
    }

    /// What a cell holds, or `None` off the map.
    #[must_use]
    pub fn at(&self, at: (i32, i32)) -> Option<FourCc> {
        let (x, y) = at;

        if (0..self.side).contains(&x) && (0..self.side).contains(&y) {
            self.cells
                .get(usize::try_from(y * self.side + x).ok()?)
                .copied()
        } else {
            None
        }
    }

    /// Every placed object.
    #[must_use]
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    /// Every clump, in walk order.
    #[must_use]
    pub fn grown(&self) -> &[Grown] {
        &self.grown
    }

    /// Every walk point and its refusal; empty unless [`Trace::On`].
    #[must_use]
    pub fn walked(&self) -> &[Walk] {
        &self.walked
    }

    /// Stamp count, the counter the cap tests. Exceeds [`Chart::objects`]:
    /// repeated cells count twice.
    #[must_use]
    pub const fn stamps(&self) -> u32 {
        self.stamps
    }

    #[must_use]
    pub const fn side(&self) -> i32 {
        self.side
    }
}

/// A grid index the caller has bounded.
fn at_index(at: i32) -> usize {
    usize::try_from(at).expect("a cell inside the grid")
}

/// One object and its stamp, one draw.
///
/// **Binary** — mobject kind. Empty table: no draw.
/// `CLER`: draw, nothing placed. Reads the entry's code and stamp
/// byte, so it picks a position.
///
/// Its test of the pattern code against `NONE` cannot fire: the loader
/// drops such records (`crate::data::rgs::chart`).
fn kind(pattern: &Table, rng: &mut Rng) -> Option<(FourCc, Stamp)> {
    if pattern.table.is_empty() {
        None
    } else {
        pattern
            .table
            .pick_entry(rng)
            .and_then(|n| Some((pattern.table.at(n)?, *pattern.stamps.get(n)?)))
            .filter(|(code, _)| *code != CLEAR)
    }
}

/// One step from a clump's first point, three draws.
///
/// **Binary** — `rng_next(8)` in mobject place for direction (compass
/// from north-west, clockwise), then mobject step:
/// two `rng_next(4) + 1`. Never the point itself.
///
/// The draws are not x and y: first in `esi`, second in `ecx`, and the switch
/// uses them per case. Diagonals: `esi` x, `ecx` y. East,
/// west and north: `esi`. South: `ecx`.
fn step(from: (i32, i32), side: i32, rng: &mut Rng) -> (i32, i32) {
    let direction = rng.below(8) + 1;
    let first = rng.below(4) + 1;
    let second = rng.below(4) + 1;
    let (x, y) = from;

    let (x, y) = match direction {
        1 => (x - first, y - second),
        2 => (x, y - first),
        3 => (x + first, y - second),
        4 => (x + first, y),
        5 => (x + first, y + second),
        6 => (x, y + second),
        7 => (x - first, y + second),
        _ => (x - first, y),
    };

    // Clamped, so region at cell, which would
    // dereference null off the grid, is never asked.
    (x.clamp(0, side - 1), y.clamp(0, side - 1))
}

/// Where a map cell sits in the clump window about `seed`.
///
/// **Binary** — the offsets and `0x28` clamp in the mobject stamp.
fn in_window(seed: (i32, i32), at: (i32, i32)) -> usize {
    let x = (at.0 - seed.0 + CENTRE).clamp(0, WINDOW - 1);
    let y = (at.1 - seed.1 + CENTRE).clamp(0, WINDOW - 1);

    at_index(y * WINDOW + x)
}
