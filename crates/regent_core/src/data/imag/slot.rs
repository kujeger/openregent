//! One pose, facing or variant of a set: its points, frames and trailer.

use super::{
    BLOCKS, EFFECT_LEN, EFFECT_MASK_AT, FRAME_LEN, Frame, PLAYBACK_FLAG_AT, PLAYBACK_LEN, POINTS,
    V2_BLOCKS, Version,
};
use crate::data::cursor::Cursor;
use crate::data::error::{Error, Result, slice, u16_at, u32_at};
use crate::data::fourcc::FourCc;

/// How a set or slot plays: five words, in set header `+0x14` or a slot's
/// trailer.
///
/// **Binary**: anim playback takes the slot's, else the
/// set's, and sets the animator's mode from `kind`: 1 gives 1, 2 gives 4 or 2
/// and 4 gives 5 or 3 by bit 0 of `flags`, anything else 2. `value` goes to
/// animator. **Corpus**: `value` runs 0 to 500, mostly in tens;
/// **Inferred** a time in milliseconds. `rest` is zero on every slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playback {
    pub kind: u32,
    pub flags: u32,
    pub value: u32,
    pub rest: [u32; 2],
}

impl Playback {
    pub(super) fn read(bytes: &[u8; PLAYBACK_LEN]) -> Self {
        let (words, _) = bytes.as_chunks::<4>();
        let word = |i: usize| u32::from_le_bytes(words[i]);

        Playback {
            kind: word(0),
            flags: word(1),
            value: word(2),
            rest: [word(3), word(4)],
        }
    }

    /// Whether every word is zero: the engine then keeps none.
    pub const fn is_empty(&self) -> bool {
        self.kind == 0
            && self.flags == 0
            && self.value == 0
            && self.rest[0] == 0
            && self.rest[1] == 0
    }
}

/// One version 5 effect entry: another `IMAG` set drawn as an extra
/// part (layer) of the unit, at an offset.
///
/// File order, eight words. **Binary** from slot effect read,
/// which stores them reordered (memory 0, 1, 5, 6, 7, 2,
/// 3, 4), and `UnitIMG`'s effect pass, which makes the
/// part. Words 1 and 4 are zero everywhere, no reader found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub words: [u32; 8],
}

/// Word 0 bit 0: the entry makes a part. Set on every shipped entry.
const EFFECT_USED: u32 = 1;

/// Word 0 bit 2: the part keeps [`Effect::slot`] as the unit turns.
const EFFECT_FIXED: u32 = 1 << 2;

impl Effect {
    pub(super) fn read(bytes: &[u8; EFFECT_LEN]) -> Self {
        let mut words = [0; 8];

        for (w, chunk) in words.iter_mut().zip(bytes.as_chunks::<4>().0) {
            *w = u32::from_le_bytes(*chunk);
        }

        Effect { words }
    }

    /// Word 0 bit 0: whether `UnitIMG` makes a part of it.
    pub const fn enabled(&self) -> bool {
        self.words[0] & EFFECT_USED != 0
    }

    /// Word 0 bit 2: whether the part's slot is [`Effect::slot`], not
    /// turned with the unit.
    pub const fn fixed_slot(&self) -> bool {
        self.words[0] & EFFECT_FIXED != 0
    }

    /// Word 0's top byte: the entry's index in its list.
    pub const fn index(&self) -> u32 {
        self.words[0] >> 24
    }

    /// Word 5: the `IMAG` record drawn: `FXa1`, `XR46`
    /// or `XR47` in the install, each another record in the same archive.
    pub const fn image(&self) -> Option<FourCc> {
        Some(FourCc::from_word(self.words[5]))
    }

    /// Word 6: its set id, `0x40` (`Special`) on every
    /// entry.
    pub const fn set(&self) -> u32 {
        self.words[6]
    }

    /// Word 7: the slot of that set: a fire variant, a missile's
    /// direction.
    pub const fn slot(&self) -> u32 {
        self.words[7]
    }

    /// Words 2 and 3: screen pixels added to the part's pose offset, x
    /// right, y down. **Binary**: `IsoGenUnit`,
    /// `IsoUnitIMGPartEntry` and unit hit.
    pub const fn offset(&self) -> (i32, i32) {
        (self.words[2].cast_signed(), self.words[3].cast_signed())
    }
}

