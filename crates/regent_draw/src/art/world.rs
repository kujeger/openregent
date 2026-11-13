//! The art service's Bevy front end: the game view's sprites, and the
//! interface pictures drawn as Bevy images (the cursor). One
//! [`Sprites`], a resource, for the menus and the game.
//!
//! ```text
//!  draw code ──Piece { code, set, slot, still }──> Sprites ──> layers (map::Drawing)
//!            ──frame (FrameSource)──────────────>         ──> map::Cel: Body, Band, lift
//!            ──body, Variant (tint, glow, dark)─>         ──> Body, palette swapped
//!            ──cursor (set, slot)───────────────>         ──> Image
//!                     held by source (Held), each made once
//! ```
//!
//! An indexed frame is an index texture and a palette texture, the
//! palette looked up on the GPU (`crate::material`): one index
//! texture a tile, one palette texture a palette (`map::cel::Colours`),
//! shared by every piece, scatter and ring drawing them. A variant
//! swaps the palette alone. Nothing is scaled (`docs/openregent/enhancements.md`).

use std::collections::HashMap;

use bevy::asset::AssetId;
use bevy::prelude::*;

use crate::data::Corpus;
use crate::map::cel::{self, Cel, Drawing, Paint, Palettes, Store};
use crate::material::{Body, Pass};
use crate::plain::Plain;
use crate::{soft, texture};
use regent_core::content::imagery::FrameSource;
use regent_core::content::unit::Unit;
use regent_core::data::fourcc::FourCc;
use regent_core::data::payload::{Palette, Rgb};
use regent_core::sim::footprint;
use regent_core::sim::world::draw::Tint;
use regent_core::view::map::fog;
use regent_core::view::raster::blend::{self, TINTS};

use super::{ArtSource, Pinned, Split, Tile, cursor};

/// The asset stores a piece's, a frame's and a ring's materials are
/// written into.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Assets2<'w> {
    pub images: ResMut<'w, Assets<Image>>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub colours: ResMut<'w, Assets<Plain>>,
    pub bodies: ResMut<'w, Assets<Body>>,
    pub bands: ResMut<'w, Assets<crate::material::Band>>,
}

/// A piece's picture: its descriptor, set and slot, and the frame held,
/// or `None` for the whole stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Piece {
    pub code: FourCc,
    pub set: u16,
    pub slot: usize,
    pub still: Option<u16>,
}

/// A body with its palette made over; indices kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Variant {
    /// Draw effects (`SetDrawEffects`): colours to the tint's nearest
    /// ([`blend::tinted`]), see-through at the transparency
    /// ([`blend::see_through`]), every index alike.
    Tint(Tint),
    /// Mode 8 at this level: each index's alpha by its colour
    /// ([`blend::glow`]), a fading particle.
    Glow(i32),
    /// A [`Pass::Glass`] body's twin, lifting the shadow under it
    /// ([`soft::lifts`]): no body for any other pass.
    Lift,
    /// The fog's palette `k` ([`fog::darkened`]), alphas kept.
    Dark(usize),
}

/// A cursor image and its hotspot.
pub type Pointer = (Handle<Image>, (u16, u16));

/// One source's sprites.
#[derive(Default)]
struct Held {
    /// Each tile's materials.
    cels: HashMap<Tile, Option<Cel>>,
    /// Palette textures, shared by the tiles naming one.
    palettes: Palettes,
    pieces: HashMap<Piece, Vec<Drawing>>,
    variants: HashMap<(AssetId<Body>, Variant), Handle<Body>>,
    /// `SetDrawEffects`' colours 1 to 3 ([`TINTS`]), read on first use.
    tints: Option<[Option<Palette>; 3]>,
    cursors: HashMap<(u32, usize), Option<Pointer>>,
}

/// Sprites held, each under its source: all of the service's, or one
/// source's taken for the shelf ([`Sprites::take`]).
#[derive(Resource, Default)]
pub struct Entries(HashMap<ArtSource, Held>);

impl Entries {
    fn held(&mut self, source: ArtSource) -> &mut Held {
        self.0.entry(source).or_default()
    }
}

impl Split for Entries {
    fn split(&mut self, source: ArtSource) -> Entries {
        Entries(self.0.remove_entry(&source).into_iter().collect())
    }

    fn join(&mut self, other: Entries) {
        for (source, held) in other.0 {
            self.0.entry(source).or_insert(held);
        }
    }
}

/// The world's sprites and the interface's Bevy images, made from each
/// source's archives.
#[derive(Resource, Default)]
pub struct Sprites {
    entries: Entries,
    /// The sources [`Sprites::take`] leaves: the menus'.
    pinned: Pinned,
    /// The unit quad every sprite scales.
    quad: Option<Handle<Mesh>>,
}

