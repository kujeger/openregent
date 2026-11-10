//! The map's own keys: a key no panel control takes ([`crate::view::menu::hotkey`])
//! goes to map keys, switched on its character or
//! special code alone. **Binary** unless marked.
//!
//! ```text
//!  Backspace  centre on the last speaker (sound::speaks), or its holder
//!  Enter      the chat line, the cursor idle (AP65)
//!  Space      an attack flag on or off (World::space)
//!  "          the selection's voice, EG01 at it
//!  < >        game speed (runtime::stepped), not in a network game
//!  ` ~        the selected message scroll read: ME01, then
//!             DestroyEventFlagMessage
//!  Delete     the mode left, the cursor idle
//!  arrows     the view scrolled by ScrollSpeed
//!  else       the hot key lists (HotKeyManager): spells 0 to 9,
//!             focus F2 to F8; the buildings' list holds no key
//! ```
//!
//! The lists are a game's: made at the first key, deleted as the game
//! ends, saved nowhere. A key's default value is a
//! placeholder (`HKF2`, `HK00`) no spell or unit matches.

use crate::data::fourcc::FourCc;
use crate::gpl::runtime::SpeedKey;
use crate::view::menu::hotkey;

/// `ScrollSpeed`'s file value: an arrow key's step. Each
/// press, auto-repeat included (key window's `WM_KEYDOWN`), sets
/// the view's pending scroll to that many pixels, which its next draw
/// takes whole ([`super::pan`]).
pub const SCROLL_SPEED: i32 = 32;

/// The spell keys' characters, the list's order.
pub const SPELL_KEYS: [u8; 10] = *b"0123456789";

/// The focus keys, F2 to F8.
pub const FOCUS_KEYS: [i32; 7] = [
    hotkey::F1 + 1,
    hotkey::F1 + 2,
    hotkey::F1 + 3,
    hotkey::F1 + 4,
    hotkey::F1 + 5,
    hotkey::F1 + 6,
    hotkey::F1 + 7,
];

/// Backspace's character.
const BACKSPACE: i32 = 8;

/// What a map key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Speaker,
    Chat,
    Flag,
    Voice,
    Speed(SpeedKey),
    Read,
    Cancel,
    /// Scroll by this many [`SCROLL_SPEED`]s across and down.
    Scroll(i32, i32),
    /// A spell key, by its index in [`SPELL_KEYS`].
    Spell(usize),
    /// A focus key, by its index in [`FOCUS_KEYS`].
    Focus(usize),
}

/// The map key of `code`, as the engine codes a key
/// ([`crate::view::menu::hotkey`]); modifiers ride along unread.
#[must_use]
pub fn of(code: i32) -> Option<Key> {
    let low = code & hotkey::CODE;
    let spell = SPELL_KEYS.iter().position(|k| i32::from(*k) == low);
    let focus = FOCUS_KEYS.iter().position(|k| *k == low);

    match low {
        BACKSPACE => Some(Key::Speaker),
        hotkey::ENTER => Some(Key::Chat),
        0x20 => Some(Key::Flag),
        0x22 => Some(Key::Voice),
        0x3c => Some(Key::Speed(SpeedKey::Slower)),
        0x3e => Some(Key::Speed(SpeedKey::Faster)),
        0x60 | 0x7e => Some(Key::Read),
        hotkey::DELETE => Some(Key::Cancel),
        hotkey::RIGHT => Some(Key::Scroll(1, 0)),
        hotkey::DOWN => Some(Key::Scroll(0, 1)),
        hotkey::LEFT => Some(Key::Scroll(-1, 0)),
        hotkey::UP => Some(Key::Scroll(0, -1)),
        _ => spell.map(Key::Spell).or(focus.map(Key::Focus)),
    }
}

/// The modifiers a list asks: one alone, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mods {
    Plain,
    Shift,
    Ctrl,
    Alt,
    /// Two or more held.
    Mixed,
}

impl Mods {
    /// Of `code`'s high bits.
    #[must_use]
    pub const fn of(code: i32) -> Mods {
        let shift = code & hotkey::SHIFT != 0;
        let ctrl = code & hotkey::CTRL != 0;
        let alt = code & hotkey::ALT != 0;

        match (shift, ctrl, alt) {
            (false, false, false) => Mods::Plain,
            (true, false, false) => Mods::Shift,
            (false, true, false) => Mods::Ctrl,
            (false, false, true) => Mods::Alt,
            _ => Mods::Mixed,
        }
    }
}

/// What a focus key bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// A piece, by agent.
    Unit(u32),
    /// A point of the map, 32nds: the view's centre, nothing selected.
    Point((i32, i32)),
}

