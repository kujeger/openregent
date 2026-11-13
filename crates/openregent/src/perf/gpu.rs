//! The GPU's side of a drawn run, with [`ENV`] set (and `MAJ_PERF`):
//! series in the drawn reports, and GPU memory lines, every
//! [`WINDOW`] drawn frames.
//!
//! ```text
//!  gpu_frame          ms, the frame's command buffers on the GPU: two
//!                     timestamps around the one submit (Stamps)
//!  gpu_<pass>         ms per pass, Bevy's RenderDiagnosticsPlugin (timestamp
//!  cpu_<pass>         queries); cpu_ the pass's command encoding
//!  frag_<pass>        fragment shader invocations, where the driver counts
//!  gpu_copy_<n>       ms, camera order n's final copy and egui pass
//!  gpu_egui_<n>       (time spans: no frag_)
//!  render             ms, the render world's schedule (its own thread)
//!  draws_*, items     draw calls and phase items, per frame (count)
//!  pipelines, binds   pipeline and material bind group changes (count)
//!  memory gpu ...     wgpu's allocator: bytes allocated and reserved;
//!                     Bevy's images; the largest allocations by label
//!  memory art ...     the art service's textures: upscaled ("sized"), plain
//! ```
//!
//! Not timed: the art service's GPU shrinks, submitted in `Queue`
//! before the graph, and the screenshot copy after it. `binds` counts
//! material bind group changes, sprites' (a Bevy material) among them.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use bevy::core_pipeline::core_3d::{AlphaMask3d, Opaque3d, Transparent3d};
use bevy::core_pipeline::upscaling::upscaling;
use bevy::core_pipeline::{Core2d, Core2dSystems};
use bevy::diagnostic::{DiagnosticsPlugin, DiagnosticsStore};
use bevy::log::{info, warn};
use bevy::pbr::{PreparedMaterial, RenderMaterialInstances};
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::{
    BinnedPhaseItem, BinnedRenderPhase, ViewBinnedRenderPhases, ViewSortedRenderPhases,
};
use bevy::render::render_resource::{BindGroupId, CachedRenderPipelineId};
use bevy::render::renderer::{
    PendingCommandBuffers, RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue,
};
use bevy::render::renderer::{RenderContext, ViewQuery};
use bevy_egui::render::{EguiCameraView, egui_pass, prepare_egui_pass};
// Renamed: regent_draw's art_service test bans the module's path followed
// by `::` in openregent, where it would mean regent_draw's texture module.
use bevy::platform::collections::{HashMap, HashSet};
use bevy::render::erased_render_asset::ErasedRenderAssets;
use bevy::render::material_bind_groups::MaterialBindGroupAllocators;
use bevy::render::sync_world::MainEntity;
use bevy::render::texture as gpu_images;
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bevy_egui::egui::epaint::Primitive;
use bevy_egui::egui::{self, TextureId};
use bevy_egui::{EguiContext, EguiRenderOutput};
use gpu_images::GpuImage;
use wgpu::{
    Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Features, MapMode, QuerySet,
    QuerySetDescriptor, QueryType,
};

use super::{SPIKE, TARGET, WINDOW, named};

/// The environment variable that turns these on, e.g. `MAJ_PERF_GPU=1`.
pub const ENV: &str = "MAJ_PERF_GPU";

/// Frames whose timestamps may be on their way back at once.
const SLOTS: usize = 4;

/// Two timestamps, 8 bytes each.
const STAMPS_BYTES: u64 = 16;

/// Allocations named in a memory line: the largest labels.
const LABELS: usize = 8;

const MB: f64 = 1024.0 * 1024.0;

/// A pass's GPU time past which the timestamps are taken as broken, ms.
const IMPLAUSIBLE_MS: f64 = 1000.0;

