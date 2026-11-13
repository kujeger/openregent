//! Effects over the pieces: each effector hung on a unit and each spent
//! missile's impact set (`World::effects`), and the draw effects that
//! tint a unit or make it see-through (`World::tints`).
//!
//! ```text
//!  capture  World::effects, World::tints ──> Effects
//!  draw     one drawing per effect (`pieces::spawn`: a child per
//!           layer), its set playing from `since`: held, at its holder's
//!           drawn place plus its offset projected (height lifts it);
//!           else where it stands. An amount: a text a layer stream
//!           ([`Said`]), stepping as the layer does. A new set, frame
//!           or amount: the same entities given the new frames
//!  tint     a tinted piece's bodies made over by its draw effects:
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
use regent_draw::map::cel::Drawing;
use regent_draw::map::reel::{Beat, Reel};
use regent_draw::material::Body;

use crate::game::scope::Scoped;
use crate::game::seen::Watch;
use crate::map::mirror::Pieces;
use crate::map::pieces::{self, Coated, Depth, Key, Keyed, LAYER_STEP, Lifts, Piece, Strata};
use crate::map::tie::{Class, Tied, Who};
use crate::map::words::{self, GOLD_FOOT, Tie, Words};
use regent_draw::art::world;
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

/// An effect's drawing: its root, its layers' entities, and its
/// amount's texts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hanging {
    root: Entity,
    strata: Strata,
    words: Vec<Entity>,
}

/// The drawing of each effect, by handle, what it draws, from when and
/// its value.
#[derive(Resource, Default)]
pub struct Hung(HashMap<u32, (Hanging, Key, Look, Option<i32>)>);

impl Hung {
    /// The entity drawing `handle`.
    pub fn get(&self, handle: u32) -> Option<Entity> {
        self.0.get(&handle).map(|(hanging, ..)| hanging.root)
    }

    /// Each effect drawn: its handle and entity.
    pub fn drawn(&self) -> impl Iterator<Item = (u32, Entity)> + '_ {
        self.0
            .iter()
            .map(|(handle, (hanging, ..))| (*handle, hanging.root))
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
    mut stores: Assets2,
) {
    let depth = Depth::of(&maps);
    // An effect held with its holder is not drawn.
    let shown = || effects.now.iter().filter(|e| !e.held);
    let alive: BTreeSet<u32> = shown().map(|e| e.handle).collect();

    // Gone from the world: gone from the screen.
    let gone: Vec<u32> = hung
        .0
        .keys()
        .filter(|handle| !alive.contains(handle))
        .copied()
        .collect();

    for handle in gone {
        if let Some((hanging, ..)) = hung.0.remove(&handle) {
            commands.entity(hanging.root).despawn();
        }
    }

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
        let now = (key, effect.look, effect.value);

        match hung.0.get(&effect.handle) {
            Some(drawing) if same(drawing, now) => {
                if let Ok((mut transform, tied)) = drawn.get_mut(drawing.0.root) {
                    transform.set_if_neq(Transform {
                        translation: place.with_z(tied.keep(place.z)),
                        ..*transform
                    });
                }
            }
            old => {
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
                let start = Start {
                    who: Who(Class::Effect, effect.handle),
                    place,
                    timing,
                    ms: reel.ms,
                };
                let piece = world::Piece {
                    code: key.0,
                    set: key.1,
                    slot: key.2,
                    still: None,
                };
                let quad = sprites.quad(&mut stores.meshes);

                // A new set, frame or amount: its entities given the new
                // frames. Else drawn the first time.
                let hanging = if let Some((was, ..)) = old.cloned() {
                    let layers = sprites.piece(&corpus, &mut stores, piece);
                    let mut now = retake(&mut commands, (&quad, layers), was, (key, start));

                    now.words = effect.value.map_or_else(Vec::new, |value| {
                        amount(
                            &mut commands,
                            (&corpus, reel.ms),
                            (now.root, key),
                            value,
                            timing,
                        )
                    });
                    now
                } else {
                    let (root, strata) = pieces::spawn(
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
                    let words = effect.value.map_or_else(Vec::new, |value| {
                        amount(
                            &mut commands,
                            (&corpus, reel.ms),
                            (root, key),
                            value,
                            timing,
                        )
                    });

                    commands
                        .entity(root)
                        .insert((EffectDrawn, Keyed(key), Tied::new(start.who)));

                    Hanging {
                        root,
                        strata,
                        words,
                    }
                };

                hung.0
                    .insert(effect.handle, (hanging, key, effect.look, effect.value));
            }
        }
    }
}

