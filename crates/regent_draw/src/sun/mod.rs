//! Shadows cast by 3D models of buildings from one sun, in place of the
//! shadows painted into their sprites: `[enhancements] cast = "models"`,
//! over soft shadows ([`crate::soft`]). Experimental: Bevy's directional
//! light and shadow map cast them; the world's own materials receive.
//!
//! ```text
//!  openregent   each finished building with a model ──> Casters
//!  casters    a Mesh3d each, its model through frame::ART at its anchor's
//!             metric point, on CASTER_LAYER: only the light sees them,
//!             so they cast and are never drawn
//!  light      one DirectionalLight along the sun (cast::Light), on
//!             LIT_LAYER: the world camera's and the tracking window's;
//!             one cascade a view, fitted here to the box what that view
//!             shows lies in (fit); one map, 4096² while models cast,
//!             each view drawing it again before it draws
//!  receivers  bodies and the ground sample the map at their metric point
//!             (shaders/lit.wesl) and write coverage into alpha, as a band
//!             does: shadows still do not stack. The ground's shader reads
//!             it only while models cast (plain::Shade, receive). A body stands upright over
//!             its footing (its MeshTag, stood); a translucent one lifts
//!             the coverage under it (material::Pass::Lift)
//!  casters'   painted shadow dropped (`Band::painted`); their bodies
//!  own        tagged with their slot: each pixel at its point on the
//!             model, none where the model shades itself (relief), so
//!             they take the other models' shadows, never their own;
//!             with Own::Cast their own too
//! ```
//!
//! The light works only while a model casts: the default game draws as
//! before.

pub mod bare;
pub mod cast;
pub mod model;
pub mod relief;

use std::collections::HashMap;

use bevy::asset::{AssetEventSystems, RenderAssetUsages};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::visibility::RenderLayers;
use bevy::light::cascade::Cascade;
use bevy::light::{
    CascadeShadowConfigBuilder, Cascades, DirectionalLightShadowMap, NotShadowCaster,
    SimulationLightSystems,
};
use bevy::math::Vec3A;
use bevy::mesh::{Indices, Mesh, MeshTag, PrimitiveTopology};
use bevy::prelude::*;
use regent_core::data::fourcc::FourCc;

use crate::cull::Parked;
use crate::frame;
use crate::instance::Tag;
use crate::lie::Footing;
use crate::map::Maps;
use crate::material::Body;
use crate::plain::{Alpha, Lay, Plain, Shade};
use crate::smooth::{Sheet, touched};
use crate::stage::{self, WorldCamera};
use bare::{Bare, Sprites};
use cast::Light;
use model::Proxies;

/// The light's layer, and each view's it lights: the world camera's
/// (`present` gives it the layer) and the tracking window's. Bevy lights
/// a view only where their layers meet. Nothing is drawn on it.
pub const LIT_LAYER: usize = 7;

/// The casters' layer: the light's alone, so they are never drawn.
const CASTER_LAYER: usize = 6;

/// The shadow map's texels a side while models cast. At zoom 1 a
/// 1920×1200 view's box ([`fit`]) on hilly ground is about 3,100 by
/// 2,500 world units across the light: 0.6 to 0.8 a texel.
const SHADOW_MAP: usize = 4096;

/// How far a body's top can stand over its footing, world units: the
/// box the light is fitted to reaches this much past the ground
/// (**Inferred**: a tall tree's or building's sprite, by eye).
const BODY_REACH: f32 = 256.0;

/// How far a body's pixel can lie below its footing, world units
/// (**Inferred**: a sprite's foot a few pixels under its anchor, by eye).
const BODY_BELOW: f32 = 16.0;

/// World units the box gains on each side across the light: room for the
/// snap to texels (one texel) and the filter's reach (a few), both under
/// one world unit at zoom 1.
const MARGIN: f32 = 4.0;

/// The one cascade's far bound: past any view depth.
const EVERY_DEPTH: f32 = 1.0e9;

