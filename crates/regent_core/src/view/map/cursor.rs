//! The mouse cursor: a picture of `CUR1`, picked by mode and state.
//! **Binary** unless marked.
//!
//! ```text
//!  view or dialog under the mouse ──> its mode code (view)
//!  mode ──> MODES row: set 1000 + n, slot 0 on entering
//!  each update: the row's state callback ──> the slot
//!     Sele  2 over a piece it may select, else 0
//!     Drag  2 short of the building's price, else 0
//!     Spnn  2 short of the price, 0 a valid aim, 1 otherwise
//!     Fl00  2 short of the reward, 0 a target, 1 otherwise
//!     Fl01  1 short of the reward, else 0
//!     Mini, Dial, Move  0
//!  picture: the slot's streams, each frame 0 at its nudge; the hotspot
//!  is the tile anchor (CYCursor draws the image at the mouse)
//! ```
//!
//! `GE_Cursor` passes the state as the slot:
//! method leaves it on the stack for method, which adds [`FIRST_SET`]
//! to the set. No `CUR1` slot has more than one frame, so nothing
//! animates. A mode with [`Price::Shown`] writes a nonzero amount as
//! text at [`PRICE_AT`] in font `SM09`.
//!
//! The select and spell modes' callbacks also name the unit the ring
//! marks (cursor target set, `view::map::ring::HOVER`); no other
//! mode's does.

use crate::data::desc::UnitKind;
use crate::data::fourcc::FourCc;

use super::ring::Stand;

/// The cursor's `IMAG` (`GE_Cursor`).
pub const IMAGE: FourCc = FourCc(*b"CUR1");

/// What `GE_Cursor` adds to a mode's set.
pub const FIRST_SET: u32 = 1000;

/// The busy cursor, set and slot, while a game loads or saves.
pub const BUSY: (u32, usize) = (1001, 0);

/// Where a mode's amount is written, pixels from the hotspot, right and
/// down.
pub const PRICE_AT: (i32, i32) = (0, 20);

/// Whether a mode writes its amount beside the cursor: the row's byte 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Price {
    Shown,
    Hidden,
}

/// One row of the mode table cursor modes builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub code: FourCc,
    /// The set, less [`FIRST_SET`].
    pub set: u32,
    /// The row's word 2; unread.
    pub word: u32,
    pub price: Price,
}

const fn row(code: [u8; 4], set: u32, word: u32, price: Price) -> Row {
    Row {
        code: FourCc(code),
        set,
        word,
        price,
    }
}

/// The mode table, in cursor modes's order. Sp15 has no row: its
/// button casts at once.
pub const MODES: [Row; 34] = [
    row(*b"Sele", 0, 2, Price::Hidden),
    row(*b"Mini", 0, 1, Price::Hidden),
    row(*b"Dial", 2, 2, Price::Hidden),
    row(*b"Drag", 7, 2, Price::Shown),
    row(*b"Move", 7, 2, Price::Shown),
    row(*b"Sp10", 10, 3, Price::Shown),
    row(*b"Sp11", 11, 3, Price::Shown),
    row(*b"Sp12", 12, 3, Price::Shown),
    row(*b"Sp13", 13, 3, Price::Shown),
    row(*b"Sp14", 14, 3, Price::Shown),
    row(*b"Sp16", 16, 3, Price::Shown),
    row(*b"Sp17", 17, 3, Price::Shown),
    row(*b"Sp18", 18, 3, Price::Shown),
    row(*b"Sp19", 19, 3, Price::Shown),
    row(*b"Sp20", 20, 3, Price::Shown),
    row(*b"Sp21", 21, 3, Price::Shown),
    row(*b"Sp22", 22, 3, Price::Shown),
    row(*b"Sp23", 23, 3, Price::Shown),
    row(*b"Sp24", 24, 3, Price::Shown),
    row(*b"Sp25", 25, 3, Price::Shown),
    row(*b"Sp26", 26, 3, Price::Shown),
    row(*b"Sp27", 27, 3, Price::Shown),
    row(*b"Sp28", 28, 3, Price::Shown),
    row(*b"Sp29", 29, 3, Price::Shown),
    row(*b"Sp30", 30, 3, Price::Shown),
    row(*b"Sp31", 31, 3, Price::Shown),
    row(*b"Sp32", 36, 3, Price::Shown),
    row(*b"Sp33", 34, 3, Price::Shown),
    row(*b"Sp34", 32, 3, Price::Shown),
    row(*b"Sp37", 35, 3, Price::Shown),
    row(*b"Sp36", 37, 3, Price::Shown),
    row(*b"Sp35", 33, 3, Price::Shown),
    row(*b"Fl00", 5, 2, Price::Shown),
    row(*b"Fl01", 6, 1, Price::Shown),
];

/// What the select mode's cursor is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Over {
    Ground,
    /// A piece it may select: the pointing hand.
    Piece,
}

/// Whether the treasury holds the mode's amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purse {
    Holds,
    Short,
}

impl Purse {
    #[must_use]
    pub const fn of(short: bool) -> Purse {
        if short { Purse::Short } else { Purse::Holds }
    }
}

/// Whether the cursor's spot takes the order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aim {
    Valid,
    Invalid,
}

/// A piece under the cursor, as pick kind ok sorts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Building(Stand),
    Character,
    /// A reward flag, unit kind 4 (or a message flag, `MSGF`).
    Flag,
    /// Scatter, effects, missiles: never picked.
    Other,
}