/// Whether a drawing of `(key, look, value)` draws `now`.
fn same(
    (_, key, look, value): &(Hanging, Key, Look, Option<i32>),
    now: (Key, Look, Option<i32>),
) -> bool {
    (*key, *look, *value) == now
}

/// What a drawing takes up anew: the effect it draws (its order in a
/// tie), its place, its frames' start, and the game time it is shown at.
#[derive(Clone, Copy)]
struct Start {
    who: Who,
    place: Vec3,
    timing: pieces::Timing,
    ms: u32,
}

/// Drawing `was` given `layers` of `key`, on `quad`, as [`pieces::spawn`]
/// draws one anew: at its place, each layer stepping from its start in
/// its own entities; its amount's texts gone, for [`amount`] to make.
fn retake(
    commands: &mut Commands,
    (quad, layers): (&Handle<Mesh>, &[Drawing]),
    was: Hanging,
    (key, start): (Key, Start),
) -> Hanging {
    let Hanging {
        root,
        strata,
        words,
    } = was;
    let ground = start.place.truncate();
    let strata = pieces::lay(
        commands,
        (quad, root),
        (layers, &strata.0),
        ((start.place, ground), start.timing, start.ms),
    );

    // Ordered among ties from its maker's depth; darkened anew by the
    // fog, as its key is put again (`fog::units`).
    commands.entity(root).insert((
        Transform::from_translation(start.place),
        Visibility::Inherited,
        Tied::new(start.who),
        Keyed(key),
        strata.clone(),
    ));

    for word in words {
        commands.entity(word).despawn();
    }

    Hanging {
        root,
        strata,
        words: Vec::new(),
    }
}

/// An effect's value drawn as text (`view::map::amount`), shown at view
/// zoom 1.0 alone.
#[derive(Component)]
pub struct Amount;

/// A layer stream's text frames: each frame's nudge from the anchor, or
/// none where it shows no text; stepping as its layer does.
#[derive(Component, Clone, Debug)]
pub struct Said {
    nudges: Arc<[Option<(i16, i16)>]>,
    beat: Beat,
    /// The layer's pose offset and depth on its drawing.
    base: Vec3,
    up: Option<u16>,
}

impl Said {
    /// Where frame `up` puts the text, and whether it shows.
    fn look(&self, up: u16) -> (Option<Vec3>, Visibility) {
        match self.nudges.get(usize::from(up)).copied().flatten() {
            // The digits' foot, below the anchor plus the nudge.
            Some((nx, ny)) => {
                let at = Vec3::new(f32::from(nx), -(f32::from(ny) + FOOT), 0.0);

                (Some(self.base + at), Visibility::Inherited)
            }
            None => (None, Visibility::Hidden),
        }
    }
}

/// Each amount's text frame up at the clock's time written where it
/// changed.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn speak(reel: Res<Reel>, mut texts: Query<(&mut Said, &mut Transform, &mut Visibility)>) {
    for (mut said, mut put, mut seen) in &mut texts {
        let up = said.beat.up(reel.ms, said.nudges.len());

        if said.up != Some(up) {
            let (at, shows) = said.look(up);

            said.up = Some(up);

            if let Some(at) = at {
                put.translation = at;
            }

            seen.set_if_neq(shows);
        }
    }
}

