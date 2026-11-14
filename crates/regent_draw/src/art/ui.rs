//! The art service's interface front end: one lookup for every menu and
//! panel picture as an egui texture. One [`Art`], a resource, for the
//! menus and the panels.
//!
//! ```text
//!  draw code ──Want { source, picture, tone, wrap }, Place { scale, origin, rect }──> Art
//!            <──Ready(Shown { texture, window rect }) or Pending(stand-in)──┘
//!          ┌────────────────────────────────────────────┬─────────────────┐
//!          ▼                                            ▼                 ▼
//!   originals (Read: catalogue,              memory: plain textures, disk: xBRZ, LZ4
//!   the menus' pictures), toned              originals, sized ones    (`disk`)
//!                                            by source (`Entries`)
//! ```
//!
//! A picture is drawn plain at its rectangle, or made for the window
//! pixels the rectangle covers: upscaled with xBRZ (`[enhancements]
//! ui_upscale`) at the least of 2×, 3×, 4× covering its size on screen
//! (`scale::factor`), in Bevy's task pool, then area-shrunk to it on the
//! GPU (`gpu`), or the CPU (`shrink`) where none lends a texture;
//! meanwhile the held size nearest stands in, else the plain
//! picture. One cached on disk with no stand-in is made at once
//! (`disk::at_once`): no plain flash on a page change. A character's list
//! icon is its portrait shrunk, or upscaled, to its slot.
//!
//! The same picture from one source is held and made once, whoever
//! asks; a game's source is shelved with its dataset ([`Art::take`]),
//! the menus' stay.
//!
//! The rule for a page: as it loads, it may wait for its still pictures
//! made (they never show a stand-in where the disk cache holds them).
//! Once it shows, its animations run smooth: a frame not made by its
//! turn shows its plain picture till the made one catches up
//! ([`Art::strip`], held by [`Scene`]).
//!
//! A file in the override folder (`over`) is a picture's pixels at its
//! own size, drawn in the original's place by the same policy. The quest
//! map's levels do not read it.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::{Path, PathBuf};

use bevy::platform::hash::FixedHasher;
use bevy::prelude::Resource;
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool, futures::check_ready};
use bevy_egui::egui::emath::GuiRounding;
use bevy_egui::egui::{
    self, ColorImage, Pos2, Rect, TextureHandle, TextureId, TextureOptions, Vec2, pos2,
};
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::INTERFACE_ICON;

use super::disk::{self, Original, Other};
use super::gpu::{Drawn, Gpu};
use super::scale::{self, Edge};
use super::{
    ArtSource, Count, Pinned, Read, Shelving, Split, Tile, levels, look, moved, over, remembered,
    shrink,
};
use super::{Tone, Wrap};

/// How the interface's pictures are scaled up: the settings'
/// `ui_upscale`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum Upscale {
    /// Filtered, as drawn so far.
    #[default]
    None,
    Xbrz,
}

/// Passes a sized texture is kept undrawn.
const STALE: u64 = 60;

/// A pass's main-thread work for frames made ahead ([`Art::strip`]): an
/// original read, or a made frame uploaded and shrunk. Past it they wait
/// for the next pass, so none hitches.
pub const BUDGET: usize = 4;

/// All of a texture.
const WHOLE: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

/// One picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pic {
    /// Frame `frame` of slot `slot` of `record`'s set `set`.
    Set {
        record: FourCc,
        set: i32,
        slot: usize,
        frame: usize,
    },
    /// A unit's portrait, its set 1000.
    Portrait(FourCc),
    /// A hero's stonework, its set 1001.
    Backdrop(FourCc),
}

impl Pic {
    /// Slot 0, frame 0 of `record`'s `set`.
    #[must_use]
    pub const fn of(record: FourCc, set: i32) -> Pic {
        Pic::Set {
            record,
            set,
            slot: 0,
            frame: 0,
        }
    }

    /// The same set's `slot`.
    #[must_use]
    pub const fn slot(self, slot: usize) -> Pic {
        match self {
            Pic::Set {
                record, set, frame, ..
            } => Pic::Set {
                record,
                set,
                slot,
                frame,
            },
            other => other,
        }
    }

    /// The same slot's `frame`.
    #[must_use]
    pub const fn frame(self, frame: usize) -> Pic {
        match self {
            Pic::Set {
                record, set, slot, ..
            } => Pic::Set {
                record,
                set,
                slot,
                frame,
            },
            other => other,
        }
    }
}

/// One picture in its source, by what it is, not where it is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    /// A set's frame, a portrait or a backdrop: the panels' and the
    /// menus' pictures.
    Pic(Pic),
    /// A character's portrait as its list icon draws it, whole
    /// (`portrait::icon_source`).
    Icon(FourCc),
    /// A room animation's frame.
    Tile(Tile),
    /// A unit's set, every layer as the map stacks it (`read::stacked`).
    Stack(Stack),
}

/// Step `step` of unit `code`'s set `set` facing `facing`: every
/// first-pass layer on one canvas, each at its own frame
/// (`read::Timeline`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stack {
    pub code: FourCc,
    /// Low 16 bits of the set id, as `Unit::stack` takes them.
    pub set: u16,
    /// A direction of 32, 0 north; each layer picks its slot by it.
    pub facing: u16,
    pub step: usize,
}

/// What a caller asks for: a picture from its source, its tone, and how
/// it fills its rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Want {
    pub source: ArtSource,
    pub picture: Picture,
    pub tone: Tone,
    pub wrap: Wrap,
}

impl Want {
    /// `picture` from `source` plain, drawn once.
    #[must_use]
    pub const fn of(source: ArtSource, picture: Picture) -> Want {
        Want {
            source,
            picture,
            tone: Tone::Plain,
            wrap: Wrap::Once,
        }
    }

    /// The same in `tone`.
    #[must_use]
    pub const fn tone(self, tone: Tone) -> Want {
        Want { tone, ..self }
    }

    /// The same filling a rectangle as `wrap` says.
    #[must_use]
    pub const fn wrap(self, wrap: Wrap) -> Want {
        Want { wrap, ..self }
    }
}

/// How a placed rectangle meets the window's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Snap {
    /// Its corner on a pixel, its side rounded: every list icon at a
    /// scale one size (the panels).
    Corner,
    /// Each edge on a pixel, so pieces meeting in layout meet on the
    /// window: the quest map's frame (the menus).
    Edges,
    /// On the pixels of the picture placed [`Snap::Edges`] at this layout
    /// rectangle, cut to the whole pixels inside its own edge: a room
    /// object's frame, at its own size, meets the room pixel for pixel.
    ///
    /// ```text
    ///  room pixels  |   |   |   |   |   |
    ///  frame           [·|···|···|·]        edge, less the inset
    ///  drawn               |···|            whole pixels inside
    /// ```
    Over(Rect),
}

/// How far in from its edge a picture laid [`Snap::Over`] another is cut,
/// its own pixels: plain, linear sampling's reach (half a pixel), so none
/// is read clamped at its edge.
const LINEAR_REACH: f32 = 0.5;

/// Where a placed picture lands on the window's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Laid {
    /// The window pixels it covers.
    pub pixels: [usize; 2],
    /// Where they are, points.
    pub rect: Rect,
    /// The part of the picture shown, its own pixels from its corner;
    /// `None`: all of it.
    pub cut: Option<Rect>,
}

/// Where a picture goes: a rectangle in a page's own pixels, the page's
/// scale and origin on the window, and how it snaps to pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// Window points a page pixel.
    pub scale: f32,
    /// Where page (0, 0) lands, window points.
    pub origin: Pos2,
    pub rect: Rect,
    pub snap: Snap,
}

impl Place {
    /// Page rectangle `r` on the window.
    fn window(self, r: Rect) -> Rect {
        super::onto(r, self.scale, self.origin)
    }

    /// The window pixels the rectangle covers at `ppp` pixels a point,
    /// and where they are: what a sized picture is made at.
    #[must_use]
    pub fn pixels(self, ppp: f32) -> ([usize; 2], Rect) {
        let shown = match self.snap {
            Snap::Corner => {
                let side = (self.rect.size() * self.scale * ppp).round();
                let corner = self.window(self.rect).min.round_to_pixels(ppp);

                Rect::from_min_size(corner, side / ppp)
            }
            Snap::Edges => self.window(self.rect).round_to_pixels(ppp),
            Snap::Over(_) => self.laid(ppp, 0.0).rect,
        };
        let side = (shown.size() * ppp).round();

        (whole_pixels(side), shown)
    }

    /// Where the picture lands at `ppp` pixels a point; laid
    /// [`Snap::Over`] another, its rectangle at its own size, cut `inset`
    /// of its pixels in from its edge.
    #[must_use]
    pub fn laid(self, ppp: f32, inset: f32) -> Laid {
        match self.snap {
            Snap::Over(under) => {
                // The picture under it, window pixels, and its window
                // pixels a layout pixel.
                let drawn = self.window(under).round_to_pixels(ppp);
                let grid = Rect::from_min_max((drawn.min * ppp).round(), (drawn.max * ppp).round());
                let k = grid.size() / under.size();
                let on = |p: Pos2| grid.min + (p - under.min) * k;
                let inner = self.rect.shrink(inset);
                let min = on(inner.min).ceil();
                let max = on(inner.max).floor().max(min);
                let back = |p: Pos2| under.min + (p - grid.min) / k - self.rect.min.to_vec2();

                Laid {
                    pixels: whole_pixels(max - min),
                    rect: Rect::from_min_max(min / ppp, max / ppp),
                    cut: Some(Rect::from_min_max(back(min), back(max))),
                }
            }
            Snap::Corner | Snap::Edges => {
                let (pixels, rect) = self.pixels(ppp);

                Laid {
                    pixels,
                    rect,
                    cut: None,
                }
            }
        }
    }
}

/// `side`, whole window pixels, as a picture's size.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a picture's pixels"
)]
const fn whole_pixels(side: Vec2) -> [usize; 2] {
    [side.x as usize, side.y as usize]
}

/// A texture, where on the window to draw it, and its part drawn there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub id: TextureId,
    pub rect: Rect,
    pub uv: Rect,
}

/// What the service answers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Answer<T = Shown> {
    Ready(T),
    /// Being made: the held size nearest stands in, else the plain
    /// picture.
    Pending(T),
}

impl<T> Answer<T> {
    /// What to draw now.
    pub fn now(self) -> T {
        match self {
            Answer::Ready(t) | Answer::Pending(t) => t,
        }
    }

    fn map<U>(self, f: impl FnOnce(T) -> U) -> Answer<U> {
        match self {
            Answer::Ready(t) => Answer::Ready(f(t)),
            Answer::Pending(t) => Answer::Pending(f(t)),
        }
    }
}

/// An upscaled size's state.
enum Made {
    Ready(TextureId),
    /// Being made, another size standing in: its part (0 to 1) drawn,
    /// past its edge where it was cut for another window.
    Standing(TextureId, Rect),
    /// Being made, none to stand in.
    Making,
}

