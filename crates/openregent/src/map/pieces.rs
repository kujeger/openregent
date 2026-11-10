//! The world's pieces and missiles as sprites, a child per frame.
//!
//! ```text
//!  pieces    each mirror (`mirror`) ──> its drawing, a child per frame
//!  missiles  World::flown ──> one entity each
//! ```
//!
//! Art steps on game time, so `--speed` scales it: each set at its own
//! [`regent_core::sim::movement::action::frame_ms`], a walk a frame a step.

use std::collections::HashMap;

use bevy::prelude::*;

use regent_core::base::fixed::{self, Fixed};
use regent_core::content::imagery::Animation;
use regent_core::content::imagery::sort_cells;
use regent_core::content::unit::Unit;
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::{Pass, SetKind};
use regent_core::sim::movement::action::Look;
use regent_core::sim::world::bodies::Facing;
use regent_core::sim::world::{
    draw_view::{Doing, Housed, Shown},
    missile::Flown,
};
use regent_draw::art::{self, Assets2, Sprites};
use regent_draw::data::Corpus;
use regent_draw::lie::Footing;
use regent_draw::map::Maps;
use regent_draw::map::cel::{Drawing, Picture, rungs};
use regent_draw::map::reel::{Cycle, Reel};
use regent_draw::material;

use crate::game::clock::Snapshot;
use crate::map::mirror::{DrawingOf, Phase, Pieces, Pin, Was};
use crate::map::{effects, fog};

/// Sets a character is drawn in, in order of preference, while it walks
/// and while it does not. **Inferred**: the world keeps no set.
const WALKING: [SetKind; 3] = [SetKind::Walk, SetKind::Stand, SetKind::Active];
const STILL: [SetKind; 4] = [
    SetKind::Stand,
    SetKind::Inactive,
    SetKind::Hotspot,
    SetKind::Active,
];

/// How far a piece's layers sit above one another.
pub(crate) const LAYER_STEP: f32 = 0.01;

/// Each saved piece's set and slot, by handle: what a building shows
/// until it changes.
#[derive(Resource, Default)]
pub(crate) struct Saved(HashMap<u32, (u16, u16)>);

impl Saved {
    /// `world`'s ([`regent_core::sim::world::board::Board::saved_looks`]).
    pub(crate) fn of(world: &regent_core::sim::world::World) -> Saved {
        let looks = world.board().saved_looks().iter();

        Saved(
            looks
                .map(|(handle, d)| (*handle, (low(d.set), d.drawing.0)))
                .collect(),
        )
    }
}

/// A set id's low half; the top byte is the layer.
pub(crate) use regent_core::data::imag::set_number as low;

/// Each descriptor and set's sort lift, worked out once each; the
/// pictures are the art service's (`art::Sprites`).
#[derive(Resource, Default)]
pub(crate) struct Lifts {
    lifts: HashMap<(FourCc, u16), f32>,
}

impl crate::draw::shelf::Shelved for Lifts {}

impl Lifts {
    /// Cells of y a piece in set `set` of `code` sorts nearer
    /// (`content::imagery::sort_cells`, `IsoDrawList`): the attack
    /// flag 76, so over all about it.
    pub(crate) fn lift(&mut self, corpus: &Corpus, code: FourCc, set: u16) -> f32 {
        *self.lifts.entry((code, set)).or_insert_with(|| {
            let nudge = Unit::find(&corpus.catalogue, code)
                .and_then(Result::ok)
                .and_then(|u| u.stack(set).next().map(|a| a.nudge))
                .unwrap_or(0);

            sort_rows(nudge)
        })
    }
}

/// Cells of y a sort nudge of `px` screen pixels moves a part
/// (`content::imagery::sort_cells`).
fn sort_rows(px: i32) -> f32 {
    fixed::to_f32(sort_cells(px))
}

/// A layer whose part sorts apart from its piece's base
/// (`content::imagery::Sort::Own`): the cells of y it sorts nearer, and its rung's
/// depth within the piece.
#[derive(Component, Clone, Copy)]
pub(crate) struct Apart {
    lift: f32,
    rung: f32,
}

/// A second-pass part, a flier's shadow: its offset beside the ground's
/// under its piece ([`stands`]), so it is kept there as the piece moves.
#[derive(Component, Clone, Copy)]
pub(crate) struct Grounded {
    rest: Vec2,
}

/// What [`pieces`] reads and moves of a part.
pub(crate) type Part = (
    Option<&'static Apart>,
    Option<&'static Grounded>,
    &'static mut Transform,
    Option<&'static mut Cycle>,
    &'static mut Visibility,
);

/// What [`pieces`] moves of a piece.
pub(crate) type Drawn = (
    &'static mut Transform,
    &'static mut Visibility,
    Option<&'static Children>,
    &'static mut Under,
    Option<&'static mut Footing>,
);

/// Descriptor, set, slot.
pub type Key = (FourCc, u16, usize);

/// What a piece's drawing draws.
#[derive(Component, Clone, Copy)]
pub struct Keyed(pub Key);

/// How a piece's frames step: from game time `from`, each its set's time
/// or a walker's `stride`; or held on frame `pin`.
#[derive(Clone, Copy)]
pub(crate) struct Timing {
    pub(crate) stride: Option<u32>,
    pub(crate) from: u32,
    pub(crate) pin: Option<u16>,
}

/// A drawn piece, by agent.
#[derive(Component)]
pub struct Piece(pub u32);

/// How far the ground under a piece lies from where its body draws,
/// world units: a flier's drop to its shadow, else none. Its ring and the
/// ring's part of its box sit there (ring draw, ring rect:
/// on movement layer 4, the ground under unit pos).
#[derive(Component, Clone, Copy, Default, Debug, PartialEq)]
pub struct Under(pub Vec2);

