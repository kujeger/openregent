//! The generic chunk tree every descriptor is built from.

use super::cstr;
use crate::data::error::{Error, Result, slice, u32_at};
use crate::data::fourcc::FourCc;

/// Chunk header: tag, length, version, child count.
pub const HEADER_LEN: usize = 16;

/// How deep parts may nest: a guard against a malformed file, not the
/// engine's (**Inferred**: far past any shipped descriptor).
const NEST_MOST: usize = 256;

/// One node of a descriptor's tree. The engine's RTTI calls it
/// `GameDescSubPart`.
#[derive(Debug, Clone)]
pub struct Part<'a> {
    pub tag: FourCc,
    pub version: u32,
    /// The header's length. Can undercount [`Part::body`]; see the module note.
    pub declared: usize,
    /// Everything between this part's header and the next sibling.
    pub body: &'a [u8],
    /// Empty for a leaf.
    pub parts: Vec<Part<'a>>,
}

impl<'a> Part<'a> {
    /// Read a descriptor payload as a tree.
    ///
    /// # Errors
    ///
    /// If a header is truncated or its tag unprintable, a child runs past its
    /// parent, or a sibling cannot be located.
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        let (part, end) = Self::at(payload, 0, payload.len(), 0)?;

        if end != payload.len() {
            return Err(Error::BrokenInvariant {
                what: "descriptor payload length",
                expected: payload.len() as u64,
                found: end as u64,
            });
        }

        Ok(part)
    }

    /// Read the part at `at`, bounded by `end`, `depth` parts in. Returns
    /// it and where it stops.
    fn at(data: &'a [u8], at: usize, end: usize, depth: usize) -> Result<(Self, usize)> {
        if depth > NEST_MOST {
            return Err(Error::BadValue {
                what: "descriptor part nesting",
                value: depth as u64,
            });
        }

        let head = slice(data, "descriptor part header", at, HEADER_LEN)?;
        let tag = FourCc::from_prefix(head)?;

        if !head[..4].iter().all(u8::is_ascii_graphic) {
            return Err(Error::BadMagic {
                what: "descriptor part tag",
                found: head[..4].to_vec(),
            });
        }

        let declared = u32_at(head, "descriptor part length", 4)? as usize;
        let version = u32_at(head, "descriptor part version", 8)?;
        let children = u32_at(head, "descriptor part child count", 12)? as usize;

        let body_at = at + HEADER_LEN;
        let declared_end =
            body_at
                .checked_add(declared)
                .filter(|e| *e <= end)
                .ok_or(Error::OutOfBounds {
                    what: "descriptor part body",
                    offset: body_at,
                    len: declared,
                    size: end,
                })?;

        // A leaf stops at its declared length; the caller may stretch it.
        if children == 0 {
            let body = slice(data, "descriptor part body", body_at, declared)?;

            return Ok((
                Part {
                    tag,
                    version,
                    declared,
                    body,
                    parts: Vec::new(),
                },
                declared_end,
            ));
        }

        let parts = Self::children(data, body_at, declared_end, children, depth + 1)?;
        let body = slice(data, "descriptor part body", body_at, declared)?;

        Ok((
            Part {
                tag,
                version,
                declared,
                body,
                parts,
            },
            declared_end,
        ))
    }

    /// Split `[at, end)` into exactly `count` parts, `depth` parts in.
    fn children(
        data: &'a [u8],
        at: usize,
        end: usize,
        count: usize,
        depth: usize,
    ) -> Result<Vec<Self>> {
        let mut parts = Vec::new();
        let mut cur = at;

        for i in 0..count {
            let remaining = count - i - 1;

            // The last child owns everything left, whatever its length claims.
            let stop = if remaining == 0 {
                end
            } else {
                Self::next_sibling(data, cur, end, remaining)?
            };

            let (mut part, declared_end) = Self::at(data, cur, stop, depth)?;

            // Stretch a leaf whose length undercut its real extent.
            if part.parts.is_empty() && declared_end < stop {
                part.body = slice(
                    data,
                    "descriptor part body",
                    cur + HEADER_LEN,
                    stop - cur - HEADER_LEN,
                )?;
            }

            parts.push(part);
            cur = stop;
        }

        if cur == end {
            Ok(parts)
        } else {
            Err(Error::BrokenInvariant {
                what: "descriptor children fill their parent",
                expected: end as u64,
                found: cur as u64,
            })
        }
    }

    /// Where the part starting at `cur` stops, given `remaining` siblings after
    /// it must fill `[.., end)`.
    ///
    /// Usually the declared length lands on the next header. Otherwise exactly
    /// one sibling must remain, ending at `end`, which pins its position.
    fn next_sibling(data: &[u8], cur: usize, end: usize, remaining: usize) -> Result<usize> {
        let declared = u32_at(data, "descriptor part length", cur + 4)? as usize;
        let after = cur + HEADER_LEN + declared;

        // `fits` rejects a trailing string read as a header: `trai` of
        // `trailing` claims a length far past the parent's end.
        if after < end && Self::fits(data, after, end) {
            return Ok(after);
        }

        if remaining > 1 {
            return Err(Error::BrokenInvariant {
                what: "descriptor sibling position",
                expected: end as u64,
                found: after as u64,
            });
        }

        // One sibling left: it must end exactly at the parent's end. Pins
        // `SECD` on all 140 three-child `DACT` records.
        let mut found = None;

        for probe in after..=end.saturating_sub(HEADER_LEN) {
            if !Self::fits(data, probe, end) {
                continue;
            }

            let len = u32_at(data, "descriptor part length", probe + 4)? as usize;

            if probe + HEADER_LEN + len != end {
                continue;
            }

            if found.is_some() {
                return Err(Error::BrokenInvariant {
                    what: "descriptor sibling position is ambiguous",
                    expected: end as u64,
                    found: probe as u64,
                });
            }

            found = Some(probe);
        }

        found.ok_or(Error::BrokenInvariant {
            what: "descriptor sibling not found",
            expected: end as u64,
            found: after as u64,
        })
    }

    /// Whether a readable header sits at `at` whose body stays inside `end`.
    fn fits(data: &[u8], at: usize, end: usize) -> bool {
        if !Self::looks_like_header(data, at, end) {
            return false;
        }

        u32_at(data, "descriptor part length", at + 4)
            .is_ok_and(|len| at + HEADER_LEN + len as usize <= end)
    }

    fn looks_like_header(data: &[u8], at: usize, end: usize) -> bool {
        at + HEADER_LEN <= end
            && data
                .get(at..at + 4)
                .is_some_and(|t| t.iter().all(u8::is_ascii_graphic))
    }

    /// The first descendant with this tag, breadth before depth.
    pub fn find(&self, tag: &[u8; 4]) -> Option<&Part<'a>> {
        if self.tag == tag {
            return Some(self);
        }

        self.parts.iter().find_map(|p| p.find(tag))
    }

    /// Every part in the tree, this one first.
    pub fn walk(&self) -> Box<dyn Iterator<Item = &Part<'a>> + '_> {
        Box::new(std::iter::once(self).chain(self.parts.iter().flat_map(Part::walk)))
    }
}

