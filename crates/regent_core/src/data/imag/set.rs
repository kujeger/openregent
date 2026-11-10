//! One animation set, and what its id says it is for.

use std::fmt;

use super::{Playback, SET_CONTROL_LEN, SLOT_TABLE_AT, Slot, Version};
use crate::data::cursor::Cursor;
use crate::data::error::{Error, Result, array_at, u32_at};

/// The selection panel's portrait, 100 by 100, on every unit kind it shows.
/// See [`crate::view::panel::portrait`].
pub const INTERFACE_PORTRAIT: u16 = 1000;

/// The hero panel's backdrop, 200 by 245: stonework the panel and portrait
/// lie on.
pub const INTERFACE_PANEL: u16 = 1001;

/// The small icon a panel lists a unit with, 25 by 25.
///
/// [`SetKind::Interface`] spans a block of ids and a record carries up to
/// three, so only the id distinguishes them; set position varies (set 8 on
/// `AVb0 Veteran_City_Guard`, set 3 on `AVL1 Warrior`).
///
/// The data names no roles: HD's `ImageSetIDXRef.xml` says `Interface-00`
/// upward, and `Interface` is in neither executable. The engine hardcodes
/// purpose-to-id; see `docs/formats/art.md`.
///
/// The three roles come from a Workshop hero mod that clones a stock `IMAG`,
/// held by `a_workshop_hero_uses_the_stock_interface_ids`.
pub const INTERFACE_ICON: u16 = 1002;

/// What a set is for. The low 16 bits of its packed id.
///
/// From HD's `Data/ImageSetIDXRef.xml`: a `NameToIDTable` of 383 entries,
/// 367 distinct ids, all covered here; `tests/game_files.rs` checks it.
///
/// Families run in blocks: `Die` is 96 to 103 (`Die-1` to `Die-8` in the
/// file). [`SetKind::from_id`] drops the ordinal; see [`name`] for it.
///
/// `Other` is anything outside the file. Two shipped ids are: 196 and 212, one
/// past the `Active` and `Inactive` blocks. Left unnamed rather than guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetKind {
    Walk,
    Stand,
    Attack,
    Special,
    Build,
    Die,
    Cast,
    Carry,
    Recoil,
    StandToWalk,
    WalkToStand,
    TurnRight,
    TurnLeft,
    Active,
    Inactive,
    Dead,
    Crumble,
    HighPowerActive,
    HighPowerIdle,
    LowPowerActive,
    LowPowerIdle,
    Unpowered,
    Minimap,
    Damage,
    Assimilate,
    Hotspot,
    SelectionUnderlay,
    SelectionOverlay,
    Interface,
    ParticleBirth,
    ParticleMidlife,
    ParticleDeath,
    /// One of the five unit skins, 4000 to 4004.
    UnitTexture,
    /// A terrain blend from one texture class over another.
    ///
    /// All 256 `MapTexture{A..P}to{A..P}`, at `5000 + 16 * from + to`.
    MapTexture {
        from: u8,
        to: u8,
    },
    Other(u16),
}

/// `MapTexture` grid start and row width.
const MAP_TEXTURE_BASE: u16 = 5000;
const MAP_TEXTURE_CLASSES: u16 = 16;

impl SetKind {
    /// Public so the cross reference can be checked id by id.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the match arm bounds the grid index to 0..=255"
    )]
    pub const fn from_id(id: u16) -> Self {
        match id {
            1..=4 => SetKind::Walk,
            8 => SetKind::Stand,
            16..=19 => SetKind::Attack,
            64..=67 => SetKind::Special,
            80..=83 => SetKind::Build,
            96..=103 => SetKind::Die,
            128..=131 => SetKind::Cast,
            144..=147 => SetKind::Carry,
            160..=163 => SetKind::Recoil,
            176 => SetKind::StandToWalk,
            177 => SetKind::WalkToStand,
            178 => SetKind::TurnRight,
            179 => SetKind::TurnLeft,
            192..=195 => SetKind::Active,
            208..=211 => SetKind::Inactive,
            224 => SetKind::Dead,
            240 => SetKind::Crumble,
            256 => SetKind::HighPowerActive,
            257 => SetKind::HighPowerIdle,
            272 => SetKind::LowPowerActive,
            273 => SetKind::LowPowerIdle,
            288 => SetKind::Unpowered,
            300 => SetKind::Minimap,
            316 => SetKind::Damage,
            332 => SetKind::Assimilate,
            400 => SetKind::Hotspot,
            500 => SetKind::SelectionUnderlay,
            550 => SetKind::SelectionOverlay,
            INTERFACE_PORTRAIT..=1040 => SetKind::Interface,
            2000 => SetKind::ParticleBirth,
            2100 => SetKind::ParticleMidlife,
            2200 => SetKind::ParticleDeath,
            4000..=4004 => SetKind::UnitTexture,
            5000..=5255 => {
                let n = id - MAP_TEXTURE_BASE;

                SetKind::MapTexture {
                    from: (n / MAP_TEXTURE_CLASSES) as u8,
                    to: (n % MAP_TEXTURE_CLASSES) as u8,
                }
            }
            other => SetKind::Other(other),
        }
    }
}

