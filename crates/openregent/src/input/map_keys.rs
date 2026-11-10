//! The map's own keys (`regent_core::view::map::map_keys`): what a key no panel
//! control claimed (`hotkey`) and no text box took does.
//!
//! ```text
//!  Backspace  the view on the last speaker (audio::Speaker)
//!  "          the selection's voice (audio::Voices)
//!  < >        game speed along runtime::PACES (`speed::key`); not in a
//!             LAN game
//!  ` ~        the selected message scroll read, as its CLOSE
//!  Delete     the mode left
//!  arrows     the view scrolled, each press and repeat; held, with
//!             `[enhancements] smooth_arrows`, by `pan`
//!  0-9        a bound spell aimed; Ctrl binds the aimed one
//!  F2-F8      a bound piece selected (Shift: and centred, Alt: and
//!             followed by the tracking window) or point looked at; Ctrl
//!             binds the selection, else the view
//!  Alt        held, every hit point bar shown (`hp_bars::Reveal`)
//! ```
//!
//! Enter is the chat's (`chat::keys`), Alt+Enter the window's
//! (`fullscreen`); Space the pause's (`ui::pause_key`,
//! a departure: the original's attack flag, `World::space`).
//! Digits 1 to 3 stay the speed keys with `[enhancements] speed_keys`, 0
//! the zoom reset with smooth zoom.

use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use regent_core::sim::game::Input;
use regent_core::view::map::map_keys::{self, Bindings, Jump, Key, Mods};
use regent_draw::map::Maps;
use regent_draw::stage::View;

use crate::audio::{Interface, Speaker, Voices};
use crate::files::config::Settings;
use crate::game::seen::Watch;
use crate::game::speed::Speedup;
use crate::input::orders::{Mode, Orders};
use crate::map::mirror::Pieces;
use crate::map::select::{Reading, Selected};
use crate::panel::Panels;
use crate::{input::hotkey, map::zoom};

/// The spell and focus keys' bindings, a game's: the original's lists
/// go with the game.
#[derive(Resource, Default)]
pub struct Bound(Bindings);

/// The digits the enhancements keep: the speed keys' and smooth zoom's.
const SPEED_DIGITS: [u8; 3] = *b"123";
const ZOOM_DIGIT: u8 = b'0';

/// This frame's map keys, each as the engine codes it.
fn pressed(
    events: &mut MessageReader<KeyboardInput>,
    keys: &ButtonInput<KeyCode>,
    held: i32,
) -> Vec<i32> {
    events
        .read()
        .filter(|e| e.state.is_pressed())
        // A repeat counts while the key is the game's: the chat empties
        // the keys, a hot key takes its press.
        .filter(|e| keys.just_pressed(e.key_code) || (e.repeat && keys.pressed(e.key_code)))
        .filter_map(|e| hotkey::code(&e.logical_key))
        .map(|c| c | held)
        .collect()
}

/// Whether an enhancement keeps digit key `code`.
fn kept(code: i32, settings: &Settings, zooming: zoom::Mode) -> bool {
    let low = code & regent_core::view::menu::hotkey::CODE;
    let speed = SPEED_DIGITS.iter().any(|d| i32::from(*d) == low);

    (speed && settings.enhancements.speed_keys)
        || (low == i32::from(ZOOM_DIGIT) && zooming == zoom::Mode::Smooth)
}

/// The view centred on `at`, 32nds.
fn centre(maps: &Maps, view: &mut View, at: (i32, i32)) {
    let cell = crate::ui::tracking::cells(at);

    view.pan = maps.standing_at(cell);
}

/// Whether the game's keys are live: not while a text box has the
/// keyboard (the chat, SAVE GAME's name), a menu is open, or the game
/// has ended. Every system reading game keys asks it.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Live<'w, 's> {
    contexts: EguiContexts<'w, 's>,
    panels: Res<'w, Panels>,
    seen: Watch<'w>,
}

