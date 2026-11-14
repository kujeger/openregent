//! The mouse cursor and the piece its ring marks: the game's `CUR1`
//! pictures as the window's hardware cursor ([`regent_core::view::map::cursor`]).
//!
//! ```text
//!  mouse off the map (bar, egui) ──> Dial
//!  on the selection panel's portrait, aiming (Portrait) ──> the spell's
//!  on the map or the tracking window (select::Aimed; the engine's cursor
//!  takes the mode of the view under it): the
//!  mode (Orders), the piece the engine's pick takes
//!    ──> Cursor ──> CUR1 set, slot ──> the window's CursorIcon
//!    ──> Hover: the piece the hover ring marks (select, spell modes)
//!    ──> Tipped: what the tooltip line tells of, met on its picture
//!        (the map alone)
//!  a mode's amount ──> text beside the cursor (egui pass)
//!  the menus ──> Dial, busy while a game is made (menu::Menu::cursor)
//! ```

use bevy::prelude::*;
use bevy::window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow};
use bevy_egui::{EguiContexts, egui};

use regent_core::gpl::load::Program;
use regent_core::sim::world::{World, offer::Aim};
use regent_core::view::map::cursor::{self, Cursor, Over, PRICE_AT, Price, Purse, over};
use regent_draw::data::Corpus;
use regent_draw::map::Maps;

use crate::game::scope::Scoped;
use crate::game::seen::{Frame, Watch};
use crate::input::orders::{FlagKind, Mode, Orders, Rules};
use crate::map::select::{Picked, Picker, Pointing, Selected, Through};
use crate::menu::{Menu, Screen, undraped};
use regent_draw::art::{ArtSource, Sprites};

/// The cursor's resources, a game's: as it ends, no piece hovered, no
/// cursor on the window, so the next screen puts its own; the menus'
/// cursor. [`point`] and [`amount`] are the game's frame's (`frame`).
pub fn plugin(app: &mut App) {
    app.per_game::<Pointer>()
        .per_game::<Hover>()
        .per_game::<SideHover>()
        .per_game::<Tipped>()
        .per_game::<Portrait>()
        // The menus' cursor over a new game's loading page too.
        .add_systems(
            Update,
            menus.run_if(in_state(Screen::Menu).or_else(not(undraped))),
        );
}

/// The piece the hover ring marks, by agent.
#[derive(Resource, Default)]
pub struct Hover(pub Option<u32>);

/// The standing building under the cursor in select mode that the
/// engine's hover ring skips ([`Picked::side_only`]), by agent: the side
/// ring's alone (`[enhancements] side_ring`). Its ring takes no part in
/// the pick, as [`Hover`]'s does.
#[derive(Resource, Default)]
pub struct SideHover(pub Option<u32>);

/// What the map's tooltip would tell of, under the pointer:
/// a piece met on its picture, by agent, or a
/// message flag, by handle.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tipped {
    #[default]
    Nothing,
    Piece(u32),
    Message(u32),
}

/// The unit whose selection panel portrait (8011) is under the pointer,
/// as the last egui pass found it: its cursor is [`on_portrait`]'s.
#[derive(Resource, Default)]
pub struct Portrait(pub Option<u32>);

/// The cursor shown and the amount it writes; its pictures are the art
/// service's (`art::Sprites::cursor`).
#[derive(Resource, Default)]
pub struct Pointer {
    /// The `CUR1` set and slot on the window, and the art it is from.
    shown: Option<(ArtSource, (u32, usize))>,
    /// The mode's amount, written beside the cursor.
    amount: Option<i32>,
}

