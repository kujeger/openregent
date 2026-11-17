//! A picture cut to another size by its own rows and columns, none
//! stretched: a frame made taller or shorter inside a plain band, a
//! field made narrower or lower with its edges kept. Each canvas draws
//! the pieces as cuts of the whole picture (`panel::canvas`).
//!
//! ```text
//!  frame (rows)   [top][band][bottom]          the band dropped, or repeated:
//!  taller         [top][band][dnab][band][bottom]  each copy mirrored, so the
//!                                                   seams match
//!  field (ends)   [start ......|end]           the middle dropped, the far
//!                                              edge kept
//! ```

/// The rows of the message box `AP11`'s frame (`INDk` 1000, 323 high)
/// that are cut or repeated: inside its dark well, between the title's
/// plate and the bottom button's place, where its sides are straight.
/// openregent's own: the original's box keeps one size.
pub const MESSAGE_BAND: (f32, f32) = (100.0, 270.0);

/// The message box at its shortest, rows: the title's plate and the
/// bottom ornament with a little well between.
pub const MESSAGE_LEAST: f32 = 160.0;

/// The last row of `AP11`'s well, where no bottom button is: 5 rows
/// above its frame's lower bevel. **Inferred** from `INDk` 1000.
pub const MESSAGE_WELL_END: f32 = 285.0;

/// Whether a piece shows its rows as they are, or upside down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flip {
    Plain,
    Mirrored,
}

/// One run of a picture's rows: source rows `from..to`, drawn from row
/// `at` of the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    pub from: f32,
    pub to: f32,
    pub at: f32,
    pub flip: Flip,
}

impl Strip {
    /// Its rows.
    #[must_use]
    pub fn rows(&self) -> f32 {
        self.to - self.from
    }
}

/// A picture `whole` rows high made `tall` rows high: its rows above
/// `band` and below it kept, the band cut short or repeated, each repeat
/// mirrored. At least the rows outside the band.
///
/// `rows(323.0, (100.0, 270.0), 213.0)`: rows 0..100, 100..160, 270..323.
#[must_use]
pub fn rows(whole: f32, (top, bottom): (f32, f32), tall: f32) -> Vec<Strip> {
    let band = bottom - top;
    let need = (tall - top - (whole - bottom)).max(0.0);
    let head = Strip {
        from: 0.0,
        to: top,
        at: 0.0,
        flip: Flip::Plain,
    };
    let foot = Strip {
        from: bottom,
        to: whole,
        at: top + need,
        flip: Flip::Plain,
    };

    // The band's copies, every other one mirrored: a mirrored copy of
    // `n` rows shows the band's last `n`, so it meets the copy above.
    let mut middle = Vec::new();
    let mut at = top;

    while at < top + need {
        let n = band.min(top + need - at);
        let flip = if middle.len() % 2 == 0 {
            Flip::Plain
        } else {
            Flip::Mirrored
        };
        let (from, to) = match flip {
            Flip::Plain => (top, top + n),
            Flip::Mirrored => (bottom - n, bottom),
        };

        middle.push(Strip { from, to, at, flip });
        at += n;
    }

    std::iter::once(head)
        .chain(middle)
        .chain(std::iter::once(foot))
        .filter(|s| s.rows() > 0.0)
        .collect()
}

/// A span `whole` long cut to `target`, at most `whole`, its last `edge`
/// kept: the start's piece and the end's, each `(from, to, source)` in
/// the target, `source` where in the picture its first pixel is.
///
/// `ends(26.0, 34.0, 6.0)`: `(0, 20, 0)` and `(20, 26, 28)`.
#[must_use]
pub fn ends(target: f32, whole: f32, edge: f32) -> [(f32, f32, f32); 2] {
    let target = target.min(whole);
    let split = (target - edge).max(0.0);

    [(0.0, split, 0.0), (split, target, whole - (target - split))]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `AP11`'s frame and band.
    const WHOLE: f32 = 323.0;
    const BAND: (f32, f32) = MESSAGE_BAND;

    /// Every row of the target drawn once, top to bottom.
    #[allow(clippy::float_cmp, reason = "whole rows")]
    fn covers(strips: &[Strip], tall: f32) -> bool {
        let end = strips.iter().try_fold(0.0, |at, s| {
            (s.at == at && s.rows() > 0.0).then_some(at + s.rows())
        });

        end == Some(tall)
    }

    #[test]
    fn a_shorter_frame_cuts_the_band() {
        let strips = rows(WHOLE, BAND, 213.0);

        assert!(covers(&strips, 213.0));
        assert_eq!(strips[1].from, 100.0);
        assert_eq!(strips[1].to, 160.0);
        assert_eq!(strips[2].from, 270.0);
        assert_eq!(strips[2].at, 160.0);
    }

    #[test]
    fn a_frame_shorter_than_its_ends_keeps_them() {
        let strips = rows(WHOLE, BAND, 100.0);

        assert_eq!(strips.len(), 2);
        assert!(covers(&strips, 153.0));
    }

    /// The second copy mirrored, so its first row is the band's last.
    #[test]
    fn a_taller_frame_mirrors_every_other_copy() {
        let strips = rows(WHOLE, BAND, 500.0);
        let flips: Vec<Flip> = strips.iter().map(|s| s.flip).collect();

        assert!(covers(&strips, 500.0));
        assert_eq!(
            flips,
            [
                Flip::Plain,
                Flip::Plain,
                Flip::Mirrored,
                Flip::Plain,
                Flip::Plain
            ]
        );
        assert_eq!((strips[3].from, strips[3].to), (100.0, 107.0));
    }

    /// A mirrored copy cut short shows the band's last rows.
    #[test]
    fn a_short_mirrored_copy_is_the_bands_end() {
        let strips = rows(WHOLE, BAND, 373.0);

        assert!(covers(&strips, 373.0));
        assert_eq!((strips[2].from, strips[2].to), (220.0, 270.0));
    }

    #[test]
    fn the_whole_height_is_the_picture() {
        let strips = rows(WHOLE, BAND, WHOLE);

        assert!(covers(&strips, WHOLE));
        assert!(strips.iter().all(|s| s.flip == Flip::Plain));
    }

    #[test]
    fn a_cut_field_keeps_its_far_edge() {
        assert_eq!(
            ends(26.0, 34.0, 6.0),
            [(0.0, 20.0, 0.0), (20.0, 26.0, 28.0)]
        );
        assert_eq!(
            ends(34.0, 34.0, 6.0),
            [(0.0, 28.0, 0.0), (28.0, 34.0, 28.0)]
        );
    }

    #[test]
    fn a_field_is_never_wider_than_its_picture() {
        assert_eq!(ends(500.0, 432.0, 6.0)[1], (426.0, 432.0, 426.0));
    }
}
