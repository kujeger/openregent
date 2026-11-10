//! A whole `IMAG` record, and the walk that reads it.

use super::set::PARTICLES_AT;
use super::{SET_COUNT_AT, SET_ENTRY_LEN, SET_TABLE_AT, Set, SetKind, Version};
use crate::data::error::{Error, Result, slice, u32_at};
use crate::data::fourcc::FourCc;

/// A decoded `IMAG` record.
#[derive(Debug, Clone)]
pub struct Image<'a> {
    pub version: Version,
    pub sets: Vec<Set<'a>>,
}

impl<'a> Image<'a> {
    /// Read the version word without decoding anything else.
    ///
    /// # Errors
    ///
    /// If the payload is under four bytes. An unknown version is not an error.
    pub fn version_word(payload: &[u8]) -> Result<u32> {
        u32_at(payload, "imag version", 0)
    }

    /// # Errors
    ///
    /// If the version is unknown, the header or a table is truncated, a set or
    /// slot table is empty or out of order, or a frame table does not fit its
    /// slot.
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        let version = Version::parse(Self::version_word(payload)?)?;
        slice(payload, "imag header", 0, SET_TABLE_AT)?;
        let count = u32_at(payload, "imag set count", SET_COUNT_AT)? as usize;

        if count == 0 {
            return Err(Error::BadValue {
                what: "imag set count",
                value: 0,
            });
        }

        let table: Vec<(u32, usize)> = (0..count)
            .map(|i| {
                let at = SET_TABLE_AT + SET_ENTRY_LEN * i;
                let id = u32_at(payload, "imag set id", at)?;
                let off = u32_at(payload, "imag set offset", at + 4)? as usize;

                Ok((id, off))
            })
            .collect::<Result<_>>()?;

        // A set runs to the next one's start, so the table must be in address
        // order (true in every corpus).
        let sets = table
            .iter()
            .enumerate()
            .map(|(i, &(id, start))| {
                let end = table.get(i + 1).map_or(payload.len(), |&(_, next)| next);

                if end <= start || end > payload.len() {
                    return Err(Error::BrokenInvariant {
                        what: "imag set order",
                        expected: start as u64,
                        found: end as u64,
                    });
                }

                // Cut at the set's end: no read of its may reach the next.
                Set::parse(&payload[..end], version, id, start)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Image { version, sets })
    }

