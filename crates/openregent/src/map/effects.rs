//! Effects over the pieces: each effector hung on a unit and each spent
//! missile's impact set (`World::effects`), and the draw effects that
//! tint a unit or make it see-through (`World::tints`).
//!
//! ```text
//!  capture  World::effects, World::tints ──> Effects
//!  draw     one entity per effect, its set playing from `since`:
//!           held, at its holder's drawn place plus its offset projected
//!           (height lifts it); else where it stands
//!  pool     an ended effect's drawing parked, out of every per-frame
//!           pass; the next effect drawing the same takes it back
//!  tint     a tinted piece's bodies swapped for its draw effects':
//!           palette to the tint's nearest, or see-through glass
//! ```
//!
//! Depth follows the engine's draw list, ordered by board y:
//! an overlay's y is its holder's plus its offset,
//! one 65,536th nearer, so it draws over its holder.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;

use regent_core::base::fixed::{self, Fixed};
use regent_core::data::fourcc::FourCc;
use regent_core::sim::movement::action::Look;
use regent_core::sim::movement::action::frame_ms;
use regent_core::sim::world::{draw::Tint, effectors::Effect, flag::MESSAGE_FLAG};
use regent_core::view::map::amount;
use regent_draw::data::Corpus;
use regent_draw::map::Maps;
use regent_draw::map::reel::{Cycle, Reel};
use regent_draw::material::{Band, Body};

use crate::game::scope::{Leave, Scoped};
use crate::game::seen::Watch;
use crate::map::mirror::Pieces;
use crate::map::pieces::{self, Depth, Key, LAYER_STEP, Lifts, Piece};
use crate::map::tie::{Class, Tied, Who};
use crate::map::words::{self, GOLD_FOOT, Tie, Words};
use crate::menu::Screen;
use regent_draw::art::world::Variant;
use regent_draw::art::{Assets2, Sprites};

/// Most drawings of one [`Mold`] kept parked: past the most effects of
/// one mold that start in a drawn frame of the battle case
/// (`assets/scenes/battle.toml`): 78 `Got_Gold` at `--speed 4`, 62 at 1x.
const MOLD: usize = 80;

/// Most effect drawings kept parked in all ([`Pool`]): the battle case's
/// six molds peak at about 200 under [`MOLD`]; a bound on many molds.
const POOL: usize = 256;

/// How far an effect level with its holder draws in front of it: past
/// the holder's layers, at most six.
const AHEAD: f32 = 7.0 * LAYER_STEP;

/// An amount's foot below its anchor plus nudge, pixels.
#[allow(clippy::cast_precision_loss, reason = "a few pixels")]
const FOOT: f32 = GOLD_FOOT as f32;

/// The resources the systems below share, a game's.
pub fn plugin(app: &mut App) {
    app.per_game::<Effects>()
        .per_game::<Hung>()
        .per_game::<Pool>()
        .add_systems(OnExit(Screen::Playing), free.in_set(Leave::Take))
        .per_game::<Tinted>();
}

/// What the world shows of effects, as the last game frame left it: the
/// sim thread's [`Seen`].
#[derive(Resource, Default)]
pub struct Effects(Arc<Seen>);

impl std::ops::Deref for Effects {
    type Target = Seen;

    fn deref(&self) -> &Seen {
        &self.0
    }
}

/// What a world shows of effects: taken on the sim thread as the world
/// changes (`seen::Taken`).
#[derive(Default)]
pub struct Seen {
    now: Vec<Effect>,
    tints: HashMap<u32, Tint>,
}

impl Seen {
    /// `world`'s effects and draw effects.
    pub fn of(world: &regent_core::sim::world::World) -> Seen {
        Seen {
            now: world.effects(),
            tints: world.tints().collect(),
        }
    }

    /// Whether the world gives piece `agent` draw effects.
    pub fn tinted(&self, agent: u32) -> bool {
        self.tints.contains_key(&agent)
    }

    /// Whether effect `handle` is a message flag, which the pick takes.
    pub fn message(&self, handle: u32) -> bool {
        self.now
            .iter()
            .any(|e| e.handle == handle && e.code == MESSAGE_FLAG)
    }
}

/// Take the world's effects and draw effects: the sim thread took them
/// with the frame (`seen::Taken`).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn capture(seen: Watch, mut effects: ResMut<Effects>) {
    effects.0 = seen.taken.effects.clone();
}