/// The held size to stand in for `wanted` while it is made: the nearest
/// by area, the larger on a tie.
///
/// ```text
///  held 100², 300²   wanted 200²  →  300² (log ratios 1.39, 0.81)
/// ```
#[must_use]
pub fn stand_in(
    held: impl IntoIterator<Item = [usize; 2]>,
    wanted: [usize; 2],
) -> Option<[usize; 2]> {
    #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
    let area = |s: [usize; 2]| (s[0].max(1) * s[1].max(1)) as f32;
    let off = |s: [usize; 2]| (area(s) / area(wanted)).ln().abs();

    held.into_iter().min_by(|a, b| {
        off(*a)
            .total_cmp(&off(*b))
            .then_with(|| area(*b).total_cmp(&area(*a)))
    })
}

/// A character's list icon (set 1002, its first frame) as its portrait:
/// sharper than the 25-pixel icon scaled up. A departure from the
/// original (docs/openregent/game.md).
fn icon(want: Want) -> Option<Want> {
    match want {
        Want {
            source,
            picture:
                Picture::Pic(Pic::Set {
                    record,
                    set,
                    slot: 0,
                    frame: 0,
                }),
            wrap: Wrap::Once,
            tone,
        } if set == i32::from(INTERFACE_ICON) => {
            Some(Want::of(source, Picture::Icon(record)).tone(tone))
        }
        _ => None,
    }
}

/// Where a picture's pixels came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Archive,
    /// A file in the override folder (`over`), at its own size.
    Override,
}

/// `want`'s pixels toned, and its original's size: an override's pixels
/// where one reads, else the archive's. `None` where the original is
/// not: an override replaces a picture, it adds none.
fn pixels(
    over: &mut over::Overrides,
    read: &dyn Read,
    want: Want,
) -> Option<(ColorImage, [usize; 2], Origin)> {
    let image = read.read(want.source, want.picture)?;
    let size = image.size;
    let replaced = if over.is_empty() {
        None
    } else {
        over.read(&read.keys(want.source, want.picture))
    };

    let pixels = match replaced {
        Some(o) => (look::toned_as(o, want.tone, size), size, Origin::Override),
        None => (look::toned(image, want.tone), size, Origin::Archive),
    };

    Some(pixels)
}

/// How a sized texture is made from its original.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Way {
    /// xBRZ, then shrunk.
    Upscaled,
    /// Shrunk alone: a list icon's portrait, an override larger than
    /// its place.
    Shrunk,
}

/// One picture's texture at a size.
enum Entry {
    /// Being made: shrunk, or upscaled alone to shrink on the GPU.
    Making(Task<egui::ColorImage>, Count, On),
    Ready(Texture),
}

/// Where an upscaled picture is shrunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum On {
    Cpu,
    Gpu,
}

/// A sized texture: egui's own, or drawn on the GPU (`gpu`).
enum Texture {
    Egui(TextureHandle),
    Gpu(Drawn),
}

impl Texture {
    fn id(&self) -> TextureId {
        match self {
            Texture::Egui(handle) => handle.id(),
            Texture::Gpu(drawn) => drawn.id,
        }
    }

    fn size(&self) -> [usize; 2] {
        match self {
            Texture::Egui(handle) => handle.size(),
            Texture::Gpu(drawn) => drawn.size,
        }
    }
}

/// A cache map hashed by foldhash, not `SipHash`: asked per control each
/// frame, on the service's own keys.
type FastMap<K, V> = HashMap<K, V, FixedHasher>;

/// Each picture's textures by size, and the pass each was last asked for.
type Sizes = FastMap<[usize; 2], (Entry, u64)>;

/// A picture's sized textures: its source, pixels' hash, edge, way and
/// part.
type Key = (ArtSource, u64, Edge, Way, Cut);

/// What a picture is asked as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// Drawn alone.
    Picture,
    /// A running strip's frame drawn: read at once only as its scene is
    /// entered, else made in a task while its plain picture stands in.
    Shown(Owner, Stage),
    /// A running strip's frame made ahead, not drawn: never read at once,
    /// uploaded within the pass's [`BUDGET`].
    Ahead(Owner),
}

impl Role {
    /// The strip frame asking, if any.
    const fn owner(self) -> Option<Owner> {
        match self {
            Role::Picture => None,
            Role::Shown(owner, _) | Role::Ahead(owner) => Some(owner),
        }
    }
}

/// A strip's frame: the strip, by its first frame, and the frame's place
/// in it.
type Owner = (Want, usize);

/// Where a scene is when one of its strips is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Its first pass since it was away, or ever: as a page loads.
    Entering,
    /// Drawn the pass before too.
    Running,
}

/// A page whose running strips are held together ([`Art::strip`]): every
/// frame of each while any of them is drawn; once none is, each strip's
/// frame shown first on the scene's last entry alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scene(pub &'static str);

/// A scene's passes, and what its last entry cost.
struct Life {
    /// The pass a strip of it was last drawn.
    drawn: u64,
    /// The pass it was last entered.
    entered: u64,
    /// When it was last entered.
    since: std::time::Instant,
    /// Its passes since entered that drew a frame not yet made, and the
    /// last counted.
    plain: usize,
    counted: u64,
    /// Whether its strips were seen made whole since entered, and logged.
    whole: bool,
    /// Whether its strips hold every frame, or one each (away).
    held: Held,
}

/// What a scene's strips hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    All,
    One,
}

/// A running strip, by its first frame.
struct Run {
    scene: Scene,
    /// The frame shown first since its scene was entered: the one shown
    /// first on the next entry (the menus' animators start over), kept
    /// while away.
    first: usize,
    /// The pass it was last drawn.
    drawn: u64,
    frames: Vec<Want>,
    /// Whether each frame was made at its size the pass it was drawn.
    whole: bool,
}

/// A picture's part a sized texture shows, its edges' bits: all of it,
/// or cut for one window ([`Snap::Over`]).
type Cut = [u32; 4];

/// The part `share` (0 to 1) of `original`, its own pixels.
fn own(original: &Original, share: Rect) -> Rect {
    let whole = shrink::whole(&original.image).size();

    Rect::from_min_max(
        (share.min.to_vec2() * whole).to_pos2(),
        (share.max.to_vec2() * whole).to_pos2(),
    )
}

/// `cut` as a key.
fn cut_key(cut: Rect) -> Cut {
    [cut.min.x, cut.min.y, cut.max.x, cut.max.y].map(f32::to_bits)
}

/// The cut a key holds.
fn key_cut(key: Cut) -> Rect {
    let [x0, y0, x1, y1] = key.map(f32::from_bits);

    Rect::from_min_max(pos2(x0, y0), pos2(x1, y1))
}

/// The part (0 to 1) of a texture made of cut `made` that shows cut
/// `wanted`, both its picture's own pixels: an old window's frame standing
/// in on a resize.
///
/// ```text
///  made 10..20, wanted 12..22  →  0.2..1.2 (past the edge, clamped)
/// ```
fn seen(made: Rect, wanted: Rect) -> Rect {
    let at = |p: Pos2| ((p - made.min) / made.size()).to_pos2();

    Rect::from_min_max(at(wanted.min), at(wanted.max))
}

/// An original, its original's size, and where its pixels came from.
type Kept = (Original, [usize; 2], Origin);

/// Pictures held, each under its source: all of the service's, or one
/// source's taken for the shelf ([`Art::take`]).
#[derive(Resource, Default)]
pub struct Entries {
    /// Each picture at its own size, and its original's size. An
    /// override only where drawn so (not shrunk).
    plain: FastMap<Want, Option<(Texture, Vec2)>>,
    /// The originals, toned: while upscaling is on, list icons'; always,
    /// overrides'.
    originals: FastMap<(ArtSource, Picture, Tone), Option<Kept>>,
    /// Sized textures by source, pixels' hash, way, part and size.
    sized: FastMap<Key, Sizes>,
    /// The sized textures made as strips' frames, and whose: kept as
    /// their scene is, not as a picture is.
    frames: FastMap<Key, Vec<Owner>>,
    /// The overrides they were made under (`over::Overrides::identity`).
    overrides: u64,
}

impl Split for Entries {
    fn split(&mut self, source: ArtSource) -> Entries {
        Entries {
            plain: moved(&mut self.plain, |w| w.source == source),
            originals: moved(&mut self.originals, |k| k.0 == source),
            sized: moved(&mut self.sized, |k| k.0 == source),
            frames: moved(&mut self.frames, |k| k.0 == source),
            overrides: self.overrides,
        }
    }

    /// None made under other overrides.
    fn join(&mut self, other: Entries) {
        let same = other.overrides == self.overrides;

        for (key, value) in other.plain.into_iter().filter(|_| same) {
            self.plain.entry(key).or_insert(value);
        }

        for (key, value) in other.originals.into_iter().filter(|_| same) {
            self.originals.entry(key).or_insert(value);
        }

        for (key, value) in other.sized.into_iter().filter(|_| same) {
            self.sized.entry(key).or_insert(value);
        }

        for (key, value) in other.frames.into_iter().filter(|_| same) {
            self.frames.entry(key).or_insert(value);
        }
    }
}

impl Entries {
    /// Every sized texture's entry.
    fn sized(&self) -> impl Iterator<Item = &Entry> {
        self.sized
            .values()
            .flat_map(HashMap::values)
            .map(|(e, _)| e)
    }
}

/// The interface's pictures: originals read, and textures made from them,
/// for the menus and the panels alike.
#[derive(Resource, Default)]
pub struct Art {
    mode: Upscale,
    entries: Entries,
    /// The sources [`Art::take`] leaves: the menus'.
    pinned: Pinned,
    /// The pass each source was last asked in: its sized textures age
    /// only while it is drawn.
    asked: FastMap<ArtSource, u64>,
    /// The pass the stale textures were last dropped in.
    swept: u64,
    /// Each scene of running strips.
    scenes: FastMap<Scene, Life>,
    /// Each running strip, by its first frame.
    runs: FastMap<Want, Run>,
    /// The frames made ahead's main-thread work left this pass
    /// ([`BUDGET`]).
    budget: usize,
    /// Made frames uploaded since a scene was last entered.
    uploads: usize,
    /// The quest map's levels, made from the menus' start: the menus'.
    levels: Option<Map>,
    /// The disk cache's directory ([`disk`]); `None`: none.
    cache: Option<PathBuf>,
    /// Textures lent for shrinking on the GPU (`gpu::Plugin`); `None`:
    /// all shrunk on the CPU.
    pub(super) gpu: Option<Gpu>,
    /// The override folder ([`over`]); `None`: none.
    over_dir: Option<PathBuf>,
    /// Its files, as last scanned.
    over: over::Overrides,
}

impl Art {
    /// None held, `sources` [`Shelving::Pinned`].
    #[must_use]
    pub fn pinned(sources: impl IntoIterator<Item = ArtSource>) -> Art {
        Art {
            pinned: Pinned::of(sources),
            ..Art::default()
        }
    }

    /// Made pictures cached on disk at `dir`; `None`: not.
    #[must_use]
    pub fn cached(self, dir: Option<PathBuf>) -> Art {
        Art { cache: dir, ..self }
    }

    /// Pictures replaced by the files in the override folder at `dir`
    /// ([`over`]), once scanned ([`Art::rescan`]); `None`: none.
    #[must_use]
    pub fn overridden(self, dir: Option<PathBuf>) -> Art {
        Art {
            over_dir: dir,
            ..self
        }
    }

    /// The override folder scanned again; on a change, every picture
    /// made again from its new pixels.
    pub fn rescan(&mut self) {
        let dir = self.over_dir.clone();

        self.rescan_at(dir.as_deref());
    }

