//! An `IMAG` font's measures: a slot a character, each slot's blocks
//! its glyph's box. **Binary** unless marked.
//!
//! ```text
//!  slot c, block 1   the glyph's box: x1 the advance     CYFontImage
//!  slot 0, block 1   -y0 the ascent, y1 the descent
//!  slot 0, block 2   x1 - x0 the spacing after a glyph
//!  a line's width    advances, the spacing after all
//!                    but a space
//! ```
//!
//! **Inferred**: the font maps a character to its own slot; every
//! shipped font has 256 but `fn10`'s 130.

use crate::content::imagery::Animation;
use crate::data::imag::Image;

/// The block of a slot giving its glyph's box.
const GLYPH_BLOCK: u32 = 1;

/// Slot 0's block giving the letter spacing.
const SPACING_BLOCK: u32 = 2;

/// A font's slots' blocks, by character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    slots: Vec<Vec<(u32, [i16; 4])>>,
}

/// A font's line measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub ascent: i32,
    pub descent: i32,
    /// Pixels after every glyph but a space.
    pub spacing: i32,
}

impl Font {
    /// `image`'s first set.
    #[must_use]
    pub fn of_image(image: &Image<'_>) -> Font {
        let slots = image
            .sets
            .first()
            .map(|s| s.slots.as_slice())
            .unwrap_or_default();

        Font {
            slots: slots
                .iter()
                .map(|s| s.attachment_blocks().collect())
                .collect(),
        }
    }

    /// A set already resolved ([`crate::content::imagery::sets`]).
    #[must_use]
    pub fn of_set(set: &Animation) -> Font {
        Font {
            slots: set.poses.iter().map(|p| p.attachments.clone()).collect(),
        }
    }

    /// Block `bit` of slot `slot`, widened.
    fn block(&self, slot: usize, bit: u32) -> Option<[i32; 4]> {
        self.slots
            .get(slot)?
            .iter()
            .find(|(b, _)| *b == bit)
            .map(|(_, r)| r.map(i32::from))
    }

    /// How far character `c`'s glyph moves the pen: its box's `x1`;
    /// `None` without a glyph.
    #[must_use]
    pub fn advance(&self, c: u8) -> Option<i32> {
        self.block(usize::from(c), GLYPH_BLOCK)
            .map(|[_, _, x1, _]| x1)
    }

    /// The line's measures: slot 0's glyph box where it is not empty,
    /// else the tallest of every slot's; the spacing from slot 0's
    /// block 2.
    #[must_use]
    pub fn metrics(&self) -> Metrics {
        let glyphs = |slot| self.block(slot, GLYPH_BLOCK);
        let tallest = (0..self.slots.len())
            .filter_map(glyphs)
            .fold((0, 0), |(a, d), [_, y0, _, y1]| (a.max(-y0), d.max(y1)));
        let (ascent, descent) = match glyphs(0) {
            Some([_, y0, _, y1]) if y0 != 0 || y1 != 0 => (-y0, y1),
            _ => tallest,
        };
        let spacing = self
            .block(0, SPACING_BLOCK)
            .map_or(0, |[x0, _, x1, _]| x1 - x0);

        Metrics {
            ascent,
            descent,
            spacing,
        }
    }

    /// Slot 0's glyph box's height, ascent plus descent, as a text box
    /// counts its rows; `None` without the
    /// block.
    #[must_use]
    pub fn line(&self) -> Option<i32> {
        self.block(0, GLYPH_BLOCK).map(|[_, y0, _, y1]| y1 - y0)
    }

    /// How far `c` moves the pen in a line: its advance and the spacing
    /// after all but a space; 0 without a glyph.
    #[must_use]
    pub fn step(&self, c: u8, metrics: Metrics) -> i32 {
        let extra = if c == b' ' { 0 } else { metrics.spacing };

        self.advance(c).map_or(0, |x1| x1 + extra)
    }

    /// A line's width: its steps summed.
    #[must_use]
    pub fn width(&self, text: &str) -> i32 {
        let metrics = self.metrics();

        text.bytes().map(|c| self.step(c, metrics)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slot 0 with a glyph box and spacing, slot `a` one glyph.
    fn font() -> Font {
        let mut slots = vec![Vec::new(); 256];

        slots[0] = vec![(GLYPH_BLOCK, [0, -7, 0, 3]), (SPACING_BLOCK, [0, 0, 1, 0])];
        slots[usize::from(b'a')] = vec![(GLYPH_BLOCK, [0, -5, 6, 0])];
        slots[usize::from(b' ')] = vec![(GLYPH_BLOCK, [0, 0, 3, 0])];

        Font { slots }
    }

    #[test]
    fn a_glyph_steps_its_advance_and_the_spacing_but_a_space() {
        let font = font();

        assert_eq!(font.advance(b'a'), Some(6));
        assert_eq!(font.width("a"), 7);
        assert_eq!(font.width(" "), 3);
        assert_eq!(font.width("a a"), 17);
        assert_eq!(font.width("ab"), 7, "no glyph counts 0");
    }

    #[test]
    fn slot_zero_gives_the_line() {
        let metrics = font().metrics();

        assert_eq!(
            (metrics.ascent, metrics.descent, metrics.spacing),
            (7, 3, 1)
        );
        assert_eq!(font().line(), Some(10));
    }
}
