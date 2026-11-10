//! The minimap: `GSOverheadGameView`, view id 5, in `APMK` control 2001.
//! **Binary** unless marked; the whole
//! reading is in `docs/original/minimap.md`.
//!
//! ```text
//!  a view restored   ── control 2001's rect ──> GSOverheadGameView
//!           scale = rect width / map cells (16.16)
//!  a frame, per dirty rect (OverheadGameView):
//!           the ground, from a cached picture       ground()
//!           units: buildings, characters, flags,    shown(), colour(),
//!           message flags, each list in order       square(), blinked()
//!           the views' rectangles, 1 px outlines    viewport(), outline()
//! ```
//!
//! The ground is the map's own terrain tiles, sampled, unlit: each cell
//! writes a `block` x `block` square of its tile's pixels, at
//! `floor(cell * scale)`. Unexplored ground is black. Nothing else is
//! drawn under the units.

use crate::content::terrain::grid::{Cell, Terrain};
use std::collections::HashMap;

use crate::data::desc::UnitKind;
use crate::data::fourcc::FourCc;
use crate::data::payload::{Palette, Rgb};
use crate::data::tile::Tile;
use crate::sim::world::draw::Overhead;
use crate::sim::world::reveal::Explored;

use crate::base::fixed::{self, Fixed, ONE, mul, whole};
use crate::content::terrain::Tileset;
use crate::data::catalogue::Catalogue;
use crate::sim::units::{PER_CELL, to_pos32};
use crate::view::ScreenRect;
use crate::view::raster::colour::{self, narrow, widen_screen};
use crate::view::raster::shade::TRANSPARENT;
use crate::view::sprite::{Colour, Sprite};

mod units;

pub use units::{Look, Looks, Scene, draw, put};

/// The `APMK` control whose rectangle the minimap fills: `0x7d1`, pushed
/// by view restore. `(4, 3)`, 176 x 176 in every HD
/// `UIData` file from 1024 x 768 up.
pub const CONTROL: u32 = 2001;

/// [`CONTROL`]'s rect, right and bottom exclusive: `(4, 3)`, 176 x 176.
pub const RECT: ScreenRect = [4, 3, 180, 179];

/// Fixed div: `(a << 16) / b`, truncated toward zero.
/// By 0 the engine faults; here the quotient saturates (a zero-width
/// rect's scale). Else [`crate::base::fixed::div`], which panics by 0.
#[allow(
    clippy::cast_possible_truncation,
    reason = "as the engine keeps 32 bits"
)]
const fn fixed_div(a: Fixed, b: Fixed) -> Fixed {
    let (a, b) = (a.to_bits(), b.to_bits());

    Fixed::from_bits(match b {
        0 if a < 0 => i32::MIN,
        0 => i32::MAX,
        _ => (((a as i64) << 16) / b as i64) as i32,
    })
}

/// A 16.16 value rounded up to a whole, as `OverheadGameView` does.
const fn ceil(fixed: Fixed) -> i32 {
    let up = if fixed.to_bits().trailing_zeros() >= 16 {
        0
    } else {
        1
    };

    whole(fixed) + up
}

/// Minimap pixels per cell, 16.16: view.
///
/// ```
/// use regent_core::view::panel::minimap::Scale;
///
/// let scale = Scale::fitted(176, 128);
///
/// assert_eq!(scale.block(), 2);
/// assert_eq!(scale.size(128, 128), (176, 176));
/// assert_eq!(Scale::fitted(176, 512).block(), 1);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale(Fixed);

impl Scale {
    /// `OverheadGameView`: the rect's width over the map's width in
    /// cells. Height plays no part: a map is drawn at the width's scale.
    #[must_use]
    pub fn fitted(width: i32, cells: i32) -> Scale {
        Scale(fixed_div(width * ONE, cells * ONE))
    }

    /// The 16.16 value.
    #[must_use]
    pub const fn fixed(self) -> Fixed {
        self.0
    }

    /// The side of the square each cell writes, the scale rounded up:
    /// view. 2 on a 128-cell map, 1 on 256 and 512.
    #[must_use]
    pub const fn block(self) -> i32 {
        ceil(self.0)
    }

    /// 32nds of a cell per pixel, 16.16: view,
    /// `fixed_div(32.0, scale)`.
    fn per_pixel(self) -> Fixed {
        fixed_div(PER_CELL * ONE, self.0)
    }

