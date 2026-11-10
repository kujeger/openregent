//! The map's half of the player's orders: where the cursor puts the build
//! preview, what a click in a mode sends, and the sending.
//!
//! ```text
//!  the pointer on the map or the tracking window, alike (select::Aimed;
//!  the engine's cursor acts in the view under it)
//!  cursor ──> Maps::pick_at ──> 32nds ──> seen::Interest ──> at the step,
//!             Game::placement ──> seen::Answers ──> Preview
//!             (a camp move: World::site, the ground alone)
//!  left click in a mode (what it sends: regent_core::sim::game::orders):
//!    Build      Fits: Unpreview, then Build at the preview's half cell;
//!               else Error_Alert (sound::refused)
//!    Camp       Fits: Camp at the preview's half cell, else Error_Alert;
//!               left otherwise, its price comes back (Sim::camp_leave)
//!    Spell      a point: Cast of its unit where the cursor takes it
//!               (World::spell_point); a unit: Bless on the target the
//!               cursor takes (World::spell_target); refused, out of a
//!               reach spell's reach: GMTX 40 in the chat, no sound
//!    Flag       explore: at the point; attack: on the unit, if flaggable
//!    Idle, a double click: World::twice, a flag the decode then selects;
//!               an own flag taken down, asked first (orders::remove)
//!  the selection panel's portrait while a spell is aimed: as a click
//!               on its unit, cast at its place (World::spell_on)
//!  right click or Escape: back to Idle
//!  Orders::outbox ──> Sim::input, before the next frame
//! ```

pub(crate) mod hotkey;
pub(crate) mod map_keys;
pub(crate) mod orders;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use regent_core::data::fourcc::FourCc;
use regent_core::gpl::load::Program;
use regent_core::sim::game::{Input, orders as sends};
use regent_core::sim::units::PER_HALF;
use regent_core::sim::world::{
    World, build::BuildOrder, chat, flag::Twice, guild::camp::Camp, offer::Placement,
};
use regent_core::view::map::cursor::{Over, over};
use regent_draw::map::Maps;

use regent_core::view::sound;

use crate::audio::Interface;
use crate::game::seen::{Ask, Frame, Interest, News, Outbox, Watch};
use crate::game::thread::SimThread;
use crate::input::orders::{FlagKind, Mode, Orders, Preview};
use crate::map::cursor::Hover;
use crate::map::mirror::Pieces;
use crate::map::select::{Picked, Picker, Pointing, Selected};
use crate::ui::minimap::Minimap;

/// Real ms between a double click's presses. The engine takes Windows'
/// `WM_LBUTTONDBLCLK` (class style `CS_DBLCLKS`, `CYWinApp`), timed by
/// the OS: `GetDoubleClickTime`'s default (**Inferred**: no setting read).
const DOUBLE_MS: f64 = 500.0;

/// Logical pixels a double click's second press may stray from its
/// first, each way: `SM_CXDOUBLECLK`'s default 4 about it (**Inferred**).
const DOUBLE_REACH: f32 = 2.0;

/// A left press, as Windows tells them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Single,
    Double,
}

/// The last single left press, and this update's.
#[derive(Resource, Default)]
pub struct Clicks {
    last: Option<(f64, Vec2)>,
    pub now: Option<Press>,
}

impl Clicks {
    /// A left press at `at` at real `ms`: double within the time and
    /// reach of the single one before; the press after a double is
    /// single again.
    pub(crate) fn press(&mut self, ms: f64, at: Vec2) -> Press {
        let near = |(t, p): (f64, Vec2)| {
            ms - t <= DOUBLE_MS && (at - p).abs().max_element() <= DOUBLE_REACH
        };
        let press = if self.last.is_some_and(near) {
            Press::Double
        } else {
            Press::Single
        };

        self.last = match press {
            Press::Single => Some((ms, at)),
            Press::Double => None,
        };
        press
    }
}

