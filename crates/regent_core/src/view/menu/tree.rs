//! The Build Tree Editor, `MX18`: the freestyle setup's BUILDING EDIT
//! MENU (freestyle dialog, control 5101). **Binary**:
//! `BuildTreeEditor_Controller`.
//!
//! ```text
//!      (open)   the build tree copied, its name
//!               SAVE AS box; the rows filled
//!  a row        [x] capped: the code has an entry; [+] [-] [n]
//!  the rows     box ticked when capped, greyed when a lower level is
//!               capped at 0; + - n hidden for an
//!               upgrade, the Sorcerer's Abode's ABq and ABf1, greyed
//!               uncapped; n the cap, empty uncapped
//!  box          capped: the entry erased; else capped at 0
//!  + / -        capped: the cap one up / one down, not under 0
//!  SET ALL      every row capped at 0; REMOVE ALL every entry erased
//!  SAVE         the tree named as typed, written; a shipped tree's name
//!               refused: the message GMTX 409, 410
//!  DELETE       the file of that name removed; none, the same message
//!  CANCEL       closed, nothing kept
//! ```
//!
//! A saved tree becomes the build tree and the config's pick.

use std::collections::BTreeMap;

use crate::data::build_tree::Tree;
use crate::data::catalogue::Catalogue;
use crate::data::desc::{self, BuildingFlag, User};
use crate::data::fourcc::FourCc;
use crate::sim::world::offer::by_title;

/// The panel, in `mx_textdata.cam`.
pub const EDITOR: FourCc = FourCc(*b"MX18");

/// SAVE, hot key S; DELETE, D; CANCEL, Escape.
pub const SAVE: i32 = 9000;
pub const CANCEL: i32 = 9001;
pub const DELETE: i32 = 9003;

/// SET ALL and REMOVE ALL.
pub const SET_ALL: i32 = 8006;
pub const REMOVE_ALL: i32 = 8007;

/// The name typed (`0x232e`), and its label "SAVE AS:".
pub const NAME: i32 = 9006;
pub const NAME_LABEL: i32 = 8005;

/// The sunken box the list and the rows' controls stand in.
pub const LIST_BOX: i32 = 1;

/// The list of names (`0x238c`), in rows, and its scroll bar
/// ([`super::ids::SCROLL_BAR`]).
pub const LIST: i32 = 9100;

/// A row's controls, the first row's ids: the box, +, -, the count. Row
/// `n` is each plus `n`.
pub const BOX: i32 = 2001;
pub const PLUS: i32 = 2101;
pub const MINUS: i32 = 2201;
pub const COUNT: i32 = 2301;

/// The rows the panel shows at once (tree editor rows stops at 20).
pub const ROWS: usize = 20;

/// The picture at the right, `IX22` 1003: the same whatever the row.
pub const PICTURE: i32 = 3887;

/// The title, "Build Tree Editor", and the quest pog left of it.
pub const TITLE: i32 = 8;
pub use super::ids::POG;

/// The list's scroll bar's rails, `IX22` 1001, down from its top and
/// up from its bottom.
pub const RAIL_TOP: i32 = 3;
pub const RAIL_BOTTOM: i32 = 5;

/// A network game's chat, over [`PICTURE`]: its hint "Press ENTER to
/// send chat.", heading "CHAT ROOM", scroll bar, entry, lines and SEND
/// CHAT.
pub const CHAT_HINT: i32 = 2;
pub const CHAT_HEADING: i32 = 4;
pub const CHAT_BAR: i32 = 10;
pub const CHAT_ENTRY: i32 = 5004;
pub const CHAT_BOX: i32 = 5005;
pub const SEND_CHAT: i32 = 5006;

/// The chat, hidden alone.
pub const CHAT: [i32; 6] = [
    CHAT_HINT,
    CHAT_HEADING,
    CHAT_BAR,
    CHAT_ENTRY,
    CHAT_BOX,
    SEND_CHAT,
];

/// The message a refused SAVE or DELETE shows, `AP11`: `GMTX` 409
/// "Illegal File Name", 410 "The filename is reserved, or the file does
/// not exist".
pub const REFUSED_TITLE: usize = 409;
pub const REFUSED_TEXT: usize = 410;

/// The code prefix whose rows hide the count: the Sorcerer's Abode's
/// (tree editor rows, a compare with `0x714241`).
const UNCOUNTED_PREFIX: [u8; 3] = *b"ABq";

/// The one code that hides it too.
const UNCOUNTED: FourCc = FourCc(*b"ABf1");

/// One building of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub code: FourCc,
    /// Its title, as listed.
    pub name: String,
    /// Whether + - and the count show.
    pub counted: bool,
}

/// The list and what greys a row.
#[derive(Clone, Debug, Default)]
pub struct Rows {
    pub rows: Vec<Row>,
    /// Each building's `UpgradeFrom`, `BuildingDesc`.
    from: BTreeMap<FourCc, FourCc>,
}