    /// The picture's size: `ceil(scale * cells)` each way, view.
    #[must_use]
    pub fn size(self, across: i32, down: i32) -> (i32, i32) {
        (
            ceil(mul(self.0, across * ONE)),
            ceil(mul(self.0, down * ONE)),
        )
    }

    /// The pixel a cell's square starts at: `floor(cell * scale)`.
    #[must_use]
    pub fn cell(self, cell: i32) -> i32 {
        whole(mul(self.0, cell * ONE))
    }

    /// A point in 32nds of a cell to a minimap pixel: method, by
    /// `Scale::per_pixel` truncated, so not quite `x * scale / 32`.
    #[must_use]
    pub fn pixel(self, at32: i32) -> i32 {
        whole(fixed_div(at32 * ONE, self.per_pixel()))
    }

    /// A minimap pixel to 32nds of a cell: method, what a click asks.
    #[must_use]
    pub fn thirty_seconds(self, px: i32) -> i32 {
        whole(mul(px * ONE, self.per_pixel()))
    }
}

/// Whose explored ground shows: the view's player mask, set by
/// method to `1 << local player`. Bit 31 shows all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Eyes(pub u32);

impl Eyes {
    /// Every vertex shows (bit 31 masked in method).
    pub const ALL: Eyes = Eyes(0x8000_0000);

    /// One player's.
    #[must_use]
    pub const fn of(player: u8) -> Eyes {
        Eyes(1 << player)
    }

    /// The players' bits as `World::seen_level` takes them: players 0 to
    /// 7.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "eight players")]
    pub const fn mask(self) -> u8 {
        self.0 as u8
    }

    /// Whether a vertex with explored bits `flags` shows.
    #[must_use]
    pub const fn see(self, flags: u8) -> bool {
        self.0 & Self::ALL.0 != 0 || self.0 & flags as u32 != 0
    }
}

/// A picture of 16-bit 565 words, row-major; 0 is black.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub words: Vec<u16>,
}

impl Picture {
    fn blank(width: i32, height: i32) -> Picture {
        let (width, height) = (to_index(width), to_index(height));

        Picture {
            width,
            height,
            words: vec![0; width * height],
        }
    }

    /// The word at `(x, y)`, `None` outside.
    #[must_use]
    pub fn at(&self, x: usize, y: usize) -> Option<u16> {
        (x < self.width && y < self.height).then(|| self.words[y * self.width + x])
    }

    fn put(&mut self, x: i32, y: i32, word: u16) {
        if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y))
            && x < self.width
            && y < self.height
        {
            self.words[y * self.width + x] = word;
        }
    }

    /// Fill rect: `rect` filled with `word`, clipped.
    pub fn fill(&mut self, [x0, y0, x1, y1]: ScreenRect, word: u16) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.put(x, y, word);
            }
        }
    }

    /// A rect's 1 px outline in `word` ([`outline`]).
    pub fn frame(&mut self, rect: ScreenRect, word: u16) {
        for side in outline(rect) {
            self.fill(side, word);
        }
    }

    /// A midpoint circle in `word` ([`circle`]).
    pub fn ring(&mut self, centre: (i32, i32), radius: i32, word: u16) {
        for (x, y) in circle(centre, radius) {
            self.put(x, y, word);
        }
    }

    /// Imag draw of a set 300 frame, as `GSOverheadGameView`
    /// asks it: its tile anchor at `at`, palette indices 0 to 15 from
    /// `ramp` where the unit has an owner, the rest the tile's own
    /// colours; a run gap or index 255 draws nothing.
    pub fn stamp(&mut self, sprite: &Sprite, at: (i32, i32), ramp: Option<&[Rgb; RAMP]>) {
        let width = usize::from(sprite.width).max(1);
        let origin = (
            at.0 - i32::from(sprite.anchor.0),
            at.1 - i32::from(sprite.anchor.1),
        );
        let ink = |i: usize| match &sprite.colour {
            Colour::Indexed { indices, palette } => {
                let index = indices[i];
                let ramped = ramp.and_then(|r| r.get(usize::from(index)));

                (index != TRANSPARENT)
                    .then(|| narrow(*ramped.unwrap_or(&palette.colours[usize::from(index)])))
            }
            Colour::Direct { words, packing } => Some(narrow(colour::split(words[i], *packing))),
        };

        for i in 0..sprite.covered.len() {
            let pixel = sprite.covered[i].then(|| ink(i)).flatten();
            let (x, y) = (i32::try_from(i % width), i32::try_from(i / width));

            if let (Some(w), Ok(x), Ok(y)) = (pixel, x, y) {
                self.put(origin.0 + x, origin.1 + y, w);
            }
        }
    }

    /// Straight RGBA8, each word as the screen shows it
    /// ([`widen_screen`]), as the captures hold it: what a front end
    /// uploads.
    #[must_use]
    pub fn rgba(&self) -> Vec<u8> {
        self.words
            .iter()
            .flat_map(|&w| {
                let Rgb { r, g, b } = widen_screen(w);

                [r, g, b, u8::MAX]
            })
            .collect()
    }
}

