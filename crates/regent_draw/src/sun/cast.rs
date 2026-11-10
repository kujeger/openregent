//! The sun the models' shadows fall from: its direction fitted to the
//! art (`docs/original/art-camera.md`), taken into the world frame through
//! [`frame::ART`], where Bevy's directional light shines along it.
//!
//! ```text
//!        sun
//!          \      a model's top
//!           \  ┌──┐
//!            \ │  │
//!             \│  │   a point lower on the same ray: shaded
//!   ───────────┴──┴──────────
//! ```

use bevy::prelude::*;

use crate::frame;

/// The sun, in the art's terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    /// Degrees from screen right toward the viewer: φ. Shadows fall the
    /// other way.
    pub direction: f32,
    /// Degrees above the ground.
    pub elevation: f32,
    /// The sun's apparent diameter, degrees: a penumbra widens by its
    /// tangent a world unit along the light from its caster, so a
    /// shadow is sharp at its caster's foot and soft far off. 0 keeps
    /// Bevy's fixed filter.
    pub size: f32,
}

impl Light {
    /// The way to the sun in the models' frame (art X, Y, Z up), a unit
    /// vector. Its azimuth from φ, as the fits: `az = atan2(−cos φ −
    /// sin φ, cos φ − sin φ)`.
    #[must_use]
    pub fn model(self) -> Vec3 {
        let (phi, elevation) = (self.direction.to_radians(), self.elevation.to_radians());
        let azimuth = (-phi.cos() - phi.sin()).atan2(phi.cos() - phi.sin());

        Vec3::new(
            elevation.cos() * azimuth.cos(),
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
        )
    }

    /// The way to the sun in the world frame, a unit vector.
    #[must_use]
    pub fn toward(self) -> Vec3 {
        frame::ART * self.model()
    }

    /// How much a penumbra's half width grows a world unit along the
    /// light: the tangent of half the sun's size. 0 with no size.
    #[must_use]
    pub fn spread(self) -> f32 {
        (self.size.max(0.0) / 2.0).to_radians().tan()
    }

    /// How far across the light about a point the shadow map is read
    /// for its penumbra, world units: its widest, [`ALONG`] from its
    /// caster.
    #[must_use]
    pub fn search(self) -> f32 {
        self.spread() * ALONG
    }
}

/// The sun's size by default, degrees ([`Light::size`]): the user's pick,
/// by eye, on grass.
pub const SIZE: f32 = 3.0;

/// How far along the light from its caster a penumbra keeps widening,
/// world units; past it, as wide as there (`shaders/lit.wesl` `ALONG`).
/// **Inferred**: a tall building's top over the ground under the
/// default sun, by eye (about 250 units up, at 50°: 330 along).
pub const ALONG: f32 = 400.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// Where on the screen the shadow of a pole's top falls, from its
    /// foot: the top's ray to the ground, through [`frame::ART`].
    fn tip(light: Light) -> Vec2 {
        let toward = light.model();
        let top = Vec3::Z * 50.0;
        let ground = top - toward * (top.z / toward.z);

        (frame::ART * ground).truncate()
    }

    /// At φ 0° the sun is right of everything: shadows fall left, level.
    #[test]
    fn a_sun_on_screen_right_drops_shadows_to_the_left() {
        let tip = tip(Light {
            direction: 0.0,
            elevation: 45.0,
            size: 0.0,
        });

        assert!(tip.x < 0.0, "{tip}");
        assert!(tip.y.abs() < 1e-4, "{tip}");
    }

    /// A sun toward the viewer drops shadows up the screen.
    #[test]
    fn a_sun_toward_the_viewer_drops_shadows_up_the_screen() {
        let tip = tip(Light {
            direction: 90.0,
            elevation: 45.0,
            size: 0.0,
        });

        assert!(tip.y > 0.0, "{tip}");
        assert!(tip.x.abs() < 1e-4, "{tip}");
    }

    /// The world's way to the sun is a unit vector at the sun's
    /// elevation over the ground: art Z is [`frame::ART`]'s third column.
    #[test]
    fn toward_the_sun_keeps_its_elevation() {
        let light = Light {
            direction: 29.0,
            elevation: 50.0,
            size: 0.0,
        };
        let toward = light.toward();
        let up = toward.dot(frame::ART.z_axis);

        assert!((toward.length() - 1.0).abs() < 1e-5);
        assert!((up - 50.0_f32.to_radians().sin()).abs() < 1e-5, "{up}");
    }

    /// The shader's copy of [`ALONG`] is this.
    #[test]
    fn the_shaders_along_is_this() {
        let line = format!("const ALONG: f32 = {ALONG:.1};");

        assert!(
            include_str!("../shaders/lit.wesl").contains(&line),
            "{line}"
        );
    }

    /// No size, no penumbra; a 2° sun spreads by tan 1°.
    #[test]
    fn a_suns_size_is_its_spread() {
        let light = |size| Light {
            direction: 29.0,
            elevation: 50.0,
            size,
        };

        assert!(light(0.0).spread().abs() < 1e-9);
        assert!((light(2.0).spread() - 1.0_f32.to_radians().tan()).abs() < 1e-6);
        assert!((light(2.0).search() - light(2.0).spread() * ALONG).abs() < 1e-4);
    }
}
