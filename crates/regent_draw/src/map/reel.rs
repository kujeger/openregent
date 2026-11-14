//! The map's clock, and each layer of a drawing stepping on it: one
//! entity a layer and role, for the life of the drawing.
//!
//! ```text
//!  a drawing's layer ──> a body entity, a band entity where any frame
//!                        darkens, a lift entity where any frame is
//!                        translucent; each a Layer: the frames (Cel),
//!                        their Beat, its Role
//!  step, each frame, a layer that runs (Runs: more than one frame, not
//!        held) ──the frame up at Reel::ms changed──> its entity's
//!        Transform (the frame's size and anchor), Framed (its page
//!        rectangle: `instance::write` writes the row), Visibility (hidden
//!        where the frame draws nothing in the role), its material (only
//!        where it differs), a tree's lean and bounds
//! ```
//!
//! Nothing is spawned or hidden to animate. A material made over for an
//! entity (the fog's darkening, draw effects, a cast model's band) is its
//! [`Own`], frame by frame, in place of the frames'.

use std::sync::Arc;

use bevy::asset::Asset;
use bevy::camera::primitives::Aabb;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;

use regent_core::data::desc::UnitKind;
#[cfg(doc)]
use regent_core::sim::movement::action::frame_ms;

use super::cel::{Cel, Picture};
use crate::instance::{Framed, Lean};
use crate::material::{Band, Body};
use crate::plain::Plain;

/// Whether a thing of this kind cycles its frames: buildings, scatter and
/// effects do; characters hold their pose, as a save gives position and
/// facing, not stride.
///
/// Kind from the descriptor, not the chunk: `VHCL` also holds spell auras
/// and `Got_Gold`'s coin (`Overlay`).
pub(super) const fn cycles(kind: Option<UnitKind>) -> bool {
    !matches!(kind, Some(UnitKind::Character))
}

/// The map's clock, game ms: openregent sets it from its game.
#[derive(Resource, Default)]
pub struct Reel {
    /// Game ms elapsed.
    pub ms: u32,
}

/// A frame time that never passes: a held frame.
const HELD: u32 = u32::MAX;

/// How a layer's frames step: frame `start` up at game time `from`,
/// each the next `ms` later, round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Beat {
    /// The frame up at `from`: the one a save caught it on, or held.
    pub start: u16,
    /// Game time `start` is up at.
    pub from: u32,
    /// Game ms each frame shows: [`frame_ms`] of its set.
    pub ms: u32,
}

impl Beat {
    /// Frame 0 up at game time `from`, each frame `ms`: a set put on.
    #[must_use]
    pub const fn runs(from: u32, ms: u32) -> Beat {
        Beat { start: 0, from, ms }
    }

    /// Frame `frame` held from game time `from`.
    #[must_use]
    pub const fn holds(frame: u16, from: u32) -> Beat {
        Beat {
            start: frame,
            from,
            ms: HELD,
        }
    }

    /// Which of `frames` frames is up at game time `now`.
    #[must_use]
    pub fn up(self, now: u32, frames: usize) -> u16 {
        let frames = u32::try_from(frames).unwrap_or(u32::MAX).max(1);
        let tick = now.wrapping_sub(self.from) / self.ms.max(1);
        let up = u32::from(self.start).wrapping_add(tick) % frames;

        u16::try_from(up).unwrap_or(u16::MAX)
    }
}

/// Which of a frame's materials a layer entity draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Its colours ([`Cel::body`]).
    Body,
    /// Its shadow ([`Cel::band`]).
    Band,
    /// A translucent frame's lift ([`Cel::lift`]).
    Lift,
}

/// One layer of a drawing, as one entity draws it in its [`Role`]: its
/// frames, how they step, and where its anchor stands.
#[derive(Component, Clone)]
pub struct Layer {
    cels: Arc<[Option<Cel>]>,
    pub role: Role,
    pub beat: Beat,
    /// Where its anchor stands about its parent (or on the map): a
    /// frame's quad is its size, its middle the anchor's offset from here.
    pub at: Vec2,
    /// The frame last written; `None` before the first.
    up: Option<u16>,
}

