//! A resolved sprite, as something the GPU can sample.
//!
//! Two paths:
//!
//! - Flattened on the CPU ([`image`]): drawn into a [`regent_core::view::Sheet`]
//!   over a [`Backdrop`]. Not `Sprite::rgba`, which colours shadow indices 247
//!   to 254; [`Blit`] handles them, a shadow by its factor
//!   ([`crate::soft::shaded`]). With [`Backdrop::None`] shadows draw
//!   nothing. Used by the browser and map icons.
//! - Indexed ([`indices`], [`palette`]): the tile as stored, one byte per
//!   pixel, so the shadow pass can shade whatever the map drew below.
//!
//! Nearest sampling throughout: bilinear smears edges, and indices cannot be
//! averaged.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use regent_core::data::imag::Blit;
use regent_core::data::payload::Palette;
use regent_core::view::raster::shade;
use regent_core::view::{Backdrop, Colour, Ink, Shade, Sheet, Sprite as Frame};

use crate::soft;

/// One frame as a texture, drawn over `under` by `blit`, a shadow by
/// [`crate::soft`]'s factors.
///
/// `None` for a zero-sized canvas (26 in the base dataset); the GPU refuses it.
pub fn image(frame: &Frame, under: Backdrop, blit: Blit) -> Option<Image> {
    let mut composed = Sheet::blank(usize::from(frame.width), usize::from(frame.height));

    composed.draw_with(frame, (0, 0), under, blit, soft::shaded);

    sheet(&composed)
}

/// A frame's palette indices, one byte a pixel, exactly as the tile holds them.
///
/// `None` for a direct-colour frame; on the map only the Fairgrounds' ruin
/// (`a_units_frames_are_indexed_bar_the_fairgrounds_ruin`), which is flattened.
///
/// Holes become [`shade::TRANSPARENT`] (255), which the blitter never draws.
/// Returns the palette too.
pub fn indices(frame: &Frame) -> Option<(Image, &Palette)> {
    let Colour::Indexed { indices, palette } = &frame.colour else {
        return None;
    };

    let data: Vec<u8> = indices
        .iter()
        .zip(&frame.covered)
        .map(|(index, covered)| if *covered { *index } else { shade::TRANSPARENT })
        .collect();

    let image = plain(
        u32::from(frame.width),
        u32::from(frame.height),
        data,
        TextureFormat::R8Uint,
    )?;

    Some((image, palette))
}

/// `frame`'s indices without its ground shadow, for a band to draw where a
/// model casts it: a hole where `keep` says no to a darkening pixel. `None`
/// if it says yes to none.
#[must_use]
pub fn without_shadow(frame: &Frame, keep: impl Fn(usize) -> bool) -> Option<Image> {
    let (mut image, _) = indices(frame)?;
    let (kept, cut): (Vec<usize>, Vec<usize>) = (0..frame.covered.len())
        .filter(|i| matches!(frame.as_drawn(*i), Ink::Shade(Shade::Keep(_))))
        .partition(|i| keep(*i));
    let data = image.data.as_mut()?;

    for i in cut {
        data[i] = shade::TRANSPARENT;
    }

    (!kept.is_empty()).then_some(image)
}

/// A palette as a 256 by 1 texture, one texel an index.
///
/// Uploaded once per palette, not per frame. Sharing: `docs/formats/payloads.md`.
pub fn palette(colours: &Palette) -> Option<Image> {
    let data: Vec<u8> = colours
        .colours
        .iter()
        .flat_map(|c| [c.r, c.g, c.b, u8::MAX])
        .collect();

    plain(256, 1, data, TextureFormat::Rgba8UnormSrgb)
}

/// A translucent tile's palette: each index's colour at its alpha
/// ([`regent_core::view::raster::blend::alphas`]).
pub fn glass(colours: &Palette, alphas: &[u8; 256]) -> Option<Image> {
    let data: Vec<u8> = colours
        .colours
        .iter()
        .zip(alphas)
        .flat_map(|(c, a)| [c.r, c.g, c.b, *a])
        .collect();

    plain(256, 1, data, TextureFormat::Rgba8UnormSrgb)
}

/// A frame's anchor as an offset from its canvas centre, y up: rows
/// count down. A sprite is placed by it.
pub fn anchor(frame: &Frame) -> Vec2 {
    Vec2::new(
        f32::from(frame.anchor.0) - f32::from(frame.width) / 2.0,
        f32::from(frame.height) / 2.0 - f32::from(frame.anchor.1),
    )
}

/// A composed sheet as a texture.
///
/// `None` for an empty sheet, which the GPU refuses.
pub fn sheet(sheet: &Sheet) -> Option<Image> {
    let (Ok(width), Ok(height)) = (u32::try_from(sheet.width), u32::try_from(sheet.height)) else {
        return None;
    };

    plain(
        width,
        height,
        sheet.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
    )
}

/// One image, nearest sampled, in both worlds.
///
/// `None` for a zero-sized canvas. Colours use an sRGB format: palette
/// triples are sRGB.
fn plain(width: u32, height: u32, data: Vec<u8>, format: TextureFormat) -> Option<Image> {
    if width == 0 || height == 0 {
        return None;
    }

    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    image.sampler = ImageSampler::nearest();

    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    use regent_core::data::payload::{Palette, Rgb};
    use regent_core::view::{Colour, PaletteSource};

    /// A two-pixel-tall sprite whose rows differ, so a vertical flip shows up.
    ///
    /// Hand-built: these tests cover conversion; the library tests decode.
    fn frame() -> Frame {
        let mut colours = [Rgb { r: 0, g: 0, b: 0 }; 256];
        colours[1] = Rgb {
            r: 200,
            g: 10,
            b: 20,
        };
        colours[2] = Rgb {
            r: 20,
            g: 10,
            b: 200,
        };

        Frame {
            width: 1,
            height: 2,
            version: 3,
            anchor: (0, 0),
            // Top pixel drawn, bottom transparent.
            covered: vec![true, false],
            colour: Colour::Indexed {
                indices: vec![1, 2],
                palette: Box::new(Palette { colours }),
            },
            source: PaletteSource::Embedded,
            mode: regent_core::data::tile::Mode::Band,
            level: 0,
        }
    }

    #[test]
    fn a_frame_becomes_a_texture_of_its_own_size() {
        let image = image(&frame(), Backdrop::None, Blit::World).expect("a 1x2 canvas");

        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 2);
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb
        );
    }

    #[test]
    fn the_first_pixel_is_the_top_row_and_not_the_bottom() {
        // Most sprite art hides a flip.
        let data = image(&frame(), Backdrop::None, Blit::World)
            .unwrap()
            .data
            .expect("pixel data");

        assert_eq!(&data[0..4], &[200, 10, 20, 255], "top row moved");
    }

    #[test]
    fn an_uncovered_pixel_is_transparent_rather_than_black() {
        // Coverage is separate from the index: index 2 is a colour.
        let data = image(&frame(), Backdrop::None, Blit::World)
            .unwrap()
            .data
            .expect("pixel data");

        assert_eq!(&data[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn red_does_not_arrive_as_blue() {
        // A channel swap is hard to spot on this game's grey art.
        let data = image(&frame(), Backdrop::None, Blit::World)
            .unwrap()
            .data
            .expect("pixel data");

        assert_eq!(data[0], 200, "red channel");
        assert_eq!(data[2], 20, "blue channel");
    }

    #[test]
    fn a_zero_sized_canvas_makes_no_texture() {
        let mut empty = frame();
        empty.width = 0;
        empty.covered.clear();

        assert!(image(&empty, Backdrop::None, Blit::World).is_none());
    }
}
