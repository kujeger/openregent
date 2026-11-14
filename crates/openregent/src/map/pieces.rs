//! The world's pieces and missiles as sprites: a drawing each, a child
//! per layer (`regent_draw::map::reel::Layer`).
//!
//! ```text
//!  pieces    each mirror (`mirror`) ──> its drawing: a root, and each
//!            layer's body, band and lift ([`Strata`])
//!  missiles  World::flown ──> one drawing each
//!  key       a new set or slot: the same entities given the new layers'
//!            frames and beat ([`lay`]); entities added or taken only
//!            where the layer count, a band or a lift differs
//!  pending   a key whose sheet pages are on their way (`Sprites::paged`):
//!            the drawing up kept, moved with the piece, each layer held
//!            on the frame it shows ([`Awaited`]); none yet, none drawn
//! ```
//!
//! While a piece's pages are pending its drawing is the last it had:
//! body, band and lift held alike, so no shadow without its body; its
//! ring, bars and pick follow that drawing. A piece with no drawing yet
//! shows nothing: no body, shadow, ring, bar or pick. Landed, the new
//! key's frames step from when the key came ([`Awaited::from`]), so it
//! shows the frame up now.
//!
//! Art steps on game time, so `--speed` scales it: each set at its own
//! [`regent_core::sim::movement::action::frame_ms`], a walk a frame a step.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use bevy::ecs::query::QueryItem;
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
use regent_core::sim::world::draw::Tint;
use regent_core::sim::world::{
    draw_view::{Doing, Housed, Shown},
    missile::Flown,
};
use regent_draw::art::world::Variant;
use regent_draw::art::{self, Assets2, Sprites};
use regent_draw::data::Corpus;
use regent_draw::lie::Footing;
use regent_draw::map::Maps;
use regent_draw::map::cel::{Cel, Drawing, Picture, rungs};
use regent_draw::map::reel::{Beat, Layer, Own, Reel, Role, Stepped, SteppedItem, dress, wear};
use regent_draw::material::{self, Band, Body};

use crate::game::clock::Snapshot;
use crate::map::mirror::{DrawingOf, Phase, Pieces, Pin, Was};
use crate::map::tie::{Class, Tied, Who};
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

/// A second-pass part, a flier's shadow: its pose ([`stands`]), so it is
/// kept on the ground under its piece as it moves.
#[derive(Component, Clone, Copy)]
pub(crate) struct Grounded {
    posed: Vec2,
}

/// What [`pieces`] reads and moves of a part.
pub(crate) type Part = (Option<&'static Apart>, Option<&'static Grounded>, Stepped);

/// One layer's entities: its body's, and its band's and lift's where a
/// frame draws one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stratum {
    pub(crate) body: Entity,
    pub(crate) band: Option<Entity>,
    pub(crate) lift: Option<Entity>,
}

impl Stratum {
    /// Its entities.
    pub(crate) fn entities(self) -> impl Iterator<Item = Entity> {
        std::iter::once(self.body).chain(self.band).chain(self.lift)
    }
}

/// A drawing's layers, lowest first: the entities [`lay`] gives new
/// frames as its key changes.
#[derive(Component, Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct Strata(pub(crate) Vec<Stratum>);

/// A drawing whose draw effects made its bodies over (`effects::tint`):
/// drawn anew, not given new frames, as its key changes.
#[derive(Component)]
pub(crate) struct Tinting;

/// What [`pieces`] moves of a piece.
pub(crate) type Drawn = (
    &'static mut Transform,
    &'static mut Visibility,
    Option<&'static Children>,
    &'static mut Under,
    Option<&'static mut Footing>,
    &'static Tied,
    (&'static Strata, Has<Tinting>),
);

/// Descriptor, set, slot.
pub type Key = (FourCc, u16, usize);

/// What a piece's drawing draws.
#[derive(Component, Clone, Copy)]
pub struct Keyed(pub Key);

/// How a piece's frames step: from game time `from`, each its set's time
/// or a walker's `stride`; or held on frame `pin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Timing {
    pub(crate) stride: Option<u32>,
    pub(crate) from: u32,
    pub(crate) pin: Option<u16>,
}

impl Timing {
    /// A layer's beat, its set's frames `ms` each.
    pub(crate) fn beat(self, ms: u32) -> Beat {
        match self.pin {
            Some(frame) => Beat::holds(frame, self.from),
            None => Beat::runs(self.from, self.stride.unwrap_or(ms)),
        }
    }
}

/// A drawn piece, by agent.
#[derive(Component)]
pub struct Piece(pub u32);

