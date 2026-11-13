//! A kind's sheet: the frames of one `IMAG` record packed into pages of
//! palette indices ([`regent_core::view::pack`]), a set's pages made when
//! a frame of it first draws, with a `feet` page beside each that casts.
//! A frame's page rectangle goes in each of its instances' rows
//! ([`crate::instance`]); every frame draws on the unit quad.
//!
//! ```text
//!  IMAG record ──each world set: every slot, layer, pass, stream──> tiles
//!     a set first drawn ──its tiles not yet on a page──> decoded once,
//!        packed with a gutter of 255 into pages (more where one passes SIDE)
//!     a frame ──> page, rectangle (its instances' rows: body.wesl and
//!                 band.wesl read them)
//!     materials made on a page, shared by its frames:
//!        Body  page × palette × pass (glass: × mode and level)
//!        Band  page × ramps
//! ```
//!
//! A frame draws from its page exactly the texels its own texture held:
//! the shaders clamp to the rectangle and offset into the page, so the
//! picture is unchanged. The quad keeps its size and place.
//!
//! A set's pages are asked for: decoded, packed and built on the compute
//! pool, at most [`JOBS`] at once of every source's, and put on the sheet
//! as they land ([`Jobs::landed`], [`Sheet::put`]). Its frames are
//! [`Paged::Pending`] until then. Nothing makes them on the main thread.
//! A set something draws ([`Need::Drawn`]) goes before one asked early
//! ([`Need::Early`]), and one asked early is drawn's once drawn.
//!
//! ```text
//!  cel, or ask ──unlaid set──> asked: Jobs queued ──landed──> running (compute pool)
//!  Jobs::landed ──finished──> Sheet::put: images added, frames and looks kept ──> Ready
//! ```

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{ComputeTaskPool, Task, TaskPool};

use regent_core::content::imagery::{self, FrameSource};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::{Blit, SetKind, set_number};
use regent_core::data::payload::Palette;
use regent_core::data::tile::Mode;
use regent_core::view::pack::{self, Packed, Rect};
use regent_core::view::raster::blend;
use regent_core::view::raster::shade::TRANSPARENT;
use regent_core::view::{Ink, PaletteSource, Sprite as Frame};

use super::cel::{Cel, Colours, Palettes, Picture, Store};
use crate::data::Corpus;
use crate::material::{self, Band, Body};
use crate::smooth::Sampling;
use crate::soft::{self, Ramps};
use crate::texture;
use crate::wind::{Gust, Sway};

/// Texels of index 255 between frames: the sharp-sprite path reads a
/// neighbour one texel off a frame's edge.
pub const GUTTER: u32 = 1;

/// A page's largest side: wgpu's default limit is 8192; this keeps
/// to what weaker GPUs take (**Inferred**: Intel UHD 600 takes 8192).
pub const SIDE: u32 = 4096;

/// Archive and index of a `TILE`.
pub type Tile = (usize, usize);

/// Page jobs running at once, every source's: each takes a compute pool
/// thread, and a 2-core machine's pool also makes the ground's patches.
pub const JOBS: usize = 2;

/// Page jobs running now, every [`Jobs`]'.
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// Why a set's pages are asked for: something draws them, or something
/// may soon ([`crate::art::Sprites::ask`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Drawn,
    Early,
}

/// One page: its indices, and each pixel's rows to its run's foot
/// ([`crate::lie::feet`]) at the same places.
#[derive(Clone, Debug)]
pub struct Page {
    pub indices: Handle<Image>,
    pub feet: Handle<Image>,
}

/// Where a frame is on its sheet.
#[derive(Clone, Debug)]
pub struct Framed {
    pub page: usize,
    pub rect: Rect,
}

/// What a sheet's body material draws: palette, pass, and a translucent
/// frame's mode and level.
type Shown = (Colours, material::Pass, Option<(Mode, u16)>);

/// One `IMAG` record's frames, packed a set at a time.
#[derive(Debug, Default)]
pub struct Sheet {
    /// Its `IMAG` record.
    record: FourCc,
    /// Each world set's tiles not yet laid nor asked for (low 16 bits of
    /// its id: every layer).
    sets: HashMap<u16, BTreeSet<Tile>>,
    /// Each set asked for and not yet put, with its tiles.
    asked: HashMap<u16, BTreeSet<Tile>>,
    pub pages: Vec<Page>,
    frames: HashMap<Tile, Framed>,
    /// What each frame on a page draws with, kept from its one decode.
    looks: HashMap<Tile, Look>,
    bodies: HashMap<(usize, Shown), Handle<Body>>,
    bands: HashMap<(usize, Ramps), Handle<Band>>,
}

