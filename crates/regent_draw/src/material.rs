//! The two materials a map object is drawn with.
//!
//! Indices 247 to 254 are operations on the pixel underneath
//! ([`regent_core::view::raster::shade`]), not colours. Blend state is per pipeline, so
//! [`Body`] draws colours and [`Band`] the shadows from the same index
//! texture: as coverage that [`crate::shadow`] resolves ([`crate::soft`]),
//! or as a multiply in draw order (`multiplying`). A [`Body`] in
//! [`Pass::Glass`] draws a translucent tile, with no shadow; one in
//! [`Pass::Lift`] clears the coverage under it.
//!
//! # Ordering by depth buffer
//!
//! [`Body`] is [`AlphaMode::Mask`] (binary transparency) and writes depth.
//! [`Band`] runs later in `Transparent3d`, tests depth and writes none, so
//! shadows are rejected behind nearer objects. Under [`Shadows::Soft`] a
//! darkening pixel tests at the depth of the ground it lies on, where
//! that is nearer than its caster ([`lie`]).

use bevy::asset::{AssetPath, embedded_asset, embedded_path};
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::light::{DirectionalLightShadowMap, PointLightShadowMap};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_phase::sort_phase_system;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, ColorWrites,
    RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::{ShaderDefVal, ShaderRef, load_shader_library};
use regent_core::view::raster::shade::{CONTROL_FIRST, TRANSPARENT};

use crate::lie;
use crate::plain::unculled;
use crate::shadow::Shadows;
use crate::smooth::{self, Sampling};
use crate::soft::{self, Ramps};
use crate::sun::relief;
use crate::wind::{Gust, Sway};

/// A tile's colours, looked up through its own palette on the GPU.
///
/// Indexed, as flattening loses the shadows. Would also allow palette-range
/// recolour, which the shipped game disables (`docs/settled.md`).
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(Look)]
pub struct Body {
    #[texture(0, sample_type = "u_int")]
    pub indices: Handle<Image>,
    #[texture(1)]
    pub palette: Handle<Image>,
    pub pass: Pass,
    /// A tree's, bending in the wind ([`crate::wind`]).
    pub sway: Sway,
    #[uniform(2)]
    pub gust: Gust,
    /// Nearest, or sharp-bilinear on the pixels ([`crate::smooth`]).
    pub sampling: Sampling,
    /// The casting buildings' slots, every body's the same
    /// ([`relief::STANDS`]): a model building's bodies read theirs.
    #[texture(3, sample_type = "float", filterable = false)]
    pub stands: Handle<Image>,
    /// The models' reliefs, every body's the same ([`relief::RELIEF`]).
    #[texture(4, sample_type = "float", filterable = false, dimension = "2d_array")]
    pub relief: Handle<Image>,
}

impl Default for Body {
    /// An opaque body, no pictures yet: the shared slots and reliefs.
    fn default() -> Self {
        Body {
            indices: Handle::default(),
            palette: Handle::default(),
            pass: Pass::Solid,
            sway: Sway::Still,
            gust: Gust::STILL,
            sampling: Sampling::Nearest,
            stands: relief::STANDS,
            relief: relief::RELIEF,
        }
    }
}

/// What a [`Body`]'s pipeline is specialised on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Look {
    pub pass: Pass,
    /// [`Sway::Tree`] only while the wind blows: still trees keep the
    /// plain pipeline.
    pub sway: Sway,
    pub sampling: Sampling,
}

/// How a [`Body`] meets what is under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Pass {
    /// Opaque where drawn, in `AlphaMask3d`, writing depth.
    #[default]
    Solid,
    /// Translucent: draw mode 5 or 8, or a see-through unit
    /// ([`regent_core::view::raster::blend`]); its palette's alpha is each index's
    /// share. In `Transparent3d`, depth-tested, writing no depth; colour
    /// blends, alpha keeps the shadow's coverage. **Inferred**: the blend
    /// is in linear colour, the original's in its 16-bit screen's.
    Glass,
    /// A [`Pass::Glass`] picture's twin: alpha 1 where its palette's alpha
    /// is not 0, so the shadow's coverage under it is lifted
    /// ([`soft::lifts`]). In `Transparent3d`, depth-tested, writing no
    /// depth and no colour. Needed by [`Shadows::Soft`], which resolves
    /// last; harmless under [`Shadows::Blend`], which reads no alpha.
    Lift,
}

