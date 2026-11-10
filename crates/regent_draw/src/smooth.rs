//! The art sampled sharp-bilinear where it is drawn, per sprite:
//! `view_upscale = "sharp-sprite"` and `"fsr-sprite"`.
//!
//! The world is drawn at physical pixels, as by nearest; each sprite and
//! the ground smooth their own texels on the way.
//!
//! ```text
//!  sharp, fsr, nearest   Body, Band: the nearest texel, placed as given
//!                        ground: its Plain, patches apron-less
//!  sharp-sprite          Body: SHARP (smooth.wesl's blend); Body, Band:
//!  fsr-sprite              SNAP, each quad's corner on a pixel below
//!                          SNAP_BELOW (shift)
//!                        ground: Sheet, patches with an apron
//!                        (fsr-sprite: then RCAS over the world image)
//! ```
//!
//! A sprite's edges (a neighbour of index 247 and up, or off the tile)
//! stay nearest: indices cannot be averaged, and the band pass draws the
//! shadow indices nearest from the same texels, so colour and shadow
//! meet on one pixel edge.
//!

use bevy::asset::{AssetEventSystems, AssetPath, embedded_asset, embedded_path};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

use crate::material::{Band, Body, CUTOFF};
use crate::plain::{self, GROUND};
use crate::present::ViewUpscale;

/// How the art's texels are read.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sampling {
    /// The nearest texel: as the original draws.
    #[default]
    Nearest,
    /// Sharp-bilinear: flat inside a texel, a blend one physical pixel wide
    /// at its edges; each quad snapped to the pixels ([`shift`]).
    Sharp,
}

/// Physical pixels a texel below which a quad's corner is snapped to a
/// pixel: a cel's edges lie on half world units, and unsnapped at one
/// pixel a texel every pixel would blend half and half. Above, the blend
/// shows the subpixel place. `smooth.wesl`'s `snapped`.
pub const SNAP_BELOW: f32 = 2.0;

/// How far under [`SNAP_BELOW`] a factor must be to snap. A quad's factor
/// comes out of clip space with f32 noise (1.99997 at 2.0, one texel wide),
/// which would flip quads between snapped and not at exactly 2.
pub const SNAP_MARGIN: f32 = 0.01;

/// Pixels added before rounding, past clip space's f32 noise: a corner half
/// way between pixels (a half-unit place at 1×) always rounds up, or a pan
/// can flip the whole piece a pixel. `smooth.wesl`'s `SNAP_TIE`.
pub const SNAP_TIE: f32 = 1.0 / 256.0;

/// How far a quad moves, physical pixels, its texel grid's corner at
/// `corner` (physical pixels, framebuffer coordinates) and `factor`
/// pixels a texel: to the nearest pixel corner below [`SNAP_BELOW`],
/// else not at all. `smooth.wesl`'s `snapped`.
///
/// ```text
///  corner 10.5, factor 1: +0.5   (11, the half rounded up)
///  corner 10.3, factor 3: 0
/// ```
#[must_use]
pub fn shift(corner: Vec2, factor: f32) -> Vec2 {
    if factor < SNAP_BELOW - SNAP_MARGIN {
        (corner + 0.5 + SNAP_TIE).floor() - corner
    } else {
        Vec2::ZERO
    }
}

/// Steps a world unit [`lattice`] rounds a corner to when it takes off the
/// whole units: past a corner's f32 noise, so 123.99999 counts as 124.
pub const LATTICE_STEPS: f32 = 64.0;

/// The world point whose pixel decides how a quad with its corner at
/// world point `corner` moves: the corner's place within its world unit.
/// A piece's frames differ in size and anchor, but their corners lie whole
/// world units apart, so all share this point and move alike. Rounding
/// each corner to a pixel instead moved each frame its own way, shifting
/// it against the next wherever a world unit is not a whole pixel.
/// `smooth.wesl`'s `lattice`.
///
/// ```text
///  corners 123.5 and 120.5: 0.5, both
///  corners 124 and 123.99999: 0, and -0.00001
/// ```
#[must_use]
pub fn lattice(corner: Vec2) -> Vec2 {
    corner - ((corner * LATTICE_STEPS + 0.5).floor() / LATTICE_STEPS).floor()
}

/// How far a quad moves, physical pixels: its texel grid's corner at
/// world point `corner`, `pixel` a world point's physical pixel.
/// `smooth.wesl`'s `snapped`.
#[must_use]
pub fn moved(corner: Vec2, pixel: impl Fn(Vec2) -> Vec2, factor: f32) -> Vec2 {
    shift(pixel(lattice(corner)), factor)
}

/// The composed ground, read sharp-bilinear: the [`Sampling::Sharp`]
/// stand-in for a patch's [`plain::Plain`]. One linear tap at a warped
/// uv, as `sharp.wesl`. Its vertices at their metric points, as the
/// ground's ([`plain::Lay::Ground`]).
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Sheet {
    #[texture(0)]
    #[sampler(1)]
    pub picture: Handle<Image>,
}

