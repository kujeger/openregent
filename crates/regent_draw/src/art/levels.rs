//! The quest map's picture drawn from a level made for the window's scale,
//! cut in tiles under egui's texture limit, and a hovered quest lit on
//! the same level's pixels, so the two always meet.
//!
//! ```text
//!  window px a map px   ≤ 0.5   ≤ 1      ≤ 2        above
//!  level                halved  the map  xBRZ 2×    xBRZ 4×
//!  ui_upscale none      halved  the map  the map    the map
//!
//!  level ──cut──▶ tiles, 1 px of gutter each side ──▶ textures
//!  lit   = level + w · (T(level) − level) over the quest's lit rectangle,
//!          w from its ellipse at the level's pixels ([`glow`])
//! ```
//!
//! A derived source of the art service: the table T is fitted to the lit
//! pictures once and cached on disk beside its pictures, keyed by the
//! pictures' hash. Each level, its tiles and lit patches, is one file of
//! the disk cache (`disk`), keyed by the map, the quests and the table, its
//! parts read in parallel; made, in Bevy's task pool, only where missing.
//!
//! ```text
//!  menus start ──task──▶ table, the map's own level ──▶ 2× ahead (`Level::AHEAD`)
//!  a level made ──▶ its tiles uploaded, 32 MB a frame ──▶ shown
//!  the map drawn, wanted level not shown, none but the map's own standing
//!  in: read at once if on disk, its task waited on if reading; all uploaded
//!  off the map's pages: 4× dropped, the rest kept
//! ```

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::glow::{self, Grid, Picture, Table};
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool, block_on, futures::check_ready};
use bevy_egui::egui::{
    self, Color32, ColorImage, Painter, Pos2, Rect, TextureHandle, TextureOptions, pos2,
};

use super::disk::{self, Original};
use super::scale::{self, Edge};
use super::{Count, Upscale, shrink};
use regent_core::data::fnv;

/// The table's method version, in its cache path: bump on a change to
/// the fit.
const TABLE_VERSION: u32 = 1;

/// The levels' method version, in their files' key: bump on a change to
/// the cut, the gutter or the look.
const LEVEL_VERSION: u32 = 1;

/// A tile's side, level pixels, without its gutters: within egui's 2048.
const TILE: usize = 1024;

/// Level pixels round a tile or lit patch that it holds and does not
/// show: linear sampling at its edge reads its neighbour's pixels, so
/// there is no seam.
const GUTTER: usize = 1;

/// Texture bytes uploaded a frame to a level not wanted at once: about
/// 5 ms (release, 0.15 ms a MB measured).
const UPLOAD: usize = 32 << 20;

/// Picture sizes the map is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    Half,
    One,
    Two,
    Four,
}

impl Level {
    const ALL: [Level; 4] = [Level::Half, Level::One, Level::Two, Level::Four];

    /// Made ahead as the menus start: the level the map opens at on a
    /// 1920 × 1200 window.
    pub const AHEAD: Level = Level::Two;

    /// Level pixels a map pixel.
    const fn scale(self) -> f32 {
        match self {
            Level::Half => 0.5,
            Level::One => 1.0,
            Level::Two => 2.0,
            Level::Four => 4.0,
        }
    }

    /// The xBRZ factor, or 1; `None` for the halved map.
    const fn times(self) -> Option<usize> {
        match self {
            Level::Half => None,
            Level::One => Some(1),
            Level::Two => Some(2),
            Level::Four => Some(4),
        }
    }

    /// Whether it is kept off the map's pages: all but 4×, 144 MB of
    /// textures.
    const fn kept(self) -> bool {
        !matches!(self, Level::Four)
    }

    /// The least level at or over `scale` window pixels a map pixel, the
    /// largest above: never minified past half.
    ///
    /// ```text
    ///  scale  0.4   0.6  1    1.2  2.01  3
    ///  level  Half  One  One  Two  Two   Four
    /// ```
    #[must_use]
    pub fn of(scale: f32) -> Level {
        Level::ALL
            .into_iter()
            .find(|l| l.scale() + scale::SLACK >= scale)
            .unwrap_or(Level::Four)
    }

    /// The level `mode` makes in its place: none upscaled with upscaling
    /// off, as no other picture is.
    #[must_use]
    pub const fn within(self, mode: Upscale) -> Level {
        match (self, mode) {
            (Level::Two | Level::Four, Upscale::None) => Level::One,
            _ => self,
        }
    }

    /// The level's picture size for a `map`-sized map.
    #[must_use]
    pub fn size(self, map: [usize; 2]) -> [usize; 2] {
        match self.times() {
            Some(f) => map.map(|n| n * f),
            None => map.map(|n| n.div_ceil(2)),
        }
    }
}

/// A rectangle of pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub at: [usize; 2],
    pub size: [usize; 2],
}

/// A part of a level as a texture: what it shows, and what it holds,
/// that and its gutter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub shows: Area,
    pub holds: Area,
}

impl Piece {
    /// `shows` with [`GUTTER`] round it, kept inside a `size` picture.
    fn of(shows: Area, size: [usize; 2]) -> Piece {
        let at = shows.at.map(|n| n.saturating_sub(GUTTER));
        let end = [0, 1].map(|i| (shows.at[i] + shows.size[i] + GUTTER).min(size[i]));

        Piece {
            shows,
            holds: Area {
                at,
                size: [end[0] - at[0], end[1] - at[1]],
            },
        }
    }

