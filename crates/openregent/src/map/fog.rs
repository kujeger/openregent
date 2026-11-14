//! Fog of war, as the iso view draws it ([`regent_core::view::map::fog`]).
//!
//! ```text
//!  capture  each piece's and effect's World::seen_level, each missile's
//!           cell's; the explored ground
//!  ground   a black veil per 16-cell chunk over cells with a corner
//!           unexplored, Gouraud from the corners; a chunk is redrawn
//!           when one of its vertices is revealed
//!  units    seen level 0: hidden, unclickable; 1 to 99: drawn in the
//!           darkened palette fog::palette names
//!  L        lifts the fog: a debug key, [enhancements] debug_keys
//!  spectator  the fog lifted, as the key lifts it (`Sim::surrender`)
//! ```
//!
//! **Inferred**: the veil multiplies the drawn ground by the corners'
//! interpolated share, where the engine interpolates the shade itself;
//! the two agree where a cell's shades do. Particles are hidden by their
//! own cell, missiles and spent impacts by theirs, without the owner's
//! word; neither is darkened.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::ecs::query::QueryItem;
use bevy::prelude::*;

use regent_core::sim::world::cheat::Cheats;
use regent_core::sim::world::{World, draw_view::Shown, reveal::Explored};
use regent_core::view::map::fog;
use regent_draw::data::Corpus;
use regent_draw::map::reel::{Layer, Role, Stepped, SteppedItem};
use regent_draw::map::things::Record;
use regent_draw::map::{Maps, SCATTER};
use regent_draw::material::Body;
use regent_draw::veil::{self, Veil};

use crate::files::config::Settings;
use crate::game::scope::Scoped;
use crate::game::seen::Watch;
use crate::map::effects::{Effects, Hung};
use crate::map::ground;
use crate::map::mirror::Pieces;
use crate::map::pieces::{Keyed, make_over};
use regent_draw::art::Sprites;
use regent_draw::art::world::Variant;

/// Registers the state the systems share; the veil's material is
/// `regent_draw`'s (`veil::Plugin`).
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.per_game::<Fog>()
            .per_game::<Veils>()
            .per_game::<Darkened>();
    }
}

/// Whether the fog is drawn: the debug key lifts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Sight {
    #[default]
    Fogged,
    Lifted,
}

impl Sight {
    const fn toggled(self) -> Sight {
        match self {
            Sight::Fogged => Sight::Lifted,
            Sight::Lifted => Sight::Fogged,
        }
    }
}

/// What the local player sees, as the last game frame left it: the sim
/// thread's [`Seen`], and the debug key's sight.
#[derive(Resource, Default)]
pub struct Fog {
    pub sight: Sight,
    seen: Arc<Seen>,
}

impl std::ops::Deref for Fog {
    type Target = Seen;

    fn deref(&self) -> &Seen {
        &self.seen
    }
}

/// What the local player sees of a world: taken on the sim thread as
/// the world changes (`seen::Taken`).
#[derive(Default)]
pub struct Seen {
    /// Seen level by agent.
    pieces: HashMap<u32, i32>,
    /// Seen level by effect handle.
    effects: HashMap<u32, i32>,
    /// Seen level by missile handle.
    missiles: HashMap<u32, i32>,
    explored: Arc<Explored>,
    /// The local player's bit in `explored` (`fog::mask`).
    mask: u8,
    /// Bumped as the explored ground changes.
    revision: u64,
    /// `Board::relaid` when taken: the ground's heights.
    relaid: u32,
}