    /// The override folder at `dir` scanned again; none without.
    fn rescan_at(&mut self, dir: Option<&Path>) {
        let found = dir.map(over::Overrides::scan).unwrap_or_default();
        let identity = found.identity();

        if identity != self.entries.overrides {
            if let Some(dir) = dir {
                tracing::info!("art: {} overrides in {}", found.len(), dir.display());
            }

            self.over = found;
            self.entries = Entries {
                overrides: identity,
                ..Entries::default()
            };
        }
    }

    /// `source`'s pictures moved out, for the next game on its dataset;
    /// none of a pinned source, which the menus draw.
    pub fn take(&mut self, source: ArtSource) -> Entries {
        self.pinned.take(&mut self.entries, source)
    }

    /// Pictures [`Art::take`] gave, back.
    pub fn put(&mut self, entries: Entries) {
        self.entries.join(entries);
    }

    /// Upscaling set to `mode`; every sized texture dropped on a change.
    pub fn set(&mut self, mode: Upscale) {
        if self.mode != mode {
            self.mode = mode;
            self.entries.sized.clear();
            self.entries.frames.clear();
            self.entries.originals.clear();
        }
    }

    /// Each plain texture held, and its picture: the perf runs' draw
    /// dump names the textures egui draws with.
    #[must_use]
    pub fn names(&self) -> Vec<(TextureId, String)> {
        self.entries
            .plain
            .iter()
            .filter_map(|(want, held)| Some((held.as_ref()?.0.id(), format!("{:?}", want.picture))))
            .collect()
    }

    /// Whether pictures are upscaled.
    fn on(&self) -> bool {
        self.mode != Upscale::None
    }

    /// `want`'s own size, pixels; `None` where it is not.
    pub fn size(&mut self, ctx: &egui::Context, read: &dyn Read, want: Want) -> Option<Vec2> {
        let held = self.entries.plain.contains_key(&want);

        // An override's from its original: no texture at its own size
        // made for it.
        if !held && (self.filed(want) || self.replaced(read, want)) {
            #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
            self.held(read, want)
                .map(|(_, [w, h], _)| Vec2::new(w as f32, h as f32))
        } else {
            self.plain(ctx, read, want).map(|(_, size)| size)
        }
    }