/// With [`ENV`] set and a renderer, the hooks put in `app`.
pub fn install(app: &mut App) {
    if std::env::var_os(ENV).is_some() && app.get_sub_app(RenderApp).is_some() {
        if !app.is_plugin_added::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
        }

        app.add_plugins(RenderDiagnosticsPlugin)
            .add_systems(Last, (passes, egui, art).before(super::ended));

        app.sub_app_mut(RenderApp)
            .add_systems(RenderStartup, stamps)
            .add_systems(
                Render,
                (
                    began.in_set(RenderSystems::ExtractCommands),
                    count.in_set(RenderSystems::PrepareBindGroups),
                    // Strictly after: Bevy's sets order only conflicting systems.
                    (ended, memory)
                        .in_set(RenderSystems::PostCleanup)
                        .after(RenderSystems::Cleanup),
                ),
            )
            // Each 2D camera's egui pass and final copy, timed apart.
            .add_systems(
                Core2d,
                (
                    egui_opened
                        .after(Core2dSystems::MainPass)
                        .after(prepare_egui_pass)
                        .before(egui_pass),
                    egui_closed.after(egui_pass).before(copy_opened),
                    copy_opened
                        .after(Core2dSystems::PostProcess)
                        .before(upscaling),
                    copy_closed.after(upscaling),
                ),
            )
            .add_systems(
                RenderGraph,
                (
                    harvest.in_set(RenderGraphSystems::Begin),
                    bracket
                        .after(RenderGraphSystems::Render)
                        .before(RenderGraphSystems::Submit),
                ),
            );
    }
}

/// Each new render diagnostic noted, summed per frame: a pass drawn by
/// two cameras counts once, both cameras' time.
///
/// ```text
///  render/main_transparent_pass_2d/elapsed_gpu  ->  gpu_main_transparent_pass_2d
/// ```
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn passes(store: Option<Res<DiagnosticsStore>>, mut seen: Local<Option<Instant>>) {
    let Some(store) = store else {
        return;
    };
    let mut newest = *seen;

    for diagnostic in store.iter() {
        let Some(name) = series(diagnostic.path().as_str()) else {
            continue;
        };

        // The measurements since the last frame's, one sum per frame.
        let mut frames: BTreeMap<Instant, f64> = BTreeMap::new();
        for m in diagnostic.measurements() {
            if seen.is_none_or(|t| m.time > t) {
                *frames.entry(m.time).or_default() += m.value;
                newest = newest.max(Some(m.time));
            }
        }

        // Timestamps inside a pass read 0 or nonsense on Asahi's tiler:
        // those left out, so the series is absent, not wrong.
        let plausible = |ms: f64| !name.starts_with("gpu_") || (ms > 0.0 && ms < IMPLAUSIBLE_MS);
        for value in frames.values().filter(|v| plausible(**v)) {
            named(&name, *value);
        }
    }

    *seen = newest;
}

/// A render diagnostic's series name, `None` for one not kept.
fn series(path: &str) -> Option<String> {
    let (pass, field) = path.strip_prefix("render/")?.rsplit_once('/')?;
    let kind = match field {
        "elapsed_gpu" => Some("gpu"),
        "elapsed_cpu" => Some("cpu"),
        "fragment_shader_invocations" => Some("frag"),
        _ => None,
    }?;
    let pass: String = pass
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();

    Some(format!("{kind}_{pass}"))
}

/// egui's draw calls this frame, one a mesh, and its texture changes;
/// the distinct textures drawn, and the calls split by a clip rect alone
/// (`clips_egui`: same texture as the call before). With [`DUMP`] set to
/// a frame, that frame's calls are logged one a line.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn egui(
    outputs: Query<&EguiRenderOutput>,
    mut contexts: Query<&mut EguiContext>,
    art: Option<Res<regent_draw::art::Art>>,
    mut frames: Local<u32>,
) {
    let mut draws = 0_u32;
    let mut binds = 0_u32;
    let mut clips = 0_u32;
    let mut textures = HashSet::new();

    *frames += 1;

    for output in &outputs {
        let mut last = None;

        for job in &output.paint_jobs {
            draws += 1;
            if let Primitive::Mesh(mesh) = &job.primitive {
                textures.insert(mesh.texture_id);

                if last.is_some_and(|(t, _)| t == mesh.texture_id) {
                    clips += 1;
                } else {
                    binds += 1;
                }
                last = Some((mesh.texture_id, job.clip_rect));
            }
        }
    }

    named("draws_egui", f64::from(draws));
    named("binds_egui", f64::from(binds));
    named("clips_egui", f64::from(clips));
    #[allow(clippy::cast_precision_loss, reason = "a count")]
    named("textures_egui", textures.len() as f64);

    if dump_at() == Some(*frames) {
        dumped(&outputs, &mut contexts, art.as_deref());
    }
}