/// A frame's look: what `cel::picture` reads of its tile beside the
/// pixels.
#[derive(Debug)]
struct Look {
    size: Vec2,
    anchor: Vec2,
    mode: Mode,
    level: u16,
    colours: Colours,
    casts: bool,
    palette: Box<Palette>,
}

/// One tile decoded for a sheet: its indices as [`texture::indices`]
/// makes them, its feet where it casts a band, and its look.
struct Decoded {
    tile: Tile,
    width: u32,
    height: u32,
    indices: Vec<u8>,
    feet: Option<Vec<u16>>,
    look: Option<Look>,
}

/// One page built: its indices, its feet where a frame casts, and its
/// frames' rectangles.
type Built = (Image, Option<Image>, Vec<(Tile, Rect)>);

/// A set's pages, made off the main thread: each page's images and
/// members' rectangles, and each frame's look.
pub struct Laid {
    pages: Vec<Built>,
    looks: Vec<(Tile, Look)>,
}

/// A frame's materials, or that its set's pages are on their way.
#[derive(Clone)]
pub enum Paged {
    Ready(Option<Cel>),
    Pending,
}

impl Paged {
    /// The materials; `None` off the sheet or pending.
    #[must_use]
    pub fn ready(self) -> Option<Cel> {
        match self {
            Paged::Ready(cel) => cel,
            Paged::Pending => None,
        }
    }
}

/// A set asked for: its sheet's record, the set, its tiles, and the
/// catalogue they read from.
struct Job {
    record: FourCc,
    set: u16,
    tiles: BTreeSet<Tile>,
    catalogue: Arc<Catalogue<'static>>,
}

impl Job {
    /// Whether it is set `set` of `record`'s.
    fn of(&self, (record, set): (FourCc, u16)) -> bool {
        (self.record, self.set) == (record, set)
    }
}

/// A job on the compute pool, counted in [`RUNNING`] while it lives:
/// dropped, its task is cancelled.
struct Running {
    record: FourCc,
    set: u16,
    need: Need,
    task: Task<Laid>,
}

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Page jobs asked and not yet put on their sheets: queued, then running
/// on the compute pool, at most [`JOBS`] at once of every source's.
#[derive(Default)]
pub struct Jobs {
    /// Sets something draws, started first.
    drawn: VecDeque<Job>,
    /// Sets asked early.
    early: VecDeque<Job>,
    running: Vec<Running>,
}

impl Jobs {
    /// Jobs queued or running.
    #[must_use]
    pub fn len(&self) -> usize {
        self.drawn.len() + self.early.len() + self.running.len()
    }

    /// Jobs queued or running for `need`.
    #[must_use]
    pub fn of(&self, need: Need) -> usize {
        let queued = match need {
            Need::Drawn => self.drawn.len(),
            Need::Early => self.early.len(),
        };

        queued + self.running.iter().filter(|r| r.need == need).count()
    }

    /// A job queued for `need`.
    fn queue(&mut self, job: Job, need: Need) {
        match need {
            Need::Drawn => self.drawn.push_back(job),
            Need::Early => self.early.push_back(job),
        }
    }

    /// Set `set` of `record`, asked early, now drawn: queued ahead of the
    /// early ones, or counted drawn's where it runs.
    fn drawn_now(&mut self, owner: (FourCc, u16)) {
        if let Some(at) = self.early.iter().position(|job| job.of(owner)) {
            let job = self.early.remove(at);

            self.drawn.extend(job);
        }

        for run in &mut self.running {
            if (run.record, run.set) == owner {
                run.need = Need::Drawn;
            }
        }
    }

    /// Whether none is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The jobs finished, taken, with why each was asked; queued ones
    /// started in their place, the drawn's first.
    pub fn landed(&mut self) -> Vec<(FourCc, u16, Laid, Need)> {
        let mut landed = Vec::new();

        self.running.retain_mut(
            |run| match bevy::tasks::futures::check_ready(&mut run.task) {
                Some(laid) => {
                    landed.push((run.record, run.set, laid, run.need));
                    false
                }
                None => true,
            },
        );

        while RUNNING.load(Ordering::Relaxed) < JOBS {
            let next = match self.drawn.pop_front() {
                Some(job) => Some((job, Need::Drawn)),
                None => self.early.pop_front().map(|job| (job, Need::Early)),
            };
            let Some((job, need)) = next else {
                break;
            };
            let pool = ComputeTaskPool::get_or_init(TaskPool::default);
            let (record, set) = (job.record, job.set);
            let task = pool.spawn(async move { laid(&job.catalogue, job.tiles) });

            RUNNING.fetch_add(1, Ordering::Relaxed);
            self.running.push(Running {
                record,
                set,
                need,
                task,
            });
        }

        landed
    }
}