    /// Where it shows within its texture, 0 to 1.
    #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
    fn uv(self) -> Rect {
        let at =
            |i: usize| (self.shows.at[i] - self.holds.at[i]) as f32 / self.holds.size[i] as f32;
        let end = |i: usize| at(i) + self.shows.size[i] as f32 / self.holds.size[i] as f32;

        Rect::from_min_max(pos2(at(0), at(1)), pos2(end(0), end(1)))
    }
}

/// A `size` level cut in tiles of at most `most` a side, rows first.
#[must_use]
pub fn tiles(size: [usize; 2], most: usize) -> Vec<Piece> {
    let starts = |n: usize| {
        (0..n)
            .step_by(most.max(1))
            .map(move |a| (a, most.min(n - a)))
    };

    starts(size[1])
        .flat_map(|(y, h)| {
            starts(size[0]).map(move |(x, w)| {
                Piece::of(
                    Area {
                        at: [x, y],
                        size: [w, h],
                    },
                    size,
                )
            })
        })
        .collect()
}

/// The level pixels covering a lit picture `lit` at `at`, map pixels, on a
/// `level`-sized level of a `map`-sized map.
#[must_use]
pub fn patch(at: [usize; 2], lit: [usize; 2], map: [usize; 2], level: [usize; 2]) -> Piece {
    let low = |i: usize| (at[i] * level[i] / map[i]).min(level[i]);
    let high = |i: usize| ((at[i] + lit[i]) * level[i]).div_ceil(map[i]).min(level[i]);
    let shows = Area {
        at: [low(0), low(1)],
        size: [high(0) - low(0), high(1) - low(1)],
    };

    Piece::of(shows, level)
}

/// `area` of a `level`-sized level in map pixels, a `map`-sized map.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
pub fn on_map(area: Area, level: [usize; 2], map: [usize; 2]) -> Rect {
    let at = |n: usize, i: usize| n as f32 * map[i] as f32 / level[i] as f32;

    Rect::from_min_max(
        pos2(at(area.at[0], 0), at(area.at[1], 1)),
        pos2(
            at(area.at[0] + area.size[0], 0),
            at(area.at[1] + area.size[1], 1),
        ),
    )
}

/// A painted quest: its slot and lit rectangle, map pixels.
#[derive(Clone, Copy, Debug)]
pub struct Quest {
    pub slot: usize,
    pub at: [usize; 2],
    pub size: [usize; 2],
}

/// A lit picture and its corner, map pixels: the table's fit only.
pub struct Lit {
    pub at: [usize; 2],
    pub image: ColorImage,
}

/// `picture` drawn over `map` at `at`, map pixels: a downloadable quest's
/// building, which the map's art lacks. Cut at the map's edge.
pub fn paste(map: &mut ColorImage, at: [usize; 2], picture: &ColorImage) {
    let [w, h] = picture.size;
    let inside = |x: usize, y: usize| at[0] + x < map.size[0] && at[1] + y < map.size[1];

    for (x, y) in (0..h).flat_map(|y| (0..w).map(move |x| (x, y))) {
        if inside(x, y) {
            let i = (at[1] + y) * map.size[0] + at[0] + x;

            map.pixels[i] = map.pixels[i].blend(picture.pixels[y * w + x]);
        }
    }
}

/// A `size` texture's bytes, RGBA.
const fn area(size: [usize; 2]) -> usize {
    size[0] * size[1] * 4
}

/// `image`'s pixels as RGBA bytes.
fn rgba(image: &ColorImage) -> Vec<u8> {
    image
        .pixels
        .iter()
        .flat_map(Color32::to_srgba_unmultiplied)
        .collect()
}

/// The table's key for `map` and `lits`.
fn table_key(map: &Original, lits: &[Lit]) -> u64 {
    lits.iter().fold(map.hash, |h, lit| {
        let at = lit.at.iter().flat_map(|n| n.to_le_bytes());
        let image = disk::hash(&lit.image).to_le_bytes();

        fnv::fold(h, at.chain(image))
    })
}

/// The table's file for `key` in the cache at `dir`.
///
/// ```text
///  <dir>/lit-v1/<key>.lut
/// ```
fn table_file(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("lit-v{TABLE_VERSION}"))
        .join(format!("{key:016x}.lut"))
}

/// The look's table for `map`, from the cache at `dir`, or fitted to
/// `lits` and put there.
fn table(map: &Original, lits: &[Lit], dir: Option<&Path>) -> Table {
    let path = dir.map(|d| table_file(d, table_key(map, lits)));
    let cached = path
        .as_deref()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| Table::from_bytes(&b));

    cached.unwrap_or_else(|| {
        let pixels: Vec<Vec<u8>> = lits.iter().map(|l| rgba(&l.image)).collect();
        let pictures: Vec<([usize; 2], Picture)> = lits
            .iter()
            .zip(&pixels)
            .map(|(l, rgba)| {
                let picture = Picture {
                    size: l.image.size,
                    rgba,
                };

                (l.at, picture)
            })
            .collect();
        let map_rgba = rgba(&map.image);
        let whole = Picture {
            size: map.image.size,
            rgba: &map_rgba,
        };
        let table = Table::fit(whole, &pictures);

        if let Some(Err(e)) = path.as_deref().map(|p| disk::store(p, &table.bytes())) {
            tracing::warn!("lit table: {e}");
        }

        table
    })
}