/// What casts the buildings' shadows.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum Cast {
    /// The shadows painted into the sprites, as the original.
    #[default]
    Painted,
    /// The buildings with a model cast from it, from [`Sun::light`].
    Models,
}

/// Whose models' shadows a model building's own pixels take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Own {
    /// The other models' alone: its sprite has its own shade painted in.
    #[default]
    Painted,
    /// Every model's, its own too. Experimental: for sprites with their
    /// painted shade removed; stock ones darken twice.
    Cast,
}

/// What casts, the sun the models cast under, and whether a model
/// building shades itself.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Sun {
    pub cast: Cast,
    pub light: Light,
    pub own: Own,
}

impl Default for Sun {
    /// φ 29°, elevation 50°: right for buildings, by eye
    /// (`docs/original/art-camera.md`); [`cast::SIZE`] across.
    fn default() -> Self {
        Sun {
            cast: Cast::Painted,
            light: Light {
                direction: 29.0,
                elevation: 50.0,
                size: cast::SIZE,
            },
            own: Own::Painted,
        }
    }
}

impl Sun {
    /// Whether models cast.
    #[must_use]
    pub fn models(&self) -> bool {
        self.cast == Cast::Models
    }
}

/// One building casting from its model: its piece, descriptor and set,
/// the flat ground point its anchor stands on, world units, and the
/// ground's lift there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Casting {
    pub piece: Entity,
    pub code: FourCc,
    pub set: u16,
    pub at: Vec2,
    pub lift: f32,
    /// The piece's draw depth: where its model draws in the order while
    /// shown ([`bare`]).
    pub depth: f32,
}

/// Every building casting from its model now: openregent's to fill.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Casters(pub Vec<Casting>);

/// On a piece casting from its model: its bodies take the other models'
/// shadows, never its own ([`relief`]).
#[derive(Component, Clone, Copy, Debug)]
pub struct Casts;

/// A body's flat ground point's world height as its [`Tag`]: the
/// bits, where 0 is none; a height of exactly 0 is written as −0.
#[must_use]
pub fn stand(ground: f32) -> Tag {
    let ground = if ground == 0.0 { -0.0 } else { ground };

    Tag(ground.to_bits())
}

/// The sun.
#[derive(Component)]
struct Sunlight;

/// A caster's mesh.
#[derive(Component)]
struct Thrown;

/// Each model's mesh, by descriptor.
#[derive(Resource)]
struct Shapes(HashMap<FourCc, Handle<Mesh>>);

/// The material every caster shares, and every model shown.
#[derive(Resource)]
struct Rig {
    caster: Handle<StandardMaterial>,
    bare: Handle<Bare>,
}

/// A model drawn plain in its building's place ([`bare`]).
#[derive(Component)]
struct Shown;

/// What [`place`] spawned last: casters and models shown.
type Placed = Or<(With<Thrown>, With<Shown>)>;

/// The ground's least and most lift, world units, and the map revision
/// they were measured at: [`fitted`]'s own, keyed by the revision, so a
/// new game measures again.
#[derive(Default)]
struct Relief {
    revision: Option<u64>,
    lifts: Vec2,
}

/// `model` in its own frame, flat shaded: Bevy's shadow pass reads
/// positions; normals keep `StandardMaterial`'s pipeline whole.
fn shape(model: &model::Model) -> Mesh {
    let positions: Vec<[f32; 3]> = model
        .triangles
        .iter()
        .flatten()
        .map(Vec3::to_array)
        .collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.compute_flat_normals();
    mesh
}

