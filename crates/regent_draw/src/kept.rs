//! egui's texture kept from frame to frame: only the tiles whose paint
//! changed are cleared and drawn again, so the texture holds what a whole
//! redraw would.
//!
//! ```text
//!  main world (Last)   each tile's hash: the triangles over it, in draw
//!                      order (corners, colours, uvs, clip rect, texture
//!                      and its version), as compose::Tiles marks them
//!                      tiles whose hash differs from the last frame's:
//!                      dirty; egui's paint cut to them (cut): each job's
//!                      clip rect met with each dirty rect, its triangles
//!                      those that can reach it
//!  whole (Wipe::All)   a change of size or scale, a frame not painted,
//!                      an egui texture freed, a user texture added or
//!                      moved, a paint callback, or the texture not kept
//!                      (Steady)
//!  render world        Wipe::All: the texture cleared, egui draws all;
//!                      Wipe::Rects: each rect cleared (a blit of a clear
//!                      texel, scissored), egui draws its cut paint
//! ```
//!
//! A texture's version moves as its image changes (an asset event), as
//! egui sets it (the font atlas grown) or as a camera draws into it (the
//! tracking window's), so a picture changed
//! under unchanged triangles is drawn again too. egui rounds a clip rect
//! to pixels, and rounding keeps order, so a clip rect met with a dirty
//! rect scissors to the meeting of the two rounded: the same pixels.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::camera::RenderTarget;
use bevy::core_pipeline::Core2d;
use bevy::core_pipeline::Core2dSystems;
use bevy::core_pipeline::blit::{BlitPipeline, BlitPipelineKey};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_resource::{
    Extent3d, LoadOp, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
    SpecializedRenderPipelines, StoreOp, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureView, TextureViewDescriptor,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{ResolvedCompositingSpace, ViewTarget};
use bevy::render::{RenderApp, RenderStartup};
use bevy_egui::egui::epaint::{ClippedPrimitive, Primitive};
use bevy_egui::egui::{self, TextureId};
use bevy_egui::render::egui_pass;
use bevy_egui::{EguiPostUpdateSet, EguiRenderOutput, EguiUserTextures};

use crate::compose::{Compose, Tiles};

/// Pixels past a triangle's bounds it may still reach (`compose`'s
/// marks' margin): rounding to pixel centres, and clip rects rounded.
const REACH: f32 = 2.0;

/// A multiplier of the golden ratio's bits: [`fold`]'s mix.
const MIX: u64 = 0x9e37_79b9_7f4a_7c15;

/// What of egui's texture is drawn again this frame.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, ExtractComponent)]
#[extract_app(RenderApp)]
#[extract_component_filter(With<Camera>)]
pub enum Wipe {
    /// All of it: cleared, egui's whole paint drawn.
    #[default]
    All,
    /// These pixel rects: each cleared, egui's paint cut to them.
    Rects(Vec<URect>),
}

/// Each egui texture's version: moved as its picture changes.
#[derive(Resource, Default)]
pub struct Versions {
    ids: HashMap<TextureId, u64>,
    /// Whether everything is drawn again this frame.
    pub everything: bool,
    /// An egui texture freed this frame, seen before it is drained.
    freed: bool,
}

/// Whether the render world kept egui's texture last frame: its texture
/// the same and its clear ready. Shared by both worlds.
#[derive(Resource, Clone, Default)]
pub struct Steady(Arc<AtomicBool>);

impl Steady {
    #[must_use]
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn set(&self, kept: bool) {
        self.0.store(kept, Ordering::Relaxed);
    }
}

/// One job's paint as last drawn: the hash of all of it, and each tile
/// it reaches with the hash of its triangles there, in order.
#[derive(Default)]
struct Painted {
    hash: u64,
    tiles: HashMap<usize, u64>,
}

/// The last frame's jobs ([`Painted`]), size, scale and frame.
#[derive(Default)]
pub struct Held {
    jobs: Vec<Painted>,
    size: UVec2,
    scale: f32,
    frame: Option<u32>,
}

impl Held {
    /// This frame's `jobs` (`size` pixels, `scale` a point, frame
    /// `frame`) against the last's: the tiles they mark, and what to draw
    /// again. A job whose hash is the same job's of the last frame keeps
    /// its tiles unread; one changed is read triangle by triangle, and its
    /// tiles whose hash moved are drawn again. All of it after a change of
    /// size or scale, a frame missed, everything's version moved, a paint
    /// callback, or the texture not `steady`.
    pub(crate) fn frame(
        &mut self,
        jobs: &[ClippedPrimitive],
        (size, scale): (UVec2, f32),
        (versions, frame, steady): (&Versions, u32, bool),
    ) -> (Tiles, Wipe) {
        #[allow(clippy::float_cmp, reason = "the same scale, bit for bit")]
        let alike = self.size == size
            && self.scale == scale
            && self.frame.is_some_and(|f| f.wrapping_add(1) == frame);
        // The last frame's jobs taken either way: unlike this frame, their
        // tiles are another grid's, and none may stay.
        let last = std::mem::take(&mut self.jobs);
        let mut was = if alike { last } else { Vec::new() };
        let mut marks = Tiles::new(size);
        let mut dirty = Tiles::new(size);
        let mut called = false;

        for (at, job) in jobs.iter().enumerate() {
            let hash = whole(job, versions);
            let old = was.get_mut(at).map(std::mem::take).unwrap_or_default();

            called |= matches!(job.primitive, Primitive::Callback(_));

            let painted = if old.hash == hash && at < was.len() {
                old
            } else {
                let now = Painted {
                    hash,
                    tiles: reached(job, versions, scale, &marks),
                };
                let moved =
                    |t: &usize, h: &u64, other: &HashMap<usize, u64>| other.get(t) != Some(h);

                for (t, h) in &now.tiles {
                    dirty.marked[*t] |= moved(t, h, &old.tiles);
                }

                for (t, h) in &old.tiles {
                    dirty.marked[*t] |= moved(t, h, &now.tiles);
                }

                now
            };

            for t in painted.tiles.keys() {
                marks.marked[*t] = true;
            }

            self.jobs.push(painted);
        }

        // Jobs gone since: their tiles drawn again.
        for gone in was.iter().skip(jobs.len()) {
            for t in gone.tiles.keys() {
                dirty.marked[*t] = true;
            }
        }

        let wipe = if !alike || called || versions.everything || !steady {
            Wipe::All
        } else {
            Wipe::Rects(dirty.rects())
        };

        (self.size, self.scale, self.frame) = (size, scale, Some(frame));

        (marks, wipe)
    }
}

/// `hash` taken into `state`, order kept: (a, b) and (b, a) differ.
#[must_use]
pub const fn fold(state: u64, hash: u64) -> u64 {
    (state.rotate_left(23) ^ hash).wrapping_mul(MIX)
}

/// `bytes` folded eight at a time, the last few padded.
fn folded(state: u64, bytes: &[u8]) -> u64 {
    let (words, rest) = bytes.as_chunks::<8>();
    let mut last = [0; 8];

    last[..rest.len()].copy_from_slice(rest);

    words
        .iter()
        .chain([&last])
        .fold(state, |h, w| fold(h, u64::from_le_bytes(*w)))
}

/// All of `job`: its [`seed`], vertices and indices.
fn whole(job: &ClippedPrimitive, versions: &Versions) -> u64 {
    let seed = seed(job, versions);

    match &job.primitive {
        Primitive::Mesh(mesh) => folded(
            folded(seed, bytemuck::cast_slice(&mesh.vertices)),
            bytemuck::cast_slice(&mesh.indices),
        ),
        Primitive::Callback(callback) => [callback.rect.min, callback.rect.max]
            .map(|p| [p.x.to_bits(), p.y.to_bits()])
            .into_iter()
            .flatten()
            .fold(seed, |h, w| fold(h, u64::from(w))),
    }
}

/// The tiles of `grid` `job` reaches, each with the hash of its
/// triangles over it in order, `scale` pixels a point: each triangle's
/// bounds within the clip rect, as `compose` marks them.
fn reached(
    job: &ClippedPrimitive,
    versions: &Versions,
    scale: f32,
    grid: &Tiles,
) -> HashMap<usize, u64> {
    let seed = seed(job, versions);
    let drawn: Vec<(egui::Rect, u64)> = match &job.primitive {
        Primitive::Mesh(mesh) => {
            let points = points(mesh);

            mesh.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|corners| {
                    let pos = |at: usize| mesh.vertices[corners[at] as usize].pos;
                    let hash = corners
                        .iter()
                        .fold(seed, |h, c| fold(h, points[*c as usize]));

                    (egui::Rect::from_points(&[pos(0), pos(1), pos(2)]), hash)
                })
                .collect()
        }
        Primitive::Callback(callback) => vec![(callback.rect, whole(job, versions))],
    };
    let mut out: HashMap<usize, u64> = HashMap::new();

    for (r, hash) in drawn {
        let r = r.intersect(job.clip_rect);

        if r.is_positive() {
            let pixels = Rect::new(
                r.min.x * scale,
                r.min.y * scale,
                r.max.x * scale,
                r.max.y * scale,
            );

            for t in grid.touched(pixels) {
                let tile = out.entry(t).or_default();

                *tile = fold(*tile, hash);
            }
        }
    }

    out
}

