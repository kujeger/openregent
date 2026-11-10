//! A stopped game said at once: the sim thread's panic, a VM error, a
//! desync. The map stays drawn, nothing steps and no save is made, so
//! the box stays until the player leaves. The look is a placeholder.
//!
//! ```text
//!  The game has stopped and cannot be saved.
//!  the game's thread has ended: attempt to add with overflow
//!                                                  [Main Menu]
//! ```

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::game::thread::SimThread;
use crate::menu::{Return, Returning, Screen};

/// The box's first line: a placeholder.
const STOPPED: &str = "The game has stopped and cannot be saved.";

/// The press back to the menus: a placeholder.
const MAIN_MENU: &str = "Main Menu";

/// Where the box sits: the window's centre.
const AT: egui::Vec2 = egui::vec2(0.0, 0.0);

/// The box once the game has stopped: why, and a press back to the
/// menus, as the end panel's Main Menu goes.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn shown(
    mut contexts: EguiContexts,
    thread: Res<SimThread>,
    (mut returning, mut screen): (ResMut<Returning>, ResMut<NextState<Screen>>),
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();

    if let Some(why) = &thread.stopped {
        egui::Area::new(egui::Id::new("stopped"))
            .anchor(egui::Align2::CENTER_CENTER, AT)
            .order(egui::Order::Foreground)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.strong(STOPPED);
                    ui.label(why);

                    if ui.button(MAIN_MENU).clicked() {
                        returning.0 = Some(Return::Main);
                        screen.set(Screen::Menu);
                    }
                });
            });
    }

    Ok(())
}