/// The entity drawing each effect, by handle, what it draws, from when
/// and its value.
#[derive(Resource, Default)]
pub struct Hung(HashMap<u32, (Entity, Key, Look, Option<i32>)>);

impl Hung {
    /// The entity drawing `handle`.
    pub fn get(&self, handle: u32) -> Option<Entity> {
        self.0.get(&handle).map(|(entity, ..)| *entity)
    }

    /// Each effect drawn: its handle and entity.
    pub fn drawn(&self) -> impl Iterator<Item = (u32, Entity)> + '_ {
        self.0
            .iter()
            .map(|(handle, (entity, ..))| (*handle, *entity))
    }
}

/// An effect's entity.
#[derive(Component)]
pub struct EffectDrawn;

/// What an effect's drawing is made of: its key, whether its frames are
/// held, and whether it shows an amount. Drawings of one mold differ only
/// in place, start and amount.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Mold {
    key: Key,
    held: bool,
    valued: bool,
}

impl Mold {
    /// The mold of a drawing of `key`, looking `look`, with `value`.
    const fn of(key: Key, look: Look, value: Option<i32>) -> Mold {
        Mold {
            key,
            held: matches!(look, Look::Frame { .. }),
            valued: value.is_some(),
        }
    }
}

/// Ended effects' drawings, parked by mold for the next effect drawing
/// the same: a battle's gold share starts dozens at once. At most
/// [`POOL`]; freed with the game.
#[derive(Resource, Default)]
pub struct Pool {
    parked: HashMap<Mold, Vec<Entity>>,
    count: usize,
}

impl Pool {
    /// A parked drawing of `mold`, taken.
    fn take(&mut self, mold: Mold) -> Option<Entity> {
        let taken = self.parked.get_mut(&mold).and_then(Vec::pop);

        if taken.is_some() {
            self.count -= 1;
        }

        taken
    }

    /// `entity`, of `mold`, kept; given back when the pool or the mold is
    /// full.
    fn keep(&mut self, mold: Mold, entity: Entity) -> Option<Entity> {
        let same = self.parked.entry(mold).or_default();

        if same.len() < MOLD && self.count < POOL {
            same.push(entity);
            self.count += 1;
            None
        } else {
            Some(entity)
        }
    }
}

/// An ended effect's drawing `entity`, of `mold`: parked, or despawned
/// when the pool is full. Parked, it and its parts are `Disabled`: out
/// of every query, so of every per-frame pass (the reel, Bevy's change
/// scans, visibility and extraction, the words' paint).
fn end(commands: &mut Commands, pool: &mut Pool, entity: Entity, mold: Mold) {
    match pool.keep(mold, entity) {
        Some(full) => {
            commands.entity(full).despawn();
        }
        None => {
            commands
                .entity(entity)
                .insert_recursive::<Children>(Disabled);
        }
    }
}

/// The parked drawings despawned as the game ends: out of every query,
/// the teardown (`draw::uninstall`) does not see them.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn free(mut commands: Commands, pool: Res<Pool>) {
    for entity in pool.parked.values().flatten() {
        commands.entity(*entity).despawn();
    }
}