impl SetKind {
    /// The packed id of this kind's first member, variant 0: `Stand` 8,
    /// `Inactive` `0xd0`, `Special` `0x40`. [`SetKind::from_id`] takes it
    /// back.
    #[must_use]
    pub const fn id(self) -> u32 {
        let first: u16 = match self {
            SetKind::Walk => 1,
            SetKind::Stand => 8,
            SetKind::Attack => 16,
            SetKind::Special => 64,
            SetKind::Build => 80,
            SetKind::Die => 96,
            SetKind::Cast => 128,
            SetKind::Carry => 144,
            SetKind::Recoil => 160,
            SetKind::StandToWalk => 176,
            SetKind::WalkToStand => 177,
            SetKind::TurnRight => 178,
            SetKind::TurnLeft => 179,
            SetKind::Active => 192,
            SetKind::Inactive => 208,
            SetKind::Dead => 224,
            SetKind::Crumble => 240,
            SetKind::HighPowerActive => 256,
            SetKind::HighPowerIdle => 257,
            SetKind::LowPowerActive => 272,
            SetKind::LowPowerIdle => 273,
            SetKind::Unpowered => 288,
            SetKind::Minimap => 300,
            SetKind::Damage => 316,
            SetKind::Assimilate => 332,
            SetKind::Hotspot => 400,
            SetKind::SelectionUnderlay => 500,
            SetKind::SelectionOverlay => 550,
            SetKind::Interface => INTERFACE_PORTRAIT,
            SetKind::ParticleBirth => 2000,
            SetKind::ParticleMidlife => 2100,
            SetKind::ParticleDeath => 2200,
            SetKind::UnitTexture => 4000,
            SetKind::MapTexture { from, to } => {
                MAP_TEXTURE_BASE + MAP_TEXTURE_CLASSES * from as u16 + to as u16
            }
            SetKind::Other(id) => id,
        };

        first as u32
    }
}

impl fmt::Display for SetKind {
    /// Spelled as in `ImageSetIDXRef.xml`, so it can be grepped there.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let letter = |n: u8| char::from(b'A' + n);

        match self {
            SetKind::StandToWalk => f.write_str("Stand-to-Walk"),
            SetKind::WalkToStand => f.write_str("Walk-to-Stand"),
            SetKind::TurnRight => f.write_str("Turn-Right"),
            SetKind::TurnLeft => f.write_str("Turn-Left"),
            SetKind::HighPowerActive => f.write_str("High-Power-Active"),
            SetKind::HighPowerIdle => f.write_str("High-Power-Idle"),
            SetKind::LowPowerActive => f.write_str("Low-Power-Active"),
            SetKind::LowPowerIdle => f.write_str("Low-Power-Idle"),
            SetKind::SelectionUnderlay => f.write_str("Selection-Underlay"),
            SetKind::SelectionOverlay => f.write_str("Selection-Overlay"),
            SetKind::ParticleBirth => f.write_str("Particle-Birth"),
            SetKind::ParticleMidlife => f.write_str("Particle-Midlife"),
            SetKind::ParticleDeath => f.write_str("Particle-Death"),
            SetKind::MapTexture { from, to } => {
                write!(f, "MapTexture{}to{}", letter(*from), letter(*to))
            }
            SetKind::Other(id) => write!(f, "set-{id}"),
            named => write!(f, "{named:?}"),
        }
    }
}

/// Widest numbered block (`Die`). Longer runs (`Interface`, `MapTexture`)
/// are not numbered.
const BLOCK_MAX: u16 = 8;

