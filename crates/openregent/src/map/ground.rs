//! Ground and scatter a building's placement changed, redrawn.
//!
//! ```text
//!  Board::relaid moved ──> Source::of_world: heights, shade, paint,
//!   (the drawn Source's     pictures, scatter
//!    generation behind) └──> GRND entities by record handle: one the
//!                            world dropped despawned, one it reseated
//!                            lifted by the height it changed, the rest
//!                            renumbered
//!                       └──> the ground recomposed
//! ```
//!
//! The viewer composes only the ground on screen, so a recompose costs
//! what a pan does.

use std::collections::BTreeMap;

use bevy::prelude::*;

use regent_core::base::fixed::Fixed;
use regent_core::sim::mapgen::custom::Art;
use regent_core::sim::world::World;
use regent_draw::lie::Footing;
use regent_draw::map::load::Loaded;
use regent_draw::map::things::Record;
use regent_draw::map::{MapSource, Maps, Placed, SCATTER};

use crate::game::seen::Watch;

/// A drawn place whose record was cleared: past any source's records, so
/// no record is found.
pub const STRUCK: usize = usize::MAX;

/// What the drawing last took from the world.
#[derive(Resource)]
pub struct Laid {
    /// Each drawn `GRND` record's handle and height, in the drawn
    /// source's order.
    records: Vec<(u32, Fixed)>,
    /// Picks a picture for a cell the world knows none of.
    art: Art,
}

/// Each `GRND` record's handle and height in `source`.
fn records(source: &MapSource) -> Vec<(u32, Fixed)> {
    source.objects[SCATTER]
        .iter()
        .map(|o| (o.handle, o.height))
        .collect()
}

impl Laid {
    /// `world`'s source to draw, and what it holds. `None` without a map.
    pub fn drawn(world: &World, art: Art) -> Option<(MapSource, Laid)> {
        let source = MapSource::of_world(world, &|c, at| art.dress(c, at))?;
        let records = records(&source);

        Some((source, Laid { records, art }))
    }

    /// `world`'s source as a new drawing would draw it: a test's.
    #[cfg(test)]
    pub fn fresh(&self, world: &World) -> Option<MapSource> {
        MapSource::of_world(world, &|c, at| self.art.dress(c, at))
    }
}

/// What [`relay`] moves of a drawn scatter part.
type Scattered = (
    Entity,
    &'static mut Record,
    &'static mut Transform,
    Option<&'static mut Footing>,
);

/// Redraw what a placement changed, once a frame: when the world's
/// ground generation is past the drawn source's.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn relay(
    seen: Watch,
    mut commands: Commands,
    mut laid: ResMut<Laid>,
    mut maps: ResMut<Maps>,
    mut things: Query<Scattered>,
) {
    let world = seen.world();
    let generation = Some(world.board().relaid());
    let behind = maps
        .current()
        .is_some_and(|l| l.source.generation != generation);

    if behind {
        let at = maps.at;

        match redrawn(&maps, &laid, world) {
            Some(loaded) => {
                let moves = moves(&laid.records, &loaded.source);

                // World units a height unit lifts a thing.
                let rise = maps.aloft_at(Vec2::ZERO, 1.0).y - maps.aloft_at(Vec2::ZERO, 0.0).y;

                // The fast path: what changed moves, no full redraw.
                for (entity, mut record, mut transform, footing) in &mut things {
                    if record.chunk == SCATTER {
                        match moves.get(record.index).copied().flatten() {
                            Some((index, dz)) => {
                                transform.translation.y += rise * rose(dz);
                                record.index = index;

                                // The ground its shadow lies from.
                                if let Some(mut footing) = footing {
                                    footing.ground += rise * rose(dz);
                                }
                            }
                            None => commands.entity(entity).despawn(),
                        }
                    }
                }

                // Ring and click test follow the sprite; a struck one keeps
                // its row, as the depths drawn did (`map::pieces::Depth`), but
                // names no record.
                let follow = |p: Placed| match p.chunk {
                    SCATTER => match moves.get(p.index).copied().flatten() {
                        Some((index, dz)) => Placed {
                            index,
                            height: p.height + rose(dz),
                            ..p
                        },
                        None => Placed { index: STRUCK, ..p },
                    },
                    _ => p,
                };

                maps.placed = maps.placed.iter().copied().map(follow).collect();
                maps.selected = maps.selected.map(follow);
                laid.records = records(&loaded.source);
                maps.loaded[at] = loaded;
                maps.recompose();
            }
            // No map to redraw: not tried again until the ground moves.
            None => maps.loaded[at].source.generation = generation,
        }
    }
}

/// `world`'s map to draw in place of the drawn one; `None` without one.
fn redrawn(maps: &Maps, laid: &Laid, world: &World) -> Option<Loaded> {
    maps.current().and_then(|l| {
        let source = MapSource::of_world(world, &|c, at| laid.art.dress(c, at))?;

        Some(Loaded::drawn(l.name.clone(), source))
    })
}

