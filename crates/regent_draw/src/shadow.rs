//! Full-screen pass after the main one: the soft shadows' coverage
//! resolved ([`crate::soft`]), and every pixel put on the 565 grid, as the
//! original's present shows its 16-bit frame.
//!
//! The pass writes the camera's output (the window, or an image) itself,
//! in place of Bevy's final copy (`upscaling`): a full-screen pass fewer.
//!
//! ```text
//!  Bevy's     main pass ──> main texture ──resolve──> main texture ──upscaling──> output
//!  this pass  main pass ──> main texture ──resolve──> output
//!  under      main pass ──> main texture ──resolve──> another camera's
//!                                                     main texture, its marks
//!  composed   main pass ──> main texture ──(the compose pass resolves it)
//! ```
//!
//! The shader holds only the sRGB transfer and the resolve's one line;
//! the numbers are [`crate::soft`]'s, which tests check.

use std::collections::HashMap;

use bevy::asset::{embedded_asset, load_embedded_asset};
use bevy::camera::{Camera, CameraOutputMode, Viewport};
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::system::SystemParam;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::camera::{ExtractedCamera, NormalizedRenderTargetExt};
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{texture_2d, uniform_buffer};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, Buffer, BufferInitDescriptor,
    BufferUsages, CachedRenderPipelineId, ColorTargetState, ColorWrites,
    DynamicBindGroupLayoutEntries, FragmentState, LoadOp, Operations, PipelineCache,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    ShaderStages, SpecializedRenderPipeline, SpecializedRenderPipelines, StoreOp, TextureFormat,
    TextureSampleType, TextureView,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::sync_world::MainEntity;
use bevy::render::texture::{GpuImage, ManualTextureViews, OutputColorAttachment};
use bevy::render::view::{
    ExtractedWindow, ViewTarget, ViewTargetAttachments, prepare_view_attachments,
    prepare_view_targets,
};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::{Shader, ShaderDefVal, load_shader_library};

use crate::compose::{Compose, Marks};
use crate::material::Band;
use crate::soft;

/// How shadows reach the picture (`[enhancements] shadows`, openregent's).
///
/// The original's own arithmetic, shadows stacking through its HSV
/// tables, is the library's (`regent_core::view::raster::stack`); its last
/// drawing here is `local/archive/shadow-marks.md`.
#[derive(
    Resource,
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Deserialize,
    serde::Serialize,
    ExtractResource,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[extract_app(RenderApp)]
#[serde(rename_all = "lowercase")]
pub enum Shadows {
    /// A multiply blend as each shadow draws ([`crate::material`]):
    /// shadows stack in draw order, as the original's.
    Blend,
    /// Coverage in alpha, the most of it kept, one strength at the end
    /// ([`crate::soft`]): shadows do not stack, hue is kept.
    #[default]
    Soft,
}

/// Every band material to the wanted [`Shadows`], on its change and as
/// bands are made.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn follow(
    wanted: Res<Shadows>,
    mut made: MessageReader<AssetEvent<Band>>,
    mut bands: ResMut<Assets<Band>>,
) {
    let added: Vec<AssetId<Band>> = made
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id } => Some(*id),
            _ => None,
        })
        .collect();

    let ids: Vec<AssetId<Band>> = if wanted.is_changed() {
        bands.ids().collect()
    } else {
        added
    };

    for id in ids {
        let stale = bands.get(id).is_some_and(|band| band.shadows != *wanted);

        if stale && let Some(mut band) = bands.get_mut(id) {
            band.shadows = *wanted;
        }
    }
}

/// The camera whose picture holds shadows waiting to be resolved. The
/// resolve writes its output, so Bevy's own copy is skipped
/// ([`CameraOutputMode::Skip`], set as this is added).
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
#[extract_app(RenderApp)]
#[extract_component_filter(With<Camera>)]
#[component(on_add = skip_output)]
pub struct Resolve;

/// Where a [`Resolve`] camera's resolve writes; [`Written::Output`]
/// without one.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, ExtractComponent)]
#[extract_app(RenderApp)]
#[extract_component_filter(With<Camera>)]
pub enum Written {
    /// The camera's output: the window, or an image.
    #[default]
    Output,
    /// The [`Over`] camera's main texture, in its viewport and within its
    /// [`Marks`] (all of the viewport without): its pixel `p` takes this
    /// camera's pixel `p − at` (`present`'s straight route). The compose
    /// pass resolves the rest straight into the window.
    Under(IVec2),
    /// Nothing: the compose pass resolves into the window
    /// ([`crate::compose`]).
    Composed,
}