/// On a piece's mirror: a key whose sheet pages are on their way, and
/// the game time its frames step from, as worked out when it came.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Awaited {
    key: Key,
    from: u32,
}

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
    mut states: Query<(&mut Phase, &mut Pin, Option<&Awaited>)>,
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
        let Ok((mut phase, mut pinned, awaited)) = states.get_mut(mirror) else {
            continue;
        };
        let awaited = awaited.copied();
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

        // The same picture: moved, and a held frame moved on. One held
        // while another key's pages were pending is laid again.
        let same = awaited.is_none()
            && matches!(old, Some((_, was))
            if was == key && (was_pinned == pin || was_pinned.and(pin).is_some()));
        let at = (place, ground, footing, seen);

        if let (true, Some((entity, _))) = (same, old) {
            let under = maps.standing_at(cell) - place.truncate();
            let held = pin.filter(|_| was_pinned != pin);

            if let Ok(root) = drawn.get_mut(entity) {
                placed(root, at);
            }

            follow(
                &mut commands,
                &mut parts,
                (drawn.get(entity).ok().and_then(|r| r.2), shown.housed),
                (&depth, cell, place, under),
                (held, reel.ms),
            );

            continue;
        }

        // A set the world keeps starts when it says; else a turn
        // keeps the frame, and a new set starts on frame 0,
        // as does one an action held. Pages awaited: from when the key came.
        let from = match (shown.look, old, awaited) {
            (_, _, Some(awaited)) if awaited.key == key => awaited.from,
            (Some(Look::Plays { set, since }), ..) if low(set) == key.1 => since,
            (_, Some((_, was)), _) if (was.0, was.1) == (key.0, key.1) && was_pinned.is_none() => {
                phase.0.unwrap_or(reel.ms)
            }
            _ => reel.ms,
        };
        let timing = Timing {
            stride: stride(shown, key),
            from,
            pin,
        };
        let piece = art::world::Piece {
            code: key.0,
            set: key.1,
            slot: key.2,
            still: None,
        };

        // Its pages on their way: the drawing up kept and moved, its
        // layers held, until they land.
        let quad = sprites.quad(&mut stores.meshes);
        let Some(layers) = sprites
            .piece(&corpus, &mut stores, piece)
            .map(<[Drawing]>::to_vec)
        else {
            if let Some(root) = old.and_then(|(entity, _)| drawn.get_mut(entity).ok()) {
                placed(root, at);
            }

            let children = old.and_then(|(entity, _)| drawn.get(entity).ok()?.2);
            let under = maps.standing_at(cell) - place.truncate();
            let fresh = awaited.is_none_or(|a| a.key != key);

            if fresh {
                hold(&mut parts, children, reel.ms);
                commands.entity(mirror).insert(Awaited { key, from });
            }

            follow(
                &mut commands,
                &mut parts,
                (children, shown.housed),
                (&depth, cell, place, under),
                (None, reel.ms),
            );

            continue;
        };

        if awaited.is_some() {
            commands.entity(mirror).remove::<Awaited>();
        }

        if phase.0 != Some(from) {
            phase.0 = Some(from);
        }

        if crate::perf::on()
            && let Some((_, was)) = old
            && swaps(&corpus, &mut sprites, &mut stores, (was, key))
        {
            crate::perf::churn(crate::perf::Churn::Swaps, 1);
        }

        let kept = old.and_then(|(entity, _)| {
            let root = drawn.get_mut(entity).ok()?;
            let (strata, tinting) = root.6;

            (!tinting).then(|| (entity, strata.clone(), root))
        });

        // A new key: the same entities, given the new layers' frames.
        // None yet, or one whose draw effects made it over: drawn anew.
        if let Some((entity, strata, root)) = kept {
            let floor = maps.standing_at(cell);
            let strata = lay(
                &mut commands,
                (&quad, entity),
                (&layers, &strata.0),
                ((place, floor), timing, reel.ms),
            );

            placed(root, at);
            commands.entity(entity).insert((strata, Keyed(key)));
            crate::perf::churn(crate::perf::Churn::Rekeys, 1);
        } else {
            if let Some((entity, _)) = old {
                commands.entity(entity).despawn();
                crate::perf::churn(crate::perf::Churn::Respawns, 1);
            }

            let (entity, _) = spawn(
                &mut commands,
                (&quad, &layers),
                &reel,
                (
                    Some(shown.agent),
                    place,
                    maps.standing_at(cell),
                    seen,
                    timing,
                ),
            );

            commands.entity(entity).insert((
                ground,
                footing,
                Keyed(key),
                DrawingOf(mirror),
                Tied::new(Who(Class::Piece, shown.agent)),
            ));
        }
    }
}

/// A piece's root moved to `place`, its drop to the ground, its footing
/// and visibility set: each written only when changed, as a write marks
/// the tree for transform propagation. A tie's order kept.
fn placed(
    (mut transform, mut visibility, _, mut drop, lies, tied, _): QueryItem<'_, '_, Drawn>,
    (place, ground, footing, seen): (Vec3, Under, Footing, Visibility),
) {
    transform.set_if_neq(Transform {
        translation: place.with_z(tied.keep(place.z)),
        ..*transform
    });
    drop.set_if_neq(ground);

    if let Some(mut lies) = lies {
        lies.set_if_neq(footing);
    }

    visibility.set_if_neq(seen);
}

/// A drawing's layers held on the frames they show: another key's pages
/// are on their way.
fn hold(parts: &mut Query<Part, Without<Piece>>, children: Option<&Children>, now: u32) {
    for child in children.into_iter().flatten() {
        if let Ok((_, _, mut stepped)) = parts.get_mut(*child)
            && let Some(up) = stepped.layer.up()
        {
            stepped.layer.beat = Beat::holds(up, now);
        }
    }
}