/// Whether a set is drawn on the map: not a panel's, the minimap's or
/// the ground's.
pub(crate) const fn world(kind: SetKind) -> bool {
    !matches!(
        kind,
        SetKind::Interface | SetKind::Minimap | SetKind::MapTexture { .. } | SetKind::UnitTexture
    )
}

/// Whether `frame` draws a shadow band: an opaque frame with a pixel the
/// world blit shades (`cel::picture`).
fn casts(frame: &Frame, palette: &Palette) -> bool {
    blend::alphas(frame.mode, frame.level, palette).is_none()
        && frame
            .covered
            .iter()
            .enumerate()
            .any(|(i, drawn)| *drawn && matches!(frame.ink(i, Blit::World), Ink::Shade(_)))
}

/// `tile` as a sheet holds it; `None` for direct colour or no canvas.
fn decode(catalogue: &Catalogue<'_>, tile: Tile) -> Option<Decoded> {
    let frame = Frame::at(catalogue, tile.0, tile.1)?.ok()?;
    let (image, palette) = texture::indices(&frame)?;
    let indices = image.data.clone()?;
    let width = u32::from(frame.width);
    let shadowed = casts(&frame, palette);
    let feet = shadowed
        .then(|| crate::lie::drops(&indices, usize::from(frame.width), Ramps::of(frame.mode)));
    let colours = match frame.source {
        PaletteSource::Splt { archive, index } => Colours::Splt(archive, index),
        _ => Colours::Own(tile.0, tile.1),
    };
    let look = Look {
        size: Vec2::new(f32::from(frame.width), f32::from(frame.height)),
        anchor: texture::anchor(&frame),
        mode: frame.mode,
        level: frame.level,
        colours,
        casts: shadowed,
        palette: Box::new(palette.clone()),
    };

    Some(Decoded {
        tile,
        width,
        height: u32::from(frame.height),
        indices,
        feet,
        look: Some(look),
    })
}

/// Each world set of `record`, by its low 16 bits, with the tiles every
/// layer, slot and stream of it names.
fn sets(corpus: &Corpus, record: FourCc) -> HashMap<u16, BTreeSet<Tile>> {
    let catalogue = &corpus.catalogue;
    let animations = catalogue
        .find(b"IMAG", record)
        .and_then(|at| imagery::sets(catalogue, at).ok())
        .unwrap_or_default();
    let mut sets: HashMap<u16, BTreeSet<Tile>> = HashMap::new();

    for set in animations.iter().filter(|set| world(set.kind)) {
        let tiles = set
            .poses
            .iter()
            .flat_map(|pose| pose.frames.iter())
            .filter_map(FrameSource::tile);

        sets.entry(set_number(set.id)).or_default().extend(tiles);
    }

    sets
}

/// `sizes` in pages of at most [`SIDE`]: one if they fit, else halved,
/// in order, until each does. Each page's members and packing.
#[must_use]
pub fn paged(sizes: &[(u32, u32)], side: u32) -> Vec<(Vec<usize>, Packed)> {
    split(sizes, (0..sizes.len()).collect(), side)
}

fn split(sizes: &[(u32, u32)], members: Vec<usize>, side: u32) -> Vec<(Vec<usize>, Packed)> {
    let wanted: Vec<(u32, u32)> = members.iter().map(|i| sizes[*i]).collect();
    let packed = pack::pack(&wanted, GUTTER, side);
    let fits = packed.width <= side && packed.height <= side;

    if fits || members.len() == 1 {
        vec![(members, packed)]
    } else {
        let (first, second) = members.split_at(members.len() / 2);
        let mut pages = split(sizes, first.to_vec(), side);

        pages.extend(split(sizes, second.to_vec(), side));
        pages
    }
}

/// `data` (`width` texels a row) copied into `page` (`across` a row) at
/// `rect`.
pub fn blit<T: Copy>(page: &mut [T], across: u32, rect: Rect, data: &[T]) {
    let (w, across) = (rect.w as usize, across as usize);

    for row in 0..rect.h as usize {
        let to = (rect.y as usize + row) * across + rect.x as usize;

        page[to..to + w].copy_from_slice(&data[row * w..(row + 1) * w]);
    }
}