impl From<&Body> for Look {
    fn from(body: &Body) -> Look {
        Look {
            pass: body.pass,
            sway: swaying(body.sway, &body.gust),
            sampling: body.sampling,
        }
    }
}

/// `sway`, while `gust` blows; else [`Sway::Still`].
fn swaying(sway: Sway, gust: &Gust) -> Sway {
    if gust.blows() { sway } else { Sway::Still }
}

impl Material for Body {
    /// Its own, to bend in the wind ([`crate::wind`]).
    fn vertex_shader() -> ShaderRef {
        Body::fragment_shader()
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/body.wesl")).with_source("embedded"),
        )
    }

    fn alpha_mode(&self) -> AlphaMode {
        match self.pass {
            Pass::Solid => AlphaMode::Mask(CUTOFF),
            Pass::Glass | Pass::Lift => AlphaMode::Blend,
        }
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
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        Body::shape(descriptor, key.bind_group_data);

        Ok(())
    }
}

impl Body {
    /// [`Material::specialize`]'s work, without Bevy's pipeline: the
    /// compile tests call it.
    pub fn shape(descriptor: &mut RenderPipelineDescriptor, look: Look) {
        let keep = BlendComponent {
            src_factor: BlendFactor::Zero,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        };

        let Look {
            pass,
            sway,
            sampling,
        } = look;

        unculled(descriptor);

        if sway == Sway::Tree {
            descriptor.vertex.shader_defs.push("SWAY".into());
        }

        if sampling == Sampling::Sharp {
            descriptor.vertex.shader_defs.extend(snapping());

            if let Ok(fragment) = descriptor.fragment_mut() {
                fragment.shader_defs.push("SHARP".into());
            }
        }

        match (pass, descriptor.fragment_mut()) {
            (Pass::Glass, Ok(fragment)) => {
                fragment.shader_defs.push("GLASS".into());

                for target in fragment.targets.iter_mut().flatten() {
                    target.blend = Some(BlendState {
                        color: BlendState::ALPHA_BLENDING.color,
                        alpha: keep,
                    });
                }
            }

            // Colour kept, alpha replaced: the coverage under it gone.
            (Pass::Lift, Ok(fragment)) => {
                fragment.shader_defs.push("LIFT".into());

                for target in fragment.targets.iter_mut().flatten() {
                    target.blend = Some(BlendState {
                        color: keep,
                        alpha: BlendComponent::REPLACE,
                    });
                    target.write_mask = ColorWrites::ALPHA;
                }
            }

            _ => {}
        }
    }
}

/// The same tile's shadow. No palette: the index is the operation.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(Key)]
pub struct Band {
    #[texture(0, sample_type = "u_int")]
    pub indices: Handle<Image>,
    /// The depth each row's ground draws at: [`lie::ROWS`].
    #[texture(1, sample_type = "float", filterable = false)]
    pub rows: Handle<Image>,
    /// Coverage ([`crate::soft`]) or a multiply blend (`multiplying`).
    pub shadows: Shadows,
    /// The tile's draw mode's ramps: what each index darkens by.
    pub ramps: Ramps,
    /// A tree's, bending with its body ([`crate::wind`]).
    pub sway: Sway,
    #[uniform(2)]
    pub gust: Gust,
    /// As its body's: under [`Sampling::Sharp`] snapped with it.
    pub sampling: Sampling,
    /// Whether its darkening draws: dropped where a model casts the
    /// shadow instead ([`crate::sun`]); glows draw either way.
    pub painted: Painted,
}

/// Whether a [`Band`]'s darkening draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Painted {
    #[default]
    Kept,
    Dropped,
}