/// On a layer whose frames step with the clock: more than one, not
/// held. [`step`] visits these alone; a still one (most of a late
/// game's scatter) changes only as it is dressed again.
#[derive(Component)]
pub struct Runs;

/// A layer entity's materials made over, frame by frame, in place of its
/// frames' own (the fog's darkening, draw effects, a cast model's band).
/// A body's `None` draws its frame's own; a lift's or a band's draws
/// nothing.
#[derive(Component, Clone)]
pub struct Own<M: Asset>(pub Arc<[Option<Handle<M>>]>);

/// A tree layer's lean and bounds, frame by frame ([`crate::wind`]).
#[derive(Component, Clone)]
pub struct Sways(pub Arc<[(Lean, Aabb)]>);

/// What a layer entity draws on one frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Skin {
    Body(Handle<Body>),
    Plain(Handle<Plain>),
    Band(Handle<Band>),
}

/// The made-over materials a skin is read through.
pub type Owned<'a> = (Option<&'a Own<Body>>, Option<&'a Own<Band>>);

impl Layer {
    /// `cels` drawn in `role`, stepping by `beat`, its anchor at `at`.
    #[must_use]
    pub const fn new(cels: Arc<[Option<Cel>]>, role: Role, beat: Beat, at: Vec2) -> Layer {
        Layer {
            cels,
            role,
            beat,
            at,
            up: None,
        }
    }

    /// The same frames, beat and place, drawn in `role`.
    #[must_use]
    pub fn as_role(&self, role: Role) -> Layer {
        Layer {
            role,
            up: None,
            ..self.clone()
        }
    }

    /// Its frames, in order.
    #[must_use]
    pub fn cels(&self) -> &[Option<Cel>] {
        &self.cels
    }

    /// The frame last written.
    #[must_use]
    pub const fn up(&self) -> Option<u16> {
        self.up
    }

    /// The frame last written, if it has a picture.
    #[must_use]
    pub fn cel(&self) -> Option<&Cel> {
        self.up
            .and_then(|i| self.cels.get(usize::from(i)))
            .and_then(Option::as_ref)
    }

    /// Whether its frames step with the clock: more than one, not held.
    #[must_use]
    pub fn runs(&self) -> bool {
        self.cels.len() > 1 && self.beat.ms != HELD
    }

    /// The frame up at game time `now`.
    #[must_use]
    pub fn due(&self, now: u32) -> u16 {
        self.beat.up(now, self.cels.len())
    }

    /// What frame `i` draws in its role, read through `own`; `None` where
    /// it draws nothing.
    #[must_use]
    pub fn skin(&self, i: u16, (body, band): Owned) -> Option<Skin> {
        let at = usize::from(i);
        let cel = self.cels.get(at)?.as_ref()?;
        let owned = |own: Option<&Own<Body>>| own.map(|o| o.0.get(at).cloned().flatten());

        match self.role {
            Role::Body => match (owned(body).flatten(), &cel.body) {
                (Some(made), _) => Some(Skin::Body(made)),
                (None, Picture::Indexed(body)) => Some(Skin::Body(body.clone())),
                (None, Picture::Colour(colour)) => Some(Skin::Plain(colour.clone())),
            },
            Role::Lift => owned(body)
                .unwrap_or_else(|| cel.lift.clone())
                .map(Skin::Body),
            Role::Band => band
                .map_or_else(|| cel.band.clone(), |o| o.0.get(at).cloned().flatten())
                .map(Skin::Band),
        }
    }

    /// Whether any frame draws in its role: a band or lift entity is made
    /// only then.
    #[must_use]
    pub fn draws(&self) -> bool {
        (0..self.cels.len()).any(|i| {
            let i = u16::try_from(i).unwrap_or(u16::MAX);

            self.skin(i, (None, None)).is_some()
        })
    }