/// Put each effect where the world has it, in the set it plays.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(
    clippy::too_many_lines,
    reason = "an effect's place and drawing are set in one pass"
)]
pub fn draw(
    mut commands: Commands,
    effects: Res<Effects>,
    pieces: Pieces,
    maps: Res<Maps>,
    corpus: Res<Corpus>,
    reel: Res<Reel>,
    holders: Query<&Transform, (With<Piece>, Without<EffectDrawn>)>,
    mut drawn: Query<(&mut Transform, &Tied), (With<EffectDrawn>, Without<Piece>)>,
    mut lifts: ResMut<Lifts>,
    mut sprites: ResMut<Sprites>,
    mut hung: ResMut<Hung>,
    mut pool: ResMut<Pool>,
    mut stores: Assets2,
) {
    let depth = Depth::of(&maps);
    // An effect held with its holder is not drawn.
    let shown = || effects.now.iter().filter(|e| !e.held);
    let alive: BTreeSet<u32> = shown().map(|e| e.handle).collect();

    // Gone from the world: gone from the screen.
    hung.0.retain(|handle, (entity, key, look, value)| {
        let keep = alive.contains(handle);

        if !keep {
            end(
                &mut commands,
                &mut pool,
                *entity,
                Mold::of(*key, *look, *value),
            );
        }

        keep
    });

    for effect in shown() {
        let holder = pieces
            .drawn(effect.holder)
            .and_then(|entity| holders.get(entity).ok())
            .zip(
                pieces
                    .get(effect.holder)
                    .map(|s| crate::ui::tracking::cells(s.at)),
            );
        let Some(key) = key(&corpus, effect) else {
            continue;
        };
        let lift = lifts.lift(&corpus, key.0, key.1);
        let place = match (effect.holder, holder) {
            (0, _) => Some(standing(&maps, &depth, effect.at, lift)),
            (_, Some((t, at))) => {
                Some(hanging(&maps, &depth, (effect.at, lift), t.translation, at))
            }
            _ => None,
        };
        let Some(place) = place else {
            continue;
        };
        match hung.0.get(&effect.handle) {
            Some((entity, was, look, value))
                if *was == key && *look == effect.look && *value == effect.value =>
            {
                if let Ok((mut transform, tied)) = drawn.get_mut(*entity) {
                    transform.set_if_neq(Transform {
                        translation: place.with_z(tied.keep(place.z)),
                        ..*transform
                    });
                }
            }
            old => {
                if let Some((entity, was, look, value)) = old {
                    end(
                        &mut commands,
                        &mut pool,
                        *entity,
                        Mold::of(*was, *look, *value),
                    );
                }

                let timing = match effect.look {
                    Look::Plays { since, .. } => pieces::Timing {
                        stride: None,
                        from: since,
                        pin: None,
                    },
                    Look::Frame { frame, .. } => pieces::Timing {
                        stride: None,
                        from: reel.ms,
                        pin: u16::try_from(frame).ok(),
                    },
                };
                let who = Who(Class::Effect, effect.handle);
                let start = Start {
                    who,
                    place,
                    timing,
                    value: effect.value,
                    ms: reel.ms,
                };
                // A parked drawing of the same mold taken back; else made.
                let entity =
                    if let Some(entity) = pool.take(Mold::of(key, effect.look, effect.value)) {
                        commands.queue(move |world: &mut World| revive(world, entity, start));
                        entity
                    } else {
                        let entity = pieces::spawn(
                            &mut commands,
                            &corpus,
                            &mut sprites,
                            &mut stores,
                            &reel,
                            key,
                            // Its own place for ground: no effect set has a
                            // second pass (**Corpus**).
                            (None, place, place.truncate(), Visibility::Inherited, timing),
                        );

                        commands
                            .entity(entity)
                            .insert((EffectDrawn, Tied::new(who)));

                        if let Some(value) = effect.value {
                            amount(
                                &mut commands,
                                (&corpus, &reel),
                                (entity, key),
                                value,
                                timing,
                            );
                        }

                        entity
                    };

                hung.0
                    .insert(effect.handle, (entity, key, effect.look, effect.value));
            }
        }
    }
}

/// Every part under drawing `root`, its children's children too.
fn parts(world: &World, root: Entity) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut next = vec![root];

    while let Some(entity) = next.pop() {
        for child in world.get::<Children>(entity).into_iter().flatten() {
            found.push(*child);
            next.push(*child);
        }
    }

    found
}

/// What a parked drawing takes up again: the effect it now draws (its
/// order in a tie), its place, its frames' start, its amount, and the
/// game time it is shown at.
#[derive(Clone, Copy)]
struct Start {
    who: Who,
    place: Vec3,
    timing: pieces::Timing,
    value: Option<i32>,
    ms: u32,
}

/// Parked drawing `root` shown as [`pieces::spawn`] and [`amount`] draw
/// one anew: at its place, each frame stepping from its start, its
/// amount's text.
fn revive(world: &mut World, root: Entity, start: Start) {
    let text = start.value.map(amount::text);

    for entity in std::iter::once(root).chain(parts(world, root)) {
        let Ok(mut part) = world.get_entity_mut(entity) else {
            continue;
        };

        part.remove::<Disabled>();

        // Its frame from the new start, shown if up, as `pieces::spawn`.
        if let Some(cycle) = part.get::<Cycle>().copied() {
            let cycle = restart(cycle, start.timing);
            let shows = if cycle.showing(start.ms) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };

            part.insert((cycle, shows));
        }

        if let (Some(text), Some(mut words)) = (&text, part.get_mut::<Words>()) {
            words.text.clone_from(text);
        }
    }

    if let Ok(mut drawing) = world.get_entity_mut(root) {
        // Ordered among ties as its new effect, from its maker's depth.
        drawing.insert((
            Transform::from_translation(start.place),
            Visibility::Inherited,
            Tied::new(start.who),
        ));
    }
}