/// Sort this update's left press; any other button forgets the last.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn press(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    window: Single<&Window, With<PrimaryWindow>>,
    orders: Res<Orders>,
    mut clicks: ResMut<Clicks>,
) {
    // Shift with the left button pans (`stage::pan`).
    let panning = keys.pressed(KeyCode::ShiftLeft);
    let left = buttons.just_pressed(MouseButton::Left) && !panning;
    let other = buttons.get_just_pressed().any(|b| *b != MouseButton::Left);

    if other {
        clicks.last = None;
    }

    clicks.now = window
        .cursor_position()
        .filter(|_| left)
        .map(|at| clicks.press(time.elapsed_secs_f64() * 1000.0, at));

    // **Inferred**: a press in a mode, completing its order, starts no
    // double click; only the select mode's do.
    if orders.mode != Mode::Idle {
        clicks.last = None;
    }
}

/// A double click's flag, selected once made.
#[derive(Resource, Default)]
pub struct Awaited(Option<Await>);

/// A flag awaited: its code, the highest agent shown as it was sent,
/// the hand-over that carries it (`game::thread`), and the frame its
/// decode comes before, as the game reports it took the order.
#[derive(Clone, Copy, Debug)]
struct Await {
    code: FourCc,
    highest: u32,
    hand: Option<u64>,
    before: Option<u32>,
}

impl Awaited {
    /// Flag `code`, sent now, to be selected once made: an agent past
    /// `highest`, the highest shown now.
    pub(crate) fn expect(&mut self, code: FourCc, highest: Option<u32>) {
        self.0 = Some(Await {
            code,
            highest: highest.unwrap_or(0),
            hand: None,
            before: None,
        });
    }

    /// The flag goes with hand-over `hand`, if not yet stamped.
    fn sent(&mut self, hand: u64) {
        if let Some(awaited) = &mut self.0 {
            awaited.hand.get_or_insert(hand);
        }
    }

    /// The game took the orders of hand-over `hand` for the frame
    /// before `before`.
    fn ordered(&mut self, hand: u64, before: u32) {
        if let Some(awaited) = self.0.as_mut().filter(|a| a.hand == Some(hand)) {
            awaited.before = Some(before);
        }
    }
}

/// Select the flag a double click made, as its decode does for the
/// local player: looked for once
/// the frame the game took it for has run. A pipelined game may take an
/// order a frame or more after the one shown, so the frame is the
/// game's report, not the one shown at the click.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn choose(
    frame: Res<Frame>,
    pieces: Pieces,
    mut news: ResMut<News>,
    mut awaited: ResMut<Awaited>,
    mut selected: ResMut<Selected>,
) {
    for (hand, before) in news.ordered.drain(..) {
        awaited.ordered(hand, before);
    }

    let due = |a: &Await| a.before.is_some_and(|before| frame.frames >= before);

    if let Some(Await { code, highest, .. }) = awaited.0.filter(due) {
        let made = pieces
            .iter()
            .find(|s| s.code == code && s.owner == frame.local && s.agent > highest);

        if let Some(made) = made {
            selected.0 = Some(made.agent);
        }

        awaited.0 = None;
    }
}

/// The cursor on the map or the tracking window, in 32nds of a cell, as
/// the engine picks it: a whole vertex ([`Maps::pick_at`]). `None` off
/// both.
fn cursor_at(pointing: &Pointing, maps: &Maps) -> Option<(i32, i32)> {
    pointing.aimed().and_then(|aimed| aimed.spot(maps))
}

/// Right click or Escape leaves a mode; a right click on the minimap,
/// whose `Mini` row takes no mode, does not.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn cancel(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    minimap: Res<Minimap>,
    mut orders: ResMut<Orders>,
) {
    let on_minimap = window
        .cursor_position()
        .is_some_and(|at| minimap.covers(at));
    let right = buttons.just_pressed(MouseButton::Right) && !on_minimap;

    if right || keys.just_pressed(KeyCode::Escape) {
        orders.mode = Mode::Idle;
    }
}