/// What `piece` of `level`, `scale` level pixels a map pixel, holds, with
/// `quest` lit through `table` where it shows: each pixel at its weight
/// from the quest's ellipse, measured on the level's own pixels. The
/// gutter is the level's, as the map round the patch is.
fn glowed(
    level: &ColorImage,
    piece: Piece,
    (quest, table): (&Quest, &Table),
    scale: f32,
) -> ColorImage {
    let (holds, shows) = (piece.holds, piece.shows);
    let inside =
        |p: [usize; 2]| (0..2).all(|i| (shows.at[i]..shows.at[i] + shows.size[i]).contains(&p[i]));
    let grid = Grid {
        at: holds.at,
        size: holds.size,
        scale: f64::from(scale),
    };
    #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
    let rect = [quest.at[0], quest.at[1], quest.size[0], quest.size[1]].map(|n| n as f64);
    let weights = glow::weights(&glow::ellipse(rect, grid), grid);
    let width = level.size[0];
    let pixels = (0..holds.size[1])
        .flat_map(|y| (0..holds.size[0]).map(move |x| [x, y]))
        .map(|[x, y]| {
            let at = [holds.at[0] + x, holds.at[1] + y];
            let base = level.pixels[at[1] * width + at[0]];

            if inside(at) {
                let [r, g, b, a] = base.to_array();
                let [r, g, b] = table.glow([r, g, b], weights[y * holds.size[0] + x]);

                Color32::from_rgba_premultiplied(r, g, b, a)
            } else {
                base
            }
        })
        .collect();

    ColorImage::new(holds.size, pixels)
}

/// One level's pictures, made.
struct Made {
    size: [usize; 2],
    tiles: Vec<(Piece, ColorImage)>,
    /// By the quests' order.
    lits: Vec<(Piece, ColorImage)>,
}

impl Made {
    /// The pieces level `level` of `base` is cut in: its tiles, then each
    /// quest's lit patch.
    fn pieces(base: &Base, level: Level) -> [Vec<Piece>; 2] {
        let size = level.size(base.map);
        let lits = base
            .quests
            .iter()
            .map(|q| patch(q.at, q.size, base.map, size))
            .collect();

        [tiles(size, TILE), lits]
    }

    /// Level `level` of `base` from a file's `parts`, if they are its
    /// pieces.
    fn of(base: &Base, level: Level, parts: Vec<disk::Part>) -> Option<Made> {
        let [tiles, lits] = Made::pieces(base, level);
        let fits = parts.len() == tiles.len() + lits.len()
            && tiles
                .iter()
                .chain(&lits)
                .zip(&parts)
                .all(|(p, part)| p.holds.at == part.at && p.holds.size == part.image.size);
        let mut images = parts.into_iter().map(|p| p.image);
        let tiles = tiles.into_iter().zip(images.by_ref()).collect();
        let lits = lits.into_iter().zip(images).collect();

        fits.then(|| Made {
            size: level.size(base.map),
            tiles,
            lits,
        })
    }

    /// Its tiles and lit patches as a file's parts.
    fn parts(&self) -> Vec<disk::Part> {
        self.tiles
            .iter()
            .chain(&self.lits)
            .map(|(p, image)| disk::Part {
                at: p.holds.at,
                image: image.clone(),
            })
            .collect()
    }
}

/// What every level is made from, shared with the tasks.
#[derive(Clone)]
struct Base {
    /// The map's pixels.
    image: Original,
    /// The map's size, pixels.
    map: [usize; 2],
    quests: Arc<[Quest]>,
    table: Arc<Table>,
    /// The levels' files' key: the map, its size, the quests,
    /// the table and [`LEVEL_VERSION`].
    key: u64,
}

impl Base {
    fn new(image: Original, quests: Vec<Quest>, table: (Table, u64)) -> Base {
        let map = image.image.size;
        let quest = |q: &Quest| [q.slot, q.at[0], q.at[1], q.size[0], q.size[1]];
        let words = map
            .into_iter()
            .chain(quests.iter().flat_map(quest))
            .flat_map(usize::to_le_bytes);
        let head = LEVEL_VERSION
            .to_le_bytes()
            .into_iter()
            .chain(table.1.to_le_bytes());
        let key = fnv::fold(image.hash, head.chain(words));

        Base {
            image,
            map,
            quests: quests.into(),
            table: Arc::new(table.0),
            key,
        }
    }

    /// Level `level`'s file in the cache at `dir`.
    ///
    /// ```text
    ///  <dir>/xbrz-v1/<key>-<w>x<h>-level.lz4
    /// ```
    fn file(&self, dir: &Path, level: Level) -> PathBuf {
        let [w, h] = level.size(self.map);

        disk::folder(dir).join(format!("{:016x}-{w}x{h}-level.lz4", self.key))
    }

    /// What level `level`'s file says of it.
    fn header(&self, level: Level) -> disk::Header {
        disk::Header {
            size: level.size(self.map),
            edge: Edge::Clamp,
            factor: level.times().unwrap_or(0),
        }
    }

    /// Whether level `level` is in the cache at `dir`.
    fn cached(&self, dir: Option<&Path>, level: Level) -> Found {
        if dir.is_some_and(|d| self.file(d, level).is_file()) {
            Found::Cached
        } else {
            Found::Missing
        }
    }
}

/// Level `level` of `base`, from the cache at `dir`, or made and put
/// there.
fn made(base: &Base, level: Level, dir: Option<&Path>) -> Made {
    let path = dir.map(|d| base.file(d, level));
    let header = base.header(level);
    let read = path
        .as_deref()
        .and_then(|p| disk::read(p, header))
        .and_then(|parts| Made::of(base, level, parts));

    read.unwrap_or_else(|| {
        let made = make(base, level);

        if let Some(p) = &path {
            disk::write(p, header, &made.parts());
        }

        made
    })
}