/// `cycle`'s frame stepping from `timing`'s start, as [`pieces::spawn`]
/// makes it. A mold keeps whether frames are held, so an unheld cycle's
/// start and rate are the ones it was made with.
const fn restart(cycle: Cycle, timing: pieces::Timing) -> Cycle {
    match timing.pin {
        Some(frame) => pieces::held(cycle.index(), cycle.frames(), frame, timing.from),
        None => cycle.from(timing.from),
    }
}

/// An effect's value drawn as text (`view::map::amount`): a layer of its
/// frames, shown at view zoom 1.0 alone.
#[derive(Component)]
pub struct Amount;

/// `value` drawn under effect entity `parent`, drawing `key`: each
/// frame of reference kind text in the set's layers centred at the
/// anchor plus its nudge, stepping as `timing` says;
/// its words painted by egui (`words`).
fn amount(
    commands: &mut Commands,
    (corpus, reel): (&Corpus, &Reel),
    (parent, key): (Entity, Key),
    value: i32,
    timing: pieces::Timing,
) {
    let (code, set, slot) = key;
    let Some(unit) = corpus.unit(code) else {
        return;
    };
    let words = Words {
        text: amount::text(value),
        look: words::GOLD,
        tie: Tie::default(),
    };

    for layer in unit.stack(set) {
        let Some(pose) = layer.poses.get(slot).or_else(|| layer.poses.first()) else {
            continue;
        };
        let base = if layer.variant > 0 {
            Vec2::new(f32::from(pose.x), -f32::from(pose.y))
        } else {
            Vec2::ZERO
        };
        let ms = frame_ms(layer.playback);

        for stream in 0..pose.streams.max(1) {
            let texts = pose.texts(stream);

            if texts.iter().all(Option::is_none) {
                continue;
            }

            let frames = u16::try_from(texts.len()).unwrap_or(u16::MAX);
            let shown = commands
                .spawn((
                    Amount,
                    Transform::from_translation(base.extend(f32::from(layer.variant) * LAYER_STEP)),
                    Visibility::Inherited,
                    ChildOf(parent),
                ))
                .id();

            for (index, nudge) in texts.iter().enumerate() {
                let Some((nx, ny)) = nudge else { continue };
                let index = u16::try_from(index).unwrap_or(u16::MAX);
                let cycle = match timing.pin {
                    Some(frame) => pieces::held(index, frames, frame, timing.from),
                    None => Cycle::new(index, frames, 0, ms).from(timing.from),
                };
                // The digits' foot, below the anchor plus the nudge.
                let at = Vec2::new(f32::from(*nx), -(f32::from(*ny) + FOOT));
                let seen = if cycle.showing(reel.ms) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };

                commands.spawn((
                    words.clone(),
                    Transform::from_translation(at.extend(0.0)),
                    seen,
                    cycle,
                    ChildOf(shown),
                ));
            }
        }
    }
}

/// What an effect draws: its descriptor, set, and the slot its facing
/// picks. `None` where the art lacks the set.
fn key(corpus: &Corpus, effect: &Effect) -> Option<Key> {
    let unit = corpus.unit(effect.code)?;
    let set = pieces::low(effect.look.set());
    let first = unit.stack(set).next()?;

    Some((effect.code, set, pieces::slot(&unit, first, effect.facing)))
}

/// 16.16 cells as cells.
fn fixed(at: [Fixed; 3]) -> [f32; 3] {
    at.map(fixed::to_f32)
}

/// Where an effect standing at `at`, 16.16 cells and height, draws,
/// sorting `lift` cells nearer.
fn standing(maps: &Maps, depth: &Depth, at: [Fixed; 3], lift: f32) -> Vec3 {
    let [x, y, z] = fixed(at);
    let cell = Vec2::new(x, y);

    maps.aloft_at(cell, z).extend(depth.at(y + lift))
}

