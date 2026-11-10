//! A game's own state: each module says which of its resources live for
//! one game, and how; the state machine does the rest.
//!
//! ```text
//!  plugin build   app.per_game::<T>()       T's default now, and again as each game ends
//!                 app.made_by_game::<T>()   put in by draw::install, gone as the game ends
//!  OnExit(Screen::Playing)
//!    Leave::Take    draw::uninstall: the game's entities; corpus and art to the shelf
//!    Leave::Reset   every made_by_game removed, every per_game back to its start
//! ```
//!
//! No list of a game's state is kept by hand: a resource a game uses is
//! registered where it is defined. `draw::tests::a_game_leaves_nothing_for_the_next`
//! finds one that is not.

use bevy::prelude::*;

use crate::menu::Screen;

/// The steps of taking a game down, in order.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Leave {
    /// Entities despawned, the dataset's corpus and art shelved: they read
    /// the game's resources.
    Take,
    /// The game's resources removed or back to their start.
    Reset,
}

/// Orders [`Leave`]'s steps.
pub fn plugin(app: &mut App) {
    app.configure_sets(OnExit(Screen::Playing), (Leave::Take, Leave::Reset).chain());
}

/// How a module registers its resources' life.
pub trait Scoped {
    /// `T` for each game: `start()` now, and again as each game ends.
    fn per_game_as<T: Resource>(&mut self, start: fn() -> T) -> &mut Self;

    /// `T` for each game, from its default.
    fn per_game<T: Resource + Default>(&mut self) -> &mut Self {
        self.per_game_as(T::default)
    }

    /// `T`, which a game is made with (`draw::install`, a game's
    /// `OnEnter`), removed as it ends.
    fn made_by_game<T: Resource>(&mut self) -> &mut Self;
}

impl Scoped for App {
    fn per_game_as<T: Resource>(&mut self, start: fn() -> T) -> &mut Self {
        let reset = move |mut commands: Commands| commands.insert_resource(start());

        self.insert_resource(start())
            .add_systems(OnExit(Screen::Playing), reset.in_set(Leave::Reset))
    }

    fn made_by_game<T: Resource>(&mut self) -> &mut Self {
        let remove = |mut commands: Commands| commands.remove_resource::<T>();

        self.add_systems(OnExit(Screen::Playing), remove.in_set(Leave::Reset))
    }
}
