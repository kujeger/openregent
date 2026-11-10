//! `GOPT` — the engine's own settings, and the constants in them.
//!
//! One `DATA` record in `miscdata.cam`, plain text, `key = value` a line.
//!
//! The constants below mirror [`Settings::get`] values; test
//! `the_shipped_settings_hold_the_lighting_constants` pins them to the archive.
//!
//! ## The keys the engine accepts
//!
//! See [`keys`]: the base game knows 118 across three tables. The base `GOPT`
//! sets 25, the expansion's 28, 29 distinct.
//!
//! Unset keys run at the binary's defaults; [`ISO_CAMERA_ANGLE`] is one. Each
//! key's parser case, global and default in HD: [`cases`].
//!
//! HD reads only the expansion's `GOPT` (`mx_miscdata.cam`), so its values
//! win there: [`PARABOLA_ANGLE`], and `GplPseudorandom` 0.

pub mod cases;
pub mod keys;

use super::error::{Error, Result};

/// The camera's pitch in degrees, which no shipped file sets.
///
/// **Binary**: [`keys::ENGINE`]`[18]`, parser case 34 in HD
/// ([`cases::ENGINE`]). The engine stores it as the camera angle, file value 55.
/// `MaxSlope` is the next key, read by the same function.
///
/// From `SetCameraAngle` and `SetScale`:
///
/// ```text
/// t     = scale * TILE_PIXELS
/// rise  = sin(angle) * t          26.21 * scale at 55 degrees
/// run   = cos(angle) * t          18.35 * scale
/// ```
///
/// So ground rises `cos(55°)` = 0.5736 cells per height unit and is
/// foreshortened by `sin(55°)` = 0.819. `scale` is the zoom (two levels).
///
/// Four sites divide a screen distance by one stored factor to recover cells, so the
/// camera is orthographic. See `docs/formats/saves.md`.
pub const ISO_CAMERA_ANGLE: i32 = 55;

/// A terrain tile's size in pixels, and the unit `scale` multiplies.
///
/// The `32.0` in `SetScale`; the tileset art matches (asserted in
/// `regent_draw`).
pub const TILE_PIXELS: f32 = 32.0;

/// How far down the screen a cell reaches, against how far across:
/// `sin(`[`ISO_CAMERA_ANGLE`]`)`. The inverse transform divides only y,
/// by `sin(angle) * 32 * scale`. See `docs/formats/saves.md`.
///
/// A literal: `sin` is not `const fn`. `tests/settings.rs` recomputes it.
pub const SQUASH: f32 = 0.819_152_f32;

/// `sin(`[`ISO_CAMERA_ANGLE`]`)` to the last bit of an `f64`: the
/// camera's over its scale, as play reads it
/// (`sim::placement::unproject`, `crate::sim::mapgen::mobject::reach`).
///
/// A literal so play never calls the platform's `sin`, whose last bit may
/// differ between machines. `tests/settings.rs` checks it equals the `sin`
/// the code called before.
pub const ISO_CAMERA_SIN: f64 = 0.819_152_044_288_991_8;

/// How far a height unit lifts the ground up the screen, in cells:
/// `cos(`[`ISO_CAMERA_ANGLE`]`)`.
///
/// A literal: `cos` is not `const fn`. `tests/settings.rs` recomputes it.
pub const RELIEF: f32 = 0.573_576_f32;

/// The direction the terrain is lit from, as `GOPT` writes it.
///
/// **Game**: `x` equals `y`, as every fit of stored vertex shades finds. With
/// `+x` right and `+y` down, the sun is lower right, matching in-game shadows.
///
/// Not normalised: the file's values.
pub(crate) const LIGHT: [f32; 3] = [0.90, 0.90, 1.0];

/// The diffuse coefficient, `GOPT`'s `Contrast`.
pub(crate) const CONTRAST: f32 = 1.5;

/// The ambient term, `GOPT`'s `Ambient`.
pub const AMBIENT: f32 = 0.35;

