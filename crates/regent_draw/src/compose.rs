//! The window written once a frame: one render pass on the last camera
//! ([`Compose`]), in place of each camera's own copy (Bevy's `upscaling`)
//! and of the shadows' resolve into the window.
//!
//! ```text
//!  the window, cleared to the clear colour, then by route ([`Base`]):
//!   straight  the world resolved, read `shift` back, in the map rect;
//!             the shown camera's texture over its marks (the overlays,
//!             drawn over the world resolved under them: Written::Under)
//!   window    the world resolved in the world camera's viewport
//!   grid      the shown camera's texture in the map rect (sharp, RCAS)
//!   menus     nothing
//!  then the compose camera's own texture (egui, over transparent)
//!  blended over its marks
//! ```
//!
//! Each draw is the pipeline the pass it replaces used (the resolve, or
//! Bevy's blit with the same blend), so the picture is the same byte for
//! byte; only the pixels each draws are cut, to [`Marks`]: whole tiles
//! about what was drawn. A tile beyond the drawing blends nothing (egui's
//! texture is clear there) or copies the world unchanged.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraMainTextureUsages, CameraOutputMode};
use bevy::core_pipeline::Core2d;
use bevy::core_pipeline::blit::{BlitPipeline, BlitPipelineKey};
use bevy::core_pipeline::upscaling::upscaling;
use bevy::diagnostic::FrameCount;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::camera::{ExtractedCamera, SortedCameras, sort_cameras};
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_resource::{
    BindGroup, BlendState, PipelineCache, RenderPassDescriptor, RenderPipeline,
    SpecializedRenderPipelines, TextureUsages, TextureView,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{ResolvedCompositingSpace, ViewTarget};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy_egui::render::egui_pass;
use bevy_egui::{EguiOutput, EguiRenderOutput};

use crate::kept;
use crate::present::OVERLAY;
use crate::shadow::{self, Over, Resolve, Resolver, Shift, Written};

/// A tile's side, in physical pixels: what a mark covers at least.
pub const TILE: u32 = 16;

/// Pixels a mark keeps past what it bounds: rounding to pixel centres,
/// and egui's clip rects rounded to whole pixels.
const MARGIN: f32 = 2.0;

/// The last camera: its pass writes the window ([`compose`]). Its own
/// copy is skipped, and its main texture is its own (an extra usage keys
/// it apart from the other cameras on the window), kept between frames
/// ([`kept`]).
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
#[extract_app(RenderApp)]
#[extract_component_filter(With<Camera>)]
#[require(Marks)]
#[require(kept::Wipe)]
#[component(on_add = own_texture)]
pub struct Compose;

/// The compose camera's copy skipped, its main texture its own.
fn own_texture(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut camera) = world.get_mut::<Camera>(context.entity) {
        camera.output_mode = CameraOutputMode::Skip;
    }

    if let Some(mut usages) = world.get_mut::<CameraMainTextureUsages>(context.entity) {
        usages.0 |= TextureUsages::COPY_DST;
    }
}

/// Where a camera's picture may hold anything, in its target's physical
/// pixels: whole tiles ([`TILE`]), merged into rects that do not overlap,
/// so a blend over them blends each pixel once.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, ExtractComponent)]
#[extract_app(RenderApp)]
#[extract_component_filter(With<Camera>)]
pub struct Marks(pub Vec<URect>);

/// A grid of tiles over a picture, each marked or not.
pub(crate) struct Tiles {
    pub(crate) size: UVec2,
    across: u32,
    pub(crate) marked: Vec<bool>,
}

impl Tiles {
    /// No tile marked, over `size` pixels.
    pub(crate) fn new(size: UVec2) -> Tiles {
        let across = size.x.div_ceil(TILE);

        Tiles {
            size,
            across,
            marked: vec![false; (across * size.y.div_ceil(TILE)) as usize],
        }
    }

    /// Every tile `rect` (pixels) touches, and [`MARGIN`] past it.
    fn mark(&mut self, rect: Rect) {
        for at in self.touched(rect) {
            self.marked[at] = true;
        }
    }