/// `value` drawn under effect entity `parent`, drawing `key`: a text for
/// each stream of reference kind frames in the set's layers, centred at
/// the anchor plus its frame's nudge, stepping as `timing` says; its
/// words painted by egui (`words`). Its entities.
fn amount(
    commands: &mut Commands,
    (corpus, now): (&Corpus, u32),
    (parent, key): (Entity, Key),
    value: i32,
    timing: pieces::Timing,
) -> Vec<Entity> {
    let (code, set, slot) = key;
    let Some(unit) = corpus.unit(code) else {
        return Vec::new();
    };
    let words = Words {
        text: amount::text(value),
        look: words::GOLD,
        tie: Tie::default(),
    };
    let mut made = Vec::new();

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

            let mut said = Said {
                nudges: texts.into(),
                beat: timing.beat(ms),
                base: base.extend(f32::from(layer.variant) * LAYER_STEP),
                up: None,
            };
            let up = said.beat.up(now, said.nudges.len());
            let (at, seen) = said.look(up);

            said.up = Some(up);

            let shown = commands.spawn((
                Amount,
                words.clone(),
                Transform::from_translation(at.unwrap_or(said.base)),
                seen,
                said,
                ChildOf(parent),
            ));

            made.push(shown.id());
        }
    }

    made
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

