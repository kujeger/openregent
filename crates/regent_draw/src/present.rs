//! The world drawn at the art grid and scaled into the map's rect
//! (`[enhancements] view_upscale`, openregent's): one texel an art pixel at
//! any zoom, so the scaler does all the scaling, display scale times zoom.
//!
//! ```text
//!  nearest  WorldCamera ──> the window, as without this module
//!  sharp    WorldCamera ──> world image (Grid), shadows resolved
//!           Shown camera, the window's map rect ──> sharp-bilinear quad,
//!             then the overlays (OVERLAY: hit point bars)
//!    at one texel a pixel (Straight), where the quad would copy:
//!           WorldCamera ──> its main texture at the art grid, shadows
//!             resolved straight into the Shown camera's main texture
//!           Shown camera ──> the overlays over it
//!  fsr      WorldCamera ──> world image
//!           Easu camera ──> easu image, the map rect's pixels: EASU quad
//!           Shown camera ──> RCAS quad, the easu image 1:1, then the
//!             overlays
//!  sharp-sprite  WorldCamera ──> the window, as nearest; each sprite
//!                  and the ground sharp-bilinear ([`crate::smooth`])
//!  fsr-sprite    WorldCamera ──> easu image, as sharp-sprite
//!                Shown camera ──> RCAS quad, as fsr
//! ```
//!
//! The sprite modes pin the world camera to the physical pixels
//! ([`pinned`]): whole world units on pixel edges where the factor allows.
//!
//! The world camera's translation snaps to the image's texels; the quad
//! takes the remainder ([`Grid::start`]) to the nearest physical pixel, so
//! a pan moves the picture a pixel at a time, not a texel. Where an art
//! pixel is smaller than a physical pixel ([`texel`]), the image is drawn
//! at physical pixels and only placed.
//!
//! Picking stays [`View`]'s arithmetic: the quad maps a physical pixel to
//! the world as [`View::world_at`] does, to half a pixel.
//!
//! Selection rings stay in the world image: their glow indices need the
//! shadow pass. Names and amounts are egui's already.

use bevy::asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraUpdateSystems, ImageRenderTarget, RenderTarget, ScalingMode};
use bevy::core_pipeline::core_2d::Transparent2d;
use bevy::ecs::schedule::SystemCondition;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_phase::sort_phase_system;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};
use bevy::transform::TransformSystems;
use bevy::window::{PrimaryWindow, WindowRef};

use crate::shadow::{Over, Written};
use crate::smooth::Sampling;
use crate::stage::{View, WorldCamera};

/// How the world reaches the window.
#[derive(
    Resource, Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum ViewUpscale {
    /// Drawn straight to the window, each art pixel sampled nearest.
    Nearest,
    /// At the art grid, then sharp-bilinear: whole pixels, a blended seam.
    /// The default: zoomed, it keeps the art's look, where nearest shows
    /// the ground's triangle edges.
    #[default]
    Sharp,
    /// At the art grid, then AMD FSR 1: EASU, then RCAS.
    Fsr,
    /// At physical pixels, each sprite and the ground sharp-bilinear in
    /// its own texels ([`crate::smooth`]): motion, sway and zoom at
    /// screen pixels; outlines nearest.
    SharpSprite,
    /// Sharp-sprite, then FSR 1's RCAS over the world. No EASU: nothing
    /// is left to scale.
    FsrSprite,
}

/// Where the world camera draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// The window, at its pixels.
    Window,
    /// The world image, at the art grid ([`Grid`]).
    Grid,
    /// The easu image, at the map rect's pixels: RCAS reads it.
    Physical,
    /// At the art grid as [`Route::Grid`], its shadows resolved into the
    /// shown camera's main texture where the quad would put them: sharp at
    /// one texel a pixel, where the quad would copy the image unchanged.
    Straight,
}

/// The route this frame: [`Route`] while it changes, for [`route`].
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
struct Routed(Route);

impl ViewUpscale {
    /// How the art's texels are read.
    #[must_use]
    pub const fn sampling(self) -> Sampling {
        match self {
            ViewUpscale::SharpSprite | ViewUpscale::FsrSprite => Sampling::Sharp,
            ViewUpscale::Nearest | ViewUpscale::Sharp | ViewUpscale::Fsr => Sampling::Nearest,
        }
    }

    /// Where the world camera draws at `ppu` physical pixels a world unit
    /// (zoom times display scale). Sharp at an art pixel no larger than a
    /// physical pixel is one texel a pixel ([`texel`]): straight.
    fn route(self, ppu: f32) -> Route {
        match self {
            ViewUpscale::Nearest | ViewUpscale::SharpSprite => Route::Window,
            ViewUpscale::Sharp if ppu <= 1.0 => Route::Straight,
            ViewUpscale::Sharp | ViewUpscale::Fsr => Route::Grid,
            ViewUpscale::FsrSprite => Route::Physical,
        }
    }

    /// Whether RCAS shows the world.
    const fn sharpened(self) -> bool {
        matches!(self, ViewUpscale::Fsr | ViewUpscale::FsrSprite)
    }
}

/// The render layer of the marks that are interface, not world art: drawn
/// at the window's pixels, over the scaled world. Nearest draws them with
/// the world.
pub const OVERLAY: usize = 1;

/// The scaled world's quad's layer: the [`Shown`] camera's alone.
const SHOWN_LAYER: usize = 2;

/// The EASU quad's layer: the [`Easu`] camera's alone.
const EASU_LAYER: usize = 3;

/// The cameras' orders: the world's is 0; then EASU, the shown quad, and
/// the first free above them, for the interface's camera.
const EASU_ORDER: isize = 1;
const SHOWN_ORDER: isize = 2;
pub const TOP_ORDER: isize = 3;

