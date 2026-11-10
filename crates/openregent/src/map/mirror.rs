//! The world's pieces as entities: one per shown agent, its components
//! copied from the step, read only by the rest.
//!
//! ```text
//!  Frame::Step  game::clock::tick ──> Snapshot (the step's Shown, the step before's places)
//!               sync: new agent ──> spawn (Seen, Was, Phase, Pin)
//!                     changed   ──> Seen, Was written only then
//!                     gone      ──> despawn, its drawing with it
//!  later        Pieces: by agent, in agent order; or Query<&Seen>, Changed<Seen>
//!               map::pieces::pieces: a drawing per mirror (DrawingOf ──> Drawing)
//! ```
//!
//! `sync` is the only writer of [`Seen`], [`Was`] and the [`Index`];
//! `map::pieces::pieces` of a mirror's [`Phase`] and [`Pin`]. With
//! `MAJESTY_CHECK_MIRROR` set, [`check`] compares the mirrors with the
//! world's [`World::shown`](regent_core::sim::world::World::shown) each drawn
//! frame.

use std::collections::{BTreeMap, HashMap};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use regent_core::base::fixed::Fixed;
use regent_core::base::switch::Switch;
use regent_core::sim::world::draw_view::Shown;

use crate::game::clock::Snapshot;
use crate::game::seen::Watch;

/// Set to check the mirrors against the world each drawn frame ([`check`]).
pub static CHECK_MIRROR: Switch = Switch::new("MAJESTY_CHECK_MIRROR");

/// A piece as the world showed it at the last step.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen(Shown);

impl Seen {
    pub const fn shown(&self) -> &Shown {
        &self.0
    }
}

/// Where a piece was a step before, if it walked there: its place, and
/// a flier's height. `None` for one new, or set there outright
/// ([`Shown::jumps`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Was {
    pub at: Option<(i32, i32)>,
    pub aloft: Option<Fixed>,
}

/// The game time a piece's set went on: where its frames count from.
/// `None` until first drawn.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Phase(pub Option<u32>);

/// The frame a piece is held on: an action's `InitAction` steps it, not
/// the clock.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pin(pub Option<u16>);

/// A drawing's mirror: despawned with it.
#[derive(Component, Clone, Copy, Debug)]
#[relationship(relationship_target = Drawing)]
pub struct DrawingOf(pub Entity);

/// A mirror's drawing, `map::pieces::pieces`'s.
#[derive(Component, Debug)]
#[relationship_target(relationship = DrawingOf, linked_spawn)]
pub struct Drawing(Entity);

impl Drawing {
    pub const fn entity(&self) -> Entity {
        self.0
    }
}

/// Each shown agent's mirror, in agent order.
#[derive(Resource, Default)]
pub struct Index(BTreeMap<u32, Entity>);

impl Index {
    /// Agent `agent`'s mirror.
    pub fn get(&self, agent: u32) -> Option<Entity> {
        self.0.get(&agent).copied()
    }
}

/// The mirrors, read by agent: what the interface reads of the pieces.
#[derive(SystemParam)]
pub struct Pieces<'w, 's> {
    index: Res<'w, Index>,
    mirrors: Query<'w, 's, (&'static Seen, &'static Was, Option<&'static Drawing>)>,
}

impl Pieces<'_, '_> {
    /// Agent `agent`'s piece.
    pub fn get(&self, agent: u32) -> Option<&Shown> {
        self.mirror(agent).map(|(seen, ..)| seen.shown())
    }

    /// Where agent `agent` was a step before.
    pub fn was(&self, agent: u32) -> Option<&Was> {
        self.mirror(agent).map(|(_, was, _)| was)
    }

    /// Agent `agent`'s drawing.
    pub fn drawn(&self, agent: u32) -> Option<Entity> {
        self.mirror(agent)
            .and_then(|(.., drawing)| drawing)
            .map(Drawing::entity)
    }

    /// Agent `agent`'s mirror entity.
    pub fn mirror_of(&self, agent: u32) -> Option<Entity> {
        self.index.get(agent)
    }

    /// The highest agent shown.
    pub fn highest(&self) -> Option<u32> {
        self.index.0.keys().next_back().copied()
    }

    pub fn contains(&self, agent: u32) -> bool {
        self.index.0.contains_key(&agent)
    }

    pub fn is_empty(&self) -> bool {
        self.index.0.is_empty()
    }

    /// Every piece, in agent order.
    pub fn iter(&self) -> impl Iterator<Item = &Shown> {
        self.index
            .0
            .values()
            .filter_map(|e| self.mirrors.get(*e).ok())
            .map(|(seen, ..)| seen.shown())
    }