/// One point list: `(x, y)` pairs about the anchor.
pub type Points = Vec<(i32, i32)>;

/// One pose, facing or variant, with its frames.
#[derive(Debug, Clone)]
pub struct Slot<'a> {
    /// Pixel offset from layer 0 of its set.
    ///
    /// `+0x00` on versions 3 and up, `+0x18` on version 2, per
    /// imag slot v4 and imag slot v2.
    /// All 339 shipped version 2 slots hold zero; 76 carry a
    /// [`Slot::footprint`] at `+0x00`, not an offset.
    pub x: i16,
    pub y: i16,
    /// Where the thing stands, pixels about its anchor: `left`, `top`,
    /// `right`, `bottom`. Points 0 and 1 of [`Slot::points`]: block 0.
    ///
    /// **Binary**: `IsoFootprintRectManager` reads a
    /// loaded slot's block 0 as a rect when
    /// `left < right` and `top < bottom`. The map generator's scatter pass
    /// divides it by the cell size to fit objects. See `docs/formats/art.md`.
    ///
    /// Zero when block mask bit 0 is clear: the reader zeroes a missing block.
    ///
    /// HD `Data`: 3,244 version 3 to 5 slots hold a proper rect. Held by
    /// `a_slots_footprint_is_where_the_thing_stands`.
    pub footprint: [i16; 4],
    /// The block mask at `+0x08`; each set bit adds an 8-byte block.
    ///
    /// **Binary**: imag slot blocks keeps the first
    /// three bits' blocks in the slot, zeroing a clear
    /// bit's. A block is two points; see [`Slot::points`]. Zero on version 2,
    /// which has no mask and always three blocks.
    pub attachments: u32,
    /// Which blocks are present: [`Slot::attachments`], or all three on
    /// version 2.
    block_mask: u32,
    /// Header `+0x14` (version 5): one [`Effect`] list per set bit.
    pub effect_mask: u32,
    /// The blocks: `8 × attachments.count_ones()` bytes.
    blocks: &'a [u8],
    /// [`Slot::point_lists`], decoded.
    lists: Vec<Points>,
    /// [`Slot::playback`], decoded.
    playback: Option<Playback>,
    /// [`Slot::effects`], decoded.
    effects: Vec<Effect>,
    /// Point lists between the blocks and the frame table. Empty unless the
    /// word at `+0x0c` is set. Read with [`Slot::point_lists`].
    ///
    /// A building's list 0 is the polygon around its base, painted bare ground
    /// on placement.
    ///
    /// One `[count][count pairs]` per set bit of `+0x0c`. In versions 4
    /// and 5 the first count is the header's last dword, before this slice,
    /// and the frame list's zero word ends it:
    ///
    /// ```text
    /// ..header, last dword = n0
    /// n0 x (i32, i32)      list 0 -- the pad polygon on a building
    /// n1, n1 x (i32, i32)  list 1
    /// 0
    /// ```
    ///
    /// In memory, slot point list returns
    /// `slot + 8 * which`, two `{count, pointer}` vectors.
    pub points: &'a [u8],
    pub frames: u16,
    pub streams: u16,
    /// `frames × streams` entries, stream-major.
    table: &'a [u8],
    /// See [`Slot::trailer`].
    trailer: &'a [u8],
}

impl<'a> Slot<'a> {
    /// Each block present with its mask bit, in bit (storage) order.
    ///
    /// Four `i16`: two points, see [`Slot::points`].
    pub fn attachment_blocks(&self) -> impl Iterator<Item = (u32, [i16; 4])> + 'a {
        let set = self.block_mask;
        let mut bits = (0..u32::BITS).filter(move |b| set & (1 << b) != 0);