    /// The indices of the tiles `rect` (pixels) touches, and [`MARGIN`]
    /// past it.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "clamped to the picture, non-negative; a tile's side"
    )]
    pub(crate) fn touched(&self, rect: Rect) -> impl Iterator<Item = usize> + use<> {
        let size = self.size.as_vec2();
        let min = (rect.min - MARGIN).clamp(Vec2::ZERO, size);
        let max = (rect.max + MARGIN).clamp(Vec2::ZERO, size);
        let tile = TILE as f32;
        let (x0, y0) = ((min.x / tile) as u32, (min.y / tile) as u32);
        let (x1, y1) = ((max.x / tile).ceil() as u32, (max.y / tile).ceil() as u32);
        let across = self.across;

        (y0..y1).flat_map(move |y| (x0..x1).map(move |x| (y * across + x) as usize))
    }

    /// The marked tiles as pixel rects: runs along each row, each grown
    /// down while the rows below hold the same run.
    #[allow(clippy::cast_possible_truncation, reason = "a picture's tile rows")]
    pub(crate) fn rects(&self) -> Vec<URect> {
        let mut open: Vec<URect> = Vec::new();
        let mut done: Vec<URect> = Vec::new();

        for (y, row) in self.marked.chunks(self.across as usize).enumerate() {
            let y = y as u32;
            let runs = runs(row);
            let (kept, ended): (Vec<URect>, Vec<URect>) = open
                .into_iter()
                .partition(|r| runs.contains(&(r.min.x, r.max.x)));

            done.extend(ended);

            // A run open above grows a row; a new one opens.
            open = runs
                .into_iter()
                .map(|(x0, x1)| {
                    kept.iter()
                        .find(|r| (r.min.x, r.max.x) == (x0, x1))
                        .map_or(URect::new(x0, y, x1, y + 1), |r| {
                            URect::from_corners(r.min, UVec2::new(x1, y + 1))
                        })
                })
                .collect();
        }

        done.extend(open);
        done.into_iter()
            .map(|r| URect::from_corners(r.min * TILE, (r.max * TILE).min(self.size)))
            .collect()
    }
}

/// A row's runs of marked tiles, each `(first, past the last)`.
#[allow(clippy::cast_possible_truncation, reason = "a picture's tile columns")]
fn runs(row: &[bool]) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut start = None;

    for (x, marked) in row.iter().chain([&false]).enumerate() {
        let x = x as u32;

        match (start, *marked) {
            (None, true) => start = Some(x),
            (Some(first), false) => {
                runs.push((first, x));
                start = None;
            }
            _ => {}
        }
    }

    runs
}

/// The compose camera's marks: egui's paint, each mesh's bounds within
/// its clip rect, at egui's pixels a point. Then what of egui's texture
/// to draw again ([`kept`]): its paint cut to the tiles that changed.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
fn interface(
    mut cameras: Query<
        (
            &Camera,
            &mut EguiRenderOutput,
            &EguiOutput,
            &mut Marks,
            &mut kept::Wipe,
        ),
        With<Compose>,
    >,
    versions: Res<kept::Versions>,
    steady: Res<kept::Steady>,
    frames: Res<FrameCount>,
    mut held: Local<kept::Held>,
) {
    for (camera, mut output, egui, mut marks, mut wipe) in &mut cameras {
        let Some(size) = camera.physical_target_size() else {
            continue;
        };
        let scale = egui.pixels_per_point;
        let (tiles, now) = held.frame(
            &output.paint_jobs,
            (size, scale),
            (&versions, frames.0, steady.get()),
        );

        marks.set_if_neq(Marks(tiles.rects()));

        if let kept::Wipe::Rects(rects) = &now {
            output.paint_jobs = kept::cut(&output.paint_jobs, rects, scale);
        }

        wipe.set_if_neq(now);
    }
}