impl Under {
    /// A piece drawn at `place` over `ground`: the drop, if it flies.
    fn of(shown: &Shown, ground: Vec2, place: Vec3) -> Under {
        match shown.aloft {
            Some(_) => Under(ground - place.truncate()),
            None => Under::default(),
        }
    }
}

/// The tile a piece's frame draws, archive and index: what the pick
/// reads (`select::Picker`).
#[derive(Component)]
pub struct Shows(pub (usize, usize));

/// Put each piece where the world has it, in the set it is in.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(
    clippy::too_many_lines,
    reason = "a piece's place, parts and drop are set in one pass"
)]
pub(crate) fn pieces(
    mut commands: Commands,
    snapshot: Res<Snapshot>,
    maps: Res<Maps>,
    corpus: Res<Corpus>,
    saved: Res<Saved>,
    reel: Res<Reel>,
    mut lifts: ResMut<Lifts>,
    mut sprites: ResMut<Sprites>,
    mirrors: Pieces,
    mut states: Query<(&mut Phase, &mut Pin)>,
    keys: Query<&Keyed>,
    fog: Res<fog::Fog>,
    mut stores: Assets2,
    mut drawn: Query<Drawn, With<Piece>>,
    mut parts: Query<Part, Without<Piece>>,
) {
    let depth = Depth::of(&maps);
    let between = snapshot.between;

    // A piece gone from the world takes its drawing with it (`mirror`).
    for shown in mirrors.iter() {
        let Some(key) = key(&corpus, &saved, shown) else {
            continue;
        };
        let Some(mirror) = mirrors.mirror_of(shown.agent) else {
            continue;
        };
        let Ok((mut phase, mut pinned)) = states.get_mut(mirror) else {
            continue;
        };
        let was = mirrors.was(shown.agent).copied().unwrap_or_default();
        let pin = pin(shown, key);
        let was_pinned = pinned.0;

        if was_pinned != pin {
            pinned.0 = pin;
        }

        let cell = cell(shown.at, was.at.as_ref(), between);
        let lift = lifts.lift(&corpus, key.0, key.1);
        let place = match shown.housed {
            Housed::Hung { on, offset } => {
                hung(&mirrors, (&maps, &depth), (on, offset), (lift, between))
            }
            Housed::Out | Housed::Held => {
                let aloft = height(shown, &was, between);
                let place = body_at(&maps, cell, aloft).extend(depth.at(cell.y + lift));

                Some((place, cell))
            }
        };
        let Some((place, foot)) = place else {
            continue;
        };
        // Where its soft shadow lies from: its own cell, or its holder's.
        let footing = Footing {
            row: foot.y,
            ground: maps.standing_at(foot).y,
        };
        let ground = Under::of(shown, maps.standing_at(cell), place);
        // Held, or unseen under the fog.
        let seen = match (shown.housed, fog.hides(shown.agent)) {
            (Housed::Held, _) | (_, true) => Visibility::Hidden,
            (Housed::Out | Housed::Hung { .. }, false) => Visibility::Inherited,
        };
        let old = mirrors
            .drawn(shown.agent)
            .and_then(|entity| keys.get(entity).ok().map(|k| (entity, k.0)));

        match old {
            // A held frame moved on: the same parts, held on the new one.
            Some((entity, was))
                if was == key && (was_pinned == pin || was_pinned.and(pin).is_some()) =>
            {
                if let Ok((mut transform, mut visibility, children, mut drop, lies)) =
                    drawn.get_mut(entity)
                {
                    // Written only when changed: a write marks the
                    // tree for transform propagation.
                    transform.set_if_neq(Transform {
                        translation: place,
                        ..*transform
                    });
                    drop.set_if_neq(ground);

                    if let Some(mut lies) = lies {
                        lies.set_if_neq(footing);
                    }

                    let under = maps.standing_at(cell) - place.truncate();

                    for child in children.into_iter().flatten() {
                        let Ok((layer, grounded, mut put, cycle, mut shows)) =
                            parts.get_mut(*child)
                        else {
                            continue;
                        };

                        if let (Some(mut cycle), Some(frame)) = (cycle, pin)
                            && was_pinned != pin
                        {
                            *cycle = held(cycle.index(), cycle.frames(), frame, reel.ms);
                            *shows = if cycle.showing(reel.ms) {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            };
                        }

                        let mut next = *put;

                        // A layer sorting apart: at its own row's depth.
                        if let (Housed::Out | Housed::Held, Some(layer)) = (shown.housed, layer) {
                            next.translation.z =
                                depth.at(cell.y + layer.lift) - place.z + layer.rung;
                        }

                        // A shadow: on the ground under the piece, as it
                        // climbs or the ground under it does.
                        if let Some(grounded) = grounded {
                            next.translation = (under + grounded.rest).extend(next.translation.z);
                        }

                        put.set_if_neq(next);
                    }

                    if *visibility != seen {
                        *visibility = seen;
                    }
                }
            }
            old => {
                if let Some((entity, _)) = old {
                    commands.entity(entity).despawn();
                }

                // A set the world keeps starts when it says; else a turn
                // keeps the frame, and a new set starts on frame 0,
                // as does one an action held.
                let from = match (shown.look, old) {
                    (Some(Look::Plays { set, since }), _) if low(set) == key.1 => since,
                    (_, Some((_, was)))
                        if (was.0, was.1) == (key.0, key.1) && was_pinned.is_none() =>
                    {
                        phase.0.unwrap_or(reel.ms)
                    }
                    _ => reel.ms,
                };
                let timing = Timing {
                    stride: stride(shown, key),
                    from,
                    pin,
                };

                if phase.0 != Some(from) {
                    phase.0 = Some(from);
                }

                let entity = spawn(
                    &mut commands,
                    &corpus,
                    &mut sprites,
                    &mut stores,
                    &reel,
                    key,
                    (
                        Some(shown.agent),
                        place,
                        maps.standing_at(cell),
                        seen,
                        timing,
                    ),
                );

                commands
                    .entity(entity)
                    .insert((ground, footing, Keyed(key), DrawingOf(mirror)));
            }
        }
    }
}

