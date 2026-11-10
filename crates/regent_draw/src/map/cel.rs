//! A frame as materials ([`Cel`]), a set's layers as pictures
//! ([`Drawing`]), and who makes them ([`Paint`]).

use std::collections::BTreeMap;

use bevy::prelude::*;

use regent_core::content::imagery::{self, Animation, Blitted, FrameSource, Pose};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::imag::{Blit, Pass, SetKind};
use regent_core::sim::footprint::ArtRect;
use regent_core::sim::movement::action::frame_ms;
use regent_core::view::raster::blend;
use regent_core::view::{Backdrop, Ink, PaletteSource, Sprite as Frame};

use super::Item;
use crate::data::Corpus;
use crate::plain::{Alpha, Plain};
use crate::smooth::Sampling;
use crate::wind::{Gust, Sway};
use crate::{material, soft, texture};

/// The asset stores a draw writes into.
pub struct Store<'a> {
    pub images: &'a mut Assets<Image>,
    pub meshes: &'a mut Assets<Mesh>,
    pub colours: &'a mut Assets<Plain>,
    pub bodies: &'a mut Assets<material::Body>,
    pub bands: &'a mut Assets<material::Band>,
    /// The unit quad every object is drawn on.
    pub quad: Handle<Mesh>,
}

/// One texture per palette.
pub type Palettes = BTreeMap<Colours, Handle<Image>>;

/// What makes a redraw's materials: a frame's ([`cel`]). openregent's is
/// its art service, which keeps them.
pub trait Paint {
    /// Frame `source` as materials.
    fn cel(&mut self, corpus: &Corpus, store: &mut Store, source: FrameSource) -> Option<Cel>;

    /// A whole redraw begins.
    fn begin(&mut self) {}
}

/// Palette identity. [`regent_core::view::PaletteSource`] alone cannot tell
/// embedded palettes apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Colours {
    /// The `index`th `SPLT` of an archive, shared by every tile naming it.
    Splt(usize, usize),
    /// Held inside one tile, and shared with nothing.
    Own(usize, usize),
}

/// Whether a thing moves, which decides its slot word's meaning.
///
/// Static objects: a slot (which hovel, which pine). Mobiles: a direction of
/// 32, mapped onto the set's slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Mobility {
    Placed,
    Walks,
}

impl Mobility {
    /// Which slot of `slots` this thing draws.
    ///
    /// Out-of-range is the caller's problem: one-slot buildings can carry a
    /// value (two `ABE2 Guardhouse2` read 19 and 15).
    fn slot(self, item: &Item, slots: usize) -> usize {
        match self {
            Mobility::Walks => usize::from(item.facing(u16::try_from(slots).unwrap_or(u16::MAX))),
            Mobility::Placed => usize::from(item.drawing.0),
        }
    }
}

/// Low half of a packed set id; the top byte is the layer, and all layers
/// draw together.
use regent_core::data::imag::set_number as low;

/// The set an object is drawn in: its own.
fn named_set(unit: &regent_core::content::unit::Unit<'_>, item: &Item) -> Option<u16> {
    unit.drawn(low(item.set))
}

/// What draws a layer's colours.
#[derive(Clone)]
pub enum Picture {
    /// Indexed art, palette resolved on the GPU. All map art bar one ruin.
    Indexed(Handle<material::Body>),
    /// Direct colour: no shadow possible. Only the Fairgrounds ruin frame; see
    /// `a_units_frames_are_indexed_bar_the_fairgrounds_ruin`.
    Colour(Handle<Plain>),
}

/// A frame's materials: its picture, shadow band and lift ([`Cel`]).
type Materials = (
    Picture,
    Option<Handle<material::Band>>,
    Option<Handle<material::Body>>,
);