/// The environment variable that logs one frame's egui draw calls, e.g.
/// `MAJ_PERF_EGUI_DUMP=1700` (with [`ENV`]).
const DUMP: &str = "MAJ_PERF_EGUI_DUMP";

/// The frame [`DUMP`] names, read once.
fn dump_at() -> Option<u32> {
    static AT: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();

    *AT.get_or_init(|| std::env::var(DUMP).ok()?.parse().ok())
}

/// Each egui draw call, one a line: texture, its name (an egui texture's,
/// or the art service's picture), clip rect, vertices' bounds and count.
///
/// ```text
///  egui 12 tex User(40) Pic(Set { record: .. }) clip [0 0 1920 1200] at [1700 0 1920 400] verts 4
/// ```
fn dumped(
    outputs: &Query<&EguiRenderOutput>,
    contexts: &mut Query<&mut EguiContext>,
    art: Option<&regent_draw::art::Art>,
) {
    let mut names: HashMap<TextureId, String> = art
        .map(regent_draw::art::Art::names)
        .unwrap_or_default()
        .into_iter()
        .collect();

    for mut context in contexts.iter_mut() {
        let manager = context.get_mut().tex_manager();
        for (id, meta) in manager.read().allocated() {
            names.insert(*id, meta.name.clone());
        }
    }

    let jobs = outputs.iter().flat_map(|o| o.paint_jobs.iter());
    for (at, job) in jobs.enumerate() {
        let r = |r: egui::Rect| {
            format!(
                "[{:.0} {:.0} {:.0} {:.0}]",
                r.min.x, r.min.y, r.max.x, r.max.y
            )
        };

        if let Primitive::Mesh(mesh) = &job.primitive {
            let name = names.get(&mesh.texture_id).map_or("?", String::as_str);

            info!(
                target: TARGET,
                "egui {at} tex {:?} {name} clip {} at {} verts {}",
                mesh.texture_id,
                r(job.clip_rect),
                r(mesh.calc_bounds()),
                mesh.vertices.len()
            );
        } else {
            info!(target: TARGET, "egui {at} callback clip {}", r(job.clip_rect));
        }
    }
}

/// Every [`WINDOW`] frames, the art service's textures: upscaled
/// pictures ("sized"), plain ones, the quest map's levels.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn art(art: Option<Res<regent_draw::art::Art>>, mut frames: Local<u32>) {
    *frames += 1;

    if frames.is_multiple_of(WINDOW)
        && let Some(art) = art
    {
        let stats = art.stats();
        #[allow(clippy::cast_precision_loss, reason = "bytes shown in MB")]
        let mb = |[n, bytes]: [usize; 2]| format!("{n} {:.1}", bytes as f64 / MB);

        info!(
            target: TARGET,
            "memory art sized {} plain {} map {} (count MB)",
            mb(stats.sized),
            mb(stats.plain),
            mb(stats.map)
        );
    }
}

/// The span bracketing a 2D camera's final copy (Bevy's `upscaling`):
/// `gpu_copy_<order>`, the camera's order.
const COPY: &str = "copy";

/// The span bracketing a camera's egui pass: `gpu_egui_<order>`.
const EGUI: &str = "egui";

/// A time span `<prefix>_<order>` opened on the view's encoder. Neither
/// Bevy's copy nor egui's pass holds a pass span, so they have no `frag_`
/// series: a copy shades its scissor rect, the window or the camera's
/// viewport.
fn open(prefix: &str, camera: &ExtractedCamera, ctx: &mut RenderContext) {
    let diagnostics = ctx.diagnostic_recorder();
    let name = format!("{prefix}_{}", camera.order);

    diagnostics
        .as_deref()
        .begin_time_span(ctx.command_encoder(), name.into());
}

/// The span [`open`] opened, closed.
fn close(ctx: &mut RenderContext) {
    let diagnostics = ctx.diagnostic_recorder();

    diagnostics.as_deref().end_time_span(ctx.command_encoder());
}

fn copy_opened(view: ViewQuery<&ExtractedCamera>, mut ctx: RenderContext) {
    open(COPY, view.into_inner(), &mut ctx);
}

fn copy_closed(_view: ViewQuery<&ExtractedCamera>, mut ctx: RenderContext) {
    close(&mut ctx);
}