/// The camera a [`Written::Under`] resolve writes into: it draws over the
/// world, and the compose pass copies it out.
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
#[extract_app(RenderApp)]
#[extract_component_filter(With<Camera>)]
#[require(Marks)]
pub struct Over;

/// The camera's output left to the resolve.
fn skip_output(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut camera) = world.get_mut::<Camera>(context.entity) {
        camera.output_mode = CameraOutputMode::Skip;
    }
}

/// What the resolve pass is built from, once.
#[derive(Resource)]
pub(crate) struct Pass {
    layout: BindGroupLayoutDescriptor,
    fullscreen: FullscreenShader,
    fragment: Handle<Shader>,
    /// The shift each output pixel reads its pixel back by: 0, and the
    /// one [`Written::Under`] camera's, written each frame.
    unshifted: Buffer,
    under: Buffer,
}

impl SpecializedRenderPipeline for Pass {
    /// The output's format and shadows wanted.
    type Key = (TextureFormat, Shadows);

    fn specialize(&self, (format, shadows): Self::Key) -> RenderPipelineDescriptor {
        let shader_defs = defs(shadows);

        RenderPipelineDescriptor {
            label: Some("resolve shadows".into()),
            layout: vec![self.layout.clone()],
            vertex: self.fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.fragment.clone(),
                shader_defs,
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        }
    }
}

/// The resolve's fragment defs for `shadows`.
pub(crate) fn defs(shadows: Shadows) -> Vec<ShaderDefVal> {
    match shadows {
        Shadows::Blend => vec![],
        Shadows::Soft => vec![
            "SOFT".into(),
            ShaderDefVal::UInt("STRENGTH".into(), soft::STRENGTH),
        ],
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn init(
    mut commands: Commands,
    fullscreen: Res<FullscreenShader>,
    assets: Res<AssetServer>,
    device: Res<RenderDevice>,
) {
    // Read the screen exactly, without a sampler, shifted by a whole pixel.
    let entries = DynamicBindGroupLayoutEntries::new_with_indices(
        ShaderStages::FRAGMENT,
        (
            (
                0,
                texture_2d(TextureSampleType::Float { filterable: false }),
            ),
            (1, uniform_buffer::<IVec4>(false)),
        ),
    );
    let shift = |label| {
        device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(&[0i32; 4]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        })
    };

    commands.insert_resource(Pass {
        layout: BindGroupLayoutDescriptor::new("resolve shadows", &entries),
        fullscreen: fullscreen.clone(),
        fragment: load_embedded_asset!(assets.as_ref(), "shaders/shadow.wesl"),
        unshifted: shift("resolve unshifted"),
        under: shift("resolve under"),
    });
}

/// The output of each resolving or composing camera: Bevy makes none for
/// a camera that skips its copy, and drops the views of a camera with
/// none, as for an image only the world camera draws into.
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
fn attach(
    cameras: Query<&ExtractedCamera, Or<(With<Resolve>, With<Compose>)>>,
    windows: Query<(MainEntity, &ExtractedWindow)>,
    (images, manual): (Res<RenderAssets<GpuImage>>, Res<ManualTextureViews>),
    mut attachments: ResMut<ViewTargetAttachments>,
) {
    for target in cameras.iter().filter_map(|c| c.target.as_ref()) {
        let output = target
            .get_texture_view(&windows, &images, &manual)
            .zip(target.get_texture_view_format(&windows, &images, &manual));

        if let Some((view, format)) = output
            && !attachments.contains_key(target)
        {
            attachments.insert(
                target.clone(),
                OutputColorAttachment::new(view.clone(), format),
            );
        }
    }
}

/// How far back each output pixel reads its pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shift {
    /// Its own pixel.
    None,
    /// Pixel `p − at`: the one [`Written::Under`] camera's.
    Under(IVec2),
}

/// What a resolve draw needs: the pass, its pipelines, and the one last
/// drawn for each format ([`Drawn`]).
#[derive(SystemParam)]
pub(crate) struct Resolver<'w, 's> {
    pass: Res<'w, Pass>,
    queue: Res<'w, RenderQueue>,
    pipelines: ResMut<'w, SpecializedRenderPipelines<Pass>>,
    drawn: Local<'s, Drawn>,
    shadows: Res<'w, Shadows>,
}