/// The materials one frame needs. `None` for zero-sized canvases (26 in
/// the base dataset).
fn picture(
    store: &mut Store,
    palettes: &mut Palettes,
    frame: &Frame,
    at: (usize, usize),
) -> Option<Materials> {
    let Some((indices, palette)) = texture::indices(frame) else {
        // Direct colour: flatten; no shadow possible.
        let image = texture::image(frame, Backdrop::None, Blit::World)?;
        let colour = Plain {
            picture: Some(store.images.add(image)),
            // Same phase as the rest, else it draws over the town.
            alpha: Alpha::Mask,
            ..default()
        };

        return Some((Picture::Colour(store.colours.add(colour)), None, None));
    };

    let indices = store.images.add(indices);

    // Draw modes 5 and 8 are translucent, and cast no shadow; their lift
    // clears the coverage of shadows drawn before them.
    if let Some(alphas) = blend::alphas(frame.mode, frame.level, palette) {
        let body = store.bodies.add(material::Body {
            indices: indices.clone(),
            palette: store.images.add(texture::glass(palette, &alphas)?),
            pass: material::Pass::Glass,
            sway: Sway::Still,
            gust: Gust::STILL,
            sampling: Sampling::Nearest,
            ..material::Body::default()
        });
        let lift = store.bodies.add(material::Body {
            indices,
            palette: store
                .images
                .add(texture::glass(palette, &soft::lifts(&alphas))?),
            pass: material::Pass::Lift,
            sway: Sway::Still,
            gust: Gust::STILL,
            sampling: Sampling::Nearest,
            ..material::Body::default()
        });

        return Some((Picture::Indexed(body), None, Some(lift)));
    }

    let key = match frame.source {
        PaletteSource::Splt { archive, index } => Colours::Splt(archive, index),
        _ => Colours::Own(at.0, at.1),
    };
    let colours = if let Some(handle) = palettes.get(&key) {
        handle.clone()
    } else {
        let handle = store.images.add(texture::palette(palette)?);

        palettes.insert(key, handle.clone());
        handle
    };

    // No shadow, no second draw.
    let band = frame
        .covered
        .iter()
        .enumerate()
        .any(|(i, drawn)| *drawn && matches!(frame.ink(i, Blit::World), Ink::Shade(_)))
        .then(|| {
            store.bands.add(material::Band {
                indices: indices.clone(),
                rows: crate::lie::ROWS,
                // `shadow::follow` sets the wanted one.
                shadows: crate::shadow::Shadows::default(),
                ramps: soft::Ramps::of(frame.mode),
                sway: Sway::Still,
                gust: Gust::STILL,
                sampling: Sampling::Nearest,
                painted: crate::material::Painted::Kept,
            })
        });

    let body = store.bodies.add(material::Body {
        indices,
        palette: colours,
        pass: material::Pass::Solid,
        sway: Sway::Still,
        gust: Gust::STILL,
        sampling: Sampling::Nearest,
        ..material::Body::default()
    });

    Some((Picture::Indexed(body), band, None))
}

/// One frame of a layer. Frames differ in size and anchor, so a cycling
/// layer moves its quad.
#[derive(Clone)]
pub struct Cel {
    /// What draws the colours.
    pub body: Picture,
    /// What draws the shadow, absent on a frame that casts none.
    pub band: Option<Handle<material::Band>>,
    /// The band's indices where a model casts the ground shadow: what is
    /// left to draw, a flag or a fire's smoke ([`keep_parts`]). Absent if
    /// nothing is.
    pub aloft: Option<Handle<Image>>,
    /// A translucent frame's lift, clearing the shadow's coverage under it
    /// ([`crate::soft::lifts`]); absent on an opaque one.
    pub lift: Option<Handle<material::Body>>,
    /// Tile size in world units: the quad's scale.
    pub size: Vec2,
    /// The anchor as an offset from the canvas centre.
    pub anchor: Vec2,
    /// The tile drawn: archive, index.
    pub tile: (usize, usize),
}

/// One layer of a descriptor's picture, ready to place.
#[derive(Clone)]
pub struct Drawing {
    /// Frames in playback order; one if held. `None` for unresolved tiles,
    /// keeping the cycle length.
    pub cels: Vec<Option<Cel>>,
    /// The pose offset, y negated.
    pub pose: Vec2,
    /// Layer index from the set id's top byte, not list position (dropped
    /// layers would shift it). See [`regent_core::content::imagery::Animation::variant`].
    pub layer: u8,
    /// How far right of the anchor the picture reaches ([`Maps::reach`]).
    pub right: f32,
    /// Game ms each frame shows: [`frame_ms`] of the set.
    pub ms: u32,
    /// Screen pixels its part sorts nearer
    /// ([`regent_core::content::imagery::Animation::listed`]).
    pub nudge: i32,
    /// Its part's pass: the second stands on the ground under the thing.
    pub pass: Pass,
    /// Its part's index in the piece, layers and passes counted.
    pub part: usize,
}

impl Drawing {
    /// Every layer of the thing's set, lowest first.
    ///
    /// A repeated set kind is a stack (Palace `Active`: building on 0, banner on
    /// 1); see [`regent_core::content::unit::Unit::stack`]. The set comes from
    /// [`Item::set`]. Layers become materials so
    /// shadows land on whatever is below.
    pub(super) fn of(
        corpus: &Corpus,
        cel: &mut dyn FnMut(FrameSource) -> Option<Cel>,
        object: &Item,
        moves: Mobility,
        still: Option<u16>,
    ) -> Vec<Self> {
        let Some(Ok(unit)) = regent_core::content::unit::Unit::find(&corpus.catalogue, object.id)
        else {
            return Vec::new();
        };

        let Some(named) = named_set(&unit, object) else {
            return Vec::new();
        };

        Self::layers(
            &corpus.catalogue,
            &unit,
            named,
            &|slots| moves.slot(object, slots),
            still,
            cel,
        )
    }