/// The steepest slope the engine acknowledges, in degrees.
///
/// Geometry, not lighting, despite sitting beside the five lighting constants.
/// `SetMaxSlope` looks the angle up in two 256-entry 16.16 trig tables (sine
/// and cosine) and stores their ratio beside the camera pitch:
///
/// ```text
/// slope = sin(MaxSlope) / cos(MaxSlope) = tan(MaxSlope)
/// ```
///
/// The binary's own default is 55; every `GOPT` sets 45.
///
/// 45 degrees truncates to table index 31, so the stored tangent is
/// `tan(43.59°)` = 0.9520569, not 1.0 — see `crate::sim::mapgen::rim::tan_of`.
///
/// `IsoMap` passes it to `Map` (HD `Map`), a flood fill that
/// pulls any neighbour vertex further than the tangent back to it, after
/// generation and building flattening. The steepest adjacent pair in the save
/// corpus differs by exactly 0.9520569. See `docs/formats/saves.md`.
pub const MAX_SLOPE: f32 = 45.0;

/// A lobbed projectile's launch angle in degrees, as HD runs it.
///
/// **Binary**: `GE_OrderProcessor` reads it through the 256-entry trig
/// table. The binary's default is 60; the expansion's `GOPT`, the one HD
/// loads, sets 75. The base game's sets neither parabola key.
pub const PARABOLA_ANGLE: f32 = 75.0;

/// A lobbed projectile's gravity, as HD runs it: `-g / 32` cells in 16.16.
///
/// Binary default 8; the expansion's `GOPT` sets 4. See [`PARABOLA_ANGLE`].
pub(crate) const PARABOLA_GRAVITY: f32 = 4.0;

/// The map-difficulty bands a Random settlement, scenario or land pick
/// accepts: Beginner, Advanced, Expert, inclusive.
///
/// **Binary**: pattern in difficulty band tests a pattern's
/// difficulty against the six `DifficultyLimit*` globals; no shipped file
/// sets them, so these are their file values ([`cases::GAME`]).
pub const DIFFICULTY_BANDS: [(i32, i32); 3] = [(0, 39), (40, 79), (80, 100)];

/// The keys whose values these constants mirror, for the test that pins them.
const PINNED: [(&str, f32); 8] = [
    ("LightX", LIGHT[0]),
    ("LightY", LIGHT[1]),
    ("LightZ", LIGHT[2]),
    ("Contrast", CONTRAST),
    ("Ambient", AMBIENT),
    ("MaxSlope", MAX_SLOPE),
    ("ParabolaAngle", PARABOLA_ANGLE),
    ("ParabolaGravity", PARABOLA_GRAVITY),
];

/// A parsed `GOPT`.
///
/// Borrows the payload. A list, not a map: keys repeat (`LoadCAM` fourteen
/// times in the base game); [`Settings::all`] keeps every repeat.
#[derive(Debug, Clone)]
pub struct Settings<'a> {
    entries: Vec<(&'a str, &'a str)>,
}

impl<'a> Settings<'a> {
    /// Read a `GOPT` payload.
    ///
    /// Every line is `key = value`, blank, or a `#` comment. **Corpus**: no
    /// shipped `GOPT` carries a comment; they are kept from the reader's
    /// first form, **Inferred** harmless.
    ///
    /// # Errors
    ///
    /// If the payload is not UTF-8 (it is ASCII in every edition), or a line
    /// is none of the three; the error's value is its line number, from 1.
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        let text = std::str::from_utf8(payload).map_err(|_| Error::BrokenInvariant {
            what: "GOPT is not text",
            expected: 0,
            found: payload.len() as u64,
        })?;

        // CRLF; `lines` drops the CR with the LF.
        let entries = text
            .lines()
            .map(str::trim)
            .enumerate()
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
            .map(|(n, line)| {
                line.split_once('=')
                    .map(|(key, value)| (key.trim(), value.trim()))
                    .ok_or(Error::BadValue {
                        what: "GOPT line without `=`",
                        value: n as u64 + 1,
                    })
            })
            .collect::<Result<_>>()?;

