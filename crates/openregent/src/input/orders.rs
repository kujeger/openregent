//! The player's orders: the mode a click on the map completes, the reward
//! panel's settings, and the inputs to send. The panels' buttons
//! ([`crate::panel`]) fill it; what is offered, and at what price, is the
//! library's (`regent_core::sim::world::offer`), and so is what a press or
//! click sends (`regent_core::sim::game::orders`); a click becomes a mode
//! the map completes ([`crate::input`]) or an `Input` sent at once.
//!
//! ```text
//!  panel button ──> Mode::Build / Spell / Flag / Camp ──> input: the map's click
//!               └─> Input (recruit, research, upgrade, raise…) ──> outbox
//!  outbox, a camp move begun, the reward settings ──> input::send,
//!               before the next frame
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use regent_core::content::unit::Unit;
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::load::Program;
use regent_core::sim::game::{Input, orders::reward_initial};
use regent_core::sim::world::{
    World, chat,
    flag::{Removal, Rewards},
    offer::{Menu, Offer, Placement, SpellOffer, Unavailable, by_title},
    sound::{self, Voice},
};
use regent_draw::data::Corpus;

use crate::ui::Memo;

/// What a left click on the map does now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Select a piece.
    #[default]
    Idle,
    /// Place a building of this code.
    Build(FourCc),
    /// Aim this spell.
    Spell(SpellOffer),
    /// Place a reward flag.
    Flag(FlagKind),
    /// Move this rangers' guild: its preview follows the cursor.
    Camp(u32),
}

/// The reward panel's two flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagKind {
    /// On a unit, `Flag_Attack`.
    Attack,
    /// At a point, `Flag_Explore`.
    Explore,
}

/// The build preview where the cursor puts it: code, half cell in
/// 32nds, and the library's verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preview {
    pub code: FourCc,
    pub at: (i32, i32),
    pub placement: Placement,
}

/// The player's pending orders: the mode, the reward panel's settings,
/// the inputs to send before the next frame.
#[derive(Resource, Default)]
pub struct Orders {
    pub mode: Mode,
    /// The reward panel's settings as the panel moves them, the game's
    /// (`Sim::rewards`) at the start, given back by
    /// [`crate::input::send`].
    pub rewards: Rewards,
    /// `#RewardInitial`, which the settings follow a treasury under.
    initial: i32,
    /// The flag whose removal waits for a yes or no (panel `AP62`).
    pub confirm: Option<u32>,
    /// Sent to the game by [`crate::input::send`].
    pub outbox: Vec<Input>,
    pub preview: Option<Preview>,
    /// A camp move pressed: its guild and price, begun by
    /// [`crate::input::send`] (`Sim::camp_begin`).
    pub camp_begun: Option<(u32, i32)>,
    /// The panels' own chat lines, said by [`crate::input::send`]:
    /// a refused press's.
    pub said: Vec<chat::Line>,
    /// The panels' own sounds, played by [`crate::input::send`]: the
    /// extortion's.
    pub sounds: Vec<(Voice, sound::Phase)>,
    /// The Options menu's surrender, confirmed: sent by
    /// [`crate::input::send`] (`Sim::surrender`).
    pub surrender: bool,
}

impl Orders {
    /// The reward panel at `rewards`, the game's.
    pub fn new(program: &Program, rewards: Rewards) -> Orders {
        Orders {
            rewards,
            initial: reward_initial(program),
            ..Orders::default()
        }
    }

    /// `#RewardInitial`.
    pub const fn initial(&self) -> i32 {
        self.initial
    }
}

/// The build menu's and panels' data, from HD's descriptors.
#[derive(Resource)]
pub struct Rules(pub std::sync::Arc<Menu>);

/// Descriptor titles, each looked up once.
#[derive(Resource, Default)]
pub struct Titles(HashMap<FourCc, String>);

impl Titles {
    /// Descriptor `code`'s title, else the code.
    fn of(&mut self, corpus: &Corpus, code: FourCc) -> String {
        self.0
            .entry(code)
            .or_insert_with(|| {
                Unit::find(&corpus.catalogue, code)
                    .and_then(Result::ok)
                    .map_or_else(|| code.to_string(), |u| u.desc.head.title().into_owned())
            })
            .clone()
    }
}

/// The build menu's rows: each building's title and offer.
type Rows = Arc<[(String, Offer)]>;

