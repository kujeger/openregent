//! How a picture is drawn: its tone, and how it fills a larger
//! rectangle.

use bevy_egui::egui::{Color32, ColorImage};
use regent_core::view::menu::faces;
use regent_core::view::panel::spell_bar;

use super::scale::Edge;

/// How a picture is toned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    Plain,
    /// A grave's portrait: grey and darker (**Game**: `ui_ranger_grave.png`,
    /// `ui_giant_spider_grave.png`; **Inferred**: how the engine tones it).
    Grave,
    /// A button that cannot be pressed: darker (**Inferred**).
    Dim,
    /// A one-slot spell icon not castable: grey inside its frame
    /// (`spell_bar::grey`), as a full set's slot 2.
    Unlit,
    /// A one-slot spell icon hovered: its frame lit
    /// (`spell_bar::LIT_FRAME`), as a full set's slot 6.
    Lit,
    /// A level 5 weapon or armour's picture, which the art lacks: level
    /// 1's grey ramped to purple as levels 2 to 4 ramp to copper, steel
    /// and gold (openregent's, `docs/openregent/enhancements.md`).
    Purple,
    /// The faces behind the menus: [`faces::sepia`].
    Sepia,
}

/// How a picture fills a rectangle larger than itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wrap {
    /// Drawn once.
    Once,
    /// Repeated: the faces behind, the frames' tiled bars.
    Tiled,
}

impl Wrap {
    /// How xBRZ continues the picture's edges.
    pub const fn edge(self) -> Edge {
        match self {
            Wrap::Once => Edge::Clamp,
            Wrap::Tiled => Edge::Repeat,
        }
    }
}

/// A grave's grey keeps this share of the brightness (**Inferred**, by
/// eye against the captures).
const GRAVE_LIGHT: f32 = 0.75;

/// A dimmed picture keeps this share of its colour (**Inferred**).
const DIM_LIGHT: f32 = 0.45;

/// [`Tone::Purple`]'s colour at [`PURPLE_AT`] of full brightness: below
/// it a ramp up from black, above it one up to white, as the art's own
/// levels run (the user's pick, by eye, over orange: too like bronze
/// alone in the panel).
const PURPLE: [f32; 3] = [150.0, 60.0, 220.0];

/// Where the ramp reaches [`PURPLE`]: about where the gold frames' mid
/// tones are.
const PURPLE_AT: f32 = 0.6;

/// Grey brightness 0 to 1 ramped through [`PURPLE`]:
///
/// ```text
///  0 ──── black ──── 0.6 ──── purple ──── 1 ──── white
/// ```
fn purple(light: f32) -> [f32; 3] {
    PURPLE.map(|key| {
        if light < PURPLE_AT {
            key * light / PURPLE_AT
        } else {
            key + (255.0 - key) * (light - PURPLE_AT) / (1.0 - PURPLE_AT)
        }
    })
}

/// `image` in `tone`.
#[must_use]
pub fn toned(image: ColorImage, tone: Tone) -> ColorImage {
    let size = image.size;

    toned_as(image, tone, size)
}

/// `image`, standing for a `base`-sized picture (an override at its own
/// size), in `tone`: a spell icon's frame is where the base's would be.
#[must_use]
pub fn toned_as(mut image: ColorImage, tone: Tone, base: [usize; 2]) -> ColorImage {
    let size = (image.size[0].max(1), image.size[1].max(1));
    let on_frame = |(x, y): (usize, usize)| {
        let at = (x * base[0] / size.0, y * base[1] / size.1);

        spell_bar::on_frame(at, (base[0], base[1]))
    };
    let shade = |c: Color32, at: (usize, usize)| match tone {
        Tone::Unlit if !on_frame(at) => {
            let [red, green, blue, alpha] = c.to_srgba_unmultiplied();
            let level = spell_bar::grey([red, green, blue].into());

            Color32::from_rgba_unmultiplied(level, level, level, alpha)
        }
        Tone::Lit if on_frame(at) => {
            let [red, green, blue] = spell_bar::LIT_FRAME.channels();

            Color32::from_rgba_unmultiplied(red, green, blue, c.a())
        }
        Tone::Plain | Tone::Unlit | Tone::Lit => c,
        Tone::Grave => {
            let [red, green, blue, alpha] = c.to_srgba_unmultiplied();
            let grey = (0.30 * f32::from(red) + 0.59 * f32::from(green) + 0.11 * f32::from(blue))
                * GRAVE_LIGHT;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a channel"
            )]
            let level = grey.round().clamp(0.0, 255.0) as u8;

            Color32::from_rgba_unmultiplied(level, level, level, alpha)
        }
        Tone::Purple => {
            let [red, green, blue, alpha] = c.to_srgba_unmultiplied();
            let light =
                (0.30 * f32::from(red) + 0.59 * f32::from(green) + 0.11 * f32::from(blue)) / 255.0;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a channel"
            )]
            let [r, g, b] = purple(light).map(|v| v.round().clamp(0.0, 255.0) as u8);

            Color32::from_rgba_unmultiplied(r, g, b, alpha)
        }
        Tone::Sepia => {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            let [r, g, b] = faces::sepia([r, g, b].into()).channels();

            Color32::from_rgba_unmultiplied(r, g, b, a)
        }
        Tone::Dim => {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a channel"
            )]
            let dim = |v: u8| (f32::from(v) * DIM_LIGHT).round() as u8;

            Color32::from_rgba_unmultiplied(dim(r), dim(g), dim(b), a)
        }
    };

    for (i, pixel) in image.pixels.iter_mut().enumerate() {
        *pixel = shade(*pixel, (i % size.0, i / size.0));
    }

    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_graves_picture_is_grey_and_darker() {
        let image = ColorImage::new([1, 1], vec![Color32::from_rgb(200, 100, 50)]);
        let [r, g, b, _] = toned(image, Tone::Grave).pixels[0].to_array();

        assert_eq!((r, g), (g, b));
        assert!(r < 120, "{r}");
    }

    #[test]
    fn purple_runs_from_black_through_purple_to_white() {
        let at = |level: u8| {
            let image = ColorImage::new([1, 1], vec![Color32::from_gray(level)]);

            toned(image, Tone::Purple).pixels[0].to_array()
        };

        assert_eq!(at(0), [0, 0, 0, 255]);
        assert_eq!(at(153), [150, 60, 220, 255]);
        assert_eq!(at(255), [255, 255, 255, 255]);
    }

    /// A spell icon's lit frame on a 2× override is two pixels wide, as
    /// the base's one pixel covers.
    #[test]
    fn a_larger_overrides_frame_is_where_the_bases_is() {
        let image = ColorImage::filled([8, 8], Color32::BLACK);
        let lit = toned_as(image, Tone::Lit, [4, 4]);
        let framed = |x: usize| lit.pixels[3 * 8 + x] != Color32::BLACK;

        assert_eq!(
            (0..8).map(framed).collect::<Vec<_>>(),
            [true, true, false, false, false, false, true, true]
        );
    }
}