/// The half cell under the cursor, 32nds, where a preview stands.
fn half_at(pointing: &Pointing, maps: &Maps) -> Option<(i32, i32)> {
    cursor_at(pointing, maps).map(|at| {
        (
            at.0.div_euclid(PER_HALF) * PER_HALF,
            at.1.div_euclid(PER_HALF) * PER_HALF,
        )
    })
}

/// The build preview the step answers (`seen::Interest`): asked before
/// it, so [`aim`] finds the answer this frame.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn want(
    pointing: Pointing,
    maps: Res<Maps>,
    orders: Res<Orders>,
    mut interest: ResMut<Interest>,
) {
    let at = half_at(&pointing, &maps);

    interest.preview = match (orders.mode, at) {
        (Mode::Build(code), Some(at)) => Some((code, at)),
        _ => None,
    };
}

/// Where the build preview stands under the cursor, and whether it
/// fits: the step's answer, [`Placement::Pending`] until it comes, and
/// while an order handed over is not yet in the answer (a build sent
/// there may take the spot).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn aim(
    pointing: Pointing,
    maps: Res<Maps>,
    (seen, thread): (Watch, Res<SimThread>),
    mut orders: ResMut<Orders>,
) {
    let at = half_at(&pointing, &maps);
    let answered = !thread.in_flight(&seen);

    orders.preview = match (orders.mode, at) {
        (Mode::Build(code), Some(at)) => Some(Preview {
            code,
            at,
            placement: seen
                .answers
                .placement(code, at)
                .filter(|_| answered)
                .unwrap_or(Placement::Pending),
        }),
        // The guild's own preview, asking the ground alone.
        (Mode::Camp(guild), Some(at)) => {
            let world = seen.world();

            world.piece(guild).map(|p| {
                let code = p.code;

                Preview {
                    code,
                    at,
                    placement: world.site(seen.program, seen.local, code, at),
                }
            })
        }
        _ => None,
    };
}

/// A left click on the map or the tracking window in a mode: the order
/// it completes, the same in either.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn click(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    pointing: Pointing,
    maps: Res<Maps>,
    (seen, mut outbox): (Watch, ResMut<Outbox>),
    pieces: Pieces,
    selected: Res<Selected>,
    hover: Res<Hover>,
    clicks: Res<Clicks>,
    mut picker: Picker,
    mut orders: ResMut<Orders>,
    mut awaited: ResMut<Awaited>,
    mut interface: ResMut<Interface>,
) {
    // Shift with the left button pans (`stage::pan`).
    let panning = keys.pressed(KeyCode::ShiftLeft);
    let clicked = buttons.just_pressed(MouseButton::Left) && !panning;
    let aimed = pointing.aimed();
    let ground = cursor_at(&pointing, &maps);
    let at = ground.filter(|_| clicked);

    // The piece the engine's pick takes under the cursor.
    let world = aimed.map(|a| a.world);

    // The select mode's double click: a message read, a reward flag, or
    // nothing.
    let double = clicks.now == Some(Press::Double) && orders.mode == Mode::Idle;
    let in_map = aimed.is_some();

    // A message flag read (`DestroyEventFlagMessage`) before the rest.
    let message = world
        .and_then(|w| picker.message_under(w))
        .and_then(|h| seen.world().reads(h));

    if let (true, Some(Twice::Read(handle))) = (double && in_map, message) {
        orders.outbox.push(Input::Read(handle));
    } else if double && in_map {
        let target = aimed
            .and_then(|a| a.under(&mut picker, (selected.0, hover.0)))
            .filter(|p| over(p.kind, p.hit) == Over::Piece)
            .map(|p| p.agent);
        match seen
            .world()
            .twice(seen.local, target, ground, orders.rewards)
        {
            Some(Twice::Flag(flag)) => {
                awaited.expect(flag.code, pieces.highest());
                orders.outbox.push(Input::Flag(flag));
            }
            // The first press selected it; its panel asks, if it must.
            Some(Twice::Unflag(id)) => orders::remove(&mut orders, seen.world(), id),
            // An own ruin or body: `DESTROY_VEHICLE` by its handle.
            Some(Twice::Clear(id)) => {
                if let Some(p) = seen.world().piece(id) {
                    orders.outbox.push(Input::Clear(p.handle));
                }
            }
            _ => {}
        }
    }

    let mut under = || aimed.and_then(|a| a.under(&mut picker, (selected.0, hover.0)));
    let standing = |agent: u32| pieces.get(agent).map(|s| s.at);

    if let Some(at) = at {
        // A sent move keeps its price (`Sim::camp_place`).
        let camp = match (orders.mode, placed(&orders)) {
            (Mode::Camp(guild), Some(at)) => Some(Camp { guild, at }),
            _ => None,
        };
        let sent = match camp {
            Some(_) => Vec::new(),
            None => order(
                &orders,
                (seen.world(), seen.program, seen.local),
                at,
                &mut under,
                standing,
            ),
        };

        if let Some(camp) = camp {
            outbox.push(Ask::CampPlace(camp));
            orders.mode = after_sent(orders.mode);
        } else if !sent.is_empty() {
            orders.outbox.extend(sent);
            orders.mode = after_sent(orders.mode);
        } else if refused(&orders) {
            interface.0.push(sound::refused());
        } else if let Some(line) =
            refusal(&orders, (seen.world(), seen.program, seen.local), at, under)
        {
            outbox.push(Ask::Say(line));
        }
    }
}