/// Pick the cursor and the hovered piece, and put the cursor on the
/// window when it changes.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn point(
    mut commands: Commands,
    corpus: Res<Corpus>,
    pointing: Pointing,
    maps: Res<Maps>,
    window: Single<Entity, With<PrimaryWindow>>,
    seen: Watch,
    rules: Res<Rules>,
    orders: Res<Orders>,
    (selected, portrait): (Res<Selected>, Res<Portrait>),
    mut picker: Picker,
    mut images: ResMut<Assets<Image>>,
    mut sprites: ResMut<Sprites>,
    mut pointer: ResMut<Pointer>,
    mut hover: ResMut<Hover>,
    mut side_hover: ResMut<SideHover>,
    mut tipped: ResMut<Tipped>,
) {
    let entity = *window;
    let aimed = pointing.aimed();
    let world_at = aimed.map(|a| a.world);

    // The engine's pick, the boxes of the selected and last hovered piece
    // with their rings.
    let under = aimed.and_then(|a| a.under(&mut picker, (selected.0, hover.0)));

    let world = seen.world();
    let spot = aimed.and_then(|a| a.spot(&maps));
    let mode = Moment {
        world,
        frame: &seen,
        rules: &rules,
        orders: &orders,
        under,
        message: world_at.and_then(|at| picker.message_under(at)),
        spot,
    };
    let (cursor, marked, amount) = match (aimed, portrait.0) {
        (Some(_), _) => mode.cursor(),
        // The portrait's own cursor.
        (None, Some(unit)) => on_portrait(
            world,
            (seen.program, seen.local),
            orders.mode,
            (unit, picker.lit(unit)),
        ),
        (None, None) => (Cursor::Dialog, None, None),
    };

    hover.0 = marked;
    side_hover.0 = under
        .filter(|u| orders.mode == Mode::Idle && u.side_only())
        .map(|u| u.agent);

    // The tooltip's own pick: a picture hit, no holder. **Inferred**: a
    // message flag first, as the click takes it (`Picker::message_under`).
    // Not modelled over the tracking window: the map's alone.
    let tip_at = aimed.filter(|a| a.through == Through::Map).map(|a| a.world);
    let message = tip_at.and(mode.message);

    *tipped = match (message, tip_at.and_then(|at| picker.pictured(at))) {
        (Some(handle), _) => Tipped::Message(handle),
        (None, Some(agent)) => Tipped::Piece(agent),
        (None, None) => Tipped::Nothing,
    };

    pointer.amount = amount.filter(|a| *a != 0 && cursor.price() == Price::Shown);

    // A new picture: onto the window. One that will not read is tried
    // again next frame.
    let source = corpus.source();
    let key = cursor.set().map(|set| (source, (set, cursor.slot())));

    if let Some(key) = key
        && Some(key) != pointer.shown
    {
        let read = || {
            regent_draw::art::cursor::picture(&corpus.catalogue, key.1.0, key.1.1)
                .ok()
                .flatten()
        };

        if let Some((handle, hotspot)) = sprites.cursor(&mut images, key, read) {
            pointer.shown = Some(key);
            commands
                .entity(entity)
                .insert(CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
                    handle,
                    hotspot,
                    ..default()
                })));
        }
    }
}

/// The menus' cursor onto the window when it changes.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn menus(
    mut commands: Commands,
    (menu, art): (Res<Menu>, Res<crate::menu::Art>),
    window: Single<Entity, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: ResMut<Sprites>,
    mut pointer: ResMut<Pointer>,
    mut options: Single<&mut bevy::window::CursorOptions, With<PrimaryWindow>>,
) {
    // Hidden over a full-screen video.
    let visible = !menu.hides_cursor();

    if options.visible != visible {
        options.visible = visible;
    }

    let wanted = menu
        .cursor(&art)
        .map(|(key, shown)| ((art.cursor_source(), key), shown));

    if let Some((key, shown)) = wanted
        && pointer.shown != Some(key)
    {
        let read = || Some(shown.clone());

        if let Some((handle, hotspot)) = sprites.cursor(&mut images, key, read) {
            pointer.shown = Some(key);
            commands
                .entity(*window)
                .insert(CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
                    handle,
                    hotspot,
                    ..default()
                })));
        }
    }
}

/// What the cursor's state callbacks read this frame.
struct Moment<'a> {
    world: &'a World,
    frame: &'a Frame,
    rules: &'a Rules,
    orders: &'a Orders,
    under: Option<Picked>,
    /// A message flag met on its pixels: the pointing hand.
    message: Option<u32>,
    /// The picked vertex under the mouse, 32nds.
    spot: Option<(i32, i32)>,
}

impl Moment<'_> {
    /// The cursor on the map, the piece the ring marks, the mode's amount.
    fn cursor(&self) -> (Cursor, Option<u32>, Option<i32>) {
        let program = self.frame.program;
        let lit = self.under.filter(|u| u.lit).map(|u| u.agent);

        match self.orders.mode {
            Mode::Idle => {
                let over = match self.message {
                    Some(_) => Over::Piece,
                    None => self.under.map_or(Over::Ground, |u| over(u.kind, u.hit)),
                };

                (Cursor::Select(over), lit, None)
            }
            Mode::Build(code) => {
                let price = self
                    .world
                    .build_menu(&self.rules.0, self.frame.local)
                    .iter()
                    .find(|o| o.code == code)
                    .map_or(0, |o| o.price);

                (Cursor::Drag(self.purse(price)), None, Some(price))
            }
            Mode::Camp(_) => (Cursor::Move, None, self.frame.camp.map(|m| m.price)),
            Mode::Spell(offer) => {
                let spell = offer.row.spell;
                let target = self.under.map(|u| u.agent).filter(|t| {
                    self.world
                        .spell_target(program, self.frame.local, spell, *t)
                });
                let valid = match offer.aim {
                    Aim::Unit => target.is_some(),
                    Aim::Point(_) => self.spot.is_some_and(|at| {
                        self.world.spell_point(program, self.frame.local, spell, at)
                    }),
                    Aim::Now => false,
                };
                // Only a unit spell's valid target is marked.
                let marked = match offer.aim {
                    Aim::Unit => target.filter(|t| lit == Some(*t)),
                    _ => None,
                };
                let cursor = Cursor::Spell(spell, self.purse(offer.price), aim(valid));

                (cursor, marked, Some(offer.price))
            }
            Mode::Flag(FlagKind::Attack) => {
                let reward = self.orders.rewards.attack;
                // Met on its pixels or a character.
                let valid = self
                    .under
                    .filter(|u| u.met())
                    .is_some_and(|u| self.world.flaggable(self.frame.local, u.agent, reward));
                let cursor = Cursor::Attack(self.purse(reward), aim(valid));

                (cursor, None, Some(reward))
            }
            Mode::Flag(FlagKind::Explore) => {
                let reward = self.orders.rewards.explore;

                (Cursor::Explore(self.purse(reward)), None, Some(reward))
            }
        }
    }

    /// Whether the local player affords `amount` (`World::affords`).
    fn purse(&self, amount: i32) -> Purse {
        Purse::of(!self.world.affords(self.frame.local, amount))
    }
}

