//! Ground a building takes when placed.
//!
//! **Binary** — `BuildingRec` runs
//! Building flatten twice, then
//! `GE_Level` method `(unit, 'GR00')` (call in `BuildingRec`).
//!
//! **Game** — runs in play at construction start; destroying the building
//! does not restore the ground. A new game's start runs it too
//! (`World::fresh`).
//!
//! # The shape is a polygon
//!
//! The mask comes from `GenUnit` via
//! Footprint mask build: point list 0 of the art's slot,
//! rasterised. See [`crate::data::imag::Slot::points`]. The rect from
//! Footprint rect build is the fallback for a slot with no polygon; no
//! shipped building lacks one.
//!
//! `ABY1 Wizards_Guild1`: six points, 44 cells, against a 30-cell rect.
//!
//! ```text
//! ...#########..        a half cell is set if ANY of its four
//! ..##########..        corners is inside the polygon:
//! .############.        touching is covering
//! .############.
//! #############.
//!     ##############.
//!     ###############
//!     ###############
//! ##############
//! ..############
//! ....#########.
//! .......#####..
//! ```
//!
//! # The arithmetic
//!
//! ```text
//! points are i32 SCREEN pixels about the slot's anchor
//!   y = trunc(y * 32 / (sin(IsoCameraAngle) * 32 as f32))
//!                                     -- IsoFootprintRectManager
//!                                        WHOLE ground pixels, 32 to a cell
//!   x0 = floor(min x / 16)            -- HALF cells; the engine floors and
//!   y0 = floor(min y / 16)               ceils by adding -0xf or +0xf
//!   x1 = ceil (max x / 16)
//!   y1 = ceil (max y / 16)
//!   half cell (i, j) is set if any of (16i, 16i+15) x (16j, 16j+15) is
//!   inside the polygon, by even-odd crossing -- point in polygon
//!
//! all integer, since the unprojection truncates first
//!
//! per set half cell, the CELL painted is
//!   ((px + x0 + i) / 2, (py + y0 + j) / 2)
//! where (px, py) is the building's position IN HALF CELLS.
//! ```
//!
//! # Scope
//!
//! Names cells; does not paint them. Painting is the engine's cell paint and the
//! blend cascade, which resolves Dirt against a class with no blend art.
//! Writing class 0 straight into a cell field gives tiles the tileset lacks.
//!
//! # The flatten
//!
//! [`flatten`] runs first: two building flatten calls over the same
//! mask, phase 0 then phase 1, before the Dirt.
//!
//! Not gated. No bit-9 test exists: MSVC emitted one accessor per `Info` bit
//! used, and those are bits 2, 8, 11, 15, 17 and 18. `BuildingRec`'s one
//! gate encloses flatten and paint; `GE_Level` tests bit 8 itself at
//! `GE_Level`. So every building flattens, and only the Dirt is conditional.
//! Data cannot tell the readings apart (all 19 descriptors with bit 9 also
//! have bit 8), so this rests on the binary.
//! [`shape`] is the ungated mask, [`pad`] the gated one.

use std::collections::BTreeMap;

use crate::data::desc::Info;
use crate::data::imag;
use crate::data::settings::ISO_CAMERA_SIN;

use super::slope::{Flood, MAX_SLOPE_TAN, Slope, Spread};
use super::units::{PER_HALF, PIXELS_PER_CELL, PIXELS_PER_HALF};
use crate::base::fixed::{Fixed, ONE, div, mul, whole};
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;

/// Sets searched for a polygon, in order.
///
/// **Binary** — footprint mask build asks for set 8,
/// slot 0, list 0; `IsoFootprintRectManager` falls back
/// to set `0xd0` when set 8 is missing or its list is empty.
/// Slot point list matches the whole set id, so layer
/// 0 only. The mask is cached in the descriptor's `PRIM`, so
/// every placement of one descriptor gets one pad.
const SETS: [u32; 2] = [imag::SetKind::Stand.id(), imag::SetKind::Inactive.id()];

