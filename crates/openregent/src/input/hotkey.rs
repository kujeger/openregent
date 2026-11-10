//! The original's hot keys (`regent_core::view::menu::hotkey`): a key press is a
//! click on the drawn, enabled control whose `SMNU` key it is.
//!
//! ```text
//!  PreUpdate  press  the key as the engine codes it; claimed when a
//!                    control drawn last frame offered it, and taken from
//!                    the game's own keys (M, Escape, debug keys)
//!  egui pass  hit    each enabled control offers its key; the first
//!                    drawn whose key was claimed takes it as a click
//! ```
//!
//! The chat line empties the keys before `press` reads them, so typing
//! presses nothing.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use regent_core::view::menu::hotkey;

use crate::input::orders::{Mode, Orders};
use crate::panel::sheet::Able;

/// The keys between `press` and the controls, kept in egui's memory.
#[derive(Clone, Default)]
struct Keys {
    /// This frame's claimed key, until a control takes it.
    pressed: Option<i32>,
    /// The keys the controls drawn this frame offer.
    offered: Vec<i32>,
    /// Last frame's.
    last: Vec<i32>,
}

fn memory() -> egui::Id {
    egui::Id::new("hot keys")
}

/// The modifier keys and their bits.
const MODIFIERS: [(KeyCode, i32); 6] = [
    (KeyCode::ShiftLeft, hotkey::SHIFT),
    (KeyCode::ShiftRight, hotkey::SHIFT),
    (KeyCode::ControlLeft, hotkey::CTRL),
    (KeyCode::ControlRight, hotkey::CTRL),
    (KeyCode::AltLeft, hotkey::ALT),
    (KeyCode::AltRight, hotkey::ALT),
];

/// Backspace's character, `WM_CHAR` 8.
const BACKSPACE: i32 = 8;

/// The function keys in order, F1 first.
const FUNCTION: [Key; 15] = [
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
    Key::F13,
    Key::F14,
    Key::F15,
];

/// Claims this frame's key where a control offered it last frame.
/// Escape stays the game's while a mode is on: it cancels the mode.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn press(
    mut events: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    orders: Res<Orders>,
    mut contexts: EguiContexts,
) {
    let held = held(&keys);
    let presses: Vec<(KeyCode, i32)> = events
        .read()
        .filter(|e| e.state.is_pressed() && keys.just_pressed(e.key_code))
        .filter_map(|e| code(&e.logical_key).map(|c| (e.key_code, c | held)))
        .filter(|(_, c)| c & hotkey::CODE != hotkey::ESCAPE || orders.mode == Mode::Idle)
        .collect();

    if let Ok(ctx) = contexts.ctx_mut()
        && let Some(key) = claim(ctx, &presses)
    {
        keys.clear_just_pressed(key);
    }
}

/// The modifier bits of the keys held.
pub(crate) fn held(keys: &ButtonInput<KeyCode>) -> i32 {
    MODIFIERS
        .iter()
        .filter(|(k, _)| keys.pressed(*k))
        .fold(0, |bits, (_, bit)| bits | bit)
}

/// The first of `presses` a control offered last frame, kept for the
/// controls to take ([`hit`]); none while egui types (the chat line).
pub fn claim(ctx: &egui::Context, presses: &[(KeyCode, i32)]) -> Option<KeyCode> {
    let typing = ctx.egui_wants_keyboard_input();

    ctx.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<Keys>(memory());

        state.last = std::mem::take(&mut state.offered);

        let claimed = presses
            .iter()
            .filter(|_| !typing)
            .find(|(_, c)| state.last.iter().any(|k| hotkey::hits(*k, *c)))
            .copied();

        state.pressed = claimed.map(|(_, c)| c);
        claimed.map(|(key, _)| key)
    })
}

/// Whether control key `key`, drawn `able`, was pressed this frame: the
/// first such control takes the claimed key.
pub fn hit(ctx: &egui::Context, key: Option<i32>, able: Able) -> bool {
    match (key, able) {
        (Some(key), Able::Enabled) => ctx.data_mut(|d| {
            let state = d.get_temp_mut_or_default::<Keys>(memory());
            let hit = state.pressed.is_some_and(|p| hotkey::hits(key, p));

            state.offered.push(key);

            if hit {
                state.pressed = None;
            }

            hit
        }),
        _ => false,
    }
}