/// The light, idle; each model's mesh and the casters' material.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn rig(
    mut commands: Commands,
    (mut meshes, mut standards): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    mut bares: ResMut<Assets<Bare>>,
    proxies: Res<Proxies>,
) {
    commands.spawn((
        Sunlight,
        DirectionalLight {
            shadow_maps_enabled: false,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.0,
            first_cascade_far_bound: EVERY_DEPTH,
            maximum_distance: EVERY_DEPTH,
            overlap_proportion: 0.0,
        }
        .build(),
        RenderLayers::from_layers(&[LIT_LAYER, CASTER_LAYER]),
    ));

    let made = proxies
        .all()
        .map(|(code, model)| (code, meshes.add(shape(model))))
        .collect();

    commands.insert_resource(Shapes(made));
    commands.insert_resource(Rig {
        caster: standards.add(StandardMaterial::default()),
        bare: bares.add(Bare::default()),
    });
}

/// Whether models cast now: wanted, one finished, and the world drawn.
fn casting(sun: &Sun, casters: &Casters, world: &Camera) -> bool {
    sun.models() && !casters.0.is_empty() && world.is_active
}

/// The casters' meshes again as the casters, the sun or the sprites'
/// showing change: one each, turned by [`frame::ART`] and standing on its
/// anchor's metric point, for the light alone; and with the sprites
/// hidden, the same drawn plain for the views ([`bare`]).
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn place(
    mut commands: Commands,
    (sun, casters, shapes, rig): (Res<Sun>, Res<Casters>, Res<Shapes>, Res<Rig>),
    sprites: Res<Sprites>,
    placed: Query<Entity, Placed>,
) {
    if !sun.is_changed() && !casters.is_changed() && !sprites.is_changed() {
        return;
    }

    for entity in &placed {
        commands.entity(entity).despawn();
    }

    let placing = casters
        .0
        .iter()
        .filter(|_| sun.models())
        .filter_map(|c| shapes.0.get(&c.code).map(|shape| (c, shape)));
    let turned = Quat::from_mat3(&frame::ART);

    for (casting, shape) in placing {
        let drawn = casting.at + Vec2::Y * casting.lift;

        let put = Transform {
            translation: frame::point(drawn, casting.lift),
            rotation: turned,
            ..default()
        };

        commands.spawn((
            Thrown,
            Mesh3d(shape.clone()),
            MeshMaterial3d(rig.caster.clone()),
            put,
            RenderLayers::layer(CASTER_LAYER),
        ));

        // Seen by both views (the light's layer), casting nothing more:
        // its caster above casts.
        if *sprites == Sprites::Hidden {
            commands.spawn((
                Shown,
                Mesh3d(shape.clone()),
                MeshMaterial3d(rig.bare.clone()),
                put,
                MeshTag::new(casting.depth.to_bits()),
                RenderLayers::layer(LIT_LAYER),
                NotShadowCaster,
            ));
        }
    }
}

/// The light along the sun, its shadow map on and full size only while
/// models cast; the sun's spread as Bevy's soft shadow size, which only
/// our own sampling reads (`shaders/lit.wesl`).
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn aim(
    (sun, casters): (Res<Sun>, Res<Casters>),
    world: Single<&Camera, With<WorldCamera>>,
    light: Single<(&mut DirectionalLight, &mut Transform), With<Sunlight>>,
    mut map: ResMut<DirectionalLightShadowMap>,
) {
    let (mut light, mut toward) = light.into_inner();
    let on = casting(&sun, &casters, &world);
    let size = if on {
        SHADOW_MAP
    } else {
        stage::IDLE_SHADOW_MAP
    };

    if light.shadow_maps_enabled != on {
        light.shadow_maps_enabled = on;
    }

    if map.size != size {
        map.size = size;
    }

    if sun.is_changed() {
        let spread = sun.light.spread();

        *toward = Transform::IDENTITY.looking_to(-sun.light.toward(), Vec3::Y);
        light.soft_shadow_size = (spread > 0.0).then_some(spread);
    }
}