/// The cursor over the selection panel's portrait of `unit`, the piece
/// the ring marks, the mode's amount.
/// Aiming a spell: its cursor, the slot `unit`'s as a press would test
/// it (`World::spell_target`, as `World::spell_on`), and the ring on
/// `unit` where `lit`, valid or not, the treasury holding the price
/// (cursor hover update, cursor at 1). Else the `Dial`
/// hand. **Inferred**: spell key ready holds while a spell is
/// aimed.
fn on_portrait(
    world: &World,
    (program, local): (&Program, u32),
    mode: Mode,
    (unit, lit): (u32, bool),
) -> (Cursor, Option<u32>, Option<i32>) {
    match mode {
        Mode::Spell(offer) => {
            let spell = offer.row.spell;
            let purse = Purse::of(!world.affords(local, offer.price));
            let valid = world.spell_target(program, local, spell, unit);
            let marked = (purse == Purse::Holds && lit).then_some(unit);

            (
                Cursor::Spell(spell, purse, aim(valid)),
                marked,
                Some(offer.price),
            )
        }
        _ => (Cursor::Dialog, None, None),
    }
}

const fn aim(valid: bool) -> cursor::Aim {
    if valid {
        cursor::Aim::Valid
    } else {
        cursor::Aim::Invalid
    }
}

/// The mode's amount beside the cursor, [`PRICE_AT`] below the hotspot.
/// egui's font stands in for `SM09`.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn amount(
    mut contexts: EguiContexts,
    pointer: Res<Pointer>,
    window: Single<&Window, With<PrimaryWindow>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let at = window.cursor_position();

    if let (Some(amount), Some(at)) = (pointer.amount, at) {
        #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
        let offset = egui::vec2(PRICE_AT.0 as f32, PRICE_AT.1 as f32);
        let layer = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("cursor amount"),
        ));

        layer.text(
            egui::pos2(at.x, at.y) + offset,
            egui::Align2::LEFT_TOP,
            amount.to_string(),
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::data::fourcc::FourCc;
    use regent_core::sim::world::CROWN;
    use regent_core::sim::world::defeat::PALACE_CODE;
    use regent_core::sim::world::offer::{SPELLS, SpellOffer, aim};

    /// The portrait under the pointer while aiming shows the spell's
    /// cursor, as over the map: lightning bolt valid on the crown's
    /// palace, wind storm, wanting ground, the red bar; a short treasury
    /// the coins and no ring; the ring where the unit highlights, valid
    /// or not. Not aiming, the `Dial` hand.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_portrait_while_aiming_shows_the_spells_cursor() {
        let sim = crate::game::begin::test_game(&[]).0.sim;
        let (world, program) = (sim.world(), sim.program());
        let palace = world
            .shown()
            .iter()
            .find(|s| s.owner == CROWN && s.code.0.starts_with(PALACE_CODE))
            .map(|s| s.agent)
            .expect("the crown's palace");

        let aimed = |code: &[u8; 4], price| {
            let row = *SPELLS
                .iter()
                .find(|r| r.spell == FourCc(*code))
                .expect("a spell row");

            Mode::Spell(SpellOffer {
                row,
                price,
                aim: aim(row.spell),
                ready: true,
            })
        };
        let over = |mode, lit| on_portrait(world, (program, CROWN), mode, (palace, lit));

        let bolt = FourCc(*b"Sp12");
        let storm = FourCc(*b"Sp29");
        let valid = Cursor::Spell(bolt, Purse::Holds, cursor::Aim::Valid);

        assert_eq!(
            over(aimed(b"Sp12", 0), true),
            (valid, Some(palace), Some(0))
        );
        assert_eq!(over(aimed(b"Sp12", 0), false), (valid, None, Some(0)));
        assert_eq!(
            over(aimed(b"Sp29", 0), true),
            (
                Cursor::Spell(storm, Purse::Holds, cursor::Aim::Invalid),
                Some(palace),
                Some(0)
            )
        );

        // Short: the coins, no ring.
        let dear = 1_000_000_000;
        let (short, marked, _) = over(aimed(b"Sp12", dear), true);

        assert_eq!((short.slot(), marked), (2, None));
        assert_eq!(over(Mode::Idle, true), (Cursor::Dialog, None, None));
    }
}