/// The build menu ([`Panel::menu`]), kept by game frame and toggle.
#[derive(Resource, Default)]
pub struct Built(Memo<(u32, Unavailable), Rows>);

/// What the order panels read and write.
#[derive(SystemParam)]
pub struct Panel<'w> {
    pub orders: ResMut<'w, Orders>,
    rules: Res<'w, Rules>,
    titles: ResMut<'w, Titles>,
    built: ResMut<'w, Built>,
}

impl Panel<'_> {
    /// The build menu's and panels' rules.
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// Descriptor `code`'s title.
    pub fn title(&mut self, corpus: &Corpus, code: FourCc) -> String {
        self.titles.of(corpus, code)
    }

    /// The palace's list with each building's title, sorted as the
    /// original sorts it: what is offered, and with
    /// [`Unavailable::Listed`] the rest, not offered; read once a game
    /// `frame`.
    pub fn menu(
        &mut self,
        corpus: &Corpus,
        (world, local): (&World, u32),
        (frame, unavailable): (u32, Unavailable),
    ) -> Rows {
        let Panel {
            rules,
            titles,
            built,
            ..
        } = self;

        built
            .0
            .get((frame, unavailable), || {
                let offers = world.build_list(&rules.0, local, unavailable);

                titled(offers, |code| titles.of(corpus, code)).into()
            })
            .clone()
    }
}

/// `offers` each with its title, by title (`by_title`).
fn titled(offers: Vec<Offer>, mut title: impl FnMut(FourCc) -> String) -> Vec<(String, Offer)> {
    let mut rows: Vec<_> = offers
        .into_iter()
        .map(|offer| (title(offer.code), offer))
        .collect();

    by_title(&mut rows);
    rows
}

/// Take down crown flag `id` as own flag twice does: at once, or
/// ask first ([`Removal`]).
pub fn remove(orders: &mut Orders, world: &World, id: u32) {
    match (world.removal(id), world.piece(id)) {
        (Some(Removal::Now), Some(p)) => orders.outbox.push(Input::Unflag(p.handle)),
        (Some(Removal::Ask), Some(_)) => orders.confirm = Some(id),
        _ => {}
    }
}

/// Panel `AP62`'s yes on the flag asked about (`KillConfirmController`):
/// `DESTROY_REWARD_FLAG_MESSAGE`. No sends nothing.
pub fn confirmed(orders: &mut Orders, world: &World) {
    if let Some(p) = orders.confirm.and_then(|id| world.piece(id)) {
        orders.outbox.push(Input::Unflag(p.handle));
    }

    orders.confirm = None;
}

/// Whether a click on the map selects: no order is waiting for it. No
/// orders before a game.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy run condition takes its parameters by value"
)]
pub fn idle(orders: Option<Res<Orders>>) -> bool {
    orders.is_some_and(|o| o.mode == Mode::Idle)
}

/// The rules, the orders and the titles, made as each game begins
/// ([`setup`]) and gone as it ends.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.made_by_game::<Rules>()
        .made_by_game::<Orders>()
        .made_by_game::<Titles>()
        .made_by_game::<Built>()
        .add_systems(OnEnter(crate::menu::Screen::Playing), setup);
}

/// The orders, from the program, at start-up; the reward settings the
/// game's, a resumed game's as saved. The rules come with the game
/// (`draw::install`).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn setup(mut commands: Commands, frame: Res<crate::game::seen::Frame>) {
    commands.insert_resource(Orders::new(frame.program, frame.rewards));
    commands.init_resource::<Titles>();
    commands.init_resource::<Built>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawn_build_rows_go_by_title_not_by_code() {
        let offer = |code: &[u8; 4]| Offer {
            code: FourCc(*code),
            price: 0,
            offered: true,
        };
        let titles = |code: FourCc| {
            match &code.0 {
                b"ABC1" => "Marketplace",
                b"ABD1" => "Blacksmith",
                b"ABE1" => "Inn",
                _ => "Agrela's Temple",
            }
            .to_owned()
        };
        let rows = titled(
            vec![
                offer(b"ABC1"),
                offer(b"ABD1"),
                offer(b"ABE1"),
                offer(b"ABO1"),
            ],
            titles,
        );
        let names: Vec<_> = rows.iter().map(|(name, _)| name.as_str()).collect();

        assert_eq!(
            names,
            ["Agrela's Temple", "Blacksmith", "Inn", "Marketplace"]
        );
    }
}