/// The over camera's marks: each overlay it draws (a 2D sprite or mesh
/// on [`OVERLAY`]), its bounds through the camera.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
fn overlays(
    mut cameras: Query<(&Camera, &GlobalTransform, &mut Marks), With<Over>>,
    items: Query<
        (&Aabb, &GlobalTransform, &ViewVisibility, &RenderLayers),
        Or<(With<Sprite>, With<Mesh2d>)>,
    >,
) {
    let overlay = RenderLayers::layer(OVERLAY);

    for (camera, eye, mut marks) in &mut cameras {
        let (Some(size), Some(viewport)) = (
            camera.physical_target_size(),
            camera.physical_viewport_rect(),
        ) else {
            continue;
        };
        let mut tiles = Tiles::new(size);
        let shown = items
            .iter()
            .filter(|(_, _, seen, layers)| seen.get() && layers.intersects(&overlay));

        let pixel = |ndc| on_viewport(ndc, viewport);

        for (bounds, place, ..) in shown {
            let (middle, half) = (Vec3::from(bounds.center), Vec3::from(bounds.half_extents));
            let corners = [-1.0, 1.0].into_iter().flat_map(|x| {
                [-1.0, 1.0].map(|y| place.transform_point(middle + half * Vec3::new(x, y, 0.0)))
            });
            let pixels: Option<Vec<Vec2>> = corners
                .map(|c| camera.world_to_ndc(eye, c).map(pixel))
                .collect();

            if let Some(pixels) = pixels {
                let min = pixels.iter().copied().reduce(Vec2::min).unwrap_or_default();
                let max = pixels.iter().copied().reduce(Vec2::max).unwrap_or_default();

                tiles.mark(Rect::from_corners(min, max));
            }
        }

        marks.set_if_neq(Marks(tiles.rects()));
    }
}

/// A point in NDC to the target's pixels, across `viewport`: rows count
/// down.
fn on_viewport(ndc: Vec3, viewport: URect) -> Vec2 {
    let across = ndc.truncate() * Vec2::new(0.5, -0.5) + 0.5;

    viewport.min.as_vec2() + across * viewport.size().as_vec2()
}

/// What the compose pass draws before egui.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Base {
    /// The world resolved, read `at` back, in the over camera's viewport;
    /// then the over camera's picture over its marks.
    Under(IVec2),
    /// The world resolved in its own camera's viewport.
    Resolved,
    /// The over camera's picture in its viewport.
    Over,
    /// Nothing: the clear colour (the menus).
    Bare,
}

/// Whether the over camera draws this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    Yes,
    No,
}

impl Base {
    /// The base for the world camera's [`Written`] (none: no world drawn)
    /// and the over camera.
    const fn of(world: Option<Written>, over: Seen) -> Base {
        match (world, over) {
            (Some(Written::Under(at)), Seen::Yes) => Base::Under(at),
            (Some(Written::Composed), _) => Base::Resolved,
            (Some(Written::Output) | None, Seen::Yes) => Base::Over,
            (Some(Written::Under(_) | Written::Output) | None, Seen::No) => Base::Bare,
        }
    }
}

/// The compose camera counted as writing the window: Bevy takes a
/// window's frame only for a camera that copies into it, and every camera
/// on the window leaves its copy to the compose pass.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn writes(composing: Query<(), With<Compose>>, mut sorted: ResMut<SortedCameras>) {
    for camera in sorted.0.iter_mut().filter(|c| composing.contains(c.entity)) {
        camera.output_mode = CameraOutputMode::Write {
            blend_state: None,
            clear_color: ClearColorConfig::None,
        };
    }
}

/// A copy of `source` by Bevy's blit with `key`, as its `upscaling`
/// makes one; none until the pipeline is ready.
fn copied<'c>(
    (blits, blit): (&mut SpecializedRenderPipelines<BlitPipeline>, &BlitPipeline),
    (device, cache): (&RenderDevice, &'c PipelineCache),
    key: BlitPipelineKey,
    source: &TextureView,
) -> Option<(&'c RenderPipeline, BindGroup)> {
    let id = blits.specialize(cache, blit, key);

    cache
        .get_render_pipeline(id)
        .map(|pipeline| (pipeline, blit.create_bind_group(device, source, cache)))
}