/// What a [`Band`]'s pipeline is specialised on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub shadows: Shadows,
    pub ramps: Ramps,
    /// As [`Look::sway`].
    pub sway: Sway,
    pub sampling: Sampling,
    pub painted: Painted,
}

impl From<&Band> for Key {
    fn from(band: &Band) -> Key {
        Key {
            shadows: band.shadows,
            ramps: band.ramps,
            sway: swaying(band.sway, &band.gust),
            sampling: band.sampling,
            painted: band.painted,
        }
    }
}

impl Material for Band {
    /// Its own, for the instance's ground row ([`lie::tag`]).
    fn vertex_shader() -> ShaderRef {
        Band::fragment_shader()
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/band.wesl")).with_source("embedded"),
        )
    }

    /// `Transparent3d`: `AlphaMask3d` is binned, not depth-sorted, so a
    /// body could overwrite a shadow.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
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
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        Band::shape(descriptor, key.bind_group_data);

        Ok(())
    }
}

impl Band {
    /// No depth written, so shadows do not occlude each other; each
    /// index's factor, and its alpha, from [`crate::soft`] as defs.
    ///
    /// [`Material::specialize`]'s work, without Bevy's pipeline: the
    /// compile tests call it.
    pub fn shape(descriptor: &mut RenderPipelineDescriptor, key: Key) {
        let Key {
            shadows,
            ramps,
            sway,
            sampling,
            painted,
        } = key;

        unculled(descriptor);

        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }

        if let Ok(fragment) = descriptor.fragment_mut() {
            let indices = (CONTROL_FIRST..TRANSPARENT)
                .zip(soft::factors(ramps).into_iter().zip(soft::alphas(ramps)));

            for (index, (factor, alpha)) in indices {
                fragment
                    .shader_defs
                    .push(ShaderDefVal::UInt(format!("FACTOR_{index}").into(), factor));
                fragment.shader_defs.push(ShaderDefVal::UInt(
                    format!("ALPHA_{index}").into(),
                    alpha.into(),
                ));
            }

            fragment.shader_defs.extend(lying());

            if painted == Painted::Dropped {
                fragment.shader_defs.push("DROPPED".into());
            }
        }

        match shadows {
            Shadows::Blend => multiplying(descriptor),
            Shadows::Soft => covering(descriptor),
        }

        // One file, both stages: the vertex stage reads the same defs.
        if let Ok(fragment) = descriptor.fragment_mut() {
            let defs = fragment.shader_defs.clone();

            descriptor.vertex.shader_defs.extend(defs);
        }

        if sway == Sway::Tree {
            descriptor.vertex.shader_defs.push("SWAY".into());
        }

        if sampling == Sampling::Sharp {
            descriptor.vertex.shader_defs.extend(snapping());
        }
    }
}

/// The vertex stage's defs for [`smooth::shift`]'s snap: on, and
/// [`smooth::SNAP_BELOW`] less [`smooth::SNAP_MARGIN`] in hundredths, and
/// [`smooth::LATTICE_STEPS`].
pub(crate) fn snapping() -> [ShaderDefVal; 3] {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a few pixels; a few steps"
    )]
    let (below, steps) = (
        ((smooth::SNAP_BELOW - smooth::SNAP_MARGIN) * 100.0).round() as u32,
        smooth::LATTICE_STEPS as u32,
    );

    [
        "SNAP".into(),
        ShaderDefVal::UInt("SNAP_BELOW".into(), below),
        ShaderDefVal::UInt("SNAP_LATTICE".into(), steps),
    ]
}

/// What [`lie`]'s lookup needs as shader defs: art pixels a row in
/// thousandths, the table's samples a cell and a texture row.
fn lying() -> [ShaderDefVal; 3] {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "about 26,000"
    )]
    let row = (lie::ROW * soft::ONE as f32).round() as u32;

    [
        ShaderDefVal::UInt("ROW".into(), row),
        ShaderDefVal::UInt("PER_CELL".into(), lie::PER_CELL),
        ShaderDefVal::UInt("WIDTH".into(), lie::WIDTH),
    ]
}