        Ok(Settings { entries })
    }

    /// Every entry, in file order, repeats included.
    pub fn all(&self) -> impl Iterator<Item = (&'a str, &'a str)> + '_ {
        self.entries.iter().copied()
    }

    /// The first value for `key`, matched without regard to case.
    ///
    /// The file writes `CDAUDIO` for the binary's `CDAudio`, and both
    /// `LOADCAM` and `LoadCAM`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&'a str> {
        self.entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }

    /// Every value for `key`, which is what `LoadCAM` needs.
    pub fn every(&self, key: &'a str) -> impl Iterator<Item = &'a str> + '_ {
        self.entries
            .iter()
            .filter(move |(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }

    /// The value for `key` as a number.
    #[must_use]
    pub fn number(&self, key: &str) -> Option<f32> {
        self.get(key).and_then(|v| v.parse().ok())
    }

    /// Every key in this file that no edition's engine knows, in file order.
    ///
    /// Empty on all three shipped `GOPT`s. A hit is a typo the engine ignores
    /// or a gap in [`keys`].
    #[must_use]
    pub fn unknown_keys(&self) -> Vec<&'a str> {
        self.entries
            .iter()
            .map(|(key, _)| *key)
            .filter(|key| !keys::known(key))
            .collect()
    }

    /// Check the constants in this module against the file they came from.
    ///
    /// Returns the keys that disagree; empty on every shipped `GOPT`. Public
    /// for the per-edition integration tests.
    ///
    /// Exact comparison: each literal came from a shipped `GOPT`, so both parse
    /// to the same bits.
    #[must_use]
    #[allow(clippy::float_cmp)]
    pub fn disagreements(&self) -> Vec<(&'static str, f32, Option<f32>)> {
        PINNED
            .iter()
            .filter_map(|(key, want)| {
                let got = self.number(key)?;

                (got != *want).then_some((*key, *want, Some(got)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"# a comment\r\nLightX = 0.90\r\nLoadCAM = A.CAM\r\n\r\nLoadCAM = B.CAM\r\nCDAUDIO = ON\r\n";

    #[test]
    fn a_key_is_found_whatever_its_case() {
        let settings = Settings::parse(SAMPLE).unwrap();

        assert_eq!(settings.get("CDAudio"), Some("ON"));
        assert_eq!(settings.get("cdaudio"), Some("ON"));
    }

    #[test]
    fn a_repeated_key_keeps_every_value() {
        let settings = Settings::parse(SAMPLE).unwrap();

        assert_eq!(
            settings.every("LoadCAM").collect::<Vec<_>>(),
            ["A.CAM", "B.CAM"]
        );

        // The single accessor takes the first, not the last.
        assert_eq!(settings.get("LoadCAM"), Some("A.CAM"));
    }

    #[test]
    fn a_line_without_an_equals_sign_is_refused() {
        assert!(matches!(
            Settings::parse(b"LightX = 0.90\r\nLightY\r\n"),
            Err(Error::BadValue { value: 2, .. })
        ));
    }

    #[test]
    fn comments_and_blank_lines_are_not_entries() {
        assert_eq!(Settings::parse(SAMPLE).unwrap().all().count(), 4);
    }

    /// The bands are the `DifficultyLimit*` file values, low and high.
    #[test]
    fn the_difficulty_bands_are_the_limits_file_values() {
        let initial = |key: &str| match cases::find(key).map(|c| c.target) {
            Some(cases::Target::Int { initial, .. }) => initial.unwrap_or(0),
            other => panic!("{key}: {other:?}"),
        };

        let bands = ["Beginner", "Advanced", "Expert"].map(|band| {
            (
                initial(&format!("DifficultyLimit{band}Low")),
                initial(&format!("DifficultyLimit{band}High")),
            )
        });

        assert_eq!(bands, DIFFICULTY_BANDS);
    }

    #[test]
    fn a_constant_that_drifts_from_the_file_is_reported() {
        let settings = Settings::parse(b"LightX = 0.75").unwrap();
        let off = settings.disagreements();

        assert_eq!(off.len(), 1, "only the key present should be checked");
        assert_eq!(off[0].0, "LightX");
    }
}