/// `world`'s map drawn whole, as a new game's: a LAN resync's world,
/// whose `Board::relaid` says nothing of the drawn one's. The ground,
/// and the scatter as `map::redraw` draws a new map's (`Maps::revision`):
/// what only `world` holds spawned, every other despawned.
pub fn anew(maps: &mut Maps, laid: &mut Laid, world: &World) {
    if let Some(loaded) = redrawn(maps, laid, world) {
        let at = maps.at;

        laid.records = records(&loaded.source);
        maps.loaded[at] = loaded;
        maps.placed.clear();
        maps.selected = None;
        maps.revision += 1;
        maps.recompose();
    }
}

/// Where each record of `before` lands in `now`, by handle, and how far
/// it rose; `None` once cleared.
fn moves(before: &[(u32, Fixed)], now: &MapSource) -> Vec<Option<(usize, Fixed)>> {
    let now: BTreeMap<u32, (usize, Fixed)> = now.objects[SCATTER]
        .iter()
        .enumerate()
        .map(|(i, o)| (o.handle, (i, o.height)))
        .collect();

    before
        .iter()
        .map(|(handle, was)| now.get(handle).map(|&(i, is)| (i, is - *was)))
        .collect()
}

/// A 16.16 rise in the ground's units, as `map::height_of` reads one.
fn rose(dz: Fixed) -> f32 {
    regent_core::base::fixed::to_f32(dz)
}

/// What the drawn ground holds, which the game is made with.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.made_by_game::<Laid>();
}

#[cfg(test)]
mod tests {
    use regent_core::base::fixed;
    use regent_core::content::terrain::grid::{Cell, Vertex};
    use regent_core::data::fourcc::FourCc;

    use regent_core::sim::paint;
    use regent_core::sim::units::{PER_HALF, from_pos32};

    use regent_draw::map::Item;
    use regent_draw::map::project::positions;

    use super::*;
    use crate::game::sim::Sim;

    const TREE: FourCc = FourCc(*b"RG07");

    /// A flat 4-cell grass map holding `things`.
    fn bare_source(things: &[Item]) -> MapSource {
        let vertex = Vertex {
            height: 0.0,
            shade: 0,
            flags: 0x80,
        };
        let grass = Cell {
            pair: paint::Cell::uniform(1).pair,
            case: 0,
            variant: 0,
            rotation: 0,
        };
        let mut objects: [Vec<Item>; 4] = Default::default();

        objects[SCATTER] = things.to_vec();

        MapSource::of_arrays(vec![vertex; 9], vec![grass; 4], objects)
    }

    /// A tree owned by player 7 at `at`, 32nds of a cell, handle `handle`.
    const fn tree(handle: u32, at: (i32, i32)) -> Item {
        Item {
            id: TREE,
            handle,
            owner: Some(7),
            held: false,
            fixed: (from_pos32(at.0), from_pos32(at.1)),
            height: Fixed::ZERO,
            standing: (at.0 / PER_HALF, at.1 / PER_HALF),
            set: 0,
            drawing: (0, 0),
        }
    }

    /// Three trees; a placement clears the first and raises the second:
    /// the first is gone, the second moves up and down a row, the third
    /// down a row.
    #[test]
    fn a_redrawn_record_is_found_by_its_handle() {
        let before = bare_source(&[tree(1, (16, 16)), tree(2, (32, 16)), tree(3, (48, 16))]);
        let mut now = bare_source(&[tree(2, (32, 16)), tree(3, (48, 16))]);

        now.objects[SCATTER][0].height += fixed::from_whole(3);

        assert_eq!(
            moves(&records(&before), &now),
            [
                None,
                Some((0, fixed::from_whole(3))),
                Some((1, Fixed::ZERO))
            ]
        );
    }

    /// A tree reseated 3 units up: the fast path's shift lands its sprite
    /// where a full redraw of the rebuilt source draws it.
    #[test]
    fn a_shifted_tree_lands_where_a_redraw_draws_it() {
        let up = fixed::from_whole(3);
        let source = bare_source(&[tree(1, (16, 16))]);
        let mut rebuilt = source.clone();

        rebuilt.objects[SCATTER][0].height = up;

        let was = source.objects[SCATTER][0].height;

        // The tree as a full redraw places it, and a height unit's rise.
        let place = |source: MapSource| {
            let maps = Maps::with(vec![Loaded::drawn("bare".to_owned(), source)]);
            let rise = maps.aloft_at(Vec2::ZERO, 1.0).y - maps.aloft_at(Vec2::ZERO, 0.0).y;

            (maps.drawn_at(positions(&maps)[0]), rise)
        };
        let (before, rise) = place(source);
        let (after, _) = place(rebuilt);

        assert_eq!(after, before + Vec2::Y * rise * rose(up - was));
        assert_ne!(after, before);
    }