/// Level `level` of `base`'s map, and each quest's lit patch through its
/// table; in parallel.
fn make(base: &Base, level: Level) -> Made {
    let size = level.size(base.map);
    let map = &base.image.image;
    let image: Cow<ColorImage> = match level.times() {
        Some(1) => Cow::Borrowed(map),
        Some(f) => Cow::Owned(scale::xbrz(map, f, Edge::Clamp)),
        None => Cow::Owned(shrink::shrink(map, size)),
    };
    let [tiles, lits] = Made::pieces(base, level);
    let tiles = disk::parallel(&tiles, |p| {
        (*p, scale::cut(&image, p.holds.at, p.holds.size))
    });
    let jobs: Vec<_> = lits.into_iter().zip(base.quests.iter()).collect();
    let lits = disk::parallel(&jobs, |(p, quest)| {
        let lit = glowed(&image, *p, (quest, &base.table), level.scale());

        (*p, lit)
    });

    Made { size, tiles, lits }
}

/// A lit patch, uploaded when first hovered.
struct Patch {
    piece: Piece,
    image: Option<ColorImage>,
    texture: Option<TextureHandle>,
}

/// One level as textures.
struct Shown {
    size: [usize; 2],
    tiles: Vec<(Piece, TextureHandle)>,
    lits: Vec<Patch>,
}

/// Whether a level was in the disk cache when asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Found {
    Cached,
    Missing,
}

enum Stage {
    /// In a task: one reading from the cache is waited on where nothing
    /// stands in. Counted as pending work while the map's page wants it.
    Making(Task<Made>, Option<Count>, Found),
    /// Tiles going up a few a frame: those up, and the rest. Counted as
    /// `Making` is.
    Uploading(Made, Vec<(Piece, TextureHandle)>, Option<Count>),
    Shown(Shown),
}

/// The quest map's levels and lit patches.
pub struct Levels {
    base: Base,
    made: HashMap<Level, Stage>,
    /// The settings' `ui_upscale` ([`Levels::set`]).
    upscale: Upscale,
    /// The disk cache's directory; `None`: none.
    dir: Option<PathBuf>,
}

impl Levels {
    /// `map` and its painted `quests`, lit by a table from the cache or
    /// fitted to their `lits`; the map's own level made now, so one is
    /// always there to draw; cached at `dir`. Made in a task
    /// (`Art::levels`).
    #[must_use]
    pub fn new(map: ColorImage, quests: Vec<Quest>, lits: &[Lit], dir: Option<PathBuf>) -> Levels {
        let map = Original::new(map);
        let table = (table(&map, lits, dir.as_deref()), table_key(&map, lits));
        let base = Base::new(map, quests, table);
        let one = made(&base, Level::One, dir.as_deref());

        Levels {
            base,
            made: HashMap::from([(
                Level::One,
                Stage::Uploading(one, Vec::new(), Some(Count::new())),
            )]),
            upscale: Upscale::default(),
            dir,
        }
    }

    /// Upscaling set to `mode`: the levels it does not make dropped.
    pub fn set(&mut self, mode: Upscale) {
        self.upscale = mode;
        self.made.retain(|l, _| l.within(mode) == *l);
    }

    /// The textures held, and their bytes.
    pub fn held(&self) -> [usize; 2] {
        let handles = self.made.values().flat_map(|s| match s {
            Stage::Shown(shown) => shown
                .tiles
                .iter()
                .map(|(_, t)| t)
                .chain(shown.lits.iter().filter_map(|p| p.texture.as_ref()))
                .collect(),
            Stage::Uploading(_, up, _) => up.iter().map(|(_, t)| t).collect(),
            Stage::Making(..) => Vec::new(),
        });

        handles.fold([0, 0], |[n, bytes], t| [n + 1, bytes + area(t.size())])
    }

    /// The levels counted as pending work (`art::busy`).
    #[cfg(test)]
    fn counted(&self) -> usize {
        self.made
            .values()
            .filter(|s| {
                matches!(
                    s,
                    Stage::Making(_, Some(_), _) | Stage::Uploading(_, _, Some(_))
                )
            })
            .count()
    }

    /// The levels not kept dropped (`Level::kept`): the page left the map.
    /// Those kept count as pending no more.
    pub fn rest(&mut self) {
        self.made.retain(|l, _| l.kept());
        self.uncount();
    }

    /// No level counted as pending work: off the map's pages a level
    /// part made goes on only while a menu page draws, and no picture
    /// waits for it.
    fn uncount(&mut self) {
        for stage in self.made.values_mut() {
            if let Stage::Making(_, count, _) | Stage::Uploading(_, _, count) = stage {
                *count = None;
            }
        }
    }

    /// `level` counted as pending work until it shows: the map's page
    /// wants it.
    fn count(&mut self, level: Level) {
        if let Some(Stage::Making(_, count, _) | Stage::Uploading(_, _, count)) =
            self.made.get_mut(&level)
        {
            count.get_or_insert_with(Count::new);
        }
    }

    /// [`Level::AHEAD`] made, and uploaded a few tiles a frame, while the
    /// map is not drawn: uncounted.
    pub fn ahead(&mut self, ctx: &egui::Context) {
        self.uncount();
        self.ask(Level::AHEAD.within(self.upscale));
        self.poll(ctx, None);
    }

