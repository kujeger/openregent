//! A stopped game said at once: the sim thread's panic, a VM error, a
//! desync. The map stays drawn, nothing steps and no save is made, so
//! the box stays until the player leaves. The box is `AP11`'s art
//! (`notice`), grown to hold the reason in full; its words are
//! openregent's own.
//!
//! ```text
//!  ┌───────── Game Stopped ─────────┐
//!  │  The game has stopped and      │   title colour
//!  │  cannot be saved.              │
//!  │  the game's thread has ended:  │   the reason, wrapped in full
//!  │  attempt to add with overflow  │
//!  │        [  Main Menu  ]         │   OK's button
//!  └────────────────────────────────┘
//! ```

use bevy::prelude::*;
use bevy_egui::EguiContexts;

use regent_draw::art::Art;
use regent_draw::data::Corpus;

use super::notice::{self, Ink, Item, Means, Notice, Place, Pressed};
use crate::game::thread::SimThread;
use crate::menu::{Return, Returning, Screen};
use crate::panel::Panels;
use crate::ui::Covered;

/// The box's title: a placeholder.
const TITLE: &str = "Game Stopped";

/// The box's first line: a placeholder.
const STOPPED: &str = "The game has stopped and cannot be saved.";

/// The press back to the menus: a placeholder.
const MAIN_MENU: &str = "Main Menu";

/// Between the first line and the reason, pixels: openregent's own.
const GAP: f32 = 8.0;

/// The box once the game has stopped: why, and a press back to the
/// menus, as the end panel's Main Menu goes.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn shown(
    mut contexts: EguiContexts,
    thread: Res<SimThread>,
    (corpus, mut panels, mut art, mut covered): (
        Res<Corpus>,
        ResMut<Panels>,
        ResMut<Art>,
        ResMut<Covered>,
    ),
    (mut returning, mut screen): (ResMut<Returning>, ResMut<NextState<Screen>>),
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();

    if let Some(why) = &thread.stopped {
        let said = Notice {
            title: TITLE.to_owned(),
            items: vec![
                Item::Text(STOPPED.to_owned(), Ink::Strong),
                Item::Gap(GAP),
                Item::Text(why.clone(), Ink::Plain),
            ],
            button: Some(MAIN_MENU.to_owned()),
        };
        let means = Means {
            panels: &mut panels,
            art: &mut art,
            corpus: &corpus,
        };

        if let Some((pressed, rect)) = notice::show(&ctx, means, &said, (Place::Middle, "stopped"))
        {
            if pressed == Some(Pressed::Button) {
                returning.0 = Some(Return::Main);
                screen.set(Screen::Menu);
            }

            covered
                .0
                .push(Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y));
        }
    }

    Ok(())
}