/// The identity block every descriptor carries.
///
/// ```text
/// char[4]  id, matching the record's own id in the archive directory
/// u32      unknown; 0 to 5, most often 1
/// cstr     name
/// cstr     description, often the same as the name
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head<'a> {
    pub id: FourCc,
    pub word: u32,
    pub name: &'a [u8],
    pub description: &'a [u8],
}

impl<'a> Head<'a> {
    /// The player-facing name, e.g. `MRb1`: `hands_of_steel_icon` displays as
    /// *"Hands of Steel"*. All 732 SDK XML descriptors have both names.
    ///
    /// Spell names live here, not in a string table. `GMTX` repeats them
    /// keyless; HD's id-keyed `UNTN`/`ACTN` agree everywhere.
    ///
    /// Falls back to the internal name if empty; never in shipped data.
    #[must_use]
    pub fn title(&self) -> std::borrow::Cow<'a, str> {
        let pick = if self.description.is_empty() {
            self.name
        } else {
            self.description
        };

        String::from_utf8_lossy(pick)
    }

    /// # Errors
    ///
    /// If the block is under eight bytes or either string is unterminated.
    pub fn parse(body: &'a [u8]) -> Result<Self> {
        let id = FourCc::from_prefix(body)?;
        let word = u32_at(body, "descriptor head word", 4)?;
        let rest = slice(body, "descriptor head strings", 8, body.len() - 8)?;

        let name = cstr(rest, "descriptor name")?;
        let description = cstr(&rest[name.len() + 1..], "descriptor description")?;

        Ok(Head {
            id,
            word,
            name,
            description,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[tag][len][version][children]` followed by `body`.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "fixtures are a few dozen bytes"
    )]
    fn part(tag: [u8; 4], version: u32, children: u32, body: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&children.to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    /// Parts nested too deep are refused, not read until the stack runs
    /// out.
    #[test]
    fn parts_nested_too_deep_are_refused() {
        const DEPTH: usize = 100_000;

        let bytes: Vec<u8> = (0..DEPTH)
            .flat_map(|i| {
                let inside = u32::try_from((DEPTH - 1 - i) * HEADER_LEN).unwrap();
                let children = u32::from(i + 1 < DEPTH);

                [
                    *b"NEST",
                    inside.to_le_bytes(),
                    [0; 4],
                    children.to_le_bytes(),
                ]
            })
            .flatten()
            .collect();

        assert!(matches!(Part::parse(&bytes), Err(Error::BadValue { .. })));
    }

    fn head_body(id: [u8; 4], name: &str, description: &str) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        out.extend_from_slice(description.as_bytes());
        out.push(0);
        out
    }

    #[test]
    fn a_tree_recurses_to_its_leaves() {
        let inner = [
            part(
                *b"HEAD",
                0,
                0,
                &head_body(*b"AVB1", "Barbarian", "A barbarian"),
            ),
            part(*b"PRIM", 1, 0, &[7; 12]),
        ]
        .concat();
        let bytes = part(*b"DUNT", 0, 1, &part(*b"DATA", 0, 2, &inner));

        let root = Part::parse(&bytes).unwrap();

        assert_eq!(root.tag, b"DUNT");
        assert_eq!(root.parts.len(), 1);
        assert_eq!(root.parts[0].tag, b"DATA");
        assert_eq!(
            root.parts[0]
                .parts
                .iter()
                .map(|p| p.tag.to_string())
                .collect::<Vec<_>>(),
            ["HEAD", "PRIM"]
        );
        assert_eq!(root.find(b"PRIM").unwrap().body, [7; 12]);
        assert_eq!(root.walk().count(), 4);
    }

    #[test]
    fn a_leaf_whose_bytes_look_like_a_tag_stays_a_leaf() {
        // "SECD" inside a leaf's payload is not a sibling; the child count rules.
        let bytes = part(
            *b"DUNT",
            0,
            1,
            &part(*b"PRIM", 0, 0, b"SECD\x00\x00\x00\x00junk"),
        );

        let root = Part::parse(&bytes).unwrap();

        assert_eq!(root.parts.len(), 1);
        assert!(root.parts[0].parts.is_empty());
        assert!(root.find(b"SECD").is_none());
    }

    #[test]
    fn a_short_declared_length_is_stretched_to_the_next_sibling() {
        // As in DACT: a string trails PRIM's declared length. The last child
        // ending at the parent's end locates SECD.
        let mut inner = part(*b"PRIM", 1, 0, &[1, 2, 3, 4]);
        inner.extend_from_slice(b"trailing\0");
        inner.extend_from_slice(&part(*b"SECD", 3, 0, &[9; 8]));

        let bytes = part(*b"DUNT", 0, 1, &part(*b"DATA", 0, 2, &inner));
        let data = Part::parse(&bytes).unwrap();
        let kids = &data.parts[0].parts;

        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].tag, b"PRIM");
        assert_eq!(kids[0].declared, 4, "the header still says four");
        assert_eq!(
            kids[0].body, b"\x01\x02\x03\x04trailing\0",
            "but the body runs to the next sibling"
        );
        assert_eq!(kids[1].tag, b"SECD");
        assert_eq!(kids[1].body, [9; 8]);
    }

    #[test]
    fn an_ambiguous_sibling_position_is_refused() {
        // Two candidates ending at the parent's end: error, not a pick.
        let mut inner = part(*b"PRIM", 1, 0, &[0]);
        inner.extend_from_slice(&[0, 0, 0, 0]);
        // AAAA holds BBBB's header as its body, so both end at the parent's end.
        inner.extend_from_slice(&part(*b"AAAA", 0, 0, &part(*b"BBBB", 0, 0, &[])));
        let bytes = part(*b"DUNT", 0, 1, &part(*b"DATA", 0, 2, &inner));

        assert!(matches!(
            Part::parse(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }

    #[test]
    fn a_child_overrunning_its_parent_is_refused() {
        let mut inner = part(*b"PRIM", 0, 0, &[1, 2, 3, 4]);
        inner[4..8].copy_from_slice(&999u32.to_le_bytes());
        let bytes = part(*b"DUNT", 0, 1, &inner);

        assert!(Part::parse(&bytes).is_err());
    }

    #[test]
    fn a_tag_that_is_not_printable_is_refused() {
        let mut bytes = part(*b"DUNT", 0, 0, &[1, 2, 3]);
        bytes[1] = 0x01;

        assert!(matches!(
            Part::parse(&bytes).unwrap_err(),
            Error::BadMagic { .. }
        ));
    }

    #[test]
    fn head_carries_the_id_and_both_names() {
        let body = head_body(*b"ABV1", "Warriors_Guild", "Warriors Guild");
        let head = Head::parse(&body).unwrap();

        assert_eq!(head.id, FourCc::new(b"ABV1"));
        assert_eq!(head.word, 1);
        assert_eq!(head.name, b"Warriors_Guild");
        assert_eq!(head.description, b"Warriors Guild");
    }

    #[test]
    fn an_unterminated_name_is_refused() {
        let mut body = b"ABV1".to_vec();
        body.extend_from_slice(&1u32.to_le_bytes());
        body.extend_from_slice(b"no terminator");

        assert!(matches!(
            Head::parse(&body).unwrap_err(),
            Error::Truncated { .. }
        ));
    }
}