    /// The map at `scale` window points a map pixel, its corner at
    /// `origin`, from the level for its scale, or the nearest made while
    /// that is; slot `lit`'s quest lit.
    pub fn draw(
        &mut self,
        (ctx, painter): (&egui::Context, &Painter),
        (scale, origin): (f32, Pos2),
        lit: Option<usize>,
    ) {
        let wanted = Level::of(scale * ctx.pixels_per_point()).within(self.upscale);
        let now = self.want(wanted);

        self.poll(ctx, now);

        let map = self.base.map;
        let lit = lit.and_then(|s| self.base.quests.iter().position(|q| q.slot == s));
        let level = self.nearest(wanted);

        if let Some(Stage::Shown(shown)) = self.made.get_mut(&level) {
            let place = |p: Piece| super::onto(on_map(p.shows, shown.size, map), scale, origin);

            for (piece, texture) in &shown.tiles {
                painter.image(texture.id(), place(*piece), piece.uv(), Color32::WHITE);
            }

            if let Some(patch) = lit.and_then(|i| shown.lits.get_mut(i)) {
                if let Some(image) = patch.image.take() {
                    let name = format!("quest map {level:?} lit {:?}", patch.piece);

                    patch.texture = Some(ctx.load_texture(name, image, TextureOptions::LINEAR));
                }

                if let Some(texture) = &patch.texture {
                    painter.image(
                        texture.id(),
                        place(patch.piece),
                        patch.piece.uv(),
                        Color32::WHITE,
                    );
                }
            }
        }
    }

    /// `wanted` asked for. Where it is not shown and no level but the
    /// map's own stands in (a first frame), it is the level uploaded
    /// whole this frame: read now if cached, its reading task waited on,
    /// else made in a task as with a stand-in (`disk::at_once`'s rule).
    fn want(&mut self, wanted: Level) -> Option<Level> {
        let standing = |l: &Level, s: &Stage| {
            matches!(s, Stage::Shown(_)) && (*l == wanted || *l != Level::One)
        };

        if self.made.iter().any(|(l, s)| standing(l, s)) {
            self.ask(wanted);
            self.count(wanted);
            None
        } else {
            let dir = self.dir.clone();
            let stage = match self.made.remove(&wanted) {
                Some(Stage::Making(task, count, Found::Cached)) => {
                    Stage::Uploading(block_on(task), Vec::new(), count)
                }
                Some(stage) => stage,
                None => match self.base.cached(dir.as_deref(), wanted) {
                    Found::Cached => {
                        let made = made(&self.base, wanted, dir.as_deref());

                        Stage::Uploading(made, Vec::new(), None)
                    }
                    Found::Missing => self.task(wanted, Found::Missing),
                },
            };

            self.made.insert(wanted, stage);
            self.count(wanted);
            Some(wanted)
        }
    }

    /// `level` made in the background, if it is not: uncounted.
    fn ask(&mut self, level: Level) {
        if !self.made.contains_key(&level) {
            let found = self.base.cached(self.dir.as_deref(), level);
            let task = self.task(level, found);

            self.made.insert(level, task);
        }
    }

    /// A task making `level`.
    fn task(&self, level: Level, found: Found) -> Stage {
        let base = self.base.clone();
        let dir = self.dir.clone();
        let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
        let task = pool.spawn(async move { made(&base, level, dir.as_deref()) });

        Stage::Making(task, None, found)
    }

    /// Levels made since last pass uploaded: `now` whole, the others
    /// [`UPLOAD`] bytes a frame between them.
    fn poll(&mut self, ctx: &egui::Context, now: Option<Level>) {
        let mut left = UPLOAD;
        let mut all = usize::MAX;

        self.made = std::mem::take(&mut self.made)
            .into_iter()
            .map(|(level, stage)| {
                let stage = if Some(level) == now {
                    step(ctx, level, stage, &mut all)
                } else {
                    step(ctx, level, stage, &mut left)
                };

                (level, stage)
            })
            .collect();
    }

    /// `wanted` if it is shown, else the shown level nearest its scale,
    /// the larger on a tie.
    fn nearest(&self, wanted: Level) -> Level {
        let shown = |l: &Level| matches!(self.made.get(l), Some(Stage::Shown(_)));
        let off = |l: &Level| (l.scale() / wanted.scale()).log2().abs();

        Level::ALL
            .into_iter()
            .rev()
            .filter(shown)
            .min_by(|a, b| off(a).total_cmp(&off(b)))
            .unwrap_or(Level::One)
    }
}

/// `stage` of `level` a pass on: a made level's tiles uploaded while
/// `left` bytes last, one at least; shown once all are up.
fn step(ctx: &egui::Context, level: Level, stage: Stage, left: &mut usize) -> Stage {
    match stage {
        Stage::Making(mut task, count, found) => {
            if let Some(made) = check_ready(&mut task) {
                step(ctx, level, Stage::Uploading(made, Vec::new(), count), left)
            } else {
                ctx.request_repaint();
                Stage::Making(task, count, found)
            }
        }
        Stage::Uploading(mut made, mut up, count) => {
            let mut n = 0;

            while n < made.tiles.len() && *left > 0 {
                *left = left.saturating_sub(area(made.tiles[n].1.size));
                n += 1;
            }

            let first = up.len();
            let tiles = made.tiles.drain(..n).enumerate().map(|(i, (p, image))| {
                let name = format!("quest map {level:?} tile {}", first + i);

                (p, ctx.load_texture(name, image, TextureOptions::LINEAR))
            });

            up.extend(tiles);

            if made.tiles.is_empty() {
                let lits = made
                    .lits
                    .into_iter()
                    .map(|(piece, image)| Patch {
                        piece,
                        image: Some(image),
                        texture: None,
                    })
                    .collect();

                Stage::Shown(Shown {
                    size: made.size,
                    tiles: up,
                    lits,
                })
            } else {
                ctx.request_repaint();
                Stage::Uploading(made, up, count)
            }
        }
        shown @ Stage::Shown(_) => shown,
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small whole numbers, exact in f32"
)]
mod tests {
    use super::*;

