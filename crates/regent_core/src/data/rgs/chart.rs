//! A generator chart: the named patterns a quest's regions are built from.
//!
//! `constants.rgs` is the base game's. A `.q` file names *patterns* here, never
//! terrain classes or trees directly.
//!
//! # Five sections, each a counted run of records
//!
//! ```text
//! [16-byte framing]  [preamble]
//!   [u32 n] record x n     terrain    -> GR## classes, weighted
//!   [u32 n] record x n     surface    -> terrain patterns
//!   [u32 n] record x n     object     -> DUNT ids: trees, rocks, ponds
//!   [u32 n] record x n     landscape  -> object patterns
//!   [u32 n] profile x n    fractal    -> named height profiles
//! ```
//!
//! Base: 27, 21, 73, 104 and 9; the expansion has more of each. Names from the
//! five RTTI classes `RGSTerrainChart`, `RGSSurfaceChart`, `RGSMobjectChart`,
//! `RGSLandscapeChart`, `RGSFractalChart`. Order from what each section
//! references; it agrees with the class list.
//!
//! # A record
//!
//! ```text
//! char[4] key         the map's key, read by the section loop
//! char[4] code        the same four characters again, the record's own field
//! cstr    name        `#Plains_Mostly_Parent`, `$Mayhew_Forest`, `Rocky`
//! u32     flag        zero in every shipped record
//! u32     flag        zero in every shipped record
//! u32     count
//!   ( char[4] id  u32 weight  u32 ?  u32 ?  u32 ?  u32 ? ) x count
//! ```
//!
//! **Binary** — pattern read reads the body into an
//! `RGSPattern`: the second code, the name, the description, and the two
//! words as booleans.
//! Pattern section read reads the first code,
//! keys the layer's map on it, and drops a record whose key or code is `NONE`.
//!
//! The entry list keeps count and order, which matters because the landscape
//! chart stores entry indices. The loop in pattern read reads the count, then
//! per entry calls chart entry read and
//! Distribution append at `_Mylast`: a `push_back`, no
//! filter, no sort. Chart entry read takes the six fields in file order
//! and stores the fifth (`rest[2]`) as a boolean byte, returned by
//! Entry byte.
//!
//! Records are found by code, not name prefix: `#` for most, `$` for the four
//! named after places, `%` for the expansion's, none for `Rocky` or `Dirt`.
//!
//! The first word after an id is a weight: `All_Parent` is one entry at 100,
//! `Mostly_Parent` 78 against 30. See [`Entry::rest`] for the other four.
//!
//! # The preamble
//!
//! **Binary** — chart file read: one dword, 4000, then five 16-byte
//! records, each two ranges read
//! by stream read pair: 0 to 100 and 0 to 20.
//! World range pair returns one; only record 2's second
//! bound is read, by surface blotch. `RGCB` prefixes a four-byte version
//! stamp (`"Rel@"` in the expansion). The 4000's use is unknown.
//!
//! An unset pattern carries the code `NONE`: pattern new, the
//! `RGSPattern` default constructor, writes it with the name
//! `none` and the description `default`. Mobject kind and the section
//! loader refuse it.

use crate::data::error::{Error, Result, cstr, slice, u32_at};
use crate::data::fourcc::FourCc;

/// Bytes before the first section count, after the 16-byte framing.
const PREAMBLE: usize = 84;

/// `RGCB` prefixes the preamble with four more bytes.
const RGCB_EXTRA: usize = 4;

/// Words after an entry's id: a weight and four more ([`Entry::rest`]).
const ENTRY_WORDS: usize = 5;

/// Words after a height profile's name.
const PROFILE_WORDS: usize = 3;

/// Sanity bound on entries per record (shipped maximum 8), not a format limit.
const MAX_ENTRIES: u32 = 64;

