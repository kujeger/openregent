//! The world's 3D frame: the art's metric frame, turned so the world
//! camera's axes are the world's.
//!
//! The art camera is orthographic, yawed 45° and 35° up (**Inferred**,
//! `docs/original/art-camera.md`). Turning the whole metric frame by that camera
//! leaves an unturned camera: world x is screen right, y screen up, z
//! toward the viewer. Distances and angles stay true (a rotation), and a
//! view depth is one world axis, so draw order keeps f32's full
//! precision; with the camera turned instead, a depth on a 512-cell map
//! mixes positions of 8,000 units and drifts by about 0.001.
//!
//! ```text
//!   art metric (X, Y, Z up, art pixels)  ──ART──>  world (x, y, z)
//!     x = (X − Y)/√2                               screen right
//!     y = (X + Y)/√2 · sin 35° + Z · cos 35°       screen up
//!     z = Z · sin 35° − (X + Y)/√2 · cos 35°       toward the viewer
//! ```
//!
//! What sits where:
//!
//! - a sprite: a quad facing the camera at its anchor, slid along the
//!   view to today's draw depth (`map::depth`): its translation is the
//!   screen's x, y and that depth. Sliding moves no pixel.
//! - the ground: each vertex at its metric point ([`ground_z`]); its
//!   vertex stage puts its depth back on its rung (`card.wesl`), as the
//!   metric depths span more than the camera's depth range.
//! - a model, the sun: through [`ART`].

use bevy::math::{Mat3, Vec2, Vec3};

/// The art camera's elevation, degrees (**Inferred**,
/// `docs/original/art-camera.md`).
pub const ELEVATION: f32 = 35.0;

/// Art metric `(X, Y, Z)` to world: each column an art axis in world
/// terms. Rows: screen right, screen up, toward the viewer.
pub const ART: Mat3 = {
    // sin 35°, cos 35°, 1/√2: const fns cannot take sin.
    const SIN: f32 = 0.573_576_4;
    const COS: f32 = 0.819_152_05;
    const HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;

    Mat3::from_cols(
        Vec3::new(HALF, HALF * SIN, -HALF * COS),
        Vec3::new(-HALF, HALF * SIN, -HALF * COS),
        Vec3::new(0.0, COS, SIN),
    )
};

/// The world z of the ground point drawn at screen height `y` with the
/// map's lift `lift` there (world units): its metric point's depth.
///
/// ```text
///  flat y_f = y − lift:  X + Y = √2 · y_f / sin 35°,  Z = lift / cos 35°
///  z = Z sin 35° − (X + Y)/√2 · cos 35° = lift · tan 35° − y_f / tan 35°
/// ```
#[must_use]
pub fn ground_z(y: f32, lift: f32) -> f32 {
    let tan = ELEVATION.to_radians().tan();

    lift * tan - (y - lift) / tan
}

/// The world point drawn at screen point `drawn`, `lift` above its flat
/// ground point: a ground point with the map's lift there, a model's
/// anchor, or a body's pixel upright over its footing (lift its height
/// on the screen over the footing's flat ground, `shaders/lit.wesl`).
#[must_use]
pub fn point(drawn: Vec2, lift: f32) -> Vec3 {
    drawn.extend(ground_z(drawn.y, lift))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constant is the rotation it says: the two angles' sines and
    /// cosines, and orthonormal.
    #[test]
    fn art_is_the_art_cameras_rotation() {
        let (sin, cos) = ELEVATION.to_radians().sin_cos();

        assert!((ART.x_axis.y - std::f32::consts::FRAC_1_SQRT_2 * sin).abs() < 1e-7);
        assert!((ART.z_axis.y - cos).abs() < 1e-7);
        assert!((ART * ART.transpose()).abs_diff_eq(Mat3::IDENTITY, 1e-6));
        assert!((ART.determinant() - 1.0).abs() < 1e-6);
    }

    /// A model vertex lands on the screen point the art camera draws
    /// it at (`sun::model`: x = (X − Y)/√2, y = (X + Y)/√2 sin 35° +
    /// Z cos 35°).
    #[test]
    fn a_model_vertex_lands_where_the_art_draws_it() {
        let (sin, cos) = ELEVATION.to_radians().sin_cos();
        let half = std::f32::consts::FRAC_1_SQRT_2;

        for v in [
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(0.0, 25.0, 0.0),
            Vec3::new(0.0, 0.0, 40.0),
            Vec3::new(-31.5, 12.25, 77.0),
        ] {
            let world = ART * v;
            let screen = Vec2::new((v.x - v.y) * half, (v.x + v.y) * half * sin + v.z * cos);

            assert!((world.truncate() - screen).length() < 1e-4, "{v}: {world}");
        }
    }

    /// A point upright over a ground point, drawn `h` higher on the
    /// screen, is the ground point plus art height `h / cos 35°`.
    #[test]
    fn an_upright_point_stands_on_its_footing() {
        let cos = ELEVATION.to_radians().cos();
        let foot = ART * Vec3::new(120.0, -40.0, 8.0);
        let lift = 8.0 * cos;
        let h = 30.0;

        assert!((point(foot.truncate(), lift) - foot).length() < 1e-2);

        let top = point(foot.truncate() + Vec2::Y * h, lift + h);

        assert!(
            (top - (foot + ART.z_axis * (h / cos))).length() < 1e-2,
            "{top}"
        );
    }

    /// The shaders' copy of the elevation's tangent is this one's.
    #[test]
    fn the_shaders_tangent_is_the_elevations() {
        let source = include_str!("shaders/lit.wesl");
        let line = source
            .lines()
            .find_map(|l| l.strip_prefix("const TAN_ELEVATION: f32 = "))
            .expect("the constant");
        let value: f32 = line.trim_end_matches(';').parse().expect("a number");

        assert!(
            (value - ELEVATION.to_radians().tan()).abs() < 1e-6,
            "{value}"
        );
    }

    /// The ground's depth is its metric point's: a point on the ground
    /// sent through [`ART`] has the z [`ground_z`] gives at its screen
    /// height and lift.
    #[test]
    fn the_grounds_depth_is_its_metric_points() {
        for (ground, height) in [(10.0, 4.0), (-300.0, 120.0), (5_000.0, 0.0)] {
            let metric = Vec3::new(ground, ground * 0.5, height);
            let world = ART * metric;
            let lift = height * ELEVATION.to_radians().cos();

            assert!(
                (ground_z(world.y, lift) - world.z).abs() < 1e-2,
                "{metric}: {} {}",
                ground_z(world.y, lift),
                world.z
            );
        }
    }
}