/// The scaled world's depth: behind every overlay, inside a 2D camera's
/// `-1000 ..= 1000`.
const QUAD_Z: f32 = -990.0;

/// Texels kept past the needed picture on each side: EASU reads two
/// beyond the texel it is in, and the remainder moves half of one.
const PAD: u32 = 3;

/// Where the world camera snaps, in texels past a whole one: texel
/// middles then sit a quarter past whole world units. Art pixel edges
/// lie on whole or half units (a cel's quad is centred on its anchor),
/// so no middle lands on an edge and each art pixel fills one texel.
/// On whole units, odd and even cels sampled their edges and dropped or
/// doubled columns (measured against nearest at zoom 1, scale 1).
const SNAP: f32 = 0.75;

/// Image sides are allotted in steps of this many texels, so a smooth
/// zoom remakes the image now and then, not every frame.
const STEP: u32 = 64;

/// RCAS's strength, in stops (halvings) below the most: the default of
/// AMD's FSR 1 sample (`rcasAttenuation`, `sample/src/VK/SampleRenderer.h`).
pub const SHARPNESS: f32 = 0.25;

/// The camera that shows the scaled world in the map's rect.
#[derive(Component)]
pub struct Shown;

/// The camera that runs EASU into the easu image.
#[derive(Component)]
pub struct Easu;

/// The scaled world's quad.
#[derive(Component)]
struct ShownQuad;

/// The EASU quad.
#[derive(Component)]
struct EasuQuad;

/// The RCAS quad: the shown camera's in place of [`ShownQuad`] for fsr.
#[derive(Component)]
struct RcasQuad;

/// Where each output pixel reads: `texel = (pixel − origin) / factor + start`,
/// `pixel` a fragment's framebuffer position, `texel` in texels (texel `i`
/// spans `i .. i + 1`).
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct Mapping {
    pub origin: Vec2,
    pub start: Vec2,
    pub factor: f32,
}

impl Mapping {
    /// The pixel a texel point lands on: the inverse.
    #[must_use]
    pub fn pixel(self, texel: Vec2) -> Vec2 {
        self.origin + (texel - self.start) * self.factor
    }

    /// EASU's constants for this mapping, from an image `input` texels to
    /// one `output` pixels: the texels the output covers are the
    /// viewport, and the offset puts pixel middles where [`Mapping`] does.
    ///
    /// ```text
    ///  FsrEasuF: pp = ip * con0.xy + con0.zw
    ///              = (ip + 0.5) / factor − 0.5 + offset
    ///  Mapping:  pp = (ip + 0.5 − origin) / factor + start − 0.5
    ///  so offset = start − origin / factor
    /// ```
    #[must_use]
    pub fn easu(self, input: UVec2, output: UVec2) -> EasuCon {
        let output = output.as_vec2();

        EasuCon::offset(
            output / self.factor,
            input.as_vec2(),
            output,
            self.start - self.origin / self.factor,
        )
    }
}

/// `FsrEasuCon`'s four words (`ffx_fsr1.h`): `f32` bits, as `AU1_AF1`.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct EasuCon {
    pub con0: UVec4,
    pub con1: UVec4,
    pub con2: UVec4,
    pub con3: UVec4,
}

impl EasuCon {
    /// `FsrEasuConOffset`, operation for operation: `viewport` input
    /// pixels, from `offset` in an image `input` pixels, scaled to
    /// `output` pixels.
    #[must_use]
    #[allow(clippy::neg_multiply, reason = "as the header writes it")]
    pub fn offset(viewport: Vec2, input: Vec2, output: Vec2, offset: Vec2) -> EasuCon {
        // ARcpF1 on the CPU: `1.0f/a`.
        let rcp = |a: f32| 1.0 / a;
        let word = |a: f32| a.to_bits();

        // Output integer position to a pixel position in viewport.
        let con0 = UVec4::new(
            word(viewport.x * rcp(output.x)),
            word(viewport.y * rcp(output.y)),
            word(0.5 * viewport.x * rcp(output.x) - 0.5 + offset.x),
            word(0.5 * viewport.y * rcp(output.y) - 0.5 + offset.y),
        );

        // Viewport pixel position to normalized image space, and the
        // gathers' middles: (0) from 'F''s upper left, the others from (0).
        let con1 = UVec4::new(
            word(rcp(input.x)),
            word(rcp(input.y)),
            word(1.0 * rcp(input.x)),
            word(-1.0 * rcp(input.y)),
        );
        let con2 = UVec4::new(
            word(-1.0 * rcp(input.x)),
            word(2.0 * rcp(input.y)),
            word(1.0 * rcp(input.x)),
            word(2.0 * rcp(input.y)),
        );
        let con3 = UVec4::new(word(0.0 * rcp(input.x)), word(4.0 * rcp(input.y)), 0, 0);

        EasuCon {
            con0,
            con1,
            con2,
            con3,
        }
    }
}

/// `FsrRcasCon`'s words (`ffx_fsr1.h`) for `stops` below the most
/// sharpness: `2^-stops`'s bits. Word 1, the 16-bit path's packed copy,
/// is left 0: that path is not ported.
#[must_use]
pub fn rcas_con(stops: f32) -> UVec4 {
    UVec4::new((-stops).exp2().to_bits(), 0, 0, 0)
}

/// A picture scaled sharp-bilinear: nearest within each texel, a linear
/// blend one output pixel wide at its edges. At one texel a pixel, a copy.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Sharp {
    #[uniform(0)]
    pub mapping: Mapping,
    #[texture(1)]
    #[sampler(2)]
    pub picture: Handle<Image>,
}