impl Seen {
    /// What player `local` sees of `world`, its pieces `shown`: the
    /// explored ground kept, and its revision, unless it changed since
    /// these. A change is found and copied by the runs of flags the
    /// world no longer shares with the kept copy, not by a walk of all.
    #[must_use]
    pub fn after(&self, world: &World, shown: &[Shown], local: u32) -> Seen {
        let explored = world.explored();
        let mask = fog::mask(local);
        let relaid = world.board().relaid();
        let moved = explored.cells() != self.explored.cells()
            || explored.differing(&self.explored).next().is_some()
            || mask != self.mask;

        crate::perf::named("fog_moved", f64::from(u8::from(moved)));

        let (kept, revision) = if moved || relaid != self.relaid {
            (Arc::new(explored.clone()), self.revision + 1)
        } else {
            (self.explored.clone(), self.revision)
        };

        let pieces = shown
            .iter()
            .map(|s| (s.agent, world.seen_level(s.agent, mask)))
            .collect();

        // An effect on a holder is seen as its holder; one alone by its cell.
        let effects = world
            .effects()
            .iter()
            .map(|e| (e.handle, world.effect_seen(e, mask)))
            .collect();
        let missiles = world
            .flown()
            .iter()
            .map(|m| (m.handle, world.missile_seen(m, mask)))
            .collect();

        Seen {
            pieces,
            effects,
            missiles,
            explored: kept,
            mask,
            revision,
            relaid,
        }
    }
}

impl Fog {
    /// The level a thing at `level` draws at: its own, or full with the
    /// fog lifted.
    const fn shown(&self, level: i32) -> i32 {
        match self.sight {
            Sight::Fogged => level,
            Sight::Lifted => regent_core::sim::world::reveal::SEEN,
        }
    }

    /// Whether piece `agent` is hidden.
    pub fn hides(&self, agent: u32) -> bool {
        !fog::drawn(self.shown(self.pieces.get(&agent).copied().unwrap_or(0)))
    }

    /// Whether missile `handle` is hidden.
    pub fn hides_missile(&self, handle: u32) -> bool {
        !fog::drawn(self.shown(self.missiles.get(&handle).copied().unwrap_or(0)))
    }

    /// Whether a particle at `at`, cells, is hidden: by its cell.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a cell of a map is a few hundred"
    )]
    pub fn hides_at(&self, at: [f32; 3]) -> bool {
        let cell = (at[0].floor() as i32, at[1].floor() as i32);

        !fog::drawn(self.shown(self.explored.seen_at(cell, self.mask)))
    }

    /// The palette piece or effect level `level` draws in.
    const fn palette(&self, level: i32) -> usize {
        fog::palette(self.shown(level))
    }
}

/// Take what the local player sees: the sim thread took it with the
/// frame (`seen::Taken`). A spectator sees the whole map: the fog lifted.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn capture(seen: Watch, mut fog: ResMut<Fog>) {
    fog.seen = seen.taken.fog.clone();

    if seen.seat == crate::game::sim::Seat::Spectates {
        fog.sight = Sight::Lifted;
    }
}

/// L lifts the fog or draws it again, with `[enhancements] debug_keys`.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn key(
    keys: Res<ButtonInput<KeyCode>>,
    mut live: crate::input::map_keys::Live,
    settings: Res<Settings>,
    seen: Watch,
    mut fog: ResMut<Fog>,
) {
    if keys.just_pressed(KeyCode::KeyL)
        && settings.enhancements.debug_keys
        && seen.world().cheats() == Cheats::Allowed
        && live.keys()
    {
        fog.sight = fog.sight.toggled();
    }
}

/// The veil's chunks as drawn.
#[derive(Resource, Default)]
pub struct Veils {
    /// The explored ground the chunks show, and its revision.
    drawn: Arc<Explored>,
    revision: u64,
    relaid: u32,
    sight: Sight,
    chunks: HashMap<(i32, i32), Entity>,
    material: Option<Handle<Veil>>,
}

impl Veils {
    /// Every chunk forgotten, to be drawn again over another ground (a
    /// LAN resync's): their entities, to despawn.
    pub fn forget(&mut self) -> Vec<Entity> {
        self.drawn = Arc::default();
        self.chunks.drain().map(|(_, entity)| entity).collect()
    }
}

/// A veil chunk's entity.
#[derive(Component)]
pub struct Chunk;

