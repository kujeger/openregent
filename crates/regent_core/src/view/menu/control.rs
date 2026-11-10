//! A control's look by state: its tick, hover, press and flags pick
//! the slot of its picture set.

use crate::content::imagery::{self, FrameSource};
use crate::data::catalogue::Catalogue;
use crate::data::error::Result;
use crate::data::fourcc::FourCc;
use crate::data::imag::{self, Image, Playback};
use crate::data::smnu::{self, Kind};
use crate::view::sprite::Sprite;

/// Whether a check or radio box is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    Checked,
    Clear,
}

impl Tick {
    /// The tick a control of `kind` shows: only a check or radio box
    /// draws its checked slots.
    #[must_use]
    pub const fn shown_by(self, kind: Option<Kind>) -> Tick {
        match kind {
            Some(Kind::Check | Kind::Radio) => self,
            _ => Tick::Clear,
        }
    }
}

/// Whether a control is the dialog manager's hovered one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hover {
    Over,
    Away,
}

impl Hover {
    /// Over while the pointer is on the control, held down or not; a
    /// press lights the state instead ([`State::pressed`]).
    #[must_use]
    pub const fn of(hovered: bool) -> Hover {
        if hovered { Hover::Over } else { Hover::Away }
    }
}

/// Whether the pointer holds a control down while over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Held,
    Free,
}

impl Press {
    /// Held while `held`: a button down that went down on the control,
    /// the pointer still over it.
    #[must_use]
    pub const fn of(held: bool) -> Press {
        if held { Press::Held } else { Press::Free }
    }
}

/// Whether a control can be pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Able {
    Enabled,
    Disabled,
}

impl Able {
    #[must_use]
    pub const fn of(on: bool) -> Able {
        if on { Able::Enabled } else { Able::Disabled }
    }

    /// The engine's state of a control with SMNU `flags`: disabled here,
    /// else as its flags say ([`State::of`]; the palace's current tab is
    /// lit by its panel).
    #[must_use]
    pub const fn state(self, flags: i32) -> State {
        match self {
            Able::Enabled => State::of(flags),
            Able::Disabled => State::Disabled,
        }
    }
}

/// A control's state: added to its picture slot, and its text colours'
/// index.
///
/// **Binary**: `CYDialogItem`: 2 with flag
/// `smnu::DISABLED`, else 1 with `smnu::LIT`, else 0.
/// `CYDialogItem` (`CYDialogItem`, message 10) disables through
/// method with 2; a press with 1 ([`State::pressed`]).
/// Hovering sets none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Normal,
    Lit,
    Disabled,
}

impl State {
    /// The state flags `flags` give.
    #[must_use]
    pub const fn of(flags: i32) -> State {
        if flags & smnu::DISABLED != 0 {
            State::Disabled
        } else if flags & smnu::LIT != 0 {
            State::Lit
        } else {
            State::Normal
        }
    }

    /// The state while `press`: held lights an enabled control, so a
    /// hovered one draws slot 7, past a seven slot set 1.
    ///
    /// **Binary**: `CYDialog` keeps the control a
    /// button went down on and each pass calls its method,
    /// which sends message 9 with 1 while the pointer is
    /// on it, else 0; letting go sends 9 with 0. Message 9 is
    /// `CYDialogItem`: unless disabled, `method(1)`
    /// held, `method(0)` let go, a panel's lit flag cleared too. Every
    /// kind alike. **Game**: `ui_button_click_hold.png`, slot 1.
    #[must_use]
    pub const fn pressed(self, press: Press) -> State {
        match (self, press) {
            (State::Normal, Press::Held) => State::Lit,
            _ => self,
        }
    }

    /// The state's number: slots on, and the colours' index.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            State::Normal => 0,
            State::Lit => 1,
            State::Disabled => 2,
        }
    }
}

/// A checked box's slots start here.
pub const CHECKED_SLOT: usize = 3;

/// The hovered control's slots start this far on.
pub const HOVER_SLOT: usize = 6;