/// Where a piece at `cell` draws its body: a flier at its own height
/// ([`Shown::aloft`]), as unit screen place projects
/// unit pos; anything else on the ground. A flier's shadow stays
/// on the ground ([`stands`]).
fn body_at(maps: &Maps, cell: Vec2, aloft: Option<Fixed>) -> Vec2 {
    match aloft {
        Some(z) => maps.aloft_at(cell, fixed::to_f32(z)),
        None => maps.standing_at(cell),
    }
}

/// How high a flier draws, `between` of the way from its height a frame
/// before (`was`), as its place.
fn height(shown: &Shown, was: &Was, between: f32) -> Option<Fixed> {
    shown.aloft.map(|z| {
        was.aloft.map_or(z, |was| {
            let rise = (z - was).to_bits();

            #[allow(clippy::cast_possible_truncation, reason = "between two heights")]
            #[allow(clippy::cast_precision_loss, reason = "16.16 cells of a map")]
            let slid = (was.to_bits() as f32 + rise as f32 * between).round() as i32;

            Fixed::from_bits(slid)
        })
    })
}

/// Where a piece hung on agent `on` at `offset` draws, sorting `lift`
/// cells nearer: at its holder's drawn place plus the offset projected,
/// as an effect (`effects::hanging`); and its holder's cell. `None`
/// where the holder is not shown.
fn hung(
    pieces: &Pieces,
    (maps, depth): (&Maps, &Depth),
    (on, offset): (u32, [Fixed; 3]),
    (lift, between): (f32, f32),
) -> Option<(Vec3, Vec2)> {
    let holder = pieces.get(on)?;
    let was = pieces.was(on).copied().unwrap_or_default();
    let at = cell(holder.at, was.at.as_ref(), between);
    let drawn = body_at(maps, at, height(holder, &was, between)).extend(depth.at(at.y));
    let place = effects::hanging(maps, depth, (offset, lift), drawn, at);

    Some((place, at))
}

/// The frame the world holds a piece drawing `key` on: an action's
/// `InitAction`'s, or the last where no after-set plays; a walker's `Walk`
/// frame, which its steps move on. `None` where the clock steps it.
pub(crate) fn pin(shown: &Shown, key: Key) -> Option<u16> {
    match shown.look {
        Some(Look::Frame { set, frame }) if low(set) == key.1 && shown.doing != Doing::Walking => {
            u16::try_from(frame).ok()
        }
        _ => shown
            .walk_frame
            .filter(|_| walks(shown, key))
            .and_then(|f| u16::try_from(f).ok()),
    }
}

/// The missiles drawn, and the game time they were drawn for.
#[derive(Resource, Default)]
pub(crate) struct Shots {
    drawn: Vec<Entity>,
    /// The game time and [`Snapshot::between`] drawn for: stepped, a
    /// game frame's; smooth, every screen frame's.
    at: Option<(u32, u32)>,
}

/// Draw each missile in flight where the world has it, lifted by its
/// height, its set playing from its launch; with `[enhancements]
/// smooth_position`, slid from where it was a frame before, as a piece.
/// Redrawn when either changes: a missile steps a run and has no agent
/// to follow it by.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub(crate) fn missiles(
    mut commands: Commands,
    snapshot: Res<Snapshot>,
    maps: Res<Maps>,
    corpus: Res<Corpus>,
    reel: Res<Reel>,
    mut sprites: ResMut<Sprites>,
    mut shots: ResMut<Shots>,
    fog: Res<fog::Fog>,
    mut stores: Assets2,
) {
    let at = (reel.ms, snapshot.between.to_bits());

    if shots.at != Some(at) {
        let depth = Depth::of(&maps);

        for entity in shots.drawn.drain(..) {
            commands.entity(entity).despawn();
        }

        for flown in &snapshot.flown {
            let Some(key) = missile_key(&corpus, flown) else {
                continue;
            };

            let [x, y, z] = flight(
                flown.at,
                snapshot.flown_before.get(&flown.handle),
                snapshot.between,
            );
            let cell = Vec2::new(x, y);
            let place = maps.aloft_at(cell, z).extend(depth.at(cell.y));
            let timing = Timing {
                stride: None,
                from: flown.launched,
                pin: None,
            };
            let seen = if fog.hides_missile(flown.handle) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            let entity = spawn(
                &mut commands,
                &corpus,
                &mut sprites,
                &mut stores,
                &reel,
                key,
                (None, place, maps.standing_at(cell), seen, timing),
            );

            let footing = Footing {
                row: cell.y,
                ground: maps.standing_at(cell).y,
            };

            commands.entity(entity).insert(footing);
            shots.drawn.push(entity);
        }

        shots.at = Some(at);
    }
}

/// What a missile draws: its walk, else its first set, in the slot its
/// heading picks. **Inferred**: a flying missile plays its moving set.
fn missile_key(corpus: &Corpus, flown: &Flown) -> Option<Key> {
    let unit = corpus.unit(flown.code)?;
    let set = WALKING
        .iter()
        .find_map(|k| unit.animation(*k))
        .or_else(|| unit.animations.first())?;

    Some((flown.code, low(set.id), slot(&unit, set, flown.facing)))
}