impl Material for Sheet {
    fn vertex_shader() -> ShaderRef {
        Sheet::fragment_shader()
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/sheet.wesl")).with_source("embedded"),
        )
    }

    /// As the ground's [`plain::Plain`]: cells without art leave holes.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(CUTOFF)
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        Sheet::shape(descriptor);

        Ok(())
    }
}

impl Sheet {
    /// [`Material::specialize`]'s work, without Bevy's pipeline: the
    /// compile tests call it.
    pub fn shape(descriptor: &mut RenderPipelineDescriptor) {
        plain::unculled(descriptor);
        plain::define(descriptor, &[GROUND.into()]);
    }
}

/// [`Sampling`] as the upscale wants it.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn choose(mode: Res<ViewUpscale>, mut wanted: ResMut<Sampling>) {
    wanted.set_if_neq(mode.sampling());
}

/// The assets an event reader saw added, or all of them on `changed`.
fn touched<A: Asset>(
    made: &mut MessageReader<AssetEvent<A>>,
    assets: &Assets<A>,
    changed: bool,
) -> Vec<AssetId<A>> {
    let added = made.read().filter_map(|event| match event {
        AssetEvent::Added { id } => Some(*id),
        _ => None,
    });

    if changed {
        assets.ids().collect()
    } else {
        added.collect()
    }
}

/// Every body and band to the wanted [`Sampling`], on its change and
/// as they are made (as `shadow::follow` keeps the bands).
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn retag(
    wanted: Res<Sampling>,
    (mut made, mut banded): (
        MessageReader<AssetEvent<Body>>,
        MessageReader<AssetEvent<Band>>,
    ),
    mut bodies: ResMut<Assets<Body>>,
    mut bands: ResMut<Assets<Band>>,
) {
    let changed = wanted.is_changed();

    for id in touched(&mut made, &bodies, changed) {
        let stale = bodies.get(id).is_some_and(|body| body.sampling != *wanted);

        if stale && let Some(mut body) = bodies.get_mut(id) {
            body.sampling = *wanted;
        }
    }

    for id in touched(&mut banded, &bands, changed) {
        let stale = bands.get(id).is_some_and(|band| band.sampling != *wanted);

        if stale && let Some(mut band) = bands.get_mut(id) {
            band.sampling = *wanted;
        }
    }
}

