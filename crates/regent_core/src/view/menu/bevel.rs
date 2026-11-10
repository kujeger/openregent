//! The bevel a control's style draws round it: two pixels, each side's
//! rows a shade of what is under them, not a colour.
//!
//! **Binary**: `CYDialogManager`, from a text box's
//! method after its ground.
//! Each pass `i` of two shades four strips with rect shade:
//! mode 1 keeps a share of the value through
//! Ramps shadow8, mode 2 adds steps through the
//! brighten family (rect shade brighten, tables).
//!
//! ```text
//!  raised            sunken
//!  ┌──── lit ────┐   ┌─── dark ────┐    lit: mode 2 table 3, +8 steps
//!  lit        dark   dark        lit    dark: mode 1, outer table 0 (50 %)
//!  └─── dark ────┘   └──── lit ────┘          inner 3 (68 %) raised, 2 (62 %) sunken
//! ```

use crate::data::smnu::style;
use crate::view::raster::shade::{RAMPS, Shade};

/// How many pixels deep: the loop's two passes.
pub const DEPTH: i32 = 2;

/// The brighten table both bevels light with: mode 2's table 3.
const LIT_TABLE: u32 = 3;

/// The darkening tables, outer pass first: raised adds 3 a pass,
/// sunken 2.
const RAISED_DARK: [u32; 2] = [0, 3];
const SUNKEN_DARK: [u32; 2] = [0, 2];

/// The brighten family's table steps are 32 to the percent's 100
/// (`ramp_build_brighten8`, ramp build brighten8: `first << 5` over 100).
const BRIGHTEN_SCALE: u32 = 32;

/// Which bevel a style draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bevel {
    Raised,
    Sunken,
}

impl Bevel {
    /// The bevel of a control of `style`: raised tested first.
    #[must_use]
    pub const fn of(style: i32) -> Option<Bevel> {
        if style & style::RAISED != 0 {
            Some(Bevel::Raised)
        } else if style & style::SUNKEN != 0 {
            Some(Bevel::Sunken)
        } else {
            None
        }
    }

    /// The eight strips round `[x0, y0, x1, y1]` (far edges exclusive),
    /// in the order they are shaded: per pass top, right, left, bottom.
    #[must_use]
    pub fn strips(self, [x0, y0, x1, y1]: [i32; 4]) -> Vec<([i32; 4], Shade)> {
        let lit = brighten(LIT_TABLE);
        let dark = match self {
            Bevel::Raised => RAISED_DARK,
            Bevel::Sunken => SUNKEN_DARK,
        };

        (0..DEPTH)
            .zip(dark)
            .flat_map(|(i, table)| {
                let dark = keep(table);
                let (near, far) = match self {
                    Bevel::Raised => (lit, dark),
                    Bevel::Sunken => (dark, lit),
                };

                [
                    ([x0 + i, y0 + i, x1 - i, y0 + i + 1], near),
                    ([x1 - i - 1, y0 + i + 1, x1 - i, y1 - i], far),
                    ([x0 + i, y0 + i + 1, x0 + i + 1, y1 - i - 1], near),
                    ([x0 + i, y1 - i - 1, x1 - i - 1, y1 - i], far),
                ]
            })
            .collect()
    }
}

/// Mode 1's table `n`: the `(50, 6)` family, `50 + 6n` percent kept.
const fn keep(n: u32) -> Shade {
    let (first, step) = RAMPS[0];

    Shade::Keep(first + n * step)
}

/// Mode 2's table `n`: the `(10, 6)` family, `(10 + 6n) × 32 / 100` steps
/// added.
const fn brighten(n: u32) -> Shade {
    let (first, step) = RAMPS[2];

    Shade::Raise((first + n * step) * BRIGHTEN_SCALE / 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raised_bevel_lights_its_top_and_left_and_darkens_the_rest() {
        let strips = Bevel::Raised.strips([10, 20, 30, 40]);

        assert_eq!(strips.len(), 8);
        assert_eq!(strips[0], ([10, 20, 30, 21], Shade::Raise(8)));
        assert_eq!(strips[1], ([29, 21, 30, 40], Shade::Keep(50)));
        assert_eq!(strips[2], ([10, 21, 11, 39], Shade::Raise(8)));
        assert_eq!(strips[3], ([10, 39, 29, 40], Shade::Keep(50)));
        assert_eq!(strips[5], ([28, 22, 29, 39], Shade::Keep(68)));
    }

    #[test]
    fn a_sunken_bevel_darkens_its_top_and_left() {
        let strips = Bevel::Sunken.strips([0, 0, 10, 10]);

        assert_eq!(strips[0].1, Shade::Keep(50));
        assert_eq!(strips[1].1, Shade::Raise(8));
        assert_eq!(strips[4], ([1, 1, 9, 2], Shade::Keep(62)));
    }

    #[test]
    fn raised_wins_over_sunken() {
        assert_eq!(
            Bevel::of(style::RAISED | style::SUNKEN),
            Some(Bevel::Raised)
        );
        assert_eq!(Bevel::of(style::SUNKEN), Some(Bevel::Sunken));
        assert_eq!(Bevel::of(style::TILED), None);
    }
}