/// Redraw the veil's chunks the explored ground changed; all of them
/// where the heights did.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn ground(
    mut commands: Commands,
    fog: Res<Fog>,
    maps: Res<Maps>,
    mut veils: ResMut<Veils>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<Veil>>,
    mut shown: Query<&mut Visibility, With<Chunk>>,
) {
    let side = fog.explored.cells();
    let fresh = veils.drawn.cells() != side || veils.relaid != fog.relaid;

    if veils.revision != fog.revision || fresh {
        let dirty = if fresh {
            veil::chunks(side).collect()
        } else {
            veil::changed(&veils.drawn, &fog.explored, fog.mask)
        };
        let material = veils
            .material
            .get_or_insert_with(|| materials.add(Veil::default()))
            .clone();
        let visibility = visibility(fog.sight);

        for at in dirty {
            if let Some(entity) = veils.chunks.remove(&at) {
                commands.entity(entity).despawn();
            }

            if let Some(mesh) = veil::mesh(&maps, &fog.explored, at, fog.mask) {
                let entity = commands
                    .spawn((
                        Chunk,
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(0.0, 0.0, veil::DEPTH),
                        visibility,
                    ))
                    .id();

                veils.chunks.insert(at, entity);
            }
        }

        veils.drawn = fog.explored.clone();
        veils.revision = fog.revision;
        veils.relaid = fog.relaid;
    }

    if veils.sight != fog.sight {
        for mut seen in &mut shown {
            *seen = visibility(fog.sight);
        }

        veils.sight = fog.sight;
    }
}

const fn visibility(sight: Sight) -> Visibility {
    match sight {
        Sight::Fogged => Visibility::Inherited,
        Sight::Lifted => Visibility::Hidden,
    }
}

/// A scatter frame hidden: its mesh, put back when seen.
#[derive(Component)]
pub struct Shrouded(Handle<Mesh>);

/// The palette each drawn piece or effect entity was last given; the
/// darkened bodies are the art service's (`art::world::Variant::Dark`),
/// its layers' [`regent_draw::map::reel::Own`].
#[derive(Resource, Default)]
pub struct Darkened {
    given: HashMap<Entity, usize>,
    /// The fog revision and sight the scatter shows.
    scatter: Option<(u64, Sight)>,
    /// The explored ground and mask the scatter shows.
    explored: Option<(Arc<Explored>, u8)>,
}

/// What [`dim`] makes with.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Stores<'w> {
    darkened: ResMut<'w, Darkened>,
    corpus: Res<'w, Corpus>,
    sprites: ResMut<'w, Sprites>,
    images: ResMut<'w, Assets<Image>>,
    bodies: ResMut<'w, Assets<Body>>,
}

/// Each piece and effect hidden or darkened by its seen level, and again
/// as its key changes (its layers given new frames). Before the tint: a
/// tinted piece draws its tint's palette, not a darkened one (sprite blit
/// takes the tint first), so its bodies go back.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn units(
    mut commands: Commands,
    fog: Res<Fog>,
    effects: Res<Effects>,
    pieces: Pieces,
    hung: Res<Hung>,
    children: Query<&Children>,
    keys: Query<Ref<Keyed>>,
    mut layers: Query<Stepped>,
    mut visible: Query<&mut Visibility, Without<Layer>>,
    mut stores: Stores,
) {
    let drawn = pieces.iter().filter_map(|s| {
        let entity = pieces.drawn(s.agent)?;
        let level = fog.pieces.get(&s.agent).copied().unwrap_or(0);
        let k = if effects.tinted(s.agent) {
            0
        } else {
            fog.palette(level)
        };

        Some((entity, k, None))
    });
    let shown = hung.drawn().map(|(handle, entity)| {
        let level = fog.effects.get(&handle).copied().unwrap_or(0);

        (
            entity,
            fog.palette(level),
            Some(fog::drawn(fog.shown(level))),
        )
    });
    let all: Vec<(Entity, usize, Option<bool>)> = drawn.chain(shown).collect();
    let live: HashSet<Entity> = all.iter().map(|(e, ..)| *e).collect();

    stores.darkened.given.retain(|e, _| live.contains(e));

    for (entity, k, seen) in all {
        // An effect's own visibility is ours: nothing else hides one.
        if let (Some(seen), Ok(mut visibility)) = (seen, visible.get_mut(entity)) {
            let want = if seen {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };

            if *visibility != want {
                *visibility = want;
            }
        }

        let rekeyed = keys.get(entity).is_ok_and(|key| key.is_changed());

        if stores.darkened.given.get(&entity) != Some(&k) || rekeyed {
            for child in children.get(entity).into_iter().flatten() {
                // Its bodies; a lift keeps its frame's.
                if let Ok(mut layer) = layers.get_mut(*child)
                    && layer.layer.role == Role::Body
                {
                    dim(&mut commands, &mut layer, k, &mut stores);
                }
            }

            stores.darkened.given.insert(entity, k);
        }
    }
}

