//! Which way a ground cell is cut into the two triangles it is drawn as,
//! and a corner value interpolated over them. **Binary**.
//!
//! The engine's terrain drawer in draw ground cell in `MajestyHD.exe` projects
//! a cell's four corners, then picks its two triangles on the low bit
//! of `x ^ y`. An even cell gets corners `(0, 1, 3)` and
//! `(0, 3, 2)`, an odd one `(0, 1, 2)` and `(1, 3, 2)`, where corner `n` is
//! `(x + n % 2, y + n / 2)`. So the cut alternates in a checkerboard:
//!
//! ```text
//!  even (Main)   odd (Anti)
//!   0───1         0───1
//!   │ ╲ │         │ ╱ │
//!   2───3         2───3
//! ```
//!
//! It matters three times: the mesh keeps one diagonal as ridge or
//! valley, shade is interpolated over the same triangles, and the pick
//! tests them ([`super::pick`]). Only saddles show it.

/// The diagonal a cell is cut along.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cut {
    /// Top left to bottom right.
    Main,
    /// Top right to bottom left.
    Anti,
}

impl Cut {
    /// The cut for cell `(x, y)` in map coordinates: the checkerboard is
    /// the map's, not a picture's.
    #[must_use]
    pub const fn of(x: u32, y: u32) -> Self {
        if (x ^ y) & 1 == 0 {
            Self::Main
        } else {
            Self::Anti
        }
    }

    /// The cell's two triangles as six of its corners, given in reading
    /// order: top left, top right, bottom left, bottom right.
    #[must_use]
    pub const fn indices<T: Copy>(self, [tl, tr, bl, br]: [T; 4]) -> [T; 6] {
        match self {
            Cut::Main => [tl, br, tr, tl, bl, br],
            Cut::Anti => [tl, bl, tr, tr, bl, br],
        }
    }
}

/// A corner value at `(u, v)` across a cell, interpolated as the engine
/// does: Gouraud over the two triangles `cut` names. Corners in reading
/// order: top left, top right, bottom left, bottom right. Bilinear agrees
/// only on edges. Exact in `(u, v)`: texture to screen is affine.
#[must_use]
pub fn gouraud(corner: [f32; 4], cut: Cut, u: f32, v: f32) -> f32 {
    let [tl, tr, bl, br] = corner;

    match cut {
        Cut::Main if u >= v => tl * (1.0 - u) + tr * (u - v) + br * v,
        Cut::Main => tl * (1.0 - v) + bl * (v - u) + br * u,
        Cut::Anti if u + v <= 1.0 => tl * (1.0 - u - v) + tr * u + bl * v,
        Cut::Anti => tr * (1.0 - v) + bl * (1.0 - u) + br * (u + v - 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cut_alternates_in_a_checkerboard() {
        assert_eq!((Cut::of(0, 0), Cut::of(1, 0)), (Cut::Main, Cut::Anti));
        assert_eq!((Cut::of(0, 1), Cut::of(1, 1)), (Cut::Anti, Cut::Main));
    }

    /// Both triangles share the cut's diagonal: the module doc's corners.
    #[test]
    fn the_triangles_share_the_cut_diagonal() {
        assert_eq!(Cut::Main.indices([0, 1, 2, 3]), [0, 3, 1, 0, 2, 3]);
        assert_eq!(Cut::Anti.indices([0, 1, 2, 3]), [0, 2, 1, 1, 2, 3]);
    }

    /// Each corner reads its own value; the kept diagonal's midpoint
    /// reads the mean of its two ends, not of all four.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact at corners and midpoints")]
    fn a_saddle_reads_the_diagonal_its_cut_keeps() {
        let saddle = [1.0, 0.0, 0.0, 1.0];

        for cut in [Cut::Main, Cut::Anti] {
            assert_eq!(gouraud(saddle, cut, 0.0, 0.0), 1.0);
            assert_eq!(gouraud(saddle, cut, 1.0, 0.0), 0.0);
            assert_eq!(gouraud(saddle, cut, 0.0, 1.0), 0.0);
            assert_eq!(gouraud(saddle, cut, 1.0, 1.0), 1.0);
        }

        assert_eq!(gouraud(saddle, Cut::Main, 0.5, 0.5), 1.0, "the ridge");
        assert_eq!(gouraud(saddle, Cut::Anti, 0.5, 0.5), 0.0, "the valley");
    }
}