/// Each vertex of `mesh`'s hash: place, uv, colour.
fn points(mesh: &egui::Mesh) -> Vec<u64> {
    mesh.vertices
        .iter()
        .map(|v| {
            [v.pos.x, v.pos.y, v.uv.x, v.uv.y]
                .map(f32::to_bits)
                .into_iter()
                .chain([u32::from_le_bytes(v.color.to_array())])
                .fold(0, |h, w| fold(h, u64::from(w)))
        })
        .collect()
}

/// What every triangle of `job` shares: its texture and that texture's
/// version, and its clip rect.
fn seed(job: &ClippedPrimitive, versions: &Versions) -> u64 {
    let texture = match &job.primitive {
        Primitive::Mesh(mesh) => Some(mesh.texture_id),
        Primitive::Callback(_) => None,
    };
    let (kind, id) = match texture {
        Some(TextureId::Managed(id)) => (1, id),
        Some(TextureId::User(id)) => (2, id),
        None => (3, 0),
    };
    let version = texture.and_then(|t| versions.ids.get(&t)).copied();
    let clip = [job.clip_rect.min, job.clip_rect.max].map(|p| [p.x.to_bits(), p.y.to_bits()]);

    [kind, id, version.unwrap_or(0)]
        .into_iter()
        .chain(clip.into_iter().flatten().map(u64::from))
        .fold(0, fold)
}