        self.blocks.as_chunks::<8>().0.iter().map(move |b| {
            let word = |at: usize| i16::from_le_bytes([b[at], b[at + 1]]);

            (
                bits.next().unwrap_or(u32::BITS),
                [word(0), word(2), word(4), word(6)],
            )
        })
    }

    /// The six points the engine keeps, as it holds them: block *n* gives
    /// points `2n` and `2n + 1`, zero where the block is absent.
    ///
    /// **Binary**: slot point returns block `k / 2`,
    /// first pair for even `k`. What a point means depends on the set: in
    /// `Hotspot` slot 0, points 0 to 2 are the [`attachment_point`](super::attachment_point)s and
    /// point 5 their base; blocks 0, 1, 2 of sets 8, `0xd0`, `0x40` are rects.
    /// **Viewer**: a building's `Attack`
    /// slots differ only in points 4 and 5, a firing position.
    pub fn points(&self) -> [(i16, i16); POINTS] {
        let mut out = [(0, 0); POINTS];

        for (bit, [x0, y0, x1, y1]) in self.attachment_blocks().filter(|(bit, _)| *bit < BLOCKS) {
            let at = 2 * bit as usize;

            out[at] = (x0, y0);
            out[at + 1] = (x1, y1);
        }

        out
    }

    /// The slot's own [`Playback`], overriding its set's.
    ///
    /// **Binary**: read into slot by slot extra read
    /// when header `+0x10` is non-zero. HD `Data`: 51 slots.
    pub const fn playback(&self) -> Option<Playback> {
        self.playback
    }

    /// Version 5's effect entries, every list in order.
    ///
    /// **Binary**: slot effects read reads one list per
    /// set bit of header into slot, each replacing the last.
    /// `UnitIMG` makes each a part of the unit after
    /// its layers, last entry first: `FXa1` fires on an expansion
    /// building's `Die` stages, `XR47` dust on its `Crumble`, `XR46` a
    /// firebalm missile's trail. See [`Effect`].
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    /// Bytes after the frame table: a [`Playback`], then version 5's
    /// [`Effect`] lists, as [`Slot::playback`] and [`Slot::effects`] read
    /// them.
    #[must_use]
    pub const fn trailer(&self) -> &'a [u8] {
        self.trailer
    }

    /// Every frame, stream by stream.
    pub fn frames(&self) -> impl Iterator<Item = Frame> + 'a {
        self.table.as_chunks::<FRAME_LEN>().0.iter().map(|e| Frame {
            metadata: u32::from_le_bytes([e[0], e[1], e[2], e[3]]),
            reference: u32::from_le_bytes([e[4], e[5], e[6], e[7]]),
        })
    }

    /// The frames of one stream, in playback order.
    pub fn stream(&self, stream: u16) -> impl Iterator<Item = Frame> + 'a {
        let skip = usize::from(stream) * usize::from(self.frames);

        self.frames().skip(skip).take(usize::from(self.frames))
    }

    /// The slot's point lists, list 0 first: one per set bit of header
    /// `+0x0c`, each a count and that many pairs. See [`Slot::points`].
    #[must_use]
    pub fn point_lists(&self) -> &[Points] {
        &self.lists
    }

    /// Whether the lists account for every byte of [`Slot::points`].
    #[must_use]
    pub fn points_whole(&self) -> bool {
        if self.points.is_empty() {
            return true;
        }

        let lists = self.point_lists();
        let bodies: usize = lists.iter().map(|list| list.len() * 8).sum();

        // One count word per list: lists 1.. are preceded by theirs, and a
        // zero count closes the chain.
        bodies + lists.len() * 4 == self.points.len()
    }
}