    /// `want` drawn at `place`: plain at its rectangle, or upscaled or
    /// shrunk to the window pixels it covers, at those; `None` where the
    /// picture is not.
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        read: &dyn Read,
        want: Want,
        place: Place,
    ) -> Option<Answer> {
        self.answer(ctx, read, (want, Role::Picture), place)
    }

    /// `want` drawn at `place` as `role` asks it ([`Art::get`]).
    fn answer(
        &mut self,
        ctx: &egui::Context,
        read: &dyn Read,
        (want, role): (Want, Role),
        place: Place,
    ) -> Option<Answer> {
        let size = self.size(ctx, read, want)?;
        let (pixels, rect, part) = self.spot(ctx, place, size);
        let at = |id| Shown {
            id,
            rect,
            uv: WHOLE,
        };
        // Made only where drawn: an override shrunk never is.
        let plain = |art: &mut Art| {
            art.plain(ctx, read, want)
                .map(|(id, _)| Shown { id, rect, uv: part })
        };
        let pass = ctx.cumulative_pass_nr();

        self.sweep(pass);
        self.asked.insert(want.source, pass);

        let answer = match self.sharp(ctx, read, want, pixels) {
            Some(sharp) => sharp.map(at),
            None => match self.upscaled(ctx, read, (want, role), (pixels, part)) {
                Some(Made::Ready(id)) => Answer::Ready(at(id)),
                Some(Made::Standing(id, uv)) => Answer::Pending(Shown { id, rect, uv }),
                Some(Made::Making) => Answer::Pending(plain(self)?),
                None => match self.fitted(ctx, read, want, (pixels, part)) {
                    Some(id) => Answer::Ready(at(id)),
                    None => Answer::Ready(plain(self)?),
                },
            },
        };

        Some(answer)
    }

    /// Where a picture `size` pixels lands at `place`: the window pixels
    /// it covers, where they are, and its part (0 to 1) shown there.
    fn spot(&self, ctx: &egui::Context, place: Place, size: Vec2) -> ([usize; 2], Rect, Rect) {
        // A picture laid over another is cut clear of what its upscale or
        // linear sampling reads past its edge.
        #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
        let inset = if self.on() {
            scale::REACH as f32
        } else {
            LINEAR_REACH
        };
        let laid = place.laid(ctx.pixels_per_point(), inset);
        let cut = laid.cut.unwrap_or(Rect::from_min_size(Pos2::ZERO, size));
        let part = Rect::from_min_max(
            (cut.min.to_vec2() / size).to_pos2(),
            (cut.max.to_vec2() / size).to_pos2(),
        );

        (laid.pixels, laid.rect, part)
    }

    /// Frame `shown` of `frames`, a running animation's on `scene`, drawn
    /// at its own size, its corner at `place`'s (`Art::get`). While the
    /// scene is drawn every frame is held: the others made ahead in tasks,
    /// nearest first, uploaded a few a pass ([`BUDGET`]). One not made by
    /// its turn shows its plain picture, or its old size on a resize:
    /// never a stall. Once the scene is not drawn (another page, a game),
    /// each strip keeps the frame it showed first on entering it, so the
    /// next entry draws at once.
    ///
    /// ```text
    ///  frames  0 1 2 3 4 5 6 7     shown 4, entered at 1
    ///  drawn   all held, made in turn 5 3 6 2 7 1 0
    ///  away    1 alone
    /// ```
    pub fn strip(
        &mut self,
        ctx: &egui::Context,
        read: &dyn Read,
        frames: &[Want],
        shown: usize,
        (place, scene): (Place, Scene),
    ) -> Option<Answer> {
        let strip = *frames.first()?;
        let want = *frames.get(shown)?;
        let count = frames.len();
        let pass = ctx.cumulative_pass_nr();

        self.sweep(pass);

        let (stage, entered) = self.enter(scene, pass);
        let run = self.runs.entry(strip).or_insert_with(|| Run {
            scene,
            first: shown,
            drawn: pass,
            frames: Vec::new(),
            whole: false,
        });

        // Its first pass since the scene was entered: the frame kept away.
        if run.drawn < entered {
            run.first = shown;
        }

        run.drawn = pass;
        run.scene = scene;

        if run.frames != frames {
            run.frames = frames.to_vec();
        }

        let rect = Rect::from_min_size(place.rect.min, self.size(ctx, read, want)?);
        let answer = self.answer(
            ctx,
            read,
            (want, Role::Shown((strip, shown), stage)),
            Place { rect, ..place },
        );

        // The others, nearest first either way: the next shown is made
        // soonest.
        let ahead = (1..=count / 2).flat_map(|k| {
            let (next, back) = ((shown + k) % count, (shown + count - k) % count);

            std::iter::once(next).chain((back != next).then_some(back))
        });
        let mut made = matches!(answer, Some(Answer::Ready(_)));

        for i in ahead {
            made &= self.ahead(ctx, read, (frames[i], (strip, i)), place);
        }

        if let Some(run) = self.runs.get_mut(&strip) {
            run.whole = made;
        }

        // A frame not made by its turn counted, once a pass a scene.
        if let (Some(Answer::Pending(_)), Some(life)) = (answer, self.scenes.get_mut(&scene))
            && life.counted != pass
        {
            life.counted = pass;
            life.plain += 1;
        }

        answer
    }

    /// `scene` drawn in `pass`: entered where it was not drawn the pass
    /// before; where it stands, and the pass it was entered.
    fn enter(&mut self, scene: Scene, pass: u64) -> (Stage, u64) {
        match self.scenes.get_mut(&scene) {
            Some(life) if life.drawn + 1 >= pass => life.drawn = pass,
            _ => {
                self.scenes.insert(
                    scene,
                    Life {
                        drawn: pass,
                        entered: pass,
                        since: std::time::Instant::now(),
                        plain: 0,
                        counted: u64::MAX,
                        whole: false,
                        held: Held::All,
                    },
                );
                self.uploads = 0;
            }
        }

        let entered = self.scenes.get(&scene).map_or(pass, |l| l.entered);
        let stage = if entered == pass {
            Stage::Entering
        } else {
            Stage::Running
        };

        (stage, entered)
    }

    /// Frame `want` of a running strip, `owner`, made ahead for its corner
    /// at `place`, undrawn: its original read within the pass's
    /// [`BUDGET`]. Whether it is made at its size.
    fn ahead(
        &mut self,
        ctx: &egui::Context,
        read: &dyn Read,
        (want, owner): (Want, Owner),
        place: Place,
    ) -> bool {
        let held = self
            .entries
            .originals
            .contains_key(&(want.source, want.picture, want.tone));
        let readable = self.on() && (held || self.budget > 0);

        if readable && !held {
            self.budget -= 1;
        }

        match readable.then(|| self.original(read, want)).flatten() {
            Some(original) => {
                #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
                let size = Vec2::new(original.image.size[0] as f32, original.image.size[1] as f32);
                let rect = Rect::from_min_size(place.rect.min, size);
                let (pixels, _, part) = self.spot(ctx, Place { rect, ..place }, size);
                let role = (want, Role::Ahead(owner));

                matches!(
                    self.upscaled(ctx, read, role, (pixels, part)),
                    Some(Made::Ready(_)) | None
                )
            }
            // Drawn plain, made on its turn; else read later, or absent.
            None => !self.on() || held,
        }
    }

    /// The quest map's levels (`levels`), made in a task on first use
    /// from `map` and each painted quest: its slot, lit rectangle and lit
    /// picture, all from `source`; `when` they are needed. Each of
    /// `under`, a downloadable quest's building at its corner, is pasted
    /// into the map first, so it lights as a painted quest does.
    pub fn levels(
        &mut self,
        read: &dyn Read,
        (source, map): (ArtSource, Picture),
        quests: impl Iterator<Item = (levels::Quest, Picture)>,
        under: impl Iterator<Item = ([usize; 2], Picture)>,
        when: When,
    ) -> Option<&mut levels::Levels> {
        if self.levels.is_none() {
            let mut image = read.read(source, map)?;

            for (at, picture) in under {
                if let Some(building) = read.read(source, picture) {
                    levels::paste(&mut image, at, &building);
                }
            }

            // A rectangle or picture past the map's edge is left out.
            let inside =
                |at: [usize; 2], size: [usize; 2]| (0..2).all(|i| at[i] + size[i] <= image.size[i]);
            let (quests, lits): (Vec<_>, Vec<_>) = quests
                .filter(|(q, _)| inside(q.at, q.size))
                .map(|(q, picture)| {
                    let lit = read
                        .read(source, picture)
                        .filter(|l| inside(q.at, l.size))
                        .map(|image| levels::Lit { at: q.at, image });

                    (q, lit)
                })
                .unzip();
            let lits: Vec<levels::Lit> = lits.into_iter().flatten().collect();

            let dir = self.cache.clone();
            let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
            let task = pool.spawn(async move { levels::Levels::new(image, quests, &lits, dir) });

            self.levels = Some(Map::Making(task));
        }

        if let Some(Map::Making(task)) = &mut self.levels {
            let made = match when {
                When::Now => Some(bevy::tasks::block_on(task)),
                When::Ahead => check_ready(task),
            };

            if let Some(levels) = made {
                self.levels = Some(Map::Ready(levels));
            }
        }

        let mode = self.mode;

        match &mut self.levels {
            Some(Map::Ready(levels)) => {
                levels.set(mode);
                Some(levels)
            }
            Some(Map::Making(..)) | None => None,
        }
    }

    /// The quest map's levels but its own dropped: the page is not the
    /// map's.
    pub fn rest(&mut self) {
        if let Some(Map::Ready(levels)) = &mut self.levels {
            levels.rest();
        }
    }

    /// `want` at its own size as a texture, and that size; made on first
    /// use.
    fn plain(
        &mut self,
        ctx: &egui::Context,
        read: &dyn Read,
        want: Want,
    ) -> Option<(TextureId, Vec2)> {
        let fresh = !self.entries.plain.contains_key(&want);
        // An override is decoded once: its texture is made from the
        // original held, which shrinks and upscales read too.
        let held = (fresh && self.replaced(read, want))
            .then(|| self.held(read, want))
            .flatten();
        let (gpu, over) = (&mut self.gpu, &mut self.over);
        let made = || {
            let (image, size) = match held {
                Some((original, size, _)) => ((*original.image).clone(), size),
                None => pixels(over, read, want).map(|(image, size, _)| (image, size))?,
            };
            // An override past the texture limit is drawn from one shrunk.
            let most = scale::capped(image.size, ctx.input(|i| i.max_texture_side));
            let image = if most == image.size {
                image
            } else {
                shrink::shrink(&image, most)
            };
            #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
            let size = Vec2::new(size[0] as f32, size[1] as f32);
            let name = format!("{want:?}");

            Some((texture(ctx, gpu, (name, want.wrap.edge()), image), size))
        };

        remembered(&mut self.entries.plain, want, read.reach(), made)
            .map(|(texture, size)| (texture.id(), *size))
    }

    /// `want`, an override, its part `share` (0 to 1) shrunk to `pixels`
    /// where that part is larger; `None` for any other picture: draw it
    /// plain.
    fn fitted(
        &mut self,
        ctx: &egui::Context,
        read: &dyn Read,
        want: Want,
        (pixels, share): ([usize; 2], Rect),
    ) -> Option<TextureId> {
        let original = self
            .filed(want)
            .then(|| self.original(read, want))
            .flatten()?;
        let pixels = scale::capped(pixels, ctx.input(|i| i.max_texture_side));
        let cut = own(&original, share);
        #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
        let larger = cut.width() > pixels[0] as f32 || cut.height() > pixels[1] as f32;
        let at = (want.source, want.wrap.edge());

        larger.then(|| self.shrunk(ctx, at, &original, (pixels, cut)))
    }

    /// `want`, a character's list icon, as its portrait in its tone
    /// upscaled or shrunk to `pixels`; `None` for any other picture.
    fn sharp(
        &mut self,
        ctx: &egui::Context,
        read: &dyn Read,
        want: Want,
        pixels: [usize; 2],
    ) -> Option<Answer<TextureId>> {
        let icon = icon(want)?;
        let original = self.original(read, icon)?;
        let whole = shrink::whole(&original.image);
        let shrunk =
            |art: &mut Art| art.shrunk(ctx, (icon.source, Edge::Clamp), &original, (pixels, whole));

        // Upscaled where the slot is larger than the portrait.
        let answer = match self.upscaled(ctx, read, (icon, Role::Picture), (pixels, WHOLE)) {
            Some(Made::Ready(id)) => Answer::Ready(id),
            Some(Made::Standing(id, _)) => Answer::Pending(id),
            Some(Made::Making) => Answer::Pending(shrunk(self)),
            None => Answer::Ready(shrunk(self)),
        };

        Some(answer)
    }

    /// `want` read from `read`, or its override, toned; `None` where it
    /// is not.
    fn original(&mut self, read: &dyn Read, want: Want) -> Option<Original> {
        self.held(read, want).map(|(original, ..)| original)
    }

    /// [`Art::original`], its original's size, and where its pixels came
    /// from.
    fn held(&mut self, read: &dyn Read, want: Want) -> Option<Kept> {
        let key = (want.source, want.picture, want.tone);
        let over = &mut self.over;
        let made = || {
            let (image, size, origin) = pixels(over, read, want)?;

            Some((Original::new(image), size, origin))
        };

        remembered(&mut self.entries.originals, key, read.reach(), made).cloned()
    }

    /// Whether `want`'s original held is an override's.
    fn filed(&self, want: Want) -> bool {
        let key = (want.source, want.picture, want.tone);

        matches!(
            self.entries.originals.get(&key),
            Some(Some((.., Origin::Override)))
        )
    }

    /// Whether a file in the override folder names `want`'s picture.
    fn replaced(&self, read: &dyn Read, want: Want) -> bool {
        !self.over.is_empty() && self.over.holds(&read.keys(want.source, want.picture))
    }

    /// `original`, from `source`, its part `cut` (its own pixels) shrunk
    /// to `pixels`, its edges as `edge` says, made at once.
    fn shrunk(
        &mut self,
        ctx: &egui::Context,
        (source, edge): (ArtSource, Edge),
        original: &Original,
        (pixels, cut): ([usize; 2], Rect),
    ) -> TextureId {
        let pass = ctx.cumulative_pass_nr();
        let gpu = &mut self.gpu;
        let sizes = self
            .entries
            .sized
            .entry((source, original.hash, edge, Way::Shrunk, cut_key(cut)))
            .or_default();
        let (entry, last) = sizes.entry(pixels).or_insert_with(|| {
            let image = shrink::part(&original.image, cut, pixels);
            let name = format!("shrunk {:016x} {pixels:?}", original.hash);

            let texture = texture(ctx, gpu, (name, edge), image);

            (Entry::Ready(texture), pass)
        });

        *last = pass;

        match entry {
            Entry::Ready(texture) => texture.id(),
            Entry::Making(..) => unreachable!("a shrink is made at once"),
        }
    }

    /// `want` upscaled, its part `share` (0 to 1: an override's pixels
    /// are cut as the original's) shrunk to `pixels`, as `role` asks it;
    /// `None` with
    /// upscaling off, at scale 1 or less, or where the picture is not:
    /// draw it plain.
    fn upscaled(
        &mut self,
        ctx: &egui::Context,
        read: &dyn Read,
        (want, role): (Want, Role),
        (pixels, share): ([usize; 2], Rect),
    ) -> Option<Made> {
        let original = self.on().then(|| self.original(read, want)).flatten()?;
        let cut = own(&original, share);
        let edge = want.wrap.edge();
        let pixels = scale::capped(pixels, ctx.input(|i| i.max_texture_side));
        #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
        let shown = Vec2::new(pixels[0] as f32, pixels[1] as f32) / cut.size();
        let factor = scale::factor(original.image.size, shown)?;
        let pass = ctx.cumulative_pass_nr();
        let picture = (want.source, original.hash, edge, Way::Upscaled);
        let key = (
            want.source,
            original.hash,
            edge,
            Way::Upscaled,
            cut_key(cut),
        );
        let fresh = self
            .entries
            .sized
            .get(&key)
            .is_none_or(|sizes| !sizes.contains_key(&pixels));
        // Any size or cut drawn: a new one is made in a task, not read at
        // once, so a resize does not wait on the disk.
        let drawn = fresh && self.ready(picture).next().is_some();
        // A running strip's frame waits for a task but as its scene is
        // entered: its plain picture stands in.
        let other = match (drawn, role) {
            (false, Role::Picture | Role::Shown(_, Stage::Entering)) => Other::Missing,
            _ => Other::Ready,
        };

        if let Some(owner) = role.owner() {
            self.owned((key, owner), (fresh, pass));
        }

        let sizes = self.entries.sized.entry(key).or_default();
        let gpu = &mut self.gpu;
        let way = (&original, edge, factor);
        let (entry, last) = sizes.entry(pixels).or_insert_with(|| {
            let dir = self.cache.clone();
            let part = (pixels, cut);
            let start = std::time::Instant::now();
            let now = disk::at_once(&original, (edge, factor), (dir.as_deref(), other));
            let entry = if let Some(big) = now {
                let texture = sized(ctx, gpu, way, part, big);
                let ms = start.elapsed().as_secs_f32() * 1000.0;

                tracing::debug!("art: made at once, {pixels:?} x{factor}, {ms:.1} ms");
                Entry::Ready(texture)
            } else {
                // Upscaled alone where the GPU shrinks it.
                let on = if gpu.is_some() { On::Gpu } else { On::Cpu };
                let original = original.clone();
                let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
                let task = pool.spawn(async move {
                    match on {
                        On::Gpu => disk::cached(&original, (edge, factor), dir.as_deref()),
                        On::Cpu => disk::made(&original, (edge, factor), part, dir.as_deref()),
                    }
                });

                Entry::Making(task, Count::new(), on)
            };

            (entry, pass)
        });

        *last = pass;

        // A frame made ahead is uploaded within the pass's budget.
        let afford = match role {
            Role::Ahead(_) => self.budget > 0,
            Role::Picture | Role::Shown(..) => true,
        };

        if let Entry::Making(task, _, on) = entry
            && afford
            && let Some(image) = check_ready(task)
        {
            *entry = Entry::Ready(match on {
                On::Gpu => sized(ctx, gpu, way, (pixels, cut), image),
                On::Cpu => upscaled(ctx, gpu, (&original, edge), pixels, image),
            });
            self.uploads += usize::from(role.owner().is_some());

            if let Role::Ahead(_) = role {
                self.budget -= 1;
            }
        }

        // Being made: the nearest size of this cut stands in, else of
        // another (an old window's, on a resize), else none.
        let now = match entry {
            Entry::Ready(texture) => Ok(Made::Ready(texture.id())),
            Entry::Making(..) => {
                let ready = sizes.iter().filter_map(|(size, (e, _))| match e {
                    Entry::Ready(_) => Some(*size),
                    Entry::Making(..) => None,
                });

                Err(
                    stand_in(ready, pixels).and_then(|s| match &sizes.get(&s)?.0 {
                        Entry::Ready(texture) => Some(Made::Standing(texture.id(), WHOLE)),
                        Entry::Making(..) => None,
                    }),
                )
            }
        };
        let other = || self.cut_in(picture, (pixels, cut));

        match now {
            Ok(made) => Some(made),
            Err(stand) => {
                ctx.request_repaint();
                Some(stand.or_else(other).unwrap_or(Made::Making))
            }
        }
    }

    /// `key`'s sized textures noted as `owner`'s frame; on a new size
    /// (`fresh`), its others being made dropped ([`Art::restart`]).
    fn owned(&mut self, (key, owner): (Key, Owner), (fresh, pass): (bool, u64)) {
        if fresh {
            self.restart((key.0, key.1, key.2, key.3), pass);
        }

        let owners = self.entries.frames.entry(key).or_default();

        if !owners.contains(&owner) {
            owners.push(owner);
        }
    }

    /// The made size of `picture` nearest `pixels`, any cut, to stand in
    /// for cut `cut`: its part where the cut lies in it ([`seen`]).
    fn cut_in(
        &self,
        picture: (ArtSource, u64, Edge, Way),
        (pixels, cut): ([usize; 2], Rect),
    ) -> Option<Made> {
        let made: Vec<_> = self.ready(picture).collect();
        let size = stand_in(made.iter().map(|(_, t)| t.size()), pixels)?;
        let (made, texture) = made.into_iter().find(|(_, t)| t.size() == size)?;

        Some(Made::Standing(texture.id(), seen(made, cut)))
    }

    /// `picture`'s sizes still being made and not asked in `pass`
    /// dropped, as a frame's new size is asked: a window dragged to size
    /// starts each frame once, not once a pass.
    fn restart(&mut self, picture: (ArtSource, u64, Edge, Way), pass: u64) {
        let making = self
            .entries
            .sized
            .iter_mut()
            .filter(|&(&(s, h, e, w, _), _)| (s, h, e, w) == picture);

        for (_, sizes) in making {
            sizes.retain(|_, (e, last)| matches!(e, Entry::Ready(_)) || *last == pass);
        }
    }

    /// `picture`'s made textures, any cut and size, and the cut each
    /// shows.
    fn ready(
        &self,
        picture: (ArtSource, u64, Edge, Way),
    ) -> impl Iterator<Item = (Rect, &Texture)> {
        self.entries
            .sized
            .iter()
            .filter(move |&(&(s, h, e, w, _), _)| (s, h, e, w) == picture)
            .flat_map(|(&(.., cut), sizes)| {
                sizes.values().filter_map(move |(e, _)| match e {
                    Entry::Ready(texture) => Some((key_cut(cut), texture)),
                    Entry::Making(..) => None,
                })
            })
    }

    /// Textures not asked for in [`STALE`] of their source's passes
    /// dropped, once a pass: a size the window left, a page closed. The
    /// menus' wait out a game, a game's the menus. A pinned source's
    /// picture keeps what it was last drawn at, whatever is drawn
    /// meanwhile: the main menu, back from the quest map or a game, is
    /// drawn at once. A strip's frame keeps its newest made size too, to
    /// stand in on a resize, while its scene is drawn; once it is not,
    /// that alone where it is a strip's kept frame, else none.
    fn sweep(&mut self, pass: u64) {
        if self.swept != pass {
            self.swept = pass;
            self.budget = BUDGET;
            self.told(pass);
            self.trim(pass);

            let (scenes, runs) = (&self.scenes, &self.runs);
            let live = |strip: &Want| {
                runs.get(strip)
                    .and_then(|r| scenes.get(&r.scene))
                    .is_some_and(|l| l.drawn + 1 >= pass)
            };
            let kept = |(strip, i): &Owner| runs.get(strip).is_some_and(|r| r.first == *i);
            // What a frame holds: every size while a strip showing it is
            // on a scene drawn; its newest where it is a strip's kept
            // frame; else none.
            let hold = |owners: &Vec<Owner>| {
                if owners.iter().any(|(strip, _)| live(strip)) {
                    Some(Held::All)
                } else if owners.iter().any(kept) {
                    Some(Held::One)
                } else {
                    None
                }
            };
            let frames = &self.entries.frames;
            let last = |sizes: &Sizes| sizes.values().map(|&(_, last)| last).max().unwrap_or(0);
            let made = |sizes: &Sizes| {
                sizes
                    .values()
                    .filter(|(e, _)| matches!(e, Entry::Ready(_)))
                    .map(|&(_, last)| last)
                    .max()
            };

            // Each pinned picture's last pass, any cut: a size or cut the
            // window left still goes. Each frame's newest made, any cut.
            let mut newest: HashMap<(ArtSource, u64, Edge, Way), u64> = HashMap::new();
            let mut frame_newest: HashMap<(ArtSource, u64, Edge, Way), u64> = HashMap::new();

            for (key @ &(source, hash, edge, way, _), sizes) in &self.entries.sized {
                let picture = (source, hash, edge, way);
                let newer = match (frames.contains_key(key), self.pinned.kept(source)) {
                    (true, _) => made(sizes).map(|m| (frame_newest.entry(picture).or_default(), m)),
                    (false, Shelving::Pinned) => {
                        Some((newest.entry(picture).or_default(), last(sizes)))
                    }
                    (false, Shelving::Shelved) => None,
                };

                if let Some((held, at)) = newer {
                    *held = (*held).max(at);
                }
            }

            // A source's textures age in the passes it is drawn: one not
            // drawn this pass is measured from the last it was.
            for (key @ &(source, hash, edge, way, _), sizes) in &mut self.entries.sized {
                if let Some(&asked) = self.asked.get(&source) {
                    let picture = (source, hash, edge, way);

                    if let Some(owners) = frames.get(key) {
                        let hold = hold(owners);
                        let newest = frame_newest.get(&picture).copied();

                        sizes.retain(|_, (e, last)| {
                            let stands = matches!(e, Entry::Ready(_)) && newest == Some(*last);

                            match hold {
                                Some(Held::All) => *last + STALE > asked || stands,
                                Some(Held::One) => stands,
                                None => false,
                            }
                        });
                    } else {
                        let kept = newest.get(&picture).copied();

                        sizes.retain(|_, (_, last)| *last + STALE > asked || kept == Some(*last));
                    }
                }
            }

            self.entries.sized.retain(|_, sizes| !sizes.is_empty());

            let sized = &self.entries.sized;

            self.entries.frames.retain(|key, _| sized.contains_key(key));
        }
    }

    /// Each scene just left lets go of its strips' plain textures but
    /// each strip's kept frame's.
    fn trim(&mut self, pass: u64) {
        let left: Vec<Scene> = self
            .scenes
            .iter_mut()
            .filter(|(_, l)| l.held == Held::All && l.drawn + 1 < pass)
            .map(|(scene, l)| {
                l.held = Held::One;
                *scene
            })
            .collect();

        if !left.is_empty() {
            // A frame a strip still drawn shows stays.
            let shown: HashSet<Want> = self
                .runs
                .values()
                .filter(|r| {
                    self.scenes
                        .get(&r.scene)
                        .is_some_and(|l| l.held == Held::All)
                })
                .flat_map(|r| r.frames.iter().copied())
                .collect();

            for run in self.runs.values().filter(|r| left.contains(&r.scene)) {
                let gone = run
                    .frames
                    .iter()
                    .enumerate()
                    .filter(|&(i, want)| i != run.first && !shown.contains(want));

                for (_, want) in gone {
                    self.entries.plain.remove(want);
                }
            }
        }
    }

    /// Each scene whose strips drawn last pass were all made logged,
    /// once an entry: the passes and time it took, the passes that drew a
    /// frame not made, the frames uploaded. Their upload and shrink are
    /// queued for `gpu::lend` and the render world: not timed here.
    fn told(&mut self, pass: u64) {
        for (scene, life) in &mut self.scenes {
            let mut drawn = self
                .runs
                .values()
                .filter(|r| r.scene == *scene && r.drawn + 1 == pass)
                .peekable();
            let whole = drawn.peek().is_some() && drawn.all(|r| r.whole);

            if whole && !life.whole && life.drawn + 1 == pass {
                life.whole = true;
                tracing::debug!(
                    "art: {} made whole, {} passes, {:.0} ms; {} passes drew a frame not made; \
                     {} uploads",
                    scene.0,
                    pass - life.entered,
                    life.since.elapsed().as_secs_f32() * 1000.0,
                    life.plain,
                    self.uploads,
                );
            }
        }
    }

    /// What is held in memory, and the disk cache's reads and writes.
    #[must_use]
    pub fn stats(&self) -> Stats {
        let plain = self
            .entries
            .plain
            .values()
            .flatten()
            .fold([0, 0], |[n, bytes], (t, ..)| {
                [n + 1, bytes + area(t.size())]
            });
        let sized = self
            .entries
            .sized()
            .filter_map(|e| match e {
                Entry::Ready(t) => Some(t),
                Entry::Making(..) => None,
            })
            .fold([0, 0], |[n, bytes], t| [n + 1, bytes + area(t.size())]);
        let originals = self
            .entries
            .originals
            .values()
            .flatten()
            .fold([0, 0], |[n, bytes], (o, ..)| {
                [n + 1, bytes + area(o.image.size)]
            });
        let map = match &self.levels {
            Some(Map::Ready(levels)) => levels.held(),
            Some(Map::Making(..)) | None => [0, 0],
        };
        // Each picture once, whatever its tones.
        let drawn = self
            .entries
            .originals
            .iter()
            .filter(|(_, held)| matches!(held, Some((.., Origin::Override))))
            .map(|(&(source, picture, _), _)| (source, picture))
            .collect::<HashSet<_>>()
            .len();

        Stats {
            plain,
            sized,
            map,
            originals,
            disk: disk::counts(),
            overrides: [drawn, self.over.len()],
        }
    }
}

