//! The map generator, rebuilt from the engine.
//!
//! A `.q` is a recipe; the ground it describes is made at quest start, so only
//! a save holds a generated map. The oracle is bit-exact: pick a map number,
//! generate, diff. See [`docs/formats/saves.md`](docs/formats/saves.md).
//!
//! # How the engine builds a heightfield
//!
//! Gen heightfield, in order:
//!
//! 1. Fill every vertex with [`Field::UNSET`].
//! 2. Collect the distinct height profiles the map's regions name.
//! 3. Per profile: re-seed the RNG to the state the charts left (snapshotted at
//!    Gen heightfield), run the fractal over the whole map, then reset every
//!    vertex belonging to a *later* profile to the sentinel. The next pass
//!    fills only those.
//! 4. Pin the rim, the only step that writes the four corners.
//!
//! No per-zone rectangle: zones are carved by reset and regenerate. The RNG
//! re-seeds each pass, so only zone order couples them.
//!
//! # The eight charts
//!
//! World generate charts builds eight on one stream, the fractal last.
//! [`world::build`] runs them in order:
//!
//! | | Chart | Module |
//! |---|---|---|
//! | 1 | force | [`force`] |
//! | 2 | unit | [`unit`](mod@unit) |
//! | 3 | region | [`region`] |
//! | 4 | terrain | [`terrain`] |
//! | 5 | surface | [`surface`], then [`paint`] |
//! | 6 | landscape | [`landscape`] |
//! | 7 | mobject | [`mobject`] |
//! | 8 | fractal | [`generate`] |
//!
//! [`setup`] runs the first two off the dialog's own three picks;
//! `crate::sim::start` stands up the units the second names, after the
//! scatter ([`fresh::stand`]).
//!
//! All eight reproduce. All 60 Windows freestyle saves rebuild from the map
//! number alone: heightfield exact away from buildings, scatter cell and code
//! exact, draw count exact. A matching field also pins the region chart: it
//! depends only on stream position and zone ownership. `docs/formats/maps.md` lists
//! what each layer is held to.
//!
//! [`generate`] takes zone ownership as an argument: the RNG re-seeds per zone,
//! so the layout's draws do not reach the heights.

pub mod animate;
pub mod custom;
pub mod dressing;
pub mod field;
pub mod force;
pub mod fresh;
pub mod landscape;
pub mod mobject;
pub mod paint;
pub mod pick;
pub mod preset;
pub mod quest;
pub mod recipe;
pub mod region;
pub mod rim;
pub mod rng;
pub mod setup;
pub mod shuffle;
pub mod surface;
pub mod terrain;
pub mod unit;
pub mod world;

pub use field::{Field, Rect};
pub use rng::Rng;
pub use shuffle::shuffled_points;

use crate::base::fixed::{self, Fixed};

/// One zone's contribution: its fractal settings, and the cells it owns.
///
/// `scale` is the profile's third word and `falloff` its second — see
/// `crate::data::rgs::chart::Profile`. A profile the chart does not define
/// falls back to [`Zone::DEFAULT_SCALE`] and [`Zone::DEFAULT_FALLOFF`], which
/// fractal settings ctor builds.
pub struct Zone<'a> {
    pub scale: Fixed,
    pub falloff: Fixed,
    /// Whether a cell belongs to this zone, by `(x, y)` in cells.
    pub owns: &'a dyn Fn(u32, u32) -> bool,
}

