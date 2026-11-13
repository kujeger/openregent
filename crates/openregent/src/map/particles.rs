//! Particles drawn: each live particle of the game's systems
//! (`seen::Frame::motes`) a frame of its system's own art, at its own
//! place.
//!
//! ```text
//!  game frame ──> Frame::motes ──> one pooled entity each: its frame's
//!                                  body, and its band for the shadow pass;
//!                                  the pool's rest hidden
//! ```
//!
//! **Binary**: `IsoGenUnit` blits a particle as any unit's frame,
//! at its place projected as a lifted thing's, with no
//! scale or tint of its own; so `regent_draw`'s body and shadow pass draw
//! it, the tile's indices choosing as a piece's do. Its alpha below 1
//! draws mode 8 instead, translucent with no shadow
//! (`IsoUnitIMGPartEntry`, [`blend::fade`]): smoke fades in and
//! out. Its depth is its own row's: `IsoDrawList`
//! sorts a particle by its own place's y, as a unit by its own. The
//! engine shades each shadow as it draws it, so a particle draws over
//! the shadows of all further back; a fading one lifts the shadow under
//! it ([`regent_draw::soft::lifts`]), whose coverage resolves last.

use std::collections::HashMap;

use bevy::pbr::Material;
use bevy::prelude::*;

use regent_core::view::map::particles::Mote;
use regent_core::view::raster::blend;
use regent_draw::data::Corpus;
use regent_draw::instance::Framed;
use regent_draw::lie::Footing;
use regent_draw::map::Maps;
use regent_draw::map::cel::Picture;
use regent_draw::map::reel::Reel;
use regent_draw::material::{Band, Body};
use regent_draw::plain::Plain;
use regent_draw::wind::{self, Trees, Wind};

use crate::game::clock::Snapshot;
use crate::game::seen::Frame;
use crate::map::fog::Fog;
use crate::map::pieces::{Depth, low};
use regent_draw::art::world::{Piece, Variant};
use regent_draw::art::{Assets2, Sprites};

/// Each particle's place at a game frame, and its birth, by id
/// ([`Mote::id`]).
type Places = HashMap<(u32, usize), ([f32; 3], f32)>;

/// The particles drawn, and the game time they were drawn for; their
/// pictures are the art service's (`art::Sprites`).
#[derive(Resource, Default)]
pub struct Motes {
    /// Entities kept from frame to frame, each given a particle anew;
    /// those past the frame's particles hidden.
    pool: Vec<Slot>,
    /// The game time and [`Snapshot::between`] drawn for.
    at: Option<(u32, u32)>,
    /// The game time [`Motes::now`] is of, and the frame before's.
    ms: Option<u32>,
    now: Places,
    before: Places,
}

/// Draw every live particle where the game has it, redrawn each game
/// frame: the `Sim` steps them a frame at a time. With `[enhancements]
/// smooth_position`, slid from where it was a frame before, as a piece,
/// and redrawn each screen frame.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn motes(
    mut commands: Commands,
    frame: Res<Frame>,
    maps: Res<Maps>,
    corpus: Res<Corpus>,
    reel: Res<Reel>,
    fog: Res<Fog>,
    snapshot: Res<Snapshot>,
    mut motes: ResMut<Motes>,
    mut sprites: ResMut<Sprites>,
    mut stores: Assets2,
    (wind, trees): (Res<Wind>, Res<Trees>),
) {
    let at = (reel.ms, snapshot.between.to_bits());

    if motes.at != Some(at) {
        let depth = Depth::of(&maps);
        let live = &frame.motes;
        let mut used = 0;

        if motes.ms != Some(reel.ms) {
            motes.before = std::mem::take(&mut motes.now);
            motes.now = live.iter().map(|m| (m.id, (m.at, m.born))).collect();
            motes.ms = Some(reel.ms);
        }

        // Unseen ground hides a particle, by its own cell.
        for mote in live.iter().copied().filter(|m| !fog.hides_at(m.at)) {
            let mote = Mote {
                at: glide(&mote, motes.before.get(&mote.id), snapshot.between),
                ..mote
            };
            let look = look(&corpus, &maps, &depth, &mut sprites, &mut stores, mote)
                .map(|look| drifted(look, &mote, &wind, &trees, reel.ms));

            if let Some(look) = look {
                match motes.pool.get_mut(used) {
                    Some(slot) => slot.give(&mut commands, look),
                    None => motes.pool.push(Slot::spawn(&mut commands, look)),
                }

                used += 1;
            }
        }

        // The rest wait, hidden, for more particles.
        for slot in motes.pool.iter_mut().skip(used) {
            slot.hide(&mut commands);
        }

        motes.at = Some(at);
    }
}

/// A smoke puff's look moved east with the wind (`[enhancements] wind`,
/// [`regent_draw::wind::drift`]); any other, or no wind, as it is.
fn drifted(mut look: Look, mote: &Mote, wind: &Wind, trees: &Trees, ms: u32) -> Look {
    if wind.reach > 0.0 && wind::SMOKES.contains(&mote.code) {
        let t = wind::seconds(wind.held.unwrap_or(ms));

        look.place.x += wind::drift(t, mote.age, look.place.x, trees.wave, wind.reach);
    }

    look
}