/// What a section of the chart holds.
///
/// **Inferred** from what each section references, corroborated by the five
/// RTTI `RGS*Chart` names. The name-to-section order is a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Weighted mixes of `DTPT` terrain classes.
    Terrain,
    /// Mixes of [`Layer::Terrain`] patterns.
    Surface,
    /// Weighted mixes of `DUNT` scatter objects: trees, rocks, ponds, ruins.
    Object,
    /// Compositions of [`Layer::Object`] patterns.
    Landscape,
}

impl Layer {
    /// In file order.
    pub const ALL: [Layer; 4] = [
        Layer::Terrain,
        Layer::Surface,
        Layer::Object,
        Layer::Landscape,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Layer::Terrain => "terrain",
            Layer::Surface => "surface",
            Layer::Object => "object",
            Layer::Landscape => "landscape",
        }
    }
}

/// One weighted reference out of a pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// What is referenced: a `DTPT` id, a `DUNT` id, or another pattern's code.
    pub id: FourCc,
    /// How much of the mix this takes.
    ///
    /// Values match names: `*_All_*` is one entry at 100, `*_Mostly_*` about 78
    /// against 30. Not always summing to 100, hence not "percent".
    pub weight: u32,
    /// The four words after the weight, two of them read.
    ///
    /// `rest[0]` is a count: on the landscape layer the clump size, 1 to 9; on
    /// the region layer a zone's seeds (read by distribution seeds);
    /// zero on the object layer. `rest[2]`
    /// picks the grid a placed object stamps on, a boolean byte.
    /// The other two are unread.
    pub rest: [u32; 4],
}

/// A named pattern: what it is called, and what it is made of.
#[derive(Debug, Clone)]
pub struct Pattern<'a> {
    /// The four characters a reference uses.
    ///
    /// Stored twice: the engine's map key, then the record's field.
    /// Mobject kind draws nothing for `NONE`;
    /// the loader drops such records and none ship (test
    /// `no_shipped_chart_pattern_carries_the_code_the_gate_refuses`).
    pub code: FourCc,
    /// The long name, without its terminator.
    ///
    /// Prefixed `#` for most, `$` for four place names, `%` in the expansion,
    /// none for a handful.
    pub name: &'a [u8],
    pub entries: Vec<Entry>,
}

/// A named height profile: `Flat`, `Rolling Hills`, `Mountainous`.
///
/// The generator's fractal settings: three 16.16 fixed-point words instead of
/// an entry list, read by [`Profile::hurst`] and [`Profile::falloff`]; the
/// third is the amplitude ([maps.md](../../../../../docs/formats/maps.md)). A land reaches one only through its zones.
#[derive(Debug, Clone)]
pub struct Profile<'a> {
    pub code: FourCc,
    pub name: &'a [u8],
    pub words: [u32; PROFILE_WORDS],
}

impl Profile<'_> {
    /// The words' 16.16 scale, as for heights.
    const FIXED: f64 = crate::base::fixed::ONE_F64;

    /// How smooth the ground is. A higher exponent is flatter ground.
    ///
    /// **Binary**: `RGSFractalSettings` prints `+4` and
    /// `+8` as `Fractal Setting  id:%s  hurst exponent: %d  scale : %d`.
    ///
    /// **Corpus**: six expansion profiles differ only in this word and are
    /// named for it: `%flat` 0.90, `%slightly_hilly` 0.55, `%hilly` 0.20,
    /// `%rough_hilly` 0.15.
    #[must_use]
    pub fn hurst(&self) -> f64 {
        f64::from(self.words[0]) / Self::FIXED
    }

    /// How much smaller each step of the fractal is than the one before.
    ///
    /// **Inferred**: within 0.4% of `2^(-hurst/2)` on all fourteen profiles,
    /// the per-half-step falloff of diamond-square midpoint displacement. So a
    /// precomputed ratio of [`Profile::hurst`]. Test
    /// `a_profiles_falloff_follows_from_its_hurst`.
    #[must_use]
    pub fn falloff(&self) -> f64 {
        f64::from(self.words[1]) / Self::FIXED
    }
}