/// What [`scatter`] reads and changes of a drawn thing.
type Strewn = (
    Entity,
    Ref<'static, Record>,
    Option<&'static Mesh3d>,
    Option<&'static Shrouded>,
    Option<Stepped>,
);

/// Every scatter frame hidden or darkened by its cell's seen level, as
/// the explored ground or the fog's sight changes, and each new one.
///
/// A reveal looks only at the frames on its cells (`by_cell`); the
/// index is built again as records come or move, which walks them all.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn scatter(
    mut commands: Commands,
    fog: Res<Fog>,
    maps: Res<Maps>,
    mut things: Query<Strewn>,
    mut gone: RemovedComponents<Record>,
    mut stores: Stores,
    mut cells: Local<Vec<Option<(i32, i32)>>>,
    mut by_cell: Local<HashMap<(i32, i32), Vec<Entity>>>,
    mut counted: Local<usize>,
) {
    let now = (fog.revision, fog.sight);
    let redo = Redo::of(&stores.darkened, &fog);
    let mut shade =
        |thing: QueryItem<Strewn>, cell| hide(&mut commands, &fog, thing, cell, &mut stores);

    // Each record's cell, by index: read again as the placed things move.
    if maps.is_changed() {
        *cells = record_cells(&maps);
    }

    // Records move only as the map is relaid, which changes `Maps`; one
    // new or gone changes the count or is read here. No per-thing scan.
    let gone = gone.read().count() > 0;
    let count = things.iter().len();

    if maps.is_changed() || gone || count != *counted {
        // Records came or moved: every frame walked, and indexed again.
        by_cell.clear();
        *counted = count;

        for thing in &mut things {
            let cell = cells.get(thing.1.index).copied().flatten();

            if thing.1.chunk == SCATTER {
                if let Some(at) = cell {
                    by_cell.entry(at).or_default().push(thing.0);
                }

                if thing.1.is_changed() || redo.covers(cell) {
                    shade(thing, cell);
                }
            }
        }
    } else {
        match &redo {
            Redo::Nothing => {}
            Redo::Cells(moved) => {
                for cell in moved {
                    for entity in by_cell.get(cell).into_iter().flatten() {
                        // A frame gone since the index was built has no entity.
                        if let Ok(thing) = things.get_mut(*entity) {
                            shade(thing, Some(*cell));
                        }
                    }
                }
            }
            Redo::All => {
                for thing in &mut things {
                    let cell = cells.get(thing.1.index).copied().flatten();

                    if thing.1.chunk == SCATTER {
                        shade(thing, cell);
                    }
                }
            }
        }
    }

    stores.darkened.scatter = Some(now);
    stores.darkened.explored = Some((fog.explored.clone(), fog.mask));
}

/// Scatter frame `thing`, on `cell`, hidden or darkened by its seen level.
fn hide(
    commands: &mut Commands,
    fog: &Fog,
    (entity, _, mesh, shrouded, layer): QueryItem<Strewn>,
    cell: Option<(i32, i32)>,
    stores: &mut Stores,
) {
    let level = cell.map_or(0, |c| fog.explored.seen_at(c, fog.mask));
    let level = fog.shown(level);

    match (fog::drawn(level), mesh, shrouded) {
        (false, Some(mesh), _) => {
            commands
                .entity(entity)
                .try_insert(Shrouded(mesh.0.clone()))
                .try_remove::<Mesh3d>();
        }
        (true, None, Some(kept)) => {
            commands
                .entity(entity)
                .try_insert(Mesh3d(kept.0.clone()))
                .try_remove::<Shrouded>();
        }
        _ => {}
    }

    // Its bodies and lifts; not its band.
    if let Some(mut layer) = layer.filter(|l| l.layer.role != Role::Band) {
        dim(commands, &mut layer, fog::palette(level), stores);
    }
}

