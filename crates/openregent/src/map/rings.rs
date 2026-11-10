//! Selection rings drawn as shapes, `[enhancements] rings = "native"` or
//! `"modern"`: the band the library fits to the ring's picture
//! (`regent_core::view::map::ring::band`), drawn by `regent_draw::ring`. Which piece
//! takes which ring, its slot and place stay the art's
//! (`select`, `preview`).
//!
//! `[enhancements] side_ring`: the hover ring in its piece's side colour
//! ([`side_colour`], [`sided`]): `native` and `modern` tint their band;
//! `art`, whose tile takes no tint, draws the side line instead.

use bevy::prelude::*;

use regent_core::data::payload::Rgb;
use regent_core::sim::world::TEAMS;
use regent_core::view::map::ring::band::{Band, Paint};
use regent_core::view::map::ring::{Ring, Why};
use regent_core::view::panel::minimap::Looks;
use regent_core::view::raster::colour::quantise;
use regent_draw::data::Corpus;
use regent_draw::ring::{self, Ellipse, Rings};

/// A ring's native quads, in draw order: each material, and its place
/// about the piece's anchor, world units, y up, scaled to the band's box.
#[derive(Clone)]
pub struct Native(pub Vec<(Handle<Ellipse>, Transform)>);

/// `ring` drawn as its band `with` a look; `None` without one (a flag's
/// brackets), or with the art's.
pub fn native(
    corpus: &Corpus,
    ellipses: &mut Assets<Ellipse>,
    ring: &Ring,
    with: Rings,
) -> Option<Native> {
    let band = ring.band(&corpus.catalogue)?.ok()?;

    placed(ellipses, &band, ring::drawn(&band, with))
}

/// `ring` in side colour `colour`, drawn `with` a look: its band
/// recoloured, or under [`Rings::Art`] the side line ([`ring::side`]);
/// `None` without a band (a flag's brackets).
pub fn sided(
    corpus: &Corpus,
    ellipses: &mut Assets<Ellipse>,
    ring: &Ring,
    (with, colour): (Rings, Rgb),
) -> Option<Native> {
    let band = Band {
        paint: Paint::Colour(colour),
        ..ring.band(&corpus.catalogue)?.ok()?
    };
    let drawn = match with {
        Rings::Art => {
            let linear = Color::srgb_u8(colour.r, colour.g, colour.b).to_linear();

            vec![ring::side(&band, linear.to_vec4())]
        }
        Rings::Native | Rings::Modern => ring::drawn(&band, with),
    };

    placed(ellipses, &band, drawn)
}

/// The side colour a ring `why` drawn takes under a piece owned by
/// `owner`: the hover ring alone, its hit point bar's fill (`view::map::bar`,
/// `GEPlayerColors`), the crown blue, the monsters yellow. `None`
/// for the selection, or no player's piece.
pub fn side_colour(looks: &Looks, (why, owner): (Why, u32)) -> Option<Rgb> {
    u8::try_from(owner)
        .ok()
        .filter(|o| why == Why::Hovered && usize::from(*o) < TEAMS)
        .map(|o| quantise(looks.colour(o)))
}

/// `drawn`, `band`'s quads, placed about the piece's anchor; `None` for
/// none.
fn placed(ellipses: &mut Assets<Ellipse>, band: &Band, drawn: Vec<Ellipse>) -> Option<Native> {
    let (cx, cy) = band.centre;

    // Each layer a step nearer, so it draws after the one before.
    #[allow(clippy::cast_precision_loss, reason = "two layers")]
    let quads: Vec<_> = drawn
        .into_iter()
        .enumerate()
        .map(|(nth, ellipse)| {
            let put = Transform {
                translation: Vec3::new(cx, -cy, nth as f32 * ring::LIFT_STEP),
                scale: ellipse.shape.size.extend(1.0),
                ..default()
            };

            (ellipses.add(ellipse), put)
        })
        .collect();

    (!quads.is_empty()).then_some(Native(quads))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player's piece's hover ring takes its bar's colour: the crown
    /// blue, the first enemy red, the monsters yellow; no player's, and
    /// the selection, none.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_side_colour_is_its_owners_bar_colour() {
        let (_, catalogue) = crate::game::begin::test_game(&[]);
        let looks = Looks::new(&catalogue);
        let side = |owner| side_colour(&looks, (Why::Hovered, owner)).expect("a player's");
        let crown = side(0);
        let enemy = side(1);
        let monsters = side(7);

        assert!(crown.b > crown.r && crown.b > crown.g, "{crown:?}");
        assert!(enemy.r > enemy.g && enemy.r > enemy.b, "{enemy:?}");
        assert!(
            monsters.r > monsters.b && monsters.g > monsters.b,
            "{monsters:?}"
        );
        assert_eq!(crown, quantise(looks.colour(0)));
        assert_eq!(side_colour(&looks, (Why::Hovered, 8)), None);
        assert_eq!(side_colour(&looks, (Why::Hovered, u32::MAX)), None);
        assert_eq!(side_colour(&looks, (Why::Selected, 0)), None);
    }
}