/// The slot a control in `state` draws, `count` slots in its set: every
/// kind alike, a check or radio box adding its tick ([`Tick::shown_by`]).
///
/// ```text
///  slot = 3 if checked + 6 if hovered + state (0 rest, 1 lit, 2 disabled)
///  DEM2 1006   0 grey   1 white   2-5 dark   6 light blue   7 white
///  INTN 1001   0 rest   1 inverse 2 grey     6 bright       8 grey
///  INDk 1003   0 clear  3 checked 6 clear, hovered   9 checked, hovered
/// ```
///
/// **Binary**: `CYDialogItem` answers message `0x59`
/// with 3 for a checked box, 6 more while hovered, plus the state;
/// widget slot clamp brings a slot past the count
/// back: less 6 above 5, then less 3, else 0. Buttons, checks and
/// pictures draw it.
/// **Game**: `ui_full.png`'s hit point check, checked at rest, draws
/// `INDz` 1016 slot 3, the gold soldier; the names check, clear, slot 0,
/// grey.
#[must_use]
pub fn drawn_slot((tick, hover): (Tick, Hover), state: State, count: usize) -> usize {
    let checked = match tick {
        Tick::Checked => CHECKED_SLOT,
        Tick::Clear => 0,
    };
    let hovered = match hover {
        Hover::Over => HOVER_SLOT,
        Hover::Away => 0,
    };
    let slot = checked + hovered + state.index();

    if slot < count {
        slot
    } else {
        let less = if slot > 5 { slot - HOVER_SLOT } else { slot };

        if less < count {
            less
        } else {
            less.checked_sub(CHECKED_SLOT)
                .filter(|s| *s < count)
                .unwrap_or(0)
        }
    }
}

/// The one picture of an interface set: frame 0 of its first slot.
///
/// `None` when the record, the set or a tile is missing.
pub fn still(catalogue: &Catalogue<'_>, record: FourCc, set: i32) -> Option<Result<Sprite>> {
    state(catalogue, record, set, 0)
}

/// Frame 0 of slot `slot` of an interface set: a button's look in one
/// state.
///
/// The slot is [`drawn_slot`]'s; the text colours ([`ShownControl::colours`](super::ShownControl::colours))
/// index by the [`State`]. `DEM2` 1006: slot 0 grey, the captures'
/// buttons at rest; 6 light blue, hovered or lit by its room object
/// ([`room::LIT`](super::room::LIT)).
pub fn state(
    catalogue: &Catalogue<'_>,
    record: FourCc,
    set: i32,
    slot: usize,
) -> Option<Result<Sprite>> {
    Sprite::in_set(catalogue, record, u32::try_from(set).ok()?, (slot, 0))
}

/// One slot of an interface set: its frames, and how they play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strip {
    pub frames: Vec<FrameSource>,
    /// The slot's playback, else the set's.
    pub playback: Option<Playback>,
}