/// A non-negative pixel count as an index.
fn to_index(n: i32) -> usize {
    usize::try_from(n).unwrap_or(0)
}

/// A tile's minimap sample, `block` x `block` words: pixel `(x, y)` at
/// steps of `ceil(side / block)` from the top left (`OverheadGameView`,
/// which builds one such entry per tileset tile). A 32 px tile at block 2
/// gives pixels `(0, 0)`, `(16, 0)`, `(0, 16)`, `(16, 16)`; at block 1,
/// `(0, 0)`. `None` for a tile with no 16-bit words.
#[must_use]
pub fn sample(tile: &Tile<'_>, block: usize) -> Option<Vec<u16>> {
    let words = tile.words()?;
    let (width, height) = (usize::from(tile.width()), usize::from(tile.height()));
    let (across, down) = (width.div_ceil(block), height.div_ceil(block));

    let words = &words;
    let rows = (0..height).step_by(down.max(1));

    Some(
        rows.flat_map(|y| {
            (0..width)
                .step_by(across.max(1))
                .map(move |x| words[y * width + x].unwrap_or(0))
        })
        .collect(),
    )
}

/// The minimap's ground for a whole map: `OverheadGameView`, which
/// fills the view's cached picture.
///
/// Rows outer, cells inner, each from `floor(cell * scale)`; a cell
/// whose start pixel is the last one's is skipped, so on a big map the
/// first cell on a pixel wins. A cell writes its tile's [`sample`] where
/// `eyes` see its top-left vertex, else black; squares overlap where the
/// scale is under the block, the later cell writing over. A cell naming
/// no picture writes nothing.
#[must_use]
pub fn ground(
    terrain: &Terrain<'_>,
    tileset: &Tileset<'_>,
    catalogue: &Catalogue<'_>,
    scale: Scale,
    eyes: Eyes,
) -> Picture {
    let side = i32::try_from(terrain.side()).unwrap_or(0);
    let (width, height) = scale.size(side, side);
    let mut picture = Picture::blank(width, height);
    let seen = |x, y| terrain.vertex(x, y).is_some_and(|v| eyes.see(v.flags));
    let art = Art { tileset, catalogue };

    repaint(
        &mut picture,
        terrain,
        art,
        scale,
        &seen,
        [0, 0, width, height],
        &mut Samples::default(),
    );

    picture
}

/// What the ground is drawn with: the map's tileset and the archives its
/// tiles live in.
#[derive(Clone, Copy)]
pub struct Art<'r, 'a> {
    pub tileset: &'r Tileset<'a>,
    pub catalogue: &'r Catalogue<'a>,
}

/// Each tile's [`sample`], decoded once: `OverheadGameView`'s atlas.
#[derive(Clone, Debug, Default)]
pub struct Samples(HashMap<(usize, usize), Option<Vec<u16>>>);

