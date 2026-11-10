//! A new game with no save: the dialog's picks generated, and the
//! library's start stood up on the charts.
//!
//! ```text
//!  picks ──> custom::chosen ──> Choice ──> freestyle::setup_of
//!  quest + chart + Choice ──> world::build_over ──> the charts
//!  the charts ──> stand: crate::sim::start ──> the start's units, slots
//!  Made::start ──> crate::sim::setup::fresh ──> the library's World
//!            or stood: no script, the ground alone
//!  a scene's picks ──> generate_for ──> its units after the start's
//! ```

use crate::data::fourcc::FourCc;
use crate::data::rgs::Map;
use crate::data::rgs::chart::Chart as Patterns;
use crate::sim::freestyle::Setup;
use crate::sim::freestyle::{self, Choice, Picks, Play};
use crate::sim::scene::{Ground, Scene};
use crate::sim::setup;
use crate::sim::start::{self, Charted, Charts, Recipe, Start, Stood};
use crate::sim::units::HALVES;
use crate::sim::world::{
    World as Game,
    bodies::{Bodies, Body},
    combat::Mend,
    new_map::{Founding, OnGrid, Raised, Scatter, Seat},
    players::SESSION_PLAYERS,
};

use crate::sim::mapgen::custom::{self, Art};
use crate::sim::mapgen::mobject::{self, FOOT};
use crate::sim::mapgen::region::NONE;
use crate::sim::mapgen::world::{self, Over, World};

/// The generator's side of a new game: the charts, the start, the setup.
pub struct Made {
    pub choice: Choice,
    pub world: World,
    /// Every unit the start made, in order ([`start::stand`]).
    pub starts: Vec<Start>,
    /// [`Made::starts`] as the library takes them.
    pub units: Vec<Raised>,
    /// Every scatter object in landscape place's order, x outer:
    /// code and half cell.
    pub scatter: Vec<(FourCc, (i32, i32))>,
    /// Each of [`Made::scatter`]'s art slots.
    pub slots: Vec<u16>,
    pub setup: Setup,
    /// The players made, in slot order.
    pub seats: Vec<Seat>,
    /// The human seats, slots 0 up: the recipe's players.
    pub session: u32,
    /// The two special events, `EVSC` keys or `NONE`: none unless set
    /// after [`generate`], which they do not change.
    pub events: [FourCc; 2],
}

impl Made {
    /// What the library makes the new game from ([`setup::fresh`]).
    #[must_use]
    pub fn start(&self) -> setup::Start<'_> {
        setup::Start {
            ground: &self.world.field,
            cells: &self.world.cells,
            scatter: &self.scatter,
            slots: &self.slots,
            units: &self.units,
            setup: self.setup,
            seats: &self.seats,
            session: self.session,
            events: self.events,
        }
    }
}

/// Generate `choice`'s map and stand up its start, measured by `art`,
/// the setup holding `gold`: one player's ([`generate_for`]).
///
/// # Errors
///
/// A map [`world::build_over`] refuses ([`world::Refusal`]).
pub fn generate(
    map: &Map<'_>,
    patterns: &Patterns<'_>,
    choice: Choice,
    gold: u32,
    art: &Art,
) -> Result<Made, String> {
    generate_for(map, patterns, choice, (gold, freestyle::ONE_PLAYER), art)
}

/// [`generate`] for a game of `players`, each seat holding `gold`.
///
/// # Errors
///
/// A map [`world::build_over`] refuses ([`world::Refusal`]).
pub fn generate_for(
    map: &Map<'_>,
    patterns: &Patterns<'_>,
    choice: Choice,
    (gold, players): (u32, u32),
    art: &Art,
) -> Result<Made, String> {
    let setup = freestyle::setup_of(&choice, gold);
    let seats = Seat::freestyle(&setup, players);
    let recipe = Recipe::Freestyle(choice.picks(), Play::of(players));

    generate_with(map, patterns, (choice, recipe), (setup, seats), art)
}