/// Write the window: cleared, the [`Base`], then this camera's own
/// picture blended over its marks, each by the pipeline the pass it
/// replaces used.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
fn compose(
    view: ViewQuery<(&ViewTarget, &Marks, Option<&ResolvedCompositingSpace>), With<Compose>>,
    worlds: Query<(&ViewTarget, &ExtractedCamera, &Written), With<Resolve>>,
    over: Query<
        (
            &ViewTarget,
            &ExtractedCamera,
            &Marks,
            Option<&ResolvedCompositingSpace>,
        ),
        With<Over>,
    >,
    mut resolver: Resolver,
    (blit, mut blits): (
        Res<BlitPipeline>,
        ResMut<SpecializedRenderPipelines<BlitPipeline>>,
    ),
    cache: Res<PipelineCache>,
    clear: Res<ClearColor>,
    mut ctx: RenderContext,
) {
    let (target, marks, space) = view.into_inner();
    let (Some(format), Some(out)) = (
        target.out_texture_view_format(),
        target.out_texture_color_attachment(Some(clear.0.into())),
    ) else {
        return;
    };

    // The one world camera the compose pass reads, and the over camera.
    let world = worlds
        .iter()
        .find(|(.., written)| **written != Written::Output);
    let over = over.single().ok();
    let base = Base::of(
        world.map(|(.., written)| *written),
        if over.is_some() { Seen::Yes } else { Seen::No },
    );

    let device = ctx.render_device().clone();
    let gpu = (&device, cache.as_ref());
    let mut copy = |source, blend, space| {
        let key = BlitPipelineKey {
            target_format: format,
            blend_state: blend,
            samples: 1,
            source_space: ResolvedCompositingSpace::space(space),
        };

        copied((&mut blits, &blit), gpu, key, source)
    };

    // The base's draws: a pipeline, its bind group, and the rects.
    let mut draws = Vec::new();

    match (base, world, over) {
        (Base::Under(at), Some((world, ..)), Some((shown, overs, over_marks, over_space))) => {
            let viewport = overs.viewport.as_ref();
            let world_drawn =
                resolver.draw(gpu, format, world.main_texture_view(), Shift::Under(at));
            let rects: Vec<URect> = over_marks
                .0
                .iter()
                .filter_map(|r| shadow::within(*r, viewport))
                .collect();

            draws.extend(world_drawn.map(|d| (d, shadow::area(overs).into_iter().collect())));
            draws.extend(copy(shown.main_texture_view(), None, over_space).map(|d| (d, rects)));
        }
        (Base::Resolved, Some((world, camera, _)), _) => {
            let world_drawn = resolver.draw(gpu, format, world.main_texture_view(), Shift::None);

            draws.extend(world_drawn.map(|d| (d, shadow::area(camera).into_iter().collect())));
        }
        (Base::Over, _, Some((shown, overs, _, over_space))) => {
            let copied = copy(shown.main_texture_view(), None, over_space);

            draws.extend(copied.map(|d| (d, shadow::area(overs).into_iter().collect())));
        }
        _ => {}
    }

    // egui over all of it, blended as Bevy's copy blended it.
    let blended = copy(
        target.main_texture_view(),
        Some(BlendState::ALPHA_BLENDING),
        space,
    );

    draws.extend(blended.map(|d| (d, marks.0.clone())));

    // Timed when Bevy's render diagnostics are on (openregent's perf runs).
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let mut render_pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("compose"),
            color_attachments: &[Some(out)],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    let span = diagnostics.pass_span(&mut render_pass, "compose");

    for ((pipeline, bind_group), rects) in &draws {
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);

        for rect in rects {
            shadow::scissored(&mut render_pass, *rect);
            render_pass.draw(0..3, 0..1);
        }
    }

    span.end(&mut render_pass);
}

/// Register the pass and the marks.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<Compose>::default(),
            ExtractComponentPlugin::<Marks>::default(),
        ))
        // After the frame's paint, transforms and cameras: as extracted.
        .add_systems(Last, (interface.in_set(Marking), overlays));

        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            // After egui's pass, in place of the final copy.
            render
                .add_systems(
                    Render,
                    writes
                        .in_set(RenderSystems::CreateViews)
                        .after(sort_cameras),
                )
                .add_systems(
                    Core2d,
                    compose.in_set(Composing).after(egui_pass).before(upscaling),
                );
        }
    }
}