/// [`Sampling`] kept from [`ViewUpscale`], the bodies and bands told;
/// the ground's patches read it as they are made (`map::patch`).
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sheet.wesl");

        app.add_plugins(MaterialPlugin::<Sheet>::default())
            .init_resource::<Sampling>()
            .add_systems(PostUpdate, (choose, retag).chain().after(AssetEventSystems));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Pass;
    use crate::wind::{Gust, Sway};

    /// Below [`SNAP_BELOW`], the corner on the nearest pixel corner, a
    /// half up; above, left where it is.
    #[test]
    fn a_quad_snaps_to_the_pixels_below_two() {
        assert_eq!(shift(Vec2::new(10.5, 3.25), 1.0), Vec2::new(0.5, -0.25));
        assert_eq!(shift(Vec2::new(-2.5, 7.0), 1.5), Vec2::new(0.5, 0.0));
        assert_eq!(shift(Vec2::new(10.5, 3.25), SNAP_BELOW), Vec2::ZERO);
        assert_eq!(shift(Vec2::new(10.3, 3.9), 3.0), Vec2::ZERO);
    }

    /// A world point's physical pixel, the camera pinned as the sprite
    /// modes pin it, in a viewport 1600 x 900 pixels.
    fn pinned_pixel(pan: Vec2, ppu: f32) -> impl Fn(Vec2) -> Vec2 {
        let size = UVec2::new(1600, 900);
        let camera = crate::present::pinned(pan, ppu, size);
        let middle = size.as_vec2() / 2.0;

        move |world: Vec2| {
            let from = (world - camera) * ppu;

            middle + Vec2::new(from.x, -from.y)
        }
    }

    /// Every frame of a layer moves alike at any zoom, or its art shifts
    /// from frame to frame. Their corners are whole world units apart,
    /// which at 1.5 pixels a unit is a fraction of a pixel apart.
    #[test]
    fn a_layers_frames_move_alike_at_any_zoom() {
        // The corners of a layer's frames about its place: -ax, ay.
        let corners = [(-11.0, 30.0), (-10.0, 28.0), (-13.0, 33.0), (-9.0, 27.0)];
        let places = [
            Vec2::new(320.0, -144.0),
            Vec2::new(336.5, 96.5),
            Vec2::new(-208.0, 40.3),
        ];
        let pans = [
            Vec2::ZERO,
            Vec2::new(10.3, -7.61),
            Vec2::new(-333.5, 812.25),
        ];

        for ppu in [0.5, 0.7, 0.84, 1.0, 1.19, 1.25, 1.5, 1.68, 1.9] {
            for (place, pan) in places.into_iter().zip(pans) {
                let pixel = pinned_pixel(pan, ppu);
                let shifts = corners.map(|(x, y)| moved(place + Vec2::new(x, y), &pixel, ppu));

                for shift in shifts {
                    assert!(
                        (shift - shifts[0]).abs().max_element() < 1e-2,
                        "{ppu} pixels a unit, at {place}: {shifts:?}"
                    );
                }
            }
        }
    }

    /// At a factor of exactly [`SNAP_BELOW`] no quad snaps, whatever its
    /// size and place: the factor as `smooth.wesl`'s `snapped` works it
    /// out, through clip space in f32, must not fall under it by noise.
    #[test]
    fn noise_in_the_factor_never_flips_the_snap() {
        const WIDTH: f32 = 1600.0;
        const PPU: f32 = 2.0;

        let camera = crate::present::pinned(Vec2::new(10.3, -7.61), PPU, UVec2::new(1600, 900));
        let pixel = |x: f32| (x - camera.x) * PPU * 2.0 / WIDTH * WIDTH / 2.0 + WIDTH / 2.0;

        for texels in [1_u16, 3, 17, 33, 64, 97, 128, 255] {
            for at in 0..4000_u16 {
                let corner = f32::from(at) * 0.1 - 200.0;
                let across = corner + f32::from(texels);
                let factor = (pixel(across) - pixel(corner)).abs() / f32::from(texels);

                assert_eq!(
                    shift(Vec2::splat(10.3), factor),
                    Vec2::ZERO,
                    "{texels} texels at {corner}: factor {factor}"
                );
            }
        }
    }

    /// A tie blurred by noise still rounds up; the shader rounds alike.
    #[test]
    fn a_noisy_half_rounds_up() {
        let shader = include_str!("shaders/smooth.wesl");

        assert_eq!(10.4999 + shift(Vec2::splat(10.4999), 1.0).x, 11.0);
        assert_eq!(10.5001 + shift(Vec2::splat(10.5001), 1.0).x, 11.0);
        assert_eq!(10.49 + shift(Vec2::splat(10.49), 1.0).x, 10.0);
        assert!(shader.contains(&format!("const SNAP_TIE: f32 = {SNAP_TIE:?};")));
    }

    /// Corners whole units apart share their point, f32 noise or not, and
    /// a point's sign and half-unit fractions come out as `fract` gives.
    #[test]
    fn corners_whole_units_apart_share_a_lattice_point() {
        assert_eq!(lattice(Vec2::new(123.5, -120.5)), Vec2::splat(0.5));
        assert!(lattice(Vec2::new(124.0, 123.99999)).abs().max_element() < 1e-4);
        assert_eq!(lattice(Vec2::new(-3.25, 7.0)), Vec2::new(0.75, 0.0));

        // The fraction kept whole: at one pixel a unit the corner lands on
        // a pixel, not within a step of one.
        assert_eq!(
            lattice(Vec2::splat(40.25 + 1.0 / 512.0)).x,
            0.25 + 1.0 / 512.0
        );
    }

    /// Only the sprite modes read the art sharp.
    #[test]
    fn only_the_sprite_modes_sample_sharp() {
        let sharp = [ViewUpscale::SharpSprite, ViewUpscale::FsrSprite];

        let modes = [
            ViewUpscale::Nearest,
            ViewUpscale::Sharp,
            ViewUpscale::Fsr,
            ViewUpscale::SharpSprite,
            ViewUpscale::FsrSprite,
        ];

        for mode in modes {
            let want = if sharp.contains(&mode) {
                Sampling::Sharp
            } else {
                Sampling::Nearest
            };

            assert_eq!(mode.sampling(), want, "{mode:?}");
        }
    }

    /// A body made before the mode changed and one made after both
    /// follow its sampling, then return to nearest on the art grid.
    #[test]
    fn bodies_follow_the_sampling() {
        let mut app = App::new();

        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Body>()
            .init_asset::<Band>()
            .insert_resource(ViewUpscale::Sharp)
            .init_resource::<Sampling>()
            .add_systems(PostUpdate, (choose, retag).chain().after(AssetEventSystems));

        let body = || Body {
            indices: Handle::default(),
            palette: Handle::default(),
            pass: Pass::Solid,
            sway: Sway::Still,
            gust: Gust::STILL,
            sampling: Sampling::Nearest,
            ..Body::default()
        };
        let old = app.world_mut().resource_mut::<Assets<Body>>().add(body());

        app.update();
        *app.world_mut().resource_mut::<ViewUpscale>() = ViewUpscale::SharpSprite;
        app.update();

        let new = app.world_mut().resource_mut::<Assets<Body>>().add(body());

        app.update();

        let read = |app: &App, handle: &Handle<Body>| {
            app.world()
                .resource::<Assets<Body>>()
                .get(handle)
                .map(|b| b.sampling)
        };
        let sharp = Some(Sampling::Sharp);

        assert_eq!(read(&app, &old), sharp);
        assert_eq!(read(&app, &new), sharp);

        *app.world_mut().resource_mut::<ViewUpscale>() = ViewUpscale::FsrSprite;
        app.update();

        assert_eq!(read(&app, &new), sharp);

        *app.world_mut().resource_mut::<ViewUpscale>() = ViewUpscale::Sharp;
        app.update();

        let nearest = Some(Sampling::Nearest);

        assert_eq!(read(&app, &old), nearest);
        assert_eq!(read(&app, &new), nearest);
    }
}
