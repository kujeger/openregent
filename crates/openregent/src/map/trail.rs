//! Speed trails' clones (`World::clones`): each one shown drawn where it
//! stands, as its leader looked when it last ran, see-through at its
//! level. Hidden where its unit is unseen: a clone's holder chain ends at
//! its unit. Not darkened: a tinted piece keeps
//! its tint's palette.
//!
//! ```text
//!  draw   each game frame: one entity per shown clone, its leader's
//!         set and slot, its frames that far behind the unit's
//!  paint  a new clone's bodies made mode 5 at its level, the unit's
//!         colour; its shadow dropped (mode 5 draws nothing from 247)
//! ```

use std::collections::HashMap;

use bevy::prelude::*;

use regent_core::sim::movement::action::Look;
use regent_core::sim::world::{draw::Tint, trail::Showing};
use regent_draw::data::Corpus;
use regent_draw::map::Maps;
use regent_draw::map::reel::Reel;
use regent_draw::material::Body;

use crate::game::clock::Snapshot;
use crate::game::seen::Watch;
use crate::map::fog::Fog;
use crate::map::mirror::{Phase, Pieces};
use crate::map::pieces::{self, Coated, Depth, Lifts, Saved, Strata, Timing};
use regent_draw::art::{Assets2, Sprites};

/// A clone drawn: its entity, its unit's agent, its last two places in
/// cells, its leader's [`Shown::jumps`](regent_core::sim::world::draw_view::Shown::jumps).
type Drawn = (Entity, u32, Vec2, Vec2, u32);

/// The clones drawn, by handle; the game time they were drawn for.
#[derive(Resource, Default)]
pub struct Ghosts {
    drawn: HashMap<u32, Drawn>,
    ms: Option<u32>,
}

impl Ghosts {
    /// Drawn anew at the next frame, though its time is the last's: a
    /// LAN resync's world.
    pub const fn forget(&mut self) {
        self.ms = None;
    }
}

/// A clone's entity, still to be painted with its draw effects.
#[derive(Component)]
pub struct Ghost(Tint);

/// A clone's entity, painted.
#[derive(Component)]
pub struct Painted;

/// Draw each shown clone anew each game frame; between frames, slide it
/// from its last place as a piece slides ([`crate::game::clock::Motion`]), shown as
/// its unit is seen.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn draw(
    mut commands: Commands,
    seen: Watch,
    fog: Res<Fog>,
    snapshot: Res<Snapshot>,
    maps: Res<Maps>,
    corpus: Res<Corpus>,
    saved: Res<Saved>,
    reel: Res<Reel>,
    (mirrors, phases): (Pieces, Query<&Phase>),
    mut lifts: ResMut<Lifts>,
    mut sprites: ResMut<Sprites>,
    mut ghosts: ResMut<Ghosts>,
    mut moved: Query<(&mut Transform, &mut Visibility), With<Ghost>>,
    mut stores: Assets2,
) {
    let depth = Depth::of(&maps);

    if ghosts.ms != Some(reel.ms) {
        let world = seen.world();
        let mut pending = false;
        let was: HashMap<u32, (Vec2, u32)> = ghosts
            .drawn
            .drain()
            .map(|(handle, (entity, _, _, to, jumps))| {
                commands.entity(entity).despawn();
                (handle, (to, jumps))
            })
            .collect();

        for c in world.clones().iter().filter(|c| c.shown == Showing::Shown) {
            let shown = c.shows.shown;
            let Some(key) = pieces::key(&corpus, &saved, &shown) else {
                continue;
            };

            // Its frames run as far behind the unit's as its look is old.
            let lag = world.now().saturating_sub(c.shows.stamp);
            let from = match shown.look {
                Some(Look::Plays { set, since }) if pieces::low(set) == key.1 => since,
                _ => mirrors
                    .mirror_of(shown.agent)
                    .and_then(|m| phases.get(m).ok())
                    .and_then(|p| p.0)
                    .unwrap_or(reel.ms),
            };
            let timing = Timing {
                stride: pieces::stride(&shown, key),
                from: from.wrapping_add(lag),
                pin: pieces::pin(&shown, key),
            };
            let to = crate::ui::tracking::cells(shown.at);
            // A leader placed outright: no slide.
            let at = was
                .get(&c.handle)
                .filter(|(_, jumps)| *jumps == shown.jumps)
                .map_or(to, |(from, _)| *from);
            let lift = lifts.lift(&corpus, key.0, key.1);
            // Where the slide below puts it: stepped, at once at `to`.
            let drawn = at.lerp(to, snapshot.between);
            let place = maps.standing_at(drawn).extend(depth.at(at.y + lift));
            let quad = sprites.quad(&mut stores.meshes);
            let piece = regent_draw::art::world::Piece {
                code: key.0,
                set: key.1,
                slot: key.2,
                still: None,
            };

            // Its pages on their way: not drawn, looked at again next frame.
            let Some(layers) = sprites
                .piece(&corpus, &mut stores, piece)
                .map(<[regent_draw::map::cel::Drawing]>::to_vec)
            else {
                pending = true;
                continue;
            };
            let (entity, _) = pieces::spawn(
                &mut commands,
                (&quad, &layers),
                &reel,
                (
                    None,
                    place,
                    maps.standing_at(drawn),
                    shown_as(&fog, c.unit),
                    timing,
                ),
            );
            let tint = Tint {
                colour: c.colour,
                transparency: Some(c.level),
            };

            commands.entity(entity).insert(Ghost(tint));
            ghosts
                .drawn
                .insert(c.handle, (entity, c.unit, at, to, shown.jumps));
        }

        ghosts.ms = (!pending).then_some(reel.ms);
    }

    for (entity, unit, from, to, _) in ghosts.drawn.values() {
        if let Ok((mut transform, mut visibility)) = moved.get_mut(*entity) {
            let at = from.lerp(*to, snapshot.between);
            let z = transform.translation.z;
            let seen = shown_as(&fog, *unit);

            transform.translation = maps.standing_at(at).extend(z);

            // The fog may lift between game frames.
            if *visibility != seen {
                *visibility = seen;
            }
        }
    }
}

/// A clone of unit `unit`'s visibility: its unit's seen level's.
fn shown_as(fog: &Fog, unit: u32) -> Visibility {
    if fog.hides(unit) {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    }
}

/// Paint each new clone's bodies with its draw effects
/// (`pieces::tinted`).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn paint(
    mut commands: Commands,
    fresh: Query<(Entity, &Ghost, &Strata), Without<Painted>>,
    (corpus, reel): (Res<Corpus>, Res<Reel>),
    mut sprites: ResMut<Sprites>,
    mut parts: Query<Coated>,
    mut images: ResMut<Assets<Image>>,
    mut bodies: ResMut<Assets<Body>>,
) {
    for (entity, Ghost(tint), strata) in &fresh {
        let made = (&mut *sprites, &*corpus, (&mut *images, &mut *bodies));

        pieces::tinted(
            &mut commands,
            (entity, strata),
            &mut parts,
            (*tint, reel.ms),
            made,
        );
        commands.entity(entity).insert(Painted);
    }
}

/// The trails' ghosts, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Ghosts>();
}