/// The ground's materials as models start or stop casting, and as they
/// are made: [`Shade::Cast`] while they cast, else [`Shade::Bare`], so
/// the ground reads the shadow map only while one is drawn.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn receive(
    (sun, casters): (Res<Sun>, Res<Casters>),
    world: Single<&Camera, With<WorldCamera>>,
    (mut made, mut laid): (
        MessageReader<AssetEvent<Plain>>,
        MessageReader<AssetEvent<Sheet>>,
    ),
    (mut plains, mut sheets): (ResMut<Assets<Plain>>, ResMut<Assets<Sheet>>),
    warm: Res<Warm>,
    mut was: Local<Shade>,
) {
    let shade = if casting(&sun, &casters, &world) {
        Shade::Cast
    } else {
        Shade::Bare
    };
    let changed = *was != shade;

    *was = shade;

    for id in touched(&mut made, &plains, changed) {
        let stale = plains
            .get(id)
            .is_some_and(|p| p.lay == Lay::Ground && p.shade != shade)
            && !warm.plains.contains(&id);

        if stale && let Some(mut plain) = plains.get_mut(id) {
            plain.shade = shade;
        }
    }

    for id in touched(&mut laid, &sheets, changed) {
        let stale = sheets.get(id).is_some_and(|s| s.shade != shade) && !warm.sheets.contains(&id);

        if stale && let Some(mut sheet) = sheets.get_mut(id) {
            sheet.shade = shade;
        }
    }
}

/// The ground's pipelines of both [`Shade`]s, kept specialised: as models
/// start or stop casting the ground switches to one already compiled,
/// or Bevy would skip it while the new one compiles, a frame or more
/// without ground. [`receive`] leaves their materials be.
#[derive(Resource, Default)]
struct Warm {
    plains: Vec<AssetId<Plain>>,
    sheets: Vec<AssetId<Sheet>>,
}

/// A mesh of the ground's attributes that draws nothing: its one
/// triangle has no area.
fn unseen() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32; 2]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}

/// Each ground material's pipeline of each [`Shade`] specialised, on an
/// [`unseen`] mesh every view draws: the patches' and the inset's, the
/// floor's, the sharp patches'.
fn warm(
    mut commands: Commands,
    (mut images, mut meshes): (ResMut<Assets<Image>>, ResMut<Assets<Mesh>>),
    (mut plains, mut sheets): (ResMut<Assets<Plain>>, ResMut<Assets<Sheet>>),
) {
    let mesh = Mesh3d(meshes.add(unseen()));
    let picture = images.add(Image::default());
    let mut warm = Warm::default();

    for shade in [Shade::Bare, Shade::Cast] {
        for alpha in [Alpha::MaskFirst, Alpha::Mask] {
            let plain = plains.add(Plain {
                alpha,
                shade,
                ..Plain::ground(picture.clone())
            });

            warm.plains.push(plain.id());
            commands.spawn((mesh.clone(), MeshMaterial3d(plain), NoFrustumCulling));
        }

        let sheet = sheets.add(Sheet {
            picture: picture.clone(),
            shade,
        });

        warm.sheets.push(sheet.id());
        commands.spawn((mesh.clone(), MeshMaterial3d(sheet), NoFrustumCulling));
    }

    commands.insert_resource(warm);
}

/// A camera, as [`fitted`] reads it.
type Viewed = (
    Entity,
    &'static Camera,
    &'static Projection,
    &'static GlobalTransform,
    Option<&'static RenderLayers>,
);