/// A page image: `data` at `size`, `format`, nearest sampled, kept on
/// the CPU as `usage` says.
fn image(
    size: (u32, u32),
    data: Vec<u8>,
    format: TextureFormat,
    usage: RenderAssetUsages,
) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: size.0.max(1),
            height: size.1.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        format,
        usage,
    );

    image.sampler = ImageSampler::nearest();
    image
}

/// One page of `decoded`'s `members`, packed as `packed`: its images,
/// and each member's rectangle.
fn page(decoded: &[Decoded], (members, packed): &(Vec<usize>, Packed)) -> (Image, Option<Image>) {
    let (w, h) = (packed.width.max(1), packed.height.max(1));
    let texels = w as usize * h as usize;
    let mut indices = vec![TRANSPARENT; texels];
    let mut feet = vec![0u16; texels];

    for (member, rect) in members.iter().zip(&packed.rects) {
        let tile = &decoded[*member];

        blit(&mut indices, w, *rect, &tile.indices);

        if let Some(drops) = &tile.feet {
            blit(&mut feet, w, *rect, drops);
        }
    }

    // A page none of whose frames casts needs no feet.
    let casting = members.iter().any(|m| decoded[*m].feet.is_some());

    // As `lie::feet`: a byte a texel where every drop fits.
    let narrow = feet.iter().all(|d| u8::try_from(*d).is_ok());
    let (format, bytes) = if narrow {
        let bytes = feet.iter().map(|d| d.to_le_bytes()[0]).collect();

        (TextureFormat::R8Uint, bytes)
    } else {
        let bytes = feet.iter().flat_map(|d| d.to_le_bytes()).collect();

        (TextureFormat::R16Uint, bytes)
    };

    // The indices stay on the CPU for `perf::hide`'s coverage; feet not.
    let kept = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;

    (
        image((w, h), indices, TextureFormat::R8Uint, kept),
        casting.then(|| image((w, h), bytes, format, RenderAssetUsages::RENDER_WORLD)),
    )
}

/// `tiles`' pages, made from `catalogue`: decoded once, packed, built.
/// On the main thread or the compute pool alike.
fn laid(catalogue: &Catalogue<'_>, tiles: BTreeSet<Tile>) -> Laid {
    let decoded: Vec<Decoded> = tiles
        .into_iter()
        .filter_map(|t| decode(catalogue, t))
        .collect();
    let sizes: Vec<(u32, u32)> = decoded.iter().map(|d| (d.width, d.height)).collect();
    let pages = paged(&sizes, SIDE)
        .iter()
        .map(|members| {
            let (indices, feet) = page(&decoded, members);
            let rects = members
                .0
                .iter()
                .zip(&members.1.rects)
                .map(|(member, rect)| (decoded[*member].tile, *rect))
                .collect();

            (indices, feet, rects)
        })
        .collect();
    let looks = decoded
        .into_iter()
        .filter_map(|d| Some((d.tile, d.look?)))
        .collect();

    Laid { pages, looks }
}

impl Sheet {
    /// `record`'s sheet: its sets read, no page made; `None` without a
    /// world set.
    pub fn make(corpus: &Corpus, record: FourCc) -> Option<Sheet> {
        let sets = sets(corpus, record);

        (!sets.is_empty()).then(|| Sheet {
            record,
            sets,
            ..Sheet::default()
        })
    }

    /// Every set asked for and not yet put to lay again: its jobs let go
    /// with the sheet's source, shelved.
    pub fn forget(&mut self) {
        self.sets.extend(self.asked.drain());
    }

    /// Set `set`'s tiles not yet on a page.
    fn take(&mut self, set: u16) -> BTreeSet<Tile> {
        let wanted = self.sets.remove(&set).unwrap_or_default();

        wanted
            .into_iter()
            .filter(|t| !self.frames.contains_key(t))
            .collect()
    }

    /// Set `set` asked of `jobs` for `need`: its tiles' pages made off
    /// the main thread from `catalogue`. Nothing where it is laid; where
    /// asked early and now drawn, its job goes ahead.
    pub fn ask(
        &mut self,
        catalogue: &Arc<Catalogue<'static>>,
        (set, need): (u16, Need),
        jobs: &mut Jobs,
    ) {
        if self.sets.contains_key(&set) {
            let tiles = self.take(set);
            let job = Job {
                record: self.record,
                set,
                tiles: tiles.clone(),
                catalogue: catalogue.clone(),
            };

            self.asked.insert(set, tiles);
            jobs.queue(job, need);
        } else if need == Need::Drawn && self.asked.contains_key(&set) {
            jobs.drawn_now((self.record, set));
        }
    }