impl Resolver<'_, '_> {
    /// The pipeline and bind group that resolve `screen` into `format`,
    /// read `shift` back: while a variant compiles, the last drawn; none
    /// at the first frames.
    pub(crate) fn draw<'c>(
        &mut self,
        (device, cache): (&RenderDevice, &'c PipelineCache),
        format: TextureFormat,
        screen: &TextureView,
        shift: Shift,
    ) -> Option<(&'c RenderPipeline, BindGroup)> {
        // One camera at most is under another: its shift has the one buffer.
        let buffer = match shift {
            Shift::None => &self.pass.unshifted,
            Shift::Under(at) => {
                let words = [at.x, at.y, 0, 0];

                self.queue
                    .write_buffer(&self.pass.under, 0, bytemuck::cast_slice(&words));
                &self.pass.under
            }
        };

        let id = self
            .pipelines
            .specialize(cache, &self.pass, (format, *self.shadows));
        let bind_group = device.create_bind_group(
            None,
            &cache.get_bind_group_layout(&self.pass.layout),
            &BindGroupEntries::with_indices(((0, screen), (1, buffer.as_entire_binding()))),
        );
        let ready = |id| cache.get_render_pipeline(id).is_some();

        self.drawn
            .pick(format, id, ready)
            .and_then(|id| cache.get_render_pipeline(id))
            .map(|pipeline| (pipeline, bind_group))
    }
}

/// `rect` within `viewport`, or `rect` alone without one; `None` when
/// they do not meet.
pub(crate) fn within(rect: URect, viewport: Option<&Viewport>) -> Option<URect> {
    let cut = viewport.map_or(rect, |v| {
        rect.intersect(URect::from_corners(
            v.physical_position,
            v.physical_position + v.physical_size,
        ))
    });

    (!cut.is_empty()).then_some(cut)
}

/// Resolve the coverage the bands left, or only present, into the
/// camera's output: cleared to the clear colour first, as Bevy's copy
/// clears it, and only the camera's viewport drawn. Or, [`Written::Under`],
/// into the [`Over`] camera's main texture within its marks, as a copy
/// would put it there. [`Written::Composed`]: nothing here.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
fn resolve(
    view: ViewQuery<(&ViewTarget, &ExtractedCamera, Option<&Written>), With<Resolve>>,
    over: Query<(&ViewTarget, &ExtractedCamera, Option<&Marks>), With<Over>>,
    mut resolver: Resolver,
    cache: Res<PipelineCache>,
    clear: Res<ClearColor>,
    mut ctx: RenderContext,
) {
    let (target, camera, written) = view.into_inner();
    let written = written.copied().unwrap_or_default();

    // Into the output, cleared, in the camera's viewport; or into the over
    // camera's main texture within its marks and viewport, loaded, read
    // `at` back.
    let (format, out, rects, shift) = match (written, over.single()) {
        (Written::Composed, _) => return,
        (Written::Under(at), Ok((over, overs, marks))) => {
            let viewport = overs.viewport.as_ref();
            let rects: Vec<URect> = match marks {
                Some(marks) => marks
                    .0
                    .iter()
                    .filter_map(|r| within(*r, viewport))
                    .collect(),
                None => area(overs).into_iter().collect(),
            };
            let out = RenderPassColorAttachment {
                view: over.main_texture_view(),
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            };

            (
                Some(over.main_texture_format()),
                Some(out),
                rects,
                Shift::Under(at),
            )
        }
        _ => (
            target.out_texture_view_format(),
            target.out_texture_color_attachment(Some(clear.0.into())),
            area(camera).into_iter().collect(),
            Shift::None,
        ),
    };

    // Under no overlay, nothing to write.
    let (Some(format), Some(out)) = (format, out) else {
        return;
    };
    if rects.is_empty() && shift != Shift::None {
        return;
    }

    let device = ctx.render_device().clone();
    let drawn = resolver.draw((&device, &cache), format, target.main_texture_view(), shift);

    // Timed when Bevy's render diagnostics are on (openregent's perf runs).
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let mut render_pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("resolve shadows"),
            color_attachments: &[Some(out)],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

    // None at the first frames: the output only cleared.
    let Some((pipeline, bind_group)) = drawn else {
        return;
    };

    let span = diagnostics.pass_span(&mut render_pass, "resolve_shadows");

    render_pass.set_pipeline(pipeline);
    render_pass.set_bind_group(0, &bind_group, &[]);

    for rect in rects {
        scissored(&mut render_pass, rect);
        render_pass.draw(0..3, 0..1);
    }

    span.end(&mut render_pass);
}