impl Rows {
    /// The buildings build tree list fill lists from `catalogue`'s
    /// descriptors, the last loaded of each code: a building the human
    /// player may use, not `NotBuildable` unless an upgrade; sorted by
    /// title as the build menu (tree editor name less compares as
    /// building name less).
    #[must_use]
    pub fn of(catalogue: &Catalogue<'_>) -> Rows {
        let units: BTreeMap<FourCc, desc::Unit<'_>> = catalogue
            .tagged(b"DUNT")
            .filter_map(|(_, record)| {
                let part = desc::Part::parse(record.data).ok()?;

                Some((record.id, desc::Unit::parse(&part).ok()?))
            })
            .collect();
        let from = units
            .iter()
            .filter_map(|(code, u)| Some((*code, u.building()?.upgrade_from()?)))
            .collect::<BTreeMap<_, _>>();
        let mut named: Vec<(String, Row)> = units
            .iter()
            .filter_map(|(code, u)| {
                let building = u.building()?;
                let upgrade = from.contains_key(code);
                let listed = u.can_use(User::HumanPlayer)
                    && (!building.has(BuildingFlag::NotBuildable) || upgrade);
                let counted = !(code.0[..3] == UNCOUNTED_PREFIX || *code == UNCOUNTED || upgrade);
                let name = u.head.title().into_owned();

                listed.then(|| {
                    let row = Row {
                        code: *code,
                        name: name.clone(),
                        counted,
                    };

                    (name, row)
                })
            })
            .collect();

        by_title(&mut named);

        Rows {
            rows: named.into_iter().map(|(_, r)| r).collect(),
            from,
        }
    }

    /// Build tree row able: whether `code`'s box may be pressed in `tree`:
    /// greyed where a lower level, by `UpgradeFrom`, is capped at 0.
    #[must_use]
    pub fn able(&self, tree: &Tree, code: FourCc) -> bool {
        match self.from.get(&code) {
            None => true,
            Some(lower) if tree.caps.get(lower) == Some(&0) => false,
            Some(lower) => self.able(tree, *lower),
        }
    }

    /// SET ALL: every row capped at 0.
    pub fn set_all(&self, tree: &mut Tree) {
        for row in &self.rows {
            tree.caps.insert(row.code, 0);
        }
    }

    /// REMOVE ALL: every row's entry erased.
    pub fn remove_all(&self, tree: &mut Tree) {
        for row in &self.rows {
            tree.caps.remove(&row.code);
        }
    }
}

/// A row's box pressed: a capped code uncapped, else
/// capped at 0.
pub fn toggle(tree: &mut Tree, code: FourCc) {
    if tree.caps.remove(&code).is_none() {
        tree.caps.insert(code, 0);
    }
}

/// Its +: a capped code's cap one up, `inc`, wrapping;
/// uncapped, nothing.
pub fn raise(tree: &mut Tree, code: FourCc) {
    if let Some(cap) = tree.caps.get_mut(&code) {
        *cap = cap.wrapping_add(1);
    }
}

/// Its -: a capped code's cap one down, `dec`, wrapping,
/// then not under 0.
pub fn lower(tree: &mut Tree, code: FourCc) {
    if let Some(cap) = tree.caps.get_mut(&code) {
        *cap = cap.wrapping_sub(1).max(0);
    }
}

/// Whether `name` is a shipped tree's: SAVE and DELETE refuse it
/// (build tree shipped index, an exact compare).
#[must_use]
pub fn reserved(shipped: &[Tree], name: &str) -> bool {
    shipped.iter().any(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVEL_1: FourCc = FourCc(*b"AAA1");
    const LEVEL_2: FourCc = FourCc(*b"AAA2");

    /// `inc` and `dec` wrap: + past the most is the least, and - below
    /// the least is the most, over 0.
    #[test]
    fn a_cap_steps_in_32_bits() {
        let mut tree = Tree::default();

        tree.caps.insert(LEVEL_1, i32::MAX);
        tree.caps.insert(LEVEL_2, i32::MIN);
        raise(&mut tree, LEVEL_1);
        lower(&mut tree, LEVEL_2);

        assert_eq!(tree.caps.get(&LEVEL_1), Some(&i32::MIN));
        assert_eq!(tree.caps.get(&LEVEL_2), Some(&i32::MAX));
    }

    #[test]
    fn the_box_caps_at_0_and_uncaps_and_the_count_stays_at_or_over_0() {
        let mut tree = Tree::default();

        raise(&mut tree, LEVEL_1);
        assert!(tree.caps.is_empty(), "+ on an uncapped row does nothing");

        toggle(&mut tree, LEVEL_1);
        assert_eq!(tree.caps.get(&LEVEL_1), Some(&0));

        lower(&mut tree, LEVEL_1);
        assert_eq!(tree.caps.get(&LEVEL_1), Some(&0));

        raise(&mut tree, LEVEL_1);
        raise(&mut tree, LEVEL_1);
        assert_eq!(tree.caps.get(&LEVEL_1), Some(&2));

        toggle(&mut tree, LEVEL_1);
        assert!(tree.caps.is_empty());
    }

    #[test]
    fn an_upgrade_greys_while_a_lower_level_is_capped_at_0() {
        let rows = Rows {
            from: [(LEVEL_2, LEVEL_1)].into_iter().collect(),
            ..Rows::default()
        };
        let mut tree = Tree::default();

        assert!(rows.able(&tree, LEVEL_2));

        tree.caps.insert(LEVEL_1, 3);
        assert!(rows.able(&tree, LEVEL_2), "a cap over 0 greys nothing");

        tree.caps.insert(LEVEL_1, 0);
        assert!(!rows.able(&tree, LEVEL_2));
        assert!(rows.able(&tree, LEVEL_1));
    }
}