/// The quest map's levels: their table and the map's own level being
/// made, then made. Uncounted as pending work: the map's page waits on
/// the task, and no other page's picture needs it.
enum Map {
    Making(Task<levels::Levels>),
    Ready(levels::Levels),
}

/// When the quest map's levels are wanted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// The map is drawn: wait for them.
    Now,
    /// Made ahead: once made.
    Ahead,
}

/// A `size` picture's bytes, RGBA.
const fn area(size: [usize; 2]) -> usize {
    size[0] * size[1] * 4
}

/// What an [`Art`] holds: each kind's count and bytes, and the disk
/// cache's reads and writes this run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    pub plain: [usize; 2],
    pub sized: [usize; 2],
    pub map: [usize; 2],
    pub originals: [usize; 2],
    pub disk: [usize; 2],
    /// Pictures drawn from an override, and override files found.
    pub overrides: [usize; 2],
}

/// `image`, `original` at `pixels`, as a texture.
fn upscaled(
    ctx: &egui::Context,
    gpu: &mut Option<Gpu>,
    (original, edge): (&Original, Edge),
    pixels: [usize; 2],
    image: egui::ColorImage,
) -> Texture {
    let name = format!("upscaled {:016x} {pixels:?}", original.hash);

    texture(ctx, gpu, (name, edge), image)
}

/// `big`, `original` upscaled `factor` times, its part `cut` (the
/// original's pixels) shrunk to `pixels`: on the GPU where it lends a
/// texture, else here.
fn sized(
    ctx: &egui::Context,
    gpu: &mut Option<Gpu>,
    (original, edge, factor): (&Original, Edge, usize),
    (pixels, cut): ([usize; 2], Rect),
    big: ColorImage,
) -> Texture {
    let cut = disk::scaled(cut, factor);

    if shrink::kept(&big, cut, pixels) {
        upscaled(ctx, gpu, (original, edge), pixels, big)
    } else {
        let shrunk = match gpu {
            Some(gpu) => gpu.shrunk(big, cut, (pixels, edge)),
            None => Err(big),
        };

        match shrunk {
            Ok(drawn) => Texture::Gpu(drawn),
            Err(big) => {
                let image = shrink::part(&big, cut, pixels);

                upscaled(ctx, gpu, (original, edge), pixels, image)
            }
        }
    }
}

