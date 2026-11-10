//! Effects over the pieces: each effector hung on a unit and each spent
//! missile's impact set (`World::effects`), and the draw effects that
//! tint a unit or make it see-through (`World::tints`).
//!
//! ```text
//!  capture  World::effects, World::tints ──> Effects
//!  draw     one entity per effect, its set playing from `since`:
//!           held, at its holder's drawn place plus its offset projected
//!           (height lifts it); else where it stands
//!  tint     a tinted piece's bodies swapped for its draw effects':
//!           palette to the tint's nearest, or see-through glass
//! ```
//!
//! Depth follows the engine's draw list, ordered by board y:
//! an overlay's y is its holder's plus its offset,
//! one 65,536th nearer, so it draws over its holder.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

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

use crate::game::scope::Scoped;
use crate::game::seen::Watch;
use crate::map::mirror::Pieces;
use crate::map::pieces::{self, Depth, Key, LAYER_STEP, Lifts, Piece};
use crate::map::words::{self, GOLD_FOOT, Tie, Words};
use regent_draw::art::world::Variant;
use regent_draw::art::{Assets2, Sprites};

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
pub fn draw(
    mut commands: Commands,
    effects: Res<Effects>,
    pieces: Pieces,
    maps: Res<Maps>,
    corpus: Res<Corpus>,
    reel: Res<Reel>,
    holders: Query<&Transform, (With<Piece>, Without<EffectDrawn>)>,
    mut drawn: Query<&mut Transform, (With<EffectDrawn>, Without<Piece>)>,
    mut lifts: ResMut<Lifts>,
    mut sprites: ResMut<Sprites>,
    mut hung: ResMut<Hung>,
    mut stores: Assets2,
) {
    let depth = Depth::of(&maps);
    // An effect held with its holder is not drawn.
    let shown = || effects.now.iter().filter(|e| !e.held);
    let alive: BTreeSet<u32> = shown().map(|e| e.handle).collect();

    // Gone from the world: gone from the screen.
    hung.0.retain(|handle, (entity, ..)| {
        let keep = alive.contains(handle);

        if !keep {
            commands.entity(*entity).despawn();
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
                if let Ok(mut transform) = drawn.get_mut(*entity) {
                    transform.set_if_neq(Transform {
                        translation: place,
                        ..*transform
                    });
                }
            }
            old => {
                if let Some((entity, ..)) = old {
                    commands.entity(*entity).despawn();
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

                commands.entity(entity).insert(EffectDrawn);
                hung.0
                    .insert(effect.handle, (entity, key, effect.look, effect.value));

                if let Some(value) = effect.value {
                    amount(
                        &mut commands,
                        (&corpus, &reel),
                        (entity, key),
                        value,
                        timing,
                    );
                }
            }
        }
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