/// Slot `slot` of `record`'s interface set `set`; `None` without one.
///
/// # Errors
///
/// A record that does not parse.
pub fn strip(
    catalogue: &Catalogue<'_>,
    record: FourCc,
    set: i32,
    slot: usize,
) -> Option<Result<Strip>> {
    let at = catalogue.find(b"IMAG", record)?;
    let data = catalogue.record(b"IMAG", at)?.data;
    let set = u32::try_from(set).ok()?;
    let read = || -> Result<Option<Strip>> {
        let image = Image::parse(data)?;
        let playback = image.sets.iter().find(|s| s.id == set).and_then(|s| {
            s.slots
                .get(slot)
                .and_then(imag::Slot::playback)
                .or_else(|| s.playback())
        });
        let sets = imagery::sets(catalogue, at)?;
        let frames = sets
            .iter()
            .find(|a| a.id == set)
            .and_then(|a| a.poses.get(slot))
            .map(|pose| pose.frames.clone());

        Ok(frames.map(|frames| Strip { frames, playback }))
    };

    read().transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The minimap's checks: twelve slots (`INDz` 1016), checked at rest
    /// 3, hovered 6 or 9; a seven slot set's slot 7 is 1, as
    /// widget slot clamp's note says.
    #[test]
    fn a_checked_box_draws_three_slots_on_and_a_hovered_one_six() {
        let (normal, lit) = (State::Normal, State::Lit);

        assert_eq!(drawn_slot((Tick::Clear, Hover::Away), normal, 12), 0);
        assert_eq!(drawn_slot((Tick::Checked, Hover::Away), normal, 12), 3);
        assert_eq!(drawn_slot((Tick::Clear, Hover::Over), normal, 12), 6);
        assert_eq!(drawn_slot((Tick::Checked, Hover::Over), normal, 12), 9);
        assert_eq!(drawn_slot((Tick::Clear, Hover::Over), lit, 7), 1);
        assert_eq!(drawn_slot((Tick::Checked, Hover::Away), normal, 3), 0);
        assert_eq!(drawn_slot((Tick::Checked, Hover::Away), lit, 4), 1);
    }

    /// Each kind's slot by state: a button (`DEM2` 1006, eight slots),
    /// a picture (`INTN` 1001, twelve), a check (`INBb` 1002, six) and
    /// a radio (`INDk` 1003, twelve). Hovered is 6 on, never the lit
    /// state 1, the white look the bug drew.
    #[test]
    fn a_hovered_control_draws_six_on_whatever_its_kind() {
        use crate::data::smnu::{self, Kind};

        let slot = |kind, tick: Tick, hover, state, count| {
            drawn_slot((tick.shown_by(Some(kind)), hover), state, count)
        };
        let (on, off) = (Tick::Checked, Tick::Clear);
        let (over, away) = (Hover::Over, Hover::Away);
        let (normal, disabled) = (State::Normal, State::Disabled);

        assert_eq!(slot(Kind::Button, off, away, normal, 8), 0);
        assert_eq!(slot(Kind::Button, off, over, normal, 8), 6);
        assert_eq!(slot(Kind::Button, off, away, disabled, 8), 2);
        assert_eq!(slot(Kind::Button, off, over, disabled, 8), 2);
        assert_eq!(slot(Kind::Button, on, over, normal, 8), 6);
        assert_eq!(slot(Kind::Item, off, over, normal, 12), 6);
        assert_eq!(slot(Kind::Item, off, over, disabled, 12), 8);

        assert_eq!(slot(Kind::Check, on, away, normal, 6), 3);
        assert_eq!(slot(Kind::Check, off, over, normal, 6), 0);
        assert_eq!(slot(Kind::Check, on, over, normal, 6), 3);
        assert_eq!(slot(Kind::Check, on, away, disabled, 6), 5);
        assert_eq!(slot(Kind::Radio, off, over, normal, 12), 6);
        assert_eq!(slot(Kind::Radio, on, over, normal, 12), 9);

        assert_eq!(State::of(0), normal);
        assert_eq!(State::of(smnu::LIT), State::Lit);
        assert_eq!(State::of(smnu::DISABLED | smnu::LIT), disabled);
        assert_eq!(Hover::of(true), over);
    }

    /// A held control is lit, 1 on its hover slot:
    /// the research button (`INBb` 1006, seven slots) draws 1, a twelve
    /// slot set 7; a disabled one takes no press; let go, it rests.
    #[test]
    fn a_held_control_draws_its_lit_slot() {
        let held = |state: State, tick, count| {
            drawn_slot((tick, Hover::Over), state.pressed(Press::Held), count)
        };

        assert_eq!(held(State::Normal, Tick::Clear, 7), 1);
        assert_eq!(held(State::Normal, Tick::Clear, 12), 7);
        assert_eq!(held(State::Normal, Tick::Checked, 12), 10);
        assert_eq!(held(State::Lit, Tick::Clear, 12), 7);
        assert_eq!(held(State::Disabled, Tick::Clear, 12), 8);
        assert_eq!(State::Normal.pressed(Press::Free), State::Normal);
        assert_eq!(Press::of(true), Press::Held);
    }
}