/// A parsed `.rgs` chart.
#[derive(Debug, Clone)]
pub struct Chart<'a> {
    /// The four pattern sections, in file order. See [`Layer::ALL`].
    pub layers: Vec<Vec<Pattern<'a>>>,
    pub profiles: Vec<Profile<'a>>,
}

impl<'a> Chart<'a> {
    /// Read a chart body — everything after the 16-byte framing.
    ///
    /// # Errors
    ///
    /// If a section count or record runs past the end, or a record's code is
    /// not repeated.
    pub fn parse(tag: FourCc, body: &'a [u8]) -> Result<Self> {
        // `RGCB` carries four bytes the base version does not.
        let extra = usize::from(tag.as_bytes()[3] != b'A') * RGCB_EXTRA;
        let preamble = slice(body, "chart preamble", 0, extra + PREAMBLE)?;

        let mut at = preamble.len();
        let mut layers = Vec::with_capacity(Layer::ALL.len());

        for _ in Layer::ALL {
            let (found, next) = patterns(body, at)?;

            layers.push(found);
            at = next;
        }

        let (profiles, end) = height_profiles(body, at)?;

        if end == body.len() {
            Ok(Chart { layers, profiles })
        } else {
            Err(Error::BrokenInvariant {
                what: "chart sections fill the file",
                expected: body.len() as u64,
                found: end as u64,
            })
        }
    }

    /// One layer's patterns.
    #[must_use]
    pub fn layer(&self, layer: Layer) -> &[Pattern<'a>] {
        let at = Layer::ALL.iter().position(|l| *l == layer);

        at.and_then(|i| self.layers.get(i))
            .map_or(&[], Vec::as_slice)
    }

    /// Every pattern of every layer, with the layer it came from.
    pub fn patterns(&self) -> impl Iterator<Item = (Layer, &Pattern<'a>)> + '_ {
        Layer::ALL
            .into_iter()
            .flat_map(|l| self.layer(l).iter().map(move |p| (l, p)))
    }

    /// The pattern a code names in one layer.
    ///
    /// For following references, which know their layer. A search of every
    /// layer would return a surface pattern again, as its one entry usually
    /// repeats its own code.
    #[must_use]
    pub fn find_in(&self, layer: Layer, code: FourCc) -> Option<&Pattern<'a>> {
        self.layer(layer).iter().find(|p| p.code == code)
    }
}

/// Read a counted run of patterns, and say where it ends.
fn patterns(body: &[u8], at: usize) -> Result<(Vec<Pattern<'_>>, usize)> {
    let count = u32_at(body, "chart section count", at)?;
    let mut cur = at + 4;
    let mut out = Vec::new();

    for _ in 0..count {
        let (pattern, next) = pattern(body, cur)?;

        out.push(pattern);
        cur = next;
    }

    Ok((out, cur))
}

/// Read one pattern record.
fn pattern(body: &[u8], at: usize) -> Result<(Pattern<'_>, usize)> {
    let (code, name, mut cur) = named(body, at)?;

    // Two undecoded words, then the entry count.
    let count = u32_at(body, "pattern entry count", cur + 8)?;
    cur += 12;

    if count > MAX_ENTRIES {
        return Err(Error::BadValue {
            what: "pattern entry count",
            value: u64::from(count),
        });
    }

    let mut entries = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let id = FourCc::from_prefix(slice(body, "pattern entry", cur, 4)?)?;
        let mut words = [0u32; ENTRY_WORDS];

        for (i, word) in words.iter_mut().enumerate() {
            *word = u32_at(body, "pattern entry word", cur + 4 + i * 4)?;
        }

        entries.push(Entry {
            id,
            weight: words[0],
            rest: [words[1], words[2], words[3], words[4]],
        });

        cur += 4 + ENTRY_WORDS * 4;
    }

    Ok((
        Pattern {
            code,
            name,
            entries,
        },
        cur,
    ))
}