    /// Set `set`'s pages, landed from its job, put on the sheet; let go
    /// where its ask was forgotten since. How many pages were put.
    pub fn put(&mut self, set: u16, laid: Laid, images: &mut Assets<Image>) -> usize {
        if self.asked.remove(&set).is_some() {
            self.keep(laid, images)
        } else {
            0
        }
    }

    /// `laid`'s pages kept: their images added to `images`, their frames
    /// and looks kept; a frame already on a page stays there.
    fn keep(&mut self, laid: Laid, images: &mut Assets<Image>) -> usize {
        let made = laid.pages.len();

        for (indices, feet, members) in laid.pages {
            let at = self.pages.len();

            self.pages.push(Page {
                indices: images.add(indices),
                feet: feet.map(|f| images.add(f)).unwrap_or_default(),
            });

            for (tile, rect) in members {
                self.frames.entry(tile).or_insert(Framed { page: at, rect });
            }
        }

        for (tile, look) in laid.looks {
            self.looks.entry(tile).or_insert(look);
        }

        made
    }

    /// The asked set holding `tile`, if any.
    fn awaited(&self, tile: Tile) -> Option<u16> {
        self.asked
            .iter()
            .filter(|(_, tiles)| tiles.contains(&tile))
            .map(|(s, _)| *s)
            .min()
    }

    /// Whether set `set` is still to lay: neither asked for nor put.
    #[must_use]
    pub fn unasked(&self, set: u16) -> bool {
        self.sets.contains_key(&set)
    }

    /// Whether set `set` is asked for and not yet put.
    #[must_use]
    pub fn pending(&self, set: u16) -> bool {
        self.asked.contains_key(&set)
    }

    /// The set to lay for `tile`: `set` where it names the tile, else
    /// any not yet laid that does.
    fn naming(&self, tile: Tile, set: u16) -> Option<u16> {
        if self.sets.get(&set).is_some_and(|t| t.contains(&tile)) {
            Some(set)
        } else {
            self.sets
                .iter()
                .filter(|(_, tiles)| tiles.contains(&tile))
                .map(|(s, _)| *s)
                .min()
        }
    }

    /// Where `tile` is, if on a page yet.
    #[must_use]
    pub fn framed(&self, tile: Tile) -> Option<&Framed> {
        self.frames.get(&tile)
    }

    /// Frame `tile`'s materials, as `cel::picture` makes them for its own
    /// texture, on its page: its set's pages asked of `jobs` if need be
    /// (`set`, else a set that names it); `Ready(None)` off the sheet,
    /// `Pending` while its pages are on their way.
    pub fn cel(
        &mut self,
        corpus: &Corpus,
        store: &mut Store,
        palettes: &mut Palettes,
        (tile, set): (Tile, u16),
        jobs: &mut Jobs,
    ) -> Paged {
        let unlaid = !self.frames.contains_key(&tile);
        let laying = unlaid.then(|| self.naming(tile, set)).flatten();
        let awaited = unlaid.then(|| self.awaited(tile)).flatten();

        // One asked for first: another set naming the tile is not asked
        // too; asked early, it is drawn's now.
        match (awaited, laying) {
            (Some(awaited), _) => {
                jobs.drawn_now((self.record, awaited));
                Paged::Pending
            }
            (None, Some(laying)) => {
                self.ask(&corpus.catalogue, (laying, Need::Drawn), jobs);
                Paged::Pending
            }
            (None, None) => Paged::Ready(self.drawn(store, palettes, tile)),
        }
    }

    /// Frame `tile`'s materials on its page; `None` off the sheet.
    fn drawn(&mut self, store: &mut Store, palettes: &mut Palettes, tile: Tile) -> Option<Cel> {
        let framed = self.frames.get(&tile)?.clone();
        let page = self.pages[framed.page].clone();
        let look = self.looks.get(&tile)?;
        let (colours, palette) = (look.colours, look.palette.clone());
        let (mode, level, shadowed) = (look.mode, look.level, look.casts);
        let (size, anchor) = (look.size, look.anchor);
        let glassy = blend::alphas(mode, level, &palette);
        // As `cel::picture`: translucent with its lift, else opaque with
        // a shared palette and maybe a band.
        let (body, band, lift) = if let Some(alphas) = glassy {
            let shown = Some((mode, level));
            let glass = (colours, material::Pass::Glass, shown);
            let lifted = (colours, material::Pass::Lift, shown);
            let body = self.body(store, &page, (framed.page, glass), || {
                texture::glass(&palette, &alphas)
            })?;
            let lift = self.body(store, &page, (framed.page, lifted), || {
                texture::glass(&palette, &soft::lifts(&alphas))
            })?;

            (body, None, Some(lift))
        } else {
            let shared = if let Some(handle) = palettes.get(&colours) {
                handle.clone()
            } else {
                let handle = store.images.add(texture::palette(&palette)?);

                palettes.insert(colours, handle.clone());
                handle
            };
            let solid = (colours, material::Pass::Solid, None);
            let body = self.solid(store, &page, (framed.page, solid), shared);
            let band = shadowed.then(|| self.band(store, &page, framed.page, Ramps::of(mode)));

            (body, band, None)
        };

        Some(Cel {
            body: Picture::Indexed(body),
            band,
            aloft: None,
            lift,
            size,
            anchor,
            tile,
            frame: crate::instance::Framed(framed.rect),
        })
    }

