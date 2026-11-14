//! The build preview on the map: the building's first frame at the half
//! cell the cursor picks, on the game's green ring where it fits, red
//! where not (`ring::Look::Fits`, `Blocked`; ring image key),
//! fitted to its clearing reach (`Fit::Clearing`); drawn as its band
//! under `[enhancements] rings = "native"` or `"modern"` (`rings`).
//!
//! ```text
//!  Orders::preview ──> one entity: ring child, a child per layer frame
//!  code or verdict changed ──> rebuilt; moved ──> its transform only
//! ```

use bevy::prelude::*;

use regent_core::content::unit::Unit;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::world::offer::Placement;
use regent_core::view::map::ring::{self, Fit, Look};
use regent_draw::data::Corpus;
use regent_draw::map::cel::Picture;
use regent_draw::map::depth::NEAREST;
use regent_draw::map::{self, Maps};
use regent_draw::ring::{Ellipse, Rings};

use crate::files::config::Settings;
use crate::input::orders::{Orders, Preview};
use crate::map::rings;
use regent_draw::art::world::Piece;
use regent_draw::art::{Assets2, Sprites};
use regent_draw::map::sheet::Paged;

/// How far a layer of the preview sits above the one below.
const LAYER_STEP: f32 = 0.01;

/// The preview drawn: its entity and what it shows; its pictures are
/// the art service's (`art::Sprites`).
#[derive(Resource, Default)]
pub struct PreviewDrawn {
    shown: Option<(Entity, FourCc, Look, Rings)>,
}

/// The ring's look for a verdict.
const fn look(placement: Placement) -> Look {
    match placement {
        Placement::Fits => Look::Fits,
        Placement::Disabled
        | Placement::Capped
        | Placement::Unmet
        | Placement::Unseen
        | Placement::Taken
        | Placement::Refused
        | Placement::Pending => Look::Blocked,
    }
}

/// Keep the preview where [`Orders::preview`] puts it; none without one.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn follow(
    mut commands: Commands,
    orders: Res<Orders>,
    corpus: Res<Corpus>,
    settings: Res<Settings>,
    maps: Res<Maps>,
    mut drawn: ResMut<PreviewDrawn>,
    mut sprites: ResMut<Sprites>,
    (mut stores, mut ellipses): (Assets2, ResMut<Assets<Ellipse>>),
    mut placed: Query<&mut Transform>,
) {
    let with = settings.enhancements.rings;
    let wanted = orders.preview.map(|p| (p.code, look(p.placement), with));
    let now = drawn.shown.map(|(_, code, look, with)| (code, look, with));

    if wanted != now {
        if let Some((entity, ..)) = drawn.shown.take() {
            commands.entity(entity).despawn();
        }

        // Its pages on their way: nothing shown, asked again next frame.
        drawn.shown = orders.preview.and_then(|p| {
            let entity = spawn(
                &mut commands,
                (&corpus, &maps),
                (&mut sprites, &mut stores, &mut ellipses),
                (p, with),
            )?;

            Some((entity, p.code, look(p.placement), with))
        });
    }

    let entity = drawn.shown.map(|(e, ..)| e);

    if let (Some(p), Some(mut transform)) =
        (orders.preview, entity.and_then(|e| placed.get_mut(e).ok()))
    {
        transform.translation = at(&maps, p);
    }
}

/// The pages of each building the build list shows asked for, as it
/// changes: its preview draws at once when picked.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn listed(
    orders: Res<Orders>,
    corpus: Res<Corpus>,
    mut sprites: ResMut<Sprites>,
    mut asked: Local<Option<crate::input::orders::Rows>>,
) {
    let fresh = match (&orders.listed, &*asked) {
        (Some(now), Some(was)) => !std::sync::Arc::ptr_eq(now, was),
        (now, _) => now.is_some(),
    };

    if fresh {
        // Asked early: the list loads behind play.
        for (_, offer) in orders.listed.iter().flat_map(|l| l.iter()) {
            let piece = standing(&corpus, offer.code);
            let art = corpus
                .unit(offer.code)
                .and_then(|u| u.art.as_ref().map(|a| a.id));

            if let Some(record) = art {
                sprites.ask(&corpus, (record, piece.set));
            }
        }

        asked.clone_from(&orders.listed);
    }
}

/// Building `code`'s preview: its first set's first frame.
fn standing(corpus: &Corpus, code: FourCc) -> Piece {
    let set = Unit::find(&corpus.catalogue, code)
        .and_then(Result::ok)
        .and_then(|u| {
            u.initial()
                .map(|s| regent_core::data::imag::set_number(s.id))
        })
        .unwrap_or(0);

    Piece {
        code,
        set,
        slot: 0,
        still: Some(0),
    }
}