/// A walker's frame time while it walks in `Walk`: a frame a step, not
/// the set's rate. `None` otherwise.
pub(crate) fn stride(shown: &Shown, key: Key) -> Option<u32> {
    shown.stride.filter(|_| walks(shown, key))
}

/// Whether a piece drawing `key` walks in `Walk`.
fn walks(shown: &Shown, key: Key) -> bool {
    shown.doing == Doing::Walking && SetKind::from_id(key.1) == SetKind::Walk
}

/// A piece's place in cells, `between` of the way from where it was.
pub(crate) fn cell(now: (i32, i32), before: Option<&(i32, i32)>, between: f32) -> Vec2 {
    let cells = crate::ui::tracking::cells;
    let to = cells(now);

    before.map_or(to, |b| cells(*b).lerp(to, between))
}

/// A missile's place in cells, height last, `between` of the way from
/// where it was.
fn flight(now: [Fixed; 3], before: Option<&[Fixed; 3]>, between: f32) -> [f32; 3] {
    let cells = |at: [Fixed; 3]| at.map(fixed::to_f32);
    let to = cells(now);

    before.map_or(to, |b| {
        let from = cells(*b);

        [0, 1, 2].map(|i| from[i] + (to[i] - from[i]) * between)
    })
}

/// The depth a thing standing on row `y` draws at: between the scatter
/// rows the viewer drew either side of it, which it ranks by row.
pub(crate) use regent_draw::map::depth::Depth;

/// What a piece draws: its descriptor, the set it is in and the slot its
/// facing picks. `None` without art. The set: its action's, else its save
/// record's while it stands, else by what it is doing.
pub(crate) fn key(corpus: &Corpus, saved: &Saved, shown: &Shown) -> Option<Key> {
    // A particle system draws its particles, not a picture: a spell
    // unit's (`CreateSpellUnit`'s `fire_ball`) are the plume's.
    let unit = corpus
        .unit(shown.code)
        .filter(|u| u.desc.kind() != UnitKind::ParticleSystem)?;
    let building = unit.desc.kind() == UnitKind::Building;
    let record = saved.0.get(&shown.handle).filter(|_| shown.handle != 0);

    // The set the world keeps: a mobile's action until it walks, a
    // building's. A building keeps its record's slot, as it never turns;
    // a set the art lacks shows as if none were kept.
    let looked = shown
        .look
        .filter(|_| shown.doing != Doing::Walking)
        .and_then(|look| {
            let set = low(look.set());
            let first = unit.stack(set).next()?;
            let slot = record
                .map(|&(_, s)| usize::from(s))
                .filter(|s| building && *s < first.poses.len())
                .unwrap_or_else(|| slot(&unit, first, shown.facing));

            Some((shown.code, set, slot))
        });
    let kept = record
        .filter(|_| shown.doing == Doing::Standing)
        .filter(|(set, _)| unit.stack(*set).next().is_some());

    if looked.is_some() {
        looked
    } else if let Some(&(set, slot)) = kept {
        Some((shown.code, set, usize::from(slot)))
    } else {
        let kinds: &[SetKind] = match shown.doing {
            Doing::Walking => &WALKING,
            Doing::Standing | Doing::Waiting | Doing::Acting => &STILL,
        };
        let set = kinds
            .iter()
            .find_map(|k| unit.animation(*k))
            .or_else(|| unit.animations.first())?;

        Some((shown.code, low(set.id), slot(&unit, set, shown.facing)))
    }
}

/// The slot a piece facing `facing` draws of `set` (`Facing::slot_of`).
pub(crate) fn slot(unit: &Unit<'_>, set: &Animation, facing: i32) -> usize {
    Facing::of(&unit.desc).slot_of(facing, set.poses.len())
}

/// A piece's entity, with a child for each frame of each layer, stepping
/// as `timing` says; `ground` where the ground under it is drawn.
pub(crate) fn spawn(
    commands: &mut Commands,
    corpus: &Corpus,
    sprites: &mut Sprites,
    stores: &mut Assets2,
    reel: &Reel,
    key: Key,
    (agent, place, ground, seen, timing): (Option<u32>, Vec3, Vec2, Visibility, Timing),
) -> Entity {
    let quad = sprites.quad(&mut stores.meshes);
    let (code, set, slot) = key;
    let piece = art::world::Piece {
        code,
        set,
        slot,
        still: None,
    };
    let layers = sprites.piece(corpus, stores, piece);
    let parent = commands
        .spawn((Transform::from_translation(place), seen))
        .id();

    // A piece can be picked; a missile cannot.
    if let Some(agent) = agent {
        commands.entity(parent).insert(Piece(agent));
    }

    let base = layers.first().map_or(0, |d| d.nudge);

    #[allow(clippy::cast_precision_loss, reason = "a set has at most six layers")]
    for (nth, drawing) in rungs(layers).into_iter().zip(layers.iter()) {
        let rung = nth as f32 * LAYER_STEP;
        let apart = (drawing.nudge != base).then(|| Apart {
            lift: sort_rows(drawing.nudge),
            rung,
        });
        let frames = u16::try_from(drawing.cels.len()).unwrap_or(u16::MAX);
        let ms = timing.stride.unwrap_or(drawing.ms);
        let pose = stands(drawing, place, ground);

        for (index, cel) in drawing.cels.iter().enumerate() {
            let Some(cel) = cel else { continue };
            let index = u16::try_from(index).unwrap_or(u16::MAX);
            let cycle = match timing.pin {
                Some(frame) => held(index, frames, frame, timing.from),
                None => Cycle::new(index, frames, 0, ms).from(timing.from),
            };
            let put = Transform {
                translation: (pose - cel.anchor).extend(rung),
                scale: cel.size.extend(1.0),
                ..default()
            };
            let shows = if cycle.showing(reel.ms) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            let mut child = match &cel.body {
                Picture::Indexed(body) => commands.spawn((
                    Mesh3d(quad.clone()),
                    MeshMaterial3d(body.clone()),
                    put,
                    shows,
                )),
                Picture::Colour(colour) => commands.spawn((
                    Mesh3d(quad.clone()),
                    MeshMaterial3d(colour.clone()),
                    put,
                    shows,
                )),
            };

            child.insert((cycle, ChildOf(parent)));

            // The pick skips second-pass frames.
            if drawing.pass == Pass::First {
                child.insert(Shows(cel.tile));
            }

            let grounded = (drawing.pass == Pass::Second).then(|| Grounded {
                rest: drawing.posed() - cel.anchor,
            });

            mark(&mut child, apart, grounded);

            if let Some(lift) = &cel.lift {
                child.with_child(lifting(&quad, lift));
            }

            if let Some(band) = &cel.band {
                let mut shadow = commands.spawn((
                    Mesh3d(quad.clone()),
                    MeshMaterial3d(band.clone()),
                    put,
                    shows,
                    cycle,
                    ChildOf(parent),
                ));

                mark(&mut shadow, apart, grounded);

                if let Some(aloft) = &cel.aloft {
                    shadow.insert(super::cast::Aloft(aloft.clone()));
                }
            }
        }
    }

    parent
}

