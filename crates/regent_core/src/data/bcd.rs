//! Reader and writer for `.bcd` files — the GPL virtual machine's program
//! images.
//!
//! `datamx/datasets.xml` lists them in `<LoadGPL>` elements.
//!
//! A `.bcd` is a tree of serialised blocks, framed as [`super::block`] says,
//! with no version word. A block at offset `X` whose leading `u32` is `L`
//! occupies bytes `X .. X+L-1`.
//!
//! One outer block wraps exactly three sections:
//!
//! ```text
//! 0x00  u32   file size - 4
//! 0x04  u32   file size - 4     the outer block's own length; it ends at EOF
//! 0x08  i32   -1                fixed pair in every file, meaning unknown
//! 0x0c  u32   0
//! 0x10        section 1         one variable-length entry per defined name
//!             section 2         three name tables; a fixed-size record array
//!                               per definition
//!             section 3         data — prototype instances from `.dat` files
//! ```
//!
//! Holds for all 75 `.bcd` files of HD, its modding SDK and the eleven Steam
//! Workshop mods.
//!
//! Section 2's three name tables have 20, 825 and 649 entries in the base game,
//! as in the sources; its definitions are those section 1 counts.
//!
//! Section 1 is the instruction stream, read by [`crate::gpl::bcd::walk`].
//! Field names mislead: [`Sections::symbols`] is section 1, [`Sections::code`]
//! section 2.
//!
//! This module does not read section contents; [`crate::gpl::bcd`] does.
//! **Inferred**: network messages are framed so too.

use super::block::{self, Block, TERMINATOR};
use super::error::{Error, Result, slice, u32_at};

/// Bytes before the first section: the file length, the outer block's length,
/// and the fixed `-1`, `0` pair.
const HEADER_LEN: usize = 16;
const SENTINEL: u32 = 0xffff_ffff;

/// The three top-level sections of a program image.
///
/// `symbols` and `code` are misnomers for sections 1 and 2.
#[derive(Debug, Clone, Copy)]
pub struct Sections<'a> {
    /// Section 1: per defined name, compiled code and debug info. Read by
    /// [`crate::gpl::bcd::walk`].
    pub symbols: Block<'a>,
    /// Section 2: three name tables, each definition's body a
    /// [`crate::gpl::bcd::Scope`].
    pub code: Block<'a>,
    /// Prototype instances, as compiled from the project's `.dat` files.
    pub data: Block<'a>,
}

/// A `.bcd` program image with its header validated and its body borrowed.
#[derive(Debug, Clone, Copy)]
pub struct Container<'a> {
    /// Everything after the 16-byte header, ending with the outer terminator.
    pub body: &'a [u8],
}

impl<'a> Container<'a> {
    /// Read a whole `.bcd` file.
    ///
    /// # Errors
    ///
    /// If the 16-byte header is short, the two length fields disagree, the
    /// sentinel pair is not `-1, 0`, or the declared length does not match the
    /// file.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let head = slice(data, "bcd header", 0, HEADER_LEN)?;
        let first = u32_at(head, "bcd length", 0)? as usize;
        let second = u32_at(head, "bcd length copy", 4)? as usize;
        let sentinel = u32_at(head, "bcd sentinel", 8)?;
        let trailing = u32_at(head, "bcd trailing", 12)?;

        // Holds for all 75 files; rejects a non-`.bcd` file early.
        if first != second {
            return Err(Error::BrokenInvariant {
                what: "bcd length copy",
                expected: first as u64,
                found: second as u64,
            });
        }

        if first + 4 != data.len() {
            return Err(Error::BrokenInvariant {
                what: "bcd declared length",
                expected: (data.len() - 4) as u64,
                found: first as u64,
            });
        }

        if sentinel != SENTINEL {
            return Err(Error::BadValue {
                what: "bcd sentinel",
                value: u64::from(sentinel),
            });
        }

        if trailing != 0 {
            return Err(Error::BadValue {
                what: "bcd trailing word",
                value: u64::from(trailing),
            });
        }

        // The outer block starts at 0x04 and runs to EOF, so the file's last
        // eight bytes are its terminator.
        Block::at(data, "bcd outer block", 4)?;

        Ok(Container {
            body: &data[HEADER_LEN..],
        })
    }

    /// The three sections, checked to tile the outer block's body exactly.
    ///
    /// # Errors
    ///
    /// If a section block is malformed, or there are not exactly three tiling
    /// the body.
    pub fn sections(&self) -> Result<Sections<'a>> {
        let region = &self.body[..self.body.len() - TERMINATOR.len()];
        let mut found = Vec::with_capacity(3);
        let mut offset = 0;

        while offset < region.len() {
            let block = Block::at(region, "bcd section", offset)?;
            offset += block.len;
            found.push(block);
        }

        // A short final block would leave `offset` past the region; a long one
        // would have failed the bounds check inside `Block::at`.
        match <[Block; 3]>::try_from(found) {
            Ok([symbols, code, data]) if offset == region.len() => Ok(Sections {
                symbols,
                code,
                data,
            }),
            Ok(_) => Err(Error::BrokenInvariant {
                what: "bcd sections fill the file",
                expected: region.len() as u64,
                found: offset as u64,
            }),
            Err(found) => Err(Error::BadValue {
                what: "bcd section count",
                value: found.len() as u64,
            }),
        }
    }
}

/// Assemble a complete `.bcd` from the three section bodies.
///
/// # Panics
///
/// If the finished image exceeds 4 GB, which its `u32` length fields cannot
/// express.
pub fn write_program(symbols: &[u8], code: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // Reserved for the file length, which is only known at the end.
    out.extend_from_slice(&0u32.to_le_bytes());

    block::write(&mut out, |out| {
        out.extend_from_slice(&SENTINEL.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());

        for section in [symbols, code, data] {
            block::write(out, |out| out.extend_from_slice(section));
        }
    });

    let len = u32::try_from(out.len() - 4).expect("program fits in a u32 length");
    out[..4].copy_from_slice(&len.to_le_bytes());

    out
}
