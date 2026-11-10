//! Reader for `IMAG` payloads: animation metadata over [`super::tile`].
//!
//! A record says which tiles a unit, building or effect plays, in which order,
//! per pose and facing. No pixels.
//!
//! ```text
//! record  +0x00  u32       version — 2, 3, 4 or 5
//!         +0x14  u32       set count
//!         +0x18  8×n       set table: { u32 set_id, u32 offset }
//!
//! set at the table's offset, ending where the next entry begins
//!         +0x00  u32       slot count
//!         +0x04  60 bytes  control; +0x14 to +0x28 the set's [`Playback`]
//!         +0x40  4×slots   slot offsets, relative to the set
//!
//! slot, ending where the next slot begins; HEADER is 0x10, 0x14, 0x18 for
//! versions 3, 4, 5
//!         +0x00  i16 i16   x and y offset (+0x18 in version 2)
//!         +0x04  u16 u16   streams, frames (+0x1c in version 2)
//!         +0x08  u32       block mask (versions 3, 4 and 5 only)
//!         +0x0c  u32       point-list mask (versions 3, 4 and 5 only)
//!         +0x10  u16       non-zero: a playback trailer follows (4 and 5)
//!         +0x12  u16       frame list count, 1 on every slot (4 and 5)
//!         +0x14  u32       effect-list mask (version 5)
//!         at HEADER, 8 bytes per set bit of the block mask
//!         per set bit of the point-list mask, [u32 n] then n (i32, i32)
//!         per frame list: a u32 (versions 4 and 5; 0 on every slot), then
//!                          the frame table, stream-major, 8 bytes each:
//!                          { u32 metadata, u32 reference }
//!         the trailer: a 20-byte [`Playback`] when +0x10 says so, then in
//!                          version 5 per set bit of the effect mask,
//!                          [u32 n] then n 32-byte [`Effect`]s
//! ```
//!
//! Version 2 has no masks: three blocks at `+0x00`, the offset at `+0x18`,
//! the counts at `+0x1c`, frames from `+0x20`.
//!
//! **Binary**: the slot readers imag slot v2,
//! Imag slot v3, imag slot v4 and
//! Imag slot v5, dispatched by version from
//! Imag slot read. Walked this way, every version 3 to 5 slot of HD and
//! the Workshop mods ends where the next begins, bar one mod slot.
//!
//! # Beyond the published descriptions
//!
//! `+0x00` is a version word: it sets the count word's position and the slot
//! header length. Third-party tools ignore it. Versions 2 and 3 number 98 and
//! 28 in all three editions; 4 and 5 grow per release. Version 5 ships only in
//! the expansion archives.
//!
//! The frame table follows the point lists, one per set bit of `+0x0c`.
//! Neither a fixed start nor end anchoring works alone: `ABo1 HallOfChampions`
//! has one slot with lists and one without, and a trailer follows the table.
//! 220 of the install's 13,373 slots have lists.
//!
//! All 86,794 references are tiles. A wrong anchoring yields 101 negatives
//! (−1 to −110), point-list bytes read as frames; [`FrameRef::Sentinel`] guards
//! against that.
//!
//! Every record parses: 634 Gold, 653 HD, 35 Workshop mods.
//!
//! # Slots
//!
//! Published tooling calls these directions, capped at eight. Counts run to 256
//! (`APA2 Ranger Arrow` has 32). Only on `Walk` and `Attack` are eight slots
//! compass directions; terrain scatter uses them for variants.

mod frame;
mod image;
mod pass;
mod set;
mod slot;

pub use frame::{Frame, FrameRef};
pub use image::Image;
pub use pass::{Pass, SORTS_OWN};
pub use set::{
    INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT, SET_BASE, Set, SetKind, name, set_number,
};
pub use slot::{Effect, Playback, Slot};

use super::error::{Error, Result};

const SET_COUNT_AT: usize = 0x14;
const SET_TABLE_AT: usize = 0x18;
const SET_ENTRY_LEN: usize = 8;
const SLOT_TABLE_AT: usize = 0x40;

/// A set's control bytes, `+0x04` to the slot table. See [`Set::control`].
const SET_CONTROL_LEN: usize = SLOT_TABLE_AT - 4;
const FRAME_LEN: usize = 8;

/// The word before each frame list in versions 4 and 5.
const FRAME_LIST_WORD: usize = 4;

/// Version 2's blocks: always three, at the slot start.
const V2_BLOCKS: u32 = 0b111;

/// Versions 4 and 5: set, a [`Playback`] opens the trailer.
const PLAYBACK_FLAG_AT: usize = 0x10;
const EFFECT_MASK_AT: usize = 0x14;
const PLAYBACK_LEN: usize = 20;
const EFFECT_LEN: usize = 32;

/// Reference bit 24: the frame draws in the unit's second pass only.
///
/// **Binary**: `IsoGenUnit` draws a frame only when this
/// bit equals the pass its layer is in (layer flags bits 8 to 11); the hit
/// test unit hit skips it.
pub const PASS: u32 = 1 << 24;

/// Reference bits 27 and 28: what the low 24 bits name. A two-bit field, not
/// two flags: zero is a `TILE`, [`MODEL`] a `CY3D` record, bit 27 alone an
/// object drawn through the renderer's own call.
///
/// **Binary**: `IsoGenUnit` masks `0x18000000`, then compares with
/// each value. Tile preloading skips any non-zero value.
pub const KIND: u32 = 0x1800_0000;