    /// The quest map, `IX10` 1001.
    const MAP: [usize; 2] = [1398, 1611];

    /// Each pixel of `size`, the times `areas` cover it.
    fn covered(size: [usize; 2], areas: impl Iterator<Item = Area>) -> Vec<u32> {
        let mut times = vec![0; size[0] * size[1]];

        for a in areas {
            for y in a.at[1]..a.at[1] + a.size[1] {
                for x in a.at[0]..a.at[0] + a.size[0] {
                    times[y * size[0] + x] += 1;
                }
            }
        }

        times
    }

    #[test]
    fn a_pasted_picture_covers_the_map_and_stops_at_its_edge() {
        let (red, blue) = (Color32::RED, Color32::BLUE);
        let mut map = ColorImage::new([3, 2], vec![red; 6]);
        let picture = ColorImage::new([3, 1], vec![blue, Color32::TRANSPARENT, blue]);

        paste(&mut map, [1, 1], &picture);

        assert_eq!(map.pixels, [red, red, red, red, blue, red]);
    }

    #[test]
    fn the_level_is_the_least_at_or_over_the_scale() {
        assert_eq!(Level::of(0.4), Level::Half);
        assert_eq!(Level::of(0.6), Level::One);
        assert_eq!(Level::of(1.0), Level::One);
        assert_eq!(Level::of(1.2), Level::Two);
        assert_eq!(Level::of(2.01), Level::Two);
        assert_eq!(Level::of(3.0), Level::Four);
        assert_eq!(Level::of(9.0), Level::Four);
        assert_eq!(Level::Two.size(MAP), [2796, 3222]);
        assert_eq!(Level::Half.size(MAP), [699, 806]);
    }

    /// Every level of the map, and an odd small one: tiles show each pixel
    /// once, hold at most a gutter more, and fit egui's 2048.
    #[test]
    fn tiles_show_every_pixel_once() {
        let sizes = Level::ALL.map(|l| (l.size(MAP), TILE));

        for (size, side) in sizes.into_iter().chain([([7, 5], 3)]) {
            let pieces = tiles(size, side);
            let shown = covered(size, pieces.iter().map(|p| p.shows));

            assert!(shown.iter().all(|&n| n == 1), "{size:?}");

            for p in &pieces {
                let [s, h] = [p.shows, p.holds];

                assert!((0..2).all(|i| h.at[i] <= s.at[i] && s.at[i] - h.at[i] <= GUTTER));
                assert!((0..2).all(|i| h.at[i] + h.size[i] <= size[i]));
                assert!((0..2).all(|i| h.size[i] <= s.size[i] + 2 * GUTTER));
                assert!(h.size.iter().all(|&n| n <= 2048));
            }
        }
    }

