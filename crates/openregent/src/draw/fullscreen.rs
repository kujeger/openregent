//! Windowed or full screen (`[ui] window`), applied as it changes; Alt+Enter
//! flips it, in the menus and where the game's keys are live
//! (`map_keys::Live`), and writes the settings as the mute key does.
//!
//! ```text
//!  [ui] window ──> the primary window's mode, each change
//!  Alt+Enter   ──> windowed <-> full_screen, saved
//! ```

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, VideoModeSelection, WindowMode as Mode};
use bevy_egui::EguiContexts;

use crate::files::config::{self, Settings, WindowMode};

/// The keys that, with Alt, flip the window.
const FLIP_KEYS: [KeyCode; 2] = [KeyCode::Enter, KeyCode::NumpadEnter];

/// The Alt keys.
const ALT_KEYS: [KeyCode; 2] = [KeyCode::AltLeft, KeyCode::AltRight];

/// Bevy's window mode for `mode`, on the window's own screen.
#[must_use]
pub const fn mode(mode: WindowMode) -> Mode {
    match mode {
        WindowMode::Windowed => Mode::Windowed,
        WindowMode::FullScreen => Mode::BorderlessFullscreen(MonitorSelection::Current),
        WindowMode::Exclusive => {
            Mode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current)
        }
    }
}

/// The setting onto the primary window, where it differs; none headless.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn apply(settings: Res<Settings>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    let wanted = mode(settings.ui.window);

    for mut window in &mut windows {
        if window.mode != wanted {
            window.mode = wanted;
        }
    }
}

/// Whether Alt+Enter went down this frame.
pub fn pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed(ALT_KEYS) && keys.any_just_pressed(FLIP_KEYS)
}

/// The setting flipped and written.
fn flip(settings: &mut Settings) {
    settings.ui.window = settings.ui.window.flipped();

    if let Err(e) = config::save(settings) {
        warn!("settings not written: {e}");
    }
}

/// Alt+Enter in the menus, but while a text box has the keyboard.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn menu_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    mut settings: ResMut<Settings>,
) {
    let typing = contexts
        .ctx_mut()
        .is_ok_and(|ctx| ctx.egui_wants_keyboard_input());

    if pressed(&keys) && !typing {
        flip(&mut settings);
    }
}

/// Alt+Enter in a game, where its keys are live.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn game_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut live: crate::input::map_keys::Live,
    mut settings: ResMut<Settings>,
) {
    if pressed(&keys) && live.keys() {
        flip(&mut settings);
    }
}

/// The window kept to the setting, and the menus' key.
pub fn plugin(app: &mut App) {
    app.add_systems(Update, (apply, menu_key.run_if(not(crate::menu::in_game))));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A change of the setting reaches the window.
    #[test]
    fn the_window_takes_the_setting() {
        let mut app = App::new();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let mut settings = Settings::default();

        settings.ui.window = WindowMode::FullScreen;
        app.insert_resource(settings).add_systems(Update, apply);
        app.update();

        let shown = |app: &App| app.world().get::<Window>(window).map(|w| w.mode);

        assert_eq!(shown(&app), Some(mode(WindowMode::FullScreen)));

        app.world_mut().resource_mut::<Settings>().ui.window = WindowMode::Windowed;
        app.update();

        assert_eq!(shown(&app), Some(Mode::Windowed));
    }
}