/// The mode after a click sent its order: a spell stays chosen as the
/// library says (`orders::keeps_mode`); every other order ends its mode.
fn after_sent(mode: Mode) -> Mode {
    match mode {
        Mode::Spell(spell) if sends::keeps_mode(spell.row.spell) => mode,
        _ => Mode::Idle,
    }
}

/// The half cell a fitting preview stands on, 32nds.
fn placed(orders: &Orders) -> Option<(i32, i32)> {
    orders
        .preview
        .filter(|p| p.placement == Placement::Fits)
        .map(|p| p.at)
}

/// Whether the preview refuses the spot clicked: a building's or a camp
/// move's, which then sounds `Error_Alert` and stays.
fn refused(orders: &Orders) -> bool {
    matches!(orders.mode, Mode::Build(_) | Mode::Camp(_))
        && orders
            .preview
            .is_some_and(|p| p.placement != Placement::Fits)
}

/// What a refused spell click at `at`, 32nds, says, `under` the piece
/// the pick takes: `GMTX` 40 out of a reach spell's reach, else `GMTX`
/// 34 short of its price (`orders::cast`).
fn refusal(
    orders: &Orders,
    (world, program, local): (&World, &Program, u32),
    at: (i32, i32),
    mut under: impl FnMut() -> Option<Picked>,
) -> Option<chat::Line> {
    match orders.mode {
        Mode::Spell(spell) => sends::cast((world, program), local, &spell, at, || {
            under().map(|p| p.agent)
        })
        .err()
        .flatten(),
        _ => None,
    }
}