/// A translucent body's lift ([`regent_draw::soft::lifts`]) on `quad`, to
/// spawn as the body entity's child: it takes the body's place, scale,
/// cycle and sorting, and a swap of the piece's bodies (`effects::tint`,
/// `fog::units`, which take every child's) passes it by.
pub(crate) fn lifting(quad: &Handle<Mesh>, lift: &Handle<material::Body>) -> impl Bundle {
    (
        Mesh3d(quad.clone()),
        MeshMaterial3d(lift.clone()),
        Transform::IDENTITY,
        Visibility::Inherited,
    )
}

/// Body entity `body`, on `quad`, swapped to see-through body `glass`:
/// its lift made anew, any old one gone.
pub(crate) fn relift(
    commands: &mut Commands,
    (body, quad): (Entity, &Handle<Mesh>),
    glass: &Handle<material::Body>,
    (sprites, corpus, made): (
        &mut Sprites,
        &Corpus,
        (&mut Assets<Image>, &mut Assets<material::Body>),
    ),
) {
    let lift = sprites.variant(corpus, made, glass, art::world::Variant::Lift);
    let mut body = commands.entity(body);

    body.despawn_related::<Children>();

    if let Some(lift) = lift {
        body.with_child(lifting(quad, &lift));
    }
}

/// A part's marks for [`pieces`]: whether it sorts apart, whether it
/// keeps to the ground.
fn mark(part: &mut EntityCommands, apart: Option<Apart>, grounded: Option<Grounded>) {
    if let Some(apart) = apart {
        part.insert(apart);
    }

    if let Some(grounded) = grounded {
        part.insert(grounded);
    }
}

/// Where a drawing's part stands about its piece at `place`: a
/// second-pass part, a flier's shadow, on the `ground` under it; any
/// part but the first at its pose offset.
fn stands(drawing: &Drawing, place: Vec3, ground: Vec2) -> Vec2 {
    drawing.pass.height(Vec2::ZERO, ground - place.truncate()) + drawing.posed()
}

/// Frame `index` of `frames`, showing while frame `frame` is held, from
/// game time `from`: a cycle whose frame never steps.
pub(crate) const fn held(index: u16, frames: u16, frame: u16, from: u32) -> Cycle {
    let start = if frames == 0 { 0 } else { frame % frames };

    Cycle::new(index, frames, start, u32::MAX).from(from)
}

#[cfg(test)]
mod tests {
    use regent_core::gpl::runtime::MS_PER_FRAME;
    use regent_core::sim::world::CROWN;

    use super::*;
    use crate::files::config::Settings;
    use crate::game::clock::{Motion, Steps, walked};

    /// The corpus's unit is `Unit::find`'s, parsed once: a second ask
    /// gets the same one. No unit, none.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_corpus_parses_a_unit_once() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let code = FourCc(*b"ABp1");
        let found = Unit::find(&corpus.catalogue, code)
            .and_then(Result::ok)
            .expect("the Mausoleum");
        let first = corpus.unit(code).expect("the Mausoleum");