    /// Every layer of set `named` of `unit`, lowest first; `slot` picks a
    /// pose of the set's slots, `still` a frame or the whole stream. A
    /// layer of several streams gives one drawing a stream, in order, each
    /// frame at its nudge ([`regent_core::content::imagery::Pose::blits`]); `cel` makes a
    /// frame's materials ([`cel`], or a cache of them). The slot effects
    /// of `catalogue` follow the layers.
    pub fn layers(
        catalogue: &Catalogue<'_>,
        unit: &regent_core::content::unit::Unit<'_>,
        named: u16,
        slot: &dyn Fn(usize) -> usize,
        still: Option<u16>,
        cel: &mut dyn FnMut(FrameSource) -> Option<Cel>,
    ) -> Vec<Self> {
        let base = unit.stack(named).next();

        // A part per layer and pass, numbered as `UnitIMG` makes them.
        let parts: Vec<(usize, &Animation, Pass, &Pose)> = unit
            .stack(named)
            .flat_map(|set| set.passes.iter().map(move |pass| (set, *pass)))
            .enumerate()
            .filter_map(|(part, (set, pass))| {
                let slot = slot(set.poses.len());
                let pose = set.poses.get(slot).or_else(|| set.poses.first())?;

                Some((part, set, pass, pose))
            })
            .collect();
        let count = unit.stack(named).map(|set| set.passes.len()).sum::<usize>();

        // Then a part per slot effect of each first-pass part, last entry
        // first: the set's slot, at the entry's offset.
        let effects: Vec<(Animation, usize, (i32, i32))> = parts
            .iter()
            .filter(|(_, _, pass, _)| *pass == Pass::First)
            .flat_map(|(.., pose)| pose.effects.iter().rev())
            .filter_map(|entry| {
                let set = imagery::effect(catalogue, entry)?;
                let at = if entry.fixed_slot() {
                    entry.slot() as usize
                } else {
                    slot(set.poses.len())
                };

                Some((set, at, entry.offset()))
            })
            .collect();

        // A new effect part starts on frame 0.
        let effect_parts = effects
            .iter()
            .enumerate()
            .filter_map(|(nth, (set, at, offset))| {
                let pose = set.poses.get(*at)?;

                Some((
                    count + nth,
                    set,
                    Pass::First,
                    pose,
                    *offset,
                    still.map(|_| 0),
                ))
            });

        parts
            .iter()
            .map(|&(part, set, pass, pose)| (part, set, pass, pose, (0, 0), still))
            .chain(effect_parts)
            .flat_map(|(part, set, pass, pose, offset, still)| {
                (0..pose.streams.max(1)).map(move |s| (part, set, pass, pose, offset, still, s))
            })
            .filter_map(|(part, set, pass, pose, (x, y), still, stream)| {
                let blits = pose.blits(stream, pass);

                // Saved frame or whole stream; missing frames fall back to the first.
                let wanted: Vec<Option<Blitted>> = match still {
                    Some(at) => vec![*blits.get(usize::from(at)).or_else(|| blits.first())?],
                    None => blits,
                };

                let cels: Vec<Option<Cel>> = wanted
                    .into_iter()
                    .map(|blit| nudged(&mut *cel, blit?))
                    .collect();

                // Skip all-hole layers. The first resolved frame sizes it.
                let first = cels.iter().flatten().next()?;
                let right = first.size.x / 2.0 - first.anchor.x;

                #[allow(clippy::cast_precision_loss, reason = "screen pixels")]
                let pose = Vec2::new(
                    (i32::from(pose.x) + x) as f32,
                    -(i32::from(pose.y) + y) as f32,
                );

                Some(Drawing {
                    layer: set.variant,
                    cels,
                    pose,
                    right,
                    ms: frame_ms(set.playback),
                    nudge: base.map_or(set.nudge, |b| set.listed(b)),
                    pass,
                    part,
                })
            })
            .collect()
    }

    /// Its pose offset as drawn: every part but the first takes its own,
    /// so a missile's shadow does, its body not.
    #[must_use]
    pub const fn posed(&self) -> Vec2 {
        if self.part > 0 { self.pose } else { Vec2::ZERO }
    }
}

/// Each layer's place in its piece's draw order, 0 drawn first: by
/// the nudge its part sorts at, then layer order, as `IsoDrawList`
/// lists parts (a tie goes after). The Mausoleum's dirt diamond, layer
/// 1 at -1,000, draws under layer 0.
#[must_use]
pub fn rungs(layers: &[Drawing]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..layers.len()).collect();
    let mut rungs = vec![0; layers.len()];

    order.sort_by_key(|&i| layers[i].nudge);

    for (rung, i) in order.into_iter().enumerate() {
        rungs[i] = rung;
    }

    rungs
}