impl Sprites {
    /// None held, `sources` pinned.
    #[must_use]
    pub fn pinned(sources: impl IntoIterator<Item = ArtSource>) -> Sprites {
        Sprites {
            pinned: Pinned::of(sources),
            ..Sprites::default()
        }
    }

    /// `source`'s sprites moved out, for the next game on its dataset;
    /// none of a pinned source.
    pub fn take(&mut self, source: ArtSource) -> Entries {
        self.pinned.take(&mut self.entries, source)
    }

    /// Sprites [`Sprites::take`] gave, back.
    pub fn put(&mut self, entries: Entries) {
        self.entries.join(entries);
    }

    /// The unit quad every sprite scales.
    pub fn quad(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.quad
            .get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0)))
            .clone()
    }

    /// `piece`'s layers from `corpus`, lowest first (`Drawing::layers`):
    /// made on first use.
    pub fn piece(&mut self, corpus: &Corpus, stores: &mut Assets2, piece: Piece) -> &[Drawing] {
        let quad = self.quad(&mut stores.meshes);
        let Held {
            cels,
            palettes,
            pieces,
            ..
        } = self.entries.held(corpus.source());

        pieces.entry(piece).or_insert_with(|| {
            let mut store = store(stores, quad);
            let mut cel = |s| cached((cels, palettes), corpus, &mut store, s);

            match Unit::find(&corpus.catalogue, piece.code) {
                Some(Ok(unit)) => {
                    let mut drawings = Drawing::layers(
                        &corpus.catalogue,
                        &unit,
                        piece.set,
                        &|_| piece.slot,
                        piece.still,
                        &mut cel,
                    );
                    let stands = footprint::of_unit(&corpus.catalogue, &unit).ok().flatten();

                    cel::keep_parts(&mut drawings, (piece.set, stands), corpus, &mut store);
                    drawings
                }
                _ => Vec::new(),
            }
        })
    }

    /// What `source`'s sprites hold: every piece made and every tile made
    /// (with or without materials), for the perf runs' art dump.
    #[must_use]
    pub fn held(&self, source: ArtSource) -> (Vec<Piece>, Vec<Tile>) {
        self.entries
            .0
            .get(&source)
            .map_or_else(Default::default, |held| {
                (
                    held.pieces.keys().copied().collect(),
                    held.cels.keys().copied().collect(),
                )
            })
    }

    /// Frame `source` of `corpus` as materials (`map::cel::cel`); `None`
    /// where it draws nothing.
    pub fn frame(
        &mut self,
        corpus: &Corpus,
        stores: &mut Assets2,
        source: FrameSource,
    ) -> Option<Cel> {
        let quad = self.quad(&mut stores.meshes);
        let mut store = store(stores, quad);

        self.cel(corpus, &mut store, source)
    }

    /// `body`, one of `corpus`'s, as `variant`: made once, then kept.
    pub fn variant(
        &mut self,
        corpus: &Corpus,
        (images, bodies): (&mut Assets<Image>, &mut Assets<Body>),
        body: &Handle<Body>,
        variant: Variant,
    ) -> Option<Handle<Body>> {
        let held = self.entries.held(corpus.source());
        let key = (body.id(), variant);

        if let Some(made) = held.variants.get(&key) {
            Some(made.clone())
        } else {
            let tints = held.tints.get_or_insert_with(|| tints(corpus));
            let made = remade(bodies.get(body)?, variant, tints, images)?;
            let handle = bodies.add(made);

            held.variants.insert(key, handle.clone());
            Some(handle)
        }
    }

    /// `CUR1` set and slot `key` from `source`, as a cursor image and its
    /// hotspot; `read` gives its picture on first use.
    pub fn cursor(
        &mut self,
        images: &mut Assets<Image>,
        (source, key): (ArtSource, (u32, usize)),
        read: impl FnOnce() -> Option<cursor::Picture>,
    ) -> Option<Pointer> {
        self.entries
            .held(source)
            .cursors
            .entry(key)
            .or_insert_with(|| {
                let picture = read()?;
                let image = texture::sheet(&picture.sheet)?;

                Some((images.add(image), picture.hotspot))
            })
            .clone()
    }
}

impl Paint for Sprites {
    fn cel(&mut self, corpus: &Corpus, store: &mut Store, source: FrameSource) -> Option<Cel> {
        let Held { cels, palettes, .. } = self.entries.held(corpus.source());

        cached((cels, palettes), corpus, store, source)
    }
}

/// The stores a frame's materials are written into, as `map::cel::cel` takes
/// them.
fn store<'a>(stores: &'a mut Assets2, quad: Handle<Mesh>) -> Store<'a> {
    Store {
        images: &mut stores.images,
        meshes: &mut stores.meshes,
        colours: &mut stores.colours,
        bodies: &mut stores.bodies,
        bands: &mut stores.bands,
        quad,
    }
}

