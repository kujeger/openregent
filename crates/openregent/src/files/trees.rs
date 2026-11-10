//! Building Limitations' trees: the five shipped, then the player's.
//!
//! The original keeps a player's tree as a `.BTD` file, a `BTDT` record
//! (`regent_core::data::build_tree`). This game keeps its own as TOML in
//! [`place`], one file a tree, and reads the original's `.BTD` files too,
//! from there and from the install's `buildtree`, writing none:
//!
//! ```text
//!  $XDG_DATA_HOME/openregent/buildtree/No Inns.toml
//!
//!  version = 1
//!  name = "No Inns"
//!
//!  [caps]          building code = how many of its kind may stand
//!  ABF1 = 0
//! ```
//!
//! A shipped tree's name is refused for a player's, as the original's
//! SAVE and DELETE refuse it; a player's file of that name is not
//! listed (the original would load the shipped tree for it). Two of the
//! player's of one name: this game's TOML over a `.BTD`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regent_core::data::build_tree::{self, Tree};
use regent_core::data::fourcc::FourCc;
use regent_core::view::menu::tree;
use serde::{Deserialize, Serialize};

use super::store;

/// The extension of this game's tree files.
const OWN_EXTENSION: &str = "toml";

/// The blue the shipped trees are listed in.
pub const SHIPPED_MARK: &str = "\u{1}61B6FD";

/// A tree file of ours.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    version: u32,
    name: String,
    caps: BTreeMap<String, i32>,
}

/// Where this game keeps the player's trees: `buildtree` in the XDG data
/// directory, beside the profiles.
#[must_use]
pub fn place() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(crate::files::env::OWN).join(build_tree::DIRECTORY))
}

/// The trees the list offers.
#[derive(Clone, Debug, Default)]
pub struct Trees {
    /// `BTDT`'s, `default` first.
    pub shipped: Vec<Tree>,
    /// The player's, by name.
    pub own: Vec<Tree>,
}

impl Trees {
    /// The shipped and the player's: this game's in `own`, the
    /// original's `.BTD` files in `own` and `imports`.
    #[must_use]
    pub fn read(shipped: Vec<Tree>, own: Option<&Path>, imports: &[PathBuf]) -> Trees {
        let mut by_name: BTreeMap<String, (Kind, Tree)> = BTreeMap::new();
        let dirs = own.into_iter().chain(imports.iter().map(PathBuf::as_path));

        for path in dirs.flat_map(store::files) {
            if let Some((kind, tree)) = read_one(&path) {
                let wins = by_name.get(&tree.name).is_none_or(|(k, _)| *k <= kind);

                if wins && !tree::reserved(&shipped, &tree.name) {
                    by_name.insert(tree.name.clone(), (kind, tree));
                }
            }
        }

        Trees {
            shipped,
            own: by_name.into_values().map(|(_, t)| t).collect(),
        }
    }

    /// Every tree in list order.
    pub fn all(&self) -> impl Iterator<Item = &Tree> {
        self.shipped.iter().chain(&self.own)
    }

    /// The list's names: the shipped in blue.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let shipped = self
            .shipped
            .iter()
            .map(|t| format!("{SHIPPED_MARK}{}", t.name));

        shipped
            .chain(self.own.iter().map(|t| t.name.clone()))
            .collect()
    }

    /// The tree named `name`.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&Tree> {
        self.all().find(|t| t.name == name)
    }
}

/// Which of the player's files: the original's, or ours, which wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Original,
    Own,
}

/// The tree at `path` and its kind; not a tree file, or one that will
/// not read, none.
fn read_one(path: &Path) -> Option<(Kind, Tree)> {
    let extension = path.extension()?.to_str()?;

    if extension.eq_ignore_ascii_case(build_tree::FILE_EXTENSION) {
        let bytes = std::fs::read(path).ok()?;

        Some((Kind::Original, Tree::parse(&bytes).ok()?))
    } else if extension == OWN_EXTENSION {
        let kept: Kept = store::read(path).ok()?;

        store::checked(kept.version, path).ok()?;

        let caps = kept
            .caps
            .iter()
            .map(|(code, cap)| Some((FourCc(<[u8; 4]>::try_from(code.as_bytes()).ok()?), *cap)))
            .collect::<Option<_>>()?;

        Some((
            Kind::Own,
            Tree {
                name: kept.name,
                caps,
            },
        ))
    } else {
        None
    }
}

/// Why SAVE or DELETE did nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// A shipped tree's name, or none of ours to delete: the original's
    /// message (`tree::REFUSED_TEXT`).
    Reserved,
    /// A name no file can be named after, or a file that will not write.
    Unwritten(String),
}

/// Our file in `dir` holding the tree named `name`.
fn held(dir: &Path, name: &str) -> Option<PathBuf> {
    store::with_extension(dir, OWN_EXTENSION)
        .into_iter()
        .find(|p| read_one(p).is_some_and(|(_, t)| t.name == name))
}