/// Each lit view's cascade (the world camera's, the tracking window's),
/// in place of the one Bevy fits to its frustum: that frustum's depth
/// range is the draw order's, not where the metric ground lies. Each is
/// fitted to its own view's box; the views share the map, each drawing
/// it again before it draws. Bevy builds the cascades, then culls casters
/// and draws the map from them; this runs between.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn fitted(
    maps: Option<Res<Maps>>,
    mut relief: Local<Relief>,
    map: Res<DirectionalLightShadowMap>,
    views: Query<Viewed>,
    light: Single<(&DirectionalLight, &GlobalTransform, &mut Cascades), With<Sunlight>>,
) {
    let (light, toward, mut cascades) = light.into_inner();

    // The ground's least and most lift, measured as the map changes.
    if let Some(maps) = maps.as_ref()
        && relief.revision != Some(maps.revision)
    {
        let (low, high) = maps.lifts();

        *relief = Relief {
            revision: Some(maps.revision),
            lifts: Vec2::new(low, high),
        };
    }

    let lit = RenderLayers::layer(LIT_LAYER);
    let lifts = Vec2::new(relief.lifts.x - BODY_BELOW, relief.lifts.y + BODY_REACH);
    #[allow(clippy::cast_precision_loss, reason = "a power of two to 8192")]
    let texels = map.size as f32;

    // The views the light reaches, active, orthographic.
    let seen = views
        .iter()
        .filter(|(_, camera, ..)| light.shadow_maps_enabled && camera.is_active)
        .filter(|(.., layers)| layers.is_some_and(|l| l.intersects(&lit)))
        .filter_map(|(entity, _, projection, eye, _)| match projection {
            Projection::Orthographic(ortho) => Some((entity, ortho.area, eye.translation())),
            _ => None,
        });

    for (entity, area, eye) in seen {
        let seen = Rect::from_corners(area.min + eye.truncate(), area.max + eye.truncate());

        cascades
            .cascades
            .insert(entity, vec![fit(seen, lifts, toward.rotation(), texels)]);
    }
}

/// The corners of the box every point the view shows lies in: drawn
/// within `seen` (world x, y), its lift over its flat ground within
/// `lifts`. A ground point, or a body's pixel stood upright over its
/// footing ([`frame::point`]); the map is linear in both, so the box is
/// a parallelepiped.
fn corners(seen: Rect, lifts: Vec2) -> [Vec3; 8] {
    let mut out = [Vec3::ZERO; 8];
    let xs = [seen.min.x, seen.max.x];
    let ys = [seen.min.y, seen.max.y];
    let ls = [lifts.x, lifts.y];

    for (i, corner) in out.iter_mut().enumerate() {
        let drawn = Vec2::new(xs[i & 1], ys[(i >> 1) & 1]);

        *corner = frame::point(drawn, ls[(i >> 2) & 1]);
    }

    out
}