/// `jobs` cut to `rects` (pixels, at `scale` a point): for each rect, each
/// job met with it, keeping its triangles that can reach it, in order.
pub(crate) fn cut(jobs: &[ClippedPrimitive], rects: &[URect], scale: f32) -> Vec<ClippedPrimitive> {
    rects
        .iter()
        .flat_map(|rect| {
            let pixels = Rect::from_corners(rect.min.as_vec2(), rect.max.as_vec2());
            let points = egui::Rect::from_min_max(
                egui::pos2(pixels.min.x / scale, pixels.min.y / scale),
                egui::pos2(pixels.max.x / scale, pixels.max.y / scale),
            );

            jobs.iter().filter_map(move |job| {
                let clip_rect = job.clip_rect.intersect(points);
                let Primitive::Mesh(mesh) = &job.primitive else {
                    return None;
                };

                if clip_rect.is_positive() {
                    within(mesh, job.clip_rect, pixels, scale).map(|mesh| ClippedPrimitive {
                        clip_rect,
                        primitive: Primitive::Mesh(mesh),
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

/// `mesh`'s triangles that can reach `pixels` within `clip` (points),
/// their vertices alone, in order; `None` for none.
fn within(mesh: &egui::Mesh, clip: egui::Rect, pixels: Rect, scale: f32) -> Option<egui::Mesh> {
    let reaches = |corners: &[u32; 3]| {
        let pos = |at: usize| mesh.vertices[corners[at] as usize].pos;
        let r = egui::Rect::from_points(&[pos(0), pos(1), pos(2)]).intersect(clip);
        let (min, max) = (
            Vec2::new(r.min.x, r.min.y) * scale - REACH,
            Vec2::new(r.max.x, r.max.y) * scale + REACH,
        );

        r.is_positive() && min.cmplt(pixels.max).all() && max.cmpgt(pixels.min).all()
    };
    let mut renumbered = vec![u32::MAX; mesh.vertices.len()];
    let mut out = egui::Mesh::with_texture(mesh.texture_id);

    for corners in mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|c| reaches(c))
    {
        for &corner in corners {
            let at = &mut renumbered[corner as usize];

            if *at == u32::MAX {
                #[allow(clippy::cast_possible_truncation, reason = "egui's own u32 indices")]
                {
                    *at = out.vertices.len() as u32;
                }
                out.vertices.push(mesh.vertices[corner as usize]);
            }

            out.indices.push(*at);
        }
    }

    (!out.indices.is_empty()).then_some(out)
}

/// egui's own textures set this frame (the font atlas grown) moved on,
/// and any freed (its id may come back as another): seen before egui's
/// update drains them.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn delta(outputs: Query<&EguiRenderOutput, With<Compose>>, mut versions: ResMut<Versions>) {
    for output in &outputs {
        for id in output.textures_delta.set.keys() {
            *versions.ids.entry(*id).or_default() += 1;
        }

        versions.freed |= !output.textures_delta.free.is_empty();
    }
}

/// The versions moved: each user texture whose image changed, and each
/// active camera's target image; everything after an egui texture is
/// freed, or as user textures are added or moved.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn textures(
    mut versions: ResMut<Versions>,
    mut images: MessageReader<AssetEvent<Image>>,
    user: Option<Res<EguiUserTextures>>,
    cameras: Query<(&Camera, &RenderTarget)>,
) {
    let changed = images.read().filter_map(|event| match event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::LoadedWithDependencies { id } => Some(*id),
        _ => None,
    });
    let drawn = cameras
        .iter()
        .filter(|(camera, _)| camera.is_active)
        .filter_map(|(_, target)| match target {
            RenderTarget::Image(image) => Some(image.handle.id()),
            _ => None,
        });
    let ids: Vec<TextureId> = user
        .as_ref()
        .map(|user| {
            changed
                .chain(drawn)
                .filter_map(|id| user.image_id(id))
                .collect()
        })
        .unwrap_or_default();

    for id in ids {
        *versions.ids.entry(id).or_default() += 1;
    }

    versions.everything = versions.freed || user.is_some_and(|u| u.is_changed());
    versions.freed = false;
}

/// A texel cleared to nothing, the source of every rect's clear.
#[derive(Resource)]
struct Blank(TextureView);

#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn blank(mut commands: Commands, device: Res<RenderDevice>) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("kept blank"),
        size: Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    commands.insert_resource(Blank(
        texture.create_view(&TextureViewDescriptor::default()),
    ));
}

/// egui's texture wiped as [`Wipe`] says, before egui draws: all of it
/// cleared, or each rect by a blit of [`Blank`] with no blend. Kept
/// ([`Steady`]) only while its texture is last frame's and the blit is
/// ready; else cleared whole, and the main world draws all again.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn wipe(
    view: ViewQuery<(&ViewTarget, &Wipe), With<Compose>>,
    (blit, mut blits): (
        Res<BlitPipeline>,
        ResMut<SpecializedRenderPipelines<BlitPipeline>>,
    ),
    (cache, blank, steady): (Res<PipelineCache>, Res<Blank>, Res<Steady>),
    mut last: Local<Option<bevy::render::render_resource::TextureId>>,
    mut ctx: RenderContext,
) {
    let (target, wipe) = view.into_inner();
    let texture = target.main_texture().id();
    let same = *last == Some(texture);

    *last = Some(texture);

    let key = BlitPipelineKey {
        target_format: target.main_texture_format(),
        blend_state: None,
        samples: 1,
        source_space: ResolvedCompositingSpace::space(None),
    };
    let id = blits.specialize(&cache, &blit, key);
    let pipeline = cache.get_render_pipeline(id);
    let device = ctx.render_device().clone();

    steady.set(pipeline.is_some());

    let (load, rects) = match (wipe, pipeline) {
        (Wipe::Rects(rects), Some(pipeline)) if same => (LoadOp::Load, Some((pipeline, rects))),
        (Wipe::Rects(_), _) => {
            steady.set(false);
            (LoadOp::Clear(default()), None)
        }
        (Wipe::All, _) => (LoadOp::Clear(default()), None),
    };

    if rects.is_some_and(|(_, rects)| rects.is_empty()) {
        return;
    }

    let mut pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("kept wipe"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target.main_texture_view(),
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

    if let Some((pipeline, rects)) = rects {
        let bind_group = blit.create_bind_group(&device, &blank.0, &cache);

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);

        for rect in rects {
            crate::shadow::scissored(&mut pass, *rect);
            pass.draw(0..3, 0..1);
        }
    }
}

/// egui's texture wiped, in its camera's schedule: for others to order by.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Wiping;

/// The versions, the wipe and its pass. Ordered against `compose`'s
/// marks, which cut egui's paint.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        let steady = Steady::default();

        app.add_plugins(ExtractComponentPlugin::<Wipe>::default())
            .init_resource::<Versions>()
            .insert_resource(steady.clone())
            .add_systems(
                PostUpdate,
                delta
                    .after(EguiPostUpdateSet::ProcessOutput)
                    .before(EguiPostUpdateSet::PostProcessOutput),
            )
            .add_systems(Last, textures.before(crate::compose::Marking));

        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(steady)
                .add_systems(RenderStartup, blank)
                .add_systems(
                    Core2d,
                    wipe.in_set(Wiping)
                        .after(Core2dSystems::MainPass)
                        .before(egui_pass),
                );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rect of `colour` at `at`, points, one job.
    fn rect(at: egui::Rect, colour: egui::Color32) -> ClippedPrimitive {
        let mut mesh = egui::Mesh::default();

        mesh.add_colored_rect(at, colour);

        ClippedPrimitive {
            clip_rect: egui::Rect::EVERYTHING,
            primitive: Primitive::Mesh(mesh),
        }
    }

    /// A rect recoloured is drawn again in the tiles about it alone; the
    /// paint cut to them keeps its triangles and none far off.
    #[test]
    fn a_change_draws_its_tiles_again() {
        let size = UVec2::new(160, 64);
        let still = rect(
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(64.0, 16.0)),
            egui::Color32::RED,
        );
        let at = egui::Rect::from_min_size(egui::pos2(100.0, 40.0), egui::vec2(10.0, 10.0));
        let before = [still.clone(), rect(at, egui::Color32::RED)];
        let after = [still, rect(at, egui::Color32::BLUE)];
        let mut held = Held::default();
        let versions = Versions::default();

        assert_eq!(
            held.frame(&before, (size, 1.0), (&versions, 1, true)).1,
            Wipe::All
        );

        let Wipe::Rects(rects) = held.frame(&after, (size, 1.0), (&versions, 2, true)).1 else {
            panic!("kept");
        };

        // 98 .. 112 across, 38 .. 52 down: tiles 6 across, 2 and 3 down.
        assert_eq!(rects, vec![URect::new(96, 32, 112, 64)]);

        let cut = cut(&after, &rects, 1.0);

        assert_eq!(cut.len(), 1);
        assert_eq!(
            cut[0].clip_rect,
            egui::Rect::from_min_max(egui::pos2(96.0, 32.0), egui::pos2(112.0, 64.0))
        );
    }

    /// A smaller window forgets the larger one's paint: its tiles are
    /// none of the new grid's.
    #[test]
    fn a_smaller_size_forgets_the_larger_ones_tiles() {
        let far = egui::Rect::from_min_size(egui::pos2(140.0, 140.0), egui::vec2(10.0, 10.0));
        let near = egui::Rect::from_min_size(egui::pos2(4.0, 4.0), egui::vec2(8.0, 8.0));
        let (large, small) = (UVec2::new(160, 160), UVec2::new(32, 32));
        let mut held = Held::default();
        let versions = Versions::default();

        held.frame(
            &[rect(far, egui::Color32::RED)],
            (large, 1.0),
            (&versions, 1, true),
        );
        held.frame(
            &[rect(near, egui::Color32::RED)],
            (small, 1.0),
            (&versions, 2, true),
        );

        let (_, wipe) = held.frame(
            &[rect(near, egui::Color32::BLUE)],
            (small, 1.0),
            (&versions, 3, true),
        );

        assert_eq!(wipe, Wipe::Rects(vec![URect::new(0, 0, 16, 16)]));
        assert_eq!(held.jobs.len(), 1);
    }

    /// The same paint draws nothing again; a new size, scale or a frame
    /// missed draws all.
    #[test]
    fn the_same_paint_draws_nothing_and_a_new_size_all() {
        let size = UVec2::new(64, 64);
        let jobs = [rect(
            egui::Rect::from_min_size(egui::pos2(4.0, 4.0), egui::vec2(20.0, 20.0)),
            egui::Color32::RED,
        )];
        let mut held = Held::default();
        let versions = Versions::default();
        let mut wipe = |size: UVec2, scale: f32, frame: u32, steady: bool| {
            held.frame(&jobs, (size, scale), (&versions, frame, steady))
                .1
        };

        wipe(size, 1.0, 1, true);

        assert_eq!(wipe(size, 1.0, 2, true), Wipe::Rects(vec![]));
        assert_eq!(wipe(size, 1.25, 3, true), Wipe::All);
        assert_eq!(wipe(size, 1.25, 5, true), Wipe::All);
        assert_eq!(wipe(UVec2::new(80, 64), 1.25, 6, true), Wipe::All);
        assert_eq!(wipe(UVec2::new(80, 64), 1.25, 7, false), Wipe::All);
        assert_eq!(wipe(UVec2::new(80, 64), 1.25, 8, true), Wipe::Rects(vec![]));
    }

    /// A clip rect met with a dirty rect rounds, at any scale, to the
    /// meeting of the two rounded: egui's scissor.
    #[test]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "test pixels"
    )]
    fn a_cut_clip_scissors_to_the_same_pixels() {
        let round = |v: f32, s: f32| (v * s).round() as u32;

        for scale in [1.0_f32, 1.25, 1.5, 1.8, 2.0, 0.8] {
            for clip in [3.3_f32, 17.9, 40.01, 63.5] {
                for tile in [0_u32, 16, 32, 48, 64, 80] {
                    let met = clip.max(tile as f32 / scale);

                    assert_eq!(
                        round(met, scale),
                        round(clip, scale).max(tile),
                        "{scale} {clip} {tile}"
                    );
                }
            }
        }
    }
}
