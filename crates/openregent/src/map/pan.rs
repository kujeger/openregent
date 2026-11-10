//! The view scrolled each drawn frame by the window's edge, as the
//! original's (`regent_core::view::map::pan`), and, with
//! `[enhancements] smooth_arrows`, by held arrow keys through the same
//! ramp. Else the arrows step once a key event (`map_keys`).
//!
//! ```text
//!  cursor in the window, within pan::EDGE of its edge ─┐
//!  held arrows (smooth_arrows)                         ├─> ±scroll_speed per axis
//!                                                      └─> pan::Ramp, real time ──> the view
//! ```
//!
//! None under a menu or once the game ends (the original's app states 7
//! and 8), nor while a panel control is held.
//! Hovering a panel stops nothing: the original's edge is the window's.
//! Past the window there is no cursor to read (winit reports it left),
//! so `-edgescroll`'s reach does not arise.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

use regent_core::view::map::pan::{self, Reach};
use regent_draw::stage::View;

use crate::files::config::Settings;
use crate::game::seen::Watch;
use crate::panel::Panels;

/// The ramp's state, a game's.
#[derive(Resource, Default)]
pub struct Ramp(pan::Ramp);

/// The arrow keys and their directions, x right and y down.
const ARROWS: [(KeyCode, (i32, i32)); 4] = [
    (KeyCode::ArrowLeft, (-1, 0)),
    (KeyCode::ArrowRight, (1, 0)),
    (KeyCode::ArrowUp, (0, -1)),
    (KeyCode::ArrowDown, (0, 1)),
];

/// The held arrows' delta: `speed` toward each axis's net direction.
fn held(keys: &ButtonInput<KeyCode>, speed: i32) -> (i32, i32) {
    let (x, y) = ARROWS
        .iter()
        .filter(|(key, _)| keys.pressed(*key))
        .fold((0, 0), |(x, y), (_, (dx, dy))| (x + dx, y + dy));

    (x.signum() * speed, y.signum() * speed)
}

/// The edge's delta, else the arrows', per axis.
const fn joined(edge: i32, arrows: i32) -> i32 {
    if edge == 0 { arrows } else { edge }
}

/// The view moved by this frame's ramped edge and arrows.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn scroll(
    window: Single<&Window, With<PrimaryWindow>>,
    (keys, time): (Res<ButtonInput<KeyCode>>, Res<Time<Real>>),
    mut contexts: EguiContexts,
    (settings, panels, seen): (Res<Settings>, Res<Panels>, Watch),
    (mut view, mut ramp): (ResMut<View>, ResMut<Ramp>),
) {
    let (using, typing) = contexts.ctx_mut().map_or((false, false), |ctx| {
        (ctx.egui_is_using_pointer(), ctx.egui_wants_keyboard_input())
    });
    let menu = panels.state.options.open.is_some() || seen.ended;

    // As the original skips its edge pass: the ramp not stepped.
    if menu || using {
        return;
    }

    let speed = settings.ui.scroll();

    #[allow(clippy::cast_possible_truncation, reason = "window pixels")]
    let size = (window.width() as i32, window.height() as i32);

    #[allow(clippy::cast_possible_truncation, reason = "window pixels")]
    let edge = window.cursor_position().map_or((0, 0), |at| {
        pan::edge((at.x as i32, at.y as i32), size, Reach::Near, speed)
    });
    let arrows = if settings.enhancements.smooth_arrows && !typing {
        held(&keys, speed)
    } else {
        (0, 0)
    };

    // Real time: a paused game scrolls alike.
    let ms = time.delta_secs() * 1000.0;
    let push = (joined(edge.0, arrows.0), joined(edge.1, arrows.1));
    let step = Vec2::from(ramp.0.step(ms, push));

    // Written only on a move: `View` drives the ground's redraw.
    if step != Vec2::ZERO {
        // A drag moves the picture; a scroll moves the view.
        view.drag(-step);
    }
}

/// The ramp, a game's.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Ramp>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_arrows_point_the_net_way() {
        let mut keys = ButtonInput::<KeyCode>::default();

        assert_eq!(held(&keys, 32), (0, 0));

        keys.press(KeyCode::ArrowLeft);
        keys.press(KeyCode::ArrowDown);
        assert_eq!(held(&keys, 32), (-32, 32));

        keys.press(KeyCode::ArrowRight);
        assert_eq!(held(&keys, 48), (0, 48));
    }

    #[test]
    fn the_edge_overrides_the_arrows_on_its_axis() {
        assert_eq!(joined(0, 32), 32);
        assert_eq!(joined(-32, 32), -32);
        assert_eq!(joined(32, 0), 32);
    }
}