/// [`ground`]'s pixels inside `rect` drawn again, as the whole pass
/// leaves them: every cell whose square meets `rect`, in the pass's
/// order, its writes kept inside. `seen` asks whether vertex `(x, y)` is
/// explored. The engine's cache queues such rects.
pub fn repaint(
    picture: &mut Picture,
    terrain: &Terrain<'_>,
    art: Art<'_, '_>,
    scale: Scale,
    seen: &dyn Fn(u32, u32) -> bool,
    rect: ScreenRect,
    samples: &mut Samples,
) {
    let block = scale.block();
    let unseen = vec![0u16; to_index(block * block)];
    let side = i32::try_from(terrain.side()).unwrap_or(0);

    // A cell's first pixel, `None` where the pass skips it: its pixel the
    // last one's.
    let start = |c: i32| {
        let px = scale.cell(c);

        (c == 0 || scale.cell(c - 1) != px).then_some(px)
    };

    // The cells whose squares meet pixels `low..high`, with their first.
    // Whole pixels over a 16.16 scale: `fixed_div` on raw words gives whole
    // cells.
    let cells_at = |px: i32| fixed_div(Fixed::from_bits(px), scale.0).to_bits();
    let meeting = |low: i32, high: i32| {
        let first = cells_at(low - block).max(0);
        let last = (cells_at(high) + 2).min(side);

        (first..last).filter_map(move |c| {
            let px = start(c)?;

            (px < high && px + block > low).then_some((c, px))
        })
    };
    let inside = |x: i32, y: i32| x >= rect[0] && x < rect[2] && y >= rect[1] && y < rect[3];

    for (cy, py) in meeting(rect[1], rect[3]) {
        for (cx, px) in meeting(rect[0], rect[2]) {
            let (x, y) = (cx.cast_unsigned(), cy.cast_unsigned());
            let words = if seen(x, y) {
                terrain
                    .cell(x, y)
                    .and_then(|cell| tile_sample(art, cell, block, samples))
            } else {
                Some(unseen.clone())
            };

            if let Some(words) = words {
                let pixels = (0..block).flat_map(|dy| (0..block).map(move |dx| (dx, dy)));

                for ((dx, dy), word) in pixels.zip(words) {
                    if inside(px + dx, py + dy) {
                        picture.put(px + dx, py + dy, word);
                    }
                }
            }
        }
    }
}

/// A cell's tile sample, decoded once per tile.
fn tile_sample(
    art: Art<'_, '_>,
    cell: Cell,
    block: i32,
    samples: &mut Samples,
) -> Option<Vec<u16>> {
    let (archive, index) = art.tileset.picture(cell)?.tile()?;

    samples
        .0
        .entry((archive, index))
        .or_insert_with(|| {
            let record = art.catalogue.positional(archive, b"TILE", index)?;
            let tile = Tile::parse(record.data).ok()?;

            sample(&tile, to_index(block))
        })
        .clone()
}

/// The minimap's ground kept between frames, as the engine's cache:
/// drawn whole once, then a cell
/// again where its top-left vertex's explored bits change for `eyes`.
/// **Inferred**: the engine queues a rect per change notice.
#[derive(Clone, Debug)]
pub struct Cache {
    picture: Picture,
    samples: Samples,
    scale: Scale,
    eyes: Eyes,
    /// The explored ground drawn: shared with the world's until it
    /// changes.
    explored: Explored,
    /// Vertices a row: the cells plus one.
    across: u32,
}

impl Cache {
    /// The whole ground of `terrain`, explored as `explored` says.
    #[must_use]
    pub fn new(
        terrain: &Terrain<'_>,
        art: Art<'_, '_>,
        scale: Scale,
        eyes: Eyes,
        explored: &Explored,
    ) -> Cache {
        let side = i32::try_from(terrain.side()).unwrap_or(0);
        let (width, height) = scale.size(side, side);
        let mut cache = Cache {
            picture: Picture::blank(width, height),
            samples: Samples::default(),
            scale,
            eyes,
            explored: explored.clone(),
            across: terrain.side() + 1,
        };
        let seen = cache.seen();

        repaint(
            &mut cache.picture,
            terrain,
            art,
            scale,
            &seen,
            [0, 0, width, height],
            &mut cache.samples,
        );

        cache
    }

    /// The ground as last drawn.
    #[must_use]
    pub const fn picture(&self) -> &Picture {
        &self.picture
    }

    /// The cells whose top-left vertex `explored` shows otherwise than
    /// drawn.
    #[must_use]
    pub fn changed(&self, explored: &Explored) -> Vec<(u32, u32)> {
        let across = self.across;
        let shows = |f: &u8| self.eyes.see(*f);
        let same = explored.cells() == self.explored.cells();

        // Runs still shared skipped unread: most frames explore nothing.
        explored
            .differing(&self.explored)
            .filter(|_| same)
            .flat_map(|(at, was, now)| (at..).zip(now.iter().zip(was.iter())))
            .filter(|(_, (now, was))| shows(now) != shows(was))
            .filter_map(|(i, _)| u32::try_from(i).ok())
            .map(|i| (i % across, i / across))
            .filter(|(x, y)| x + 1 < across && y + 1 < across)
            .collect()
    }