/// Each `GRND` record's cell as [`Maps::placed`] has it, by record
/// index; `None` for a record not drawn. A place whose record a relay
/// cleared ([`ground::STRUCK`]) names none, so is left out.
pub(crate) fn record_cells(maps: &Maps) -> Vec<Option<(i32, i32)>> {
    let scatter = || {
        maps.placed
            .iter()
            .filter(|p| p.chunk == SCATTER && p.index != ground::STRUCK)
    };
    let mut cells = vec![None; scatter().map(|p| p.index + 1).max().unwrap_or(0)];

    for p in scatter() {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a cell of a map is a few hundred"
        )]
        let cell = (p.cell.x.floor() as i32, p.cell.y.floor() as i32);

        cells[p.index] = Some(cell);
    }

    cells
}

/// Which scatter cells [`scatter`] looks at again: a seen level reads
/// only its cell's four corners, so a reveal touches the cells about
/// the vertices it changed. Another player's exploring touches none.
enum Redo {
    Nothing,
    /// The cells, row by row.
    Cells(Vec<(i32, i32)>),
    All,
}

impl Redo {
    /// What changed since the scatter last took `fog`.
    fn of(darkened: &Darkened, fog: &Fog) -> Redo {
        let now = (fog.revision, fog.sight);

        match &darkened.explored {
            _ if darkened.scatter == Some(now) => Redo::Nothing,
            Some((was, mask))
                if darkened
                    .scatter
                    .is_some_and(|(_, sight)| sight == fog.sight)
                    && *mask == fog.mask
                    && was.cells() == fog.explored.cells() =>
            {
                Redo::Cells(moved(was, &fog.explored, fog.mask))
            }
            _ => Redo::All,
        }
    }

    /// Whether a thing in `cell` is looked at again; one not drawn on
    /// any cell only by [`Redo::All`].
    fn covers(&self, cell: Option<(i32, i32)>) -> bool {
        match (self, cell) {
            (Redo::Nothing, _) | (Redo::Cells(_), None) => false,
            (Redo::Cells(moved), Some((x, y))) => moved
                .binary_search_by_key(&(y, x), |&(x, y)| (y, x))
                .is_ok(),
            (Redo::All, _) => true,
        }
    }
}

/// Each cell, row by row, with a corner whose flags `mask` sees changed
/// from `was` to `now`: the cells whose seen level may differ.
fn moved(was: &Explored, now: &Explored, mask: u8) -> Vec<(i32, i32)> {
    let cells = now.cells();
    let mut moved: Vec<(i32, i32)> = veil::vertices(was, now, |a, b| a & mask != b & mask)
        .flat_map(veil::corners)
        .filter(|(x, y)| (0..cells).contains(x) && (0..cells).contains(y))
        .collect();

    moved.sort_unstable_by_key(|&(x, y)| (y, x));
    moved.dedup();
    moved
}

/// Layer `layer`'s bodies in palette `k`: its frames' darkened, or its
/// frames' own again for 0.
fn dim(commands: &mut Commands, layer: &mut SteppedItem, k: usize, stores: &mut Stores) {
    match k {
        0 => make_over(commands, layer, None),
        _ => make_over(
            commands,
            layer,
            Some(&mut |source: &Handle<Body>| darkened(source, k, stores)),
        ),
    }
}

/// Body `source` in palette `k`, made once: the same indices and pass,
/// its palette's colours [`fog::darkened`], their alphas kept.
fn darkened(source: &Handle<Body>, k: usize, stores: &mut Stores) -> Option<Handle<Body>> {
    let made = (&mut *stores.images, &mut *stores.bodies);

    stores
        .sprites
        .variant(&stores.corpus, made, source, Variant::Dark(k))
}