/// The camera's viewport, else its whole target; `None` before its size
/// is known.
pub(crate) fn area(camera: &ExtractedCamera) -> Option<URect> {
    let whole = camera
        .physical_target_size
        .map(|size| URect::from_corners(UVec2::ZERO, size));

    match &camera.viewport {
        Some(v) => Some(URect::from_corners(
            v.physical_position,
            v.physical_position + v.physical_size,
        )),
        None => whole,
    }
}

/// `pass` drawing only within `rect`.
pub(crate) fn scissored(pass: &mut wgpu::RenderPass, rect: URect) {
    let size = rect.size();

    pass.set_scissor_rect(rect.min.x, rect.min.y, size.x, size.y);
}

/// The pipeline each output format last drew with: a variant asked for
/// anew (a setting toggled) compiles for some frames, and the old one
/// draws meanwhile, not the clear colour alone.
#[derive(Default)]
struct Drawn(HashMap<TextureFormat, CachedRenderPipelineId>);

impl Drawn {
    /// The pipeline to draw `format` with: `wanted` once `ready`, else
    /// the last drawn.
    fn pick(
        &mut self,
        format: TextureFormat,
        wanted: CachedRenderPipelineId,
        ready: impl Fn(CachedRenderPipelineId) -> bool,
    ) -> Option<CachedRenderPipelineId> {
        if ready(wanted) {
            self.0.insert(format, wanted);
            Some(wanted)
        } else {
            self.0.get(&format).copied()
        }
    }
}

/// Register the pass.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/shadow.wesl");
        // The sRGB curve every linear-colour shader imports, the game's too.
        load_shader_library!(app, "shaders/srgb.wesl");

        app.add_plugins((
            ExtractComponentPlugin::<Resolve>::default(),
            ExtractComponentPlugin::<Written>::default(),
            ExtractComponentPlugin::<Over>::default(),
            ExtractResourcePlugin::<Shadows>::default(),
        ))
        .init_resource::<Shadows>()
        .add_systems(PostUpdate, follow);
    }

    /// After the render app exists.
    fn finish(&self, app: &mut App) {
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render
            .init_resource::<SpecializedRenderPipelines<Pass>>()
            .add_systems(RenderStartup, init)
            .add_systems(
                Render,
                attach
                    .in_set(RenderSystems::PrepareViews)
                    .after(prepare_view_attachments)
                    .before(prepare_view_targets),
            )
            // After the main pass, in place of the final copy.
            .add_systems(Core3d, resolve.in_set(Core3dSystems::EarlyPostProcess));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shadows toggled: the new variant compiles while the old draws.
    #[test]
    fn a_variant_compiling_leaves_the_last_drawn_one_drawing() {
        let format = TextureFormat::Bgra8UnormSrgb;
        let (old, new) = (
            CachedRenderPipelineId::new(1),
            CachedRenderPipelineId::new(2),
        );
        let mut drawn = Drawn::default();

        assert_eq!(drawn.pick(format, old, |_| false), None, "nothing yet");
        assert_eq!(drawn.pick(format, old, |id| id == old), Some(old));
        assert_eq!(drawn.pick(format, new, |id| id == old), Some(old));
        assert_eq!(drawn.pick(format, new, |_| true), Some(new));
        assert_eq!(drawn.pick(format, old, |id| id == new), Some(new));
    }

    /// Another output's format draws none of this one's.
    #[test]
    fn a_format_never_draws_another_formats_pipeline() {
        let id = CachedRenderPipelineId::new(1);
        let mut drawn = Drawn::default();

        drawn.pick(TextureFormat::Bgra8UnormSrgb, id, |_| true);

        assert_eq!(
            drawn.pick(
                TextureFormat::Rgba8UnormSrgb,
                CachedRenderPipelineId::new(2),
                |_| false
            ),
            None
        );
    }
}