/// Where a piece `offset` from a holder drawn at `drawn`, standing on
/// cell `at`, draws: the offset projected, its height lifting it; sorted
/// at its own y plus its set's `lift` cells, in
/// front of the holder when level with it in the draw order and nearer.
pub(crate) fn hanging(
    maps: &Maps,
    depth: &Depth,
    (offset, lift): ([Fixed; 3], f32),
    drawn: Vec3,
    at: Vec2,
) -> Vec3 {
    let [dx, dy, dz] = fixed(offset);
    let shift = maps.aloft_at(Vec2::new(dx, dy), dz) - maps.aloft_at(Vec2::ZERO, 0.0);
    let (own, base) = (depth.at(at.y + dy + lift), depth.at(at.y));

    #[allow(clippy::float_cmp, reason = "one rank gives one depth exactly")]
    let level = own == base;
    let z = match (level, dy + lift >= 0.0) {
        (false, _) => own,
        (true, true) => drawn.z + AHEAD,
        (true, false) => drawn.z - AHEAD,
    };

    (drawn.truncate() + shift).extend(z)
}

/// Each tinted piece's entity and draw effects, by agent; the bodies
/// are the art service's (`art::world::Variant::Tint`).
#[derive(Resource, Default)]
pub struct Tinted {
    on: HashMap<u32, (Entity, Tint)>,
}

/// A drawn frame's change to the tinted pieces ([`Tinted::turn`]).
#[derive(Debug, PartialEq, Eq)]
struct Turn {
    /// Drawings to despawn: their draw effects changed or cleared.
    gone: Vec<Entity>,
    /// Drawings to tint, with their draw effects.
    fresh: Vec<(Entity, Tint)>,
}

impl Tinted {
    /// The drawings to despawn and to tint for draw effects `tints`,
    /// each agent's drawing by `drawn`; the tinted ones remembered.
    fn turn(&mut self, tints: &HashMap<u32, Tint>, drawn: impl Fn(u32) -> Option<Entity>) -> Turn {
        // Changed, cleared or redrawn: forget it; drawn anew where still on.
        let stale: Vec<(u32, Entity, bool)> = self
            .on
            .iter()
            .filter_map(|(agent, (entity, t))| {
                let now = drawn(*agent);
                let same = tints.get(agent) == Some(t);

                (!same || now != Some(*entity)).then_some((*agent, *entity, now == Some(*entity)))
            })
            .collect();
        let mut gone = Vec::new();

        for (agent, entity, standing) in stale {
            self.on.remove(&agent);

            if standing {
                gone.push(entity);
            }
        }

        let mut fresh = Vec::new();

        // Not one despawned above: its bodies go with it, so a lift made
        // on one would be on a gone entity. Its new drawing comes next.
        for (&agent, &t) in tints {
            if let Some(entity) = drawn(agent)
                && !self.on.contains_key(&agent)
                && !gone.contains(&entity)
            {
                fresh.push((entity, t));
                self.on.insert(agent, (entity, t));
            }
        }

        Turn { gone, fresh }
    }
}

/// Swap a tinted piece's bodies for its draw effects'; one whose effects
/// change or clear is drawn anew, plain, by `map::pieces::pieces`.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn tint(
    mut commands: Commands,
    effects: Res<Effects>,
    corpus: Res<Corpus>,
    mut sprites: ResMut<Sprites>,
    pieces: Pieces,
    mut tinted: ResMut<Tinted>,
    children: Query<&Children>,
    mut bodies_on: Query<(&mut MeshMaterial3d<Body>, &Mesh3d)>,
    bands_on: Query<(), With<MeshMaterial3d<Band>>>,
    mut images: ResMut<Assets<Image>>,
    mut bodies: ResMut<Assets<Body>>,
) {
    let turn = tinted.turn(&effects.tints, |agent| pieces.drawn(agent));

    for entity in turn.gone {
        commands.entity(entity).despawn();
    }

    for (entity, t) in turn.fresh {
        for child in children.get(entity).into_iter().flatten() {
            if let Ok((mut material, quad)) = bodies_on.get_mut(*child) {
                let stores = (&mut *images, &mut *bodies);
                let made = sprites.variant(&corpus, stores, &material.0, Variant::Tint(t));

                // See-through: lifting where it is drawn as now.
                if let (Some(glass), Some(_)) = (&made, t.transparency) {
                    let made = (&mut *images, &mut *bodies);

                    pieces::relift(
                        &mut commands,
                        (*child, quad),
                        glass,
                        (&mut sprites, &corpus, made),
                    );
                }

                if let Some(handle) = made {
                    material.0 = handle;
                }
            } else if t.transparency.is_some() && bands_on.contains(*child) {
                // Mode 5 draws nothing from 247: no shadow.
                commands.entity(*child).despawn();
            }
        }
    }
}