/// The veil's chunks that differ from the fog drawn afresh over the
/// drawn ground, or are missing, or left over: a test's.
#[cfg(test)]
pub fn stale_veils(world: &bevy::prelude::World) -> Vec<(i32, i32)> {
    let fog = world.resource::<Fog>();
    let maps = world.resource::<Maps>();
    let veils = world.resource::<Veils>();
    let meshes = world.resource::<Assets<Mesh>>();

    // A mesh's corners and triangles.
    let shape = |mesh: &Mesh| {
        let corners = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .map(<[_]>::to_vec);
        let triangles = mesh.indices().map(|i| i.iter().collect::<Vec<_>>());

        (corners, triangles)
    };
    let wanted: HashMap<(i32, i32), Mesh> = veil::chunks(fog.explored.cells())
        .filter_map(|at| veil::mesh(maps, &fog.explored, at, fog.mask).map(|m| (at, m)))
        .collect();
    let drawn = |at: &(i32, i32)| {
        let entity = veils.chunks.get(at)?;

        meshes.get(&world.get::<Mesh3d>(*entity)?.0)
    };
    let mut stale: Vec<(i32, i32)> = wanted
        .keys()
        .chain(veils.chunks.keys())
        .filter(|at| drawn(at).map(shape) != wanted.get(at).map(shape))
        .copied()
        .collect();

    stale.sort_unstable();
    stale.dedup();
    stale
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reveal about (16, 3) sets vertices (16, 3) to (17, 4): the nine
    /// cells about them alone are looked at again; another player's
    /// reveal moves none.
    #[test]
    fn a_reveal_redoes_the_scatter_of_the_cells_about_it() {
        let mask = fog::mask(regent_core::sim::world::CROWN);
        let was = Explored::blank(64);
        let mut now = was.clone();
        let mut theirs = was.clone();

        now.disc((16, 3), mask, 1);
        theirs.disc((16, 3), mask << 1, 1);

        let cells = moved(&was, &now, mask);

        assert_eq!(
            cells,
            [
                (15, 2),
                (16, 2),
                (17, 2),
                (15, 3),
                (16, 3),
                (17, 3),
                (15, 4),
                (16, 4),
                (17, 4)
            ]
        );
        assert_eq!(moved(&was, &theirs, mask).len(), 0);

        // Each moved cell's level is what a full pass reads; the rest kept.
        let redo = Redo::Cells(cells);

        for y in 0..64 {
            for x in 0..64 {
                let same = was.seen_at((x, y), mask) == now.seen_at((x, y), mask);

                assert!(same || redo.covers(Some((x, y))), "({x}, {y})");
            }
        }
        assert!(!redo.covers(None));
        assert!(!redo.covers(Some((-1, 2))));
    }

    /// A reveal across two of [`Explored::differing`]'s runs, 1,024
    /// flags each, moves the cells about every vertex it changed: vertices
    /// (48, 15) and (49, 15) are flags 1,023 and 1,024 of a 64-cell map.
    #[test]
    fn a_reveal_across_blocks_moves_every_cell_about_it() {
        let mask = fog::mask(regent_core::sim::world::CROWN);
        let was = Explored::blank(64);
        let mut now = was.clone();

        now.disc((48, 15), mask, 1);

        let mut cells: Vec<(i32, i32)> = (0..64 * 64)
            .map(|i| (i % 64, i / 64))
            .filter(|&(x, y)| {
                [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)]
                    .iter()
                    .any(|(vx, vy)| {
                        let i = usize::try_from(vy * 65 + vx).unwrap_or(0);

                        was.flag(i).map(|f| f & mask) != now.flag(i).map(|f| f & mask)
                    })
            })
            .collect();

        cells.sort_unstable_by_key(|&(x, y)| (y, x));
        assert_ne!(cells.len(), 0);
        assert_eq!(moved(&was, &now, mask), cells);
    }

    /// A place struck by a relay (its record cleared, `ground::STRUCK`)
    /// gets no cell; the rest keep theirs. It used to overflow the length.
    #[test]
    fn a_struck_scatter_place_gets_no_cell() {
        use regent_draw::map::Placed;

        let at = |index, x| Placed {
            chunk: SCATTER,
            index,
            cell: Vec2::new(x, 2.5),
            height: 0.0,
        };
        let mut maps = Maps::with(Vec::new());

        maps.placed = vec![at(1, 4.5), at(crate::map::ground::STRUCK, 7.5)];

        assert_eq!(record_cells(&maps), [None, Some((4, 2))]);
    }
}