    /// Cells `cells` drawn again, explored as `explored` says.
    pub fn reveal(
        &mut self,
        terrain: &Terrain<'_>,
        art: Art<'_, '_>,
        explored: &Explored,
        cells: &[(u32, u32)],
    ) {
        if explored.cells() == self.explored.cells() {
            self.explored = explored.clone();
        }

        let seen = self.seen();
        let block = self.scale.block();

        for (x, y) in cells {
            let (px, py) = (
                self.scale.cell(x.cast_signed()),
                self.scale.cell(y.cast_signed()),
            );

            repaint(
                &mut self.picture,
                terrain,
                art,
                self.scale,
                &seen,
                [px, py, px + block, py + block],
                &mut self.samples,
            );
        }
    }

    /// Whether vertex `(x, y)` shows, by the flags drawn.
    fn seen(&self) -> impl Fn(u32, u32) -> bool + use<> {
        let (explored, eyes, across) = (self.explored.clone(), self.eyes, self.across);

        move |x, y| {
            let at = usize::try_from(y * across + x).unwrap_or(usize::MAX);

            explored.flag(at).is_some_and(|f| eyes.see(f))
        }
    }
}

/// Minimap filter: what the minimap draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter(pub u32);

impl Filter {
    /// Bit 3, mobiles (unit kind 1).
    pub const MOBILES: u32 = 0x08;
    /// Bit 4, buildings.
    pub const BUILDINGS: u32 = 0x10;
    /// Everything: prefs registry load ors in `0x7fffffff`, or
    /// `0x7ffffffb` when the saved option clears the ground.
    pub const DEFAULT: Filter = Filter(0x7fff_ffff);

    /// Whether every bit of `bits` is set.
    #[must_use]
    pub const fn has(self, bits: u32) -> bool {
        self.0 & bits == bits
    }
}

/// The unit lists the minimap draws, in order,
/// each list in its own order.
pub const LISTS: [List; 4] = [
    List::Buildings,
    List::Vehicles,
    List::RewardFlags,
    List::Messages,
];

/// A level unit list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    /// List 0.
    Buildings,
    /// List 1: characters, missiles, marks.
    Vehicles,
    /// List 4.
    RewardFlags,
    /// List `MSGF`.
    Messages,
}

/// What kind of unit a candidate is, as minimap shows asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Kind 0.
    Building {
        /// `BuildingDesc` flag `NotInMiniMap` ( bit 5).
        listed: Listed,
        /// `GenUnit` under 3.
        standing: Standing,
    },
    /// Kind 1 with a character part.
    Character {
        /// `VehicleDesc` flag `NotInMiniMap` ( bit 5).
        listed: Listed,
        /// `APV:37` invisible, `APV:45` camouflaged, or `APU:8` set: a
        /// speed trail's clone.
        cloaked: Cloak,
    },
    /// Kind 1 with no character part: a missile. Never drawn.
    Missile,
    /// Any other kind: reward and message flags.
    Other,
}

/// A descriptor's `NotInMiniMap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listed {
    Listed,
    Unlisted,
}

/// `GenUnit`: under 3 standing; ruined or lying dead is down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Standing,
    Down,
}

/// Whether a character hides from the minimap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cloak {
    Open,
    Cloaked,
}

/// Whether a holder holds the unit (unit state bit 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    Free,
    Held,
}

/// Minimap shows: whether the minimap draws a unit. `seen` is
/// unit seen level for the view's players; `overhead` unit
///  bit `0x400` (`ShowMinimap`, `NotVisibleInOverheadView`).
#[must_use]
pub fn shown(class: Class, seen: i32, overhead: Overhead, held: Held, filter: Filter) -> bool {
    let visible = seen > 0 && overhead == Overhead::Shown && held == Held::Free;

    visible
        && match class {
            Class::Building { listed, standing } => {
                filter.has(Filter::BUILDINGS)
                    && listed == Listed::Listed
                    && standing == Standing::Standing
            }
            Class::Character { listed, cloaked } => {
                filter.has(Filter::MOBILES) && listed == Listed::Listed && cloaked == Cloak::Open
            }
            Class::Missile => false,
            Class::Other => true,
        }
}