/// `image` as a texture named `name`, its edges as `edge` says: uploaded
/// to one the GPU lends (`gpu`), none of it kept in RAM; else egui's,
/// of which `bevy_egui` keeps two copies in RAM.
fn texture(
    ctx: &egui::Context,
    gpu: &mut Option<Gpu>,
    (name, edge): (String, Edge),
    image: ColorImage,
) -> Texture {
    let uploaded = match gpu {
        Some(gpu) => gpu.uploaded(image, edge),
        None => Err(image),
    };

    match uploaded {
        Ok(drawn) => Texture::Gpu(drawn),
        Err(image) => Texture::Egui(loaded(ctx, name, edge, image)),
    }
}

/// `image` as a texture named `name`, its edges as `edge` says.
fn loaded(ctx: &egui::Context, name: String, edge: Edge, image: ColorImage) -> TextureHandle {
    let options = TextureOptions {
        wrap_mode: match edge {
            Edge::Clamp => egui::TextureWrapMode::ClampToEdge,
            Edge::Repeat => egui::TextureWrapMode::Repeat,
        },
        ..TextureOptions::LINEAR
    };

    ctx.load_texture(name, image, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::Reach;
    use regent_core::view::picture_file::Key as PictureKey;
    use std::path::PathBuf;

    /// A `layout`-sized page centred in `window`, as large as fits: its
    /// scale and origin (the menus' `Fit::of`).
    fn page(window: Rect, layout: Vec2) -> (f32, egui::Pos2) {
        let scale = (window.width() / layout.x).min(window.height() / layout.y);

        (scale, window.center() - layout * scale / 2.0)
    }

    /// The nearest size held by area stands in; on a tie the larger.
    #[test]
    fn the_nearest_size_held_stands_in() {
        assert_eq!(
            stand_in([[100, 100], [300, 300]], [200, 200]),
            Some([300, 300])
        );
        assert_eq!(
            stand_in([[100, 100], [400, 400]], [200, 200]),
            Some([400, 400])
        );
        assert_eq!(
            stand_in([[150, 150], [400, 400]], [200, 200]),
            Some([150, 150])
        );
        assert_eq!(stand_in([], [200, 200]), None);
    }

    /// A room object's resting frame, the room's own pixels, upscaled
    /// over the room meets it pixel for pixel at any size: the main
    /// menu's seams.
    #[test]
    fn a_room_objects_frame_meets_the_room_pixel_for_pixel() {
        use bevy_egui::egui::{pos2, vec2};

        let (room, at, side) = ([48, 36], [13, 9], [17, 11]);
        let mut seed = 7_u32;
        let pixels = (0..room[0] * room[1])
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let [r, g, b, _] = seed.to_le_bytes();

                egui::Color32::from_rgb(r, g, b)
            })
            .collect();
        let room_image = egui::ColorImage::new(room, pixels);
        let frame_image = scale::cut(&room_image, at, side);
        let (room_image, frame_image) = (Original::new(room_image), Original::new(frame_image));
        #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
        let layout = |at: [usize; 2], size: [usize; 2]| {
            Rect::from_min_size(
                pos2(at[0] as f32, at[1] as f32),
                vec2(size[0] as f32, size[1] as f32),
            )
        };
        let under = layout([0, 0], room);
        // As the service makes it (`Art::upscaled`).
        let made = |original: &Original, laid: Laid| {
            let cut = laid.cut.unwrap_or(shrink::whole(&original.image));
            #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
            let shown = vec2(laid.pixels[0] as f32, laid.pixels[1] as f32) / cut.size();
            let factor = scale::factor(original.image.size, shown).expect("upscaled");

            disk::made(original, (Edge::Clamp, factor), (laid.pixels, cut), None)
        };

        for (window, ppp) in [
            (vec2(101.0, 77.0), 1.0),
            (vec2(130.3, 91.0), 1.25),
            (vec2(89.0, 64.6), 1.698),
            (vec2(171.0, 129.0), 1.0),
            (vec2(97.0, 75.0), 1.9),
        ] {
            let (scale, origin) = page(Rect::from_min_size(pos2(0.0, 0.0), window), under.size());
            let room_place = Place {
                scale,
                origin,
                rect: under,
                snap: Snap::Edges,
            };
            let frame_place = Place {
                scale,
                origin,
                rect: layout(at, side),
                snap: Snap::Over(under),
            };
            #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
            let (room, frame) = (
                room_place.laid(ppp, 0.0),
                frame_place.laid(ppp, scale::REACH as f32),
            );
            let (room_pixels, room_rect, frame_pixels, frame_rect) =
                (room.pixels, room.rect, frame.pixels, frame.rect);
            let (room_shown, frame_shown) = (made(&room_image, room), made(&frame_image, frame));
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "whole pixels"
            )]
            let offset = ((frame_rect.min - room_rect.min) * ppp).round();
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "whole pixels"
            )]
            let offset = [offset.x as usize, offset.y as usize];
            let off = (0..frame_pixels[1])
                .flat_map(|y| (0..frame_pixels[0]).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let a = frame_shown.pixels[y * frame_pixels[0] + x].to_array();
                    let b = room_shown.pixels[(offset[1] + y) * room_pixels[0] + offset[0] + x]
                        .to_array();

                    (0..4).any(|c| a[c].abs_diff(b[c]) > 1)
                })
                .count();

            assert_eq!(off, 0, "{window:?} at {ppp}: of {frame_pixels:?}");
        }
    }

    /// Drawn plain, a room object's frame is sampled where the room is,
    /// pixel for pixel, and never within half a pixel of its edge, where
    /// linear sampling reads it clamped.
    #[test]
    fn a_plain_frame_samples_the_room_where_the_room_does() {
        use bevy_egui::egui::{pos2, vec2};

        let under = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let frame = Rect::from_min_size(pos2(301.0, 197.0), vec2(83.0, 41.0));

        for (window, ppp) in [
            (vec2(1517.0, 913.0), 1.698),
            (vec2(1366.0, 768.0), 1.0),
            (vec2(1920.0, 1200.0), 1.25),
            (vec2(1000.0, 700.0), 2.0),
        ] {
            let (scale, origin) = page(
                Rect::from_min_size(pos2(0.0, 0.0), window / ppp),
                under.size(),
            );
            let place = |rect, snap| Place {
                scale,
                origin,
                rect,
                snap,
            };
            let room = place(under, Snap::Edges).laid(ppp, 0.0);
            let laid = place(frame, Snap::Over(under)).laid(ppp, LINEAR_REACH);
            let cut = laid.cut.expect("cut");

            for axis in 0..2 {
                #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
                let (room_side, side) = (room.pixels[axis] as f32, laid.pixels[axis] as f32);

                for i in 0..laid.pixels[axis] {
                    #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
                    let centre = laid.rect.min[axis] * ppp + i as f32 + 0.5;
                    let along = (centre - laid.rect.min[axis] * ppp) / side;
                    let texel = cut.min[axis] + along * cut.size()[axis];
                    let seen =
                        (centre - room.rect.min[axis] * ppp) / room_side * under.size()[axis];

                    assert!(
                        (frame.min[axis] + texel - seen).abs() < 1e-3,
                        "{window:?} at {ppp}, axis {axis} pixel {i}"
                    );
                    assert!(
                        (0.5..=frame.size()[axis] - 0.5).contains(&texel),
                        "{window:?} at {ppp}: {texel} clamped"
                    );
                }
            }
        }
    }

    /// Set 1002's first frame is a list icon, drawn as the portrait; any
    /// other slot, frame, set or a tiled one is itself.
    #[test]
    fn only_a_list_icon_is_drawn_as_its_portrait() {
        let code = FourCc(*b"AVL1");
        let source = ArtSource::of(&[]);
        let want = |pic: Pic| Want::of(source, Picture::Pic(pic));
        let icon_set = i32::from(INTERFACE_ICON);

        assert_eq!(
            icon(want(Pic::of(code, icon_set)).tone(Tone::Grave)),
            Some(Want::of(source, Picture::Icon(code)).tone(Tone::Grave))
        );
        assert_eq!(icon(want(Pic::of(code, icon_set).slot(1))), None);
        assert_eq!(icon(want(Pic::of(code, icon_set).frame(1))), None);
        assert_eq!(icon(want(Pic::of(code, 1000))), None);
        assert_eq!(icon(want(Pic::of(code, icon_set)).wrap(Wrap::Tiled)), None);
    }

    /// A reader of one source, its pictures `width` pixels wide; how
    /// often it read.
    struct Reader {
        source: ArtSource,
        width: usize,
        reach: Reach,
        reads: std::cell::Cell<usize>,
    }

    impl Reader {
        fn new(path: &str, width: usize, reach: Reach) -> Reader {
            Reader {
                source: ArtSource::of(&[PathBuf::from(path)]),
                width,
                reach,
                reads: std::cell::Cell::new(0),
            }
        }

        fn want(&self) -> Want {
            Want::of(self.source, Picture::Pic(Pic::of(FourCc(*b"AP29"), 1)))
        }
    }

    impl Read for Reader {
        fn read(&self, source: ArtSource, _: Picture) -> Option<egui::ColorImage> {
            self.reads.set(self.reads.get() + 1);

            (source == self.source)
                .then(|| egui::ColorImage::filled([self.width, 1], egui::Color32::WHITE))
        }

        fn reach(&self) -> Reach {
            self.reach
        }

        fn keys(&self, source: ArtSource, _: Picture) -> Vec<PictureKey> {
            if source == self.source {
                vec![key()]
            } else {
                Vec::new()
            }
        }
    }

    /// The override name of [`Reader::want`]'s picture.
    fn key() -> PictureKey {
        PictureKey::frame("Data/interfacedata.cam", FourCc(*b"AP29"), 1, 0, 0)
    }

    /// An override folder holding `image` for [`Reader::want`]'s picture.
    fn folder(name: &str, image: &ColorImage) -> PathBuf {
        let dir = over::tests::scratch(name);

        over::tests::write(&dir.join(format!("{}.png", key().name())), image);
        dir
    }

    fn red(size: [usize; 2]) -> ColorImage {
        ColorImage::filled(size, egui::Color32::RED)
    }

    /// The layout rectangle of a 4 × 1 picture at `scale` points a pixel.
    fn place(scale: f32) -> Place {
        Place {
            scale,
            origin: egui::Pos2::ZERO,
            rect: Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(4.0, 1.0)),
            snap: Snap::Corner,
        }
    }

    /// The ways the sized textures held were made, by their pixels' hash.
    fn ways(art: &Art) -> Vec<(u64, Way)> {
        art.entries.sized.keys().map(|k| (k.1, k.3)).collect()
    }

    /// The pixels the override folder's file names (`picture_file`): the
    /// name `ui-dump` writes for a set's frame and a portrait is the key
    /// the game's catalogue gives the picture, and reads back to it.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_dumps_names_are_the_pictures_keys() {
        use regent_core::content::dataset::{Bytecode, Datasets, Install};

        let paths = Install::new(&regent_corpus::hd(), Bytecode::Install)
            .files(Datasets::Expansion)
            .expect("the expansion's files")
            .archives;
        let source = ArtSource::of(&paths);
        let reader = (source, regent_corpus::expansion());
        let cases = [
            (
                Picture::Pic(Pic::of(FourCc(*b"DEM2"), 1006).slot(6)),
                "interfacedata/DEM2_1006_s6_f0.png",
            ),
            (
                Picture::Pic(Pic::Portrait(FourCc(*b"ABJ1"))),
                "portraits/maindata/ABJ1_1000_s0_f0.png",
            ),
            (
                Picture::Pic(Pic::of(FourCc(*b"IX10"), 1001)),
                "mx_interfacedata/IX10_1001_s0_f0.png",
            ),
        ];

        for (picture, name) in cases {
            let keys = reader.keys(source, picture);

            assert_eq!(
                keys,
                PictureKey::parse(name).into_iter().collect::<Vec<_>>(),
                "{name}"
            );
            assert!(name.ends_with(&format!("{}.png", keys[0].name())), "{name}");
        }
    }

    /// An override replaces its picture for every want of it, tones
    /// applied to its pixels; its size stays the original's. The stats
    /// count it.
    #[test]
    fn an_override_wins_over_the_archive() {
        let ctx = egui::Context::default();
        let reader = Reader::new("Data", 4, Reach::Whole);
        let mut art = Art::default();

        art.rescan_at(Some(&folder("art-over-wins", &red([4, 1]))));

        let dim = reader.want().tone(Tone::Dim);
        let original = art.original(&reader, dim).expect("an original");

        assert_eq!(
            art.size(&ctx, &reader, reader.want()),
            Some(Vec2::new(4.0, 1.0))
        );
        assert_eq!(
            original.image.pixels[0],
            look::toned(red([1, 1]), Tone::Dim).pixels[0]
        );
        assert_eq!(art.stats().overrides, [1, 1]);
    }

    /// An override's file is decoded once: its plain texture is made from
    /// the original held, so a file gone after the original was read is
    /// still drawn.
    #[test]
    fn an_override_is_decoded_once() {
        let ctx = egui::Context::default();
        let reader = Reader::new("Data", 4, Reach::Whole);
        let dir = folder("art-over-once", &red([4, 1]));
        let mut art = Art::default();

        art.rescan_at(Some(&dir));
        art.original(&reader, reader.want()).expect("an original");
        std::fs::remove_dir_all(&dir).expect("removed");

        assert_eq!(
            art.size(&ctx, &reader, reader.want()),
            Some(Vec2::new(4.0, 1.0))
        );
        assert_eq!(art.stats().overrides, [1, 1]);
    }

    /// An override drawn smaller than its own pixels is drawn shrunk
    /// alone: no texture at its own size is uploaded.
    #[test]
    fn a_shrunk_override_uploads_no_plain_texture() {
        let ctx = egui::Context::default();
        let reader = Reader::new("Data", 4, Reach::Whole);
        let mut art = Art::default();

        art.rescan_at(Some(&folder("art-over-no-plain", &red([16, 4]))));

        let shown = art.get(&ctx, &reader, reader.want(), place(1.0));

        assert!(matches!(shown, Some(Answer::Ready(_))));
        assert_eq!(art.stats().plain[0], 0);
        assert_eq!(art.stats().overrides, [1, 1]);
        assert_eq!(
            art.size(&ctx, &reader, reader.want()),
            Some(Vec2::new(4.0, 1.0))
        );
        assert_eq!(art.stats().plain[0], 0);
    }

    /// A 4× override keeps the original's size and rectangle: at the
    /// original's pixels it is shrunk to them, at 4× drawn as it is.
    #[test]
    fn a_larger_override_is_drawn_into_the_original_rect() {
        let ctx = egui::Context::default();
        let reader = Reader::new("Data", 4, Reach::Whole);
        let mut art = Art::default();

        art.rescan_at(Some(&folder("art-over-larger", &red([16, 4]))));

        let (plain, size) = art.plain(&ctx, &reader, reader.want()).expect("plain");
        let small = art.get(&ctx, &reader, reader.want(), place(1.0));
        let large = art.get(&ctx, &reader, reader.want(), place(4.0));
        let shrunk = art.entries.sized.values().flat_map(HashMap::keys);

        assert_eq!(size, Vec2::new(4.0, 1.0));
        assert!(matches!(small, Some(Answer::Ready(s)) if s.id != plain
            && s.rect == place(1.0).window(place(1.0).rect)));
        assert_eq!(shrunk.collect::<Vec<_>>(), [&[4, 1]]);
        assert_eq!(
            large,
            Some(Answer::Ready(Shown {
                id: plain,
                rect: Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(16.0, 4.0)),
                uv: WHOLE,
            }))
        );
    }

    /// A picture laid over another is cut by its share: on a 4× override
    /// the same part, four times the pixels.
    #[test]
    fn an_override_is_cut_where_the_original_is() {
        let share = Rect::from_min_max(egui::pos2(0.25, 0.0), egui::pos2(0.75, 1.0));
        let cut = |size| own(&Original::new(red(size)), share);

        assert_eq!(
            cut([4, 1]),
            Rect::from_min_max(egui::pos2(1.0, 0.0), egui::pos2(3.0, 1.0))
        );
        assert_eq!(
            cut([16, 4]),
            Rect::from_min_max(egui::pos2(4.0, 0.0), egui::pos2(12.0, 4.0))
        );
    }

    /// With xBRZ on, an override smaller than its place is upscaled as
    /// the original would be, keyed by its own pixels; one larger is only
    /// shrunk.
    #[test]
    fn an_override_takes_the_upscaling_policy() {
        let ctx = egui::Context::default();
        let reader = Reader::new("Data", 4, Reach::Whole);
        let mut small = red([4, 1]);

        small.pixels[1] = egui::Color32::BLUE;

        let large = red([16, 4]);

        for (name, image, way) in [
            ("art-over-xbrz-small", &small, Way::Upscaled),
            ("art-over-xbrz-large", &large, Way::Shrunk),
        ] {
            let mut art = Art::default();

            art.set(Upscale::Xbrz);
            art.rescan_at(Some(&folder(name, image)));

            for _ in 0..1000 {
                if let Some(Answer::Ready(_)) = art.get(&ctx, &reader, reader.want(), place(2.0)) {
                    break;
                }

                std::thread::sleep(std::time::Duration::from_millis(5));
            }

            assert_eq!(ways(&art), [(disk::hash(image), way)], "{name}");
        }
    }

    /// A file that is no PNG is left out: the archive's picture is drawn.
    #[test]
    fn a_bad_override_is_left_out() {
        let ctx = egui::Context::default();
        let reader = Reader::new("Data", 4, Reach::Whole);
        let dir = over::tests::scratch("art-over-bad");
        let path = dir.join(format!("{}.png", key().name()));
        let mut art = Art::default();

        std::fs::create_dir_all(path.parent().expect("a folder")).expect("a folder");
        std::fs::write(&path, b"not a png").expect("written");
        art.rescan_at(Some(&dir));

        assert_eq!(
            art.size(&ctx, &reader, reader.want()),
            Some(Vec2::new(4.0, 1.0))
        );
        assert_eq!(
            art.original(&reader, reader.want())
                .expect("an original")
                .image
                .pixels[0],
            egui::Color32::WHITE
        );
        assert_eq!(art.stats().overrides, [0, 1]);
    }

    /// A changed file is drawn after a rescan, not before; pictures
    /// shelved under the old file are not given back.
    #[test]
    fn a_changed_override_is_picked_up_after_a_rescan() {
        let ctx = egui::Context::default();
        let reader = Reader::new("Data", 4, Reach::Whole);
        let dir = folder("art-over-changed", &red([4, 1]));
        let mut art = Art::default().overridden(Some(dir.clone()));
        let first = |art: &mut Art| {
            art.original(&reader, reader.want())
                .expect("drawn")
                .image
                .pixels[0]
        };

        art.rescan();
        art.size(&ctx, &reader, reader.want());
        assert_eq!(first(&mut art), egui::Color32::RED);

        over::tests::write(
            &dir.join(format!("{}.png", key().name())),
            &ColorImage::filled([8, 2], egui::Color32::BLUE),
        );
        art.size(&ctx, &reader, reader.want());
        assert_eq!(first(&mut art), egui::Color32::RED, "before the rescan");

        let shelved = art.take(reader.source);

        art.rescan();
        art.put(shelved);
        assert_eq!(first(&mut art), egui::Color32::BLUE);
        assert_eq!(
            art.size(&ctx, &reader, reader.want()),
            Some(Vec2::new(4.0, 1.0))
        );
        // Read once a scan: the plain texture is made from the original.
        assert_eq!(reader.reads.get(), 2);
    }

    /// One id from two sources is two pictures: base and expansion
    /// `AP29` differ.
    #[test]
    fn a_picture_is_keyed_by_its_source() {
        let ctx = egui::Context::default();
        let mut art = Art::default();
        let base = Reader::new("Data", 1, Reach::Whole);
        let expansion = Reader::new("DataMX", 2, Reach::Whole);

        assert_ne!(base.source, expansion.source);
        assert_eq!(
            art.size(&ctx, &base, base.want()),
            Some(Vec2::new(1.0, 1.0))
        );
        assert_eq!(
            art.size(&ctx, &expansion, expansion.want()),
            Some(Vec2::new(2.0, 1.0))
        );
        assert_eq!(art.stats().plain[0], 2);
    }

    /// Taking a game's source leaves the menus'; given back, it is not
    /// read again. A pinned source, the menus', is not taken.
    #[test]
    fn taking_a_source_keeps_the_others() {
        let ctx = egui::Context::default();
        let menus = Reader::new("Data", 1, Reach::Part);
        let game = Reader::new("Quest", 2, Reach::Whole);
        let mut art = Art::pinned([menus.source]);

        art.size(&ctx, &menus, menus.want());
        art.size(&ctx, &game, game.want());

        let shelved = art.take(game.source);

        assert_eq!(art.pinned.kept(game.source), Shelving::Shelved);
        assert_eq!(art.stats().plain[0], 1);
        assert_eq!(art.take(menus.source).plain.len(), 0);

        art.size(&ctx, &menus, menus.want());
        assert_eq!(menus.reads.get(), 1);

        art.put(shelved);
        art.size(&ctx, &game, game.want());
        assert_eq!(game.reads.get(), 1);
        assert_eq!(art.stats().plain[0], 2);
    }

    /// The same picture from one source, asked by the menus and the
    /// panels through their own readers, is read and held once. A miss
    /// by a reader holding part of a source is not remembered.
    #[test]
    fn one_source_is_held_once_for_every_reader() {
        let ctx = egui::Context::default();
        let mut art = Art::default();
        let menus = Reader::new("Data", 1, Reach::Part);
        let panels = Reader::new("Data", 1, Reach::Whole);
        let other = Reader::new("Elsewhere", 1, Reach::Part);
        let want = menus.want();

        assert_eq!(art.size(&ctx, &other, want), None);
        assert_eq!(art.size(&ctx, &menus, want), Some(Vec2::new(1.0, 1.0)));
        assert_eq!(art.size(&ctx, &panels, want), Some(Vec2::new(1.0, 1.0)));
        assert_eq!(panels.reads.get(), 0);
        assert_eq!(art.stats().plain[0], 1);
    }

    /// A strip's frames, each its own pixels: frame `n` `n` grey.
    struct Frames(ArtSource);

    impl Read for Frames {
        fn read(&self, _: ArtSource, picture: Picture) -> Option<egui::ColorImage> {
            match picture {
                Picture::Pic(Pic::Set { frame, .. }) => {
                    let grey = u8::try_from(frame).ok()?;

                    Some(egui::ColorImage::filled(
                        [4, 1],
                        egui::Color32::from_gray(grey),
                    ))
                }
                _ => None,
            }
        }

        fn reach(&self) -> Reach {
            Reach::Whole
        }
    }

    /// Two scenes: a page's running strips, another's.
    const MENU: Scene = Scene("menu");
    const MAP: Scene = Scene("map");

    /// One pass of `art` drawing frame `shown` of `frames` on `scene`: its
    /// answer.
    fn stepped(
        art: &mut Art,
        ctx: &egui::Context,
        read: &Frames,
        (frames, shown): (&[Want], usize),
        scene: Scene,
    ) -> Option<Answer> {
        stepped_at(art, ctx, read, (frames, shown), (scene, 2.0))
    }

    /// The same at `scale`.
    fn stepped_at(
        art: &mut Art,
        ctx: &egui::Context,
        read: &Frames,
        (frames, shown): (&[Want], usize),
        (scene, scale): (Scene, f32),
    ) -> Option<Answer> {
        let place = Place {
            scale,
            origin: Pos2::ZERO,
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
            snap: Snap::Edges,
        };

        ctx.begin_pass(egui::RawInput::default());

        let answer = art.strip(ctx, read, frames, shown, (place, scene));

        ctx.end_pass().textures_delta.clear();
        answer
    }

    /// Frame `shown` of `frames` drawn on `scene` till every frame is made
    /// (their tasks run in the pool).
    fn settled(
        art: &mut Art,
        ctx: &egui::Context,
        read: &Frames,
        (frames, shown): (&[Want], usize),
        scene: Scene,
    ) {
        let mut last = None;

        for _ in 0..1000 {
            last = stepped(art, ctx, read, (frames, shown), scene);

            if let Some(Answer::Ready(_)) = last
                && frames.iter().all(|&f| made(art, read, f))
            {
                return;
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        panic!("frame {shown} not made: {last:?}");
    }

    /// `n` frames from `from` of `read`, upscaled.
    fn strip_of(read: &Frames, from: usize, n: usize) -> (Art, Vec<Want>) {
        let mut art = Art::pinned([read.0]);

        art.set(Upscale::Xbrz);

        (art, (from..from + n).map(|f| frame(read, f)).collect())
    }

    /// While its scene is drawn a strip holds every frame, one size each,
    /// however long it runs: none is made twice.
    #[test]
    fn a_drawn_scene_holds_every_frame_of_its_strips() {
        let ctx = egui::Context::default();
        let read = Frames(ArtSource::of(&[PathBuf::from("Data")]));
        let (mut art, frames) = strip_of(&read, 0, 8);

        settled(&mut art, &ctx, &read, (&frames, 0), MENU);

        for pass in 0..3 * STALE {
            #[allow(clippy::cast_possible_truncation, reason = "a few passes")]
            let shown = pass as usize % 8;

            stepped(&mut art, &ctx, &read, (&frames, shown), MENU);
        }

        let held: Vec<usize> = frames.iter().map(|&f| held(&art, &read, f)).collect();

        assert_eq!(held, [1; 8]);
        assert!(frames.iter().all(|&f| made(&art, &read, f)));
    }

    /// A scene not drawn keeps each strip's frame shown first on entering
    /// it, sized and plain, and lets go of the rest; drawn again, that
    /// frame is drawn at once.
    #[test]
    fn a_scene_left_keeps_each_strips_first_frame() {
        let ctx = egui::Context::default();
        let read = Frames(ArtSource::of(&[PathBuf::from("Data")]));
        let (mut art, menu) = strip_of(&read, 0, 8);
        let map: Vec<Want> = (20..24).map(|f| frame(&read, f)).collect();

        // Entered on frame 3, then run.
        settled(&mut art, &ctx, &read, (&menu, 3), MENU);

        for shown in [4, 5, 6, 7, 0, 1] {
            stepped(&mut art, &ctx, &read, (&menu, shown), MENU);
        }

        for _ in 0..3 {
            stepped(&mut art, &ctx, &read, (&map, 0), MAP);
        }

        let held: Vec<usize> = menu.iter().map(|&f| held(&art, &read, f)).collect();
        let plain: Vec<usize> = (0..8)
            .filter(|&i| art.entries.plain.contains_key(&menu[i]))
            .collect();

        assert_eq!(held, [0, 0, 0, 1, 0, 0, 0, 0]);
        assert_eq!(plain, [3]);
        assert!(matches!(
            stepped(&mut art, &ctx, &read, (&menu, 3), MENU),
            Some(Answer::Ready(_))
        ));
    }

    /// A frame not made by its turn shows its own plain picture: the
    /// animation runs on.
    #[test]
    fn a_frame_not_made_by_its_turn_shows_its_plain_picture() {
        let ctx = egui::Context::default();
        let read = Frames(ArtSource::of(&[PathBuf::from("Data")]));
        let (mut art, frames) = strip_of(&read, 0, 8);

        settled(&mut art, &ctx, &read, (&frames, 0), MENU);

        let next = Original::new(read.read(read.0, frames[1].picture).expect("a frame")).hash;
        let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);

        // Frame 1 still being made, as by a slow task.
        for (_, sizes) in art.entries.sized.iter_mut().filter(|(k, _)| k.1 == next) {
            for (entry, _) in sizes.values_mut() {
                *entry = Entry::Making(pool.spawn(std::future::pending()), Count::new(), On::Cpu);
            }
        }

        let answer = stepped(&mut art, &ctx, &read, (&frames, 1), MENU);
        let plain = art.entries.plain[&frames[1]].as_ref().map(|(t, ..)| t.id());

        assert!(
            matches!(answer, Some(Answer::Pending(at)) if Some(at.id) == plain),
            "{answer:?}"
        );
    }

    /// Frames made ahead are read and uploaded at most [`BUDGET`] a pass,
    /// the frame drawn besides, till all are made.
    #[test]
    fn frames_made_ahead_keep_to_the_budget() {
        let ctx = egui::Context::default();
        let read = Frames(ArtSource::of(&[PathBuf::from("Data")]));
        let (mut art, frames) = strip_of(&read, 0, 24);
        let count = |art: &Art| {
            let made = frames.iter().filter(|&&f| made(art, &read, f)).count();

            (art.entries.originals.len(), made)
        };
        let mut was = count(&art);

        for _ in 0..1000 {
            // Every task done by the pass.
            std::thread::sleep(std::time::Duration::from_millis(5));
            stepped(&mut art, &ctx, &read, (&frames, 0), MENU);

            let now = count(&art);

            assert!(now.0 <= was.0 + BUDGET + 1, "read {was:?} → {now:?}");
            assert!(now.1 <= was.1 + BUDGET + 1, "uploaded {was:?} → {now:?}");
            was = now;

            if now.1 == frames.len() {
                break;
            }
        }

        assert_eq!(was, (frames.len(), frames.len()));
    }

    /// A window dragged to size starts each frame once: a size still
    /// being made goes as the next is asked.
    #[test]
    fn a_dragged_window_starts_each_frame_once() {
        let ctx = egui::Context::default();
        let read = Frames(ArtSource::of(&[PathBuf::from("Data")]));
        let (mut art, frames) = strip_of(&read, 0, 8);

        settled(&mut art, &ctx, &read, (&frames, 0), MENU);

        for step in 1..20_u8 {
            let scale = 2.0 + f32::from(step) / 4.0;

            stepped_at(&mut art, &ctx, &read, (&frames, 0), (MENU, scale));
        }

        for &f in &frames {
            let making = sized_of(&art, &read, f)
                .filter(|e| matches!(e, Entry::Making(..)))
                .count();

            assert!(making <= 1, "{f:?}: {making} being made");
            assert!(made(&art, &read, f), "{f:?}: none stands in");
        }
    }

    /// A frame cut for another window stands in on a resize, its part
    /// shown where the new cut lies in it.
    #[test]
    fn an_old_cut_shows_where_the_new_one_lies() {
        let made = Rect::from_min_max(pos2(10.0, 0.0), pos2(20.0, 10.0));
        let wanted = Rect::from_min_max(pos2(12.0, 0.0), pos2(22.0, 5.0));

        assert_eq!(
            seen(made, wanted),
            Rect::from_min_max(pos2(0.2, 0.0), pos2(1.2, 0.5))
        );
        assert_eq!(seen(made, made), WHOLE);
        assert_eq!(key_cut(cut_key(wanted)), wanted);
    }

    /// `read`'s frame `frame`.
    fn frame(read: &Frames, frame: usize) -> Want {
        Want::of(
            read.0,
            Picture::Pic(Pic::of(FourCc(*b"AP29"), 1).frame(frame)),
        )
    }

    /// `art`'s sized textures of `want`, from `read`, any cut and size.
    fn sized_of<'a>(art: &'a Art, read: &Frames, want: Want) -> impl Iterator<Item = &'a Entry> {
        let image = read.read(want.source, want.picture).expect("a frame");
        let hash = Original::new(look::toned(image, want.tone)).hash;

        art.entries
            .sized
            .iter()
            .filter(move |(k, _)| k.1 == hash)
            .flat_map(|(_, sizes)| sizes.values().map(|(e, _)| e))
    }

    /// The sizes `art` holds of `want`.
    fn held(art: &Art, read: &Frames, want: Want) -> usize {
        sized_of(art, read, want).count()
    }

    /// Whether `art` holds `want` made at some size.
    fn made(art: &Art, read: &Frames, want: Want) -> bool {
        sized_of(art, read, want).any(|e| matches!(e, Entry::Ready(_)))
    }

    /// One pass of `art` drawing `want` at `scale`.
    fn drawn(art: &mut Art, ctx: &egui::Context, read: &Frames, (want, scale): (Want, f32)) {
        let size = art.size(ctx, read, want).expect("a frame");
        let place = Place {
            scale,
            origin: Pos2::ZERO,
            rect: Rect::from_min_size(Pos2::ZERO, size),
            snap: Snap::Edges,
        };

        ctx.begin_pass(egui::RawInput::default());
        art.get(ctx, read, want, place);
        ctx.end_pass().textures_delta.clear();
    }

    /// A pinned source's picture, the main menu's, is kept at the size
    /// last drawn however long others are drawn: the quest map, a game.
    /// Another source's goes after [`STALE`] passes; so does a size the
    /// window left.
    #[test]
    fn a_pinned_pictures_last_size_is_kept_while_away() {
        let ctx = egui::Context::default();
        let read = Frames(ArtSource::of(&[PathBuf::from("Data")]));
        let (menu, map) = (frame(&read, 0), frame(&read, 1));
        let away = |art: &mut Art| {
            art.set(Upscale::Xbrz);
            drawn(art, &ctx, &read, (menu, 2.0));
            drawn(art, &ctx, &read, (menu, 3.0));

            for _ in 0..2 * STALE {
                drawn(art, &ctx, &read, (map, 2.0));
            }
        };
        let mut pinned = Art::pinned([read.0]);
        let mut shelved = Art::default();

        away(&mut pinned);
        away(&mut shelved);

        assert_eq!(held(&pinned, &read, menu), 1, "the last size kept");
        assert_eq!(held(&shelved, &read, menu), 0, "not pinned");
        assert_eq!(held(&pinned, &read, map), 1);
    }
}