/// A blitted frame as materials, its anchor moved by its nudge.
fn nudged(cel: &mut dyn FnMut(FrameSource) -> Option<Cel>, blit: Blitted) -> Option<Cel> {
    let source = FrameSource::Tile {
        archive: blit.archive,
        index: blit.index,
        flags: 0,
    };
    let (x, y) = blit.nudge;
    let mut cel = cel(source)?;

    cel.anchor -= Vec2::new(f32::from(x), -f32::from(y));
    Some(cel)
}

/// Fill [`Cel::aloft`] on a piece's `drawings` in set `set`, standing on
/// `stands`: a damage stage's overlays (flames and smoke on layers over
/// the building) whole, any other frame's pixels [`Frame::aloft`].
pub fn keep_parts(
    drawings: &mut [Drawing],
    (set, stands): (u16, Option<ArtRect>),
    corpus: &Corpus,
    store: &mut Store,
) {
    let damage = SetKind::from_id(set) == SetKind::Die;

    for drawing in drawings {
        let overlay = damage && drawing.part > 0 && drawing.pass == Pass::First;
        let posed = drawing.posed();

        for cel in drawing.cels.iter_mut().flatten() {
            let Some(band) = &cel.band else { continue };

            cel.aloft = if overlay {
                store.bands.get(band).map(|band| band.indices.clone())
            } else {
                stands
                    .and_then(|stands| {
                        let (archive, index) = cel.tile;
                        let frame = Frame::at(&corpus.catalogue, archive, index)?.ok()?;
                        let moved = posed + texture::anchor(&frame) - cel.anchor;

                        #[allow(clippy::cast_possible_truncation, reason = "whole art pixels")]
                        let shift = (moved.x as i32, -moved.y as i32);

                        texture::without_shadow(&frame, |i| frame.aloft(i, stands, shift))
                    })
                    .map(|image| store.images.add(image))
            };
        }
    }
}

/// One frame of a layer as materials. `None` for tiles past the archive
/// end, sentinels, frames the unit draw blits no tile for
/// ([`FrameSource::tile`]), or zero-sized canvases.
pub fn cel(
    corpus: &Corpus,
    store: &mut Store,
    palettes: &mut Palettes,
    source: FrameSource,
) -> Option<Cel> {
    let (archive, index) = source.tile()?;

    let frame = Frame::at(&corpus.catalogue, archive, index)?.ok()?;
    let anchor = texture::anchor(&frame);
    let size = Vec2::new(f32::from(frame.width), f32::from(frame.height));
    let (body, band, lift) = picture(store, palettes, &frame, (archive, index))?;

    Some(Cel {
        body,
        band,
        aloft: None,
        lift,
        size,
        anchor,
        tile: (archive, index),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::data::fourcc::FourCc;

    /// The Magic Bazaar's `Die-6` draws its two layers, then its five
    /// `FXa1` fires, last entry first: entry 0, the last part, at
    /// (-70, -4) y down, sorted with the base, 100 ms a frame.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_slot_effect_is_a_part_after_the_layers() {
        let mut paths = crate::common::archives(&crate::common::hd().join("Data"));

        paths.extend(crate::common::archives(&crate::common::hd().join("DataMX")));

        let corpus = Corpus::open(&paths).expect("HD's archives");
        let catalogue = &corpus.catalogue;
        let bazaar = regent_core::content::unit::Unit::find(catalogue, FourCc(*b"ABl1"))
            .expect("the Bazaar")
            .expect("it resolves");
        let mut cel = |source: FrameSource| {
            Some(Cel {
                body: Picture::Colour(Handle::default()),
                band: None,
                aloft: None,
                lift: None,
                size: Vec2::ONE,
                anchor: Vec2::ZERO,
                tile: source.tile()?,
            })
        };
        let drawn = Drawing::layers(catalogue, &bazaar, 0x65, &|_| 0, None, &mut cel);
        let fires = &drawn[2..];

        assert_eq!(drawn.len(), 7);
        assert_eq!(
            fires.iter().map(|d| d.part).collect::<Vec<_>>(),
            [2, 3, 4, 5, 6]
        );
        assert_eq!(fires[4].pose, Vec2::new(-70.0, 4.0));
        assert_eq!(fires[0].pose, Vec2::new(12.0, -5.0));
        assert!(
            fires
                .iter()
                .all(|d| d.ms == 100 && d.nudge == drawn[0].nudge)
        );
        assert!(fires.iter().all(|d| d.cels.len() >= 20));
    }
}