/// The file of ours named for `tree` in `dir`: one holding its name
/// already, else its name made safe (`profile::folder`), numbered past
/// any file already there.
///
/// ```text
///  "a/b", "a?b"  a_b.toml, a_b-2.toml
/// ```
fn own_file(dir: &Path, name: &str) -> Result<PathBuf, Refused> {
    match held(dir, name) {
        Some(path) => Ok(path),
        None => crate::files::profile::folder(name)
            .map(|stem| store::fresh(dir, &stem, OWN_EXTENSION))
            .map_err(|e| Refused::Unwritten(format!("{name}: {e:?}"))),
    }
}

/// `tree` kept in `dir`, unless a shipped tree's name.
///
/// # Errors
///
/// A shipped name, an unsafe one, or a file that will not write.
pub fn save(dir: &Path, shipped: &[Tree], tree: &Tree) -> Result<(), Refused> {
    if tree::reserved(shipped, &tree.name) {
        Err(Refused::Reserved)
    } else {
        store::make_dir(dir).map_err(Refused::Unwritten)?;

        let path = own_file(dir, &tree.name)?;
        let kept = Kept {
            version: store::VERSION,
            name: tree.name.clone(),
            caps: tree
                .caps
                .iter()
                .map(|(code, cap)| (code.to_string(), *cap))
                .collect(),
        };

        store::write(&path, &kept).map_err(Refused::Unwritten)
    }
}

/// Our file of the tree named `name` in `dir` removed; none, or a shipped
/// name, refused. An original's `.BTD` stays: this game
/// writes none.
///
/// # Errors
///
/// No file of ours holds it, or it will not go.
pub fn delete(dir: &Path, shipped: &[Tree], name: &str) -> Result<(), Refused> {
    match held(dir, name) {
        Some(path) if !tree::reserved(shipped, name) => std::fs::remove_file(&path)
            .map_err(|e| Refused::Unwritten(format!("{}: {e}", path.display()))),
        _ => Err(Refused::Reserved),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch folder of its own.
    fn scratch(name: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("trees-{name}"))
    }

    fn shipped() -> Vec<Tree> {
        vec![Tree {
            name: build_tree::DEFAULT.to_owned(),
            ..Tree::default()
        }]
    }

    fn mine(name: &str, cap: i32) -> Tree {
        Tree {
            name: name.to_owned(),
            caps: [(FourCc(*b"ABF1"), cap)].into_iter().collect(),
        }
    }

    #[test]
    fn a_saved_tree_lists_after_the_shipped_and_deletes() {
        let dir = scratch("round_trip");

        save(&dir, &shipped(), &mine("No Inns", 0)).unwrap();
        save(&dir, &shipped(), &mine("No Inns", 2)).unwrap();

        let trees = Trees::read(shipped(), Some(&dir), &[]);

        assert_eq!(trees.own, [mine("No Inns", 2)], "saved over by name");
        assert_eq!(
            trees.names(),
            [format!("{SHIPPED_MARK}default"), "No Inns".to_owned()]
        );

        delete(&dir, &shipped(), "No Inns").unwrap();

        assert_eq!(Trees::read(shipped(), Some(&dir), &[]).own.len(), 0);
        assert_eq!(delete(&dir, &shipped(), "No Inns"), Err(Refused::Reserved));
    }

    /// Names one file name stands for keep apart, each in its own file.
    #[test]
    fn names_made_alike_keep_apart() {
        let dir = scratch("alike");

        save(&dir, &shipped(), &mine("a/b", 1)).unwrap();
        save(&dir, &shipped(), &mine("a?b", 2)).unwrap();

        let trees = Trees::read(shipped(), Some(&dir), &[]);

        assert_eq!(trees.own, [mine("a/b", 1), mine("a?b", 2)]);
    }

    #[test]
    fn a_shipped_name_is_refused_and_ours_wins_over_an_originals() {
        let dir = scratch("names");
        let original = scratch("original");

        assert_eq!(
            save(&dir, &shipped(), &mine(build_tree::DEFAULT, 0)),
            Err(Refused::Reserved)
        );

        std::fs::write(original.join("userX.BTD"), mine("X", 5).encode()).unwrap();
        std::fs::write(
            original.join("userdefault.BTD"),
            mine("default", 5).encode(),
        )
        .unwrap();

        let trees = Trees::read(shipped(), Some(&dir), &[original.to_path_buf()]);

        assert_eq!(
            trees.own,
            [mine("X", 5)],
            "the original's read, a shipped name not"
        );

        save(&dir, &shipped(), &mine("X", 1)).unwrap();

        let trees = Trees::read(shipped(), Some(&dir), &[original.to_path_buf()]);

        assert_eq!(trees.own, [mine("X", 1)]);
    }
}