        assert_eq!(first.id, found.id);
        assert_eq!(first.animations.len(), found.animations.len());
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &corpus.unit(code).expect("again")
        ));
        assert!(corpus.unit(FourCc(*b"none")).is_none());
    }

    /// A piece at `at`, placed outright `jumps` times.
    fn piece(at: (i32, i32), jumps: u32) -> Shown {
        Shown {
            agent: 1,
            code: FourCc(*b"BVr1"),
            handle: 0,
            owner: CROWN,
            at,
            aloft: None,
            facing: 0,
            doing: Doing::Standing,
            housed: Housed::Out,
            stride: None,
            walk_frame: None,
            look: None,
            ruined: false,
            jumps,
        }
    }

    /// A walker slides between its two places; a piece placed outright
    /// since draws at its new place at once.
    #[test]
    fn a_jump_draws_at_its_new_place_at_once() {
        let was = [piece((0, 0), 0)];
        let drawn = |now: Shown| {
            let before = walked(&was, &[now]);

            cell(now.at, before.get(&now.agent), 0.5)
        };

        assert_eq!(drawn(piece((64, 0), 0)), Vec2::new(1.0, 0.0));
        assert_eq!(drawn(piece((64, 0), 1)), Vec2::new(2.0, 0.0));
    }

    /// `piece`'s layers from `corpus`, as the art service makes them.
    fn made(corpus: Corpus, piece: art::world::Piece) -> Vec<Drawing> {
        use bevy::ecs::system::RunSystemOnce;

        let mut app = App::new();

        app.insert_resource(corpus)
            .init_resource::<Sprites>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<regent_draw::plain::Plain>>()
            .init_resource::<Assets<material::Body>>()
            .init_resource::<Assets<material::Band>>();
        app.world_mut()
            .run_system_once(
                move |corpus: Res<Corpus>, mut sprites: ResMut<Sprites>, mut stores: Assets2| {
                    sprites.piece(&corpus, &mut stores, piece).to_vec()
                },
            )
            .expect("made")
    }

    /// A Dryad at its walk's end, `Waiting` with set `0xb1` playing on
    /// track 0, draws that set at the world's frame, not `Stand`.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_dryad_ending_its_walk_draws_set_0xb1() {
        const PATH_END: u32 = 0xb1;

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let dryad = corpus
            .catalogue
            .tagged(b"DUNT")
            .filter_map(|(at, _)| regent_core::content::unit::Unit::at(&corpus.catalogue, at).ok())
            .find(|u| u.desc.head.name == b"Dryad")
            .expect("the Dryad");
        let shown = Shown {
            code: dryad.id,
            doing: Doing::Waiting,
            look: Some(Look::Frame {
                set: PATH_END,
                frame: 2,
            }),
            ..piece((0, 0), 0)
        };
        let drawn = key(&corpus, &Saved::default(), &shown).expect("art");

        assert_eq!(drawn.1, low(PATH_END));
        assert_eq!(pin(&shown, drawn), Some(2));
    }

    /// The Magic Bazaar's `Die-6` draws its five `FXa1` fires after its
    /// two layers, each a translucent frame with a lift.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_bazaars_last_damage_stage_draws_its_fires() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let piece = art::world::Piece {
            code: FourCc(*b"ABl1"),
            set: 0x65,
            slot: 0,
            still: None,
        };
        let layers = made(corpus, piece);

        assert_eq!(layers.len(), 7);
        assert!(
            layers[2..]
                .iter()
                .all(|d| d.cels.iter().flatten().all(|c| c.lift.is_some()))
        );
    }

    /// The Magic Bazaar's `Die-6` fires keep their place in the sprite
    /// modes at any zoom, about each other and the building: every frame
    /// of every layer moves alike.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_buildings_fires_hold_their_place_at_any_zoom() {
        const SIZE: UVec2 = UVec2::new(1600, 900);

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let piece = art::world::Piece {
            code: FourCc(*b"ABl1"),
            set: 0x65,
            slot: 0,
            still: None,
        };
        let layers = made(corpus, piece);
        let place = Vec2::new(320.0, -144.0);

        for ppu in [0.5, 0.7, 0.84, 1.0, 1.19, 1.25, 1.5, 1.68, 1.9] {
            let camera = regent_draw::present::pinned(place, ppu, SIZE);
            let pixel = |world: Vec2| {
                let from = (world - camera) * ppu;

                SIZE.as_vec2() / 2.0 + Vec2::new(from.x, -from.y)
            };

            // Each frame's art corner: its quad's top left (`spawn`).
            let shifts: Vec<Vec2> = layers
                .iter()
                .flat_map(|drawing| drawing.cels.iter().flatten().map(move |cel| (drawing, cel)))
                .map(|(drawing, cel)| {
                    let centre = place + drawing.posed() - cel.anchor;
                    let corner = centre + Vec2::new(-cel.size.x, cel.size.y) / 2.0;

                    regent_draw::smooth::moved(corner, pixel, ppu)
                })
                .collect();

            for shift in &shifts {
                assert!(
                    (*shift - shifts[0]).abs().max_element() < 1e-2,
                    "{ppu} pixels a unit: {shift} moved, the first {}",
                    shifts[0]
                );
            }
        }
    }

    /// A firebalm arrow in flight (`WPm1`, its walk) draws its `XR46`
    /// flame trail of the same direction after its own frames.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_firebalm_arrow_draws_its_trail() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let piece = art::world::Piece {
            code: FourCc(*b"WPm1"),
            set: 0x01,
            slot: 5,
            still: None,
        };
        let layers = made(corpus, piece);
        let trail = layers.last().expect("a trail");

        assert!(layers.len() >= 2);
        assert_eq!(trail.part, layers.len() - 1);
        assert_eq!((trail.cels.len(), trail.ms), (4, 100));
    }

    /// The Mausoleum's dirt diamond, its `Inactive` layer 1, sorts its
    /// own part 1,000 pixels further: drawn under
    /// the building, not on it.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_mausoleums_dirt_diamond_draws_under_it() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let piece = art::world::Piece {
            code: FourCc(*b"ABp1"),
            set: 0xd0,
            slot: 0,
            still: Some(0),
        };
        let layers = made(corpus, piece);
        let size = |d: &Drawing| d.cels[0].as_ref().map(|c| c.size);
        let rungs = rungs(&layers);

        assert_eq!(layers.len(), 2);
        assert_eq!(
            size(&layers[1]),
            Some(Vec2::new(200.0, 139.0)),
            "the diamond"
        );
        assert!(rungs[1] < rungs[0], "the diamond draws first");
    }

    /// The healer's healing effect draws mode 8, translucent, with no
    /// shadow: each frame lifts the shadow under it.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_translucent_frame_lifts_the_shadow_under_it() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let piece = art::world::Piece {
            code: FourCc(*b"HRa1"),
            set: 0x60,
            slot: 0,
            still: None,
        };
        let layers = made(corpus, piece);
        let cels: Vec<_> = layers
            .iter()
            .flat_map(|d| d.cels.iter().flatten())
            .collect();

        assert!(!cels.is_empty());
        assert!(cels.iter().all(|c| c.lift.is_some() && c.band.is_none()));
    }

    /// The energy blast missile draws two parts: its body where it flies,
    /// then its shadow, the bit 24 frames, on the ground under it.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_flying_missiles_shadow_draws_on_the_ground_under_it() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let piece = art::world::Piece {
            code: FourCc(*b"WPa2"),
            set: 0x40,
            slot: 0,
            still: None,
        };
        let layers = made(corpus, piece);
        let (place, ground) = (Vec3::new(10.0, 50.0, -1.0), Vec2::new(10.0, 20.0));
        let parts: Vec<(Pass, usize, usize)> = layers
            .iter()
            .map(|d| (d.pass, d.part, d.cels.iter().flatten().count()))
            .collect();
        let rungs = rungs(&layers);

        assert_eq!(parts, [(Pass::First, 0, 14), (Pass::Second, 1, 14)]);
        assert_eq!(stands(&layers[0], place, ground), Vec2::ZERO);
        assert_eq!(stands(&layers[1], place, ground), Vec2::new(0.0, -30.0));
        assert!(rungs[0] < rungs[1], "the shadow draws after the body");
        assert!(
            layers[1].cels.iter().flatten().all(|c| c.band.is_some()),
            "every shadow frame marks"
        );
    }

    /// An ice dragon at its least height, 180/32 cells, draws its body
    /// that high above the ground; a walker stands on the ground.
    #[test]
    fn a_flier_draws_its_body_at_its_own_height() {
        let maps = Maps::with(Vec::new());
        let cell = Vec2::new(3.0, 4.0);
        let least = fixed::from_whole(180) / 32;

        assert_eq!(body_at(&maps, cell, None), maps.standing_at(cell));
        assert_eq!(
            body_at(&maps, cell, Some(least)),
            maps.aloft_at(cell, 5.625)
        );
        assert!(body_at(&maps, cell, Some(least)).y > maps.standing_at(cell).y);
    }

    /// Off, a walker draws at its last frame's place however far into
    /// the next frame the clock is; on, between its last two places.
    #[test]
    fn a_piece_slides_only_with_smooth_position() {
        let mut settings = Settings {
            enhancements: crate::files::config::Enhancements::classic(),
            ..Settings::default()
        };
        let drawn = |settings: &Settings| {
            let between = Motion::of(settings).between(25.0, 50.0);

            cell((64, 32), Some(&(32, 32)), between)
        };

        assert_eq!(drawn(&settings), Vec2::new(2.0, 1.0));

        settings.enhancements.smooth_position = true;

        assert_eq!(drawn(&settings), Vec2::new(1.5, 1.0));
    }

    /// Agent `agent`'s drawing, and what it draws.
    fn drawing_of(app: &App, agent: u32) -> Option<(Entity, Key)> {
        let world = app.world();
        let mirror = world.resource::<crate::map::mirror::Index>().get(agent)?;
        let entity = world.get::<crate::map::mirror::Drawing>(mirror)?.entity();

        Some((entity, world.get::<Keyed>(entity)?.0))
    }

    /// Each part of agent 1's drawing, where it lands: its parent's
    /// place plus its own offset, sorted.
    fn landed(app: &mut App) -> Vec<(i64, i64)> {
        let (entity, _) = drawing_of(app, 1).expect("drawn");
        let world = app.world();
        let parent = world.get::<Transform>(entity).expect("placed").translation;
        let mut parts: Vec<(i64, i64)> = world
            .get::<Children>(entity)
            .expect("parts")
            .iter()
            .map(|c| world.get::<Transform>(c).expect("a part").translation + parent)
            .map(|at| {
                #[allow(clippy::cast_possible_truncation, reason = "a screen pixel's 64ths")]
                let at = ((at.x * 64.0).round() as i64, (at.y * 64.0).round() as i64);

                at
            })
            .collect();

        parts.sort_unstable();
        parts
    }

    /// A drawing app with [`pieces`] alone, showing `shown`.
    fn drawing(corpus: Corpus, shown: Shown) -> App {
        drawings(corpus, vec![shown])
    }

    /// The same, showing every piece in `now`.
    fn drawings(corpus: Corpus, now: Vec<Shown>) -> App {
        let mut app = App::new();

        app.insert_resource(Snapshot::of(Steps::showing(now)))
            .insert_resource(Maps::with(Vec::new()))
            .insert_resource(corpus)
            .init_resource::<Saved>()
            .init_resource::<Reel>()
            .init_resource::<Lifts>()
            .init_resource::<Sprites>()
            .init_resource::<crate::map::mirror::Index>()
            .init_resource::<fog::Fog>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<regent_draw::plain::Plain>>()
            .init_resource::<Assets<material::Body>>()
            .init_resource::<Assets<material::Band>>()
            .add_systems(Update, (crate::map::mirror::sync, pieces).chain());
        app.update();
        app
    }

    /// The frame agent `agent`'s drawing shows at game time `ms`.
    fn drawn_frame(app: &App, agent: u32, ms: u32) -> Option<u16> {
        let world = app.world();
        let (entity, _) = drawing_of(app, agent)?;

        world
            .get::<Children>(entity)?
            .iter()
            .filter_map(|c| world.get::<Cycle>(c))
            .find(|c| c.showing(ms))
            .map(|c| c.index())
    }

    /// A walker draws the world's `Walk` frame, game frame by game frame:
    /// it changes only with a step, so a turn, the frame between nodes or
    /// a hero's slower step holds the pose. A bot game's walkers.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_walkers_drawn_frame_is_the_worlds() {
        const FRAMES: u32 = 2_400;
        const BOT: u32 = 7;

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let archives = crate::game::load::archives(&hd).expect("the dataset");
        let (begun, catalogue) = crate::game::begin::test_game(&["--map-seed", "3"]);
        let mut sim = begun.sim;
        let mut bot =
            crate::game::bot::Bot::new(regent_core::sim::world::offer::Menu::of(&catalogue), BOT);
        let mut app = drawings(Corpus::open(&archives).expect("corpus"), Vec::new());

        // Per stride: walking frames, those drawn as the world has them,
        // and turn frames whose pose moved.
        let mut seen: std::collections::BTreeMap<Option<u32>, [u32; 3]> =
            std::collections::BTreeMap::default();
        let mut was: HashMap<u32, (Shown, Option<u16>)> = HashMap::new();

        for _ in 0..FRAMES {
            for input in bot.moves(&sim) {
                sim.input(input);
            }

            sim.step();

            let now = sim.world().shown();
            let ms = sim.frames().wrapping_mul(MS_PER_FRAME);

            app.insert_resource(Snapshot::of(Steps::showing(now.clone())));
            app.world_mut().resource_mut::<Reel>().ms = ms;
            app.update();

            let mut next = HashMap::new();

            for shown in now.iter().filter(|s| s.doing == Doing::Walking) {
                let Some((_, key)) = drawing_of(&app, shown.agent) else {
                    continue;
                };
                let drawn = drawn_frame(&app, shown.agent, ms);

                if SetKind::from_id(key.1) == SetKind::Walk
                    && let Some(world) = shown.walk_frame
                {
                    let tally = seen.entry(shown.stride).or_default();

                    tally[0] += 1;
                    tally[1] += u32::from(drawn.map(u32::from) == Some(world));

                    // A turn: the facing moved, the place did not.
                    if let Some((before, frame)) = was.get(&shown.agent)
                        && before.facing != shown.facing
                        && before.at == shown.at
                        && *frame != drawn
                    {
                        tally[2] += 1;
                    }
                }

                next.insert(shown.agent, (*shown, drawn));
            }

            was = next;
        }

        let walked: u32 = seen.values().map(|t| t[0]).sum();

        assert!(walked > 1_000, "walkers walked: {seen:?}");
        assert!(seen.contains_key(&Some(100)), "heroes walked: {seen:?}");
        assert!(
            seen.values().all(|t| t[0] == t[1] && t[2] == 0),
            "stride: [walking, drawn as the world, turns moving the pose] {seen:?}"
        );
    }

    /// An ice dragon climbing in one set draws as one made at its new
    /// height: its shadow stays on the ground under it.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_climbing_fliers_shadow_stays_on_the_ground() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus = || {
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus")
        };
        let dragon = |z: i32| Shown {
            code: FourCc(*b"BVm1"),
            aloft: Some(fixed::from_whole(z)),
            ..piece((96, 96), 0)
        };
        let mut climbed = drawing(corpus(), dragon(6));

        climbed.insert_resource(Snapshot::of(Steps::showing(vec![dragon(9)])));
        climbed.update();

        let mut made = drawing(corpus(), dragon(9));

        assert_eq!(landed(&mut climbed), landed(&mut made));
    }

    /// A flier's height slides with its place: halfway between, half a
    /// frame on; with no height a frame before, where it is.
    #[test]
    fn a_fliers_height_slides_with_its_place() {
        let flier = Shown {
            aloft: Some(Fixed::from_bits(200)),
            ..piece((0, 0), 0)
        };
        let mut was = Was::default();

        assert_eq!(height(&flier, &was, 0.5), Some(Fixed::from_bits(200)));

        was.aloft = Some(Fixed::from_bits(100));

        assert_eq!(height(&flier, &was, 0.5), Some(Fixed::from_bits(150)));
        assert_eq!(height(&piece((0, 0), 0), &was, 0.5), None);
    }

    /// A missile follows the same setting as a piece.
    #[test]
    fn a_missile_slides_only_with_smooth_position() {
        const CELL: Fixed = fixed::ONE;
        let mut settings = Settings {
            enhancements: crate::files::config::Enhancements::classic(),
            ..Settings::default()
        };
        let drawn = |settings: &Settings| {
            let between = Motion::of(settings).between(25.0, 50.0);

            Vec3::from(flight(
                [2 * CELL, CELL, 2 * CELL],
                Some(&[CELL, CELL, Fixed::ZERO]),
                between,
            ))
        };

        assert_eq!(drawn(&settings), Vec3::new(2.0, 1.0, 2.0));

        settings.enhancements.smooth_position = true;

        assert_eq!(drawn(&settings), Vec3::new(1.5, 1.0, 1.0));
        assert_eq!(Vec3::from(flight([CELL, CELL, CELL], None, 0.5)), Vec3::ONE);
    }

    #[test]
    fn a_piece_is_drawn_between_its_last_two_places() {
        assert_eq!(cell((64, 32), Some(&(32, 32)), 0.5), Vec2::new(1.5, 1.0));
        assert_eq!(cell((64, 32), None, 0.5), Vec2::new(2.0, 1.0));
    }

    #[test]
    fn a_new_set_opens_on_frame_0_and_steps_at_its_rate() {
        let now = 1_250;
        let up = |at| {
            (0..6)
                .find(|&i| Cycle::new(i, 6, 0, 100).from(now).showing(at))
                .expect("one frame is up")
        };

        assert_eq!(up(now), 0);
        assert_eq!(up(now + 50), 0, "a 100 ms frame spans two game frames");
        assert_eq!(up(now + 100), 1);
        assert_eq!(up(now + 600), 0, "and round");
    }
}
