//! `ATCH` — a subtree of fixed-size slot tables.
//!
//! `DUNT` hangs `EXPL`, `MOVE` and `WEAP` tables off a unit; the single `DTRN`
//! hangs one `TPRT` table naming every terrain class in the tileset.
//!
//! ```text
//! ATCH  len  version  children
//!   TPRT  len  version  slots      len == slots * 8
//!     [u32 kind][char[4] id] * slots
//! ```
//!
//! The fourth word is a slot count, not a child count, so
//! [`super::Part::parse`] would descend into slot bytes. `len == slots * 8`
//! holds on all 21 `DUNT` tables and both `DTRN` ones.
//!
//! Tables are sparse: a `DUNT`'s `MOVE` fills one slot of eight, the
//! expansion's `TPRT` eleven of sixteen. [`Slots::filled`] keeps the index
//! because it is the meaning — for `TPRT`, the `MapTexture` letter.

use crate::data::error::{Error, Result, slice, u32_at};
use crate::data::fourcc::FourCc;

use super::part::HEADER_LEN;
use super::{id_field, word};

/// One `(action, sound)` pair, and one attachment slot: both 8 bytes.
pub(super) const PAIR_LEN: usize = 8;

/// A fixed-size table of 8-byte `(kind, id)` slots, most of them empty.
#[derive(Debug, Clone, Copy)]
pub struct Slots<'a>(pub(super) &'a [u8]);

impl<'a> Slots<'a> {
    /// A table that is not there, for a descriptor whose `ATCH` lacks a tag.
    pub(crate) const EMPTY: Slots<'a> = Slots(&[]);

    /// How many slots the table has room for, filled or not.
    pub const fn len(&self) -> usize {
        self.0.len() / PAIR_LEN
    }

    /// The occupied slots, with their index. An empty slot is eight zeroes.
    pub fn filled(&self) -> impl Iterator<Item = (usize, u32, FourCc)> + '_ {
        (0..self.len()).filter_map(|i| {
            let at = i * PAIR_LEN;
            let id = id_field(self.0, at + 4)?;

            Some((i, word(self.0, at), id))
        })
    }
}

/// What an `ATCH` subtree holds: each child's tag and its table.
pub(super) type Tables<'a> = Vec<(FourCc, Slots<'a>)>;

/// Walk the `ATCH` subtree at `at`, if one starts there.
///
/// Returns each child's tag and table, and where the subtree stops.
///
/// # Errors
///
/// If a child's length disagrees with its slot count, or the children do not
/// fill the parent exactly.
pub(super) fn walk(body: &[u8], at: usize) -> Result<(Option<Tables<'_>>, usize)> {
    if body.get(at..at + 4) == Some(b"ATCH") {
        let declared = u32_at(body, "attachment tree length", at + 4)? as usize;
        let children = u32_at(body, "attachment child count", at + 12)? as usize;

        let mut tables = Vec::new();
        let mut cur = at + HEADER_LEN;

        for _ in 0..children {
            let tag = FourCc::from_prefix(slice(body, "attachment block", cur, HEADER_LEN)?)?;
            let len = u32_at(body, "attachment block length", cur + 4)? as usize;
            let count = u32_at(body, "attachment slot count", cur + 12)? as usize;

            if len != count * PAIR_LEN {
                return Err(Error::BrokenInvariant {
                    what: "attachment slot count",
                    expected: len as u64,
                    found: (count * PAIR_LEN) as u64,
                });
            }

            tables.push((
                tag,
                Slots(slice(body, "attachment slots", cur + HEADER_LEN, len)?),
            ));
            cur += HEADER_LEN + len;
        }

        if cur == at + HEADER_LEN + declared {
            Ok((Some(tables), cur))
        } else {
            Err(Error::BrokenInvariant {
                what: "attachment children fill their parent",
                expected: (at + HEADER_LEN + declared) as u64,
                found: cur as u64,
            })
        }
    } else {
        Ok((None, at))
    }
}