    /// What `before` holds that `now` does not, counting repeats.
    fn gone<T: Ord + Copy>(before: &[T], now: &[T]) -> Vec<T> {
        let mut left: BTreeMap<T, usize> = BTreeMap::new();

        for key in now {
            *left.entry(*key).or_default() += 1;
        }

        before
            .iter()
            .filter(|key| match left.get_mut(key) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    false
                }
                _ => true,
            })
            .copied()
            .collect()
    }

    /// The game `args` asks for, drawn as the game draws it, with
    /// [`relay`] each frame; and the game.
    fn drawn_game(args: &[&str]) -> (App, Sim, regent_core::data::catalogue::Catalogue<'static>) {
        let (begun, catalogue) = crate::game::begin::test_game(args);
        let (source, laid) = Laid::drawn(begun.sim.world(), Art::of(&catalogue)).expect("a map");
        let records = source.objects[SCATTER].len();
        let mut maps = Maps::with(vec![Loaded::drawn("game".to_owned(), source)]);
        let mut app = App::new();

        // Every record placed, as the drawing places them: a relay
        // strikes the ones cleared.
        maps.placed = positions(&maps);

        app.insert_resource(crate::game::seen::Frame::of(&begun.sim))
            .insert_resource(laid)
            .insert_resource(maps)
            .add_systems(Update, relay);

        for index in 0..records {
            app.world_mut().spawn((
                Record {
                    chunk: SCATTER,
                    index,
                },
                Transform::default(),
            ));
        }

        (app, begun.sim, catalogue)
    }

    /// One game frame the bot plays, then one drawn frame of it.
    fn step(app: &mut App, sim: &mut Sim, bot: &mut crate::game::bot::Bot) {
        for input in bot.moves(sim) {
            sim.input(input);
        }

        sim.step();
        app.insert_resource(crate::game::seen::Frame::of(sim));
        app.update();
    }

    /// The game `args` asks for, `frames` frames of it the bot plays:
    /// after every frame, the scatter drawn — each `GRND` entity's record
    /// in the drawn source, on the grid — is the world's, and the fog's
    /// cell index takes the places (`fog::record_cells`). The places
    /// struck by the end.
    fn follows(args: &[&str], frames: u32) -> usize {
        use regent_core::sim::world::{new_map::OnGrid, offer::Menu};

        const BOT: u32 = 7;

        let (mut app, mut sim, catalogue) = drawn_game(args);
        let mut bot = crate::game::bot::Bot::new(Menu::of(&catalogue), BOT);
        let bodies = regent_core::sim::setup::descriptors(&catalogue)
            .bodies()
            .clone();
        let gridded = |code: &FourCc| bodies.get(code).is_none_or(|b| b.grid() == OnGrid::On);

        // What is drawn and not the world's, and the world's not drawn.
        let compare = |app: &mut App, sim: &Sim| {
            let indices: Vec<usize> = app
                .world_mut()
                .query::<&Record>()
                .iter(app.world())
                .filter(|r| r.chunk == SCATTER)
                .map(|r| r.index)
                .collect();
            let maps = app.world().resource::<Maps>();
            let objects = &maps.current().expect("a drawn source").source.objects[SCATTER];
            let drawn: Vec<(FourCc, (i32, i32))> = indices
                .iter()
                .map(|i| (objects[*i].id, objects[*i].standing))
                .filter(|(code, _)| gridded(code))
                .collect();
            let world = sim.world().board().scatter();

            // Each record's cell, as the fog reads them; a struck place
            // names no record.
            let cells = crate::map::fog::record_cells(maps);

            assert!(cells.len() <= objects.len(), "a cell past the records");

            (gone(&drawn, world), gone(world, &drawn))
        };

        // The first drawn frame, before any game frame.
        app.update();

        assert_eq!(compare(&mut app, &sim), (vec![], vec![]), "at the start");

        for _ in 0..frames {
            step(&mut app, &mut sim, &mut bot);

            let frame = sim.frames();

            assert_eq!(compare(&mut app, &sim), (vec![], vec![]), "frame {frame}");
        }

        let maps = app.world().resource::<Maps>();

        maps.placed.iter().filter(|p| p.index == STRUCK).count()
    }

    /// A custom game: the bot's buildings clear scatter, and the drawing
    /// follows.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_drawn_scatter_follows_the_world_through_a_bot_game() {
        let struck = follows(&["--map-seed", "3"], 3_000);

        assert!(struck > 0, "no place struck");
    }

    /// The Barren Waste's setup spawns chests, whose rings clear scatter:
    /// the drawing, made after the setup, holds none of it.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_drawn_scatter_drops_what_a_quests_setup_cleared() {
        let struck = follows(&["--quest", "QU01"], 100);

        assert!(struck > 0, "no place struck");
    }

    /// Every quest begins, drawn as the game draws it, and its first
    /// frames' relays keep the drawing the world's: no test else starts
    /// all of them.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn every_quest_begins_drawn() {
        let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
        let quests = crate::quests(&install).expect("quests");

        assert!(!quests.is_empty(), "no quests");

        // The install's; a mod's quest may name its mod's functions.
        for offered in quests
            .iter()
            .filter(|o| o.origin == crate::game::load::Origin::Install)
        {
            follows(&["--quest", &offered.quest.key.to_string()], 30);
        }
    }
}