impl Material2d for Sharp {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/sharp.wesl")).with_source("embedded"),
        )
    }
}

/// A picture scaled by FSR 1's EASU.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct EasuPass {
    #[uniform(0)]
    pub con: EasuCon,
    #[texture(1)]
    #[sampler(2)]
    pub picture: Handle<Image>,
}

impl Material2d for EasuPass {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/easu.wesl")).with_source("embedded"),
        )
    }
}

/// RCAS's uniform: AMD's constant, and the map rect's corner in the
/// window's pixels, the easu image's first pixel.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct Rcas {
    pub con: UVec4,
    pub corner: Vec2,
}

/// The easu image sharpened by FSR 1's RCAS, one pixel a pixel.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct RcasPass {
    #[uniform(0)]
    pub rcas: Rcas,
    #[texture(1)]
    pub picture: Handle<Image>,
}

impl Material2d for RcasPass {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/rcas.wesl")).with_source("embedded"),
        )
    }
}

/// The two images and the three quads' materials.
#[derive(Resource)]
struct Targets {
    world: Handle<Image>,
    easu: Handle<Image>,
    sharp: Handle<Sharp>,
    pass: Handle<EasuPass>,
    rcas: Handle<RcasPass>,
}

/// The three quads' material stores.
type Materials<'w> = (
    ResMut<'w, Assets<Sharp>>,
    ResMut<'w, Assets<EasuPass>>,
    ResMut<'w, Assets<RcasPass>>,
);

/// World units a texel: an art pixel, or a physical pixel where that is
/// larger (zoomed out past a physical pixel an art pixel).
///
/// ```text
///  zoom 2, scale 1.8: 1     (3.6 pixels a texel)
///  zoom 0.5, scale 1: 2     (1 pixel a texel)
/// ```
#[must_use]
pub fn texel(zoom: f32, scale: f32) -> f32 {
    (1.0 / (zoom * scale)).max(1.0)
}

/// The world image's layout for a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    /// World units a texel ([`texel`]).
    pub texel: f32,
    /// Physical pixels a texel.
    pub factor: f32,
    /// Texels, even, so the camera sits on a texel corner.
    pub size: UVec2,
    /// The world camera's place: the pan snapped to the texels (`SNAP`).
    pub camera: Vec2,
    /// The texel point the pan lands on: the image's middle plus the
    /// remainder the snap left.
    pub start: Vec2,
}

impl Grid {
    /// The layout for a map rect `room` physical pixels, at `zoom` and
    /// `scale`, looking at `pan`; `held` the image's size now, kept
    /// while it fits and is no more than a `STEP` too large.
    #[must_use]
    pub fn of(room: Vec2, (zoom, scale): (f32, f32), pan: Vec2, held: UVec2) -> Grid {
        let texel = texel(zoom, scale);
        let factor = texel * zoom * scale;
        let size = allotted(needed(room, factor), held);
        let camera = ((pan / texel - SNAP).round() + SNAP) * texel;
        let left = (pan - camera) / texel;

        // Image rows count down, world y up.
        let start = size.as_vec2() / 2.0 + Vec2::new(left.x, -left.y);

        Grid {
            texel,
            factor,
            size,
            camera,
            start,
        }
    }

    /// The scaled world's mapping, the map rect's middle at `middle`
    /// physical pixels. The image's corner is put on a whole pixel, so
    /// at a whole factor each texel covers whole pixels and nothing
    /// blends; a pan moves the picture a pixel at a time.
    #[must_use]
    pub fn mapping(&self, middle: Vec2) -> Mapping {
        let corner = self.corner(middle);

        Mapping {
            origin: middle,
            start: (middle - corner) / self.factor,
            factor: self.factor,
        }
    }

    /// The image's corner on the window, a whole pixel ([`Grid::mapping`]):
    /// at one texel a pixel, window pixel `p` shows texel `p − corner`.
    #[must_use]
    pub fn corner(&self, middle: Vec2) -> Vec2 {
        (middle - self.start * self.factor).round()
    }
}

/// Texels to cover `room` physical pixels at `factor` a texel, and
/// [`PAD`] each side.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a window's pixels, non-negative"
)]
fn needed(room: Vec2, factor: f32) -> UVec2 {
    let inside = (room / factor).ceil().max(Vec2::ONE);

    UVec2::new(inside.x as u32, inside.y as u32) + UVec2::splat(2 * PAD)
}

/// `held` while it covers `need` with less than a [`STEP`] to spare, else
/// `need` rounded up to a [`STEP`].
fn allotted(need: UVec2, held: UVec2) -> UVec2 {
    let fits = need.cmple(held).all() && (held - need.min(held)).cmplt(UVec2::splat(STEP)).all();

    if fits {
        held
    } else {
        UVec2::new(need.x.div_ceil(STEP), need.y.div_ceil(STEP)) * STEP
    }
}

/// An image a camera draws into and a quad reads, `size` texels.
fn target(size: UVec2, sampler: ImageSampler) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: size.x,
            height: size.y,
            ..default()
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    image.sampler = sampler;
    image
}