/// Where inside a half cell the rasteriser samples.
///
/// **Binary** — `FootprintRectManager` tests the corners
/// at 0 and 15; 16 is the next half cell.
const SAMPLES: [i32; 2] = [0, 15];

/// A building's pad, in half cells about its own half-cell position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pad {
    /// Lowest covered half cell, relative to the building.
    pub origin: (i32, i32),
    /// Width and height in half cells.
    pub size: (u32, u32),
    /// Row-major, `size.0 * size.1` long.
    pub covered: Vec<bool>,
}

impl Pad {
    /// Whether the half cell at this offset from [`Pad::origin`] is covered.
    #[must_use]
    pub fn at(&self, i: u32, j: u32) -> bool {
        if i < self.size.0 && j < self.size.1 {
            self.covered
                .get((j * self.size.0 + i) as usize)
                .copied()
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// This pad less its last column and row: a rect fallback's
    /// (`from_rect`, inclusive as the paint loops it) as
    /// building flatten loops it, half open (building flatten,
    /// building flatten).
    #[must_use]
    pub fn half_open(&self) -> Pad {
        let size = (self.size.0.saturating_sub(1), self.size.1.saturating_sub(1));
        let covered = (0..size.1)
            .flat_map(|j| (0..size.0).map(move |i| (i, j)))
            .map(|(i, j)| self.at(i, j))
            .collect();

        Pad {
            origin: self.origin,
            size,
            covered,
        }
    }

    /// Covered half cells, about the unit's own.
    pub fn halves(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        (0..self.size.1)
            .flat_map(|j| (0..self.size.0).map(move |i| (i, j)))
            .filter(|(i, j)| self.at(*i, *j))
            .map(|(i, j)| {
                (
                    self.origin.0 + i.cast_signed(),
                    self.origin.1 + j.cast_signed(),
                )
            })
    }

    /// Whole cells painted for a building at `at` (half cells), deduplicated.
    #[must_use]
    pub fn cells(&self, at: (i32, i32)) -> Vec<(i32, i32)> {
        let mut out: Vec<(i32, i32)> = (0..self.size.1)
            .flat_map(|j| (0..self.size.0).map(move |i| (i, j)))
            .filter(|(i, j)| self.at(*i, *j))
            .map(|(i, j)| {
                (
                    (at.0 + self.origin.0 + i.cast_signed()).div_euclid(2),
                    (at.1 + self.origin.1 + j.cast_signed()).div_euclid(2),
                )
            })
            .collect();

        out.sort_unstable();
        out.dedup();
        out
    }
}

/// The pad a unit lays.
///
/// `None` without [`Info::ModifyTerrainTextureOnPlacement`] (bit 8 of the
/// `PRIM` info word, the engine's gate), or with neither polygon nor rect.
///
/// # Errors
///
/// If the art will not parse.
pub fn pad(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> crate::data::error::Result<Option<Pad>> {
    // 71 HD base descriptors carry it.
    if unit
        .desc
        .engine()
        .has(Info::ModifyTerrainTextureOnPlacement)
    {
        shape(catalogue, unit)
    } else {
        Ok(None)
    }
}

/// [`pad`] without the gate, for [`flatten`].
///
/// **Binary** — building flatten and `GE_Level` both take
/// `GenUnit` method `(0)`; only the Dirt is gated.
///
/// # Errors
///
/// If the art will not parse.
pub fn shape(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
) -> crate::data::error::Result<Option<Pad>> {
    // Rect is the fallback for art with no polygon.
    match polygon(unit) {
        Some(points) if points.len() >= 3 => Ok(Some(rasterise(&points))),
        _ => Ok(super::footprint::of_unit(catalogue, unit)?.map(from_rect)),
    }
}

/// A unit's pad rect: `[x0, y0, x1, y1]` in half cells about its half-cell
/// position, half-open. Distances between units are measured between these.
///
/// **Binary** — `PRIM` `+0x14`..`+0x20`, cached by
/// footprint rect build: the polygon's bounds, else
/// the art's rect, both in ground pixels; minima floored, maxima ceiled over
/// 16. `None` with no art.
///
/// # Errors
///
/// If the art will not parse.
pub fn pad_rect(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
) -> crate::data::error::Result<Option<[i32; 4]>> {
    let bounds = pixel_rect(catalogue, unit)?;

    Ok(bounds.map(|[x0, y0, x1, y1]| {
        let ceil = |v: i32| (v + PIXELS_PER_HALF - 1).div_euclid(PIXELS_PER_HALF);

        [
            x0.div_euclid(PIXELS_PER_HALF),
            y0.div_euclid(PIXELS_PER_HALF),
            ceil(x1),
            ceil(y1),
        ]
    }))
}

/// [`pad_rect`] before the rounding, in ground pixels: `PRIM`.
/// A field, which footprint rect build fills first;
/// `None` with no art.
///
/// # Errors
///
/// If the art will not parse.
pub fn pixel_rect(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
) -> crate::data::error::Result<Option<[i32; 4]>> {
    Ok(match polygon(unit) {
        Some(points) if !points.is_empty() => {
            let low = |pick: fn(&(i32, i32)) -> i32| points.iter().map(pick).min().unwrap_or(0);
            let high = |pick: fn(&(i32, i32)) -> i32| points.iter().map(pick).max().unwrap_or(0);

            Some([low(|p| p.0), low(|p| p.1), high(|p| p.0), high(|p| p.1)])
        }
        _ => super::footprint::of_unit(catalogue, unit)?
            .map(|[left, top, right, bottom]| {
                let deep = |v: i16| unproject(0, i32::from(v)).1;

                [i32::from(left), deep(top), i32::from(right), deep(bottom)]
            })
            .or_else(|| unit.art.is_some().then_some(UNDRAWN)),
    })
}

/// The pixel rect of a unit whose art draws none of the footprint's sets
/// (a particle system's spell unit, `fire_ball`): frame box union
/// zeroes its rect before it unions, so the unit measures
/// as a point at its half cell. **Binary**.
const UNDRAWN: [i32; 4] = [0; 4];

/// How the collision grid keeps a unit's half cells on the map.
///
/// **Binary**: mask blocked blocks where a covered
/// half cell lies off the map; rect blocked where
/// the half-open rect's start is below 0 or its end not below the map's
/// size, so a rect never covers the last row or column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// A polygon's mask: each covered half cell on the map.
    Mask,
    /// The pad rect: each half cell and the one past it on the map.
    Rect,
}

/// Place search for a unit taking `cells` about its
/// half cell, kept on a map of `grid` half cells by `edge`, from `at` in
/// 32nds: the nearest half cell where [`blocked`] finds none of them off
/// the map or `taken`, in 32nds. `None` when the search finds none
/// ([`super::space::search`]).
pub fn search(
    footprint: (&[(i32, i32)], Edge),
    grid: (i32, i32),
    at: (i32, i32),
    taken: impl Fn((i32, i32)) -> bool,
) -> Option<(i32, i32)> {
    let from = (at.0 / PER_HALF, at.1 / PER_HALF);

    super::space::search(from, grid, blocked(footprint, grid, taken))
        .map(|(x, y)| (x * PER_HALF, y * PER_HALF))
}

/// Footprint blocked: whether a unit taking `cells`
/// about a half cell has one off a map of `grid` half cells, or `taken`.
/// A pad rect, half open, never takes the last row or column
/// (rect blocked, [`Edge::Rect`]).
pub fn blocked(
    (cells, edge): (&[(i32, i32)], Edge),
    grid: (i32, i32),
    taken: impl Fn((i32, i32)) -> bool,
) -> impl Fn((i32, i32)) -> bool {
    let off = move |(x, y): (i32, i32)| !(0..grid.0).contains(&x) || !(0..grid.1).contains(&y);
    let past = match edge {
        Edge::Mask => 0,
        Edge::Rect => 1,
    };

    move |(x, y): (i32, i32)| {
        cells.iter().any(|(dx, dy)| {
            let at = (x + dx, y + dy);

            off(at) || off((at.0 + past, at.1 + past)) || taken(at)
        })
    }
}

/// Which of [`Edge`]'s tests the grid puts `unit`'s [`taken`] cells to.
///
/// Art that will not parse reads as no polygon, [`Edge::Rect`]: [`taken`]
/// reports the error for the same unit.
#[must_use]
pub fn edge(_catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> Edge {
    match polygon(unit) {
        Some(points) if points.len() >= 3 => Edge::Mask,
        _ => Edge::Rect,
    }
}

/// The half cells a unit takes on the collision grid, about its own: its
/// polygon's mask, else its pad rect, half-open
/// (rect blocked; unit grid register registers the same
/// cells). `None` with neither.
///
/// # Errors
///
/// If the art will not parse.
pub fn taken(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
) -> crate::data::error::Result<Option<Vec<(i32, i32)>>> {
    match polygon(unit) {
        Some(points) if points.len() >= 3 => {
            let pad = rasterise(&points);
            let cells = (0..pad.size.1)
                .flat_map(|j| (0..pad.size.0).map(move |i| (i, j)))
                .filter(|(i, j)| pad.at(*i, *j))
                .map(|(i, j)| {
                    (
                        pad.origin.0 + i.cast_signed(),
                        pad.origin.1 + j.cast_signed(),
                    )
                })
                .collect();

            Ok(Some(cells))
        }
        _ => Ok(pad_rect(catalogue, unit)?.map(|[x0, y0, x1, y1]| {
            (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .collect()
        })),
    }
}

/// The set a spawner's exit point is read from: `Inactive`, variant 0.
pub const EXIT_SET: u32 = imag::SetKind::Inactive.id();

/// The exit is block 2's second point: `GenUnit` keeps
/// words 2 and 3 of the rect `IsoGenUnit` gives.
pub const EXIT_POINT: usize = 5;

/// Where a unit spawned by this one appears, in ground pixels about its
/// position: `GenUnit`, through `IsoGenUnit`
/// with set [`EXIT_SET`], block 2 of slot 0, `y` unprojected as the
/// footprint's is. `(0, 0)` without the set, as method falls back to the
/// position. `None` without art. [`exits`] gives every slot's.
///
/// # Errors
///
/// If the art will not parse.
pub fn exit(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
) -> crate::data::error::Result<Option<(i32, i32)>> {
    Ok(exits(catalogue, unit)?.map(|all| all.first().copied().unwrap_or_default()))
}

/// [`exit`] for each slot of [`EXIT_SET`], in slot order: the engine reads the
/// slot the unit's layer draws (`UnitIMGPartEntry`). Empty without
/// the set. `None` without art.
///
/// **Game**: play5's Rat's Nest, drawn in slot 1, spawns its ratmen from
/// slot 1's point, not slot 0's.
///
/// # Errors
///
/// If the art will not parse.
pub fn exits(
    _catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
) -> crate::data::error::Result<Option<Vec<(i32, i32)>>> {
    let Some(image) = unit.image.as_ref() else {
        return Ok(None);
    };
    let points = image
        .sets
        .iter()
        .find(|s| s.id == EXIT_SET)
        .map(|s| {
            s.slots
                .iter()
                .map(|slot| {
                    let (x, y) = slot.points()[EXIT_POINT];

                    unproject(i32::from(x), i32::from(y))
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Some(points))
}

/// Point list 0 of slot 0 of the first of [`SETS`] holding one, unprojected
/// to ground pixels.
pub(super) fn polygon(unit: &Unit<'_>) -> Option<Vec<(i32, i32)>> {
    let polygon = |image: &imag::Image<'_>| {
        SETS.iter().find_map(|id| {
            let found = image.sets.iter().find(|s| s.id == *id)?;
            let list = found.slots.first()?.point_lists().first()?;

            (!list.is_empty()).then(|| list.iter().map(|&(x, y)| unproject(x, y)).collect())
        })
    };

    unit.image.as_ref().and_then(polygon)
}

/// A screen point as ground pixels.
///
/// **Binary** — `IsoFootprintRectManager`: `fild` of
/// `y << 5`, `fdiv` by camera `0x28` (`sin(IsoCameraAngle) * 32` as `f32`),
/// `__ftol` in float to int (truncates toward zero).
#[allow(
    clippy::cast_possible_truncation,
    reason = "the truncation is the instruction, and a sprite is a few hundred \
              pixels about its anchor"
)]
pub fn unproject(x: i32, y: i32) -> (i32, i32) {
    // The cell does not cancel: `y` is multiplied by it in `i32` and divided
    // by `sin` times it in `f32`. Plain `y / sin` moves boundary points.
    let per_cell = f64::from(PIXELS_PER_CELL);
    let deep = (ISO_CAMERA_SIN * per_cell) as f32;

    (x, (f64::from(y) * per_cell / f64::from(deep)) as i32)
}

/// Rasterise a polygon into half cells.
pub(super) fn rasterise(points: &[(i32, i32)]) -> Pad {
    let span = |pick: fn(&(i32, i32)) -> i32| {
        let low = points.iter().map(pick).min().unwrap_or(0);
        let high = points.iter().map(pick).max().unwrap_or(0);

        // Floor the minimum, ceil the maximum.
        (
            low.div_euclid(PIXELS_PER_HALF),
            (high + PIXELS_PER_HALF - 1).div_euclid(PIXELS_PER_HALF),
        )
    };
    let (x0, x1) = span(|p| p.0);
    let (y0, y1) = span(|p| p.1);
    let (width, height) = (
        (x1 - x0).max(0).cast_unsigned(),
        (y1 - y0).max(0).cast_unsigned(),
    );

    // The crossings depend on the row alone: found once per sampled row, not
    // once per sample.
    let at = |base: i32, step: u32, d: i32| (base + step.cast_signed()) * PIXELS_PER_HALF + d;
    let rows: Vec<[Vec<Cut>; SAMPLES.len()]> = (0..height)
        .map(|j| SAMPLES.map(|dy| cuts(points, at(y0, j, dy))))
        .collect();
    let covered = rows
        .iter()
        .flat_map(|row| {
            (0..width).map(move |i| {
                SAMPLES.iter().any(|dx| {
                    let px = at(x0, i, *dx);

                    row.iter().any(|cuts| odd(cuts, px))
                })
            })
        })
        .collect();

    Pad {
        origin: (x0, y0),
        size: (width, height),
        covered,
    }
}

/// The rect fallback, as a full pad.
///
/// **Binary** — map paint rect loops inclusive on both
/// bounds, so one more half cell each way than half-open.
fn from_rect(rect: super::footprint::ArtRect) -> Pad {
    let [left, top, right, bottom] = rect;
    let span = |low: i16, high: i16, foreshortened: bool| {
        let scale = |v: i16| {
            if foreshortened {
                unproject(0, i32::from(v)).1
            } else {
                i32::from(v)
            }
        };

        (
            scale(low).div_euclid(PIXELS_PER_HALF),
            (scale(high) + PIXELS_PER_HALF - 1).div_euclid(PIXELS_PER_HALF),
        )
    };
    let (x0, x1) = span(left, right, false);
    let (y0, y1) = span(top, bottom, true);
    let (width, height) = (
        (x1 - x0 + 1).max(0).cast_unsigned(),
        (y1 - y0 + 1).max(0).cast_unsigned(),
    );

    Pad {
        origin: (x0, y0),
        size: (width, height),
        covered: vec![true; (width * height) as usize],
    }
}

/// A polygon edge that crosses a row: its ends' x, and where it cuts the
/// row.
type Cut = (i32, i32, i32);

/// The edges of a closed integer polygon that cross row `py`.
///
/// **Binary** — point in polygon. Differs from the
/// textbook: every comparison is `<=`, and the crossing uses a truncating
/// integer divide. Starts at the last point, so the polygon wraps closed.
fn cuts(points: &[(i32, i32)], py: i32) -> Vec<Cut> {
    let last = points.last().copied();

    last.into_iter()
        .chain(points.iter().copied())
        .zip(points)
        .filter(|(prev, cur)| (py <= prev.1) != (py <= cur.1))
        .map(|(prev, cur)| {
            let cut = cur.0 - (prev.0 - cur.0) * (cur.1 - py) / (prev.1 - cur.1);

            (prev.0, cur.0, cut)
        })
        .collect()
}

/// Even-odd crossing test of the point at `px` along a row's [`cuts`].
fn odd(cuts: &[Cut], px: i32) -> bool {
    cuts.iter().fold(false, |odd, &(prev, cur, cut)| {
        if (px <= prev) == (px <= cur) {
            // Both ends on one side in x.
            odd ^ (px <= prev)
        } else {
            odd ^ (px <= cut)
        }
    })
}

/// Square grid of vertex heights.
///
/// Read-only: [`flatten`] returns changes for the caller to apply.
pub trait Heights {
    /// Vertices a side. Cells are one fewer.
    fn across(&self) -> u32;

    /// The height at a vertex, or `None` past the edge.
    fn height(&self, x: u32, y: u32) -> Option<Fixed>;
}

/// Vertices a flatten moved, and their new heights.
pub type Plateau = BTreeMap<(u32, u32), Fixed>;

/// Levels the ground under a placed building.
///
/// **Binary** — the engine's building flatten, called from
/// `BuildingRec` (phase 0, then phase 1), sharing one height between calls.
/// `at` is in half cells; `pad` is from [`shape`]. Not gated; see the module header.
///
/// # The two phases
///
/// ```text
/// phase 0   sum the heights under the mask, once per covered HALF cell,
///           so a vertex two half cells cover is counted twice
///           mean = fixed_div(sum, count << 16)
///           mean = ramp(mean)                         IsoMap
/// phase 1   for each covered half cell, in mask order, j outer:
///               vertex = mean
///               flood the slope clamp out from it     IsoMap
/// ```
///
/// Plateau at `mean`, ring pulled to within [`MAX_SLOPE_TAN`]. On generated
/// ground the pull does nothing: 230 vertices written over six
/// buildings, all plateau (**Measured**).
///
/// # Not done here
///
/// Phase 1 also grows the rect by one, re-seats other agents in it,
/// relights via `Map` and marks vertices
/// dirty. No height moves. The
/// relight is [`super::light::bake`]; the re-seat is the caller's, over
/// [`reseated`].
pub fn flatten(ground: &dyn Heights, at: (i32, i32), pad: &Pad) -> Plateau {
    let covered = under(ground.across(), at, pad);
    let sum = covered
        .iter()
        .filter_map(|v| ground.height(v.0, v.1))
        .fold(Fixed::ZERO, Fixed::wrapping_add);
    let mut out = Plateau::new();

    // Engine faults on a divide for an empty mask; no counterpart.
    if let Ok(count) = i32::try_from(covered.len())
        && count > 0
    {
        let mean = ramp(div(sum, count * ONE), rect(at, pad), ground.across());
        let mut flood = Flood::new(ground.across());

        // Clamp between writes, in mask order: later vertices see earlier
        // moves.
        for (x, y) in covered {
            out.insert((x, y), mean);
            flood.run(
                &mut Overlay {
                    ground,
                    out: &mut out,
                },
                (x, y),
                MAX_SLOPE_TAN,
                Spread::WhereItMoved,
            );
        }
    }

    out
}

/// Vertices a mask covers, with repeats, in engine order.
///
/// Up to four half cells land on one vertex; phase 0 counts each. `j` outer,
/// `i` inner. `across` is vertices a side.
pub fn under(across: u32, at: (i32, i32), pad: &Pad) -> Vec<(u32, u32)> {
    // Bound is width/height at `0x50`/`0x54` of `level`, **Inferred**
    // to be the map in half cells. No corpus save tests it. Decides only
    // whether the last vertex row and column can flatten; here they cannot.
    let span = 2 * across.saturating_sub(1).cast_signed();
    let (x0, y0) = (at.0 + pad.origin.0, at.1 + pad.origin.1);

    (0..pad.size.1)
        .flat_map(|j| (0..pad.size.0).map(move |i| (i, j)))
        .filter(|(i, j)| pad.at(*i, *j))
        .filter_map(|(i, j)| {
            let (hx, hy) = (x0 + i.cast_signed(), y0 + j.cast_signed());

            ((0..span).contains(&hx) && (0..span).contains(&hy))
                .then(|| ((hx / 2).cast_unsigned(), (hy / 2).cast_unsigned()))
        })
        .collect()
}

/// Mask bounding box in whole cells — building flatten.
const fn rect(at: (i32, i32), pad: &Pad) -> [i32; 4] {
    let (x0, y0) = (at.0 + pad.origin.0, at.1 + pad.origin.1);
    let (x1, y1) = (x0 + pad.size.0.cast_signed(), y0 + pad.size.1.cast_signed());

    [x0 / 2, y0 / 2, x1 / 2, y1 / 2]
}

/// The half cells phase 1 reseats units in, half open: the mask's cell
/// rect grown by one, kept to the vertices, doubled, then
/// agents in rect. `across` is vertices a side.
#[must_use]
pub fn reseated(at: (i32, i32), pad: &Pad, across: u32) -> [i32; 4] {
    let [x0, y0, x1, y1] = rect(at, pad);
    let last = across.cast_signed() - 1;

    [
        2 * (x0 - 1).max(0),
        2 * (y0 - 1).max(0),
        2 * (x1 + 1).min(last),
        2 * (y1 + 1).min(last),
    ]
}

/// Caps the plateau so ground can ramp to zero at the map's top and bottom
/// edges.
///
/// **Binary** — `IsoMap` over `IsoMap`,
/// on phase 0's mean. Uses y bounds only (`rect + 4`,
/// `rect`). Never fires on generated ground.
fn ramp(mean: Fixed, rect: [i32; 4], across: u32) -> Fixed {
    let step = |h: Fixed, cells: i32| {
        if cells - 1 <= whole(div(h, MAX_SLOPE_TAN)) {
            mul(MAX_SLOPE_TAN, (cells - 1) * ONE)
        } else {
            h
        }
    };

    step(step(mean, rect[1]), across.cast_signed() - rect[3])
}

/// The ground as a flatten sees it: its own writes over the heights it
/// was given.
struct Overlay<'a> {
    ground: &'a dyn Heights,
    out: &'a mut Plateau,
}

impl Slope for Overlay<'_> {
    fn height(&self, x: u32, y: u32) -> Option<Fixed> {
        self.out.get(&(x, y)).copied().or(self.ground.height(x, y))
    }

    fn set(&mut self, x: u32, y: u32, height: Fixed) {
        self.out.insert((x, y), height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Height grid for tests.
    struct Slab {
        across: u32,
        heights: Vec<Fixed>,
    }

    impl Heights for Slab {
        fn across(&self) -> u32 {
            self.across
        }

        fn height(&self, x: u32, y: u32) -> Option<Fixed> {
            (x < self.across && y < self.across)
                .then(|| self.heights.get((y * self.across + x) as usize).copied())
                .flatten()
        }
    }

    /// Solid pad of `size` half cells at the building.
    fn solid(size: (u32, u32)) -> Pad {
        Pad {
            origin: (0, 0),
            size,
            covered: vec![true; (size.0 * size.1) as usize],
        }
    }

    /// The engine's test as first written, one polygon walk a point.
    fn walked(points: &[(i32, i32)], (px, py): (i32, i32)) -> bool {
        let Some(mut prev) = points.last().copied() else {
            return false;
        };
        let mut odd = false;

        for cur in points.iter().copied() {
            if (py <= prev.1) != (py <= cur.1) {
                if (px <= prev.0) == (px <= cur.0) {
                    odd ^= px <= prev.0;
                } else {
                    odd ^= px <= cur.0 - (prev.0 - cur.0) * (cur.1 - py) / (prev.1 - cur.1);
                }
            }

            prev = cur;
        }

        odd
    }

    #[test]
    fn a_row_of_cuts_tests_as_the_walk_does() {
        let shapes: [&[(i32, i32)]; 3] = [
            &[(3, -20), (90, 5), (41, 77), (-8, 40), (30, 30), (12, -3)],
            &[(0, 0), (64, 0), (64, 48), (0, 48)],
            &[],
        ];

        for points in shapes {
            let pad = rasterise(points);
            let (x0, y0) = pad.origin;

            for j in 0..pad.size.1 {
                for i in 0..pad.size.0 {
                    let at = |base: i32, step: u32, d: i32| {
                        (base + step.cast_signed()) * PIXELS_PER_HALF + d
                    };
                    let walked = SAMPLES.iter().any(|dx| {
                        SAMPLES
                            .iter()
                            .any(|dy| walked(points, (at(x0, i, *dx), at(y0, j, *dy))))
                    });

                    assert_eq!(pad.at(i, j), walked, "{points:?} at {i},{j}");
                }
            }
        }
    }

    /// A rect fallback's flatten pad is its paint pad less the last column
    /// and row (building flatten, half open).
    #[test]
    fn a_rect_levels_one_half_cell_short_of_its_paint() {
        let flat = solid((4, 4)).half_open();

        assert_eq!(flat.size, (3, 3));
        assert_eq!(flat.halves().count(), 9);
        assert_eq!(flat.origin, (0, 0));
    }

    /// Plateau is the flat mean.
    #[test]
    fn a_building_levels_the_ground_to_the_mean_under_it() {
        let ground = Slab {
            across: 9,
            heights: (0..81).map(|i| (i % 3) * (ONE / 4)).collect(),
        };
        // Four half cells at (8, 8): one vertex, (4, 4).
        let got = flatten(&ground, (8, 8), &solid((2, 2)));

        assert_eq!(got.get(&(4, 4)), Some(&(ground.height(4, 4).expect("it"))));
        assert_eq!(got.len(), 1, "one half-cell square is one vertex");
    }

    /// Mean sums per half cell, not per vertex.
    #[test]
    fn the_mean_counts_a_vertex_once_per_half_cell() {
        let mut heights = vec![Fixed::ZERO; 81];

        heights[4 * 9 + 4] = 4 * ONE;

        let ground = Slab { across: 9, heights };
        // (8, 8), (9, 8) -> vertex (4, 4); (10, 8) -> (5, 4). (4 + 4 + 0) / 3.
        let got = flatten(&ground, (8, 8), &solid((3, 1)));

        assert_eq!(got.get(&(4, 4)), Some(&div(8 * ONE, 3 * ONE)));
    }

    /// Too-steep ground round the plateau is pulled in.
    #[test]
    fn the_flatten_pulls_a_neighbour_that_is_too_steep() {
        let mut heights = vec![Fixed::ZERO; 81];

        heights[4 * 9 + 5] = 9 * ONE;
        heights[4 * 9 + 6] = 9 * ONE;

        let ground = Slab { across: 9, heights };
        let got = flatten(&ground, (8, 8), &solid((2, 2)));

        assert_eq!(got.get(&(4, 4)), Some(&Fixed::ZERO), "the plateau");
        assert_eq!(
            got.get(&(5, 4)),
            Some(&MAX_SLOPE_TAN),
            "the neighbour, pulled to one step away"
        );
        assert_eq!(
            got.get(&(6, 4)),
            Some(&(2 * MAX_SLOPE_TAN)),
            "and the walk carried on past it"
        );
    }

    /// Gentle ground: only the plateau moves.
    #[test]
    fn the_clamp_leaves_ground_that_is_already_gentle_alone() {
        let ground = Slab {
            across: 9,
            heights: vec![ONE / 2; 81],
        };
        let got = flatten(&ground, (8, 8), &solid((4, 4)));

        assert_eq!(got.len(), 4, "two cells a side is four vertices");
        assert!(got.values().all(|h| *h == ONE / 2));
    }
}