/// Read the counted run of height profiles.
fn height_profiles(body: &[u8], at: usize) -> Result<(Vec<Profile<'_>>, usize)> {
    let count = u32_at(body, "profile count", at)?;
    let mut cur = at + 4;
    let mut out = Vec::new();

    for _ in 0..count {
        let (code, name, next) = named(body, cur)?;
        let mut words = [0u32; PROFILE_WORDS];

        for (i, word) in words.iter_mut().enumerate() {
            *word = u32_at(body, "profile word", next + i * 4)?;
        }

        out.push(Profile { code, name, words });
        cur = next + PROFILE_WORDS * 4;
    }

    Ok((out, cur))
}

/// Read the `code, code, name` header both record kinds share.
///
/// Returns the code, the name, and where the name's terminator left off.
fn named(body: &[u8], at: usize) -> Result<(FourCc, &[u8], usize)> {
    let head = slice(body, "record code", at, 8)?;
    let code = FourCc::from_prefix(head)?;
    let repeat = FourCc::from_prefix(&head[4..])?;

    if code != repeat {
        return Err(Error::BadMagic {
            what: "chart record repeated code",
            found: repeat.as_bytes().to_vec(),
        });
    }

    let name = cstr(&body[at + 8..], "chart record name")?;

    Ok((code, name, at + 8 + name.len() + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pattern record, built by hand: code twice, name, two words, then a
    /// count and its entries.
    fn record(code: [u8; 4], name: &str, entries: &[([u8; 4], u32)]) -> Vec<u8> {
        let mut out = Vec::new();

        out.extend_from_slice(&code);
        out.extend_from_slice(&code);
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());

        for (id, weight) in entries {
            out.extend_from_slice(id);
            out.extend_from_slice(&weight.to_le_bytes());
            out.extend_from_slice(&[0; 16]);
        }

        out
    }

    #[test]
    fn a_pattern_reads_its_code_name_and_weighted_entries() {
        let raw = record(
            *b"#Pla",
            "#Plains_Mostly_Parent",
            &[(*b"GR01", 78), (*b"GR02", 30)],
        );
        let (p, end) = pattern(&raw, 0).expect("a pattern");

        assert_eq!(p.code, FourCc::new(b"#Pla"));
        assert_eq!(p.name, b"#Plains_Mostly_Parent");
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].id, FourCc::new(b"GR01"));
        assert_eq!(p.entries[0].weight, 78);
        assert_eq!(p.entries[1].weight, 30);
        assert_eq!(end, raw.len(), "the record accounts for its bytes");
    }

    #[test]
    fn a_name_without_a_prefix_reads_the_same() {
        // `Rocky` and `Dirt` carry no prefix.
        let raw = record(*b"Rock", "Rocky", &[(*b"GR03", 100)]);
        let (p, _) = pattern(&raw, 0).expect("a pattern");

        assert_eq!(p.name, b"Rocky");
    }

    #[test]
    fn a_code_that_is_not_repeated_is_refused() {
        let mut raw = record(*b"#Pla", "#Plains", &[]);
        raw[4] = b'X';

        assert!(pattern(&raw, 0).is_err(), "the repeat is the invariant");
    }

    #[test]
    fn an_absurd_entry_count_is_refused_rather_than_read() {
        let mut raw = record(*b"#Pla", "#Plains", &[]);
        let at = raw.len() - 4;
        raw[at..].copy_from_slice(&9999u32.to_le_bytes());

        assert!(pattern(&raw, 0).is_err());
    }

    #[test]
    fn a_profile_has_three_words_and_no_entries() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.extend_from_slice(b"FS01FS01Flat\0");
        for w in [26202u32, 57250, 131_072] {
            raw.extend_from_slice(&w.to_le_bytes());
        }

        let (found, end) = height_profiles(&raw, 0).expect("a profile");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, b"Flat");
        assert_eq!(found[0].words[0], 26202);
        assert_eq!(end, raw.len());
    }
}