/// The inputs a click at `at`, 32nds, sends in `orders`' mode; `under`
/// the piece the pick takes, `standing` a piece's place. None where it
/// does not complete the order. A camp move's is the game's
/// (`Sim::camp_place`).
fn order(
    orders: &Orders,
    (world, program, local): (&World, &Program, u32),
    at: (i32, i32),
    mut under: impl FnMut() -> Option<Picked>,
    standing: impl Fn(u32) -> Option<(i32, i32)>,
) -> Vec<Input> {
    let rewards = orders.rewards;

    match orders.mode {
        Mode::Idle | Mode::Camp(_) => Vec::new(),
        Mode::Build(code) => orders
            .preview
            .filter(|p| p.code == code && p.placement == Placement::Fits)
            .map(|p| {
                sends::build(BuildOrder {
                    code,
                    at: p.at,
                    owner: local,
                })
            })
            .unwrap_or_default(),
        // A unit spell on any piece the pick takes.
        Mode::Spell(spell) => sends::cast((world, program), local, &spell, at, || {
            under().map(|p| p.agent)
        })
        .ok()
        .into_iter()
        .collect(),
        Mode::Flag(FlagKind::Explore) => {
            vec![sends::explore_flag(world, local, at, rewards.explore)]
        }
        // Met on its pixels or a character.
        Mode::Flag(FlagKind::Attack) => under()
            .filter(|p| p.met())
            .and_then(|p| {
                sends::attack_flag(world, local, (p.agent, standing(p.agent)?), rewards.attack)
            })
            .into_iter()
            .collect(),
    }
}

/// A press on the selection panel's portrait of agent `target`
/// (`StatsWindowController` 8011): while a spell is aimed, its cast
/// on the agent at its place, tested as a click on it
/// (`World::spell_on`), the mode then as after a click; refused, a reach
/// spell's line or a short treasury's. Not aiming, nothing.
pub fn portrait(
    orders: &mut Orders,
    (world, program, local): (&World, &Program, u32),
    target: u32,
) {
    if let Mode::Spell(spell) = orders.mode {
        match world.spell_on(program, local, (spell.row.spell, spell.price), target) {
            Ok(bless) => {
                orders.outbox.push(Input::Bless(bless));
                orders.mode = after_sent(orders.mode);
            }
            Err(line) => orders.said.extend(line),
        }
    }
}

/// The orders made since the last frame, to the game: each before the
/// next frame. The [`Outbox`]'s first, in order; a camp move pressed
/// pays next; one left without its move, its mode gone, gets its price
/// back (`Sim::camp_leave`). The one system that hands the game changes:
/// queued for the sim thread, which takes them with the drawn frame
/// (`game::clock::tick`).
pub fn send(
    (mut orders, mut outbox): (ResMut<Orders>, ResMut<Outbox>),
    mut thread: ResMut<SimThread>,
    mut awaited: ResMut<Awaited>,
    interface: Option<ResMut<crate::audio::Interface>>,
) {
    // A flag awaited goes with the next hand-over.
    awaited.sent(thread.next());

    for ask in outbox.0.drain(..) {
        thread.queue(ask);
    }

    if let Some((guild, price)) = orders.camp_begun.take() {
        thread.queue(Ask::CampBegin(guild, price));
    }

    if std::mem::take(&mut orders.surrender) {
        thread.queue(Ask::Surrender);
    }

    if !matches!(orders.mode, Mode::Camp(_)) {
        thread.queue(Ask::CampLeave);
    }

    for input in orders.outbox.drain(..) {
        thread.queue(Ask::Input(input));
    }

    thread.queue(Ask::Rewards(orders.rewards));

    for line in orders.said.drain(..) {
        thread.queue(Ask::Say(line));
    }

    let sounds = std::mem::take(&mut orders.sounds);

    if let Some(mut interface) = interface {
        interface.0.extend(sounds);
    }
}