    /// The frame its geometry and material are first taken from: frame
    /// `i` where it draws, else the first that does.
    fn first(&self, i: u16) -> Option<u16> {
        let draws = |i: &u16| self.skin(*i, (None, None)).is_some();

        Some(i).filter(draws).or_else(|| {
            (0..self.cels.len())
                .filter_map(|i| u16::try_from(i).ok())
                .find(draws)
        })
    }

    /// Frame `i`'s place, size and rectangle at depth `z`.
    fn placed(&self, i: u16, z: f32) -> Option<(Transform, Framed)> {
        let cel = self.cels.get(usize::from(i))?.as_ref()?;
        let put = Transform {
            translation: (self.at - cel.anchor).extend(z),
            scale: cel.size.extend(1.0),
            ..default()
        };

        Some((put, cel.frame))
    }
}

/// `skin` put on `entity`, the other kinds' materials taken off.
pub fn wear(entity: &mut EntityCommands, skin: Skin) {
    match skin {
        Skin::Body(body) => {
            entity
                .remove::<MeshMaterial3d<Plain>>()
                .insert(MeshMaterial3d(body));
        }
        Skin::Plain(plain) => {
            entity
                .remove::<MeshMaterial3d<Body>>()
                .insert(MeshMaterial3d(plain));
        }
        Skin::Band(band) => {
            entity.insert(MeshMaterial3d(band));
        }
    }
}

/// `layer` put on `entity` at depth `z`, showing the frame up at game
/// time `now`: its place, rectangle, visibility and material. A new
/// entity, or one given new frames or a new beat.
pub fn dress(entity: &mut EntityCommands, mut layer: Layer, now: u32, z: f32) {
    let up = layer.due(now);
    let shows = layer.skin(up, (None, None));
    let first = layer.first(up);
    let placed = first.and_then(|i| layer.placed(i, z));
    let skin = shows
        .clone()
        .or_else(|| first.and_then(|i| layer.skin(i, (None, None))));
    let seen = match shows {
        Some(_) => Visibility::Inherited,
        None => Visibility::Hidden,
    };

    layer.up = Some(up);

    // Stepped by the clock only where it runs.
    if layer.runs() {
        entity.insert(Runs);
    } else {
        entity.remove::<Runs>();
    }

    entity.insert((layer, seen));

    if let Some(placed) = placed {
        entity.insert(placed);
    }

    if let Some(skin) = skin {
        wear(entity, skin);
    }
}

/// What [`step`] reads and writes of a layer entity.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct Stepped {
    pub entity: Entity,
    pub layer: &'static mut Layer,
    pub put: &'static mut Transform,
    pub frame: &'static mut Framed,
    pub seen: &'static mut Visibility,
    pub body: Option<&'static mut MeshMaterial3d<Body>>,
    pub band: Option<&'static mut MeshMaterial3d<Band>>,
    pub plain: Option<&'static MeshMaterial3d<Plain>>,
    pub own: (Option<&'static Own<Body>>, Option<&'static Own<Band>>),
    pub sways: (
        Option<&'static Sways>,
        Option<&'static mut Lean>,
        Option<&'static mut Aabb>,
    ),
}

impl SteppedItem<'_, '_> {
    /// The frame up at game time `now` written, if another than the last.
    pub fn step(&mut self, now: u32, commands: &mut Commands) {
        let up = self.layer.due(now);

        if self.layer.up != Some(up) {
            self.show(up, commands);
        }
    }

    /// The frame up at game time `now` written whatever was last: after
    /// its [`Own`] or its beat changed.
    pub fn redo(&mut self, now: u32, commands: &mut Commands) {
        let up = self.layer.due(now);

        self.show(up, commands);
    }

    /// Frame `up` written: what changed of its place and rectangle, its
    /// visibility and its material.
    fn show(&mut self, up: u16, commands: &mut Commands) {
        let skin = self.layer.skin(up, self.own);
        let cel = self.layer.cels.get(usize::from(up)).cloned().flatten();

        self.layer.up = Some(up);

        // A frame with no picture keeps the last one's place, hidden.
        if let Some(cel) = cel {
            let next = Transform {
                translation: (self.layer.at - cel.anchor).extend(self.put.translation.z),
                scale: cel.size.extend(1.0),
                ..*self.put
            };

            self.put.set_if_neq(next);
            self.frame.set_if_neq(cel.frame);
            self.lean(up);
        }

        let seen = match skin {
            Some(_) => Visibility::Inherited,
            None => Visibility::Hidden,
        };

        self.seen.set_if_neq(seen);

        if let Some(skin) = skin {
            self.wear(skin, commands);
        }
    }

