//! Which interface picture a thing gets, when only the executable says.
//!
//! An `IMAG` record lists pictures, not their purpose. The
//! [`Interface`](crate::data::imag::SetKind::Interface) family is ids 1000 to 1040;
//! HD's `ImageSetIDXRef.xml` names them only `Interface-00` upward, and neither
//! executable contains the string `Interface`.
//!
//! **Inferred**: the mapping is in code. This module holds the part read so far,
//! from a g++ build; that evidence is in `local/archive/linux-port.md`.
//!
//! # A spell's icon
//!
//! One function switches on a spell's descriptor id and picks a [`SPELLBOOK`]
//! set.
//!
//! 23 spells, ids 1001 to 1023 in guild order; others get
//! [`SPELL_ICON_FALLBACK`]. `INTn` holds exactly `Interface` 1001 to 1024.

use crate::data::fourcc::FourCc;

/// The `IMAG` record the spell icons live in — the spell book's own art.
pub const SPELLBOOK: FourCc = FourCc::new(b"INTn");

/// What the switch gives an unnamed spell: `SRa1`'s id, the last of the 23.
pub const SPELL_ICON_FALLBACK: u16 = 1023;

/// Which [`SPELLBOOK`] set each spell draws, in the order the ids run.
///
/// **Binary**: from the executable's comparison chain. Grouped by guild (`WR`
/// wizard, `HR`, `MR`, `CR`, `PR`, then three singletons), ids ascending.
pub const SPELL_ICONS: [(FourCc, u16); 23] = [
    (FourCc::new(b"WRa1"), 1001),
    (FourCc::new(b"WRb1"), 1002),
    (FourCc::new(b"WRc1"), 1003),
    (FourCc::new(b"WRd1"), 1004),
    (FourCc::new(b"WRe1"), 1005),
    (FourCc::new(b"WRf1"), 1006),
    (FourCc::new(b"WRg1"), 1007),
    (FourCc::new(b"HRa1"), 1008),
    (FourCc::new(b"HRb1"), 1009),
    (FourCc::new(b"HRc1"), 1010),
    (FourCc::new(b"MRa1"), 1011),
    (FourCc::new(b"MRb1"), 1012),
    (FourCc::new(b"MRc1"), 1013),
    (FourCc::new(b"MRd1"), 1014),
    (FourCc::new(b"CRa1"), 1015),
    (FourCc::new(b"CRb1"), 1016),
    (FourCc::new(b"CRc1"), 1017),
    (FourCc::new(b"PRa1"), 1018),
    (FourCc::new(b"PRb1"), 1019),
    (FourCc::new(b"PRc1"), 1020),
    (FourCc::new(b"LRa1"), 1021),
    (FourCc::new(b"ARa1"), 1022),
    (FourCc::new(b"SRa1"), 1023),
];

/// Which [`SPELLBOOK`] set this spell draws, falling back as the engine does.
///
/// ```
/// use regent_core::data::fourcc::FourCc;
/// use regent_core::view::panel::icons;
///
/// assert_eq!(icons::spell_icon(FourCc::new(b"WRa1")), 1001);
/// assert_eq!(icons::spell_icon(FourCc::new(b"ZZZZ")), icons::SPELL_ICON_FALLBACK);
/// ```
#[must_use]
pub fn spell_icon(id: FourCc) -> u16 {
    SPELL_ICONS
        .iter()
        .find(|(spell, _)| *spell == id)
        .map_or(SPELL_ICON_FALLBACK, |(_, set)| *set)
}
