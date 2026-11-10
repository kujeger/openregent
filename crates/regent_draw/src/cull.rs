//! The map's things far off screen parked: out of Bevy's per-frame
//! passes, which visit every entity drawn or not (a late 512-cell game:
//! 22,000 entities, 200 on screen).
//!
//! ```text
//!  buckets   things by world tile, TILE units a side: each bucket's
//!            quads' bounds; built again as things come or go, or
//!            `Maps` changes (a relay moves them)
//!  frame     the screen's world rect and the inset's (the tracking
//!            window's); a bucket within NEAR of one is out, one past
//!            FAR of both is parked
//!  parked    no ViewVisibility: Bevy's visibility check, its reset and
//!            the 3D mesh and material extraction skip the entity. Put
//!            back, the component is new, so the material is extracted
//!            again, as for a thing coming into view
//! ```

use std::collections::HashMap;

use bevy::camera::visibility::ViewVisibility;
use bevy::prelude::*;

use crate::map::Maps;
use crate::map::redraw::Inset;
use crate::map::things::Thing;
use crate::stage::View;

/// World units a bucket's side.
const TILE: f32 = 512.0;

/// How far past a view, as a share of its larger side, a bucket comes
/// out.
const NEAR: f32 = 0.25;

/// How far past every view, as a share of its larger side, a bucket is
/// parked: wider than [`NEAR`], so a bucket at the edge does not flip
/// each frame.
const FAR: f32 = 0.5;

/// A thing parked by [`cull`]: its `ViewVisibility` taken.
#[derive(Component)]
pub struct Parked;

/// The things by bucket, and what each bucket is.
#[derive(Resource, Default)]
pub struct Cull {
    buckets: HashMap<(i32, i32), Bucket>,
    /// How many things they hold.
    counted: usize,
}

/// One tile's things.
struct Bucket {
    /// Their quads' union, world units.
    bounds: Rect,
    things: Vec<Entity>,
    /// Whether they are parked; `None` not yet judged.
    parked: Option<bool>,
}

/// The bucket of a thing at world point `at`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "a map is a few hundred tiles across"
)]
fn bucket_of(at: Vec2) -> (i32, i32) {
    ((at.x / TILE).floor() as i32, (at.y / TILE).floor() as i32)
}

/// The buckets of `things`, none judged.
fn buckets<'a>(things: impl Iterator<Item = (Entity, &'a Transform)>) -> Cull {
    let mut buckets: HashMap<(i32, i32), Bucket> = HashMap::new();

    for (entity, transform) in things {
        let at = transform.translation.truncate();
        let quad = Rect::from_center_size(at, transform.scale.truncate().abs());
        let bucket = buckets.entry(bucket_of(at)).or_insert_with(|| Bucket {
            bounds: quad,
            things: Vec::new(),
            parked: None,
        });

        bucket.bounds = bucket.bounds.union(quad);
        bucket.things.push(entity);
    }

    let counted = buckets.values().map(|b| b.things.len()).sum();

    Cull { buckets, counted }
}

/// What the views show: the screen's world rect, and the inset's ground
/// rect.
fn views(view: &View, inset: Option<&Inset>) -> Vec<Rect> {
    let screen = view.rect.and_then(|rect| {
        let min = view.world_at(rect.min)?;
        let max = view.world_at(rect.max)?;

        Some(Rect::from_corners(min, max))
    });
    let inset = inset.and_then(|i| i.seen);

    screen.into_iter().chain(inset).collect()
}

/// Whether `drawn` lies within `share` of a view's larger side of one of
/// `views`.
fn near(drawn: Rect, views: &[Rect], share: f32) -> bool {
    views.iter().any(|v| {
        let grown = v.inflate(v.size().max_element() * share);

        !grown.intersect(drawn).is_empty()
    })
}