    /// Its anchor stood at `at`: the quad of the frame up moved, its
    /// depth kept.
    pub fn stand(&mut self, at: Vec2) {
        if self.layer.at != at {
            self.layer.at = at;
        }

        if let Some(anchor) = self.layer.cel().map(|cel| cel.anchor) {
            let z = self.put.translation.z;

            self.put.set_if_neq(Transform {
                translation: (at - anchor).extend(z),
                ..*self.put
            });
        }
    }

    /// The frame up drawn through `own`: after the caller put it on, or
    /// took it off, by command.
    pub fn repaint(&mut self, own: Owned, commands: &mut Commands) {
        if let Some(up) = self.layer.up {
            let skin = self.layer.skin(up, own);
            let seen = match skin {
                Some(_) => Visibility::Inherited,
                None => Visibility::Hidden,
            };

            self.seen.set_if_neq(seen);

            if let Some(skin) = skin {
                self.wear(skin, commands);
            }
        }
    }

    /// A tree's lean and bounds for frame `up`.
    fn lean(&mut self, up: u16) {
        let (sways, lean, bounds) = &mut self.sways;
        let at = sways.and_then(|s| s.0.get(usize::from(up)));

        if let (Some((now, aabb)), Some(lean), Some(bounds)) = (at, lean, bounds) {
            lean.set_if_neq(*now);

            if bounds.center != aabb.center || bounds.half_extents != aabb.half_extents {
                **bounds = *aabb;
            }
        }
    }

    /// `skin` drawn: written where it differs, the material's kind
    /// changed by command (a direct-colour frame among indexed ones).
    fn wear(&mut self, skin: Skin, commands: &mut Commands) {
        match (skin, &mut self.body, &mut self.band, self.plain) {
            (Skin::Body(want), Some(body), ..) => {
                if body.0 != want {
                    body.0 = want;
                }
            }
            (Skin::Band(want), _, Some(band), _) => {
                if band.0 != want {
                    band.0 = want;
                }
            }
            (Skin::Plain(want), _, _, Some(plain)) if plain.0 == want => {}
            (skin, ..) => wear(&mut commands.entity(self.entity), skin),
        }
    }
}