/// Whether a `CombatSignal` flash hides the unit this run: its order's
/// count is odd (its low bit, in `GSOverheadGameView`). See
/// `World::flash_count`.
#[must_use]
pub const fn blinked(count: u32) -> bool {
    count & 1 == 1
}

/// The code minimap square draws small whatever its kind:
/// `BBi2`, `Slave_Cross`.
pub const SMALL: FourCc = FourCc(*b"BBi2");

/// The square a unit with no set 300 fills about its point `(x, y)`:
/// a building 5 x 5, a character or [`SMALL`]
/// 3 x 3. `None` for other kinds, whose rect the engine leaves unset.
/// The set drawn instead of a square is 300 (`SetKind::Minimap`), slot
/// 0, frame unit, anchored at the unit's point.
///
/// ```
/// use regent_core::data::desc::UnitKind;
/// use regent_core::data::fourcc::FourCc;
/// use regent_core::view::panel::minimap::square;
///
/// let tower = square((10, 20), FourCc(*b"ABM1"), UnitKind::Building);
/// let caravan = square((10, 20), FourCc(*b"AVa1"), UnitKind::Character);
///
/// assert_eq!(tower, Some([8, 18, 13, 23]));
/// assert_eq!(caravan, Some([9, 19, 12, 22]));
/// ```
#[must_use]
pub fn square((x, y): (i32, i32), code: FourCc, kind: UnitKind) -> Option<ScreenRect> {
    let small = Some([x - 1, y - 1, x + 2, y + 2]);

    match kind {
        _ if code == SMALL => small,
        UnitKind::Building => Some([x - 2, y - 2, x + 3, y + 3]),
        UnitKind::Character => small,
        _ => None,
    }
}

/// Why a unit of the local player draws white.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lit {
    /// It is selected.
    Selected,
    /// A member of its relation 101 (`HOME`) or 100 is selected; not
    /// counted for a palace (`ABJ`).
    Kin,
    /// A caravan (`AVa1`) while a local Marketplace or Trading Post is
    /// selected.
    Trade,
    Not,
}

/// The colour index of a lit unit: white.
pub const WHITE: u8 = 8;

/// The colour index of a fallen unit: grey `(90, 90, 90)`.
pub const FALLEN: u8 = 9;

/// The palace's code prefix, which [`Lit::Kin`] skips.
const PALACE: [u8; 3] = *b"ABJ";

/// Minimap colour: an index into [`colours`]. `owner`'s colour,
/// white for the local player's lit units, grey for a fallen one. `None`
/// for no owner (`-1`): a picture keeps its own palette.
///
/// ```
/// use regent_core::data::fourcc::FourCc;
/// use regent_core::view::panel::minimap::{Lit, Standing, colour};
///
/// let tower = FourCc(*b"ABM1");
///
/// assert_eq!(colour(Some(0), 0, tower, Standing::Standing, Lit::Selected), Some(8));
/// assert_eq!(colour(Some(1), 0, tower, Standing::Down, Lit::Selected), Some(9));
/// assert_eq!(colour(Some(7), 0, tower, Standing::Standing, Lit::Not), Some(7));
/// ```
#[must_use]
pub fn colour(
    owner: Option<u8>,
    local: u8,
    code: FourCc,
    standing: Standing,
    lit: Lit,
) -> Option<u8> {
    let own = owner?;
    let plain = match standing {
        Standing::Standing => own,
        Standing::Down => FALLEN,
    };
    let white = match lit {
        Lit::Selected | Lit::Trade => true,
        Lit::Kin => code.0[..3] != PALACE,
        Lit::Not => false,
    };

    Some(if own == local && white { WHITE } else { plain })
}

/// The palette `GEPlayerColors` takes player colours from: `PALT
/// PLYR` in `interfacedata.cam`.
pub const PALETTE: FourCc = FourCc(*b"PLYR");

/// `GEPlayerColors`'s ten colours: players 0 to 7 from [`PALETTE`]
/// entries 1 to 8, then white ([`WHITE`]) and grey ([`FALLEN`]). No
/// `PLY1`..`PLY8` ships to override them.
#[must_use]
pub fn colours(plyr: &Palette) -> [Rgb; 10] {
    let mut out = [BLACK; 10];

    out[..8].copy_from_slice(&plyr.colours[1..9]);
    out[usize::from(WHITE)] = FULL;
    out[usize::from(FALLEN)] = Rgb {
        r: 0x5a,
        g: 0x5a,
        b: 0x5a,
    };

    out
}

