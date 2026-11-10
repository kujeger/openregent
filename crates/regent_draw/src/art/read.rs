//! Pictures read from the archives: a sprite's pixels as interface art
//! draws them, and the [`Read`] the service takes originals from.

use bevy_egui::egui::{Color32, ColorImage};
use regent_core::content::imagery;
use regent_core::content::imagery::FrameSource;
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::imag::{INTERFACE_PANEL, INTERFACE_PORTRAIT};
use regent_core::data::payload::Rgb;
use regent_core::view::panel::portrait;
use regent_core::view::picture_file::Key;
use regent_core::view::raster::blend;
use regent_core::view::raster::shade::LEVELS;
use regent_core::view::sprite::Colour;
use regent_core::view::{Ink, Shade, Sprite};

use super::{ArtSource, Pic, Picture, Tile, stack};
use crate::soft::{self, Blending};

/// How much of a source a [`Read`] holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Every picture: one it lacks is not there.
    Whole,
    /// Those read ahead: another reader may hold more.
    Part,
}

/// Where the service reads a picture's original pixels.
pub trait Read {
    /// `picture` from `source` as read, untoned; `None` where it is not.
    fn read(&self, source: ArtSource, picture: Picture) -> Option<ColorImage>;

    /// How much of a source it holds.
    fn reach(&self) -> Reach;

    /// The override files' names for `picture` from `source`
    /// (`picture_file`), the first found used; none where it knows none.
    fn keys(&self, _source: ArtSource, _picture: Picture) -> Vec<Key> {
        Vec::new()
    }
}

/// A game's catalogue and its source: the panels' pictures and portraits.
impl Read for (ArtSource, &Catalogue<'_>) {
    fn read(&self, source: ArtSource, picture: Picture) -> Option<ColorImage> {
        let (own, catalogue) = *self;
        let unit = |code| Unit::find(catalogue, code)?.ok();

        match picture {
            _ if source != own => None,
            Picture::Pic(pic) => sprite(catalogue, pic).map(|s| image(&s)),
            Picture::Icon(code) => Some(image(
                &portrait::icon_source(catalogue, &unit(code)?)?.ok()?,
            )),
            Picture::Tile((archive, index)) => Sprite::at(catalogue, archive, index)?
                .ok()
                .map(|s| image(&s)),
            Picture::Stack(s) => stack::stacked(catalogue, s),
        }
    }

    fn reach(&self) -> Reach {
        Reach::Whole
    }

    fn keys(&self, source: ArtSource, picture: Picture) -> Vec<Key> {
        let (own, catalogue) = *self;
        // A list icon's is its portrait's, as it draws it.
        let icon = |code| {
            let unit = Unit::find(catalogue, code)?.ok()?;

            portrait::icon_source(catalogue, &unit)?.ok()?;
            interface_key(catalogue, &unit, INTERFACE_PORTRAIT)
        };

        let key = match picture {
            _ if source != own => None,
            Picture::Pic(pic) => pic_key(catalogue, pic),
            Picture::Icon(code) => icon(code),
            Picture::Tile((archive, index)) => catalogue
                .archives
                .get(archive)
                .map(|(file, _)| Key::tile(file, index)),
            Picture::Stack(_) => None,
        };

        key.into_iter().collect()
    }
}

/// The override name of `pic` in `catalogue`: a set's frame in the
/// archive its `IMAG` is found in; a portrait or backdrop its unit's
/// interface set's first frame.
#[must_use]
pub fn pic_key(catalogue: &Catalogue<'_>, pic: Pic) -> Option<Key> {
    let unit = |code| Unit::find(catalogue, code)?.ok();

    match pic {
        Pic::Set {
            record,
            set,
            slot,
            frame,
        } => Key::of(catalogue, record, u32::try_from(set).ok()?, slot, frame),
        Pic::Portrait(code) => interface_key(catalogue, &unit(code)?, INTERFACE_PORTRAIT),
        Pic::Backdrop(code) => interface_key(catalogue, &unit(code)?, INTERFACE_PANEL),
    }
}