/// Each running layer's frame up at the clock's time written where it
/// changed.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn step(mut commands: Commands, reel: Res<Reel>, mut layers: Query<Stepped, With<Runs>>) {
    for mut layer in &mut layers {
        layer.step(reel.ms, &mut commands);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use bevy::ecs::system::RunSystemOnce;
    use regent_core::view::pack::Rect;

    /// Which frame of six a beat from `start` shows at `ms`, 100 ms each.
    fn up(start: u16, ms: u32) -> u16 {
        Beat {
            start,
            from: 0,
            ms: 100,
        }
        .up(ms, 6)
    }

    #[test]
    fn a_cycle_opens_on_the_frame_the_save_caught_it_on() {
        assert_eq!(up(4, 0), 4, "the frame the save named");
        assert_eq!(up(4, 99), 4, "held its set's 100 ms");
        assert_eq!(up(4, 100), 5);
        assert_eq!(up(4, 200), 0, "and round");
        assert_eq!(up(0, 200), 2);
    }

    #[test]
    fn two_cycles_of_one_kind_keep_their_own_phase() {
        for tick in 0..12 {
            assert_ne!(up(1, tick * 100), up(4, tick * 100), "at tick {tick}");
        }
    }

    /// Every corpus object is inside its slot, so this is a guard.
    #[test]
    fn a_start_past_the_stream_still_lands_on_a_frame() {
        let beat = Beat {
            start: 100,
            from: 0,
            ms: 1,
        };

        assert_eq!(beat.up(0, 3), 1, "100 % 3 is 1");
    }

    #[test]
    fn a_new_set_opens_on_frame_0_and_steps_at_its_rate() {
        let now = 1_250;
        let beat = Beat::runs(now, 100);

        assert_eq!(beat.up(now, 6), 0);
        assert_eq!(
            beat.up(now + 50, 6),
            0,
            "a 100 ms frame spans two game frames"
        );
        assert_eq!(beat.up(now + 100, 6), 1);
        assert_eq!(beat.up(now + 600, 6), 0, "and round");
    }

    #[test]
    fn a_held_frame_holds() {
        let beat = Beat::holds(2, 500);

        assert!((500..100_000).step_by(37).all(|now| beat.up(now, 4) == 2));
        assert_eq!(Beat::holds(6, 0).up(9, 4), 2, "a frame past the stream");
    }

    /// A frame of `w` by `h`, its anchor `anchor` from its middle, at
    /// rectangle x `x`; with a band where `banded`.
    fn cel(size: (f32, f32), anchor: Vec2, x: u32, band: Option<Handle<Band>>) -> Cel {
        Cel {
            body: Picture::Indexed(Handle::default()),
            band,
            aloft: None,
            lift: None,
            size: Vec2::new(size.0, size.1),
            anchor,
            tile: (0, x as usize),
            frame: Framed(Rect {
                x,
                y: 0,
                w: 1,
                h: 1,
            }),
        }
    }

    /// Four frames, 100 ms each, from game time 0: frame 2 without a
    /// picture, frame 3 without a band.
    fn frames() -> Arc<[Option<Cel>]> {
        let band = Some(Handle::<Band>::default());

        Arc::from([
            Some(cel((4.0, 6.0), Vec2::new(1.0, 2.0), 10, band.clone())),
            Some(cel((8.0, 2.0), Vec2::new(-3.0, 0.5), 20, band)),
            None,
            Some(cel((5.0, 5.0), Vec2::ZERO, 30, None)),
        ])
    }

    /// A world with a body and a band entity of [`frames`], at `at`,
    /// dressed at game time 0; and the clock.
    fn drawn(at: Vec2) -> (World, Entity, Entity) {
        let mut world = World::new();

        world.init_resource::<Reel>();

        let (body, band) = world
            .run_system_once(move |mut commands: Commands| {
                let beat = Beat::runs(0, 100);
                let made = |role| Layer::new(frames(), role, beat, at);
                let mut body = commands.spawn_empty();

                dress(&mut body, made(Role::Body), 0, 0.5);

                let body = body.id();
                let mut band = commands.spawn_empty();

                dress(&mut band, made(Role::Band), 0, 0.5);
                (body, band.id())
            })
            .expect("dressed");

        (world, body, band)
    }

    /// `world` stepped at game time `ms`.
    fn stepped(world: &mut World, ms: u32) {
        world.resource_mut::<Reel>().ms = ms;
        world.run_system_once(step).expect("stepped");
    }

    /// A frame step moves the quad to the frame's size and anchor and
    /// its rectangle, spawning nothing; a frame without a picture hides
    /// the layer and the next shows it again.
    #[test]
    fn a_frame_step_writes_its_frame_and_spawns_nothing() {
        let at = Vec2::new(100.0, 50.0);
        let (mut world, body, band) = drawn(at);
        let count = world.entities().count_spawned();
        let placed = |world: &World| {
            let put = world.get::<Transform>(body).expect("placed");

            (
                put.translation,
                put.scale,
                world.get::<Framed>(body).expect("framed").0.x,
            )
        };
        let seen = |world: &World, e| *world.get::<Visibility>(e).expect("seen");

        assert_eq!(
            placed(&world),
            (Vec3::new(99.0, 48.0, 0.5), Vec3::new(4.0, 6.0, 1.0), 10)
        );

        stepped(&mut world, 150);
        assert_eq!(
            placed(&world),
            (Vec3::new(103.0, 49.5, 0.5), Vec3::new(8.0, 2.0, 1.0), 20)
        );
        assert_eq!(seen(&world, body), Visibility::Inherited);

        stepped(&mut world, 250);
        assert_eq!(seen(&world, body), Visibility::Hidden, "no picture");
        assert_eq!(seen(&world, band), Visibility::Hidden);

        stepped(&mut world, 350);
        assert_eq!(seen(&world, body), Visibility::Inherited, "shown again");
        assert_eq!(seen(&world, band), Visibility::Hidden, "no band");
        assert_eq!(placed(&world).2, 30);

        stepped(&mut world, 450);
        assert_eq!(seen(&world, band), Visibility::Inherited, "round to a band");
        assert_eq!(world.entities().count_spawned(), count);
    }

    /// A frame step rewrites one instance row a layer entity: its
    /// rectangle changes on the body and the band alone.
    #[test]
    fn a_frame_step_writes_one_row_an_entity() {
        let (mut world, body, band) = drawn(Vec2::ZERO);

        world.clear_trackers();
        stepped(&mut world, 150);

        let mut changed = world.query_filtered::<Entity, Changed<Framed>>();
        let mut rows: Vec<Entity> = changed.iter(&world).collect();

        rows.sort();
        assert_eq!(rows, {
            let mut both = vec![body, band];
            both.sort();
            both
        });
    }

    /// A step that keeps the frame writes nothing: no change for the
    /// instance table or Bevy's transform pass to see.
    #[test]
    fn a_step_on_the_same_frame_writes_nothing() {
        let (mut world, body, _) = drawn(Vec2::ZERO);

        stepped(&mut world, 10);
        world.clear_trackers();
        stepped(&mut world, 60);

        let mut changed = world.query_filtered::<Entity, Or<(Changed<Transform>, Changed<Framed>, Changed<Visibility>)>>();

        assert_eq!(changed.iter(&world).count(), 0);
        assert_eq!(world.get::<Layer>(body).and_then(Layer::up), Some(0));
    }

    /// The clock steps a layer of more than one frame, not held; a still
    /// or held one is left alone.
    #[test]
    fn only_a_running_layer_is_stepped() {
        let layer =
            |frames: Arc<[Option<Cel>]>, beat| Layer::new(frames, Role::Body, beat, Vec2::ZERO);
        let one: Arc<[Option<Cel>]> = Arc::from([Some(cel((1.0, 1.0), Vec2::ZERO, 0, None))]);

        assert!(layer(frames(), Beat::runs(0, 100)).runs());
        assert!(!layer(frames(), Beat::holds(1, 0)).runs());
        assert!(!layer(one, Beat::runs(0, 100)).runs());
    }

    /// A band entity draws only where some frame has a band; a lift only
    /// where some frame is translucent.
    #[test]
    fn a_band_or_lift_is_made_only_where_a_frame_draws_one() {
        let beat = Beat::runs(0, 100);
        let layer = |role| Layer::new(frames(), role, beat, Vec2::ZERO);

        assert!(layer(Role::Body).draws());
        assert!(layer(Role::Band).draws());
        assert!(!layer(Role::Lift).draws());
    }

    /// A body's own materials draw in place of its frames'; a band's
    /// `None` draws nothing.
    #[test]
    fn an_own_material_draws_in_place_of_the_frames() {
        let mut bodies = Assets::<Body>::default();
        let dark = bodies.add(Body::default());
        let layer = |role| Layer::new(frames(), role, Beat::runs(0, 100), Vec2::ZERO);
        let own = Own(Arc::from([Some(dark.clone()), None, None, None]));
        let dropped = Own::<Band>(Arc::from([None, None, None, None]));

        assert_eq!(
            layer(Role::Body).skin(0, (Some(&own), None)),
            Some(Skin::Body(dark))
        );
        assert_eq!(
            layer(Role::Body).skin(1, (Some(&own), None)),
            Some(Skin::Body(Handle::default())),
            "its own frame's"
        );
        assert_eq!(layer(Role::Band).skin(0, (None, Some(&dropped))), None);
    }
}