const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

const FULL: Rgb = Rgb {
    r: 0xff,
    g: 0xff,
    b: 0xff,
};

/// Palette indices a colour ramps over: `GEPlayerColors` to,
/// 0 to 15 from prefs registry load. Its middle, the
/// colour itself, is 7:, `(15 - 0) / 2`.
pub const RAMP: usize = 16;

/// The share of the colour index 0 takes: `PlayerColorPaletteRange`,
/// unset, is `0..=0`, ramped from `lerp(black, colour, 100, 30)`.
const DIM: i32 = 30;

/// Colour lerp: step `i` of `n` from `a` to `b`, by whole percent.
fn lerp(from: Rgb, to: Rgb, steps: i32, step: i32) -> Rgb {
    let percent = step * 100 / steps;
    let channel = |low: u8, high: u8| {
        let value = i32::from(low) + (i32::from(high) - i32::from(low)) * percent / 100;

        u8::try_from(value.clamp(0, 255)).unwrap_or(0)
    };

    Rgb {
        r: channel(from.r, to.r),
        g: channel(from.g, to.g),
        b: channel(from.b, to.b),
    }
}

/// `GEPlayerColors`: a colour's ramp. A set 300 picture's pixels
/// `0` to `15` take these (`GSOverheadGameView` hands the owner's
/// palette and range to imag draw); a square fills with the
/// middle, the colour. Black up to the colour at index 7, then on to
/// white; index 0 then rewritten as 30% of the colour.
///
/// **Game**: the palace cross's indices 4, 5, 7 read `(0, 0, 104)`,
/// `(0, 0, 136)`, `(0, 0, 216)` in `heighty_lowerleft_native.png`:
/// player 0's blue `(0, 0, 223)` so, in 565.
#[must_use]
pub fn ramp(colour: Rgb) -> [Rgb; RAMP] {
    let (mid, end) = (7, 15);
    let mut out = [BLACK; RAMP];

    for (i, slot) in (0..).zip(out.iter_mut()) {
        *slot = if i < mid {
            lerp(BLACK, colour, mid + 1, i)
        } else {
            lerp(colour, FULL, end - mid + 1, i - mid)
        };
    }

    out[0] = lerp(BLACK, colour, 100, DIM);

    out
}

/// The main view's frame, `0x80ffffff`.
pub const MAIN_FRAME: Rgb = FULL;

/// Mini view 1's frame, `0x80cccccc`.
pub const MINI_FRAME: Rgb = Rgb {
    r: 0xcc,
    g: 0xcc,
    b: 0xcc,
};

/// A view's rect in 32nds of a cell, on flat ground (`GameView`,
/// `IsoGameView`): `scroll` its top left and `size` its rect in view
/// pixels, `down` a field, `32 zoom sin(IsoCameraAngle)`. Each edge
/// truncated.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "view pixels, as the engine's float conversions"
)]
pub fn view_rect(scroll: (i32, i32), size: (i32, i32), zoom: f32, down: f32) -> ScreenRect {
    let across = |x: i32| (x as f32 / zoom) as i32;
    let deep = |y: i32| ((y * PER_CELL) as f32 / down) as i32;

    [
        across(scroll.0),
        deep(scroll.1),
        across(scroll.0 + size.0),
        deep(scroll.1 + size.1),
    ]
}

/// Where `GSOverheadGameView` reads the ground under a framed view:
/// `((right - left) / 2, bottom)` in 32nds. Half the width, not the
/// middle, as the code has it.
#[must_use]
pub const fn lift_point(rect: ScreenRect) -> (i32, i32) {
    ((rect[2] - rect[0]) / 2, rect[3])
}

