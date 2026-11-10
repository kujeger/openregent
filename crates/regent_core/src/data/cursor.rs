//! A forward reader over little-endian bytes, for formats read in order.
//!
//! Every read names what it reads, and fails with a located [`Error`]
//! rather than a default, so a misread is a finding:
//!
//! ```
//! use regent_core::data::cursor::Cursor;
//!
//! let mut r = Cursor::new(b"\x02\0\0\0name\0\x07\0");
//!
//! r.expect_u32("version", 2)?;
//! assert_eq!(r.str("name")?, "name");
//! assert_eq!(r.u16("count")?, 7);
//! assert!(r.done());
//! assert!(r.u8("past the end").is_err());
//! # Ok::<(), regent_core::data::Error>(())
//! ```

use super::error::{Error, Result, cstr, expect, slice};
use super::fourcc::FourCc;

/// A position in a byte slice, moved by each read.
#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Cursor { data, at: 0 }
    }

    /// A cursor over `data`, at `at`.
    #[must_use]
    pub const fn at_offset(data: &'a [u8], at: usize) -> Self {
        Cursor { data, at }
    }

    /// Where the next read starts.
    #[must_use]
    pub const fn at(&self) -> usize {
        self.at
    }

    /// The whole slice, read and unread.
    #[must_use]
    pub const fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Bytes not yet read.
    #[must_use]
    pub const fn left(&self) -> usize {
        self.data.len().saturating_sub(self.at)
    }

    /// Whether every byte has been read.
    #[must_use]
    pub const fn done(&self) -> bool {
        self.at >= self.data.len()
    }

    /// The next `len` bytes.
    ///
    /// # Errors
    ///
    /// Fewer than `len` left.
    pub fn bytes(&mut self, what: &'static str, len: usize) -> Result<&'a [u8]> {
        let got = slice(self.data, what, self.at, len)?;

        self.at += len;
        Ok(got)
    }

    /// The next `N` bytes, as an array.
    ///
    /// # Errors
    ///
    /// Fewer than `N` left.
    pub fn array<const N: usize>(&mut self, what: &'static str) -> Result<[u8; N]> {
        let got = self.bytes(what, N)?;
        let mut out = [0; N];

        out.copy_from_slice(got);
        Ok(out)
    }

    /// Step over `len` bytes.
    ///
    /// # Errors
    ///
    /// Fewer than `len` left.
    pub fn skip(&mut self, what: &'static str, len: usize) -> Result<()> {
        self.bytes(what, len).map(|_| ())
    }

    /// # Errors
    ///
    /// Nothing left.
    pub fn u8(&mut self, what: &'static str) -> Result<u8> {
        self.array::<1>(what).map(|[b]| b)
    }

    /// # Errors
    ///
    /// Under two bytes left.
    pub fn u16(&mut self, what: &'static str) -> Result<u16> {
        self.array(what).map(u16::from_le_bytes)
    }

    /// # Errors
    ///
    /// Under four bytes left.
    pub fn u32(&mut self, what: &'static str) -> Result<u32> {
        self.array(what).map(u32::from_le_bytes)
    }

    /// # Errors
    ///
    /// Under four bytes left.
    pub fn i32(&mut self, what: &'static str) -> Result<i32> {
        self.array(what).map(i32::from_le_bytes)
    }

    /// `N` `u32`s.
    ///
    /// # Errors
    ///
    /// Under `4 * N` bytes left.
    pub fn words<const N: usize>(&mut self, what: &'static str) -> Result<[u32; N]> {
        let mut out = [0; N];

        for word in &mut out {
            *word = self.u32(what)?;
        }

        Ok(out)
    }

    /// A four-character code, any bytes.
    ///
    /// # Errors
    ///
    /// Under four bytes left.
    pub fn fourcc(&mut self, what: &'static str) -> Result<FourCc> {
        self.array(what).map(FourCc)
    }

    /// A `u32` that must be `want`: a version, a tag, a count fixed by
    /// the format.
    ///
    /// # Errors
    ///
    /// Under four bytes left, or another value.
    pub fn expect_u32(&mut self, what: &'static str, want: u32) -> Result<()> {
        let found = self.u32(what)?;

        expect(what, want, found)
    }

    /// A `u32` count, at most the bytes left: a misread one cannot
    /// allocate gigabytes.
    ///
    /// # Errors
    ///
    /// Under four bytes left, or a count over the bytes left.
    pub fn count(&mut self, what: &'static str) -> Result<usize> {
        let n = self.u32(what)? as usize;

        if n <= self.left() {
            Ok(n)
        } else {
            Err(Error::BadValue {
                what,
                value: n as u64,
            })
        }
    }

    /// The bytes up to the next NUL, which is read too.
    ///
    /// # Errors
    ///
    /// No NUL before the end.
    pub fn cstr(&mut self, what: &'static str) -> Result<&'a [u8]> {
        let text = cstr(&self.data[self.at.min(self.data.len())..], what)?;

        self.at += text.len() + 1;
        Ok(text)
    }

    /// As [`Cursor::cstr`], as UTF-8.
    ///
    /// # Errors
    ///
    /// No NUL before the end, or not UTF-8.
    pub fn str(&mut self, what: &'static str) -> Result<&'a str> {
        let text = self.cstr(what)?;

        std::str::from_utf8(text).map_err(|_| Error::BadValue {
            what,
            value: text.len() as u64,
        })
    }

    /// A `u32` count, then that many of `one`.
    ///
    /// # Errors
    ///
    /// The count or any `one` fails.
    pub fn many<T>(
        &mut self,
        what: &'static str,
        mut one: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<Vec<T>> {
        let count = self.u32(what)?;

        (0..count).map(|_| one(self)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_past_the_end_fails_and_does_not_move() {
        let mut r = Cursor::new(&[1, 2, 3]);

        assert!(r.u32("word").is_err());
        assert_eq!(r.at(), 0);
        assert_eq!(r.u16("half").unwrap(), 0x0201);
        assert_eq!(r.left(), 1);
    }

    #[test]
    fn an_unexpected_version_is_an_error() {
        let mut r = Cursor::new(&[3, 0, 0, 0]);

        assert!(matches!(
            r.expect_u32("version", 2),
            Err(Error::BrokenInvariant {
                expected: 2,
                found: 3,
                ..
            })
        ));
    }

    #[test]
    fn a_count_over_the_bytes_left_is_refused() {
        let mut r = Cursor::new(&[9, 0, 0, 0, 1, 2]);

        assert!(matches!(
            r.count("n"),
            Err(Error::BadValue { value: 9, .. })
        ));
    }

    #[test]
    fn a_string_needs_its_terminator_and_utf8() {
        assert!(Cursor::new(b"open").cstr("s").is_err());
        assert!(Cursor::new(b"\xff\0").str("s").is_err());
        assert_eq!(Cursor::new(b"ok\0").str("s").unwrap(), "ok");
    }
}