/// [`Band`]'s pipeline for [`Shadows::Blend`]: colour times the
/// fragment's factor as each shadow draws, on whatever is drawn by then,
/// alpha kept.
///
/// ```text
///  out = dst × (src.rgb + src.a)       blend: src × Dst + dst × SrcAlpha
///  darken   src = (0, 0, 0, f)        f < 1
///  glow     src = (f − 1, …, 1)       1 ≤ f ≤ 2
/// ```
///
/// Shadows stack, as the engine's do, in draw order; things drawn after
/// a shadow stay unshaded. The target is sRGB, so the blend multiplies
/// linear values, as [`soft::DARK`] is fitted.
fn multiplying(descriptor: &mut RenderPipelineDescriptor) {
    let keep = BlendComponent {
        src_factor: BlendFactor::Zero,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    };

    // dst × (src.rgb + src.a): under one darkens, over one glows.
    let times = BlendComponent {
        src_factor: BlendFactor::Dst,
        dst_factor: BlendFactor::SrcAlpha,
        operation: BlendOperation::Add,
    };

    if let Ok(fragment) = descriptor.fragment_mut() {
        fragment.shader_defs.push("BLEND".into());

        for target in fragment.targets.iter_mut().flatten() {
            target.blend = Some(BlendState {
                color: times,
                alpha: keep,
            });
            target.write_mask = ColorWrites::ALL;
        }
    }
}

/// [`Band`]'s pipeline for [`Shadows::Soft`]: alpha the least of the
/// target's and the fragment's coverage, colour times a glow's factor
/// ([`crate::soft`]).
fn covering(descriptor: &mut RenderPipelineDescriptor) {
    // dst × (1 + src.rgb): a glow's gain, nothing for a shadow.
    let gain = BlendComponent {
        src_factor: BlendFactor::Dst,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    };

    // Min takes no factors; WebGPU wants them One.
    let least = BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Min,
    };

    if let Ok(fragment) = descriptor.fragment_mut() {
        fragment.shader_defs.push("SOFT".into());

        for target in fragment.targets.iter_mut().flatten() {
            target.blend = Some(BlendState {
                color: gain,
                alpha: least,
            });
            target.write_mask = ColorWrites::ALL;
        }
    }
}

/// The alpha a fragment must reach to count as drawn.
///
/// The shaders discard outright; any value opaque pixels clear will do.
pub const CUTOFF: f32 = 0.5;

/// Register both materials and their shaders.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/body.wesl");
        embedded_asset!(app, "shaders/band.wesl");
        load_shader_library!(app, "shaders/sway.wesl");
        load_shader_library!(app, "shaders/smooth.wesl");
        load_shader_library!(app, "shaders/lit.wesl");

        app.add_plugins((
            MaterialPlugin::<Body>::default(),
            MaterialPlugin::<Band>::default(),
            crate::plain::Plugin,
        ))
        // No shadow map texels while nothing casts (`stage::IDLE_SHADOW_MAP`).
        .insert_resource(DirectionalLightShadowMap {
            size: crate::stage::IDLE_SHADOW_MAP,
        })
        .insert_resource(PointLightShadowMap {
            size: crate::stage::IDLE_SHADOW_MAP,
        })
        .init_resource::<crate::wind::Wind>()
        .init_resource::<crate::wind::Trees>()
        .add_systems(Startup, (lie::start, relief::start, crate::stage::clusters))
        .add_systems(
            PostUpdate,
            // A band coming out is tagged the frame it does.
            (
                lie::rows,
                lie::tag.after(crate::cull::cull),
                crate::wind::blow,
            ),
        );

        // Moved glass, bands and soft bodies in their new place in the
        // order (`sort`).
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(
                Render,
                crate::sort::rekey
                    .in_set(RenderSystems::PhaseSort)
                    .before(sort_phase_system::<Transparent3d>),
            );
        }
    }
}
