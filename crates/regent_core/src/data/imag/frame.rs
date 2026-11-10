//! A slot's frame table entries: what each frame draws.

/// Reference flag bits ever observed, `OR`ed together.
///
/// Anything else in the upper half means not a tile reference.
const REFERENCE_FLAGS: u16 = 0x8000 | 0x4000 | 0x2000 | 0x0800 | 0x0100;

/// What one frame points at.
///
/// Classified by the upper 16 bits, not by checking the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRef {
    /// A positional index into this archive's `TILE` chunk, and its flags.
    Tile { index: u16, flags: u16 },
    /// A negative number. None occur; kept so one is not read as a tile.
    Sentinel(i32),
    /// Upper bits outside the observed set. None occur; kept so one is not
    /// read as a tile.
    Unknown(u32),
}

/// One entry of a slot's frame table.
///
/// Both words raw, for round-tripping. The metadata word is a
/// [`Frame::nudge`], zero in all but 2,895 of HD's 86,868 frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub metadata: u32,
    pub reference: u32,
}

impl Frame {
    /// Offset from the anchor, added to the tile's own.
    ///
    /// **Binary**: the loader swaps the two words, so in memory the reference
    /// comes first; frame box union adds `word[frame+4]`
    /// and `word[frame+6]` to the tile box. 2,896 of HD's frames carry one,
    /// `(-196, -149)` to `(110, 130)`.
    #[must_use]
    pub const fn nudge(self) -> (i16, i16) {
        let [a, b, c, d] = self.metadata.to_le_bytes();

        (i16::from_le_bytes([a, b]), i16::from_le_bytes([c, d]))
    }

    /// Classify the reference.
    pub const fn target(self) -> FrameRef {
        let flags = (self.reference >> 16) as u16;

        if flags & !REFERENCE_FLAGS == 0 {
            FrameRef::Tile {
                index: (self.reference & 0xffff) as u16,
                flags,
            }
        } else if self.reference.cast_signed() < 0 {
            FrameRef::Sentinel(self.reference.cast_signed())
        } else {
            FrameRef::Unknown(self.reference)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_is_classified_by_its_upper_bits() {
        let flagged = Frame {
            metadata: 0,
            reference: 0x4000_0065,
        };
        let plain = Frame {
            metadata: 0,
            reference: 101,
        };
        let negative = Frame {
            metadata: 0,
            reference: 0xffff_fff6,
        };
        let odd = Frame {
            metadata: 0,
            reference: 0x3634_5258, // "XR46", were it ever to appear here
        };

        assert_eq!(
            flagged.target(),
            FrameRef::Tile {
                index: 0x65,
                flags: 0x4000
            }
        );
        assert_eq!(
            plain.target(),
            FrameRef::Tile {
                index: 101,
                flags: 0
            }
        );
        assert_eq!(negative.target(), FrameRef::Sentinel(-10));
        assert_eq!(odd.target(), FrameRef::Unknown(0x3634_5258));
    }
}