/// The images, the two cameras and their quads, idle until a mode asks.
fn spawn(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    (mut sharps, mut passes, mut rcases): Materials,
) {
    // Linear: sharp-bilinear blends at texel edges. EASU gathers texels,
    // RCAS loads them.
    let world = images.add(target(UVec2::splat(STEP), ImageSampler::linear()));
    let easu = images.add(target(UVec2::splat(STEP), ImageSampler::nearest()));
    let sharp = sharps.add(Sharp {
        mapping: Mapping::default(),
        picture: world.clone(),
    });
    let pass = passes.add(EasuPass {
        con: EasuCon::default(),
        picture: world.clone(),
    });
    let rcas = rcases.add(RcasPass {
        rcas: Rcas::default(),
        picture: easu.clone(),
    });
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let idle = |order| Camera {
        order,
        is_active: false,
        ..default()
    };

    commands.spawn((
        Shown,
        Over,
        Camera2d,
        Msaa::Off,
        idle(SHOWN_ORDER),
        RenderLayers::from_layers(&[SHOWN_LAYER, OVERLAY]),
    ));
    commands.spawn((
        ShownQuad,
        Mesh2d(quad.clone()),
        MeshMaterial2d(sharp.clone()),
        RenderLayers::layer(SHOWN_LAYER),
    ));
    commands.spawn((
        RcasQuad,
        Mesh2d(quad.clone()),
        MeshMaterial2d(rcas.clone()),
        RenderLayers::layer(SHOWN_LAYER),
        Visibility::Hidden,
    ));
    commands.spawn((
        Easu,
        Camera2d,
        Msaa::Off,
        idle(EASU_ORDER),
        RenderTarget::Image(ImageRenderTarget {
            handle: easu.clone(),
            scale_factor: 1.0,
        }),
        RenderLayers::layer(EASU_LAYER),
    ));
    commands.spawn((
        EasuQuad,
        Mesh2d(quad),
        MeshMaterial2d(pass.clone()),
        RenderLayers::layer(EASU_LAYER),
    ));

    commands.insert_resource(Targets {
        world,
        easu,
        sharp,
        pass,
        rcas,
    });
}