impl<'a> Slot<'a> {
    /// The slot from `start` to `end`.
    pub(super) fn parse(
        payload: &'a [u8],
        version: Version,
        start: usize,
        end: usize,
    ) -> Result<Self> {
        // Version 2 keeps the offset at +0x18. See [`Slot::x`].
        let offset_at = if matches!(version, Version::Two) {
            start + 0x18
        } else {
            start
        };
        let x = u16_at(payload, "imag slot x", offset_at)?.cast_signed();
        let y = u16_at(payload, "imag slot y", offset_at + 2)?.cast_signed();

        let attachments = if version.has_mask() {
            u32_at(payload, "imag attachment mask", start + 8)?
        } else {
            0
        };
        let block_mask = if version.has_mask() {
            attachments
        } else {
            V2_BLOCKS
        };

        let counts = u32_at(payload, "imag frame counts", start + version.count_at())?;
        let frames = (counts >> 16) as u16;
        let streams = (counts & 0xffff) as u16;

        // Reading a count word at the wrong offset gives zero; fail rather
        // than return an empty animation.
        if frames == 0 || streams == 0 {
            return Err(Error::BadValue {
                what: "imag frame or stream count",
                value: u64::from(counts),
            });
        }

        // Each set bit of the mask adds an 8-byte block ahead of the frames.
        let blocks_len = 8 * attachments.count_ones() as usize;
        let header = version.base() + blocks_len;
        let len = usize::from(frames) * usize::from(streams) * FRAME_LEN;

        // One point list per set bit of `+0x0c`, `[u32 n]` then `n` pairs,
        // between the blocks and the frame list. See the module doc.
        let list_mask = if version.has_mask() {
            u32_at(payload, "imag slot lists", start + 0x0c)?
        } else {
            0
        };
        let (lists, lists_end) =
            Self::read_lists(payload, start + version.header() + blocks_len, list_mask)?;

        let table_at = lists_end + (version.base() - version.header());

        if table_at < start + header || table_at + len > end {
            return Err(Error::BrokenInvariant {
                what: "imag frame table does not fit its slot",
                expected: end as u64,
                found: (table_at + len) as u64,
            });
        }

        let table = slice(payload, "imag frame table", table_at, len)?;

        let (playback_flag, effect_mask) = if version.has_playback() {
            (
                u16_at(payload, "imag playback flag", start + PLAYBACK_FLAG_AT)? != 0,
                if matches!(version, Version::Five) {
                    u32_at(payload, "imag effect mask", start + EFFECT_MASK_AT)?
                } else {
                    0
                },
            )
        } else {
            (false, 0)
        };

        let blocks_at = if version.has_mask() {
            start + version.header()
        } else {
            start
        };
        let blocks = slice(
            payload,
            "imag attachment blocks",
            blocks_at,
            8 * block_mask.count_ones() as usize,
        )?;

        // Inside the slot: the table check above bounds it.
        let trailer = &payload[table_at + len..end];
        let (playback, effects) = Self::read_trailer(trailer, playback_flag, effect_mask)?;

        slice(payload, "imag slot control", start + 4, header - 4)?;

        let mut slot = Slot {
            x,
            y,
            footprint: [0; 4],
            attachments,
            block_mask,
            effect_mask,
            blocks,
            lists,
            playback,
            effects,
            points: slice(
                payload,
                "imag slot point list",
                start + header,
                table_at - start - header,
            )?,
            frames,
            streams,
            table,
            trailer,
        };

        // Footprint: block 0, the first two points.
        let [(left, top), (right, bottom), ..] = slot.points();

        slot.footprint = [left, top, right, bottom];

        Ok(slot)
    }

    /// The point lists at `at`, one per set bit of `mask`, and where they
    /// end.
    ///
    /// # Errors
    ///
    /// If a list runs past the payload.
    fn read_lists(payload: &[u8], at: usize, mask: u32) -> Result<(Vec<Points>, usize)> {
        let mut r = Cursor::at_offset(payload, at);
        let lists = (0..mask.count_ones())
            .map(|_| {
                let n = r.count("imag point list count")?;

                (0..n)
                    .map(|_| Ok((r.i32("imag point x")?, r.i32("imag point y")?)))
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;

        Ok((lists, r.at()))
    }

    /// A slot's trailer: a [`Playback`] when `flagged`, then one effect list
    /// per set bit of `effect_mask`. Bytes past them are left: a version 2
    /// slot's slack, which the loader seeks past.
    ///
    /// # Errors
    ///
    /// If the trailer is short of what the header announces.
    fn read_trailer(
        trailer: &[u8],
        flagged: bool,
        effect_mask: u32,
    ) -> Result<(Option<Playback>, Vec<Effect>)> {
        let mut r = Cursor::new(trailer);
        let playback = if flagged {
            Some(Playback::read(&r.array("imag slot playback")?))
        } else {
            None
        };
        let mut effects = Vec::new();

        for _ in 0..effect_mask.count_ones() {
            let count = r.count("imag effect count")?;

            for _ in 0..count {
                effects.push(Effect::read(&r.array("imag effect")?));
            }
        }

        Ok((playback, effects))
    }
}