/// [`generate`] for `recipe`'s world, the setup and players given: a
/// quest's ([`crate::sim::mapgen::quest`]).
///
/// # Errors
///
/// A map [`world::build_over`] refuses ([`world::Refusal`]).
pub fn generate_with(
    map: &Map<'_>,
    patterns: &Patterns<'_>,
    (choice, recipe): (Choice, Recipe),
    (setup, seats): (Setup, Vec<Seat>),
    art: &Art,
) -> Result<Made, String> {
    let reach = |code: FourCc| art.reaches.get(&code).copied().unwrap_or(mobject::POINT);
    let world = world::build_over(
        map,
        patterns,
        &choice,
        &Over {
            footprint: Some(&reach),
            ..Over::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let bodies = art.descriptors.bodies();
    let Stood { starts, slots } = stand(&world, map, recipe, bodies);
    let session = match recipe {
        Recipe::Freestyle(_, play) => play.session(),
        Recipe::Quest => SESSION_PLAYERS,
    };
    let units = start::raised(&starts, bodies);
    let scatter = scatter(&world)
        .iter()
        .map(|(code, at)| (*code, (HALVES * at.0, HALVES * at.1)))
        .collect();

    Ok(Made {
        choice,
        world,
        starts,
        units,
        scatter,
        slots,
        setup,
        seats,
        session,
        events: [setup::NO_EVENTS; 2],
    })
}

/// `scene`'s freestyle map ([`Ground::Freestyle`]): its picks generated
/// as [`generate_for`] makes them, the setup the scene's victory, the
/// start's units kept or not, then the scene's placed units
/// ([`Scene::start_units`]).
///
/// # Errors
///
/// Not a freestyle scene, picks [`custom::chosen`] refuses, a map
/// refused, or a placed unit the world cannot make.
pub fn scene(
    map: &Map<'_>,
    patterns: &Patterns<'_>,
    scene: &Scene,
    art: &Art,
) -> Result<Made, String> {
    let Ground::Freestyle(given) = scene.map else {
        return Err("not a freestyle scene".to_owned());
    };
    let picks = Picks {
        players: Some(scene.players),
        ..given
    };

    scene.check().map_err(|e| e.to_string())?;

    let choice = custom::chosen(&picks, map)?;
    let mut made = generate_for(map, patterns, choice, (scene.gold, scene.players), art)?;

    made.setup = freestyle::tagged(scene.aimed(made.setup), picks.asked());
    made.units = scene
        .start_units(&made.units, choice.side, art.descriptors.bodies())
        .map_err(|e| e.to_string())?;
    Ok(made)
}

/// The library's start on `world`'s charts ([`start::stand`]), measured
/// by `bodies`.
///
/// # Panics
///
/// If the map side exceeds `i32`.
#[must_use]
pub fn stand(world: &World, map: &Map<'_>, recipe: Recipe, bodies: &Bodies) -> Stood {
    let scatter = scatter(world);
    let units = |cell: (i32, i32)| {
        world
            .units
            .at(cell)
            .filter(|code| *code != NONE && *code != FOOT)
            .map(|code| Charted {
                code,
                source: world.units.source(cell),
                key: world.units.key(cell),
            })
    };
    let charts = Charts {
        side: world.field.side(),
        scatter: &scatter,
        units: &units,
        after: world.after.state.cast_unsigned(),
    };

    start::stand(&charts, map, recipe, bodies)
}

/// Every scatter object landscape place makes, in its walk: x
/// outer, y inner; code and cell.
fn scatter(world: &World) -> Vec<(FourCc, (i32, i32))> {
    let side = i32::try_from(world.field.side()).expect("a map side");

    (0..side)
        .flat_map(|x| (0..side).map(move |y| (x, y)))
        .filter_map(|at| {
            world
                .mobjects
                .at(at)
                .filter(|code| *code != NONE && *code != FOOT)
                .map(|code| (code, at))
        })
        .collect()
}

/// The library's world on `made` with no script ([`Game::fresh`]),
/// measured by `art`'s descriptors: each start unit made, and the ground
/// as its placement leaves it — levelled, paved, cleared, revealed.
///
/// A scatter object is on the grid unless its body says not.
#[must_use]
pub fn stood(made: &Made, art: &Art) -> Game {
    let scatter: Vec<Scatter> = made
        .scatter
        .iter()
        .zip(made.slots.iter().copied().chain(std::iter::repeat(0)))
        .map(|(&(code, at), slot)| Scatter {
            code,
            at,
            grid: art.descriptors.body(code).map_or(OnGrid::On, Body::grid),
            slot,
        })
        .collect();

    Game::fresh(
        &Founding {
            ground: &made.world.field,
            cells: &made.world.cells,
            scatter: &scatter,
            units: &made.units,
            setup: made.setup,
            seats: &made.seats,
            session: made.session,
            mend: Mend::default(),
        },
        art.descriptors.clone(),
    )
}