/// Point the cameras where the route says, on its change: the world
/// camera at the window or the world image, the others on or off, and
/// the shown camera's quad: sharp's, RCAS's, or none while straight.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
fn route(
    mut commands: Commands,
    (mode, routed): (Res<ViewUpscale>, Res<Routed>),
    targets: Res<Targets>,
    view: Res<View>,
    mut quads: Query<(&mut Visibility, Has<RcasQuad>), Or<(With<ShownQuad>, With<RcasQuad>)>>,
    world: Single<(Entity, &mut Camera, &mut RenderTarget, &mut Projection), With<WorldCamera>>,
    shown: Single<&mut Camera, (With<Shown>, Without<WorldCamera>)>,
    easu: Single<&mut Camera, (With<Easu>, Without<WorldCamera>, Without<Shown>)>,
) {
    let (entity, mut camera, mut target, mut projection) = world.into_inner();
    let (mut shown, mut easu) = (shown.into_inner(), easu.into_inner());
    let route = routed.0;
    let scaled = route != Route::Window;
    let straight = route == Route::Straight;

    // The map rect's viewport stays with whichever camera draws it.
    if scaled {
        let image = if route == Route::Physical {
            &targets.easu
        } else {
            &targets.world
        };

        shown.viewport = camera.viewport.take().or(shown.viewport.take());
        *target = RenderTarget::Image(ImageRenderTarget {
            handle: image.clone(),
            scale_factor: 1.0,
        });
        commands
            .entity(entity)
            .insert(RenderLayers::from_layers(&[0, crate::sun::LIT_LAYER]));
    } else {
        camera.viewport = camera.viewport.take().or(shown.viewport.take());
        *target = RenderTarget::Window(WindowRef::Primary);
        commands.entity(entity).insert(RenderLayers::from_layers(&[
            0,
            OVERLAY,
            crate::sun::LIT_LAYER,
        ]));

        if let Projection::Orthographic(ortho) = &mut *projection {
            ortho.scaling_mode = ScalingMode::WindowSize;
            ortho.scale = 1.0 / view.zoom;
        }
    }

    // Straight, the resolve writes the shown camera's main texture
    // (`follow` places it), and the shown camera draws over it, as it drew
    // over the quad.
    let (written, cleared) = if straight {
        (Written::Under(IVec2::ZERO), ClearColorConfig::None)
    } else {
        (Written::Output, ClearColorConfig::Default)
    };

    commands.entity(entity).insert(written);
    shown.clear_color = cleared;
    shown.is_active = scaled;
    easu.is_active = *mode == ViewUpscale::Fsr;

    // Sharp shows the world, fsr RCAS's pass over the easu image; straight
    // shows no quad.
    for (mut visibility, rcas) in &mut quads {
        let drawn = if rcas {
            mode.sharpened()
        } else {
            !mode.sharpened() && !straight
        };

        *visibility = if drawn {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// This frame's route, written on change.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn choose(
    mode: Res<ViewUpscale>,
    view: Res<View>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut routed: ResMut<Routed>,
) {
    routed.set_if_neq(Routed(mode.route(view.zoom * window.scale_factor())));
}

/// Each quad's material on while its mode is shown, off otherwise: Bevy
/// checks every material type with an entity for specialization each
/// frame, a parallel pass costing about as much for one hidden quad as for
/// every piece's body (**Inferred** from a timeline: task start-up, not
/// entities).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn equip(
    mut commands: Commands,
    mode: Res<ViewUpscale>,
    targets: Res<Targets>,
    shown: Single<Entity, With<ShownQuad>>,
    rcas: Single<Entity, With<RcasQuad>>,
    easu: Single<Entity, With<EasuQuad>>,
) {
    let sharp = *mode == ViewUpscale::Sharp;
    let fsr = *mode == ViewUpscale::Fsr;

    wear(&mut commands, *shown, sharp.then(|| targets.sharp.clone()));
    wear(
        &mut commands,
        *rcas,
        mode.sharpened().then(|| targets.rcas.clone()),
    );
    wear(&mut commands, *easu, fsr.then(|| targets.pass.clone()));
}

/// `entity` drawn with `material`, or with none.
fn wear<M: Material2d>(commands: &mut Commands, entity: Entity, material: Option<Handle<M>>) {
    match material {
        Some(handle) => commands.entity(entity).insert(MeshMaterial2d(handle)),
        None => commands.entity(entity).remove::<MeshMaterial2d<M>>(),
    };
}

/// The cameras, quads, images and mappings for this frame's view, while
/// scaled: the world camera snapped to its texels at one a texel, the
/// shown camera where the world camera stands without this module.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
fn follow(
    (mode, routed): (Res<ViewUpscale>, Res<Routed>),
    view: Res<View>,
    targets: Res<Targets>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    (mut sharps, mut passes, mut rcases): Materials,
    mut cameras: ParamSet<(
        Single<
            (
                &mut Camera,
                &mut Transform,
                &mut Projection,
                Option<&mut Written>,
            ),
            With<WorldCamera>,
        >,
        Single<(&mut Camera, &mut Transform, &mut Projection), With<Shown>>,
    )>,
    mut quads: ParamSet<(
        Query<&mut Transform, (Or<(With<ShownQuad>, With<RcasQuad>)>, Without<Camera>)>,
        Single<&mut Transform, (With<EasuQuad>, Without<Camera>)>,
    )>,
) {
    let Some(rect) = view
        .rect
        .filter(|_| matches!(routed.0, Route::Grid | Route::Straight))
    else {
        return;
    };
    let scale = window.scale_factor();

    // The map rect's viewport, given the world camera by the panels.
    let viewport = {
        let given = cameras.p0().into_inner().0.viewport.take();
        let (mut shown, ..) = cameras.p1().into_inner();

        shown.viewport = given.or(shown.viewport.take());
        shown.viewport.clone()
    };
    let Some(viewport) = viewport else {
        return;
    };

    let held = images.get(&targets.world).map_or(UVec2::ZERO, Image::size);
    let grid = Grid::of(
        viewport.physical_size.as_vec2(),
        (view.zoom, scale),
        view.pan,
        held,
    );

    // A new size: a new image, and its readers told.
    if grid.size != held {
        let _ = images.insert(&targets.world, target(grid.size, ImageSampler::linear()));
    }

    {
        let (_, mut transform, mut projection, written) = cameras.p0().into_inner();

        transform.translation = grid.camera.extend(transform.translation.z);

        if let Projection::Orthographic(ortho) = &mut *projection {
            let side = grid.size.as_vec2() * grid.texel;

            ortho.scaling_mode = ScalingMode::Fixed {
                width: side.x,
                height: side.y,
            };
            ortho.scale = 1.0;
        }

        // Straight: window pixel p takes texel p − corner, as the quad.
        let under = Written::Under(grid.corner(rect.center() * scale).as_ivec2());

        if routed.0 == Route::Straight
            && let Some(mut written) = written
        {
            written.set_if_neq(under);
        }
    }

    // As `stage::aim` puts the world camera without this module.
    {
        let (_, mut transform, mut projection) = cameras.p1().into_inner();

        transform.translation = view.pan.extend(transform.translation.z);

        if let Projection::Orthographic(ortho) = &mut *projection {
            ortho.scale = 1.0 / view.zoom;
        }
    }

    // The quads over the whole rect, and a little past it.
    for mut quad in &mut quads.p0() {
        quad.translation = view.pan.extend(QUAD_Z);
        quad.scale = (rect.size() / view.zoom * 2.0).extend(1.0);
    }

    // The easu image is the map rect's pixels.
    let easu_size = viewport.physical_size;
    let middle = rect.center() * scale;
    let corner = viewport.physical_position.as_vec2();
    let shown = grid.mapping(middle);
    let (con, rcas) = match *mode {
        ViewUpscale::Fsr => (
            grid.mapping(middle - corner).easu(grid.size, easu_size),
            Rcas {
                con: rcas_con(SHARPNESS),
                corner,
            },
        ),
        ViewUpscale::Nearest
        | ViewUpscale::Sharp
        | ViewUpscale::SharpSprite
        | ViewUpscale::FsrSprite => (EasuCon::default(), Rcas::default()),
    };

    let easu_remade =
        *mode == ViewUpscale::Fsr && images.get(&targets.easu).map(Image::size) != Some(easu_size);

    if easu_remade {
        let _ = images.insert(&targets.easu, target(easu_size, ImageSampler::nearest()));
    }

    if *mode == ViewUpscale::Fsr {
        quads.p1().into_inner().scale = easu_size.as_vec2().extend(1.0);
    }

    // Written on change, or when an image was remade under it.
    let remade = grid.size != held || easu_remade;

    if let Some(sharp) = sharps.get(&targets.sharp)
        && (sharp.mapping != shown || remade)
        && let Some(mut sharp) = sharps.get_mut(&targets.sharp)
    {
        sharp.mapping = shown;
    }

    if let Some(easu) = passes.get(&targets.pass)
        && (easu.con != con || remade)
        && let Some(mut easu) = passes.get_mut(&targets.pass)
    {
        easu.con = con;
    }

    if let Some(pass) = rcases.get(&targets.rcas)
        && (pass.rcas != rcas || remade)
        && let Some(mut pass) = rcases.get_mut(&targets.rcas)
    {
        pass.rcas = rcas;
    }
}

/// The world camera's place for `pan`, at `ppu` physical pixels a world
/// unit, in a viewport `size` pixels: within half a pixel of `pan`, and
/// world point 0 on a pixel corner, so at a whole `ppu` every whole
/// world unit is.
///
/// ```text
///  pixel x = middle + (x − camera) · ppu      middle: size / 2, maybe a half
///  pixel y = middle − (y − camera) · ppu      (rows count down)
/// ```
#[must_use]
pub fn pinned(pan: Vec2, ppu: f32, size: UVec2) -> Vec2 {
    let middle = (size.as_vec2() / 2.0).fract();
    let x = ((pan.x * ppu - middle.x).round() + middle.x) / ppu;
    let y = ((pan.y * ppu + middle.y).round() - middle.y) / ppu;

    Vec2::new(x, y)
}

/// The sprite modes' cameras for this frame: the world camera pinned to
/// the pixels ([`pinned`]); for fsr-sprite drawing the map rect's pixels
/// into the easu image, which the shown camera's RCAS quad reads 1:1.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
fn sprites(
    mode: Res<ViewUpscale>,
    view: Res<View>,
    targets: Res<Targets>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut rcases: ResMut<Assets<RcasPass>>,
    mut cameras: ParamSet<(
        Single<(&mut Camera, &mut Transform, &mut Projection), With<WorldCamera>>,
        Single<(&mut Camera, &mut Transform, &mut Projection), With<Shown>>,
    )>,
    mut quads: Query<&mut Transform, (With<RcasQuad>, Without<Camera>)>,
) {
    let Some(rect) = view.rect.filter(|_| mode.sampling() == Sampling::Sharp) else {
        return;
    };
    let scale = window.scale_factor();
    let ppu = view.zoom * scale;

    // As nearest: the world camera's own viewport, given by the panels.
    if mode.route(ppu) == Route::Window {
        let (camera, mut transform, _) = cameras.p0().into_inner();
        let size = camera
            .viewport
            .as_ref()
            .map_or(window.physical_size(), |v| v.physical_size);

        transform.translation = pinned(view.pan, ppu, size).extend(transform.translation.z);
        return;
    }

    let viewport = {
        let given = cameras.p0().into_inner().0.viewport.take();
        let (mut shown, ..) = cameras.p1().into_inner();

        shown.viewport = given.or(shown.viewport.take());
        shown.viewport.clone()
    };
    let Some(viewport) = viewport else {
        return;
    };
    let size = viewport.physical_size;
    let remade = images.get(&targets.easu).map(Image::size) != Some(size);

    if remade {
        let _ = images.insert(&targets.easu, target(size, ImageSampler::nearest()));
    }

    {
        let (_, mut transform, mut projection) = cameras.p0().into_inner();

        transform.translation = pinned(view.pan, ppu, size).extend(transform.translation.z);

        if let Projection::Orthographic(ortho) = &mut *projection {
            let span = size.as_vec2() / ppu;

            ortho.scaling_mode = ScalingMode::Fixed {
                width: span.x,
                height: span.y,
            };
            ortho.scale = 1.0;
        }
    }

    // As `stage::aim` puts the world camera without this module.
    {
        let (_, mut transform, mut projection) = cameras.p1().into_inner();

        transform.translation = view.pan.extend(transform.translation.z);

        if let Projection::Orthographic(ortho) = &mut *projection {
            ortho.scale = 1.0 / view.zoom;
        }
    }

    for mut quad in &mut quads {
        quad.translation = view.pan.extend(QUAD_Z);
        quad.scale = (rect.size() / view.zoom * 2.0).extend(1.0);
    }

    let rcas = Rcas {
        con: rcas_con(SHARPNESS),
        corner: viewport.physical_position.as_vec2(),
    };

    if let Some(pass) = rcases.get(&targets.rcas)
        && (pass.rcas != rcas || remade)
        && let Some(mut pass) = rcases.get_mut(&targets.rcas)
    {
        pass.rcas = rcas;
    }
}

/// After the panels give the map its rect (`bevy_egui`'s pass), before
/// the cameras and transforms are worked out.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Present;

/// The materials, cameras and the systems that drive them; idle while
/// [`ViewUpscale::Nearest`].
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sharp.wesl");
        embedded_asset!(app, "shaders/easu.wesl");
        embedded_asset!(app, "shaders/rcas.wesl");

        app.add_plugins((
            Material2dPlugin::<Sharp>::default(),
            Material2dPlugin::<EasuPass>::default(),
            Material2dPlugin::<RcasPass>::default(),
        ))
        .init_resource::<ViewUpscale>()
        .insert_resource(Routed(Route::Window))
        .configure_sets(
            PostUpdate,
            Present
                .after(bevy_egui::EguiPostUpdateSet::EndPass)
                .before(CameraUpdateSystems)
                .before(TransformSystems::Propagate),
        )
        .add_systems(Startup, spawn)
        .add_systems(
            PostUpdate,
            (
                choose,
                route.run_if(resource_changed::<ViewUpscale>.or_else(resource_changed::<Routed>)),
                equip.run_if(resource_changed::<ViewUpscale>),
                follow,
                sprites,
            )
                .chain()
                .in_set(Present),
        )
        .add_plugins(Rekeyed);
    }
}