/// `GSOverheadGameView`: a frame lowered by the ground's lift:
/// `height` (16.16, at [`lift_point`]) truncated to a 32nd, times
/// `camera_cos`, whole; cut so the bottom stays within
/// `bottom_limit`, the map's height in 32nds.
#[must_use]
pub fn lift(rect: ScreenRect, height: Fixed, camera_cos: Fixed, bottom_limit: i32) -> ScreenRect {
    // `(height & !0x7ff) << 5`: whole 32nds, as 16.16.
    let thirty_seconds = fixed::from_whole(to_pos32(height));
    let cells = fixed_div(thirty_seconds, PER_CELL * ONE);
    let mut lift = whole(mul(cells, camera_cos));

    if rect[3] + lift > bottom_limit {
        lift = bottom_limit - rect[3];
    }

    [rect[0], rect[1] + lift, rect[2], rect[3] + lift]
}

/// `OverheadGameView`: a rect in 32nds to minimap pixels, width and
/// height converted apart from the corner.
#[must_use]
pub fn viewport(rect: ScreenRect, scale: Scale) -> ScreenRect {
    let (left, top) = (scale.pixel(rect[0]), scale.pixel(rect[1]));

    [
        left,
        top,
        left + scale.pixel(rect[2] - rect[0]),
        top + scale.pixel(rect[3] - rect[1]),
    ]
}

/// Outline rect: a frame's four 1 px fills, top, bottom, right,
/// left.
#[must_use]
pub const fn outline([x0, y0, x1, y1]: ScreenRect) -> [ScreenRect; 4] {
    [
        [x0, y0, x1, y0 + 1],
        [x0, y1 - 1, x1, y1],
        [x1 - 1, y0, x1, y1],
        [x0, y0, x0 + 1, y1],
    ]
}

/// A mouse event on the minimap (minimap click, app state 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// A left press, or the mouse moving with it held.
    Left,
    /// A right press, or the mouse moving with it held.
    Right,
    /// A left double click.
    LeftTwice,
}

/// What a [`Press`] asks, at a point in 32nds of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Centre the main view there.
    CentreMain((i32, i32)),
    /// Centre mini view 1 there; it stops following its unit.
    CentreMini((i32, i32)),
    /// An explore flag there at the panel's reward, kept on the map
    /// (`World::explore_point`).
    Explore((i32, i32)),
}

/// Minimap click: a press at `(x, y)`, pixels from the minimap's
/// top left.
///
/// ```
/// use regent_core::view::panel::minimap::{Order, Press, Scale, click};
///
/// let scale = Scale::fitted(176, 128);
///
/// assert_eq!(click(Press::Left, (88, 88), scale), Order::CentreMain((2047, 2047)));
/// ```
#[must_use]
pub fn click(press: Press, (x, y): (i32, i32), scale: Scale) -> Order {
    let at = (scale.thirty_seconds(x), scale.thirty_seconds(y));

    match press {
        Press::Left => Order::CentreMain(at),
        Press::Right => Order::CentreMini(at),
        Press::LeftTwice => Order::Explore(at),
    }
}

/// The reach circles' colour, `0x80ffffff`.
pub const REACH: Rgb = FULL;

/// Circle outline: the pixels of a radius `r` circle about
/// `(cx, cy)`, midpoint, eight octants a step.
///
/// ```
/// use regent_core::view::panel::minimap::circle;
///
/// let ring = circle((0, 0), 2);
///
/// assert!(ring.contains(&(0, 2)) && ring.contains(&(2, 1)) && !ring.contains(&(2, 2)));
/// ```
#[must_use]
pub fn circle((cx, cy): (i32, i32), radius: i32) -> Vec<(i32, i32)> {
    let octants = |x: i32, y: i32| {
        [
            (cx + y, cy + x),
            (cx + y, cy - x),
            (cx - y, cy + x),
            (cx - y, cy - x),
            (cx + x, cy + y),
            (cx + x, cy - y),
            (cx - x, cy + y),
            (cx - x, cy - y),
        ]
    };

    let mut out = Vec::new();
    let (mut x, mut y, mut d) = (0, radius, 3 - 2 * radius);

    while x < y {
        out.extend(octants(x, y));

        if d < 0 {
            d += 6 + 4 * x;
        } else {
            d += 10 + 4 * (x - y);
            y -= 1;
        }

        x += 1;
    }

    if x == y {
        out.extend(octants(x, y));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zero-width rect gives scale 0; a point still maps, not a
    /// divide fault.
    #[test]
    fn a_zero_scale_divides_without_a_fault() {
        let scale = Scale::fitted(0, 128);

        assert_eq!(scale.fixed(), Fixed::ZERO);
        assert_eq!(scale.pixel(64), 0);
    }
}