/// Where preview `p` draws: its half cell standing on the ground, in
/// front of every piece.
fn at(maps: &Maps, p: Preview) -> Vec3 {
    let cell = crate::ui::tracking::cells(p.at);

    maps.standing_at(cell).extend(NEAREST)
}

/// The preview's entity: its ring, drawn `with` the art or as its band,
/// then each layer's first frame. `None` while their pages are on their
/// way: nothing spawned.
fn spawn(
    commands: &mut Commands,
    (corpus, maps): (&Corpus, &Maps),
    (sprites, stores, ellipses): (&mut Sprites, &mut Assets2, &mut Assets<Ellipse>),
    (p, with): (Preview, Rings),
) -> Option<Entity> {
    let quad = sprites.quad(&mut stores.meshes);
    let unit = Unit::find(&corpus.catalogue, p.code).and_then(Result::ok);
    let ring = unit
        .as_ref()
        .and_then(|u| ring::fitted(&corpus.catalogue, u, look(p.placement), Fit::Clearing).ok()?);
    let native = ring.and_then(|r| rings::native(corpus, ellipses, &r, with));
    let ring = match ring.filter(|_| native.is_none()) {
        Some(r) => match sprites.frame(corpus, stores, (r.frame, Some(r.owner()))) {
            Paged::Ready(cel) => cel.map(|cel| (r.at, cel)),
            Paged::Pending => return None,
        },
        None => None,
    };
    let layers = match unit {
        Some(_) => Some(
            sprites
                .piece(corpus, stores, standing(corpus, p.code))?
                .to_vec(),
        ),
        None => None,
    };
    // At its place now: a move this frame cannot reach it until the
    // spawn applies, so the default would draw a frame at the map's centre.
    let parent = commands
        .spawn((
            Transform::from_translation(at(maps, p)),
            Visibility::Inherited,
        ))
        .id();

    // The band under the body, in the ring's place.
    for (ellipse, mut put) in native.map(|n| n.0).unwrap_or_default() {
        put.translation.z -= LAYER_STEP;

        commands.spawn((
            Mesh3d(quad.clone()),
            MeshMaterial3d(ellipse),
            put,
            ChildOf(parent),
        ));
    }

    // The ring under the body, fitted to the clearing reach.
    if let Some((offset, cel)) = ring {
        #[allow(clippy::cast_precision_loss, reason = "screen pixels")]
        let at = Vec2::new(offset.0 as f32, -(offset.1 as f32)) - cel.anchor;

        quads(commands, (parent, &quad), &cel, at.extend(-LAYER_STEP));
    }

    let layers = layers.unwrap_or_default();

    #[allow(clippy::cast_precision_loss, reason = "a set has at most six layers")]
    for (nth, drawing) in map::cel::rungs(&layers).into_iter().zip(layers.iter()) {
        let base = drawing.posed();

        if let Some(Some(cel)) = drawing.cels.first() {
            quads(
                commands,
                (parent, &quad),
                cel,
                (base - cel.anchor).extend(nth as f32 * LAYER_STEP),
            );
        }
    }

    Some(parent)
}

/// One frame's body, lift and shadow quads under `parent`, at `at`, on
/// `quad`.
fn quads(
    commands: &mut Commands,
    (parent, quad): (Entity, &Handle<Mesh>),
    cel: &map::cel::Cel,
    at: Vec3,
) {
    let put = Transform {
        translation: at,
        scale: cel.size.extend(1.0),
        ..default()
    };

    match &cel.body {
        Picture::Indexed(body) => {
            commands.spawn((
                Mesh3d(quad.clone()),
                cel.frame,
                MeshMaterial3d(body.clone()),
                put,
                ChildOf(parent),
            ));
        }
        Picture::Colour(colour) => {
            commands.spawn((
                Mesh3d(quad.clone()),
                cel.frame,
                MeshMaterial3d(colour.clone()),
                put,
                ChildOf(parent),
            ));
        }
    }

    if let Some(lift) = &cel.lift {
        commands.spawn((
            Mesh3d(quad.clone()),
            cel.frame,
            MeshMaterial3d(lift.clone()),
            put,
            ChildOf(parent),
        ));
    }

    if let Some(band) = &cel.band {
        commands.spawn((
            Mesh3d(quad.clone()),
            cel.frame,
            MeshMaterial3d(band.clone()),
            put,
            ChildOf(parent),
        ));
    }
}

/// The placing preview drawn, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<PreviewDrawn>();

    // The other early asks: what a placed building will show.
    crate::map::early::plugin(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_fitting_preview_shows_the_green_ring() {
        assert_eq!(look(Placement::Fits), Look::Fits);
        assert_eq!(look(Placement::Taken), Look::Blocked);
        assert_eq!(look(Placement::Unseen), Look::Blocked);
        assert_eq!(look(Placement::Unmet), Look::Blocked);
    }
}