fn egui_opened(view: ViewQuery<&ExtractedCamera, With<EguiCameraView>>, mut ctx: RenderContext) {
    open(EGUI, view.into_inner(), &mut ctx);
}

fn egui_closed(_view: ViewQuery<(), With<EguiCameraView>>, mut ctx: RenderContext) {
    close(&mut ctx);
}

/// When the render world's schedule began.
#[derive(Resource)]
struct Began(Instant);

fn began(mut commands: Commands) {
    commands.insert_resource(Began(Instant::now()));
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn ended(began: Option<Res<Began>>, mut frames: Local<u32>) {
    *frames += 1;

    if let Some(began) = began {
        let ms = began.0.elapsed().as_secs_f64() * 1e3;

        named("render", ms);

        // A spike's render frame named, as the main world's.
        if ms > SPIKE {
            info!(target: TARGET, "spike render frame {} ms {ms:.1}", *frames);
        }
    }
}

/// Draw calls and state changes counted in one phase.
#[derive(Clone, Copy, Default)]
struct Tally {
    items: u32,
    draws: u32,
    pipelines: u32,
    binds: u32,
    last: Option<(CachedRenderPipelineId, Option<BindGroupId>)>,
}

impl Tally {
    /// One draw call with `pipeline` and `bind`, standing for `items`
    /// phase items.
    fn draw(&mut self, items: usize, pipeline: CachedRenderPipelineId, bind: Option<BindGroupId>) {
        self.items += u32::try_from(items).unwrap_or(u32::MAX);
        self.draws += 1;

        // The first call of a phase sets both.
        let (p, b) = self.last.map_or((None, None), |(p, b)| (Some(p), Some(b)));
        self.pipelines += u32::from(p != Some(pipeline));
        self.binds += u32::from(b != Some(bind));
        self.last = Some((pipeline, bind));
    }
}

/// This frame's draw calls in the 3D phases (the world's), every view, after
/// batching: as the phases' `render` walks them, one call a bin (a
/// batch set, unbatchable entity or non-mesh item) or a sorted item's
/// batch.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn count(
    opaque: Option<Res<ViewBinnedRenderPhases<Opaque3d>>>,
    masked: Option<Res<ViewBinnedRenderPhases<AlphaMask3d>>>,
    sorted: Option<Res<ViewSortedRenderPhases<Transparent3d>>>,
    instances: Option<Res<RenderMaterialInstances>>,
    materials: Option<Res<ErasedRenderAssets<PreparedMaterial>>>,
    allocators: Option<Res<MaterialBindGroupAllocators>>,
) {
    // An entity's material bind group: through its material asset to
    // the slab the allocator put it in.
    let bind = |entity: &MainEntity| {
        let instance = instances.as_ref()?.instances.get(entity)?.asset_id;
        let material = materials.as_ref()?.get(instance)?;
        let allocator = allocators.as_ref()?.get(&instance.type_id())?;

        Some(allocator.get(material.binding.group)?.bind_group()?.id())
    };

    let opaque = opaque.map_or_else(Tally::default, |phases| {
        phases.values().fold(Tally::default(), |t, phase| {
            binned(t, phase, |k, e| (k.pipeline, bind(e)))
        })
    });
    let masked = masked.map_or_else(Tally::default, |phases| {
        phases.values().fold(Tally::default(), |t, phase| {
            binned(t, phase, |k, e| (k.pipeline, bind(e)))
        })
    });
    let mut transparent = Tally::default();
    for phase in sorted.iter().flat_map(|phases| phases.values()) {
        let items: Vec<&Transparent3d> = phase.items.values().collect();
        let mut at = 0;

        transparent.last = None;
        while at < items.len() {
            let item = items[at];
            let batch = item.batch_range.len();

            if batch == 0 {
                at += 1;
            } else {
                transparent.draw(batch, item.pipeline, bind(&item.entity.1));
                at += batch;
            }
        }
    }

    let all = [opaque, masked, transparent];
    let sum = |f: fn(&Tally) -> u32| f64::from(all.iter().map(f).sum::<u32>());

    named("draws_opaque", f64::from(opaque.draws));
    named("draws_mask", f64::from(masked.draws));
    named("draws_transparent", f64::from(transparent.draws));
    named("draws_world", sum(|t| t.draws));
    named("items", sum(|t| t.items));
    named("pipelines", sum(|t| t.pipelines));
    named("binds", sum(|t| t.binds));
}