/// A piece's parts kept up with it: a layer sorting apart at its own
/// row's depth, a shadow on the ground `under` it, a frame `held` moved
/// on.
fn follow(
    commands: &mut Commands,
    parts: &mut Query<Part, Without<Piece>>,
    (children, housed): (Option<&Children>, Housed),
    (depth, cell, place, under): (&Depth, Vec2, Vec3, Vec2),
    (held, now): (Option<u16>, u32),
) {
    for child in children.into_iter().flatten() {
        let Ok((layer, grounded, mut stepped)) = parts.get_mut(*child) else {
            continue;
        };

        if let Some(frame) = held {
            stepped.layer.beat = Beat::holds(frame, now);
            stepped.redo(now, commands);
        }

        // A layer sorting apart: at its own row's depth.
        if let (Housed::Out | Housed::Held, Some(layer)) = (housed, layer) {
            let z = depth.at(cell.y + layer.lift) - place.z + layer.rung;

            if stepped.put.translation.z.to_bits() != z.to_bits() {
                stepped.put.translation.z = z;
            }
        }

        // A shadow: on the ground under the piece, as it climbs or the
        // ground under it does.
        if let Some(grounded) = grounded {
            stepped.stand(under + grounded.posed);
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

/// The missiles drawn, by handle, and the game time they were drawn for.
#[derive(Resource, Default)]
pub(crate) struct Shots {
    drawn: HashMap<u32, Shot>,
    /// The game time and [`Snapshot::between`] drawn for: stepped, a
    /// game frame's; smooth, every screen frame's.
    at: Option<(u32, u32)>,
}

/// A missile's drawing: its entity, what it draws and its launch.
#[derive(Clone, Copy, Debug)]
struct Shot {
    entity: Entity,
    key: Key,
    launched: u32,
}

/// What [`missiles`] moves of a missile's drawing.
type Flying = (
    &'static mut Transform,
    &'static mut Visibility,
    &'static mut Footing,
    Option<&'static Children>,
    &'static Tied,
    &'static Strata,
);

/// Draw each missile in flight where the world has it, lifted by its
/// height, its set playing from its launch; with `[enhancements]
/// smooth_position`, slid from where it was a frame before, as a piece.
/// Moved when either changes; given new frames as what it draws or its
/// launch changes.
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
    reason = "a missile's place and drawing are set in one pass"
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
    mut flying: Query<Flying, Without<Grounded>>,
    mut shadows: Query<(&Grounded, Stepped)>,
) {
    let at = (reel.ms, snapshot.between.to_bits());

    if shots.at != Some(at) {
        let depth = Depth::of(&maps);
        let mut was = std::mem::take(&mut shots.drawn);
        let mut pending = false;

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
            let ground = maps.standing_at(cell);
            let seen = if fog.hides_missile(flown.handle) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            let footing = Footing {
                row: cell.y,
                ground: ground.y,
            };
            let timing = Timing {
                stride: None,
                from: flown.launched,
                pin: None,
            };
            let moved = (place, ground, seen, footing);

            // The same picture: moved. Another, or a new launch: its
            // entities given the new frames. Else drawn the first time,
            // once its pages are in: in flight, at its place and frame
            // now. Its pages on their way: the drawing up moved, as it
            // was; none yet, none drawn.
            let old = was.remove(&flown.handle);
            let same = old.is_some_and(|o| (o.key, o.launched) == (key, flown.launched));
            let quad = sprites.quad(&mut stores.meshes);
            let piece = art::world::Piece {
                code: key.0,
                set: key.1,
                slot: key.2,
                still: None,
            };
            let layers = (!same)
                .then(|| {
                    sprites
                        .piece(&corpus, &mut stores, piece)
                        .map(<[Drawing]>::to_vec)
                })
                .flatten();
            let shot = match (old, layers) {
                (Some(old), _) if same => {
                    fly(&mut flying, &mut shadows, old.entity, moved);
                    Some(old)
                }
                (Some(old), Some(layers)) => {
                    let kept = flying
                        .get(old.entity)
                        .map(|f| f.5.0.clone())
                        .unwrap_or_default();
                    let strata = lay(
                        &mut commands,
                        (&quad, old.entity),
                        (&layers, &kept),
                        ((place, ground), timing, reel.ms),
                    );

                    fly(&mut flying, &mut shadows, old.entity, moved);
                    commands.entity(old.entity).insert(strata);
                    crate::perf::churn(crate::perf::Churn::Rekeys, 1);
                    Some(Shot {
                        entity: old.entity,
                        key,
                        launched: flown.launched,
                    })
                }
                (Some(old), None) => {
                    fly(&mut flying, &mut shadows, old.entity, moved);
                    pending = true;
                    Some(old)
                }
                (None, Some(layers)) => {
                    let (entity, _) = spawn(
                        &mut commands,
                        (&quad, &layers),
                        &reel,
                        (None, place, ground, seen, timing),
                    );

                    commands
                        .entity(entity)
                        .insert((footing, Tied::new(Who(Class::Missile, flown.handle))));
                    crate::perf::churn(crate::perf::Churn::Missiles, 1);
                    Some(Shot {
                        entity,
                        key,
                        launched: flown.launched,
                    })
                }
                (None, None) => {
                    pending = true;
                    None
                }
            };

            // A handle listed twice keeps its last drawing.
            if let Some(shot) = shot
                && let Some(twice) = shots.drawn.insert(flown.handle, shot)
            {
                commands.entity(twice.entity).despawn();
            }
        }

        // Landed or gone.
        for shot in was.into_values() {
            commands.entity(shot.entity).despawn();
        }

        // Pages on their way: looked at again next frame, though the
        // clock stands.
        shots.at = (!pending).then_some(at);
    }
}

/// Missile drawing `entity` moved to `place`, over `ground`: its shadow
/// parts on the ground under it, placed as [`lay`] places them.
fn fly(
    flying: &mut Query<Flying, Without<Grounded>>,
    shadows: &mut Query<(&Grounded, Stepped)>,
    entity: Entity,
    (place, ground, seen, footing): (Vec3, Vec2, Visibility, Footing),
) {
    if let Ok((mut transform, mut visibility, mut lies, children, tied, _)) = flying.get_mut(entity)
    {
        let under = ground - place.truncate();

        // A tie's order kept while its depth stays (`tie::untie`).
        transform.set_if_neq(Transform {
            translation: place.with_z(tied.keep(place.z)),
            ..*transform
        });
        visibility.set_if_neq(seen);
        lies.set_if_neq(footing);

        for child in children.into_iter().flatten() {
            if let Ok((grounded, mut stepped)) = shadows.get_mut(*child) {
                stepped.stand(under + grounded.posed);
            }
        }
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

/// Whether key `now`'s drawing takes a body material key `was`'s does
/// not: a perf count of what a key change costs in materials.
fn swaps(
    corpus: &Corpus,
    sprites: &mut Sprites,
    stores: &mut Assets2,
    (was, now): (Key, Key),
) -> bool {
    let mut bodies = |(code, set, slot): Key| -> BTreeSet<bevy::asset::AssetId<material::Body>> {
        let piece = art::world::Piece {
            code,
            set,
            slot,
            still: None,
        };

        sprites
            .piece(corpus, stores, piece)
            .into_iter()
            .flatten()
            .flat_map(|d| d.cels.iter().flatten())
            .filter_map(|cel| match &cel.body {
                Picture::Indexed(body) => Some(body.id()),
                Picture::Colour(_) => None,
            })
            .collect()
    };
    let before = bodies(was);

    !bodies(now).is_subset(&before)
}

/// The slot a piece facing `facing` draws of `set` (`Facing::slot_of`).
pub(crate) fn slot(unit: &Unit<'_>, set: &Animation, facing: i32) -> usize {
    Facing::of(&unit.desc).slot_of(facing, set.poses.len())
}

/// A piece's entity drawing `layers` on `quad`, with its layers'
/// entities as children ([`lay`]), stepping as `timing` says; `ground`
/// where the ground under it is drawn. Its entity and layers.
pub(crate) fn spawn(
    commands: &mut Commands,
    (quad, layers): (&Handle<Mesh>, &[Drawing]),
    reel: &Reel,
    (agent, place, ground, seen, timing): (Option<u32>, Vec3, Vec2, Visibility, Timing),
) -> (Entity, Strata) {
    let parent = commands
        .spawn((Transform::from_translation(place), seen))
        .id();

    // A piece can be picked; a missile cannot.
    if let Some(agent) = agent {
        commands.entity(parent).insert(Piece(agent));
    }

    let strata = lay(
        commands,
        (quad, parent),
        (layers, &[]),
        ((place, ground), timing, reel.ms),
    );

    commands.entity(parent).insert(strata.clone());
    (parent, strata)
}

/// `layers` drawn on `root` at `place` over `ground`, stepping as
/// `timing` says from game time `now`: layer by layer in `old`'s entities,
/// the rest despawned, any more spawned on `quad`. Each layer's body,
/// then its band where a frame casts, then its lift where a frame is
/// translucent.
pub(crate) fn lay(
    commands: &mut Commands,
    (quad, root): (&Handle<Mesh>, Entity),
    (layers, old): (&[Drawing], &[Stratum]),
    ((place, ground), timing, now): ((Vec3, Vec2), Timing, u32),
) -> Strata {
    let base = layers.first().map_or(0, |d| d.nudge);
    let rungs = rungs(layers);
    let mut strata = Vec::with_capacity(layers.len());

    #[allow(clippy::cast_precision_loss, reason = "a set has at most six layers")]
    for (nth, (drawing, rung)) in layers.iter().zip(rungs).enumerate() {
        let was = old.get(nth);
        let z = rung as f32 * LAYER_STEP;
        let apart = (drawing.nudge != base).then(|| Apart {
            lift: sort_rows(drawing.nudge),
            rung: z,
        });
        let grounded = (drawing.pass == Pass::Second).then(|| Grounded {
            posed: drawing.posed(),
        });
        let made = |role| {
            let beat = timing.beat(drawing.ms);

            Layer::new(
                drawing.cels.clone(),
                role,
                beat,
                stands(drawing, place, ground),
            )
        };
        let band = made(Role::Band).draws();
        let lift = made(Role::Lift).draws();
        let mut entity = |was: Option<Entity>| part(commands, (quad, root), was);
        let body = entity(was.map(|w| w.body));
        let lift = lift.then(|| entity(was.and_then(|w| w.lift)));
        let band = band.then(|| entity(was.and_then(|w| w.band)));
        let stratum = Stratum { body, band, lift };

        // A band or lift the new frames have none of: gone.
        let gone = [
            (was.and_then(|w| w.band), band.is_some()),
            (was.and_then(|w| w.lift), lift.is_some()),
        ];

        for (entity, kept) in gone {
            if let (Some(entity), false) = (entity, kept) {
                commands.entity(entity).despawn();
            }
        }

        let parts = [
            (Some(body), Role::Body),
            (lift, Role::Lift),
            (band, Role::Band),
        ];

        for (entity, role) in parts {
            let Some(entity) = entity else { continue };
            let mut part = commands.entity(entity);

            dress(&mut part, made(role), now, z);

            // The pick reads a first-pass body's frame.
            if role == Role::Body && drawing.pass == Pass::First {
                part.insert(Hits);
            }

            if let Some(apart) = apart {
                part.insert(apart);
            }

            if let Some(grounded) = grounded {
                part.insert(grounded);
            }
        }

        strata.push(stratum);
    }

    // Layers past the new count: gone.
    for stratum in old.iter().skip(layers.len()) {
        for entity in stratum.entities() {
            commands.entity(entity).despawn();
        }
    }

    Strata(strata)
}

/// A layer's entity: `was`, what its old frames made of it taken off;
/// else a new one on `quad` under `root`.
fn part(
    commands: &mut Commands,
    (quad, root): (&Handle<Mesh>, Entity),
    was: Option<Entity>,
) -> Entity {
    match was {
        Some(was) => {
            commands
                .entity(was)
                .remove::<(Apart, Grounded, Hits, Own<Body>, Own<Band>)>();
            was
        }
        None => commands.spawn((Mesh3d(quad.clone()), ChildOf(root))).id(),
    }
}

/// What a layer's bodies are made over by, one body at a time.
pub(crate) type Maker<'a> = &'a mut dyn FnMut(&Handle<Body>) -> Option<Handle<Body>>;

/// Layer `stepped`'s bodies (a lift's: its lifts) made over by `made`,
/// frame by frame, each its own where `made` makes none; or, `None`, its
/// frames' own again. Its [`Own`] put on or taken off, the frame up drawn
/// so at once.
pub(crate) fn make_over(commands: &mut Commands, stepped: &mut SteppedItem, made: Option<Maker>) {
    let band = stepped.own.1;

    match made {
        Some(made) => {
            let role = stepped.layer.role;
            let base = |cel: &Option<Cel>| match (role, cel.as_ref()?) {
                (Role::Lift, cel) => cel.lift.clone(),
                (
                    _,
                    Cel {
                        body: Picture::Indexed(body),
                        ..
                    },
                ) => Some(body.clone()),
                (_, _) => None,
            };
            let own = Own(stepped
                .layer
                .cels()
                .iter()
                .map(|cel| base(cel).map(|base| made(&base).unwrap_or(base)))
                .collect());

            stepped.repaint((Some(&own), band), commands);
            commands.entity(stepped.entity).insert(own);
        }
        None => {
            if stepped.own.0.is_some() {
                stepped.repaint((None, band), commands);
                commands.entity(stepped.entity).remove::<Own<Body>>();
            }
        }
    }
}

/// What [`tinted`] reads and changes of a drawing's layers.
pub(crate) type Coated = (
    Stepped,
    &'static Mesh3d,
    Option<&'static Apart>,
    Option<&'static Grounded>,
);

/// The art service and stores [`tinted`] makes bodies with.
type Making<'a> = (
    &'a mut Sprites,
    &'a Corpus,
    (&'a mut Assets<Image>, &'a mut Assets<Body>),
);

/// Drawing `root`'s bodies made over by draw effects `tint`
/// (`art::world::Variant::Tint`). See-through, each layer lifts the
/// shadow under it where drawn as now ([`regent_draw::soft::lifts`]), a
/// lift entity made where it had none, and its bands go: mode 5 draws
/// nothing from 247.
pub(crate) fn tinted(
    commands: &mut Commands,
    (root, strata): (Entity, &Strata),
    parts: &mut Query<Coated>,
    (tint, now): (Tint, u32),
    (sprites, corpus, (images, bodies)): Making,
) {
    let mut made = |body: &Handle<Body>, variant| {
        sprites.variant(corpus, (&mut *images, &mut *bodies), body, variant)
    };

    commands.entity(root).insert(Tinting);

    for stratum in &strata.0 {
        let Ok((mut body, quad, apart, grounded)) = parts.get_mut(stratum.body) else {
            continue;
        };

        make_over(
            commands,
            &mut body,
            Some(&mut |b| made(b, Variant::Tint(tint))),
        );

        if tint.transparency.is_none() {
            continue;
        }

        // Each frame's lift: its see-through body's.
        let lifts: Arc<[Option<Handle<Body>>]> = body
            .layer
            .cels()
            .iter()
            .map(|cel| match &cel.as_ref()?.body {
                Picture::Indexed(b) => {
                    let glass = made(b, Variant::Tint(tint))?;

                    made(&glass, Variant::Lift)
                }
                Picture::Colour(_) => None,
            })
            .collect();
        let own = Own(lifts);
        let (layer, z) = (body.layer.clone(), body.put.translation.z);
        let marks = (apart.copied(), grounded.copied(), quad.0.clone());

        if let Some(Ok((mut lift, ..))) = stratum.lift.map(|lift| parts.get_mut(lift)) {
            lift.repaint((Some(&own), None), commands);
            commands.entity(lift.entity).insert(own);
        } else {
            lifted(
                commands,
                (root, layer.as_role(Role::Lift), z),
                marks,
                (own, now),
            );
        }

        if let Some(band) = stratum.band {
            commands.entity(band).despawn();
        }
    }
}

/// A lift `lift` at depth `z` under `root`, drawing `own`'s lifts from
/// game time `now`, sorting and standing as its body (`marks`): for a
/// body made see-through that had none.
fn lifted(
    commands: &mut Commands,
    (root, lift, z): (Entity, Layer, f32),
    (apart, grounded, quad): (Option<Apart>, Option<Grounded>, Handle<Mesh>),
    (own, now): (Own<Body>, u32),
) {
    let up = lift.due(now);
    let seen = lift.skin(up, (Some(&own), None));
    let mut part = commands.spawn((Mesh3d(quad), ChildOf(root)));

    dress(&mut part, lift, now, z);

    // Drawn through its own: the frame its body shows.
    let visibility = match seen {
        Some(skin) => {
            wear(&mut part, skin);
            Visibility::Inherited
        }
        None => Visibility::Hidden,
    };

    part.insert((visibility, own));

    if let Some(apart) = apart {
        part.insert(apart);
    }

    if let Some(grounded) = grounded {
        part.insert(grounded);
    }
}

/// A layer whose frame's tile the pick reads (`select::Picker`): a
/// first-pass body's.
#[derive(Component)]
pub struct Hits;

/// Where a drawing's part stands about its piece at `place`: a
/// second-pass part, a flier's shadow, on the `ground` under it; any
/// part but the first at its pose offset.
fn stands(drawing: &Drawing, place: Vec3, ground: Vec2) -> Vec2 {
    drawing.pass.height(Vec2::ZERO, ground - place.truncate()) + drawing.posed()
}

#[cfg(test)]
pub(crate) mod tests {
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
        // Asked, then its pages landed.
        loop {
            let made = app
                .world_mut()
                .run_system_once(
                    move |corpus: Res<Corpus>,
                          mut sprites: ResMut<Sprites>,
                          mut stores: Assets2| {
                        sprites
                            .piece(&corpus, &mut stores, piece)
                            .map(<[Drawing]>::to_vec)
                    },
                )
                .expect("made");

            if let Some(made) = made {
                break made;
            }

            std::thread::yield_now();
            app.world_mut()
                .run_system_once(regent_draw::art::world::land)
                .expect("landed");
        }
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

    /// A drawing app with [`pieces`] alone, showing `shown`, updated
    /// until its sprite pages land.
    fn drawing(corpus: Corpus, shown: Shown) -> App {
        const MOST: u32 = 5_000;

        let mut app = drawings(corpus, vec![shown]);

        for _ in 0..MOST {
            if drawing_of(&app, shown.agent).is_some() {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
        }

        app
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
            .add_systems(
                Update,
                (
                    crate::map::mirror::sync,
                    regent_draw::map::reel::step,
                    regent_draw::art::world::land,
                    pieces,
                )
                    .chain(),
            );
        app.update();
        app
    }

    /// The frame agent `agent`'s drawing shows at game time `ms`.
    fn drawn_frame(app: &App, agent: u32) -> Option<u16> {
        let world = app.world();
        let (entity, _) = drawing_of(app, agent)?;

        world
            .get::<Children>(entity)?
            .iter()
            .filter_map(|c| world.get::<Layer>(c))
            .find(|l| l.role == Role::Body)
            .and_then(Layer::up)
    }

    /// Updates of `app` until agent 1's drawing draws `key`, at most
    /// `most`: its sprite pages land.
    fn landed_key(app: &mut App, key: Key, most: u32) {
        for _ in 0..most {
            if drawing_of(app, 1).is_some_and(|(_, k)| k == key) {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
        }
    }

    /// An attack whose pages land late draws the frame the attack is on
    /// when they land, not its first; while they are on their way the
    /// drawing up keeps its key and its frame. Each set's pages here are
    /// pending at first: a fresh art service.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn an_attack_landing_late_draws_its_frame_now() {
        // Game ms the attack started at, and the frames it is in when
        // its pages land.
        const SINCE: u32 = 50_000;
        const INTO: u32 = 3;
        const MOST: u32 = 5_000;

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let unit = corpus
            .catalogue
            .tagged(b"DUNT")
            .filter_map(|(at, _)| regent_core::content::unit::Unit::at(&corpus.catalogue, at).ok())
            .find(|u| {
                u.animation(SetKind::Attack)
                    .and_then(|a| a.poses.first())
                    .is_some_and(|p| p.frames.len() > (INTO + 1) as usize)
            })
            .expect("a unit with a long attack");
        let attack = unit.animation(SetKind::Attack).expect("its attack").id;
        let standing = Shown {
            code: unit.id,
            ..piece((0, 0), 0)
        };
        let attacking = Shown {
            doing: Doing::Acting,
            look: Some(Look::Plays {
                set: attack,
                since: SINCE,
            }),
            ..standing
        };
        let stand = key(&corpus, &Saved::default(), &standing).expect("standing art");
        let strike = key(&corpus, &Saved::default(), &attacking).expect("attack art");
        let piece = art::world::Piece {
            code: strike.0,
            set: strike.1,
            slot: strike.2,
            still: None,
        };
        let ms = made(
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus"),
            piece,
        )[0]
        .ms;
        let mut app = drawings(corpus, vec![standing]);

        assert!(drawing_of(&app, 1).is_none(), "nothing drawn while pending");

        app.world_mut().resource_mut::<Reel>().ms = SINCE;
        landed_key(&mut app, stand, MOST);

        let held = drawn_frame(&app, 1);

        assert!(held.is_some(), "standing drawn once landed");

        // The attack starts: its pages pending, the stand stays, held.
        app.insert_resource(Snapshot::of(Steps::showing(vec![attacking])));
        app.update();

        assert_eq!(drawing_of(&app, 1).map(|(_, k)| k), Some(stand));
        assert_eq!(drawn_frame(&app, 1), held);

        // Landing INTO frames on, the attack shows frame INTO.
        app.world_mut().resource_mut::<Reel>().ms = SINCE + INTO * ms + ms / 2;
        landed_key(&mut app, strike, MOST);

        assert_eq!(drawing_of(&app, 1).map(|(_, k)| k), Some(strike));
        assert_eq!(
            drawn_frame(&app, 1),
            Some(u16::try_from(INTO).expect("small"))
        );
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
                let drawn = drawn_frame(&app, shown.agent);

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

    /// A frame of a layer, its rectangle at `x`, with a band where
    /// `banded`.
    fn cel(x: u32, banded: bool) -> Cel {
        Cel {
            body: Picture::Indexed(Handle::default()),
            band: banded.then(Handle::default),
            aloft: None,
            lift: None,
            size: Vec2::new(4.0, 6.0),
            anchor: Vec2::new(1.0, 2.0),
            tile: (0, x as usize),
            frame: regent_draw::instance::Framed(regent_core::view::pack::Rect {
                x,
                y: 0,
                w: 4,
                h: 6,
            }),
        }
    }

    /// A layer of `frames` frames, 100 ms each, from rectangle x `from`;
    /// banded where `banded`; in `pass`.
    pub(crate) fn layer(frames: u32, from: u32, banded: bool, pass: Pass) -> Drawing {
        Drawing {
            cels: (from..from + frames)
                .map(|x| Some(cel(x, banded)))
                .collect(),
            pose: Vec2::ZERO,
            layer: 0,
            right: 0.0,
            ms: 100,
            nudge: 0,
            pass,
            part: usize::from(pass == Pass::Second),
        }
    }

    /// `layers` laid on a root in `world` over `old`'s entities, from
    /// game time `now`, held on `pin`; the root and its strata.
    fn laid(
        world: &mut World,
        layers: Vec<Drawing>,
        (root, old): (Option<Entity>, Strata),
        (now, pin): (u32, Option<u16>),
    ) -> (Entity, Strata) {
        use bevy::ecs::system::RunSystemOnce;

        world
            .run_system_once(move |mut commands: Commands| {
                let root = root.unwrap_or_else(|| commands.spawn(Transform::default()).id());
                let timing = Timing {
                    stride: None,
                    from: now,
                    pin,
                };
                let strata = lay(
                    &mut commands,
                    (&Handle::default(), root),
                    (&layers, &old.0),
                    ((Vec3::ZERO, Vec2::ZERO), timing, now),
                );

                (root, strata)
            })
            .expect("laid")
    }

    /// The drawn entities in `world`: roots and layers.
    fn count(world: &mut World) -> usize {
        world
            .query_filtered::<Entity, With<Transform>>()
            .iter(world)
            .count()
    }

    /// A drawing is its root and, for each layer, a body and a band where
    /// a frame casts one: no entity a frame.
    #[test]
    fn a_drawings_entities_are_its_layers_and_their_bands() {
        let mut world = World::new();
        let layers = vec![
            layer(14, 0, false, Pass::First),
            layer(14, 20, true, Pass::Second),
        ];
        let (root, strata) = laid(&mut world, layers, (None, Strata::default()), (0, None));

        assert_eq!(count(&mut world), 1 + 2 + 1);
        assert_eq!(strata.0.len(), 2);
        assert!(strata.0[0].band.is_none() && strata.0[1].band.is_some());
        assert_eq!(world.get::<Children>(root).map(Children::len), Some(3));
    }

    /// A turn or a new action: the same entities, given the new layers'
    /// frames, each opening on frame 0 at its rate; nothing spawned.
    #[test]
    fn a_turn_spawns_nothing() {
        let mut world = World::new();
        let banded = |from| vec![layer(4, from, true, Pass::First)];
        let (root, old) = laid(&mut world, banded(0), (None, Strata::default()), (0, None));
        let before = count(&mut world);
        let (_, now) = laid(
            &mut world,
            banded(10),
            (Some(root), old.clone()),
            (1_250, None),
        );
        let after = count(&mut world);
        let body = world.get::<Layer>(now.0[0].body).expect("a body");

        assert_eq!(after, before);
        assert_eq!(now, old);
        assert_eq!(body.cels()[0].as_ref().map(|c| c.tile), Some((0, 10)));
        assert_eq!(
            (body.up(), body.due(1_349), body.due(1_350)),
            (Some(0), 0, 1)
        );
    }

    /// A layer count or band that differs adds or takes entities only
    /// for what differs.
    #[test]
    fn a_new_key_adds_or_takes_only_what_differs() {
        let mut world = World::new();
        let two = vec![
            layer(4, 0, true, Pass::First),
            layer(4, 10, false, Pass::First),
        ];
        let (root, old) = laid(&mut world, two, (None, Strata::default()), (0, None));
        let one = vec![layer(6, 20, false, Pass::First)];
        let (_, now) = laid(&mut world, one, (Some(root), old.clone()), (0, None));

        assert_eq!(
            now.0,
            [Stratum {
                body: old.0[0].body,
                band: None,
                lift: None
            }]
        );
        assert_eq!(count(&mut world), 1 + 1);
    }

    /// A held frame holds, whatever the clock; a pin moved on shows the
    /// new frame.
    #[test]
    fn a_held_frame_holds() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        let (_, strata) = laid(
            &mut world,
            vec![layer(6, 0, false, Pass::First)],
            (None, Strata::default()),
            (500, Some(3)),
        );
        let body = strata.0[0].body;

        world.init_resource::<Reel>();

        for ms in [500, 5_000, 50_000] {
            world.resource_mut::<Reel>().ms = ms;
            world
                .run_system_once(regent_draw::map::reel::step)
                .expect("stepped");

            assert_eq!(
                world.get::<Layer>(body).and_then(Layer::up),
                Some(3),
                "at {ms}"
            );
        }
    }

    /// A flying missile's shadow stays on the ground under it: its anchor
    /// stood where `lay` stands it, its frame's quad moved with it.
    #[test]
    fn a_flying_missiles_drawing_follows_it() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        let layers = vec![
            layer(4, 0, false, Pass::First),
            layer(4, 10, true, Pass::Second),
        ];
        let (missile, strata) = laid(
            &mut world,
            layers.clone(),
            (None, Strata::default()),
            (0, None),
        );
        let footing = Footing {
            row: 0.0,
            ground: 0.0,
        };

        world.entity_mut(missile).insert((
            Visibility::Inherited,
            footing,
            Tied::new(Who(Class::Missile, 7)),
            strata.clone(),
        ));

        let (place, ground) = (Vec3::new(10.0, 50.0, -1.0), Vec2::new(10.0, 20.0));
        let now = Footing {
            row: 3.0,
            ground: 20.0,
        };

        world
            .run_system_once(
                move |mut flying: Query<Flying, Without<Grounded>>,
                      mut shadows: Query<(&Grounded, Stepped)>| {
                    let moved = (place, ground, Visibility::Hidden, now);

                    fly(&mut flying, &mut shadows, missile, moved);
                },
            )
            .expect("flown");

        let shadow = strata.0[1].body;
        let anchor = layers[1].cels[0].as_ref().expect("a frame").anchor;

        assert_eq!(
            world.get::<Transform>(missile).map(|t| t.translation),
            Some(place)
        );
        assert_eq!(world.get::<Visibility>(missile), Some(&Visibility::Hidden));
        assert_eq!(world.get::<Footing>(missile), Some(&now));

        for part in [shadow, strata.0[1].band.expect("a band")] {
            assert_eq!(
                world.get::<Transform>(part).map(|t| t.translation),
                Some((stands(&layers[1], place, ground) - anchor).extend(LAYER_STEP))
            );
        }
    }

    /// A missile whose pages land mid-flight is first drawn at its place
    /// and frame then, not its launch's: nothing before.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_missile_landing_late_draws_its_place_and_frame_now() {
        const LAUNCHED: u32 = 50_000;
        const INTO: u32 = 5;
        const MOST: u32 = 5_000;

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let open = || {
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus")
        };
        let corpus = open();
        let flown = |at: [i32; 3]| Flown {
            handle: 7,
            code: FourCc(*b"WPa2"),
            at: at.map(fixed::from_whole),
            facing: 0,
            launched: LAUNCHED,
        };
        let key = missile_key(&corpus, &flown([0, 0, 0])).expect("a missile");
        let piece = art::world::Piece {
            code: key.0,
            set: key.1,
            slot: key.2,
            still: None,
        };
        let drawing = made(open(), piece)[0].clone();
        let frames = u32::try_from(drawing.cels.len()).expect("a few");
        let mut app = App::new();
        let mut steps = Steps::showing(Vec::new());

        steps.flown = vec![flown([5, 6, 1])];
        app.insert_resource(Snapshot::of(steps))
            .insert_resource(Maps::with(Vec::new()))
            .insert_resource(corpus)
            .insert_resource(Reel {
                ms: LAUNCHED + INTO * drawing.ms + drawing.ms / 2,
            })
            .init_resource::<Sprites>()
            .init_resource::<Shots>()
            .init_resource::<fog::Fog>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<regent_draw::plain::Plain>>()
            .init_resource::<Assets<material::Body>>()
            .init_resource::<Assets<material::Band>>()
            .add_systems(
                Update,
                (
                    regent_draw::map::reel::step,
                    regent_draw::art::world::land,
                    missiles,
                )
                    .chain(),
            );
        app.update();

        assert!(
            app.world().resource::<Shots>().drawn.is_empty(),
            "nothing drawn while pending"
        );

        for _ in 0..MOST {
            if !app.world().resource::<Shots>().drawn.is_empty() {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
        }

        let shot = app.world().resource::<Shots>().drawn[&7];
        let world = app.world();
        let place = world.get::<Transform>(shot.entity).expect("placed");
        let body = world
            .get::<Children>(shot.entity)
            .expect("parts")
            .iter()
            .filter_map(|c| world.get::<Layer>(c))
            .find(|l| l.role == Role::Body)
            .and_then(Layer::up);
        let maps = world.resource::<Maps>();

        assert_eq!(
            place.translation.truncate(),
            maps.aloft_at(Vec2::new(5.0, 6.0), 1.0)
        );
        assert_eq!(body, u16::try_from(INTO % frames).ok());
    }

    #[test]
    fn a_piece_is_drawn_between_its_last_two_places() {
        assert_eq!(cell((64, 32), Some(&(32, 32)), 0.5), Vec2::new(1.5, 1.0));
        assert_eq!(cell((64, 32), None, 0.5), Vec2::new(2.0, 1.0));
    }
}
