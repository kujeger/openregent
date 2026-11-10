use std::fmt;

use super::cursor::Cursor;
use super::fourcc::FourCc;

/// Errors reading a Majesty data file.
///
/// Parsing is strict: an unexpected value is a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file is shorter than a structure it claims to contain.
    Truncated {
        what: &'static str,
        need: usize,
        have: usize,
    },
    /// A chunk the format requires is absent.
    Missing { tag: FourCc },
    /// A magic number or fixed tag did not match.
    BadMagic { what: &'static str, found: Vec<u8> },
    /// An offset or length points outside the file.
    OutOfBounds {
        what: &'static str,
        offset: usize,
        len: usize,
        size: usize,
    },
    /// A field held a value the format is not known to allow.
    BadValue { what: &'static str, value: u64 },
    /// A documented invariant did not hold.
    BrokenInvariant {
        what: &'static str,
        expected: u64,
        found: u64,
    },
    /// A file would not read, or changed since its index was read.
    Unread { path: String, why: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { what, need, have } => {
                write!(f, "truncated {what}: need {need} bytes, have {have}")
            }
            Error::Missing { tag } => write!(f, "no {tag} chunk"),
            Error::BadMagic { what, found } => {
                write!(f, "bad {what} magic: {:?}", String::from_utf8_lossy(found))
            }
            Error::OutOfBounds {
                what,
                offset,
                len,
                size,
            } => write!(
                f,
                "{what} at {offset:#x}+{len} runs past end of {size}-byte file"
            ),
            Error::BadValue { what, value } => write!(f, "unexpected {what}: {value}"),
            Error::BrokenInvariant {
                what,
                expected,
                found,
            } => write!(f, "{what}: expected {expected:#x}, found {found:#x}"),
            Error::Unread { path, why } => write!(f, "{path}: {why}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Borrow `len` bytes at `offset`, or fail with a located error.
pub(crate) fn slice<'a>(
    data: &'a [u8],
    what: &'static str,
    offset: usize,
    len: usize,
) -> Result<&'a [u8]> {
    let end = offset.checked_add(len);

    match end.filter(|e| *e <= data.len()) {
        Some(end) => Ok(&data[offset..end]),
        None => Err(Error::OutOfBounds {
            what,
            offset,
            len,
            size: data.len(),
        }),
    }
}

/// Borrow `N` bytes at `offset` as an array, or fail with a located error.
pub(crate) fn array_at<'a, const N: usize>(
    data: &'a [u8],
    what: &'static str,
    offset: usize,
) -> Result<&'a [u8; N]> {
    data.get(offset..)
        .and_then(<[u8]>::first_chunk)
        .ok_or(Error::OutOfBounds {
            what,
            offset,
            len: N,
            size: data.len(),
        })
}

/// Borrow up to the next NUL.
pub(crate) fn cstr<'a>(data: &'a [u8], what: &'static str) -> Result<&'a [u8]> {
    match data.iter().position(|b| *b == 0) {
        Some(end) => Ok(&data[..end]),
        None => Err(Error::Truncated {
            what,
            need: data.len() + 1,
            have: data.len(),
        }),
    }
}

/// Read a little-endian u32 at `offset`.
pub(crate) fn u32_at(data: &[u8], what: &'static str, offset: usize) -> Result<u32> {
    Cursor::at_offset(data, offset).u32(what)
}

/// `found` must be `expected`: a version, or a word the format fixes.
pub(crate) fn expect(what: &'static str, expected: u32, found: u32) -> Result<()> {
    if found == expected {
        Ok(())
    } else {
        Err(Error::BrokenInvariant {
            what,
            expected: u64::from(expected),
            found: u64::from(found),
        })
    }
}

/// Read a little-endian u16 at `offset`.
pub(crate) fn u16_at(data: &[u8], what: &'static str, offset: usize) -> Result<u16> {
    Cursor::at_offset(data, offset).u16(what)
}