impl Live<'_, '_> {
    pub fn keys(&mut self) -> bool {
        let typing = self
            .contexts
            .ctx_mut()
            .is_ok_and(|ctx| ctx.egui_wants_keyboard_input());

        !typing && self.panels.state.options.open.is_none() && !self.seen.ended
    }
}

/// The map's keys, after the panels' and the chat's.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn keys(
    (mut events, keys, mut live): (
        MessageReader<KeyboardInput>,
        Res<ButtonInput<KeyCode>>,
        Live,
    ),
    (mut settings, pieces, maps): (ResMut<Settings>, Pieces, Res<Maps>),
    seen: Watch,
    (mut orders, mut bound): (ResMut<Orders>, ResMut<Bound>),
    (mut selected, mut reading, speaker): (ResMut<Selected>, ResMut<Reading>, Res<Speaker>),
    (mut view, mut speed, mut interface): (ResMut<View>, ResMut<Speedup>, ResMut<Interface>),
    (mut tracking, mut voices): (ResMut<crate::ui::tracking::Tracking>, ResMut<Voices>),
) {
    let pressed = pressed(&mut events, &keys, hotkey::held(&keys));

    if !live.keys() {
        return;
    }

    let zooming = zoom::Mode::of(&settings);

    for code in pressed {
        let mods = Mods::of(code);
        let at = |agent: u32| pieces.get(agent).map(|s| s.at);

        match map_keys::of(code) {
            Some(Key::Speaker) => {
                if let Some(at) = speaker.0.and_then(at) {
                    centre(&maps, &mut view, at);
                }
            }
            Some(Key::Voice) => voices.0.extend(selected.0),
            Some(Key::Speed(way)) => {
                let play = crate::ui::options::Play::of(seen.players);

                crate::game::speed::key(way, play, (&mut speed, &mut settings));
            }
            Some(Key::Read) => {
                if let Some(handle) = reading.0.take() {
                    interface
                        .0
                        .push(regent_core::view::panel::message::closed());
                    orders.outbox.push(Input::Read(handle));
                }
            }
            Some(Key::Cancel) => orders.mode = Mode::Idle,
            Some(Key::Scroll(x, y)) if !settings.enhancements.smooth_arrows => {
                #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
                let step = Vec2::new(x as f32, y as f32) * settings.ui.scroll() as f32;

                // A drag moves the picture; a scroll moves the view.
                view.drag(-step);
            }
            Some(Key::Spell(index)) if !kept(code, &settings, zooming) => {
                let aimed = match orders.mode {
                    Mode::Spell(o) => Some(o.row.spell),
                    _ => None,
                };
                let chosen = bound.0.spell(index, mods, aimed);
                let offer = chosen.and_then(|spell| {
                    seen.world()
                        .spells(seen.program, seen.local)
                        .into_iter()
                        .find(|o| o.row.spell == spell && o.ready)
                });

                if let Some(offer) = offer {
                    orders.mode = Mode::Spell(offer);
                }
            }
            Some(Key::Focus(index)) => {
                let looked = maps.pick_at(view.pan).unwrap_or_default();

                match bound.0.focus(index, mods, (selected.0, looked)) {
                    Some(Jump::Look(at)) => centre(&maps, &mut view, at),
                    Some(Jump::Select(id)) => {
                        focus(&seen, (&mut selected, &mut reading), &mut orders, id);
                    }
                    Some(Jump::Follow(id)) => {
                        let live = regent_core::view::panel::tracking::Live {
                            world: seen.world(),
                            local: seen.local,
                        };

                        focus(&seen, (&mut selected, &mut reading), &mut orders, id);
                        crate::ui::tracking::follow(
                            &mut tracking,
                            &live,
                            regent_core::view::panel::tracking::Target::Piece(id),
                        );
                    }
                    Some(Jump::Centre(id)) => {
                        focus(&seen, (&mut selected, &mut reading), &mut orders, id);

                        if let Some(at) = at(id) {
                            centre(&maps, &mut view, at);
                        }
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }
}

/// A focus key's piece selected where it may be, then the select mode.
fn focus(
    seen: &Watch,
    (selected, reading): (&mut Selected, &mut Reading),
    orders: &mut Orders,
    id: u32,
) {
    if seen.world().focusable(id) {
        selected.0 = Some(id);
        reading.0 = None;
    }

    orders.mode = Mode::Idle;
}

/// The bindings, a game's.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Bound>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::options::Play;
    use regent_core::sim::world::cheat::Cheats;

    /// What holds the keyboard as the game's keys are pressed.
    #[derive(Clone, Copy, Debug)]
    enum Over {
        Bare,
        Typing,
        Menu,
    }

    /// Whether M, 2 and L each did something, pressed `over` in a
    /// game `play`, the speed and debug keys on.
    fn game_keys_acted(over: Over, cheats: Cheats, play: Play) -> [bool; 3] {
        let (mut begun, _) = crate::game::begin::test_game(&[]);
        begun.sim.set_cheats(cheats);
        let mut app = App::new();
        let mut settings = Settings::default();
        let mut panels = Panels::default();
        let mut frame = crate::game::seen::Frame::of(&begun.sim);

        if play == Play::Lan {
            frame.players = 2;
        }

        settings.enhancements.speed_keys = true;
        settings.enhancements.debug_keys = true;

        if matches!(over, Over::Menu) {
            panels.state.options.open = Some(crate::ui::options::Dialog::Options);
        }

        app.insert_resource(settings)
            .insert_resource(panels)
            .insert_resource(frame)
            .insert_resource(Speedup(1.0))
            .init_resource::<crate::map::fog::Fog>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy_egui::EguiUserTextures>()
            .add_systems(
                Update,
                (
                    crate::ui::mute_key,
                    crate::game::speed::speed_key,
                    crate::map::fog::key,
                ),
            );

        // A text box focused: egui wants the keyboard.
        if matches!(over, Over::Typing) {
            let mut egui = bevy_egui::EguiContext::default();

            egui.get_mut()
                .memory_mut(|m| m.request_focus(bevy_egui::egui::Id::new("save name")));
            app.world_mut().spawn((egui, bevy_egui::PrimaryEguiContext));
        }

        // M on the bare map would write the settings file: not pressed.
        let pressed = match over {
            Over::Bare => &[KeyCode::Digit2, KeyCode::KeyL][..],
            Over::Typing | Over::Menu => &[KeyCode::KeyM, KeyCode::Digit2, KeyCode::KeyL],
        };
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();

        for key in pressed {
            keys.press(*key);
        }

        app.update();

        let world = app.world();

        [
            world.resource::<Settings>().audio.muted,
            (world.resource::<Speedup>().0 - 1.0).abs() > f32::EPSILON,
            world.resource::<crate::map::fog::Fog>().sight == crate::map::fog::Sight::Lifted,
        ]
    }

    /// The game's keys act on the bare map alone: not while a text box
    /// (SAVE GAME's name) has the keyboard, nor under a menu.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn game_keys_are_not_taken_from_a_text_box_or_a_menu() {
        assert_eq!(
            game_keys_acted(Over::Bare, Cheats::Allowed, Play::Alone),
            [false, true, true]
        );
        assert_eq!(
            game_keys_acted(Over::Typing, Cheats::Allowed, Play::Alone),
            [false; 3]
        );
        assert_eq!(
            game_keys_acted(Over::Menu, Cheats::Allowed, Play::Alone),
            [false; 3]
        );
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn debug_keys_are_barred_with_cheats() {
        assert_eq!(
            game_keys_acted(Over::Bare, Cheats::Barred, Play::Alone),
            [false, true, false]
        );
    }

    /// 1, 2 and 3 do nothing in a LAN game, as `<` `>` do not: the
    /// host's speed is everyone's.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn speed_keys_do_nothing_in_a_lan_game() {
        assert_eq!(
            game_keys_acted(Over::Bare, Cheats::Allowed, Play::Lan),
            [false, false, true]
        );
    }
}