    fn mirror(&self, agent: u32) -> Option<(&Seen, &Was, Option<&Drawing>)> {
        self.mirrors.get(self.index.get(agent)?).ok()
    }
}

/// Mirror the step's pieces: spawn the new, write the changed, despawn
/// the gone.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn sync(
    mut commands: Commands,
    snapshot: Res<Snapshot>,
    mut index: ResMut<Index>,
    mut mirrors: Query<(&mut Seen, &mut Was)>,
) {
    let was = |shown: &Shown| snapshot.was(shown.agent);
    let now: HashMap<u32, &Shown> = snapshot.now.iter().map(|s| (s.agent, s)).collect();

    index.0.retain(|agent, entity| {
        let keep = now.contains_key(agent);

        if !keep {
            commands.entity(*entity).despawn();
        }

        keep
    });

    for shown in &snapshot.now {
        if let Some(entity) = index.get(shown.agent) {
            if let Ok((mut seen, mut before)) = mirrors.get_mut(entity) {
                seen.set_if_neq(Seen(*shown));
                before.set_if_neq(was(shown));
            }
        } else {
            let entity = commands
                .spawn((Seen(*shown), was(shown), Phase::default(), Pin::default()))
                .id();

            index.0.insert(shown.agent, entity);
        }
    }
}

/// Whether [`check`] runs: [`CHECK_MIRROR`] set, read once.
pub fn checking() -> bool {
    CHECK_MIRROR.on()
}

/// The mirrors against the world: every shown piece mirrored as shown,
/// in agent order, and no other.
///
/// # Panics
///
/// A mirror that differs, named.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn check(watch: Watch, pieces: Pieces) {
    let world = watch.world().shown();
    let mirrored: Vec<Shown> = pieces.iter().copied().collect();

    assert_eq!(
        mirrored, world,
        "the mirrors differ from the world at frame {}",
        watch.frames
    );
}

/// The index, a game's.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Index>();
}

#[cfg(test)]
mod tests {
    use super::*;

    use regent_core::data::fourcc::FourCc;
    use regent_core::sim::world::draw_view::{Doing, Housed};

    /// Agent `agent` standing at `at`.
    fn piece(agent: u32, at: (i32, i32)) -> Shown {
        Shown {
            agent,
            code: FourCc(*b"BVr1"),
            handle: 0,
            owner: 0,
            at,
            aloft: None,
            facing: 0,
            doing: Doing::Standing,
            housed: Housed::Out,
            stride: None,
            walk_frame: None,
            look: None,
            ruined: false,
            jumps: 0,
        }
    }

    /// Mirrors changed at the last update, by agent.
    #[derive(Resource, Default)]
    struct Changes(Vec<u32>);

    fn changes(written: Query<&Seen, Changed<Seen>>, mut changes: ResMut<Changes>) {
        changes.0 = written.iter().map(|s| s.shown().agent).collect();
        changes.0.sort_unstable();
    }

    /// An app mirroring its `Snapshot`, as the game's step does.
    fn mirroring() -> App {
        let mut app = App::new();

        app.init_resource::<Snapshot>()
            .init_resource::<Index>()
            .init_resource::<Changes>()
            .add_systems(Update, (sync, changes).chain());
        app
    }

    fn show(app: &mut App, now: Vec<Shown>) {
        app.insert_resource(Snapshot::of(crate::game::clock::Steps::showing(now)));
        app.update();
    }

    /// A piece is mirrored while shown, written only as it changes, and
    /// goes with its drawing once gone.
    #[test]
    fn a_mirror_lives_while_its_piece_is_shown() {
        let mut app = mirroring();

        show(&mut app, vec![piece(1, (0, 0)), piece(2, (0, 0))]);
        assert_eq!(app.world().resource::<Changes>().0, [1, 2], "both new");

        let mirror = app.world().resource::<Index>().get(2).expect("mirrored");
        let drawing = app.world_mut().spawn(DrawingOf(mirror)).id();

        show(&mut app, vec![piece(1, (0, 0)), piece(2, (32, 0))]);
        assert_eq!(app.world().resource::<Changes>().0, [2], "the one moved");

        show(&mut app, vec![piece(1, (0, 0))]);
        assert!(app.world().resource::<Index>().get(2).is_none());
        assert!(app.world().get_entity(mirror).is_err(), "the mirror gone");
        assert!(app.world().get_entity(drawing).is_err(), "its drawing gone");
    }

    /// An app checking the mirrors of a new game's pieces, a game minute
    /// in, against its world; `shown` the step's pieces, from the world's.
    fn checking_game(shown: impl FnOnce(Vec<Shown>) -> Vec<Shown>) -> App {
        const FRAMES: u32 = 1_200;

        let (begun, _) = crate::game::begin::test_game(&[]);
        let mut sim = begun.sim;
        let mut app = mirroring();

        for _ in 0..FRAMES {
            sim.step();
        }

        app.insert_resource(Snapshot::of(crate::game::clock::Steps::showing(shown(
            sim.world().shown(),
        ))));
        app.insert_resource(crate::game::seen::Frame::of(&sim))
            .add_systems(PostUpdate, check);
        app
    }

    /// The mirrors of a game's pieces agree with its world.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_mirrors_agree_with_the_world() {
        let mut app = checking_game(|shown| shown);

        app.update();
        assert!(
            !app.world().resource::<Index>().0.is_empty(),
            "pieces shown"
        );
    }

    /// A mirror the step did not keep up with fails the check.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    #[should_panic(expected = "the mirrors differ from the world")]
    fn the_check_finds_a_mirror_astray() {
        let mut app = checking_game(|mut shown| {
            shown[0].at.0 += 1;
            shown
        });

        app.update();
    }
}