/// [`KIND`] value naming a `CY3D` record.
/// None ships.
pub const MODEL: u32 = 0x1000_0000;

/// [`KIND`] value drawn through the renderer's own call,
/// `IsoUnitIMG`: the `CreateEffector` value as text.
/// `Got_Gold`'s and `got_gold_bldg`'s second layer.
pub const TEXT: u32 = 0x0800_0000;

/// Reference bit 29: draw the tile mirrored.
///
/// **Binary**: `IsoGenUnit` shifts bit 29 down to sprite mirror,
/// which flips the tile until its [`super::tile::MIRRORED`] flag agrees.
pub const MIRROR: u32 = 1 << 29;

/// Reference bit 30: the frame is not drawn, loaded or hit-tested.
///
/// **Binary**: `IsoGenUnit` tests bit 30 and skips the frame; the preload
/// and frame box union skip it too.
pub const HIDDEN: u32 = 1 << 30;

/// Reference bit 31: an `InitAction` reaching this frame fires its
/// callback `0x1000` — a missile's launch, an action's function — before
/// its last frame.
///
/// **Binary**: `OrderAnimate` asks the part for the frame it stepped
/// onto, tests bit 31, and sets the flag
/// the last frame sets; init action peak runs once (order
/// bit 0). **Corpus**: in HD, casts and some attacks carry one.
pub(crate) const CUE: u32 = 1 << 31;

/// The set attachment points are read from: `Hotspot`, slot 0.
///
/// **Binary**: `IsoGenUnit` and method push 400 and ask
/// slot 0 for a point. See [`attachment_point`].
pub const ATTACHMENT_SET: u32 = SetKind::Hotspot.id();

/// The point an attachment's height is measured against.
///
/// **Binary**: `0x05` in `IsoGenUnit`, before the named point.
pub const ATTACHMENT_BASE: usize = 5;

/// Points a slot holds: two per block, three blocks.
const POINTS: usize = 6;

/// Blocks the engine keeps; a mask bit above these is read and dropped.
const BLOCKS: u32 = 3;

/// Which point an `AttachmentPointID` names, or `None` if the engine refuses it.
///
/// **Binary**: one-based. `GenUnit` reads an overlay's id
/// and turns 0 into 1; `IsoGenUnit` maps 1, 2, 3 to points
/// 0, 1, 2 and fails on anything
/// else, falling back to set `0xd0`. So 0 is the default, not "none", and the
/// id is no mask bit: points pack two to a block.
pub const fn attachment_point(id: u32) -> Option<usize> {
    match id {
        0 | 1 => Some(0),
        2 => Some(1),
        3 => Some(2),
        _ => None,
    }
}

/// Which layout a record uses.
///
/// Every offset depends on it, so an unknown value is an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    Two,
    Three,
    Four,
    Five,
}

impl Version {
    fn parse(word: u32) -> Result<Self> {
        match word {
            2 => Ok(Version::Two),
            3 => Ok(Version::Three),
            4 => Ok(Version::Four),
            5 => Ok(Version::Five),
            other => Err(Error::BadValue {
                what: "imag version",
                value: u64::from(other),
            }),
        }
    }

    /// Slot header length: where the blocks start. Version 2 is fixed,
    /// its three blocks inside.
    const fn header(self) -> usize {
        match self {
            Version::Two => 0x20,
            Version::Three => 0x10,
            Version::Four => 0x14,
            Version::Five => 0x18,
        }
    }

    /// Where the frame table follows the blocks, with no point lists: the
    /// header plus, in versions 4 and 5, the frame list's leading word.
    const fn base(self) -> usize {
        match self {
            Version::Two | Version::Three => self.header(),
            Version::Four | Version::Five => self.header() + FRAME_LIST_WORD,
        }
    }

    /// Whether the header carries the playback flag at `+0x10`.
    const fn has_playback(self) -> bool {
        matches!(self, Version::Four | Version::Five)
    }

    /// Offset of the frame and stream counts within a slot.
    const fn count_at(self) -> usize {
        match self {
            Version::Two => 0x1c,
            _ => 0x04,
        }
    }

    /// Version 2 has no attachment mask; the word at `+0x08` is other data.
    const fn has_mask(self) -> bool {
        !matches!(self, Version::Two)
    }

    pub const fn number(self) -> u32 {
        match self {
            Version::Two => 2,
            Version::Three => 3,
            Version::Four => 4,
            Version::Five => 5,
        }
    }
}

/// Which blit draws a tile, deciding what a high index means.
///
/// The tile cannot say; the caller must (see `docs/formats/art.md`). Both readings
/// occur: the Warrior's panel picture gives 247–255 muted colours; the modal
/// dialog is 407,865 pixels of index 247 on a magenta marker, a 50 % scrim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Blit {
    /// 247–254 are an operation, 255 a hole, whatever the palette. **Binary**.
    World,
    /// The palette decides per index. See `view::raster::shade::marker`.
    Interface,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attachment_id_is_one_based_and_zero_means_one() {
        assert_eq!(attachment_point(0), Some(0));
        assert_eq!(attachment_point(1), Some(0));
        assert_eq!(attachment_point(2), Some(1));
        assert_eq!(attachment_point(3), Some(2));
        assert_eq!(attachment_point(4), None);
        assert_eq!(
            attachment_point(13_001),
            None,
            "a shipped particle system's id"
        );
    }
}
