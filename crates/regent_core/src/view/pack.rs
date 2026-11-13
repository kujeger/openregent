//! Frames packed into one texture: a shelf packer, and a frame's drawn
//! bounds to trim it to. openregent's own layout, not the engine's: the
//! engine blits each tile from its own buffer.
//!
//! ```text
//!  sizes ──sort: tallest first──> shelves, left to right, gutter apart
//!        ──try a few widths────> the smallest atlas wins
//!
//!  +------+ g +----+ g +--+
//!  |  a   |   | b  |   |c |      a shelf is as tall as its first
//!  |      |   +----+   +--+      (tallest) rect; the next starts a
//!  +------+                      gutter below it
//!     g
//!  +----+ g +---+
//!  | d  |   | e |
//!  +----+   +---+
//! ```
//!
//! Deterministic: equal sizes keep their input order, so a set packs the
//! same on every run and machine.

/// A rectangle in texels, `x`, `y` its top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    /// Texels it covers.
    #[must_use]
    pub const fn area(&self) -> u64 {
        self.w as u64 * self.h as u64
    }

    /// Whether it and `other`, each grown right and down by `gutter`,
    /// share a texel: closer than `gutter` texels apart.
    #[must_use]
    pub const fn crowds(&self, other: &Rect, gutter: u32) -> bool {
        let apart_x = self.x + self.w + gutter <= other.x || other.x + other.w + gutter <= self.x;
        let apart_y = self.y + self.h + gutter <= other.y || other.y + other.h + gutter <= self.y;

        !(apart_x || apart_y)
    }
}

/// An atlas: its size, and each input's place in input order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Packed {
    pub width: u32,
    pub height: u32,
    pub rects: Vec<Rect>,
}

impl Packed {
    /// Texels the atlas holds.
    #[must_use]
    pub const fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Texels its rects cover.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.rects.iter().map(Rect::area).sum()
    }
}

/// Widths [`pack`] tries, as quarters of the square side its area
/// suggests: 1, 1.25, 1.5, 2.
const WIDTHS_IN_QUARTERS: [u64; 4] = [4, 5, 6, 8];

/// Quarters in one.
const QUARTERS: u64 = 4;

/// `sizes` (width, height) packed at `width` texels across, `gutter`
/// texels between neighbours; none at the atlas edge, where nothing is.
/// Shelves, tallest first. A zero-sized input gets an empty rect at the
/// origin.
#[must_use]
pub fn shelf(sizes: &[(u32, u32)], width: u32, gutter: u32) -> Packed {
    let mut order: Vec<usize> = (0..sizes.len()).collect();

    // Tallest first, then widest; a stable sort keeps ties in input order.
    order.sort_by(|&a, &b| (sizes[b].1, sizes[b].0).cmp(&(sizes[a].1, sizes[a].0)));

    let mut rects = vec![Rect::default(); sizes.len()];
    let (mut x, mut top, mut tall) = (0, 0, 0);
    let mut right = 0;

    for i in order {
        let (w, h) = sizes[i];

        if w == 0 || h == 0 {
            continue;
        }

        // A new shelf when this one is full; never for its first rect.
        if x > 0 && x + w > width {
            top += tall + gutter;
            x = 0;
            tall = 0;
        }

        rects[i] = Rect { x, y: top, w, h };
        right = right.max(x + w);
        tall = tall.max(h);
        x += w + gutter;
    }

    Packed {
        width: right,
        height: top + tall,
        rects,
    }
}

/// `sizes` packed by [`shelf`] at a few widths, from the widest input to
/// twice the side of a square of their area, and `side`: of those
/// within `side` on both axes if any, the smallest atlas, then the
/// squarest, then the narrowest. `side` is a texture size limit, such
/// as 4096.
#[must_use]
pub fn pack(sizes: &[(u32, u32)], gutter: u32, side: u32) -> Packed {
    let widest = sizes.iter().map(|s| s.0).max().unwrap_or(0);
    let area: u64 = sizes
        .iter()
        .filter(|(w, h)| *w > 0 && *h > 0)
        .map(|(w, h)| u64::from(w + gutter) * u64::from(h + gutter))
        .sum();
    let square = area.isqrt();
    let widths = WIDTHS_IN_QUARTERS
        .iter()
        .map(|q| {
            u32::try_from(square * q / QUARTERS)
                .unwrap_or(u32::MAX)
                .max(widest)
        })
        .chain([widest, side.max(widest)]);

    widths
        .map(|w| shelf(sizes, w, gutter))
        .min_by_key(|p| {
            (
                p.width.max(p.height) > side,
                p.area(),
                p.width.max(p.height),
                p.width,
            )
        })
        .unwrap_or_default()
}