/// A set id as its family and its member number within that family.
///
/// The member matters: on a building the eight `Die` ids are damage stages.
/// Counts from 1; the block is found by walking [`SetKind::from_id`], so no
/// second table can drift.
///
/// Matches `ImageSetIDXRef.xml` on 49 of its 97 numbered entries. The file also
/// numbers single-member families (`Crumble-1`, `Hotspot-1`, five more) and
/// `Interface-00` to `Interface-40`. Held by
/// `image_set_kinds_match_the_cross_reference`.
#[must_use]
pub fn name(id: u16) -> String {
    let same = |n: u16| SetKind::from_id(n) == SetKind::from_id(id);

    // Block extent each way, up to the cap.
    let below = (1..=BLOCK_MAX)
        .take_while(|n| id.checked_sub(*n).is_some_and(same))
        .count();
    let above = (1..=BLOCK_MAX)
        .take_while(|n| id.checked_add(*n).is_some_and(same))
        .count();

    if below + above == 0 || below + above >= usize::from(BLOCK_MAX) {
        SetKind::from_id(id).to_string()
    } else {
        format!("{}-{}", SetKind::from_id(id), below + 1)
    }
}

/// A set id's low half, its kind's number; the top byte is the
/// [`Set::variant`].
#[must_use]
pub const fn set_number(id: u32) -> u16 {
    (id & 0xffff) as u16
}

/// A set id less its top byte: a stacked layer's counter.
pub const SET_BASE: u32 = 0x00ff_ffff;

/// One animation set: a pose, and a slot per facing or variant.
#[derive(Debug, Clone)]
pub struct Set<'a> {
    /// Packed id: low 16 bits the kind, top byte the [`Set::variant`].
    pub id: u32,
    /// `+0x04` to `+0x40`. `+0x14` to `+0x28` is the [`Set::playback`]; the
    /// rest is copied to the loaded set by the set readers.
    pub control: &'a [u8; SET_CONTROL_LEN],
    pub slots: Vec<Slot<'a>>,
}

/// Where the set's [`Playback`] sits in [`Set::control`]: set `+0x14`.
const SET_PLAYBACK_AT: usize = 0x10;

/// Where [`super::Image::particle_system`] sits in [`Set::control`]: set `+0x08`.
pub(super) const PARTICLES_AT: usize = 0x04;

/// Where [`Set::layer_flags`] sits in [`Set::control`]: set `+0x0c`.
const LAYER_FLAGS_AT: usize = 0x08;

/// Where [`Set::part_key`] sits in [`Set::control`]: set `+0x10`.
const PART_KEY_AT: usize = 0x0c;

impl Set<'_> {
    /// The set's default [`Playback`], or `None` when all five words are zero.
    ///
    /// **Binary**: every set reader passes set `+0x14` to `+0x24` to
    /// set playback assign, which keeps them at loaded set unless all zero.
    pub fn playback(&self) -> Option<Playback> {
        let words = Cursor::at_offset(self.control, SET_PLAYBACK_AT).array("imag set playback");

        words
            .ok()
            .map(|w| Playback::read(&w))
            .filter(|p| !p.is_empty())
    }

    /// The four [`Set::control`] bytes at `at`.
    pub(super) const fn bytes(&self, at: usize) -> [u8; 4] {
        let c = self.control;

        [c[at], c[at + 1], c[at + 2], c[at + 3]]
    }

    /// Set header, the loaded set's layer flags (`ImagSet`).
    pub const fn layer_flags(&self) -> u32 {
        u32::from_le_bytes(self.bytes(LAYER_FLAGS_AT))
    }

    /// Set header, the loaded set's, which `UnitIMG`
    /// copies to each of the layer's parts; play set parts
    /// gives only a part keyed 0 an `Animate`. **Corpus**: not 0 only on
    /// interface sets.
    pub const fn part_key(&self) -> u32 {
        u32::from_le_bytes(self.bytes(PART_KEY_AT))
    }

    pub const fn kind(&self) -> SetKind {
        SetKind::from_id(set_number(self.id))
    }

    /// Which layer of its kind this is, within the record.
    ///
    /// The top byte; bits 16 to 23 are zero on all 4,031 install sets.
    pub const fn variant(&self) -> u8 {
        (self.id >> 24) as u8
    }
}

