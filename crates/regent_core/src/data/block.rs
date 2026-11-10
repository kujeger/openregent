//! Block framing, the one shape `.bcd` images ([`super::bcd`]) and saves'
//! GPL chunk are built of.
//!
//! ```text
//! [u32 len]  [u32 version]  [body ...]  [u32 0]  [i32 -1]
//!  ^ len counts itself, the version, the body and the 8-byte terminator
//! ```
//!
//! A save's blocks carry the version word; a `.bcd`'s do not. No alignment
//! or padding.

use super::cursor::Cursor;
use super::error::{Error, Result};

/// Every block ends with `[u32 0][i32 -1]`; a save's writer closes its
/// blocks with them in block end write.
pub const TERMINATOR: [u8; 8] = [0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff];

/// The length word.
const LEN: usize = 4;

/// The version word, where a block has one.
const VERSION: usize = 4;

/// The length word plus the terminator: a block with an empty body and no
/// version.
const MIN_LEN: usize = LEN + TERMINATOR.len();

/// A block being read: where it started, and the size it claimed.
#[derive(Debug, Clone, Copy)]
pub struct Open {
    what: &'static str,
    start: usize,
    size: usize,
}

/// Opens the block at `r`: its length, and its version word when
/// `version` names the one expected.
///
/// # Errors
///
/// A word missing; with a version, a different version or a size too
/// small for the header and terminator; without, a size smaller than an
/// empty block.
pub fn open(r: &mut Cursor<'_>, what: &'static str, version: Option<u32>) -> Result<Open> {
    let start = r.at();
    let size = r.u32(what)? as usize;

    match version {
        Some(want) => {
            let found = r.u32(what)?;

            if found == want && size >= MIN_LEN + VERSION {
                Ok(Open { what, start, size })
            } else {
                Err(Error::BrokenInvariant {
                    what,
                    expected: u64::from(want),
                    found: u64::from(found),
                })
            }
        }
        None if size < MIN_LEN => Err(Error::BadValue {
            what,
            value: size as u64,
        }),
        None => Ok(Open { what, start, size }),
    }
}

/// Closes `block` at `r`: the terminator, ending where its size said.
///
/// # Errors
///
/// The terminator missing or wrong, or the block not ending at its size.
pub fn close(r: &mut Cursor<'_>, block: Open) -> Result<()> {
    let end = r.array::<8>(block.what)?;

    if end != TERMINATOR {
        Err(Error::BrokenInvariant {
            what: block.what,
            expected: u64::from_le_bytes(TERMINATOR),
            found: u64::from_le_bytes(end),
        })
    } else if r.at() - block.start != block.size {
        Err(Error::BrokenInvariant {
            what: block.what,
            expected: block.size as u64,
            found: (r.at() - block.start) as u64,
        })
    } else {
        Ok(())
    }
}

/// A block with no version word, its terminator checked and its body
/// borrowed.
#[derive(Debug, Clone, Copy)]
pub struct Block<'a> {
    /// The block's whole extent, including the length field and terminator.
    pub len: usize,
    /// The bytes between the length field and the terminator.
    pub body: &'a [u8],
}

impl<'a> Block<'a> {
    /// Read the block that starts at `offset` within `data`.
    ///
    /// # Errors
    ///
    /// If the length field is missing, shorter than an empty block, or runs
    /// past the end of `data`, or the terminator is wrong.
    pub fn at(data: &'a [u8], what: &'static str, offset: usize) -> Result<Self> {
        let mut r = Cursor::at_offset(data, offset);
        let block = open(&mut r, what, None)?;
        let body = r.bytes(what, block.size - MIN_LEN)?;

        close(&mut r, block)?;
        Ok(Block {
            len: block.size,
            body,
        })
    }
}

/// Append `[u32 len][body][u32 0][i32 -1]`, with `len` backfilled once the body
/// is written. The mirror of [`Block::at`].
///
/// # Panics
///
/// If the block exceeds 4 GB, which its `u32` length field cannot express.
pub fn write(out: &mut Vec<u8>, body: impl FnOnce(&mut Vec<u8>)) {
    let start = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());

    body(out);

    out.extend_from_slice(&TERMINATOR);

    // Truncating to u32 would write a corrupt file that still parses.
    let len = u32::try_from(out.len() - start).expect("block fits in a u32 length");
    out[start..start + LEN].copy_from_slice(&len.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_length_spans_the_whole_block() {
        let mut out = Vec::new();
        write(&mut out, |o| o.extend_from_slice(b"abc"));

        // 4 length + 3 body + 8 terminator, and no padding to a word boundary.
        assert_eq!(out.len(), 15);
        assert_eq!(u32::from_le_bytes(out[..4].try_into().unwrap()), 15);

        let block = Block::at(&out, "test", 0).expect("round trip");
        assert_eq!(block.body, b"abc");
        assert_eq!(block.len, 15);
    }

    #[test]
    fn a_missing_terminator_is_an_error() {
        let mut out = Vec::new();
        write(&mut out, |o| o.extend_from_slice(b"abc"));
        let last = out.len() - 1;
        out[last] = 0;

        assert!(Block::at(&out, "test", 0).is_err());
    }

    #[test]
    fn a_length_past_the_end_is_an_error() {
        let short = 99u32.to_le_bytes();

        assert!(Block::at(&short, "test", 0).is_err());
    }

    #[test]
    fn nested_blocks_round_trip() {
        let mut out = Vec::new();
        write(&mut out, |o| {
            write(o, |i| i.extend_from_slice(b"inner"));
        });

        let outer = Block::at(&out, "outer", 0).expect("outer");
        let inner = Block::at(outer.body, "inner", 0).expect("inner");

        assert_eq!(inner.body, b"inner");
        assert_eq!(inner.len, outer.body.len());
    }

    /// `[u32 len][u32 version][body][u32 0][i32 -1]`, as a save writes it.
    fn versioned(version: u32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();

        write(&mut out, |o| {
            o.extend_from_slice(&version.to_le_bytes());
            o.extend_from_slice(body);
        });
        out
    }

    #[test]
    fn a_versioned_block_opens_and_closes() {
        let bytes = versioned(3, b"ab");
        let mut r = Cursor::new(&bytes);
        let block = open(&mut r, "test", Some(3)).expect("opens");

        assert_eq!(r.bytes("test", 2).expect("body"), b"ab");
        close(&mut r, block).expect("closes");
        assert!(r.done());
    }

    #[test]
    fn another_version_is_refused() {
        let bytes = versioned(2, b"");

        assert!(matches!(
            open(&mut Cursor::new(&bytes), "test", Some(3)),
            Err(Error::BrokenInvariant {
                what: "test",
                expected: 3,
                found: 2
            })
        ));
    }

    #[test]
    fn a_body_read_short_of_its_size_does_not_close() {
        let bytes = versioned(0, b"ab");
        let mut r = Cursor::new(&bytes);
        let block = open(&mut r, "test", Some(0)).expect("opens");

        r.skip("test", 1).expect("one byte");
        assert!(close(&mut r, block).is_err());
    }
}