/// [`hit`] for a control drawn in `ui`: one under a disabled ui (a menu
/// over it, the game ended) neither offers nor takes its key.
pub fn hit_in(ui: &egui::Ui, key: Option<i32>, able: Able) -> bool {
    hit(
        ui.ctx(),
        key,
        Able::of(able == Able::Enabled && ui.is_enabled()),
    )
}

/// A key as the engine codes it: the character it
/// types, else Backspace, Tab, Enter, Escape, F1 to F15 and the cursor
/// keys.
pub(crate) fn code(key: &Key) -> Option<i32> {
    match key {
        Key::Character(s) => {
            let mut chars = s.chars();

            match (chars.next(), chars.next()) {
                (Some(c), None) => i32::try_from(u32::from(c))
                    .ok()
                    .filter(|&c| c <= hotkey::CODE),
                _ => None,
            }
        }
        Key::Space => Some(i32::from(b' ')),
        Key::Backspace => Some(BACKSPACE),
        Key::Tab => Some(hotkey::TAB),
        Key::Enter => Some(hotkey::ENTER),
        Key::Escape => Some(hotkey::ESCAPE),
        Key::PageUp => Some(hotkey::PAGE_UP),
        Key::PageDown => Some(hotkey::PAGE_DOWN),
        Key::ArrowRight => Some(hotkey::RIGHT),
        Key::ArrowLeft => Some(hotkey::LEFT),
        Key::ArrowUp => Some(hotkey::UP),
        Key::ArrowDown => Some(hotkey::DOWN),
        Key::Home => Some(hotkey::HOME),
        Key::End => Some(hotkey::END),
        Key::Insert => Some(hotkey::INSERT),
        Key::Delete => Some(hotkey::DELETE),
        other => FUNCTION
            .iter()
            .zip(hotkey::F1..)
            .find(|(f, _)| *f == other)
            .map(|(_, c)| c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: i32 = b'R' as i32;

    fn claim(ctx: &egui::Context, pressed: i32) {
        ctx.data_mut(|d| d.get_temp_mut_or_default::<Keys>(memory()).pressed = Some(pressed));
    }

    #[test]
    fn keys_code_as_their_characters_and_the_engine_specials() {
        assert_eq!(code(&Key::Character("r".into())), Some(i32::from(b'r')));
        assert_eq!(code(&Key::Character("?".into())), Some(i32::from(b'?')));
        assert_eq!(code(&Key::Escape), Some(hotkey::ESCAPE));
        assert_eq!(code(&Key::F1), Some(hotkey::F1));
        assert_eq!(code(&Key::F12), Some(hotkey::F1 + 11));
        assert_eq!(code(&Key::ArrowLeft), Some(hotkey::LEFT));
        assert_eq!(code(&Key::Shift), None);
    }

    #[test]
    fn the_first_enabled_control_takes_the_key_once() {
        let ctx = egui::Context::default();

        claim(&ctx, i32::from(b'r'));

        assert!(!hit(&ctx, Some(R), Able::Disabled));
        assert!(!hit(&ctx, Some(i32::from(b'H')), Able::Enabled));
        assert!(hit(&ctx, Some(R), Able::Enabled));
        assert!(!hit(&ctx, Some(R), Able::Enabled));
    }

    /// A control under a disabled ui leaves the key to the next.
    #[test]
    fn a_control_under_a_disabled_ui_takes_no_key() {
        let ctx = egui::Context::default();
        let mut hits = Vec::new();

        claim(&ctx, R);

        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.scope(|ui| {
                ui.disable();
                hits.push(hit_in(ui, Some(R), Able::Enabled));
            });
            hits.push(hit_in(ui, Some(R), Able::Enabled));
        });

        output.textures_delta.clear();
        assert_eq!(hits, [false, true]);
    }

    #[test]
    fn only_enabled_controls_offer_their_keys() {
        let ctx = egui::Context::default();

        hit(&ctx, Some(R), Able::Disabled);
        hit(&ctx, Some(i32::from(b'H')), Able::Enabled);

        let offered = ctx.data_mut(|d| d.get_temp::<Keys>(memory()).unwrap_or_default().offered);

        assert_eq!(offered, [i32::from(b'H')]);
    }
}