/// The smallest rect holding every texel of a `width` by `height` frame
/// that `drawn` says draws (row-major index); `None` if none does.
#[must_use]
pub fn bounds(width: u32, height: u32, drawn: impl Fn(usize) -> bool) -> Option<Rect> {
    let (w, h) = (width as usize, height as usize);
    let mut found: Option<(usize, usize, usize, usize)> = None;

    for y in 0..h {
        for x in (0..w).filter(|x| drawn(y * w + x)) {
            let (x0, y0, x1, y1) = found.unwrap_or((x, y, x, y));

            found = Some((x0.min(x), y0.min(y), x1.max(x), y1.max(y)));
        }
    }

    found.map(|(x0, y0, x1, y1)| {
        let at = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);

        Rect {
            x: at(x0),
            y: at(y0),
            w: at(x1 - x0 + 1),
            h: at(y1 - y0 + 1),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A texture side limit no test reaches.
    const SIDE: u32 = 4096;

    /// A spread of frame sizes, as a set's frames vary.
    fn frames() -> Vec<(u32, u32)> {
        (0..40u32)
            .map(|i| (8 + (i * 37) % 120, 6 + (i * 53) % 90))
            .collect()
    }

    /// Every pair of a packing's rects at least `gutter` apart.
    fn apart(packed: &Packed, gutter: u32) -> bool {
        let rects = &packed.rects;

        (0..rects.len()).all(|a| (a + 1..rects.len()).all(|b| !rects[a].crowds(&rects[b], gutter)))
    }

    #[test]
    fn packing_is_deterministic() {
        assert_eq!(pack(&frames(), 1, SIDE), pack(&frames(), 1, SIDE));
    }

    #[test]
    fn packed_rects_do_not_overlap() {
        assert!(apart(&pack(&frames(), 0, SIDE), 0));
    }

    #[test]
    fn packed_rects_keep_their_gutter() {
        for gutter in [1, 2] {
            assert!(
                apart(&pack(&frames(), gutter, SIDE), gutter),
                "gutter {gutter}"
            );
        }
    }

    #[test]
    fn packed_rects_lie_inside_the_atlas_at_their_size() {
        let sizes = frames();
        let packed = pack(&sizes, 2, SIDE);

        for (rect, (w, h)) in packed.rects.iter().zip(&sizes) {
            assert_eq!((rect.w, rect.h), (*w, *h));
            assert!(rect.x + rect.w <= packed.width && rect.y + rect.h <= packed.height);
        }
    }

    /// Two rects one texel apart crowd a gutter of two, not of one.
    #[test]
    fn a_gutter_is_texels_between() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
        };
        let b = Rect { x: 5, ..a };

        assert!(!a.crowds(&b, 1));
        assert!(a.crowds(&b, 2));
    }

    /// One frame fills its atlas exactly: no gutter at the edge.
    #[test]
    fn one_frame_is_its_own_atlas() {
        let packed = pack(&[(30, 20)], 2, SIDE);

        assert_eq!((packed.width, packed.height), (30, 20));
        assert_eq!(packed.used(), packed.area());
    }

    /// Equal frames keep input order along a shelf.
    #[test]
    fn equal_frames_keep_their_order() {
        let packed = shelf(&[(4, 4); 3], 100, 1);
        let xs: Vec<u32> = packed.rects.iter().map(|r| r.x).collect();

        assert_eq!(xs, [0, 5, 10]);
    }

    /// A shelf too narrow for two starts another, a gutter below.
    #[test]
    fn a_full_shelf_starts_the_next_below() {
        let packed = shelf(&[(10, 6), (10, 4)], 15, 2);

        assert_eq!(
            packed.rects[1],
            Rect {
                x: 0,
                y: 8,
                w: 10,
                h: 4
            }
        );
        assert_eq!((packed.width, packed.height), (10, 12));
    }

    #[test]
    fn a_zero_sized_frame_takes_no_room() {
        let packed = pack(&[(0, 5), (3, 3)], 1, SIDE);

        assert_eq!(packed.rects[0].area(), 0);
        assert_eq!((packed.width, packed.height), (3, 3));
    }

    /// Twenty frames fit in least area as one row, 200 wide; within a
    /// side of 100, as shelves.
    #[test]
    fn a_packing_keeps_within_its_side_where_it_can() {
        let sizes = [(10, 30); 20];
        let free = pack(&sizes, 0, SIDE);
        let kept = pack(&sizes, 0, 100);

        assert_eq!((free.width, free.height), (200, 30));
        assert!(kept.width.max(kept.height) <= 100, "{kept:?}");
    }

    #[test]
    fn nothing_packs_to_nothing() {
        assert_eq!(pack(&[], 1, SIDE), Packed::default());
    }

    /// The bounds of an L of drawn texels in a 6 by 5 frame.
    #[test]
    fn bounds_hold_every_drawn_texel_and_no_more() {
        let drawn = [(2, 1), (2, 2), (2, 3), (3, 3), (4, 3)];
        let found = bounds(6, 5, |i| drawn.contains(&(i % 6, i / 6)));

        assert_eq!(
            found,
            Some(Rect {
                x: 2,
                y: 1,
                w: 3,
                h: 3
            })
        );
    }

    #[test]
    fn a_frame_drawing_nothing_has_no_bounds() {
        assert_eq!(bounds(4, 4, |_| false), None);
    }
}