/// One cascade over the box ([`corners`]) for a light turned `rotation`
/// and a map `texels` a side: the box's bounds across the light, plus
/// [`MARGIN`], whole world units, centred on a texel, as Bevy's own fit
/// (`bevy_light::cascade::calculate_cascade`) but not square and not a
/// sphere's: a pan moves the box, never resizes it, so the texels stay
/// put on the ground and edges do not shimmer.
///
/// ```text
///  light space   x, y across the light, z toward the sun
///  near          the box's nearest the sun; casters nearer still are
///                clamped there by Bevy's shadow pass
///  far           the box's farthest
/// ```
fn fit(seen: Rect, lifts: Vec2, rotation: Quat, texels: f32) -> Cascade {
    let world_from_light = Mat4::from_quat(rotation);
    let light_from_world = world_from_light.transpose();
    let (min, max) = corners(seen, lifts).iter().fold(
        (Vec3A::splat(f32::MAX), Vec3A::splat(f32::MIN)),
        |(min, max), c| {
            let c = light_from_world.transform_point3a(Vec3A::from(*c));

            (min.min(c), max.max(c))
        },
    );

    let span = ((max - min).truncate() + 2.0 * MARGIN).ceil();
    let texel = span / texels;
    let middle = ((min + max).truncate() / 2.0 / texel).floor() * texel;
    let near = middle.extend(max.z);

    let cascade_from_world = Mat4::from_cols(
        light_from_world.x_axis,
        light_from_world.y_axis,
        light_from_world.z_axis,
        (-near).extend(1.0),
    );
    let world_from_cascade = Mat4::from_cols(
        world_from_light.x_axis,
        world_from_light.y_axis,
        world_from_light.z_axis,
        world_from_light * near.extend(1.0),
    );

    // Orthographic, reverse Z: 1 at the near plane, 0 at the far.
    let depth = (max.z - min.z).recip();
    let clip_from_cascade = Mat4::from_cols(
        Vec4::new(2.0 / span.x, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 2.0 / span.y, 0.0, 0.0),
        Vec4::new(0.0, 0.0, depth, 0.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
    );

    Cascade {
        world_from_cascade,
        clip_from_cascade,
        clip_from_world: clip_from_cascade * cascade_from_world,
        texel_size: texel.max_element(),
    }
}

/// What [`stood`] reads of a body.
type Standing = (
    Entity,
    Option<&'static Footing>,
    Option<&'static ChildOf>,
    Option<&'static Tag>,
);

/// Each body's [`Tag`] while models cast: on a casting building's
/// piece its slot ([`relief::model_tag`]), so it takes the other models'
/// shadows, not its own; else its footing, its own or its parent's, as
/// [`stand`] packs it; none (0) without one, or on a building past the
/// slots. A parked body keeps its tag. Tags left after the models stop
/// read no cascade.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn stood(
    mut commands: Commands,
    (sun, casters, maps): (Res<Sun>, Res<Casters>, Option<Res<Maps>>),
    world: Single<&Camera, With<WorldCamera>>,
    bodies: Query<Standing, (With<MeshMaterial3d<Body>>, Without<Parked>)>,
    parents: Query<(Option<&Footing>, Has<Casts>)>,
) {
    let Some(maps) = maps.filter(|_| casting(&sun, &casters, &world)) else {
        return;
    };

    let slots: HashMap<Entity, u32> = casters
        .0
        .iter()
        .zip(0..relief::SLOTS)
        .map(|(c, slot)| (c.piece, slot))
        .collect();

    for (entity, own, parent, had) in &bodies {
        let parent = parent.map(ChildOf::parent);
        let above = parent.and_then(|p| parents.get(p).ok());
        let slot = parent.and_then(|p| slots.get(&p));
        let footing = match (own, above) {
            (_, Some((_, true))) | (None, None) => None,
            (Some(footing), _) => Some(*footing),
            (None, Some((footing, false))) => footing.copied(),
        };
        let tag = match (slot, footing) {
            (Some(slot), _) => relief::model_tag(*slot),
            (None, Some(f)) => stand(maps.flat_y(f.row)),
            (None, None) => Tag(0),
        };

        if had != Some(&tag) {
            commands.entity(entity).insert(tag);
        }
    }
}

/// The models' reliefs under the sun, as models first cast, as the sun
/// turns or resizes, and as [`Own`] changes: the light's work done once
/// ([`relief::bake`]).
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn bake(
    (sun, casters, proxies): (Res<Sun>, Res<Casters>, Res<Proxies>),
    world: Single<&Camera, With<WorldCamera>>,
    mut images: ResMut<Assets<Image>>,
    mut baked: Local<Option<(Light, Own)>>,
) {
    let wanted = (sun.light, sun.own);

    if casting(&sun, &casters, &world) && *baked != Some(wanted) {
        relief::bake(&mut images, &proxies, sun.light, sun.own);
        *baked = Some(wanted);
    }
}

/// The casting buildings' slots as they change: slot n the n-th caster
/// ([`stood`] tags its bodies with it).
fn seat(
    (sun, casters, proxies): (Res<Sun>, Res<Casters>, Res<Proxies>),
    mut images: ResMut<Assets<Image>>,
) {
    if sun.models() && casters.is_changed() {
        let stands: Vec<relief::Stand> = casters
            .0
            .iter()
            .map(|c| relief::Stand {
                anchor: c.at + Vec2::Y * c.lift,
                flat: c.at.y,
                layer: proxies.layer(c.code).unwrap_or(0),
            })
            .collect();

        relief::seat(&mut images, &stands);
    }
}