/// The clicks told single or double and the order awaited, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Clicks>().per_game::<Awaited>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::sim::world::CROWN;
    use regent_core::sim::world::spell::cast::{Bless, Cast};

    #[test]
    fn only_a_refused_preview_sounds_the_error() {
        let code = FourCc::new(b"ABC1");
        let shown = |placement| {
            let mut orders = Orders::default();

            orders.mode = Mode::Build(code);
            orders.preview = Some(Preview {
                code,
                at: (320, 480),
                placement,
            });
            orders
        };

        assert!(refused(&shown(Placement::Taken)));
        assert!(!refused(&shown(Placement::Fits)));
        assert!(!refused(&Orders::default()));
    }

    /// A Farseeing click on hidden ground beyond every wizards' guild's
    /// and tower's reach sends nothing and says `GMTX` 40; within reach it
    /// casts, or, the treasury short, sends nothing and says `GMTX` 34.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn farseeing_out_of_reach_is_refused() {
        use regent_core::sim::world::{
            chat::{NO_GOLD_SPELL, OUT_OF_REACH},
            offer::{SPELLS, SpellOffer, aim},
        };

        const WIND_STORM: FourCc = FourCc(*b"Sp29");

        let sim = crate::game::begin::test_game(&[]).0.sim;
        let program = sim.program();

        let farseeing = SPELLS[0];
        let mut orders = Orders::default();

        orders.mode = Mode::Spell(SpellOffer {
            row: farseeing,
            price: 0,
            aim: aim(farseeing.spell),
            ready: true,
        });

        // Hidden ground, cast or not: the map's cells by their middles.
        let world = sim.world();
        let cells = world.explored().cells();
        let hidden: Vec<(i32, i32)> = (0..cells)
            .flat_map(|x| (0..cells).map(move |y| (x * 32 + 16, y * 32 + 16)))
            .filter(|at| !world.spell_point(program, CROWN, WIND_STORM, *at))
            .collect();
        let casts = |at: &(i32, i32)| world.spell_point(program, CROWN, farseeing.spell, *at);
        let near = *hidden
            .iter()
            .find(|at| casts(at))
            .expect("hidden ground in reach");
        let far = *hidden
            .iter()
            .find(|at| !casts(at))
            .expect("hidden ground out of reach");
        let sent = |at| order(&orders, (world, program, CROWN), at, || None, |_| None);

        assert!(matches!(sent(near)[..], [Input::Cast(Cast { at, .. })] if at == near));
        assert_eq!(sent(far).len(), 0);
        assert_eq!(
            refusal(&orders, (world, program, CROWN), far, || None),
            Some(chat::Line::Text(OUT_OF_REACH))
        );
        assert_eq!(
            refusal(&orders, (world, program, CROWN), near, || None),
            None
        );

        // Priced past the treasury: the spot taken, the gold refused.
        let gold = world.treasury(CROWN).expect("a treasury");

        if let Mode::Spell(offer) = &mut orders.mode {
            offer.price = gold + 1;
        }

        assert_eq!(
            order(&orders, (world, program, CROWN), near, || None, |_| None).len(),
            0
        );
        assert_eq!(
            refusal(&orders, (world, program, CROWN), near, || None),
            Some(chat::Line::Text(NO_GOLD_SPELL))
        );
        assert_eq!(
            refusal(&orders, (world, program, CROWN), far, || None),
            Some(chat::Line::Text(OUT_OF_REACH))
        );
    }

    /// A Farseeing click in the tracking window sends what a click on the
    /// same ground in the main view sends: a cast in reach; out of it
    /// nothing, and `GMTX` 40 alike; an explore flag the same. The pointer on the picture under
    /// something else, or off both, aims nowhere.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_spell_clicked_in_the_tracking_window_is_the_maps_click() {
        use regent_core::sim::world::{
            chat::OUT_OF_REACH,
            offer::{SPELLS, SpellOffer, aim},
        };
        use regent_draw::map::load::Loaded;
        use regent_draw::stage::View;

        use crate::map::select::{Aimed, Through, aimed};
        use crate::ui::Covered;
        use crate::ui::tracking::{PICTURE, Tracking, cells};

        const WIND_STORM: FourCc = FourCc(*b"Sp29");

        let (begun, catalogue) = crate::game::begin::test_game(&[]);
        let sim = begun.sim;
        let (world, program) = (sim.world(), sim.program());
        let art = regent_core::sim::mapgen::custom::Art::of(&catalogue);
        let (source, _) = crate::map::ground::Laid::drawn(world, art).expect("a map");
        let maps = Maps::with(vec![Loaded::drawn("game".to_owned(), source)]);

        let farseeing = SPELLS[0];
        let mut orders = Orders::default();

        orders.mode = Mode::Spell(SpellOffer {
            row: farseeing,
            price: 0,
            aim: aim(farseeing.spell),
            ready: true,
        });

        // Hidden ground in and out of every wizards' guild's reach,
        // vertices clear of the map's edge.
        let cells_a_side = world.explored().cells();
        let inside = 4..cells_a_side - 4;
        let hidden: Vec<(i32, i32)> = inside
            .clone()
            .flat_map(|x| inside.clone().map(move |y| (x * 32, y * 32)))
            .filter(|at| !world.spell_point(program, CROWN, WIND_STORM, *at))
            .collect();
        let casts = |at: &(i32, i32)| world.spell_point(program, CROWN, farseeing.spell, *at);
        let near = *hidden.iter().find(|at| casts(at)).expect("in reach");
        let far = *hidden.iter().find(|at| !casts(at)).expect("out of reach");

        // The main view right of the column, the window in it.
        let covered = Covered::default();
        let screen = Rect::new(PICTURE.max.x + 10.0, 0.0, 1920.0, 1200.0);

        for target in [near, far] {
            // Just off the vertex: the pick's tests are strict on it.
            let ground = maps.standing_at(cells(target) + Vec2::new(0.1, 0.2));
            let view = View {
                rect: Some(screen),
                zoom: 1.0,
                pan: ground + Vec2::new(40.0, -25.0),
                ..View::default()
            };
            let tracking = Tracking::shown(PICTURE, ground);
            let on_map = view.screen_at(ground).expect("a rect");
            let on_window = Vec2::new(PICTURE.center().x, PICTURE.center().y);

            let by_map = aimed(&view, &covered, &tracking, on_map).expect("on the map");
            let by_window = aimed(&view, &covered, &tracking, on_window).expect("on the window");

            assert_eq!(by_map.through, Through::Map);
            assert_eq!(by_window.through, Through::Window);

            let spot = |a: Aimed| a.spot(&maps).expect("on the ground");
            let sent = |at| order(&orders, (world, program, CROWN), at, || None, |_| None);
            let said = |at| refusal(&orders, (world, program, CROWN), at, || None);

            assert_eq!(spot(by_window), target);
            assert_eq!(spot(by_map), spot(by_window));
            assert_eq!(sent(spot(by_map)), sent(spot(by_window)));
            assert_eq!(said(spot(by_map)), said(spot(by_window)));

            // An explore flag the same.
            let mut exploring = Orders::default();

            exploring.mode = Mode::Flag(FlagKind::Explore);

            let flagged = |at| order(&exploring, (world, program, CROWN), at, || None, |_| None);

            assert_eq!(flagged(spot(by_map)), flagged(spot(by_window)));
            assert!(matches!(flagged(spot(by_window))[..], [Input::Flag(_)]));
        }

        // What each sends.
        let sent = |at| order(&orders, (world, program, CROWN), at, || None, |_| None);

        assert!(matches!(sent(near)[..], [Input::Cast(Cast { at, .. })] if at == near));
        assert_eq!(sent(far).len(), 0);
        assert_eq!(
            refusal(&orders, (world, program, CROWN), far, || None),
            Some(chat::Line::Text(OUT_OF_REACH))
        );

        // Off the picture, or the picture not under the pointer in egui's
        // pass (something over it, or not shown): nowhere.
        let view = View {
            rect: Some(screen),
            zoom: 1.0,
            ..View::default()
        };
        let shown = Tracking::shown(PICTURE, Vec2::ZERO);
        let on_window = Vec2::new(PICTURE.center().x, PICTURE.center().y);

        assert!(aimed(&view, &covered, &shown, Vec2::new(2.0, 2.0)).is_none());
        assert!(aimed(&view, &covered, &Tracking::default(), on_window).is_none());
    }

    /// The portrait pressed while aiming: a lightning bolt on the crown's
    /// palace at its place, the spell kept; wind storm, wanting ground,
    /// and an unknown agent send nothing; not aiming, nothing.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_portrait_press_while_aiming_casts_on_its_unit() {
        use regent_core::sim::world::{
            defeat::PALACE_CODE,
            offer::{SPELLS, SpellOffer, aim},
        };

        let sim = crate::game::begin::test_game(&[]).0.sim;
        let program = sim.program();
        let world = sim.world();
        let palace = world
            .shown()
            .iter()
            .find(|s| s.owner == CROWN && s.code.0.starts_with(PALACE_CODE))
            .map(|s| s.agent)
            .expect("the crown's palace");

        let aimed = |code: &[u8; 4]| {
            let row = *SPELLS
                .iter()
                .find(|r| r.spell == FourCc(*code))
                .expect("a spell row");

            Mode::Spell(SpellOffer {
                row,
                price: 0,
                aim: aim(row.spell),
                ready: true,
            })
        };
        let pressed = |mode: Mode, target: u32| {
            let mut orders = Orders::default();

            orders.mode = mode;
            portrait(&mut orders, (world, program, CROWN), target);
            orders
        };

        let bolt = pressed(aimed(b"Sp12"), palace);

        assert_eq!(
            bolt.outbox,
            [Input::Bless(Bless {
                spell: FourCc(*b"Sp12"),
                target: palace,
                owner: CROWN,
                at: world.position(palace).expect("placed"),
            })]
        );
        assert_eq!(bolt.mode, aimed(b"Sp12"));

        for (mode, target) in [
            (aimed(b"Sp29"), palace),
            (aimed(b"Sp12"), u32::MAX),
            (Mode::Idle, palace),
        ] {
            let left = pressed(mode, target);

            assert!(left.outbox.is_empty() && left.said.is_empty());
            assert_eq!(left.mode, mode);
        }
    }

    /// A cast keeps its spell chosen, but for the Abode's six.
    #[test]
    fn a_spell_stays_chosen_after_a_cast_but_the_abodes() {
        use regent_core::sim::world::offer::{SPELLS, SpellOffer, aim};

        let chosen = |code: &[u8; 4]| {
            let row = *SPELLS
                .iter()
                .find(|r| r.spell == FourCc(*code))
                .expect("a spell row");

            Mode::Spell(SpellOffer {
                row,
                price: 0,
                aim: aim(row.spell),
                ready: true,
            })
        };
        let farseeing = chosen(b"Sp10");

        assert_eq!(after_sent(farseeing), farseeing);
        assert_eq!(after_sent(chosen(b"Sp33")), Mode::Idle);
        assert_eq!(after_sent(Mode::Idle), Mode::Idle);
    }

    /// Windows' rule: a second press within 500 ms and 2 pixels each way
    /// is a double click; the press after it starts over.
    #[test]
    fn a_second_press_near_in_time_and_place_is_a_double_click() {
        let mut clicks = Clicks::default();
        let at = Vec2::new(100.0, 100.0);

        assert_eq!(clicks.press(0.0, at), Press::Single);
        assert_eq!(
            clicks.press(DOUBLE_MS, at + Vec2::splat(DOUBLE_REACH)),
            Press::Double
        );
        assert_eq!(clicks.press(DOUBLE_MS + 10.0, at), Press::Single);

        // Too late, then too far.
        assert_eq!(clicks.press(2000.0, at), Press::Single);
        assert_eq!(clicks.press(2000.0 + DOUBLE_MS + 1.0, at), Press::Single);
        assert_eq!(
            clicks.press(2600.0, at + Vec2::new(3.0, 0.0)),
            Press::Single
        );
    }
}