/// A particle's place, `between` of the way from where it was a frame
/// before: only the same birth in its slot, a new one starts where it is.
fn glide(mote: &Mote, before: Option<&([f32; 3], f32)>, between: f32) -> [f32; 3] {
    match before {
        Some((from, born)) if born.to_bits() == mote.born.to_bits() => {
            [0, 1, 2].map(|i| from[i] + (mote.at[i] - from[i]) * between)
        }
        _ => mote.at,
    }
}

/// One particle as drawn: its parent's place and footing; its
/// frame's parts, each placed on the parent by `put`.
struct Look {
    place: Vec3,
    footing: Footing,
    put: Transform,
    body: Picture,
    lift: Option<Handle<Body>>,
    band: Option<Handle<Band>>,
    /// The quad its parts are drawn on, and its frame's texels.
    quad: (Handle<Mesh>, Framed),
}

/// One particle's look: its frame, placed by its anchor. `None` where
/// the art lacks the frame.
fn look(
    corpus: &Corpus,
    maps: &Maps,
    depth: &Depth,
    sprites: &mut Sprites,
    stores: &mut Assets2,
    mote: Mote,
) -> Option<Look> {
    let piece = Piece {
        code: mote.code,
        set: low(mote.set),
        slot: usize::from(mote.slot),
        still: None,
    };
    // The set a particle's part draws: layer 0 (`UnitIMGParticle`,
    // its set by whole id).
    let cel = sprites
        .piece(corpus, stores, piece)
        .iter()
        .find(|d| d.layer == 0)?
        .cels
        .get(usize::from(mote.frame))?
        .clone()?;

    // Fading: mode 8 at its level, no shadow; and its lift, clearing the
    // shadow under it, as the engine draws it over shadows drawn
    // before it. A frame drawn translucent has its own.
    let glass = blend::fade(mote.alpha).and_then(|steps| match &cel.body {
        Picture::Indexed(body) => {
            let made = (&mut *stores.images, &mut *stores.bodies);

            sprites.variant(corpus, made, body, Variant::Glow(steps))
        }
        Picture::Colour(_) => None,
    });
    let lift = match &glass {
        Some(glass) => {
            let made = (&mut *stores.images, &mut *stores.bodies);

            sprites.variant(corpus, made, glass, Variant::Lift)
        }
        None => cel.lift.clone(),
    };
    let [x, y, z] = mote.at;
    let ground = Vec2::new(x, y);
    let place = maps.aloft_at(ground, z).extend(depth.at(y));
    let footing = Footing {
        row: y,
        ground: maps.standing_at(ground).y,
    };
    let band = match glass {
        Some(_) => None,
        None => cel.band.clone(),
    };

    Some(Look {
        place,
        footing,
        put: Transform {
            translation: (-cel.anchor).extend(0.0),
            scale: cel.size.extend(1.0),
            ..default()
        },
        body: glass.map_or(cel.body, Picture::Indexed),
        lift,
        band,
        quad: (sprites.quad(&mut stores.meshes), cel.frame),
    })
}

/// A pooled particle's entities, and what each was last given: its
/// parent, its body, and its lift and band once one was asked. A part
/// not asked for since is hidden, not despawned.
struct Slot {
    parent: Entity,
    shown: bool,
    put: Transform,
    body: (Entity, Picture),
    lift: Option<Part<Body>>,
    band: Option<Part<Band>>,
    quad: (Handle<Mesh>, Framed),
}

/// A pooled part: its entity, and its material while shown.
struct Part<M: Material> {
    entity: Entity,
    shown: Option<Handle<M>>,
}

impl Slot {
    /// A new slot drawing `look`: a parent and its parts, as spawned
    /// before the pool.
    fn spawn(commands: &mut Commands, look: Look) -> Slot {
        let Look {
            place,
            footing,
            put,
            body,
            lift,
            band,
            quad,
        } = look;
        let shape = (&quad.0, quad.1);
        // `Visibility` too: its cels inherit it.
        let parent = commands
            .spawn((
                Transform::from_translation(place),
                Visibility::default(),
                footing,
            ))
            .id();
        let body_part = match &body {
            Picture::Indexed(body) => commands.spawn((
                Mesh3d(quad.0.clone()),
                quad.1,
                MeshMaterial3d(body.clone()),
                put,
                ChildOf(parent),
            )),
            Picture::Colour(colour) => commands.spawn((
                Mesh3d(quad.0.clone()),
                MeshMaterial3d(colour.clone()),
                put,
                ChildOf(parent),
            )),
        }
        .id();

        Slot {
            parent,
            shown: true,
            put,
            body: (body_part, body),
            lift: lift.map(|m| Part::spawn(commands, shape, (parent, put), m)),
            band: band.map(|m| Part::spawn(commands, shape, (parent, put), m)),
            quad: quad.clone(),
        }
    }