/// Park the map's things far from every view, and bring back those
/// coming near one; the buckets built again first as things come, go or
/// move.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
pub fn cull(
    mut commands: Commands,
    view: Res<View>,
    inset: Option<Res<Inset>>,
    mut cull: ResMut<Cull>,
    things: Query<(Entity, &Transform), With<Thing>>,
    maps: Option<Res<Maps>>,
    mut gone: RemovedComponents<Thing>,
    parked: Query<Has<Parked>, With<Thing>>,
) {
    // Things move only as the map is relaid, which changes `Maps`, and
    // come with a redraw, which does too; a count is no per-thing walk.
    let moved = maps.is_some_and(|m| m.is_changed());
    let gone = gone.read().count() > 0;

    if moved || gone || things.iter().len() != cull.counted {
        *cull = buckets(things.iter());
    }

    let views = views(&view, inset.as_deref());

    // No view yet: nothing judged.
    let judged = cull.buckets.values_mut().filter(|_| !views.is_empty());

    for bucket in judged {
        let want = match bucket.parked {
            Some(true) => !near(bucket.bounds, &views, NEAR),
            Some(false) | None => !near(bucket.bounds, &views, FAR),
        };

        if bucket.parked != Some(want) {
            for entity in &bucket.things {
                match (want, parked.get(*entity)) {
                    (true, Ok(false)) => {
                        commands
                            .entity(*entity)
                            .try_remove::<ViewVisibility>()
                            .try_insert(Parked);
                    }
                    (false, Ok(true)) => {
                        commands
                            .entity(*entity)
                            .try_insert(ViewVisibility::default())
                            .try_remove::<Parked>();
                    }
                    _ => {}
                }
            }

            bucket.parked = Some(want);
        }
    }
}

/// The cull's systems, for an app that draws a map's things: after the
/// view and the inset are set, before Bevy's transform and visibility
/// passes.
pub fn plugin(app: &mut App) {
    use bevy::transform::TransformSystems;

    app.init_resource::<Cull>()
        .add_systems(PostUpdate, cull.before(TransformSystems::Propagate));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view of 1000 by 600 points at zoom 1, looking at the origin.
    fn view() -> View {
        View {
            rect: Some(Rect::new(0.0, 0.0, 1000.0, 600.0)),
            zoom: 1.0,
            ..default()
        }
    }

    /// An app culling, its view `view`.
    fn app(view: View) -> App {
        let mut app = App::new();

        app.insert_resource(view)
            .init_resource::<Cull>()
            .add_systems(Update, cull);
        app
    }

    /// A thing at world point `at`, 64 units square.
    fn thing(app: &mut App, at: Vec2) -> Entity {
        app.world_mut()
            .spawn((
                Thing,
                Transform::from_translation(at.extend(0.0)).with_scale(Vec3::new(64.0, 64.0, 1.0)),
                ViewVisibility::default(),
            ))
            .id()
    }

    fn parked(app: &App, entity: Entity) -> bool {
        let world = app.world();
        let parked = world.get::<Parked>(entity).is_some();

        assert_eq!(parked, world.get::<ViewVisibility>(entity).is_none());
        parked
    }

    /// A thing far off screen loses its `ViewVisibility`; one on screen
    /// keeps it; the far one has it back once the view comes to it.
    #[test]
    fn a_thing_far_off_screen_is_parked_until_the_view_comes() {
        let mut app = app(view());
        let near = thing(&mut app, Vec2::new(100.0, 50.0));
        let far = thing(&mut app, Vec2::new(9000.0, 0.0));

        app.update();
        assert!(!parked(&app, near));
        assert!(parked(&app, far));

        app.world_mut().resource_mut::<View>().pan = Vec2::new(8800.0, 0.0);
        app.update();
        assert!(!parked(&app, far));
        assert!(parked(&app, near));
    }

    /// A thing moved as the map is relaid is judged where it now is.
    #[test]
    fn a_relaid_thing_is_judged_where_it_went() {
        let mut app = app(view());
        let moved = thing(&mut app, Vec2::new(100.0, 50.0));

        app.insert_resource(Maps::with(Vec::new()));
        app.update();
        assert!(!parked(&app, moved));

        app.world_mut()
            .get_mut::<Transform>(moved)
            .expect("placed")
            .translation
            .x = 9000.0;
        app.world_mut().resource_mut::<Maps>().set_changed();
        app.update();
        assert!(parked(&app, moved));
    }

    /// Before the panels give the view a rect, nothing is parked.
    #[test]
    fn nothing_is_parked_without_a_view() {
        let mut app = app(View::default());
        let far = thing(&mut app, Vec2::new(9000.0, 0.0));

        app.update();
        assert!(!parked(&app, far));
    }

    /// The tracking window's ground keeps its things out wherever the
    /// main view is.
    #[test]
    fn the_inset_keeps_its_things() {
        let mut app = app(view());
        let watched = thing(&mut app, Vec2::new(-7000.0, 3000.0));

        let mut inset = Inset::default();

        inset.seen = Some(Rect::from_center_size(
            Vec2::new(-7000.0, 3000.0),
            Vec2::splat(300.0),
        ));
        app.insert_resource(inset);
        app.update();
        assert!(!parked(&app, watched));
    }
}