/// Frame `source`'s materials, made once a tile.
fn cached(
    (cels, palettes): (&mut HashMap<Tile, Option<Cel>>, &mut Palettes),
    corpus: &Corpus,
    store: &mut Store,
    source: FrameSource,
) -> Option<Cel> {
    cels.entry(source.tile()?)
        .or_insert_with(|| cel::cel(corpus, store, palettes, source))
        .clone()
}

/// `corpus`'s tint palettes, `SetDrawEffects`' colours 1 to 3.
fn tints(corpus: &Corpus) -> [Option<Palette>; 3] {
    let palette = |tag: [u8; 4]| {
        let at = corpus.catalogue.find(b"PALT", FourCc(tag))?;
        let record = corpus.catalogue.record(b"PALT", at)?;

        Palette::parse(record.data).ok()
    };

    TINTS.map(palette)
}

/// `body` as `variant`, its new palette added to `images`; `tints` the
/// tint palettes.
fn remade(
    body: &Body,
    variant: Variant,
    tints: &[Option<Palette>; 3],
    images: &mut Assets<Image>,
) -> Option<Body> {
    let data = images.get(&body.palette)?.data.clone()?;
    let own = Palette {
        colours: std::array::from_fn(|i| Rgb {
            r: data[4 * i],
            g: data[4 * i + 1],
            b: data[4 * i + 2],
        }),
    };
    let kept: [u8; 256] = std::array::from_fn(|i| data[4 * i + 3]);
    let (colours, alphas, pass) = match variant {
        Variant::Tint(t) => {
            let tint = t
                .colour
                .and_then(|c| tints.get(usize::from(c).checked_sub(1)?)?.as_ref());
            let colours = match tint {
                Some(tint) => blend::tinted(&own, tint),
                None => own,
            };

            match t.transparency {
                Some(level) => (colours, blend::see_through(level), Pass::Glass),
                None => (colours, kept, body.pass),
            }
        }
        Variant::Glow(steps) => {
            let alphas = blend::glow(steps, &own);

            (own, alphas, Pass::Glass)
        }
        Variant::Lift => {
            (body.pass == Pass::Glass).then(|| (own, soft::lifts(&kept), Pass::Lift))?
        }
        Variant::Dark(k) => (fog::darkened(&own, k), kept, body.pass),
    };

    Some(Body {
        indices: body.indices.clone(),
        palette: images.add(texture::glass(&colours, &alphas)?),
        pass,
        sway: body.sway,
        gust: body.gust,
        sampling: body.sampling,
        ..Body::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No tint palettes.
    const NO_TINTS: [Option<Palette>; 3] = [None, None, None];

    /// An opaque grey body, its palette in `images`.
    fn solid(images: &mut Assets<Image>) -> Body {
        let grey = Palette {
            colours: [Rgb { r: 9, g: 9, b: 9 }; 256],
        };

        Body {
            indices: Handle::default(),
            palette: images.add(texture::palette(&grey).expect("palette")),
            pass: Pass::Solid,
            sway: crate::wind::Sway::Still,
            gust: crate::wind::Gust::STILL,
            sampling: crate::smooth::Sampling::Nearest,
            ..Body::default()
        }
    }

    /// `level`'s see-through tint of a solid body, then its lift: the
    /// lift's alpha at index 1 and at [`blend::UNDRAWN`].
    fn lifted(level: u8) -> Option<(u8, u8)> {
        let mut images = Assets::<Image>::default();
        let tint = Tint {
            colour: None,
            transparency: Some(level),
        };
        let solid = solid(&mut images);
        let glass = remade(&solid, Variant::Tint(tint), &NO_TINTS, &mut images)?;
        let lift = remade(&glass, Variant::Lift, &NO_TINTS, &mut images)?;
        let data = images.get(&lift.palette)?.data.clone()?;

        assert_eq!(lift.pass, Pass::Lift);
        Some((data[4 + 3], data[4 * blend::UNDRAWN + 3]))
    }

    /// See-through at level 4 (alpha 153) lifts the shadow under it, at
    /// level 5 (127) not; undrawn indices never.
    #[test]
    fn a_see_through_unit_lifts_from_half_drawn() {
        assert_eq!(lifted(4), Some((u8::MAX, 0)));
        assert_eq!(lifted(5), Some((0, 0)));
    }

    /// Only a translucent body has a lift.
    #[test]
    fn an_opaque_body_has_no_lift() {
        let mut images = Assets::<Image>::default();
        let solid = solid(&mut images);

        assert!(remade(&solid, Variant::Lift, &NO_TINTS, &mut images).is_none());
    }
}
