//! `DTRN` and `DTPT` — the tileset and the terrain classes in it.
//!
//! `terrtype.cam` holds one `DTRN` and ten `DTPT`; the expansion's one and
//! eleven. The `DTRN`'s `PRIM` has a `TPRT` table of terrain classes; slot
//! index is the `MapTexture` letter (0 is `A`, 10 is `K`).
//!
//! ```text
//! terrtype.cam                       tilesetdata.cam
//!   DTRN "GRIT" ──same id──────────────> IMAG "GRIT"
//!     └─ TPRT slot 1 = "GR01" ─┐           └─ MapTextureBtoB, MapTextureAtoB
//!                              │              ^
//!   DTPT "GR01" Plains_parent <┘              slot 1 is B
//! ```
//!
//! Take the letter from the table, not record order: `mx_terrtype.cam` runs
//! `GR00, GR10, GR01, GR02, …`, so record 1 is `Snow_child` but slot 1 is
//! `Plains_parent`.
//!
//! Not decoded:
//!
//! - A `DTPT`'s `PRIM`: 29 zero bytes in all 21 records of both editions.
//! - The 61 bytes before `TPRT`: id, name, zero padding. Only two samples, both
//!   `GRIT Gritty`, so field intersection cannot apply. Awaits the tier-2 RTTI
//!   walk.

use crate::data::error::{Error, Result};
use crate::data::fourcc::FourCc;

use super::atch::{self, Slots};
use super::{Head, Part, require};

/// Where the `ATCH` subtree starts in a `DTRN`'s `PRIM`.
///
/// Preceded by `"GRIT"`, `"Gritty"` and padding. **Inferred** from the two
/// shipped records.
const PROLOGUE_LEN: usize = 61;

/// The table binding a `MapTexture` letter to its `DTPT` record.
const CLASS_TABLE: [u8; 4] = *b"TPRT";

/// A `DTRN` record: the tileset descriptor.
#[derive(Debug, Clone, Copy)]
pub struct Terrain<'a> {
    pub head: Head<'a>,
    classes: Slots<'a>,
}

impl<'a> Terrain<'a> {
    /// # Errors
    ///
    /// If the tree lacks a `HEAD` or `PRIM`, `HEAD` will not parse, or `ATCH`
    /// is malformed. No `TPRT` yields an empty table.
    pub fn parse(part: &Part<'a>) -> Result<Self> {
        let head = Head::parse(require(part, *b"HEAD")?.body)?;
        let prim = require(part, *b"PRIM")?.body;

        prim.get(..PROLOGUE_LEN).ok_or(Error::Truncated {
            what: "terrain PRIM prologue",
            need: PROLOGUE_LEN,
            have: prim.len(),
        })?;

        let (tables, _) = atch::walk(prim, PROLOGUE_LEN)?;
        let classes = tables
            .unwrap_or_default()
            .into_iter()
            .find(|(tag, _)| *tag == &CLASS_TABLE)
            .map_or(Slots::EMPTY, |(_, slots)| slots);

        Ok(Terrain { head, classes })
    }

    /// Every class the table names, as `(letter, DTPT id)`.
    ///
    /// Blank slots are skipped: the expansion's 16-slot table yields 11 pairs,
    /// letters 0 to 10. Slots past 255 are skipped, not wrapped.
    pub fn classes(&self) -> impl Iterator<Item = (u8, FourCc)> + '_ {
        self.classes
            .filled()
            .filter_map(|(index, _, id)| Some((u8::try_from(index).ok()?, id)))
    }

    /// How many slots the table has room for, filled or not.
    ///
    /// 10 in the base game, 16 in the expansion, the `MapTexture` grid width
    /// (see [`crate::data::imag::SetKind::MapTexture`]).
    pub const fn capacity(&self) -> usize {
        self.classes.len()
    }
}

/// A `DTPT` record: one terrain class.
///
/// `PRIM` is kept whole, undecoded: 29 zero bytes in every shipped record.
#[derive(Debug, Clone, Copy)]
pub struct TerrainPart<'a> {
    pub head: Head<'a>,
    pub prim: &'a [u8],
}

impl<'a> TerrainPart<'a> {
    /// # Errors
    ///
    /// If the tree lacks a `HEAD` or `PRIM`, or `HEAD` will not parse.
    pub fn parse(part: &Part<'a>) -> Result<Self> {
        let head = Head::parse(require(part, *b"HEAD")?.body)?;
        let prim = require(part, *b"PRIM")?.body;

        Ok(TerrainPart { head, prim })
    }

    /// Whether `PRIM` is all zero, as in all 21 shipped records. For tests.
    pub fn is_blank(&self) -> bool {
        self.prim.iter().all(|b| *b == 0)
    }
}