impl<'a> Set<'a> {
    /// The set at `start`, ending where `payload` does.
    pub(super) fn parse(
        payload: &'a [u8],
        version: Version,
        id: u32,
        start: usize,
    ) -> Result<Self> {
        let end = payload.len();
        let count = u32_at(payload, "imag slot count", start)? as usize;

        if count == 0 {
            return Err(Error::BadValue {
                what: "imag slot count",
                value: 0,
            });
        }

        let control = array_at(payload, "imag set control", start + 4)?;
        let table_end = SLOT_TABLE_AT + 4 * count;

        let offsets = (0..count)
            .map(|k| {
                u32_at(payload, "imag slot offset", start + SLOT_TABLE_AT + 4 * k)
                    .map(|o| o as usize)
            })
            .collect::<Result<Vec<_>>>()?;

        // Offsets must ascend and not land inside the slot table. One slot at
        // 0x44 is the tight case, so the bound is inclusive.
        if offsets.windows(2).any(|p| p[0] >= p[1]) || offsets[0] < table_end {
            return Err(Error::BrokenInvariant {
                what: "imag slot offsets",
                expected: table_end as u64,
                found: offsets[0] as u64,
            });
        }

        // Every slot starts inside the set, so each ends inside it too.
        let last = start + offsets[count - 1];

        if last >= end {
            return Err(Error::OutOfBounds {
                what: "imag slot past its set",
                offset: last,
                len: 0,
                size: end,
            });
        }

        let slots = offsets
            .iter()
            .enumerate()
            .map(|(k, &off)| {
                let slot_end = offsets.get(k + 1).map_or(end, |&next| start + next);

                Slot::parse(payload, version, start + off, slot_end)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Set { id, control, slots })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_kinds_cover_their_families() {
        assert_eq!(SetKind::from_id(1), SetKind::Walk);
        assert_eq!(SetKind::from_id(99), SetKind::Die);
        assert_eq!(SetKind::from_id(1002), SetKind::Interface);
        assert_eq!(SetKind::from_id(7), SetKind::Other(7));
        assert_eq!(SetKind::Die.to_string(), "Die");
        assert_eq!(SetKind::Other(7).to_string(), "set-7");
    }

    #[test]
    fn the_map_texture_grid_is_sixteen_by_sixteen() {
        // Corners and one interior cell.
        for (id, from, to, name) in [
            (5000, 0, 0, "MapTextureAtoA"),
            (5015, 0, 15, "MapTextureAtoP"),
            (5016, 1, 0, "MapTextureBtoA"),
            (5170, 10, 10, "MapTextureKtoK"),
            (5255, 15, 15, "MapTexturePtoP"),
        ] {
            assert_eq!(SetKind::from_id(id), SetKind::MapTexture { from, to });
            assert_eq!(SetKind::from_id(id).to_string(), name);
        }

        assert_eq!(SetKind::from_id(4004), SetKind::UnitTexture);
        assert_eq!(SetKind::from_id(4005), SetKind::Other(4005));
        assert_eq!(SetKind::from_id(5256), SetKind::Other(5256));
    }

    #[test]
    fn a_kinds_id_is_its_first_member() {
        let first = (0..=u16::MAX).filter(|&id| {
            SetKind::from_id(id) != SetKind::Other(id)
                && (id == 0 || SetKind::from_id(id - 1) != SetKind::from_id(id))
        });

        for id in first {
            assert_eq!(SetKind::from_id(id).id(), u32::from(id), "{id}");
        }

        assert_eq!(SetKind::Other(7).id(), 7);
    }

    #[test]
    fn hyphenated_names_match_the_cross_reference() {
        // Spelled as in the file.
        assert_eq!(SetKind::from_id(176).to_string(), "Stand-to-Walk");
        assert_eq!(SetKind::from_id(500).to_string(), "Selection-Underlay");
        assert_eq!(SetKind::from_id(2100).to_string(), "Particle-Midlife");
        assert_eq!(SetKind::from_id(256).to_string(), "High-Power-Active");
    }

    /// A set's layer number is the id's top byte, not its top half.
    ///
    /// Bits 16 to 23 are zero on all 4,031 install sets; a `u16` read would
    /// give 256 for 1.
    #[test]
    fn a_sets_variant_is_the_top_byte_of_its_id() {
        let set = |id: u32| Set {
            id,
            control: &[0; SET_CONTROL_LEN],
            slots: Vec::new(),
        };

        assert_eq!(set(0x0000_00d0).variant(), 0);
        assert_eq!(set(0x0100_00d0).variant(), 1);
        assert_eq!(set(0x0500_0067).variant(), 5);

        // The kind is unaffected.
        assert_eq!(set(0x0500_0067).kind(), SetKind::Die);
    }
}
