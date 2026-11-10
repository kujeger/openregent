use std::fmt;

use super::error::{Error, Result};

/// A four-character tag: chunk type or record id.
///
/// Raw bytes: a tag may be unprintable, and a trailing space counts (`"CUT "`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FourCc(pub [u8; 4]);

impl FourCc {
    pub const fn new(tag: &[u8; 4]) -> Self {
        FourCc(*tag)
    }

    /// The first four bytes of `b`, which may be longer. [`TryFrom`] wants
    /// exactly four.
    ///
    /// # Errors
    ///
    /// If `b` is shorter than four bytes. Callers' bounds checks fail first.
    pub const fn from_prefix(b: &[u8]) -> Result<Self> {
        match b.first_chunk::<4>() {
            Some(tag) => Ok(FourCc(*tag)),
            None => Err(Error::Truncated {
                what: "four-character tag",
                need: 4,
                have: b.len(),
            }),
        }
    }

    pub const fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    /// The tag as the engine holds it in a word: its bytes little-endian.
    ///
    /// ```
    /// use regent_core::data::fourcc::FourCc;
    ///
    /// assert_eq!(FourCc(*b"ABCD").word(), 0x4443_4241);
    /// ```
    pub const fn word(self) -> u32 {
        u32::from_le_bytes(self.0)
    }

    /// The tag a word holds, as [`FourCc::word`] wrote it.
    ///
    /// ```
    /// use regent_core::data::fourcc::FourCc;
    ///
    /// assert_eq!(FourCc::from_word(0x4443_4241), FourCc(*b"ABCD"));
    /// ```
    pub const fn from_word(word: u32) -> Self {
        FourCc(word.to_le_bytes())
    }

    /// The tag with trailing spaces removed, for display and file naming.
    pub fn trimmed(&self) -> &str {
        std::str::from_utf8(&self.0)
            .unwrap_or("????")
            .trim_end_matches(' ')
    }
}

/// A FOURCC's length: string id packs a string of it.
const FOURCC_LEN: usize = 4;

/// What string fourcc pads a short code with.
const FOURCC_PAD: u8 = b' ';

/// What string fourcc skips.
const QUOTES: [u8; 2] = *b"'\"";

/// A string as the engine's id, string id: empty
/// 0; four bytes a FOURCC, quotes skipped, space padded, read
/// little-endian; else opening with a digit its
/// `atoi`; else CRC-32 from all ones, not inverted ([`super::crc`]). A
/// descriptor's XML `ID` is read so (string id cached, the same with a
/// cache): the SDK's `"MagicBall"` names its descriptor and its item,
/// a mod's `"MBPortal"` its building.
///
/// ```
/// use regent_core::data::fourcc::string_id;
///
/// assert_eq!(string_id("2000"), i32::from_le_bytes(*b"2000"));
/// assert_eq!(string_id("20000"), 20_000);
/// ```
///
/// **Inferred**: a narrow string; `atoi` wraps as `atoi` does.
#[must_use]
pub fn string_id(text: &str) -> i32 {
    let bytes = text.as_bytes();
    let digit = bytes.first().is_some_and(u8::is_ascii_digit);

    if bytes.is_empty() {
        0
    } else if bytes.len() == FOURCC_LEN {
        let mut code = [FOURCC_PAD; FOURCC_LEN];

        for (slot, b) in code
            .iter_mut()
            .zip(bytes.iter().filter(|b| !QUOTES.contains(b)))
        {
            *slot = *b;
        }
        i32::from_le_bytes(code)
    } else if digit {
        bytes
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .fold(0i32, |n, d| {
                n.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
            })
    } else {
        super::crc::crc(bytes).cast_signed()
    }
}

/// Exactly four bytes, no more. [`FourCc::from_prefix`] is the lenient form.
impl TryFrom<&[u8]> for FourCc {
    type Error = Error;

    fn try_from(b: &[u8]) -> Result<Self> {
        match <[u8; 4]>::try_from(b) {
            Ok(tag) => Ok(FourCc(tag)),
            Err(_) => Err(Error::BadValue {
                what: "four-character tag length",
                value: b.len() as u64,
            }),
        }
    }
}

impl fmt::Display for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Tags are ASCII in all shipped data; fall back instead of panicking.
        match std::str::from_utf8(&self.0) {
            Ok(s) => f.write_str(s),
            Err(_) => write!(f, "{:02x?}", self.0),
        }
    }
}

impl PartialEq<&[u8; 4]> for FourCc {
    fn eq(&self, other: &&[u8; 4]) -> bool {
        self.0 == **other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_is_taken_from_a_longer_header() {
        let head = b"CYLBPC  \x01\x00\x01\x00";

        assert_eq!(FourCc::from_prefix(head).unwrap(), FourCc::new(b"CYLB"));
    }

    #[test]
    fn a_short_slice_is_an_error_and_not_a_panic() {
        for len in 0..4 {
            let err = FourCc::from_prefix(&b"CYLB"[..len]).unwrap_err();

            assert!(
                matches!(err, Error::Truncated { need: 4, have, .. } if have == len),
                "{len}: {err:?}"
            );
        }
    }

    #[test]
    fn a_word_is_the_tag_read_little_endian() {
        let tile = FourCc::new(b"TILE");

        assert_eq!(tile.word(), 0x454C_4954);
        assert_eq!(FourCc::from_word(0x454C_4954), tile);
        assert_eq!(FourCc::from_word(7).word(), 7);
    }

    #[test]
    fn try_from_wants_exactly_four() {
        assert_eq!(
            FourCc::try_from(&b"TILE"[..]).unwrap(),
            FourCc::new(b"TILE")
        );

        for slice in [&b"TIL"[..], &b"TILES"[..], &b""[..]] {
            assert!(
                matches!(FourCc::try_from(slice).unwrap_err(), Error::BadValue { .. }),
                "{slice:?} should not be a tag"
            );
        }
    }
}