/// What a focus key asks of the front end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jump {
    /// Select the piece if it stands or is no building, then the select
    /// mode (unit select maybe, map mode set `Sele`).
    Select(u32),
    /// As [`Jump::Select`], and the view centred on it (Shift,
    /// view center unit).
    Centre(u32),
    /// As [`Jump::Select`], and mini view 1 follows it (Alt,
    /// view follow).
    Follow(u32),
    /// Centre the view on the point.
    Look((i32, i32)),
}

/// The spell and focus lists' bindings (`SpellHotKeyList`,
/// `FastHotKeyList`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bindings {
    spells: [Option<FourCc>; SPELL_KEYS.len()],
    focus: [Option<Focus>; FOCUS_KEYS.len()],
}

impl Bindings {
    /// Spell key `index` pressed with `mods`, `aimed` the spell the cursor
    /// holds: Ctrl alone binds it; alone, the
    /// spell bound, to aim where its button is lit
    /// (spell key ready, the Abode's timer clear).
    pub fn spell(&mut self, index: usize, mods: Mods, aimed: Option<FourCc>) -> Option<FourCc> {
        match (mods, aimed) {
            (Mods::Ctrl, Some(spell)) => {
                if let Some(slot) = self.spells.get_mut(index) {
                    *slot = Some(spell);
                }

                None
            }
            (Mods::Plain, _) => self.spells.get(index).copied().flatten(),
            _ => None,
        }
    }

    /// Focus key `index` pressed with `mods`:
    /// Ctrl alone binds `selected`, else the view's `centre`; else what
    /// the bound piece or point asks.
    pub fn focus(
        &mut self,
        index: usize,
        mods: Mods,
        (selected, centre): (Option<u32>, (i32, i32)),
    ) -> Option<Jump> {
        if mods == Mods::Ctrl {
            if let Some(slot) = self.focus.get_mut(index) {
                *slot = Some(selected.map_or(Focus::Point(centre), Focus::Unit));
            }

            None
        } else {
            match (self.focus.get(index).copied().flatten(), mods) {
                (Some(Focus::Point(at)), _) => Some(Jump::Look(at)),
                (Some(Focus::Unit(id)), Mods::Plain) => Some(Jump::Select(id)),
                (Some(Focus::Unit(id)), Mods::Shift) => Some(Jump::Centre(id)),
                (Some(Focus::Unit(id)), Mods::Alt) => Some(Jump::Follow(id)),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SP10: FourCc = FourCc(*b"Sp10");

    #[test]
    fn the_switch_takes_the_character_alone() {
        assert_eq!(of(0x20 | hotkey::SHIFT), Some(Key::Flag));
        assert_eq!(of(i32::from(b'<')), Some(Key::Speed(SpeedKey::Slower)));
        assert_eq!(of(i32::from(b'~')), Some(Key::Read));
        assert_eq!(of(i32::from(b'3') | hotkey::CTRL), Some(Key::Spell(3)));
        assert_eq!(of(hotkey::F1 + 7), Some(Key::Focus(6)));
        assert_eq!(of(hotkey::F1), None);
        assert_eq!(of(i32::from(b'a')), None);
    }

    #[test]
    fn ctrl_binds_a_spell_key_and_the_key_alone_aims_it() {
        let mut keys = Bindings::default();

        assert_eq!(keys.spell(1, Mods::Plain, None), None);
        assert_eq!(keys.spell(1, Mods::Ctrl, Some(SP10)), None);
        assert_eq!(keys.spell(1, Mods::Plain, None), Some(SP10));
        assert_eq!(keys.spell(1, Mods::Shift, None), None);
        assert_eq!(keys.spell(1, Mods::Ctrl, None), None);
        assert_eq!(keys.spell(1, Mods::Plain, None), Some(SP10));
    }

    #[test]
    fn a_focus_key_binds_the_selection_or_the_view() {
        let mut keys = Bindings::default();

        assert_eq!(keys.focus(0, Mods::Plain, (None, (0, 0))), None);
        assert_eq!(keys.focus(0, Mods::Ctrl, (Some(7), (0, 0))), None);
        assert_eq!(
            keys.focus(0, Mods::Plain, (None, (0, 0))),
            Some(Jump::Select(7))
        );
        assert_eq!(
            keys.focus(0, Mods::Shift, (None, (0, 0))),
            Some(Jump::Centre(7))
        );
        assert_eq!(
            keys.focus(0, Mods::Alt, (None, (0, 0))),
            Some(Jump::Follow(7))
        );
        assert_eq!(keys.focus(0, Mods::Mixed, (None, (0, 0))), None);

        keys.focus(1, Mods::Ctrl, (None, (64, 96)));

        assert_eq!(
            keys.focus(1, Mods::Shift, (None, (0, 0))),
            Some(Jump::Look((64, 96)))
        );
    }
}
