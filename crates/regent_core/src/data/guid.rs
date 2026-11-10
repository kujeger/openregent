//! A GUID, as mods, quests and saves name a quest or mod.

use std::fmt;

/// A mod's or quest's id, `{8-4-4-4-12}` hex. HD keys mods by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Guid(pub u128);

/// The hex digits of each group.
const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];

impl Guid {
    /// From `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, braces optional, any
    /// case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Guid> {
        let bare = text.trim();
        let bare = bare
            .strip_prefix('{')
            .and_then(|b| b.strip_suffix('}'))
            .unwrap_or(bare);
        let parts: Vec<&str> = bare.split('-').collect();
        let shaped = parts.len() == GROUPS.len()
            && parts
                .iter()
                .zip(GROUPS)
                .all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()));

        shaped
            .then(|| u128::from_str_radix(&parts.concat(), 16).ok())
            .flatten()
            .map(Guid)
    }

    /// Its 16 bytes as Windows lays them out, and HD reads and writes
    /// them (a save's `SVGD`): the first group a
    /// little-endian word, the next two little-endian halves, the last
    /// eight bytes as written.
    ///
    /// ```text
    ///  {31305551-0054-0032-0000-000000000000}
    ///   51 55 30 31  54 00  32 00  00 00 00 00 00 00 00 00
    /// ```
    #[must_use]
    pub fn bytes(self) -> [u8; 16] {
        let mut bytes = self.0.to_be_bytes();

        bytes[..4].reverse();
        bytes[4..6].reverse();
        bytes[6..8].reverse();
        bytes
    }

    /// From [`Guid::bytes`]'s layout.
    #[must_use]
    pub fn from_bytes(mut bytes: [u8; 16]) -> Guid {
        bytes[..4].reverse();
        bytes[4..6].reverse();
        bytes[6..8].reverse();
        Guid(u128::from_be_bytes(bytes))
    }
}

impl fmt::Display for Guid {
    /// As the manifests write it, upper case.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = format!("{:032X}", self.0);

        write!(
            f,
            "{{{}-{}-{}-{}-{}}}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guid_reads_with_or_without_braces_in_any_case() {
        let guid = Guid::parse("{d666fcbe-9906-4582-a6a0-dc96f812441e}").expect("a GUID");

        assert_eq!(guid.to_string(), "{D666FCBE-9906-4582-A6A0-DC96F812441E}");
        assert_eq!(
            Guid::parse("D666FCBE-9906-4582-A6A0-DC96F812441E"),
            Some(guid)
        );
        assert_eq!(Guid::parse("{D666FCBE-9906-4582-A6A0}"), None);
        assert_eq!(Guid::parse("{D666FCBE-9906-4582-A6A0-DC96F81244XX}"), None);
    }

    #[test]
    fn a_guids_bytes_are_windows_layout() {
        let guid = Guid::parse("{D666FCBE-9906-4582-A6A0-DC96F812441E}").expect("a GUID");
        let bytes = [
            0xbe, 0xfc, 0x66, 0xd6, 0x06, 0x99, 0x82, 0x45, 0xa6, 0xa0, 0xdc, 0x96, 0xf8, 0x12,
            0x44, 0x1e,
        ];

        assert_eq!(guid.bytes(), bytes);
        assert_eq!(Guid::from_bytes(bytes), guid);
    }
}