/// What headless play would have drawn of effects: each effect by code,
/// the units tinted, and speed trails' clones, made and standing.
#[derive(Default)]
pub struct Tally {
    effects: BTreeSet<(u32, FourCc)>,
    tinted: BTreeSet<u32>,
    clones: BTreeSet<u32>,
    standing: usize,
}

impl Tally {
    /// Count what game frame `world` shows.
    pub fn add(&mut self, world: &regent_core::sim::world::World) {
        self.effects
            .extend(world.effects().iter().map(|e| (e.handle, e.code)));
        self.tinted.extend(world.tints().map(|(id, _)| id));
        self.clones.extend(world.clones().iter().map(|c| c.handle));
        self.standing = world.clones().len();
    }

    /// One line, then the effects by code, most first.
    pub fn print(&self) {
        let mut codes: HashMap<FourCc, usize> = HashMap::new();

        for (_, code) in &self.effects {
            *codes.entry(*code).or_default() += 1;
        }

        let mut codes: Vec<(FourCc, usize)> = codes.into_iter().collect();

        codes.sort_by_key(|(code, n)| (std::cmp::Reverse(*n), *code));

        println!(
            "{} effects drawn, {} units tinted",
            self.effects.len(),
            self.tinted.len()
        );

        for (code, n) in codes {
            println!("  {n:>8}  {code}");
        }

        println!(
            "speed trails: {} clones made, {} standing",
            self.clones.len(),
            self.standing
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp, reason = "exact binary fractions")]
    fn a_cell_is_thirty_two_32nds() {
        assert_eq!(crate::ui::tracking::cells((64, 16)), Vec2::new(2.0, 0.5));
        assert_eq!(
            fixed([fixed::ONE, fixed::ONE / 2, Fixed::ZERO]),
            [1.0, 0.5, 0.0]
        );
    }

    /// Frames in [`drawing`]'s stream.
    const FRAMES: u16 = 4;

    /// Game ms each of [`drawing`]'s frames shows.
    const FRAME_MS: u32 = 100;

    /// An effect drawing as `pieces::spawn` and `amount` make one, from
    /// game time `from`: [`FRAMES`] frames, each with a band, and an
    /// amount's text on each frame. Its root, frames and texts.
    fn drawing(world: &mut World, from: u32) -> (Entity, Vec<Entity>, Vec<Entity>) {
        let root = world
            .spawn((
                Transform::default(),
                Visibility::Inherited,
                EffectDrawn,
                Tied::new(Who(Class::Effect, 1)),
            ))
            .id();
        let shown = world
            .spawn((
                Amount,
                Transform::default(),
                Visibility::Inherited,
                ChildOf(root),
            ))
            .id();
        let cycle = |index| Cycle::new(index, FRAMES, 0, FRAME_MS).from(from);
        let words = Words {
            text: amount::text(10),
            look: words::GOLD,
            tie: Tie::default(),
        };
        let frames = (0..FRAMES)
            .flat_map(|i| [i, i])
            .map(|i| {
                let part = (Transform::default(), Visibility::Hidden, cycle(i));

                world.spawn((part, ChildOf(root))).id()
            })
            .collect();
        let texts = (0..FRAMES)
            .map(|i| {
                let part = (Transform::default(), Visibility::Hidden, cycle(i));

                world.spawn((part, words.clone(), ChildOf(shown))).id()
            })
            .collect();

        (root, frames, texts)
    }

    /// The frame index each of `parts` shows, if any.
    fn showing(world: &World, parts: &[Entity]) -> Vec<u16> {
        parts
            .iter()
            .filter(|p| world.get::<Visibility>(**p) == Some(&Visibility::Inherited))
            .filter_map(|p| world.get::<Cycle>(*p).map(|c| c.index()))
            .collect()
    }

    /// [`drawing`]'s mold.
    fn mold(slot: usize) -> Mold {
        Mold::of(
            (FourCc(*b"Gold"), 1, slot),
            Look::Plays { set: 1, since: 0 },
            Some(10),
        )
    }