impl Zone<'_> {
    /// The scale the default settings object carries: 10.0.
    ///
    /// **Binary** — `"10.0"` in fractal settings ctor. Saved heights sit in 0 to 11,
    /// unscaled.
    pub const DEFAULT_SCALE: Fixed = fixed::from_decimal("10.0");

    /// The hurst exponent the default settings object carries: 0.5.
    ///
    /// **Binary** — `"0.5"` in fractal settings ctor.
    pub const DEFAULT_HURST: Fixed = fixed::from_decimal("0.5");

    /// The multiplier that hurst names: 0.840896.
    ///
    /// **Binary**, a table lookup. Fractal settings ctor
    /// indexes `fixed_mul(hurst, 100.0) >> 16` into 102 strings at hurst multiplier table
    /// (stride 12, built at start-up from a static initialiser) and parses the hit.
    /// Index 50, at a static initialiser, is `"0.840896"`.
    ///
    /// The table is `2^(-hurst/2)`, as
    /// `crate::data::rgs::chart::Profile::falloff` records over the fourteen
    /// shipped profiles. Parsed, not computed: `Str2Fixed` truncates, giving
    /// 55,108 against 55,109. See [`fixed::from_decimal`].
    ///
    /// [`crate::sim::mapgen::world`] uses this for a profile code the chart lacks; no
    /// shipped land has one.
    pub const DEFAULT_FALLOFF: Fixed = fixed::from_decimal("0.840896");
}

/// Generate a heightfield.
///
/// `zones` are taken in order: each generates the whole map, then yields the
/// cells of every zone after it.
#[must_use]
pub fn generate(side: u32, zones: &[Zone<'_>], seed: i32) -> Field {
    generate_from(side, zones, seed).0
}

/// The same, and where the stream stands afterwards.
///
/// The fractal re-seeds once per distinct height profile, so later draws (e.g.
/// the `rng_next(slot_count)` picking each scatter object's picture) resume
/// from the last profile's pass, not the seven charts' total.
///
/// Zero zones leaves the stream at the seed.
#[must_use]
pub fn generate_from(side: u32, zones: &[Zone<'_>], seed: i32) -> (Field, Rng) {
    let mut field = Field::new(side);
    let mut rng = Rng::new(seed);
    let rect = Rect {
        x0: 0,
        y0: 0,
        x1: side,
        y1: side,
    };

    for (i, zone) in zones.iter().enumerate() {
        // Every zone restarts the identical stream — gen heightfield, with the
        // seed snapshotted once in gen heightfield.
        rng = Rng::new(seed);

        field::midpoint(&mut field, &mut rng, rect, side, zone.scale, zone.falloff);

        // Reset the later zones' cells — gen zone reset.
        for later in &zones[i + 1..] {
            reset(&mut field, later);
        }
    }

    rim::pin(&mut field, crate::sim::slope::MAX_SLOPE_TAN);

    (field, rng)
}

/// Put every vertex of a zone's cells back to the sentinel.
///
/// **Binary** — gen zone reset writes `0xffff0000` at
/// each owned cell's own vertex, not its four corners, so the last vertex row
/// and column are never reset.
fn reset(field: &mut Field, zone: &Zone<'_>) {
    for y in 0..field.side() {
        for x in 0..field.side() {
            if (zone.owns)(x, y) {
                field.set(x, y, Field::UNSET);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::base::fixed::ONE;

    use super::*;

    /// The default settings object's two numbers are one setting.
    ///
    /// The falloff table is `2^(-hurst/2)` (see [`Zone::DEFAULT_FALLOFF`]), so
    /// index 50 and the string in fractal settings ctor agree to within `Str2Fixed`'s
    /// truncation.
    #[test]
    fn the_default_falloff_is_the_default_hursts_own() {
        let hurst = fixed::to_f64(Zone::DEFAULT_HURST);
        let want = 2.0f64.powf(-hurst / 2.0) * fixed::ONE_F64;
        let falloff = Zone::DEFAULT_FALLOFF.to_bits();

        assert_eq!(Zone::DEFAULT_HURST, ONE / 2);
        assert_eq!(falloff, 55_108);
        assert!(
            (f64::from(falloff) - want).abs() < 2.0,
            "the table's index 50 is {falloff}, against 2^(-hurst/2) at {want:.2}",
        );
    }

    /// The engine's string constants, against the fixed point they reach.
    ///
    /// `"3.14"` is `crate::sim::trig::PI`; `"10.0"` is
    /// [`Zone::DEFAULT_SCALE`].
    #[test]
    fn the_engines_own_parser_reaches_its_own_constants() {
        assert_eq!(fixed::from_decimal("3.14"), crate::sim::trig::PI);
        assert_eq!(fixed::from_decimal("10.0"), Zone::DEFAULT_SCALE);
    }
}