/// The models, the light and its fit, the bodies' footings. openregent's.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        bare::register(app);

        app.init_resource::<Sun>()
            .init_resource::<Casters>()
            .init_resource::<Proxies>()
            .init_resource::<Warm>()
            .add_systems(Startup, (rig, warm))
            .add_systems(
                PostUpdate,
                (
                    place,
                    aim.before(SimulationLightSystems::UpdateDirectionalLightCascades),
                    // Told the frame the light is: both read `casting`.
                    receive.after(aim).after(AssetEventSystems),
                    fitted
                        .after(SimulationLightSystems::UpdateDirectionalLightCascades)
                        .before(SimulationLightSystems::UpdateLightFrusta),
                    stood.after(crate::cull::cull),
                    bake,
                    seat,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A height of 0 is a footing, not none.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same bits back")]
    fn height_zero_is_tagged() {
        assert_ne!(stand(0.0).0, 0);
        assert_eq!(f32::from_bits(stand(-12.5).0), -12.5);
    }

    /// The light as [`aim`] turns it, under the default sun.
    fn sunward() -> Quat {
        Transform::IDENTITY
            .looking_to(-Sun::default().light.toward(), Vec3::Y)
            .rotation
    }

    /// Whether world point `p` lands inside `cascade`'s map and depth.
    fn inside(cascade: &Cascade, p: Vec3) -> bool {
        let clip = cascade.clip_from_world * p.extend(1.0);
        let ndc = clip.truncate() / clip.w;

        ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z)
    }

    /// The cascade holds what the view shows: the ground at its corners at
    /// either lift, a tall body's top at the bottom edge; not a point
    /// well off the side.
    #[test]
    fn the_cascade_holds_the_view() {
        let seen = Rect::new(-400.0, -300.0, 900.0, 500.0);
        let lifts = Vec2::new(-10.0, 300.0);
        let cascade = fit(seen, lifts, sunward(), 4096.0);

        for p in corners(seen, lifts) {
            assert!(inside(&cascade, p), "{p}");
        }

        assert!(inside(
            &cascade,
            frame::point(Vec2::new(250.0, -300.0), 300.0)
        ));
        assert!(!inside(
            &cascade,
            frame::point(Vec2::new(5_000.0, 0.0), 0.0)
        ));
    }

    /// A pan moves the cascade by whole texels and keeps its size: the
    /// texels stay put on the ground.
    #[test]
    fn a_pan_keeps_the_texels_on_the_ground() {
        let seen = Rect::new(-400.0, -300.0, 900.0, 500.0);
        let lifts = Vec2::new(0.0, 256.0);
        let first = fit(seen, lifts, sunward(), 4096.0);
        let moved = Rect::from_center_size(seen.center() + Vec2::new(37.3, -12.9), seen.size());
        let second = fit(moved, lifts, sunward(), 4096.0);

        assert!(
            first
                .clip_from_cascade
                .abs_diff_eq(second.clip_from_cascade, 1e-9)
        );

        // A ground point's texel coordinate differs by a whole number.
        let p = frame::point(Vec2::new(100.0, 50.0), 20.0);
        let texel = |c: &Cascade| {
            let clip = c.clip_from_world * p.extend(1.0);

            (clip.truncate().truncate() + 1.0) / 2.0 * 4096.0
        };
        let step = texel(&first) - texel(&second);

        assert!((step - step.round()).abs().max_element() < 1e-2, "{step}");
    }

    /// At zoom 1 on a hilly map the map's texels are under a world unit,
    /// a screen pixel.
    #[test]
    fn a_texel_is_under_a_pixel_at_zoom_one() {
        let seen = Rect::new(0.0, 0.0, 1920.0, 1200.0);
        let lifts = Vec2::new(-BODY_BELOW, 120.0 + BODY_REACH);
        let cascade = fit(seen, lifts, sunward(), 4096.0);

        assert!(cascade.texel_size < 1.0, "{}", cascade.texel_size);
    }
}