/// The first frame of `unit`'s interface set `set`, in its `IMAG`'s
/// archive, as `portrait::portrait` draws it.
fn interface_key(catalogue: &Catalogue<'_>, unit: &Unit<'_>, set: u16) -> Option<Key> {
    let art = unit.art?;
    let (file, _) = catalogue.archives.get(art.at?.archive)?;
    let found = unit.interface(set)?;

    Some(Key::frame(file, art.id, found.id, 0, 0))
}

/// The tiles a run of frames draws; a frame of another kind is none
/// (`FrameSource::tile`).
#[must_use]
pub fn tiles(frames: &[FrameSource]) -> Vec<Tile> {
    frames.iter().filter_map(FrameSource::tile).collect()
}

/// `pic`'s sprite from `catalogue`.
#[must_use]
pub fn sprite(catalogue: &Catalogue<'_>, pic: Pic) -> Option<Sprite> {
    let unit = |code| Unit::find(catalogue, code)?.ok();

    match pic {
        Pic::Set {
            record,
            set,
            slot,
            frame,
        } => {
            let sets = imagery::sets(catalogue, catalogue.find(b"IMAG", record)?).ok()?;
            let set = u32::try_from(set).ok()?;
            let found = sets.iter().find(|a| a.id == set)?;
            let pose = found.poses.get(slot)?;
            let source = pose.frames.get(frame).or_else(|| pose.frames.first())?;
            // A frame of another kind is no picture.
            let (archive, index) = source.tile()?;

            Sprite::at(catalogue, archive, index)?.ok()
        }
        Pic::Portrait(code) => portrait::portrait(catalogue, &unit(code)?)?.ok(),
        Pic::Backdrop(code) => portrait::backdrop(catalogue, &unit(code)?)?.ok(),
    }
}

/// A sprite as interface art draws it: its shadow and glow indices, as
/// its draw mode reads them (`Sprite::as_drawn`), a translucent black or
/// white over what is beneath. Black at soft shadows' share
/// ([`soft::veil`]): egui blends on linear colour, so it darkens as a soft
/// shadow; white stands in for a raise (**Inferred**). The summary's
/// porthole ring shades the still's rim so. A tile in
/// draw mode 5 or 8 is see-through by index (`blend::alphas`): the quest
/// map's cloud glows.
#[must_use]
pub fn image(sprite: &Sprite) -> ColorImage {
    let size = [usize::from(sprite.width), usize::from(sprite.height)];
    let alphas = match &sprite.colour {
        Colour::Indexed { indices, palette } => {
            blend::alphas(sprite.mode, sprite.level, palette).map(|a| (indices, a))
        }
        Colour::Direct { .. } => None,
    };
    let pixels = (0..sprite.covered.len())
        .map(|i| {
            let drawn = sprite.as_drawn(i);

            match (alphas, drawn) {
                (None, _) => ink(drawn),
                (Some((indices, a)), Ink::Colour(Rgb { r, g, b })) => {
                    Color32::from_rgba_unmultiplied(r, g, b, a[usize::from(indices[i])])
                }
                (Some(_), _) => Color32::TRANSPARENT,
            }
        })
        .collect();

    ColorImage::new(size, pixels)
}

/// One pixel's ink as a colour.
#[must_use]
pub fn ink(ink: Ink) -> Color32 {
    let alpha = |share: u32| u8::try_from(share.min(u32::from(u8::MAX))).unwrap_or(u8::MAX);

    match ink {
        Ink::Hole => Color32::TRANSPARENT,
        Ink::Colour(Rgb { r, g, b }) => Color32::from_rgb(r, g, b),
        Ink::Shade(shade @ Shade::Keep(_)) => {
            Color32::from_black_alpha(soft::veil(shade, Blending::Linear).unwrap_or(0))
        }
        Ink::Shade(Shade::Raise(steps)) => Color32::from_white_alpha(alpha(255 * steps / LEVELS)),
    }
}