/// Make a tinted piece's bodies over for its draw effects; one whose
/// effects change or clear is drawn anew, plain, by `map::pieces::pieces`.
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
    reel: Res<Reel>,
    drawings: Query<&Strata>,
    mut parts: Query<Coated>,
    mut images: ResMut<Assets<Image>>,
    mut bodies: ResMut<Assets<Body>>,
) {
    let turn = tinted.turn(&effects.tints, |agent| pieces.drawn(agent));

    for entity in turn.gone {
        commands.entity(entity).despawn();
    }

    for (entity, t) in turn.fresh {
        if let Ok(strata) = drawings.get(entity) {
            let made = (&mut *sprites, &*corpus, (&mut *images, &mut *bodies));

            pieces::tinted(
                &mut commands,
                (entity, strata),
                &mut parts,
                (t, reel.ms),
                made,
            );
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

    use regent_draw::map::reel::Layer;

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
    const FRAMES: u32 = 4;

    /// Game ms each of [`drawing`]'s frames shows.
    const FRAME_MS: u32 = 100;

    /// [`drawing`]'s layers: one of [`FRAMES`] frames, each with a band.
    fn layers() -> Vec<Drawing> {
        vec![pieces::tests::layer(
            FRAMES,
            0,
            true,
            regent_core::data::imag::Pass::First,
        )]
    }

    /// Its timing from game time `from`, held on `pin`.
    const fn timing(from: u32, pin: Option<u16>) -> pieces::Timing {
        pieces::Timing {
            stride: None,
            from,
            pin,
        }
    }

    /// An effect drawing as `draw` makes one, from game time `from`: a
    /// layer of [`FRAMES`] frames with its band, and an amount's text on
    /// each frame.
    fn drawing(world: &mut World, from: u32) -> Hanging {
        use bevy::ecs::system::RunSystemOnce;

        world
            .run_system_once(move |mut commands: Commands| {
                let root = commands
                    .spawn((
                        Transform::default(),
                        Visibility::Inherited,
                        EffectDrawn,
                        Tied::new(Who(Class::Effect, 1)),
                    ))
                    .id();
                let strata = pieces::lay(
                    &mut commands,
                    (&Handle::default(), root),
                    (&layers(), &[]),
                    ((Vec3::ZERO, Vec2::ZERO), timing(from, None), from),
                );
                let said = Said {
                    nudges: (0..FRAMES).map(|_| Some((0, 0))).collect(),
                    beat: Beat::runs(from, FRAME_MS),
                    base: Vec3::ZERO,
                    up: Some(0),
                };
                let words = Words {
                    text: amount::text(10),
                    look: words::GOLD,
                    tie: Tie::default(),
                };
                let text = commands
                    .spawn((
                        Amount,
                        words,
                        said,
                        Transform::default(),
                        Visibility::Inherited,
                        ChildOf(root),
                    ))
                    .id();

                Hanging {
                    root,
                    strata,
                    words: vec![text],
                }
            })
            .expect("drawn")
    }

    /// The frame each of `drawing`'s layer entities and texts shows.
    fn showing(world: &World, drawing: &Hanging) -> (Vec<Option<u16>>, Vec<Option<u16>>) {
        let layers = drawing.strata.0.iter().flat_map(|s| s.entities());

        (
            layers
                .map(|e| world.get::<Layer>(e).and_then(Layer::up))
                .collect(),
            drawing
                .words
                .iter()
                .map(|e| world.get::<Said>(*e).and_then(|s| s.up))
                .collect(),
        )
    }

    /// `drawing` given its layers again from `start`, as `draw` does when
    /// its effect's set, frame or amount changes.
    fn retaken(world: &mut World, drawing: &Hanging, start: Start) -> Hanging {
        use bevy::ecs::system::RunSystemOnce;

        let drawing = drawing.clone();
        let key = (FourCc(*b"Gold"), 1, 0);

        world
            .run_system_once(move |mut commands: Commands| {
                let layers = layers();

                retake(
                    &mut commands,
                    (&Handle::default(), &layers),
                    drawing.clone(),
                    (key, start),
                )
            })
            .expect("retaken")
    }

    /// An effect whose set or frame changes keeps its root and layer
    /// entities: it ties as its effect, starts on frame 0 at its new
    /// start and place, and steps at its rate from there.
    #[test]
    fn a_changed_effect_keeps_its_entities() {
        use bevy::ecs::system::RunSystemOnce;

        const LATER: u32 = 12_345;

        let mut world = World::new();
        let drawing = drawing(&mut world, 0);
        let other = tied(&mut world, 3, 0.0);
        let place = Vec3::new(64.0, -32.0, 0.5);
        let start = Start {
            who: Who(Class::Effect, 5),
            place,
            timing: timing(LATER, None),
            ms: LATER,
        };

        world.init_resource::<Reel>();
        world.resource_mut::<Reel>().ms = 250;
        world
            .run_system_once(regent_draw::map::reel::step)
            .expect("stepped");
        assert_eq!(showing(&world, &drawing).0, [Some(2), Some(2)]);

        let now = retaken(&mut world, &drawing, start);

        assert_eq!((now.root, &now.strata), (drawing.root, &drawing.strata));
        assert_eq!(
            world.get::<Transform>(drawing.root).map(|t| t.translation),
            Some(place)
        );
        assert_eq!(showing(&world, &now).0, [Some(0), Some(0)]);
        assert!(now.words.is_empty(), "no amount now");

        untied(&mut world);
        assert!(
            z(&world, drawing.root) > z(&world, other),
            "handle 5 before 3"
        );

        world.resource_mut::<Reel>().ms = LATER + FRAME_MS;
        world
            .run_system_once(regent_draw::map::reel::step)
            .expect("stepped");
        assert_eq!(showing(&world, &now).0, [Some(1), Some(1)]);
    }

    /// A held effect whose held frame changes holds the new one.
    #[test]
    fn a_changed_held_effect_holds_its_new_frame() {
        let mut world = World::new();
        let drawing = drawing(&mut world, 0);
        let start = Start {
            who: Who(Class::Effect, 2),
            place: Vec3::ZERO,
            timing: timing(500, Some(2)),
            ms: 500,
        };
        let now = retaken(&mut world, &drawing, start);

        assert_eq!(showing(&world, &now).0, [Some(2), Some(2)]);
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