    ///
    /// A kind may repeat: `ABl1 MagicBazaar1` has two `Inactive` and two
    /// `Active`, told apart by [`Set::variant`].
    pub fn sets_of(&self, kind: SetKind) -> impl Iterator<Item = &Set<'a>> {
        self.sets.iter().filter(move |s| s.kind() == kind)
    }

    /// The particle system a part playing `set` carries: set header
    /// `+0x08`, a `DUNT` id, versions 4 and 5 only.
    ///
    /// **Binary**: imag set v4 and imag set v5
    /// keep it at loaded set; `UnitIMG`
    /// makes one per part on a set change, method
    /// stops the old set's.
    #[must_use]
    pub fn particle_system(&self, set: &Set<'_>) -> Option<FourCc> {
        let word = Some(set.bytes(PARTICLES_AT)).filter(|w| *w != [0; 4]);

        match self.version {
            Version::Four | Version::Five => word.map(FourCc),
            Version::Two | Version::Three => None,
        }
    }

    /// The first set of a kind.
    pub fn set_of(&self, kind: SetKind) -> Option<&Set<'a>> {
        self.sets_of(kind).next()
    }

    pub fn slot_count(&self) -> usize {
        self.sets.iter().map(|s| s.slots.len()).sum()
    }

    pub fn frame_count(&self) -> usize {
        self.sets
            .iter()
            .flat_map(|s| &s.slots)
            .map(|s| usize::from(s.frames) * usize::from(s.streams))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        EFFECT_MASK_AT, FRAME_LEN, PLAYBACK_FLAG_AT, PLAYBACK_LEN, Playback, SLOT_TABLE_AT, Slot,
    };
    use super::*;

    /// Build a record of one set holding `slots` identical slots.
    ///
    /// `version`, `mask`, `trailer` and `points` each move the frame table.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the fixture sizes itself; every value is a small literal"
    )]
    fn record(
        version: u32,
        mask: u32,
        frames: u16,
        streams: u16,
        refs: &[u32],
        slots: usize,
        trailer: usize,
    ) -> Vec<u8> {
        laid_out(version, mask, frames, streams, refs, slots, trailer, 0)
    }

    /// As [`record`], plus a point list of `points` `(i32, i32)` pairs, with
    /// `+0x0c` set.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::too_many_arguments,
        reason = "the fixture sizes itself; every value is a small literal"
    )]
    fn laid_out(
        version: u32,
        mask: u32,
        frames: u16,
        streams: u16,
        refs: &[u32],
        slots: usize,
        trailer: usize,
        points: usize,
    ) -> Vec<u8> {
        let base = match version {
            2 => 0x20,
            3 => 0x10,
            4 => 0x18,
            _ => 0x1c,
        };
        let header = base + 8 * mask.count_ones() as usize;
        // Pairs, then the frame list's word.
        let listed = if points == 0 { 0 } else { points * 8 + 4 };
        let slot_len = header + listed + refs.len() * FRAME_LEN + trailer;

        let mut out = vec![0u8; SET_TABLE_AT];
        out[0..4].copy_from_slice(&version.to_le_bytes());
        out[SET_COUNT_AT..SET_COUNT_AT + 4].copy_from_slice(&1u32.to_le_bytes());

        out.extend_from_slice(&1u32.to_le_bytes()); // set id 1 = Walk
        let set = out.len() + 4;
        out.extend_from_slice(&(set as u32).to_le_bytes());

        let table_end = SLOT_TABLE_AT + 4 * slots;
        out.resize(set + table_end, 0);
        out[set..set + 4].copy_from_slice(&(slots as u32).to_le_bytes());

        for k in 0..slots {
            let at = set + SLOT_TABLE_AT + 4 * k;
            let off = table_end + k * slot_len;
            out[at..at + 4].copy_from_slice(&(off as u32).to_le_bytes());
        }

        for _ in 0..slots {
            let slot = out.len();
            out.resize(slot + slot_len, 0);

            let counts = (u32::from(frames) << 16) | u32::from(streams);
            let count_at = slot + if version == 2 { 0x1c } else { 0x04 };
            out[count_at..count_at + 4].copy_from_slice(&counts.to_le_bytes());

            if version != 2 {
                out[slot + 8..slot + 12].copy_from_slice(&mask.to_le_bytes());
            }

            if points > 0 {
                out[slot + 12..slot + 16].copy_from_slice(&1u32.to_le_bytes());

                // One list: its count ends the blocks, where version 4 and 5
                // keep the frame list's word when there are no lists.
                out[slot + header - 4..slot + header]
                    .copy_from_slice(&(points as u32).to_le_bytes());

                // Recognisable values, distinct from the frames.
                for i in 0..points {
                    let at = slot + header + i * 8;
                    out[at..at + 4].copy_from_slice(&(100u32 + i as u32).to_le_bytes());
                    out[at + 4..at + 8].copy_from_slice(&(200u32 + i as u32).to_le_bytes());
                }
            }

            for (i, r) in refs.iter().enumerate() {
                let at = slot + header + listed + i * FRAME_LEN;
                out[at..at + 4].copy_from_slice(&(0xd000 + i as u32).to_le_bytes());
                out[at + 4..at + 8].copy_from_slice(&r.to_le_bytes());
            }
        }

        out
    }

    fn only_slot(bytes: &[u8]) -> Slot<'_> {
        Image::parse(bytes).unwrap().sets[0].slots[0].clone()
    }

    #[test]
    fn the_version_word_moves_the_count_word() {
        let v4 = record(4, 0, 3, 1, &[10, 11, 12], 1, 0);
        let mut v2 = v4.clone();
        v2[0] = 2;

        assert_eq!(only_slot(&v4).frames, 3);
        // Version 2 looks at +0x1c, which holds nothing here.
        assert!(Image::parse(&v2).is_err());
    }

    #[test]
    fn a_version_two_slot_does_not_decode_to_nothing() {
        // At the version 4 offset a version 2 slot reads zero counts.
        let bytes = record(2, 0, 2, 1, &[7, 8], 1, 0);
        let slot = only_slot(&bytes);

        assert_eq!(slot.frames, 2);
        assert_eq!(slot.frames().count(), 2);
        assert_eq!(u32::from_le_bytes(bytes[0x64..0x68].try_into().unwrap()), 0);
    }

    /// Where the one slot of a one-slot fixture starts.
    const ONLY_SLOT_AT: usize = SET_TABLE_AT + SET_ENTRY_LEN + SLOT_TABLE_AT + 4;

    #[test]
    fn a_block_is_two_points_and_a_missing_block_is_zero() {
        // Bits 1 and 2: blocks at the header end, in bit order.
        let mut bytes = record(4, 0b110, 1, 1, &[5], 1, 0);
        let blocks_at = ONLY_SLOT_AT + Version::Four.header();
        let words: [i16; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

        for (i, w) in words.iter().enumerate() {
            bytes[blocks_at + 2 * i..blocks_at + 2 * i + 2].copy_from_slice(&w.to_le_bytes());
        }

        let slot = only_slot(&bytes);

        assert_eq!(
            slot.points(),
            [(0, 0), (0, 0), (1, 2), (3, 4), (5, 6), (7, 8)]
        );
        assert_eq!(slot.footprint, [0; 4], "no block 0, no footprint");
    }

    #[test]
    fn the_footprint_is_block_zero() {
        let mut bytes = record(3, 0b1, 1, 1, &[5], 1, 0);
        let at = ONLY_SLOT_AT + Version::Three.header();

        for (i, w) in [-8_i16, -4, 8, 4].iter().enumerate() {
            bytes[at + 2 * i..at + 2 * i + 2].copy_from_slice(&w.to_le_bytes());
        }

        assert_eq!(only_slot(&bytes).footprint, [-8, -4, 8, 4]);
    }

    #[test]
    fn a_playback_flag_opens_the_trailer() {
        let mut bytes = record(4, 0, 1, 1, &[5], 1, PLAYBACK_LEN);
        let trailer = ONLY_SLOT_AT + Version::Four.base() + FRAME_LEN;

        bytes[ONLY_SLOT_AT + PLAYBACK_FLAG_AT] = 1;
        bytes[trailer] = 2;
        bytes[trailer + 8] = 50;

        let slot = only_slot(&bytes);

        assert_eq!(
            slot.playback(),
            Some(Playback {
                kind: 2,
                flags: 0,
                value: 50,
                rest: [0; 2]
            })
        );
    }

    #[test]
    fn a_playback_flag_over_a_short_trailer_is_refused() {
        let mut bytes = record(4, 0, 1, 1, &[5], 1, PLAYBACK_LEN - 1);

        bytes[ONLY_SLOT_AT + PLAYBACK_FLAG_AT] = 1;

        assert!(matches!(
            Image::parse(&bytes).unwrap_err(),
            Error::OutOfBounds { .. }
        ));
    }

    #[test]
    fn an_effect_list_past_the_trailer_is_refused() {
        // One list, claiming two entries in a trailer with room for none.
        let mut bytes = record(5, 0, 1, 1, &[5], 1, 4);
        let trailer = ONLY_SLOT_AT + Version::Five.base() + FRAME_LEN;

        bytes[ONLY_SLOT_AT + EFFECT_MASK_AT] = 1;
        bytes[trailer] = 2;

        assert!(Image::parse(&bytes).is_err());
    }

    #[test]
    fn the_attachment_mask_lengthens_the_header() {
        // Same frames, four header lengths.
        for mask in [0b0, 0b1, 0b11, 0b111] {
            let bytes = record(4, mask, 2, 1, &[21, 22], 1, 0);
            let slot = only_slot(&bytes);

            assert_eq!(slot.attachments, mask, "mask {mask:#b}");
            assert_eq!(
                slot.frames().map(|f| f.reference).collect::<Vec<_>>(),
                [21, 22],
                "mask {mask:#b}"
            );
        }
    }

    #[test]
    fn a_trailer_does_not_move_the_frame_table() {
        // End-anchoring would read the 16-byte trailer as the two frames.
        let bytes = record(4, 0, 2, 1, &[31, 32], 1, 16);
        let slot = only_slot(&bytes);

        assert_eq!(slot.trailer().len(), 16);
        assert_eq!(
            slot.frames().map(|f| f.reference).collect::<Vec<_>>(),
            [31, 32]
        );
    }

    /// A point list pushes the frame table to the end of the slot.
    ///
    /// Counterpart of [`a_trailer_does_not_move_the_frame_table`].
    #[test]
    fn a_point_list_pushes_the_frame_table_to_the_end() {
        let bytes = laid_out(4, 0b101, 2, 1, &[31, 32], 1, 0, 4);
        let slot = only_slot(&bytes);

        assert_eq!(slot.points.len(), 4 * 8 + 4, "four pairs and the list word");
        assert_eq!(slot.point_lists().len(), 1, "one list per mask bit");
        assert!(
            slot.trailer().is_empty(),
            "the table is last, so nothing follows"
        );
        assert_eq!(
            slot.frames().map(|f| f.reference).collect::<Vec<_>>(),
            [31, 32],
            "the frames were read out of the point list"
        );
    }

    #[test]
    fn repeated_references_are_holds_and_survive() {
        // A repeat is a hold; deduplicating would speed animations up.
        let bytes = record(4, 0, 4, 1, &[7, 7, 7, 8], 1, 0);

        assert_eq!(
            only_slot(&bytes)
                .frames()
                .map(|f| f.reference)
                .collect::<Vec<_>>(),
            [7, 7, 7, 8]
        );
    }

    #[test]
    fn streams_are_stored_one_after_another() {
        // Stream-major, not interleaved: stream 1 is the second half.
        let bytes = record(4, 0, 3, 2, &[10, 11, 12, 20, 21, 22], 1, 0);
        let slot = only_slot(&bytes);

        assert_eq!(slot.streams, 2);
        assert_eq!(
            slot.stream(0).map(|f| f.reference).collect::<Vec<_>>(),
            [10, 11, 12]
        );
        assert_eq!(
            slot.stream(1).map(|f| f.reference).collect::<Vec<_>>(),
            [20, 21, 22]
        );
    }

    #[test]
    fn more_than_eight_slots_are_accepted() {
        // Published tooling caps at 8; the install goes to 256.
        let bytes = record(4, 0, 1, 1, &[5], 9, 0);
        let image = Image::parse(&bytes).unwrap();

        assert_eq!(image.sets[0].slots.len(), 9);
        assert_eq!(image.frame_count(), 9);
    }

    #[test]
    fn a_frame_table_that_does_not_fit_is_refused() {
        // Claim four frames in a slot sized for two.
        let mut bytes = record(4, 0, 2, 1, &[1, 2], 1, 0);
        bytes[0x68..0x6c].copy_from_slice(&0x0004_0001u32.to_le_bytes());

        assert!(Image::parse(&bytes).is_err());
    }

    #[test]
    fn empty_counts_are_refused() {
        let mut bytes = record(4, 0, 2, 1, &[1, 2], 1, 0);
        bytes[0x68..0x6c].copy_from_slice(&0u32.to_le_bytes());

        assert!(matches!(
            Image::parse(&bytes).unwrap_err(),
            Error::BadValue { .. }
        ));
    }

    #[test]
    fn a_slot_offset_inside_the_slot_table_is_refused() {
        let mut bytes = record(4, 0, 1, 1, &[5], 1, 0);
        // 0x40 lands on the table itself; the first legal value is 0x44.
        bytes[0x60..0x64].copy_from_slice(&0x40u32.to_le_bytes());

        assert!(matches!(
            Image::parse(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }

    #[test]
    fn a_slot_past_its_set_is_refused() {
        // Slot 1's offset, at set + 0x44, past the set's end.
        let mut bytes = record(4, 0, 1, 1, &[5], 2, 0);
        bytes[0x64..0x68].copy_from_slice(&0x1000u32.to_le_bytes());

        assert!(matches!(
            Image::parse(&bytes).unwrap_err(),
            Error::OutOfBounds { .. }
        ));
    }

    #[test]
    fn an_unknown_version_is_refused_but_still_readable() {
        let mut bytes = record(4, 0, 1, 1, &[5], 1, 0);
        bytes[0] = 6;

        assert_eq!(Image::version_word(&bytes).unwrap(), 6);
        assert!(matches!(
            Image::parse(&bytes).unwrap_err(),
            Error::BadValue { .. }
        ));
    }
}
