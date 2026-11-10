//! A build tree: a name and building caps, as the build-tree object at
//! Build tree holds them. **Binary**.
//!
//! One record shape in three places, all read by gsgl tail read
//! and written by build tree encode:
//!
//! ```text
//!  u32    version: written 1, read and dropped
//!  char*  name, NUL-terminated (Windows-1250)
//!  u32    count
//!         that many: char[4] building code, i32 cap
//!
//!  BTDT records   mx_btdata.cam, the five shipped (build tree load record),
//!                 named by STRT BTDN's entry of the same key
//! BTD files     BuildTreePath (buildtree) in the user directory,
//!                 user<name>.BTD (build tree file path, build tree write)
//!  GSGL tail      a save's
//! ```
//!
//! The writer walks the cap map, a `std::map` on the code's word, so the
//! pairs come out in [`FourCc::word`] order. A cap is how many of a code's
//! three-letter prefix an owner may have; a code
//! with no entry has none.

use std::collections::BTreeMap;

use super::catalogue::Catalogue;
use super::cp1250;
use super::error::{Error, Result, cstr, slice, u32_at};
use super::fourcc::FourCc;
use super::text::Text;

/// The version build tree encode writes first.
pub const VERSION: u32 = 1;

/// The shipped trees' records, and the names their keys look up.
pub const RECORDS: [u8; 4] = *b"BTDT";
pub const NAMES: [u8; 4] = *b"BTDN";

/// The name a new game's tree has with nothing picked: `BTDN`'s `BTD1`,
/// which caps nothing.
pub const DEFAULT: &str = "default";

/// What a tree's file is called: `"user"`, its name, `"."`, `"BTD"`
/// (build tree file path, strings `"user"`, `"BTD"`).
pub const FILE_PREFIX: &str = "user";
pub const FILE_EXTENSION: &str = "BTD";

/// The directory `GOPT`'s `BuildTreePath` names in the expansion.
pub const DIRECTORY: &str = "buildtree";

/// The most name bytes the writer copies, its NUL included.
pub const NAME_ROOM: usize = 0x104;

/// One pair: a code and its cap.
const PAIR: usize = 8;

/// A build tree: its name and caps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    pub name: String,
    /// Cap by exact code.
    pub caps: BTreeMap<FourCc, i32>,
}

impl Tree {
    /// A tree from its record or file.
    ///
    /// # Errors
    ///
    /// An unterminated name, or pairs that do not end the bytes exactly: a
    /// half-read tree is wrong rules.
    pub fn parse(bytes: &[u8]) -> Result<Tree> {
        u32_at(bytes, "build tree version", 0)?;

        let rest = slice(bytes, "build tree name", 4, bytes.len().saturating_sub(4))?;
        let name = cstr(rest, "build tree name")?;
        let at = 4 + name.len() + 1;
        let count = u32_at(bytes, "build tree count", at)? as usize;
        let pairs = slice(bytes, "build tree pairs", at + 4, count * PAIR)?;
        let end = at + 4 + count * PAIR;

        if end != bytes.len() {
            return Err(Error::BrokenInvariant {
                what: "build tree pairs do not end it",
                expected: bytes.len() as u64,
                found: end as u64,
            });
        }

        let caps = pairs
            .as_chunks::<PAIR>()
            .0
            .iter()
            .map(|p| {
                let (code, cap) = p.split_at(4);

                Ok((
                    FourCc::from_prefix(code)?,
                    u32_at(cap, "build tree cap", 0)?.cast_signed(),
                ))
            })
            .collect::<Result<_>>()?;

        Ok(Tree {
            name: cp1250::decode(name).into_owned(),
            caps,
        })
    }

    /// The bytes build tree encode writes: [`VERSION`], the name cut to
    /// [`NAME_ROOM`], the pairs in word order.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut name = cp1250::encode(&self.name);

        name.truncate(NAME_ROOM - 1);

        let mut pairs: Vec<(FourCc, i32)> = self.caps.iter().map(|(c, n)| (*c, *n)).collect();

        pairs.sort_by_key(|(code, _)| code.word());

        let count = u32::try_from(pairs.len()).unwrap_or(u32::MAX);
        let mut out = Vec::with_capacity(name.len() + 9 + pairs.len() * PAIR);

        out.extend(VERSION.to_le_bytes());
        out.extend(name);
        out.push(0);
        out.extend(count.to_le_bytes());

        for (code, cap) in pairs {
            out.extend(code.0);
            out.extend(cap.to_le_bytes());
        }

        out
    }

    /// The shipped trees in record order, each named by `BTDN`'s entry of
    /// its key as build tree load record renames it: `BTD5`'s record says `Wiz
    /// War`, the list `Wizard War`. A record that will not read is left
    /// out.
    #[must_use]
    pub fn shipped(catalogue: &Catalogue<'_>) -> Vec<Tree> {
        let text = Text::build(catalogue);

        catalogue
            .tagged(&RECORDS)
            .filter_map(|(_, record)| {
                let tree = Tree::parse(record.data).ok()?;
                let name = text
                    .keyed(&NAMES, record.id)
                    .map_or(tree.name.clone(), std::borrow::Cow::into_owned);

                Some(Tree { name, ..tree })
            })
            .collect()
    }

    /// Its file's name: `user<name>.BTD`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{FILE_PREFIX}{}.{FILE_EXTENSION}", self.name)
    }

    /// The caps as text, `CODE=N` by commas in code order: a recorded
    /// game's setup holds them so ([`Tree::caps_of`]). Empty with none.
    #[must_use]
    pub fn caps_text(&self) -> String {
        self.caps
            .iter()
            .map(|(code, cap)| format!("{code}={cap}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The caps [`Tree::caps_text`] wrote.
    ///
    /// # Errors
    ///
    /// A pair not `CODE=N`, or a code not four bytes.
    pub fn caps_of(text: &str) -> std::result::Result<BTreeMap<FourCc, i32>, String> {
        text.split(',')
            .filter(|p| !p.is_empty())
            .map(|pair| {
                let (code, cap) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("{pair}: want CODE=N"))?;
                let code = <[u8; 4]>::try_from(code.as_bytes())
                    .map_err(|_| format!("{code}: not four characters"))?;
                let cap = cap.parse().map_err(|_| format!("{cap}: not a number"))?;

                Ok((FourCc(code), cap))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_encodes_its_pairs_in_word_order_and_reads_back() {
        let tree = Tree {
            name: "Mine".to_owned(),
            caps: [(FourCc(*b"ABl2"), 0), (FourCc(*b"ABm1"), 3)]
                .into_iter()
                .collect(),
        };
        let bytes = tree.encode();

        // `ABm1` before `ABl2`: the word's top byte is the fourth letter.
        assert_eq!(&bytes[13..17], b"ABm1");
        assert_eq!(Tree::parse(&bytes), Ok(tree));
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut bytes = Tree::default().encode();

        bytes.push(0);

        assert!(Tree::parse(&bytes).is_err());
    }

    #[test]
    fn the_caps_text_reads_back() {
        let tree = Tree {
            caps: [(FourCc(*b"ABB1"), 0), (FourCc(*b"ABW1"), 1)]
                .into_iter()
                .collect(),
            ..Tree::default()
        };

        assert_eq!(tree.caps_text(), "ABB1=0,ABW1=1");
        assert_eq!(Tree::caps_of(&tree.caps_text()), Ok(tree.caps));
        assert_eq!(Tree::caps_of(""), Ok(BTreeMap::new()));
        assert!(Tree::caps_of("ABB=1").is_err());
    }
}