/// `phase`'s draw calls added to `tally`, in its render order:
/// multidrawn batch sets, batchable bins, unbatchable entities, non-mesh
/// items. `key` reads the pipeline and material bind group from a batch
/// set key and one of its entities.
fn binned<B: BinnedPhaseItem>(
    mut tally: Tally,
    phase: &BinnedRenderPhase<B>,
    key: impl Fn(&B::BatchSetKey, &MainEntity) -> (CachedRenderPipelineId, Option<BindGroupId>),
) -> Tally {
    tally.last = None;

    // GPU-culled meshes: counted as one call a batch set, no state.
    tally.draws += u32::try_from(phase.multidrawable_meshes.len()).unwrap_or(u32::MAX);

    // A bin's entities share its key, so its first stands for them all.
    for ((bin_key, _), bin) in &phase.batchable_meshes {
        if let Some(entity) = bin.entities().keys().next() {
            let (pipeline, bind) = key(bin_key, entity);
            tally.draw(bin.entities().len(), pipeline, bind);
        }
    }
    for ((bin_key, _), unbatched) in &phase.unbatchable_meshes {
        for entity in unbatched.entities.keys() {
            let (pipeline, bind) = key(bin_key, entity);
            tally.draw(1, pipeline, bind);
        }
    }
    for ((bin_key, _), items) in &phase.non_mesh_items {
        for entity in items.entities.keys() {
            let (pipeline, bind) = key(bin_key, entity);
            tally.draw(1, pipeline, bind);
        }
    }

    tally
}

/// Every [`WINDOW`] render frames, the GPU memory in use: wgpu's
/// allocator (every buffer and texture, render targets too) and Bevy's
/// images; the largest allocations by label.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn memory(device: Res<RenderDevice>, images: Res<RenderAssets<GpuImage>>, mut frames: Local<u32>) {
    *frames += 1;

    if frames.is_multiple_of(WINDOW) {
        said(&device, &images);
    }
}

/// The memory lines.
#[allow(clippy::cast_precision_loss, reason = "bytes shown in MB")]
fn said(device: &RenderDevice, images: &RenderAssets<GpuImage>) {
    let image_bytes: u64 = images.iter().map(|(_, image)| bytes(image)).sum();
    let count = images.iter().count();

    if let Some(report) = device.wgpu_device().generate_allocator_report() {
        let mut labels: BTreeMap<&str, u64> = BTreeMap::new();
        for a in &report.allocations {
            *labels.entry(a.name.as_str()).or_default() += a.size;
        }
        let mut largest: Vec<(&str, u64)> = labels.into_iter().collect();
        largest.sort_by_key(|&(_, size)| std::cmp::Reverse(size));
        let largest: Vec<String> = largest
            .iter()
            .take(LABELS)
            .map(|(name, size)| format!("{name}={:.1}", *size as f64 / MB))
            .collect();

        info!(
            target: TARGET,
            "memory gpu allocated {:.1} reserved {:.1} blocks {} allocations {} images {count} {:.1} (MB)",
            report.total_allocated_bytes as f64 / MB,
            report.total_reserved_bytes as f64 / MB,
            report.blocks.len(),
            report.allocations.len(),
            image_bytes as f64 / MB,
        );
        info!(target: TARGET, "memory labels {}", largest.join(", "));
    } else {
        info!(
            target: TARGET,
            "memory gpu images {count} {:.1} (MB), no allocator report",
            image_bytes as f64 / MB
        );
    }
}

/// An image's bytes on the GPU, every mip level and layer.
fn bytes(image: &GpuImage) -> u64 {
    let d = &image.texture_descriptor;
    let block = u64::from(d.format.block_copy_size(None).unwrap_or(4));
    let (bw, bh) = d.format.block_dimensions();

    (0..d.mip_level_count)
        .map(|level| {
            let size = d.size.mip_level_size(level, d.dimension);
            u64::from(size.width.div_ceil(bw))
                * u64::from(size.height.div_ceil(bh))
                * u64::from(size.depth_or_array_layers)
                * block
        })
        .sum()
}

/// A slot's state: free, its frame on the GPU, or its stamps readable.
const FREE: u8 = 0;
const FLIGHT: u8 = 1;
const MAPPED: u8 = 2;