    /// The body material for `key` on `page`, its palette made by
    /// `palette` the first time; `None` where that makes none.
    fn body(
        &mut self,
        store: &mut Store,
        page: &Page,
        key: (usize, Shown),
        palette: impl FnOnce() -> Option<Image>,
    ) -> Option<Handle<Body>> {
        let pass = key.1.1;

        if let Some(made) = self.bodies.get(&key) {
            Some(made.clone())
        } else {
            let made = store.bodies.add(Body {
                indices: page.indices.clone(),
                palette: store.images.add(palette()?),
                pass,
                sway: Sway::Still,
                gust: Gust::STILL,
                sampling: Sampling::Nearest,
                on: material::On::Sheet,
                ..Body::default()
            });

            self.bodies.insert(key, made.clone());
            Some(made)
        }
    }

    /// An opaque body on `page` drawing through shared `palette`.
    fn solid(
        &mut self,
        store: &mut Store,
        page: &Page,
        key: (usize, Shown),
        palette: Handle<Image>,
    ) -> Handle<Body> {
        self.bodies
            .entry(key)
            .or_insert_with(|| {
                store.bodies.add(Body {
                    indices: page.indices.clone(),
                    palette,
                    pass: material::Pass::Solid,
                    sway: Sway::Still,
                    gust: Gust::STILL,
                    sampling: Sampling::Nearest,
                    on: material::On::Sheet,
                    ..Body::default()
                })
            })
            .clone()
    }