/// The compose camera's marks made and egui's paint cut ([`kept`]):
/// what reads egui's whole paint runs before.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Marking;

/// The compose pass in its camera's schedule, for others to order by.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Composing;

#[cfg(test)]
mod tests {
    use super::*;

    /// Each route's base: the world resolved under the overlays
    /// (straight), alone (nearest, sharp-sprite), the shown camera's
    /// picture (grid, fsr, fsr-sprite), nothing (the menus).
    #[test]
    fn each_route_composes_its_own_base() {
        let at = IVec2::new(3, -2);
        let cases = [
            (Some(Written::Under(at)), Seen::Yes, Base::Under(at)),
            (Some(Written::Composed), Seen::No, Base::Resolved),
            (Some(Written::Output), Seen::Yes, Base::Over),
            (None, Seen::No, Base::Bare),
            (Some(Written::Under(at)), Seen::No, Base::Bare),
        ];

        for (world, over, base) in cases {
            assert_eq!(Base::of(world, over), base, "{world:?}, {over:?}");
        }
    }

    /// A mark covers what it bounds and the margin, in whole tiles, cut
    /// at the picture's edge.
    #[test]
    fn a_mark_covers_its_tiles_and_margin() {
        let mut tiles = Tiles::new(UVec2::new(100, 40));

        tiles.mark(Rect::new(20.5, 3.0, 31.0, 14.0));

        // 18.5 .. 33 across: tiles 1 and 2; 1 .. 16 down: tile 0.
        assert_eq!(tiles.rects(), vec![URect::new(16, 0, 48, 16)]);

        tiles.mark(Rect::new(95.0, 30.0, 120.0, 50.0));

        assert!(tiles.rects().contains(&URect::new(80, 16, 100, 40)));
    }

    /// The rects cover the marked tiles once each: runs grown down, a run
    /// that changes width starting a rect of its own.
    #[test]
    fn the_rects_cover_each_marked_tile_once() {
        let size = UVec2::new(TILE * 6, TILE * 4);
        let mut tiles = Tiles::new(size);
        let tile = |x: u32, y: u32| {
            let at = (UVec2::new(x, y) * TILE).as_vec2() + MARGIN + 1.0;

            Rect::from_corners(at, at + 1.0)
        };

        for (x, y) in [
            (0, 0),
            (1, 0),
            (0, 1),
            (1, 1),
            (4, 1),
            (0, 2),
            (2, 2),
            (3, 3),
        ] {
            tiles.mark(tile(x, y));
        }

        let rects = tiles.rects();
        let covered = |x: u32, y: u32| {
            let p = UVec2::new(x, y) * TILE + TILE / 2;

            rects.iter().filter(|r| r.contains(p)).count()
        };

        for y in 0..4 {
            for x in 0..6 {
                let marked = tiles.marked[(y * tiles.across + x) as usize];

                assert_eq!(covered(x, y), usize::from(marked), "tile {x}, {y}");
            }
        }

        assert!(rects.contains(&URect::new(0, 0, 2 * TILE, 2 * TILE)));
    }

    /// An overlay's corners land in the map rect's pixels: NDC's corners
    /// on the viewport's, its top up.
    #[test]
    fn an_overlay_lands_in_the_viewport() {
        let viewport = URect::new(320, 40, 1120, 640);

        assert_eq!(
            on_viewport(Vec3::new(-1.0, 1.0, 0.5), viewport),
            Vec2::new(320.0, 40.0)
        );
        assert_eq!(
            on_viewport(Vec3::new(1.0, -1.0, 0.5), viewport),
            Vec2::new(1120.0, 640.0)
        );
        assert_eq!(on_viewport(Vec3::ZERO, viewport), Vec2::new(720.0, 340.0));
    }

    /// Nothing marked, nothing drawn.
    #[test]
    fn no_mark_draws_nothing() {
        assert_eq!(Tiles::new(UVec2::new(1920, 1080)).rects(), vec![]);
    }
}