/// The frame's two timestamps: a query set, resolved and copied to one
/// of [`SLOTS`] buffers a frame, read back frames later.
#[derive(Resource)]
struct Stamps {
    set: QuerySet,
    resolve: Buffer,
    slots: Vec<(Buffer, Arc<AtomicU8>)>,
    next: usize,
    /// ns a tick.
    period: f32,
}

/// The query set and buffers, where the device writes timestamps in
/// command encoders.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn stamps(mut commands: Commands, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    let wanted = Features::TIMESTAMP_QUERY | Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;

    if device.features().contains(wanted) {
        let wgpu = device.wgpu_device();
        let buffer = |label, usage| {
            wgpu.create_buffer(&BufferDescriptor {
                label: Some(label),
                size: STAMPS_BYTES,
                usage,
                mapped_at_creation: false,
            })
        };

        commands.insert_resource(Stamps {
            set: wgpu.create_query_set(&QuerySetDescriptor {
                label: Some("perf frame stamps"),
                ty: QueryType::Timestamp,
                count: 2,
            }),
            resolve: buffer(
                "perf frame stamps resolved",
                BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC,
            ),
            slots: (0..SLOTS)
                .map(|_| {
                    let read = buffer(
                        "perf frame stamps read",
                        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                    );
                    (read, Arc::new(AtomicU8::new(FREE)))
                })
                .collect(),
            next: 0,
            period: queue.get_timestamp_period(),
        });
    } else {
        warn!(target: TARGET, "no GPU timestamps in command encoders: gpu_frame not timed");
    }
}

/// The frame's command buffers put between two timestamps, in the one
/// submit that follows; skipped while the next slot is still out.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn bracket(
    stamps: Option<ResMut<Stamps>>,
    device: Res<RenderDevice>,
    mut pending: ResMut<PendingCommandBuffers>,
) {
    let Some(mut stamps) = stamps else {
        return;
    };
    let (read, state) = stamps.slots[stamps.next].clone();
    if pending.is_empty() || state.load(Ordering::Acquire) != FREE {
        return;
    }

    let encoder = || device.create_command_encoder(&CommandEncoderDescriptor::default());
    let mut first = encoder();
    let mut last = encoder();

    first.write_timestamp(&stamps.set, 0);
    last.write_timestamp(&stamps.set, 1);
    last.resolve_query_set(&stamps.set, 0..2, &stamps.resolve, 0);
    last.copy_buffer_to_buffer(&stamps.resolve, 0, &read, 0, STAMPS_BYTES);

    let mapped = state.clone();
    last.map_buffer_on_submit(&read, MapMode::Read, .., move |result| {
        mapped.store(
            if result.is_ok() { MAPPED } else { FREE },
            Ordering::Release,
        );
    });
    state.store(FLIGHT, Ordering::Release);

    let work: Vec<_> = pending.finish().collect();
    pending.push(
        std::iter::once(first.finish())
            .chain(work)
            .chain(std::iter::once(last.finish())),
    );

    stamps.next = (stamps.next + 1) % SLOTS;
}

/// Each slot read back since: its frame's GPU time noted, the slot
/// freed.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn harvest(stamps: Option<Res<Stamps>>) {
    for (read, state) in stamps.iter().flat_map(|s| s.slots.iter()) {
        if state.load(Ordering::Acquire) == MAPPED {
            let ticks = read.slice(..).get_mapped_range().map_or(0, |data| {
                let stamp =
                    |at: usize| u64::from_le_bytes(data[at..at + 8].try_into().unwrap_or([0; 8]));
                stamp(8).saturating_sub(stamp(0))
            });
            read.unmap();
            state.store(FREE, Ordering::Release);

            #[allow(clippy::cast_precision_loss, reason = "ticks of one frame")]
            let ms = ticks as f64 * f64::from(stamps.as_ref().map_or(1.0, |s| s.period)) / 1e6;
            named("gpu_frame", ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_names_are_one_word() {
        assert_eq!(
            series("render/main_transparent_pass_2d/elapsed_gpu").as_deref(),
            Some("gpu_main_transparent_pass_2d")
        );
        assert_eq!(
            series("render/resolve shadows/elapsed_cpu").as_deref(),
            Some("cpu_resolve_shadows")
        );
        assert_eq!(series("render/a/clipper_invocations"), None);
        assert_eq!(series("frame_time"), None);
    }
}
