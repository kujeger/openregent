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

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU32, Ordering};

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use regent_core::content::imagery::{self, FrameSource};
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

/// Pages made since the process started, every sheet's: what a frame
/// made is the change ([`crate::settle`]).
static MADE: AtomicU32 = AtomicU32::new(0);

/// Pages made since the process started, wrapping.
pub fn pages_made() -> u32 {
    MADE.load(Ordering::Relaxed)
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
    /// Each world set's tiles not yet laid (low 16 bits of its id: every
    /// layer).
    sets: HashMap<u16, BTreeSet<Tile>>,
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

/// Whether a set is drawn on the map: not a panel's, the minimap's or
/// the ground's.
const fn world(kind: SetKind) -> bool {
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
fn decode(corpus: &Corpus, tile: Tile) -> Option<Decoded> {
    let frame = Frame::at(&corpus.catalogue, tile.0, tile.1)?.ok()?;
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

impl Sheet {
    /// `record`'s sheet: its sets read, no page made; `None` without a
    /// world set.
    pub fn make(corpus: &Corpus, record: FourCc) -> Option<Sheet> {
        let sets = sets(corpus, record);

        (!sets.is_empty()).then(|| Sheet {
            sets,
            ..Sheet::default()
        })
    }

    /// Set `set`'s tiles not yet on a page decoded once, packed and added
    /// to `store` as pages of their own.
    fn lay(&mut self, corpus: &Corpus, store: &mut Store, set: u16) {
        let wanted = self.sets.remove(&set).unwrap_or_default();
        let decoded: Vec<Decoded> = wanted
            .into_iter()
            .filter(|t| !self.frames.contains_key(t))
            .filter_map(|t| decode(corpus, t))
            .collect();
        let sizes: Vec<(u32, u32)> = decoded.iter().map(|d| (d.width, d.height)).collect();

        for laid in &paged(&sizes, SIDE) {
            let at = self.pages.len();
            let (indices, feet) = page(&decoded, laid);

            MADE.fetch_add(1, Ordering::Relaxed);
            self.pages.push(Page {
                indices: store.images.add(indices),
                feet: feet.map(|f| store.images.add(f)).unwrap_or_default(),
            });

            for (member, rect) in laid.0.iter().zip(&laid.1.rects) {
                let framed = Framed {
                    page: at,
                    rect: *rect,
                };

                self.frames.insert(decoded[*member].tile, framed);
            }
        }

        for tile in decoded {
            if let Some(look) = tile.look {
                self.looks.insert(tile.tile, look);
            }
        }
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
    /// texture, on its page: its set's pages made first if need be (`set`,
    /// else a set that names it); `None` off the sheet.
    pub fn cel(
        &mut self,
        corpus: &Corpus,
        store: &mut Store,
        palettes: &mut Palettes,
        (tile, set): (Tile, u16),
    ) -> Option<Cel> {
        let laying = (!self.frames.contains_key(&tile))
            .then(|| self.naming(tile, set))
            .flatten();

        if let Some(laying) = laying {
            self.lay(corpus, store, laying);
        }

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

            let _ = sheet.cel(&corpus, &mut store, &mut palettes, (named[0].1, first));

            assert!(!sheet.sets.contains_key(&first) && !sheet.pages.is_empty());

            for (set, tile) in named {
                let source = FrameSource::Tile {
                    archive: tile.0,
                    index: tile.1,
                    flags: 0,
                };
                let framed = sheet
                    .cel(&corpus, &mut store, &mut palettes, (tile, set))
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
}