    /// `world` with a pool, after drawing `root` of `mold` ended.
    fn ended(world: &mut World, root: Entity, mold: Mold) {
        use bevy::ecs::system::RunSystemOnce;

        world.init_resource::<Pool>();
        world
            .run_system_once(move |mut commands: Commands, mut pool: ResMut<Pool>| {
                end(&mut commands, &mut pool, root, mold);
            })
            .expect("ended");
    }

    /// `world`'s drawing of `mold` taken from the pool and shown from
    /// `start`, as `draw` does.
    fn reused(world: &mut World, mold: Mold, start: Start) -> Option<Entity> {
        let taken = world.resource_mut::<Pool>().take(mold);

        if let Some(root) = taken {
            revive(world, root, start);
        }

        taken
    }

    /// A parked drawing is in no query: not drawn, and in no per-frame
    /// pass (the reel's `Cycle`, Bevy's visibility, change scans and
    /// extraction, the words' paint).
    #[test]
    fn a_parked_drawing_is_invisible_and_out_of_every_pass() {
        let mut world = World::new();
        let (root, frames, texts) = drawing(&mut world, 0);
        let mut cycled = world.query::<(&Cycle, &Visibility)>();
        let mut placed = world.query::<&Transform>();
        let mut painted = world.query::<&Words>();
        let mut parked = world.query_filtered::<Entity, With<Disabled>>();
        let all = frames.len() + texts.len() + 2;

        assert_eq!(placed.iter(&world).count(), all);

        ended(&mut world, root, mold(0));

        assert_eq!(cycled.iter(&world).count(), 0);
        assert_eq!(placed.iter(&world).count(), 0);
        assert_eq!(painted.iter(&world).count(), 0);
        assert_eq!(parked.iter(&world).count(), all);

        // Nor in a tie's run: a drawing at its depth keeps its own.
        let other = tied(&mut world, 2, 0.0);

        untied(&mut world);
        assert_eq!(z(&world, other), 0.0);
    }

    /// A drawing `who`, of effect handle `handle`, at depth `z`.
    fn tied(world: &mut World, handle: u32, z: f32) -> Entity {
        world
            .spawn((
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Inherited,
                Tied::new(Who(Class::Effect, handle)),
            ))
            .id()
    }

    /// `crate::map::tie::untie` run once over `world`.
    fn untied(world: &mut World) {
        use bevy::ecs::system::RunSystemOnce;

        world
            .run_system_once(crate::map::tie::untie)
            .expect("untied");
    }

    /// The depth `entity` draws at.
    fn z(world: &World, entity: Entity) -> f32 {
        world.get::<Transform>(entity).expect("drawn").translation.z
    }

    /// A drawing taken from the pool ties as its new effect: drawn
    /// behind a higher handle as the old one, in front of it as a
    /// higher one still.
    #[test]
    fn a_reused_effect_ties_as_its_new_effect() {
        let mut world = World::new();
        let (root, ..) = drawing(&mut world, 0);
        let other = tied(&mut world, 3, 0.0);
        let start = Start {
            who: Who(Class::Effect, 5),
            place: Vec3::ZERO,
            timing: pieces::Timing {
                stride: None,
                from: 0,
                pin: None,
            },
            value: Some(10),
            ms: 0,
        };

        untied(&mut world);
        assert!(z(&world, root) < z(&world, other), "handle 1 behind 3");

        ended(&mut world, root, mold(0));
        assert_eq!(reused(&mut world, mold(0), start), Some(root));
        untied(&mut world);
        assert!(z(&world, root) > z(&world, other), "handle 5 before 3");
    }