    /// The slot drawing `look` in place of what it drew.
    fn give(&mut self, commands: &mut Commands, look: Look) {
        let mut parent = commands.entity(self.parent);

        parent.insert((Transform::from_translation(look.place), look.footing));

        if !self.shown {
            parent.insert(Visibility::Inherited);
            self.shown = true;
        }

        let (entity, was) = &self.body;
        let mut body = commands.entity(*entity);

        // A material of the other kind takes the old one's place.
        match (was, &look.body) {
            (Picture::Indexed(was), Picture::Indexed(now)) if was == now => {}
            (Picture::Colour(was), Picture::Colour(now)) if was == now => {}
            (Picture::Indexed(_), Picture::Indexed(now)) => {
                body.insert(MeshMaterial3d(now.clone()));
            }
            (Picture::Colour(_), Picture::Colour(now)) => {
                body.insert(MeshMaterial3d(now.clone()));
            }
            (Picture::Colour(_), Picture::Indexed(now)) => {
                body.remove::<MeshMaterial3d<Plain>>()
                    .insert(MeshMaterial3d(now.clone()));
            }
            (Picture::Indexed(_), Picture::Colour(now)) => {
                body.remove::<MeshMaterial3d<Body>>()
                    .insert(MeshMaterial3d(now.clone()));
            }
        }

        // The parts share the frame's place and texels on the parent.
        if self.quad.1 != look.quad.1 {
            let lift = self.lift.as_ref().map(|p| p.entity);
            let band = self.band.as_ref().map(|p| p.entity);

            for part in std::iter::once(*entity).chain(lift).chain(band) {
                commands.entity(part).insert(look.quad.1);
            }

            self.quad.1 = look.quad.1;
        }

        if self.put != look.put {
            let lift = self.lift.as_ref().map(|p| p.entity);
            let band = self.band.as_ref().map(|p| p.entity);

            for part in std::iter::once(*entity).chain(lift).chain(band) {
                commands.entity(part).insert(look.put);
            }

            self.put = look.put;
        }

        self.body.1 = look.body;
        Part::give(
            &mut self.lift,
            commands,
            (&self.quad.0, self.quad.1),
            (self.parent, self.put),
            look.lift,
        );
        Part::give(
            &mut self.band,
            commands,
            (&self.quad.0, self.quad.1),
            (self.parent, self.put),
            look.band,
        );
    }

    /// The slot hidden, drawing nothing.
    fn hide(&mut self, commands: &mut Commands) {
        if self.shown {
            commands.entity(self.parent).insert(Visibility::Hidden);
            self.shown = false;
        }
    }
}

impl<M: Material> Part<M> {
    /// A part drawing `material` at `put` on `parent`.
    fn spawn(
        commands: &mut Commands,
        (quad, frame): (&Handle<Mesh>, Framed),
        (parent, put): (Entity, Transform),
        material: Handle<M>,
    ) -> Part<M> {
        let entity = commands
            .spawn((
                Mesh3d(quad.clone()),
                frame,
                MeshMaterial3d(material.clone()),
                put,
                ChildOf(parent),
            ))
            .id();

        Part {
            entity,
            shown: Some(material),
        }
    }

    /// `part` drawing `material`, made on first use; hidden without one.
    fn give(
        part: &mut Option<Part<M>>,
        commands: &mut Commands,
        quad: (&Handle<Mesh>, Framed),
        on: (Entity, Transform),
        material: Option<Handle<M>>,
    ) {
        match (part.as_mut(), material) {
            (Some(p), Some(m)) if p.shown.as_ref() == Some(&m) => {}
            (Some(p), Some(m)) => {
                commands
                    .entity(p.entity)
                    .insert((MeshMaterial3d(m.clone()), Visibility::Inherited));
                p.shown = Some(m);
            }
            (Some(p), None) => {
                if p.shown.take().is_some() {
                    commands.entity(p.entity).insert(Visibility::Hidden);
                }
            }
            (None, Some(m)) => *part = Some(Part::spawn(commands, quad, on, m)),
            (None, None) => {}
        }
    }
}

/// The particles drawn, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Motes>();
}

#[cfg(test)]
mod tests {
    use super::*;

    use regent_core::data::fourcc::FourCc;

    fn mote(at: [f32; 3], born: f32) -> Mote {
        Mote {
            id: (1, 0),
            born,
            age: 0.0,
            code: FourCc(*b"FXa1"),
            set: 0,
            slot: 0,
            frame: 0,
            at,
            alpha: 1.0,
        }
    }

    /// Half a frame on, a particle is halfway; one born since in its
    /// slot starts where it is.
    #[test]
    fn a_particle_slides_from_its_own_last_place() {
        let now = mote([2.0, 1.0, 2.0], 0.5);

        assert_eq!(
            Vec3::from(glide(&now, Some(&([1.0, 1.0, 0.0], 0.5)), 0.5)),
            Vec3::new(1.5, 1.0, 1.0)
        );
        assert_eq!(
            Vec3::from(glide(&now, Some(&([1.0, 1.0, 0.0], 0.25)), 0.5)),
            Vec3::new(2.0, 1.0, 2.0)
        );
        assert_eq!(Vec3::from(glide(&now, None, 0.5)), Vec3::new(2.0, 1.0, 2.0));
    }
}