impl Kind {
    /// The kind of a piece of descriptor kind `kind`; `flag` for a
    /// reward flag.
    #[must_use]
    pub const fn of(kind: Option<UnitKind>, stand: Stand, flag: Flagged) -> Kind {
        match (kind, flag) {
            (_, Flagged::Yes) => Kind::Flag,
            (Some(UnitKind::Building), _) => Kind::Building(stand),
            (Some(UnitKind::Character), _) => Kind::Character,
            _ => Kind::Other,
        }
    }
}

/// Whether a piece is a reward flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flagged {
    Yes,
    No,
}

/// How the pick met a piece: on its picture's pixels (`IsoGameView`
/// bit `0x20`, unit hit), or only in its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Picture,
    Box,
}

/// What the select mode makes of a seen piece under the cursor: a piece
/// pick kind ok passes, met on its picture or a character.
#[must_use]
pub fn over(kind: Kind, hit: Hit) -> Over {
    let picks = matches!(
        kind,
        Kind::Building(Stand::Standing) | Kind::Character | Kind::Flag
    );
    let met = hit == Hit::Picture || kind == Kind::Character;

    if picks && met {
        Over::Piece
    } else {
        Over::Ground
    }
}

/// The cursor: a mode and its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    /// `Sele`, the map's own mode.
    Select(Over),
    /// `Dial`: a dialog's mode, over a panel's control.
    Dialog,
    /// `Mini`: a mini view's.
    Mini,
    /// `Drag`: a building being placed.
    Drag(Purse),
    /// `Move`: a rangers' guild's camp being moved.
    Move,
    /// `Spnn`: a spell being aimed.
    Spell(FourCc, Purse, Aim),
    /// `Fl00`: an attack flag being placed.
    Attack(Purse, Aim),
    /// `Fl01`: an explore flag being placed.
    Explore(Purse),
}

impl Cursor {
    /// The mode code.
    #[must_use]
    pub const fn code(self) -> FourCc {
        match self {
            Cursor::Select(_) => FourCc(*b"Sele"),
            Cursor::Dialog => FourCc(*b"Dial"),
            Cursor::Mini => FourCc(*b"Mini"),
            Cursor::Drag(_) => FourCc(*b"Drag"),
            Cursor::Move => FourCc(*b"Move"),
            Cursor::Spell(code, ..) => code,
            Cursor::Attack(..) => FourCc(*b"Fl00"),
            Cursor::Explore(_) => FourCc(*b"Fl01"),
        }
    }

    /// The mode's row; `None` for a spell with none.
    #[must_use]
    pub fn row(self) -> Option<Row> {
        MODES.iter().find(|r| r.code == self.code()).copied()
    }

    /// The `CUR1` set shown.
    #[must_use]
    pub fn set(self) -> Option<u32> {
        self.row().map(|r| FIRST_SET + r.set)
    }

    /// The slot: the mode's state callback's answer.
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Cursor::Select(Over::Piece)
            | Cursor::Drag(Purse::Short)
            | Cursor::Spell(_, Purse::Short, _)
            | Cursor::Attack(Purse::Short, _) => 2,
            Cursor::Spell(_, _, Aim::Invalid)
            | Cursor::Attack(_, Aim::Invalid)
            | Cursor::Explore(Purse::Short) => 1,
            _ => 0,
        }
    }

    /// Whether the mode writes its amount beside the cursor.
    #[must_use]
    pub fn price(self) -> Price {
        self.row().map_or(Price::Hidden, |r| r.price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_shows_its_set_and_its_state_picks_the_slot() {
        let spell = |purse, aim| Cursor::Spell(FourCc(*b"Sp32"), purse, aim);

        assert_eq!(Cursor::Select(Over::Ground).set(), Some(1000));
        assert_eq!(Cursor::Select(Over::Piece).slot(), 2);
        assert_eq!(Cursor::Dialog.set(), Some(1002));
        assert_eq!(Cursor::Drag(Purse::Short).set(), Some(1007));
        assert_eq!(Cursor::Drag(Purse::Short).slot(), 2);
        assert_eq!(spell(Purse::Holds, Aim::Valid).set(), Some(1036));
        assert_eq!(spell(Purse::Holds, Aim::Valid).slot(), 0);
        assert_eq!(spell(Purse::Holds, Aim::Invalid).slot(), 1);
        assert_eq!(spell(Purse::Short, Aim::Valid).slot(), 2);
        assert_eq!(Cursor::Attack(Purse::Holds, Aim::Invalid).slot(), 1);
        assert_eq!(Cursor::Explore(Purse::Short).slot(), 1);
        assert_eq!(
            Cursor::Spell(FourCc(*b"Sp15"), Purse::Holds, Aim::Valid).set(),
            None
        );
    }

    #[test]
    fn the_pointing_hand_wants_a_piece_it_may_pick_met_on_its_picture() {
        let standing = Kind::Building(Stand::Standing);

        assert_eq!(over(Kind::Character, Hit::Box), Over::Piece);
        assert_eq!(over(standing, Hit::Box), Over::Ground);
        assert_eq!(over(standing, Hit::Picture), Over::Piece);
        assert_eq!(
            over(Kind::Building(Stand::Fallen), Hit::Picture),
            Over::Ground
        );
        assert_eq!(over(Kind::Flag, Hit::Picture), Over::Piece);
        assert_eq!(over(Kind::Other, Hit::Picture), Over::Ground);
    }
}