/// The 2D cameras' items keyed by their z each frame (`sort::rekey_2d`):
/// a hit point bar's fill queued apart from its frame, after the bar's z
/// moved, sorted under the frame.
pub struct Rekeyed;

impl bevy::app::Plugin for Rekeyed {
    fn build(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(
                Render,
                crate::sort::rekey_2d
                    .in_set(RenderSystems::PhaseSort)
                    .before(sort_phase_system::<Transparent2d>),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map rect, offset by panels, as openregent's.
    fn view(zoom: f32, pan: Vec2) -> View {
        View {
            rect: Some(Rect::new(320.0, 40.0, 1120.0, 640.0)),
            zoom,
            pan,
            ..default()
        }
    }

    /// Where a world point lands on the window, through the image: its
    /// texel in the image, then the mapping back to a pixel.
    fn through(grid: &Grid, mapping: Mapping, world: Vec2) -> Vec2 {
        let from = (world - grid.camera) / grid.texel;
        let texel = grid.size.as_vec2() / 2.0 + Vec2::new(from.x, -from.y);

        mapping.pixel(texel)
    }

    /// The scaled world lands where nearest draws it, and where picking
    /// looks: [`View::screen_at`] at the display's scale.
    #[test]
    fn the_scaled_world_lands_where_the_view_puts_it() {
        let sizes = [(1.0, 1.8), (2.0, 1.8), (4.0, 1.25), (0.5, 1.0), (0.75, 2.0)];

        for (zoom, scale) in sizes {
            for pan in [
                Vec2::ZERO,
                Vec2::new(10.3, -7.61),
                Vec2::new(-333.5, 812.25),
            ] {
                let view = view(zoom, pan);
                let rect = view.rect.unwrap();
                let grid = Grid::of(rect.size() * scale, (zoom, scale), pan, UVec2::ZERO);
                let mapping = grid.mapping(rect.center() * scale);

                for world in [pan, pan + Vec2::new(123.0, -45.5), Vec2::new(-8.0, 16.0)] {
                    let drawn = through(&grid, mapping, world);
                    let asked = view.screen_at(world).unwrap() * scale;

                    // Within the half pixel the corner's rounding moves it.
                    assert!(
                        (drawn - asked).abs().max_element() <= 0.5 + 1e-3,
                        "zoom {zoom}, scale {scale}, pan {pan}: {world} drawn at {drawn}, not {asked}"
                    );
                }
            }
        }
    }

    /// The camera snapped, texel middles a quarter past whole units, the
    /// remainder under a texel.
    #[test]
    fn the_camera_snaps_to_the_texels() {
        let grid = Grid::of(
            Vec2::new(1440.0, 1080.0),
            (2.0, 1.8),
            Vec2::new(10.3, -7.6),
            UVec2::ZERO,
        );
        let middle = grid.camera + Vec2::new(0.5, -0.5) * grid.texel;

        assert_eq!(grid.camera, Vec2::new(10.75, -7.25));
        assert_eq!(middle.rem_euclid(Vec2::ONE), Vec2::splat(0.25));
        assert!(grid.size.x.is_multiple_of(2) && grid.size.y.is_multiple_of(2));
        assert!(
            (grid.start - grid.size.as_vec2() / 2.0 - Vec2::new(-0.45, 0.35)).length() < 1e-4,
            "{}",
            grid.start
        );
    }

    /// One texel an art pixel down to a physical pixel an art pixel;
    /// below, a physical pixel a texel.
    #[test]
    fn a_texel_is_an_art_pixel_unless_smaller_than_a_pixel() {
        assert!((texel(4.0, 1.8) - 1.0).abs() < 1e-6);
        assert!((texel(1.0 / 1.8, 1.8) - 1.0).abs() < 1e-6);
        assert!((texel(0.5, 1.0) - 2.0).abs() < 1e-6);
        assert!((texel(0.5, 1.8) - 1.0 / 0.9).abs() < 1e-6);
    }

    /// The image covers the room and its pad, and keeps its size while
    /// that holds within a step.
    #[test]
    fn the_image_is_remade_only_past_a_step() {
        let room = Vec2::new(1440.0, 1080.0);
        let first = Grid::of(room, (2.0, 1.8), Vec2::ZERO, UVec2::ZERO);

        // 1440 / 3.6 = 400, 1080 / 3.6 = 300, and a pad each side.
        assert_eq!(first.size, UVec2::new(448, 320));

        let nudged = Grid::of(room * 1.02, (2.0, 1.8), Vec2::ZERO, first.size);

        assert_eq!(nudged.size, first.size);

        let halved = Grid::of(room, (4.0, 1.8), Vec2::ZERO, first.size);

        assert_eq!(halved.size, UVec2::new(256, 192));
    }

    /// A word of [`EasuCon`] back to its `f32`.
    fn f(word: u32) -> f32 {
        f32::from_bits(word)
    }

    /// `FsrEasuCon` on the header's own example: a 1920 × 1080 viewport
    /// of a 3840 × 2160 image to 2560 × 1440, worked by hand from the
    /// header's formulas.
    #[test]
    #[allow(clippy::float_cmp, reason = "the header's arithmetic, bit for bit")]
    fn easu_con_matches_the_header_example() {
        let con = EasuCon::offset(
            Vec2::new(1920.0, 1080.0),
            Vec2::new(3840.0, 2160.0),
            Vec2::new(2560.0, 1440.0),
            Vec2::ZERO,
        );
        let (x, y) = (1.0 / 3840.0, 1.0 / 2160.0);

        // 1920 / 2560 = 0.75; 0.5 · 0.75 − 0.5 = −0.125.
        assert_eq!(con.con0.to_array().map(f), [0.75, 0.75, -0.125, -0.125]);
        assert_eq!(con.con1.to_array().map(f), [x, y, x, -y]);
        assert_eq!(con.con2.to_array().map(f), [-x, 2.0 * y, x, 2.0 * y]);
        assert_eq!(con.con3.to_array().map(f), [0.0, 4.0 * y, 0.0, 0.0]);
    }

    /// EASU's `pp` for an output pixel is the texel point [`Mapping`]
    /// reads at the pixel's middle, less half a texel (`FsrEasuF` puts
    /// texel middles on whole numbers).
    #[test]
    fn easu_con_reads_where_the_mapping_does() {
        let room = Vec2::new(1440.0, 1080.0);
        let grid = Grid::of(room, (2.0, 1.8), Vec2::new(10.3, -7.6), UVec2::ZERO);
        let mapping = grid.mapping(room / 2.0);
        let con = mapping.easu(grid.size, UVec2::new(1440, 1080));
        let scale = Vec2::new(f(con.con0.x), f(con.con0.y));
        let at = Vec2::new(f(con.con0.z), f(con.con0.w));

        for ip in [Vec2::ZERO, Vec2::new(17.0, 3.0), Vec2::new(1439.0, 1079.0)] {
            let pp = ip * scale + at;
            let asked = (ip + 0.5 - mapping.origin) / mapping.factor + mapping.start - 0.5;

            assert!(
                (pp - asked).abs().max_element() < 1e-3,
                "{ip}: {pp}, not {asked}"
            );
        }
    }

    /// The settings' names for each mode.
    #[test]
    fn each_mode_reads_by_its_name() {
        let names = [
            ("nearest", ViewUpscale::Nearest),
            ("sharp", ViewUpscale::Sharp),
            ("fsr", ViewUpscale::Fsr),
            ("sharp-sprite", ViewUpscale::SharpSprite),
            ("fsr-sprite", ViewUpscale::FsrSprite),
        ];

        for (name, mode) in names {
            let read: ViewUpscale =
                serde::Deserialize::deserialize(serde::de::value::StrDeserializer::<
                    serde::de::value::Error,
                >::new(name))
                .expect("a mode");

            assert_eq!(read, mode, "{name}");
        }
    }

    /// Pinned, the camera is within half a pixel of the pan, and whole
    /// world units land on pixel corners at a whole factor, odd and even
    /// viewports alike.
    #[test]
    fn the_sprite_camera_pins_whole_units_to_pixels() {
        for (ppu, size) in [(1.0, UVec2::new(800, 600)), (2.0, UVec2::new(801, 599))] {
            for pan in [
                Vec2::ZERO,
                Vec2::new(10.3, -7.61),
                Vec2::new(-333.5, 812.25),
            ] {
                let camera = pinned(pan, ppu, size);
                let middle = size.as_vec2() / 2.0;

                assert!((camera - pan).abs().max_element() <= 0.5 / ppu + 1e-4);

                for world in [Vec2::ZERO, Vec2::new(17.0, -4.0)] {
                    let x = middle.x + (world.x - camera.x) * ppu;
                    let y = middle.y - (world.y - camera.y) * ppu;

                    assert!(
                        (x - x.round()).abs() < 1e-3 && (y - y.round()).abs() < 1e-3,
                        "ppu {ppu}, size {size}, pan {pan}: {world} at {x}, {y}"
                    );
                }
            }
        }
    }

    /// Sharp alone goes straight, and only at one texel a pixel: an art
    /// pixel no larger than a physical pixel.
    #[test]
    fn only_sharp_at_one_texel_a_pixel_goes_straight() {
        let cases = [
            (ViewUpscale::Sharp, 0.5, Route::Straight),
            (ViewUpscale::Sharp, 1.0, Route::Straight),
            (ViewUpscale::Sharp, 1.25, Route::Grid),
            (ViewUpscale::Fsr, 1.0, Route::Grid),
            (ViewUpscale::Nearest, 1.0, Route::Window),
            (ViewUpscale::SharpSprite, 1.0, Route::Window),
            (ViewUpscale::FsrSprite, 1.0, Route::Physical),
        ];

        for (mode, ppu, route) in cases {
            assert_eq!(mode.route(ppu), route, "{mode:?} at {ppu}");
        }
    }

    /// Straight, window pixel p takes texel p − corner: where the quad's
    /// mapping puts that texel, at every zoom that goes straight.
    #[test]
    fn straight_puts_each_texel_where_the_quad_does() {
        for (zoom, scale) in [(1.0, 1.0), (0.75, 1.0), (0.5, 2.0), (0.8, 1.25), (0.3, 1.8)] {
            for pan in [
                Vec2::ZERO,
                Vec2::new(10.3, -7.61),
                Vec2::new(-333.5, 812.25),
            ] {
                let view = view(zoom, pan);
                let rect = view.rect.unwrap();
                let grid = Grid::of(rect.size() * scale, (zoom, scale), pan, UVec2::ZERO);
                let middle = rect.center() * scale;
                let mapping = grid.mapping(middle);
                let corner = grid.corner(middle);

                assert_eq!(mode_at(zoom, scale), Route::Straight);

                for texel in [UVec2::ZERO, UVec2::new(17, 3), grid.size - 1] {
                    let middle_of = texel.as_vec2() + 0.5;
                    let quad = mapping.pixel(middle_of);

                    assert!(
                        (quad - (middle_of + corner)).abs().max_element() < 1e-2,
                        "zoom {zoom}, scale {scale}, pan {pan}: texel {texel} at {quad}"
                    );
                }
            }
        }
    }

    /// Sharp's route at `zoom` and `scale`.
    fn mode_at(zoom: f32, scale: f32) -> Route {
        ViewUpscale::Sharp.route(zoom * scale)
    }

    /// `FsrRcasCon`: stops to `2^-stops`; 0 the most, 1.
    #[test]
    #[allow(clippy::float_cmp, reason = "powers of two, exact in f32")]
    fn rcas_con_is_two_to_minus_stops() {
        assert_eq!(f(rcas_con(0.0).x), 1.0);
        assert_eq!(f(rcas_con(1.0).x), 0.5);
        assert!((f(rcas_con(SHARPNESS).x) - 0.840_896_4).abs() < 1e-6);
    }
}