    /// On the window the tiles meet edge to edge: the map's rectangle is
    /// cut with no gap, and a shared edge is the same number either side.
    #[test]
    fn tiles_meet_on_the_map_without_a_gap() {
        for level in Level::ALL {
            let size = level.size(MAP);
            let rects: Vec<Rect> = tiles(size, TILE)
                .iter()
                .map(|p| on_map(p.shows, size, MAP))
                .collect();
            let area: f32 = rects.iter().map(Rect::area).sum();

            assert!((area - 1398.0 * 1611.0).abs() < 1.0, "{level:?}: {area}");

            for a in &rects {
                for b in &rects {
                    let beside =
                        a.max.x > b.min.x - 0.5 && a.max.x < b.min.x + 0.5 && a.min.y == b.min.y;

                    if beside {
                        assert_eq!(a.max.x, b.min.x, "{level:?}");
                    }
                }
            }

            let whole = rects.iter().fold(Rect::NOTHING, |u, r| u.union(*r));

            assert_eq!(
                whole,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1398.0, 1611.0))
            );
        }
    }

    /// The tile's texture coordinates take its gutter off.
    #[test]
    fn a_tiles_uv_leaves_its_gutter_out() {
        let p = tiles([10, 10], 4)[4];

        assert_eq!(
            p.shows,
            Area {
                at: [4, 4],
                size: [4, 4]
            }
        );
        assert_eq!(
            p.holds,
            Area {
                at: [3, 3],
                size: [6, 6]
            }
        );
        let uv = Rect::from_min_max(pos2(1.0 / 6.0, 1.0 / 6.0), pos2(5.0 / 6.0, 5.0 / 6.0));

        assert!((p.uv().min - uv.min).length() < 1e-6 && (p.uv().max - uv.max).length() < 1e-6);
    }

    /// A lit patch covers its picture on every level, on the level's own
    /// pixels: The Barren Waste (815, 1222) 176 × 152, The Dark Forest
    /// (195, 908) 179 × 174.
    #[test]
    fn a_lit_patch_covers_its_picture_on_the_levels_pixels() {
        for (at, lit) in [([815, 1222], [176, 152]), ([195, 908], [179, 174])] {
            let picture = Rect::from_min_size(
                pos2(at[0] as f32, at[1] as f32),
                egui::vec2(lit[0] as f32, lit[1] as f32),
            );

            for level in Level::ALL {
                let size = level.size(MAP);
                let p = patch(at, lit, MAP, size);

                assert!(
                    on_map(p.shows, size, MAP).contains_rect(picture),
                    "{level:?}"
                );
                assert!((0..2).all(|i| p.holds.at[i] + p.holds.size[i] <= size[i]));
            }

            // At 1× it is the picture's own rectangle.
            assert_eq!(patch(at, lit, MAP, MAP).shows, Area { at, size: lit });
        }
    }

    /// A small map, a quest on it, and a table turning every colour white.
    fn sample() -> (ColorImage, Quest, Table) {
        let pixels = (0..120 * 100)
            .map(|i| Color32::from_rgb((i * 7 % 200) as u8, (i * 13 % 180) as u8, 40))
            .collect();
        let map = ColorImage::new([120, 100], pixels);
        let quest = Quest {
            slot: 0,
            at: [6, 4],
            size: [100, 80],
        };
        let white: Vec<u8> = (0..glow::SIDE.pow(3) * 3)
            .flat_map(|_| 1f32.to_le_bytes())
            .collect();

        (map, quest, Table::from_bytes(&white).expect("a table"))
    }

    /// The levels' base: `map`, `quest` on it lit through `table`.
    fn base(map: &Original, quest: Quest, table: &Table) -> Base {
        let table = Table::from_bytes(&table.bytes()).expect("a table");

        Base::new(map.clone(), vec![quest], (table, 0))
    }

    /// The sample's levels, its quest in `slot` (each test its own
    /// files), the map's own made, none other; and the cache.
    fn fresh(slot: usize) -> (Levels, PathBuf) {
        let (map, mut quest, table) = sample();

        quest.slot = slot;

        let base = base(&Original::new(map.clone()), quest, &table);
        let one = make(&base, Level::One);
        let made = HashMap::from([(
            Level::One,
            Stage::Uploading(one, Vec::new(), Some(Count::new())),
        )]);

        let dir = disk::tests();

        (
            Levels {
                base,
                made,
                upscale: Upscale::None,
                dir: Some(dir.clone()),
            },
            dir,
        )
    }

    /// Upscaling off (`ui_upscale = none`, the default): no xBRZ level is
    /// made ahead, as no other picture is upscaled; on, 2× is. Turned
    /// off, the xBRZ levels go.
    #[test]
    fn with_upscaling_off_no_xbrz_level_is_made_ahead() {
        let ctx = egui::Context::default();
        let (mut levels, _) = fresh(4);

        levels.ahead(&ctx);
        assert!(!levels.made.contains_key(&Level::Two));

        levels.set(Upscale::Xbrz);
        levels.ahead(&ctx);
        assert!(levels.made.contains_key(&Level::Two));

        levels.set(Upscale::None);
        assert!(!levels.made.contains_key(&Level::Two));
        assert!(levels.made.contains_key(&Level::One));
    }

    /// Off, a scale past the map's own wants the map; halving stays.
    #[test]
    fn with_upscaling_off_the_map_stands_for_xbrz() {
        let level = |scale, mode| Level::of(scale).within(mode);

        assert_eq!(level(3.0, Upscale::Xbrz), Level::Four);
        assert_eq!(level(3.0, Upscale::None), Level::One);
        assert_eq!(level(1.5, Upscale::None), Level::One);
        assert_eq!(level(0.4, Upscale::None), Level::Half);
    }

    /// A level read from its file is the level as made: tiles and lit
    /// patches.
    #[test]
    fn a_cached_level_reads_back_as_made() {
        let (levels, dir) = fresh(1);
        let made = make(&levels.base, Level::Two);

        let _ = std::fs::remove_file(levels.base.file(&dir, Level::Two));

        let first = super::made(&levels.base, Level::Two, Some(&dir));

        assert_eq!(levels.base.cached(Some(&dir), Level::Two), Found::Cached);

        let read = super::made(&levels.base, Level::Two, Some(&dir));

        assert_eq!(first.parts(), made.parts());
        assert_eq!(read.parts(), made.parts());
        assert_eq!(read.tiles.len() + read.lits.len(), read.parts().len());
    }

    /// The map's first frame: a cached level, nothing but the map's own to
    /// stand in, is read and uploaded at once; its reading task is waited
    /// on. With it shown, another level is made in a task.
    #[test]
    fn a_cached_level_with_no_stand_in_is_shown_at_once() {
        let ctx = egui::Context::default();
        let (mut levels, dir) = fresh(2);

        super::made(&levels.base, Level::Two, Some(&dir));

        assert_eq!(levels.want(Level::Two), Some(Level::Two));
        assert!(matches!(
            levels.made.get(&Level::Two),
            Some(Stage::Uploading(..))
        ));

        levels.poll(&ctx, Some(Level::Two));

        assert_eq!(levels.nearest(Level::Two), Level::Two);

        let _ = std::fs::remove_file(levels.base.file(&dir, Level::Four));

        assert_eq!(levels.want(Level::Four), None, "2× stands in");
        assert!(matches!(
            levels.made.get(&Level::Four),
            Some(Stage::Making(_, _, Found::Missing))
        ));

        // Asked ahead, then wanted: the reading task waited on.
        let (mut ahead, _) = fresh(2);

        ahead.ask(Level::Two);
        assert!(matches!(
            ahead.made.get(&Level::Two),
            Some(Stage::Making(_, _, Found::Cached))
        ));
        assert_eq!(ahead.want(Level::Two), Some(Level::Two));
        ahead.poll(&ctx, Some(Level::Two));
        assert_eq!(ahead.nearest(Level::Two), Level::Two);
    }

    /// Off the map's pages nothing of it counts as pending work: a level
    /// part made stays, uncounted, and work ahead adds no count. Wanted
    /// for drawing again, it counts until it shows.
    #[test]
    fn off_the_map_no_level_counts_as_pending() {
        let ctx = egui::Context::default();
        let (mut levels, dir) = fresh(4);

        super::made(&levels.base, Level::Two, Some(&dir));

        assert_eq!(levels.want(Level::Two), Some(Level::Two));
        assert_eq!(levels.counted(), 2, "the map's own and 2\u{d7}, wanted");

        levels.rest();

        assert!(levels.made.contains_key(&Level::Two), "kept");
        assert_eq!(levels.counted(), 0, "off the map");

        levels.ask(Level::Half);

        assert_eq!(levels.counted(), 0, "made ahead");

        let _ = levels.want(Level::Two);

        assert_eq!(levels.counted(), 1, "wanted again");

        levels.poll(&ctx, Some(Level::Two));

        assert_eq!(levels.counted(), 0, "shown");
    }

    /// Not wanted at once, a level goes up within the frame's bytes, one
    /// tile at least, and shows when all are up; off the map, 4× is let go.
    #[test]
    fn a_level_ahead_uploads_over_frames_and_four_is_let_go() {
        let ctx = egui::Context::default();
        let (mut levels, _) = fresh(3);
        let tile = |n: usize| {
            (
                Piece::of(
                    Area {
                        at: [0, 0],
                        size: [n, n],
                    },
                    [n, n],
                ),
                ColorImage::filled([n, n], Color32::RED),
            )
        };
        let side = 2048;
        let made = Made {
            size: [side, side * 4],
            tiles: (0..4).map(|_| tile(side)).collect(),
            lits: Vec::new(),
        };

        levels.made.insert(
            Level::Four,
            Stage::Uploading(made, Vec::new(), Some(Count::new())),
        );

        let up = |l: &Levels| match l.made.get(&Level::Four) {
            Some(Stage::Uploading(_, up, _)) => up.len(),
            Some(Stage::Shown(shown)) => shown.tiles.len() + 100,
            _ => 0,
        };

        // 16 MB a tile, 32 MB a frame: two a frame.
        levels.poll(&ctx, None);
        assert_eq!(up(&levels), 2);
        levels.poll(&ctx, None);
        assert_eq!(up(&levels), 104, "shown");

        levels.rest();
        assert!(levels.made.contains_key(&Level::One));
        assert!(!levels.made.contains_key(&Level::Four));
    }

    /// At the map's own size the patch is the look on the map: each pixel
    /// moved to white by its weight, near full at the centre, little at the
    /// rectangle's corner; its gutter is the map's.
    #[test]
    fn at_one_the_patch_is_the_look_on_the_map() {
        let (map, quest, table) = sample();
        let original = Original::new(map.clone());
        let made = make(&base(&original, quest, &table), Level::One);
        let (piece, image) = &made.lits[0];
        let at = |x: usize, y: usize| {
            image.pixels[(y - piece.holds.at[1]) * image.size[0] + x - piece.holds.at[0]]
        };

        assert_eq!(
            piece.shows,
            Area {
                at: quest.at,
                size: quest.size
            }
        );
        assert_eq!(at(5, 4), map.pixels[4 * 120 + 5]);
        assert!(at(56, 44).to_array()[..3].iter().all(|&c| c >= 250));

        let corner = at(6, 4).to_array();
        let under = map.pixels[4 * 120 + 6].to_array();

        assert!(
            (0..3).all(|c| corner[c].abs_diff(under[c]) <= 6),
            "{corner:?} {under:?}"
        );
    }

    /// Halved and doubled, a patch's gutter is the level's own pixels: it
    /// lies on the level's grid, so it cannot drift.
    #[test]
    fn away_from_its_rectangle_a_patch_is_the_levels_pixels() {
        let (map, quest, table) = sample();
        let original = Original::new(map.clone());

        for level in [Level::Half, Level::Two] {
            let made = make(&base(&original, quest, &table), level);
            let (piece, image) = &made.lits[0];
            let whole = match level.times() {
                Some(f) => scale::xbrz(&map, f, Edge::Clamp),
                None => shrink::shrink(&map, level.size(map.size)),
            };
            let level_at = |x: usize, y: usize| whole.pixels[y * whole.size[0] + x];

            // The gutter's top row lies outside the rectangle.
            let top = piece.holds.at[1];
            let row: Vec<Color32> = (0..piece.holds.size[0])
                .map(|x| level_at(piece.holds.at[0] + x, top))
                .collect();

            assert_eq!(
                &image.pixels[..piece.holds.size[0]],
                row.as_slice(),
                "{level:?}"
            );
        }
    }

    /// The same quest at 1× and 2×: the look at a map point is near the
    /// same, its weight measured in map pixels at either.
    #[test]
    fn the_weight_follows_the_map_at_every_level() {
        let (_, quest, _) = sample();
        let grid = |scale: usize| Grid {
            at: quest.at.map(|n| n * scale),
            size: quest.size.map(|n| n * scale),
            scale: scale as f64,
        };
        let rect = [6.0, 4.0, 100.0, 80.0];
        let one = glow::weights(&glow::ellipse(rect, grid(1)), grid(1));
        let two = glow::weights(&glow::ellipse(rect, grid(2)), grid(2));

        // Map pixel (x, y) of the rectangle; at 2× its four pixels' mean.
        for (x, y) in [(50, 40), (8, 40), (20, 15)] {
            let small = one[y * 100 + x];
            let big: f32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|(dx, dy)| two[(2 * y + dy) * 200 + 2 * x + dx])
                .sum::<f32>()
                / 4.0;

            assert!((small - big).abs() < 0.03, "({x}, {y}): {small} {big}");
        }
    }
}