    /// A drawing taken from the pool starts on frame 0 at its new start,
    /// at its new place, showing its new amount; its texts too.
    #[test]
    fn a_reused_effect_starts_on_frame_0_at_its_new_place() {
        const LATER: u32 = 12_345;

        let mut world = World::new();
        let (root, frames, texts) = drawing(&mut world, 0);
        let place = Vec3::new(64.0, -32.0, 0.5);
        let start = Start {
            who: Who(Class::Effect, 2),
            place,
            timing: pieces::Timing {
                stride: None,
                from: LATER,
                pin: None,
            },
            value: Some(250),
            ms: LATER,
        };

        ended(&mut world, root, mold(0));
        assert_eq!(reused(&mut world, mold(0), start), Some(root));

        let mut painted = world.query::<&Words>();
        let mut parked = world.query_filtered::<Entity, With<Disabled>>();

        assert_eq!(
            world.get::<Transform>(root).map(|t| t.translation),
            Some(place)
        );
        assert_eq!(world.get::<Visibility>(root), Some(&Visibility::Inherited));
        assert_eq!(showing(&world, &frames), [0, 0], "a body and its band");
        assert_eq!(showing(&world, &texts), [0]);
        assert_eq!(painted.iter(&world).count(), texts.len());
        assert!(painted.iter(&world).all(|w| w.text == "250"));
        assert_eq!(parked.iter(&world).count(), 0);

        // And it steps at its own rate from there.
        let later = |p: &&Entity| {
            world
                .get::<Cycle>(**p)
                .is_some_and(|c| c.showing(LATER + FRAME_MS))
        };
        let next: Vec<u16> = frames
            .iter()
            .filter(later)
            .filter_map(|p| world.get::<Cycle>(*p).map(|c| c.index()))
            .collect();

        assert_eq!(next, [1, 1]);
    }

    /// A held drawing taken again holds its new frame; a drawing is taken
    /// for its own mold alone.
    #[test]
    fn a_reused_held_effect_holds_its_new_frame() {
        let mut world = World::new();
        let (root, frames, _) = drawing(&mut world, 0);
        let held = Mold::of(
            (FourCc(*b"Gold"), 1, 0),
            Look::Frame { set: 1, frame: 0 },
            None,
        );
        let start = Start {
            who: Who(Class::Effect, 2),
            place: Vec3::ZERO,
            timing: pieces::Timing {
                stride: None,
                from: 500,
                pin: Some(2),
            },
            value: None,
            ms: 500,
        };

        ended(&mut world, root, held);
        assert_eq!(reused(&mut world, mold(0), start), None, "another mold");
        assert_eq!(reused(&mut world, held, start), Some(root));
        assert_eq!(showing(&world, &frames), [2, 2]);
    }

    /// The pool keeps at most [`MOLD`] drawings of a mold and [`POOL`] in
    /// all; one past either is given back to despawn.
    #[test]
    fn the_pool_is_bounded() {
        let mut world = World::new();
        let mut pool = Pool::default();
        // How many of `molds` the pool keeps before it gives one back.
        let mut kept = |molds: &dyn Fn(usize) -> Mold| {
            (0..=POOL)
                .map(|n| (n, world.spawn_empty().id()))
                .find(|(n, entity)| pool.keep(molds(*n), *entity) == Some(*entity))
                .map_or(0, |(n, _)| n)
        };

        assert_eq!(kept(&|_| mold(0)), MOLD, "one mold");
        assert_eq!(kept(&|n| mold(n + 1)), POOL - MOLD, "in all");
    }

    /// A game's end despawns its parked drawings, which its teardown,
    /// querying, does not see.
    #[test]
    fn a_games_end_frees_the_pool() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        let (root, ..) = drawing(&mut world, 0);

        ended(&mut world, root, mold(0));
        world.run_system_once(free).expect("freed");

        assert!(world.get_entity(root).is_err());
        assert_eq!(world.query::<&Cycle>().iter(&world).count(), 0);
    }

    /// A piece whose draw effects change is despawned, not tinted that
    /// frame too: its bodies go with it, and a lift made on one panicked
    /// (Phantoms Haunt's phantom, day 8 37.1 s). Its new drawing is
    /// tinted the next frame.
    #[test]
    fn a_piece_whose_tint_changes_is_despawned_not_tinted_again() {
        let faint = Tint {
            colour: None,
            transparency: Some(1),
        };
        let fainter = Tint {
            transparency: Some(2),
            ..faint
        };
        let mut world = World::new();
        let (agent, old, new) = (33, world.spawn_empty().id(), world.spawn_empty().id());
        let mut tinted = Tinted::default();
        let tints = |t| HashMap::from([(agent, t)]);

        let first = tinted.turn(&tints(faint), |_| Some(old));
        assert_eq!(first.fresh, [(old, faint)]);

        let changed = tinted.turn(&tints(fainter), |_| Some(old));
        assert_eq!(
            changed,
            Turn {
                gone: vec![old],
                fresh: vec![],
            }
        );

        let redrawn = tinted.turn(&tints(fainter), |_| Some(new));
        assert_eq!(
            redrawn,
            Turn {
                gone: vec![],
                fresh: vec![(new, fainter)],
            }
        );
    }
}