    /// The band material for `ramps` on `page`.
    fn band(&mut self, store: &mut Store, page: &Page, at: usize, ramps: Ramps) -> Handle<Band> {
        self.bands
            .entry((at, ramps))
            .or_insert_with(|| {
                store.bands.add(Band {
                    indices: page.indices.clone(),
                    rows: crate::lie::ROWS,
                    // `shadow::follow` sets the wanted one.
                    shadows: crate::shadow::Shadows::default(),
                    ramps,
                    sway: Sway::Still,
                    gust: Gust::STILL,
                    feet: page.feet.clone(),
                    sampling: Sampling::Nearest,
                    painted: material::Painted::Kept,
                    table: crate::instance::TABLE,
                    on: material::On::Sheet,
                })
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiles of a few sizes, each filled with its own index.
    fn tiles() -> Vec<Decoded> {
        [(5, 3), (2, 7), (4, 4)]
            .into_iter()
            .enumerate()
            .map(|(i, (w, h))| Decoded {
                tile: (0, i),
                width: w,
                height: h,
                indices: vec![u8::try_from(i + 1).unwrap(); (w * h) as usize],
                feet: (i == 1).then(|| vec![3; (w * h) as usize]),
                look: None,
            })
            .collect()
    }

    /// A page holds each tile's texels at its rectangle, and 255
    /// between: the gutter a sharp tap reads as off the frame.
    #[test]
    fn a_page_holds_each_frame_at_its_rectangle() {
        let decoded = tiles();
        let sizes: Vec<(u32, u32)> = decoded.iter().map(|d| (d.width, d.height)).collect();
        let laid = paged(&sizes, SIDE).remove(0);
        let (indices, feet) = page(&decoded, &laid);
        let data = indices.data.unwrap();
        let w = laid.1.width as usize;

        for (member, rect) in laid.0.iter().zip(&laid.1.rects) {
            assert_eq!((rect.w, rect.h), sizes[*member]);

            for y in rect.y..rect.y + rect.h {
                for x in rect.x..rect.x + rect.w {
                    assert_eq!(
                        data[y as usize * w + x as usize],
                        decoded[*member].indices[0]
                    );
                }
            }
        }

        let covered: u64 = laid.1.rects.iter().map(Rect::area).sum();
        #[allow(clippy::naive_bytecount, reason = "a test's few texels")]
        let holes = data.iter().filter(|i| **i == TRANSPARENT).count();

        assert_eq!(covered + holes as u64, data.len() as u64);
        assert_eq!(
            feet.map(|f| f.texture_descriptor.format),
            Some(TextureFormat::R8Uint)
        );
    }

    /// A page of frames none of which casts has no feet.
    #[test]
    fn a_page_casting_nothing_has_no_feet() {
        let mut decoded = tiles();

        decoded.retain(|d| d.feet.is_none());

        let sizes: Vec<(u32, u32)> = decoded.iter().map(|d| (d.width, d.height)).collect();
        let laid = paged(&sizes, SIDE).remove(0);

        assert!(page(&decoded, &laid).1.is_none());
    }

    /// Frames past a page's side go on more pages, each within it.
    #[test]
    fn frames_past_a_side_take_another_page() {
        let sizes = [(60, 60); 5];
        let pages = paged(&sizes, 100);

        assert!(pages.len() > 1);
        assert!(pages.iter().all(|(_, p)| p.width <= 100 && p.height <= 100));
        assert_eq!(pages.iter().map(|(m, _)| m.len()).sum::<usize>(), 5);
    }

    /// Every frame of the Palace and the Warrior on their sheets draws as
    /// on its own texture: the same size, anchor and band; its page
    /// rectangle holds its indices; frames share a body exactly where
    /// they share a palette; and a set's pages are made only when it is
    /// first drawn.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_sheet_frame_draws_as_its_own_texture() {
        let mut paths = crate::common::archives(&crate::common::hd().join("Data"));

        paths.extend(crate::common::archives(&crate::common::hd().join("DataMX")));

        let corpus = Corpus::open(&paths).expect("HD's archives");
        let (mut images, mut meshes) = (Assets::<Image>::default(), Assets::<Mesh>::default());
        let (mut colours, mut bodies, mut bands) = (
            Assets::<crate::plain::Plain>::default(),
            Assets::<Body>::default(),
            Assets::<Band>::default(),
        );
        let quad = meshes.add(Rectangle::new(1.0, 1.0));
        let mut store = Store {
            images: &mut images,
            meshes: &mut meshes,
            colours: &mut colours,
            bodies: &mut bodies,
            bands: &mut bands,
            quad,
        };

        for record in [FourCc(*b"ABJ1"), FourCc(*b"AVL1")] {
            let mut sheet = Sheet::make(&corpus, record).expect("a sheet");
            let (mut palettes, mut own) = (Palettes::new(), Palettes::new());
            let mut shared: HashMap<AssetId<Body>, Colours> = HashMap::new();
            let named: Vec<(u16, Tile)> = sheet
                .sets
                .iter()
                .flat_map(|(set, tiles)| tiles.iter().map(|t| (*set, *t)))
                .collect();
            let first = named[0].0;

            assert!(sheet.pages.is_empty(), "no page before a frame draws");

            let _ = landed(
                &mut sheet,
                &corpus,
                (&mut store, &mut palettes),
                (named[0].1, first),
            );

            assert!(!sheet.sets.contains_key(&first) && !sheet.pages.is_empty());

            for (set, tile) in named {
                let source = FrameSource::Tile {
                    archive: tile.0,
                    index: tile.1,
                    flags: 0,
                };
                let framed = landed(
                    &mut sheet,
                    &corpus,
                    (&mut store, &mut palettes),
                    (tile, set),
                )
                .expect("framed");
                let alone =
                    super::super::cel::cel(&corpus, &mut store, &mut own, source).expect("alone");
                let at = sheet.framed(tile).expect("on the sheet").clone();
                let page = &sheet.pages[at.page];

                assert_eq!((framed.size, framed.anchor), (alone.size, alone.anchor));
                assert_eq!(framed.band.is_some(), alone.band.is_some());
                assert_eq!(framed.lift.is_some(), alone.lift.is_some());

                // The page's rectangle is the tile's own texture.
                let (Picture::Indexed(body), Picture::Indexed(own_body)) =
                    (&framed.body, &alone.body)
                else {
                    panic!("indexed");
                };
                let own_indices = store
                    .images
                    .get(&store.bodies.get(own_body).unwrap().indices)
                    .unwrap();
                let page_image = store.images.get(&page.indices).unwrap();
                let width = page_image.width();
                let mut cut = vec![0u8; usize::try_from(at.rect.area()).unwrap()];

                for row in 0..at.rect.h {
                    let from = ((at.rect.y + row) * width + at.rect.x) as usize;
                    let w = at.rect.w as usize;

                    cut[row as usize * w..(row as usize + 1) * w]
                        .copy_from_slice(&page_image.data.as_ref().unwrap()[from..from + w]);
                }

                assert_eq!(
                    &cut,
                    own_indices.data.as_ref().unwrap(),
                    "{record} {tile:?}"
                );

                // A body is shared by one palette alone.
                let frame = Frame::at(&corpus.catalogue, tile.0, tile.1)
                    .unwrap()
                    .unwrap();
                let colours = match frame.source {
                    PaletteSource::Splt { archive, index } => Colours::Splt(archive, index),
                    _ => Colours::Own(tile.0, tile.1),
                };
                let was = shared.insert(body.id(), colours);

                assert!(was.is_none_or(|was| was == colours));
            }
        }
    }

    /// Frame `key` of `sheet`, its set's pages asked and landed first.
    fn landed(
        sheet: &mut Sheet,
        corpus: &Corpus,
        (store, palettes): (&mut Store, &mut Palettes),
        key: (Tile, u16),
    ) -> Option<Cel> {
        let mut jobs = Jobs::default();

        loop {
            match sheet.cel(corpus, store, palettes, key, &mut jobs) {
                Paged::Ready(cel) => break cel,
                Paged::Pending => {
                    for (_, set, laid, _) in jobs.landed() {
                        let _ = sheet.put(set, laid, store.images);
                    }

                    std::thread::yield_now();
                }
            }
        }
    }

    /// A set asked for is pending until its job lands, asked once however
    /// often its frames are; put, its frames are ready. A sheet shelved
    /// forgets its asks: the job landing after is let go, and the set is
    /// asked again.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_set_asked_is_pending_until_its_job_lands() {
        let corpus = Corpus::open(&crate::common::archives(&crate::common::hd().join("Data")))
            .expect("HD's archives");
        let record = FourCc(*b"AVL1");
        let mut images = Assets::<Image>::default();
        let (mut meshes, mut colours) = (Assets::<Mesh>::default(), Assets::default());
        let (mut bodies, mut bands) = (Assets::<Body>::default(), Assets::<Band>::default());
        let quad = meshes.add(Rectangle::new(1.0, 1.0));
        let mut store = Store {
            images: &mut images,
            meshes: &mut meshes,
            colours: &mut colours,
            bodies: &mut bodies,
            bands: &mut bands,
            quad,
        };
        let mut palettes = Palettes::new();
        let mut sheet = Sheet::make(&corpus, record).expect("a sheet");
        let key = sheet
            .sets
            .iter()
            .map(|(set, tiles)| (*tiles.first().expect("a tile"), *set))
            .min_by_key(|(_, set)| *set)
            .expect("a set");
        let mut jobs = Jobs::default();
        let mut drawn = |sheet: &mut Sheet, store: &mut Store, jobs: &mut Jobs, key| {
            sheet.cel(&corpus, store, &mut palettes, key, jobs)
        };

        assert!(matches!(
            drawn(&mut sheet, &mut store, &mut jobs, key),
            Paged::Pending
        ));
        assert!(matches!(
            drawn(&mut sheet, &mut store, &mut jobs, key),
            Paged::Pending
        ));
        assert_eq!(jobs.len(), 1, "asked once");
        assert_eq!(jobs.of(Need::Drawn), 1, "drawn's");

        // Another set asked early, then drawn: its job goes ahead.
        let early = sheet
            .sets
            .keys()
            .copied()
            .find(|s| *s != key.1)
            .expect("another set");
        let tile = *sheet.sets[&early]
            .iter()
            .find(|t| !sheet.asked[&key.1].contains(t))
            .expect("a tile of its own");

        sheet.ask(&corpus.catalogue, (early, Need::Early), &mut jobs);
        assert_eq!(jobs.of(Need::Early), 1);

        let _ = drawn(&mut sheet, &mut store, &mut jobs, (tile, early));

        assert_eq!((jobs.of(Need::Early), jobs.of(Need::Drawn)), (0, 2));

        // Shelved: the ask forgotten, the job let go.
        sheet.forget();
        drop(std::mem::take(&mut jobs));

        while jobs.is_empty() || !sheet.pending(key.1) {
            let _ = drawn(&mut sheet, &mut store, &mut jobs, key);
        }

        assert_eq!(jobs.len(), 1, "asked again");

        let cel = loop {
            if let Paged::Ready(cel) = drawn(&mut sheet, &mut store, &mut jobs, key) {
                break cel;
            }

            for (_, set, laid, _) in jobs.landed() {
                let _ = sheet.put(set, laid, store.images);
            }

            std::thread::yield_now();
        };

        assert!(cel.is_some());
        assert!(jobs.is_empty() && !sheet.pages.is_empty());
    }
}
