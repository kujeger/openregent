//! Reader for the `D*` descriptor payloads: the simulation's own definitions.
//!
//! Nine chunk tags: `DUNT` units and buildings, `DACT` spells and actions,
//! `DMOV` movement classes, `DTPT` and `DTRN` terrain, `DSND`, `DSDP` and
//! `DSDG` sound, `DFCT` factions. 1,438 records in HD.
//!
//! Each body repeats the 16-byte header, so a payload is a tree:
//!
//! ```text
//! [char[4] tag] [u32 length] [u32 version] [u32 child count]
//!
//! DUNT  version 0, 1 child
//! └── DATA  version 0, 3 children
//!     ├── HEAD   4-char id, a word, then two NUL-terminated names
//!     ├── PRIM   the SDK XML's <Engine> section
//!     └── SECD   the SDK XML's <Game> section
//! ```
//!
//! Section names per the SDK's `How To Make A Quest.pdf`: Engine is shared by
//! every unit subtype, Game varies by subtype.
//!
//! # A declared length can undercount
//!
//! `DUNT` children tile their parent exactly. In `DACT` and `DSND` a leaf's
//! length covers its fixed fields only; trailing strings sit outside it.
//! `DACT A010 Henchman_death` declares a 64-byte `PRIM`, then carries
//! `henchman_dead\0` in the 22 bytes before `SECD`.
//!
//! So the walk trusts the child count, not the lengths. A misplaced sibling is
//! placed by constraint: the last child ends exactly at the parent's end. On all
//! 140 three-child `DACT` records (`action.cam` 86, `mx_action.cam` 49,
//! `mdl1_action.cam` 5) that pins `SECD`. A forward scan for tag-shaped bytes
//! would hit `summ`, `Tele`, `Fire` inside `summon_`, `Teleport`, `Fireball`.
//!
//! All 1,438 records decompose. `DATA` and `BTDT` payloads are not descriptors.

mod action;
mod atch;
mod part;
mod particle;
mod sound;
mod terrain;
mod unit;
pub mod xml;

pub use action::Action;
pub use part::{Head, Part};
pub use particle::{EmitterFlag, EmitterShape, ParticleSystem};
pub use sound::{Phase, PhaseFlag, Sound, SoundGroup, spread};
pub use terrain::{Terrain, TerrainPart};
pub use unit::{
    Attachments, BUILDING_NUMBERS, Building, BuildingFlag, CHARACTER_NUMBERS, Character,
    CharacterFlag, Info, Number, NumberField, ShadowType, Tail, Unit, UnitKind, User,
};

use crate::data::error::{Error, Result, cstr};
use crate::data::fourcc::FourCc;

// --------------------------------------------------------------------------
// Typed views
// --------------------------------------------------------------------------
//
// Every offset below is solved. The HD SDK ships the same data as XML with
// named fields. Intersecting each XML value with every 4-byte-aligned
// position, over all records declaring the field, leaves one offset. The
// sample count is beside each field.
//
// Left out, under ten samples: `NameGenType` for buildings (1), `DialogID` for
// overlays (3), `SpellType` for actions (8).
//
// # Bitfields
//
// `Info` and `Flags` list one XML element per set bit. A bit is pinned when the
// records carrying it equal the records naming the flag; a superset is a
// different, overlapping field.
//
// Both editions solve together: the base game sets `NoFlaggable` and
// `NotSpellTarget` on the same 14 buildings; the expansion separates them.
// `Flags` solves per subtype, as `SECD`'s layout depends on it, and one flag
// name sits on different bits for buildings and characters, hence two enums.
//
// `CanUse` (`HumanPlayer`, `Monster`) matches no `PRIM` position: its word,
// `PRIM + 0x08`, is zero in every file, and the loader fills it from the id
// (`unit::User`).
//
// Four records read `0xcdcdcdcd`, MSVC's uninitialised-heap fill: `AVh1` (base),
// `BVt1`, `BVu1`, `BVx1` (expansion), all a recruit delay the SDK XML gives a
// value. Accessors return `None` for it, so a real 3,452,816,845 would read as
// absent; no game stat comes near.
//
// # Lengths are checked once
//
// Each record's `parse` checks that its sections hold every fixed field its
// accessors read, so an accessor is a plain read: `None` means
// uninitialised or blank, never short.

/// The fill MSVC leaves in uninitialised heap, which reached retail.
const UNINITIALISED: u32 = 0xcdcd_cdcd;

/// Where a record came from: an archive, or a `<Descriptions>` file made
/// into one ([`xml`]).
///
/// HD's archive loader fills a zero `CanUse` from the id;
/// its XML reader does not.
/// Nothing else told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Archive,
    Xml,
}

/// The root part's version [`xml`] writes. 0 on every archive record.
pub(crate) const XML_ORIGIN: u32 = 1;

impl Origin {
    /// Off a record's root part.
    #[must_use]
    pub const fn of(part: &Part<'_>) -> Origin {
        if part.version == XML_ORIGIN {
            Origin::Xml
        } else {
            Origin::Archive
        }
    }
}

/// Whether bit `bit` of the word at `at` is set.
fn bit(body: &[u8], at: usize, bit: u32) -> bool {
    field(body, at).is_some_and(|w| w >> bit & 1 == 1)
}

/// The `u32` at `at`, which `parse` has checked is there.
///
/// # Panics
///
/// Past the end: a length `parse` should have refused.
fn word(body: &[u8], at: usize) -> u32 {
    word_in(body, at).unwrap_or_else(|| {
        panic!(
            "descriptor field {at:#x} past a checked length {}",
            body.len()
        )
    })
}

/// The `u32` at `at`, or `None` past the end: for walks over a
/// variable-length tail, which check their own extent.
fn word_in(body: &[u8], at: usize) -> Option<u32> {
    body.get(at..)?
        .first_chunk::<4>()
        .map(|b| u32::from_le_bytes(*b))
}

/// A `u32` field, or `None` if it is uninitialised.
fn field(body: &[u8], at: usize) -> Option<u32> {
    let raw = word(body, at);

    (raw != UNINITIALISED).then_some(raw)
}

/// An `f32` field, or `None` if it is uninitialised.
fn float(body: &[u8], at: usize) -> Option<f32> {
    field(body, at).map(f32::from_bits)
}

/// A four-character id field, or `None` if it is zero or uninitialised.
///
/// Any other word is an id, printable or not: a mod's XML names records
/// by CRC-32 ([`xml`], `MBPortal`), and the engine compares words.
fn id_field(body: &[u8], at: usize) -> Option<FourCc> {
    field(body, at).filter(|w| *w != 0).map(FourCc::from_word)
}

/// The descendant `tag`, which the record needs.
fn require<'p, 'a>(part: &'p Part<'a>, tag: [u8; 4]) -> Result<&'p Part<'a>> {
    part.find(&tag).ok_or(Error::Missing { tag: FourCc(tag) })
}

/// `body` if it holds at least `len` bytes: the fixed fields of a section.
const fn fixed<'a>(body: &'a [u8], what: &'static str, len: usize) -> Result<&'a [u8]> {
    if body.len() >= len {
        Ok(body)
    } else {
        Err(Error::Truncated {
            what,
            need: len,
            have: body.len(),
        })
    }
}
