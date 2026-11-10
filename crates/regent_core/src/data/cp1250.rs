//! Windows-1250 decoding, for the text the game stores.
//!
//! `STRT` payloads are 8-bit Central European text. Only 0x80..=0xff differ
//! from ASCII; five are unassigned and become U+FFFD.
//!
//! **Inferred**: the codepage comes from third-party tooling. The corpora are
//! English, with high bytes only in glyph-table entries, so no localised text
//! checks it. See `docs/formats/payloads.md`.

/// U+0080 through U+00ff, in order. `\u{fffd}` marks an unassigned byte.
const HIGH: [char; 128] = [
    '\u{20ac}', '\u{fffd}', '\u{201a}', '\u{fffd}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{fffd}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{015a}', '\u{0164}', '\u{017d}', '\u{0179}',
    '\u{fffd}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{fffd}', '\u{2122}', '\u{0161}', '\u{203a}', '\u{015b}', '\u{0165}', '\u{017e}', '\u{017a}',
    '\u{00a0}', '\u{02c7}', '\u{02d8}', '\u{0141}', '\u{00a4}', '\u{0104}', '\u{00a6}', '\u{00a7}',
    '\u{00a8}', '\u{00a9}', '\u{015e}', '\u{00ab}', '\u{00ac}', '\u{00ad}', '\u{00ae}', '\u{017b}',
    '\u{00b0}', '\u{00b1}', '\u{02db}', '\u{0142}', '\u{00b4}', '\u{00b5}', '\u{00b6}', '\u{00b7}',
    '\u{00b8}', '\u{0105}', '\u{015f}', '\u{00bb}', '\u{013d}', '\u{02dd}', '\u{013e}', '\u{017c}',
    '\u{0154}', '\u{00c1}', '\u{00c2}', '\u{0102}', '\u{00c4}', '\u{0139}', '\u{0106}', '\u{00c7}',
    '\u{010c}', '\u{00c9}', '\u{0118}', '\u{00cb}', '\u{011a}', '\u{00cd}', '\u{00ce}', '\u{010e}',
    '\u{0110}', '\u{0143}', '\u{0147}', '\u{00d3}', '\u{00d4}', '\u{0150}', '\u{00d6}', '\u{00d7}',
    '\u{0158}', '\u{016e}', '\u{00da}', '\u{0170}', '\u{00dc}', '\u{00dd}', '\u{0162}', '\u{00df}',
    '\u{0155}', '\u{00e1}', '\u{00e2}', '\u{0103}', '\u{00e4}', '\u{013a}', '\u{0107}', '\u{00e7}',
    '\u{010d}', '\u{00e9}', '\u{0119}', '\u{00eb}', '\u{011b}', '\u{00ed}', '\u{00ee}', '\u{010f}',
    '\u{0111}', '\u{0144}', '\u{0148}', '\u{00f3}', '\u{00f4}', '\u{0151}', '\u{00f6}', '\u{00f7}',
    '\u{0159}', '\u{016f}', '\u{00fa}', '\u{0171}', '\u{00fc}', '\u{00fd}', '\u{0163}', '\u{02d9}',
];

/// Decode a Windows-1250 byte string. Borrows pure ASCII.
pub fn decode(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    if bytes.iter().all(u8::is_ascii) {
        // ASCII is valid UTF-8: borrows, substitutes nothing.
        String::from_utf8_lossy(bytes)
    } else {
        bytes
            .iter()
            .map(|&b| match b.checked_sub(0x80) {
                Some(high) => HIGH[usize::from(high)],
                None => char::from(b),
            })
            .collect::<String>()
            .into()
    }
}

/// What a byte stands in for where Windows-1250 has no byte for a
/// character.
const UNMAPPED: u8 = b'?';

/// Encode as Windows-1250, [`decode`]'s inverse; a character the codepage
/// lacks becomes `?`.
#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .map(|ch| match u8::try_from(u32::from(ch)) {
            Ok(b) if b < 0x80 => b,
            _ => HIGH
                .iter()
                .position(|h| *h == ch && ch != '\u{fffd}')
                .and_then(|i| u8::try_from(i + 0x80).ok())
                .unwrap_or(UNMAPPED),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_undoes_decode() {
        let bytes: Vec<u8> = (0x20..=0xff)
            .filter(|b| HIGH.get(usize::from(*b).wrapping_sub(0x80)) != Some(&'\u{fffd}'))
            .collect();

        assert_eq!(encode(&decode(&bytes)), bytes);
        assert_eq!(encode("\u{263a}"), b"?");
    }

    #[test]
    fn ascii_is_borrowed_unchanged() {
        let out = decode(b"Warriors Guild");

        assert_eq!(out, "Warriors Guild");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn high_bytes_map_to_central_european_letters() {
        // Bytes that differ from Latin-1.
        assert_eq!(decode(&[0x8c]), "\u{015a}"); // S with acute
        assert_eq!(decode(&[0xa5]), "\u{0104}"); // A with ogonek
        assert_eq!(decode(&[0xe8]), "\u{010d}"); // c with caron
    }

    #[test]
    fn unassigned_bytes_become_replacement_characters() {
        for byte in [0x81, 0x83, 0x88, 0x90, 0x98] {
            assert_eq!(decode(&[byte]), "\u{fffd}", "{byte:#04x}");
        }
    }

    #[test]
    fn every_byte_decodes_to_one_character() {
        let all: Vec<u8> = (0..=255).collect();

        assert_eq!(decode(&all).chars().count(), 256);
    }
}
