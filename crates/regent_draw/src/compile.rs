//! Every shader compiled as Bevy's pipeline cache would, without a GPU:
//! WESL to WGSL by Bevy's own [`ShaderCache`], then naga's parse and
//! validation, which wgpu runs on the device. A shader error otherwise
//! shows only at run time, in the log.
//!
//! ```text
//!  Library::new ── headless App: Bevy's shader libraries ─┐
//!               └─ ours (SHADERS) ───────────────────────┴─> ShaderCache
//!  check(path, defs) ── wesl::compile ── naga parse ── validate
//! ```
//!
//! The defs are a pipeline's: the world's materials' own `shape`, which
//! their `specialize` calls, over the 3D mesh pipeline's defs
//! ([`world`]); the upscale quads' `specialize` as Bevy's
//! `Material2dPipelineSpecializer` calls it ([`material`]).

use std::time::{Duration, Instant};

use bevy::asset::AssetId;
use bevy::mesh::{BaseMeshPipelineKey, Mesh, MeshVertexBufferLayoutRef, MeshVertexBufferLayouts};
use bevy::prelude::*;
use bevy::render::render_resource::{
    BindGroupLayoutDescriptor, RenderPipelineDescriptor, SpecializedMeshPipeline, TextureFormat,
};
use bevy::shader::{
    ShaderCache, ShaderCacheError, ShaderCacheSource, ShaderDefVal, ShaderRef, ValidateShader,
};
use bevy::sprite_render::{
    Material2d, Material2dKey, Material2dPipeline, Mesh2dPipeline, Mesh2dPipelineKey,
    alpha_mode_pipeline_key_2d,
};
use wgpu::naga;

/// `regent_draw`'s shaders as `embedded_asset!` names them, and their source.
pub const SHADERS: &[(&str, &str)] = &[
    (
        "embedded://regent_draw/shaders/band.wesl",
        include_str!("shaders/band.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/body.wesl",
        include_str!("shaders/body.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/card.wesl",
        include_str!("shaders/card.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/easu.wesl",
        include_str!("shaders/easu.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/lit.wesl",
        include_str!("shaders/lit.wesl"),
    ),
    (
        "embedded://regent_draw/sun/bare.wesl",
        include_str!("sun/bare.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/plain.wesl",
        include_str!("shaders/plain.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/rcas.wesl",
        include_str!("shaders/rcas.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/ring.wesl",
        include_str!("shaders/ring.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/shadow.wesl",
        include_str!("shaders/shadow.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/sharp.wesl",
        include_str!("shaders/sharp.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/sheet.wesl",
        include_str!("shaders/sheet.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/smooth.wesl",
        include_str!("shaders/smooth.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/srgb.wesl",
        include_str!("shaders/srgb.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/sway.wesl",
        include_str!("shaders/sway.wesl"),
    ),
    (
        "embedded://regent_draw/shaders/veil.wesl",
        include_str!("shaders/veil.wesl"),
    ),
];

/// Mesh2d's bindings, which Bevy loads only once a device is up
/// (`load_mesh2d_bindings`): with storage buffers, so without defs.
const MESH2D_BINDINGS: &str = "embedded://bevy_sprite_render/mesh2d/bindings.wesl";

/// Bevy's own vertex stage for a material without one.
const MESH2D: &str = "embedded://bevy_sprite_render/mesh2d/mesh2d.wesl";

/// The 3D mesh bindings, loaded as the 2D ones: once a device is up,
/// with storage buffers here.
const MESH3D_BINDINGS: &str = "embedded://bevy_pbr/render/mesh_bindings.wesl";

/// Bevy's 3D material bind group (`bevy_pbr::MATERIAL_BIND_GROUP_INDEX`).
const MATERIAL3D_GROUP: u32 = 3;

/// The storage buffers a shader stage may bind: wgpu's default limit.
/// Bevy's pipeline cache makes global defs of the device's (**Inferred**:
/// the GPUs this runs on allow at least as many).
const STORAGE_BINDINGS: u32 = 8;

/// The updates the asset count must hold still for Bevy's libraries to
/// count as loaded, and the longest wait for that.
const SETTLED: u32 = 20;
const PATIENCE: Duration = Duration::from_secs(30);

/// A shader stage that failed: its file, defs and the error.
#[derive(Debug)]
pub struct Failure {
    pub path: String,
    pub defs: Vec<ShaderDefVal>,
    pub error: String,
}

/// Bevy's shader libraries and ours, ready to compile.
pub struct Library {
    cache: ShaderCache<(), ()>,
    ids: Vec<(String, AssetId<Shader>)>,
}

impl Library {
    /// Bevy's libraries, loaded by a headless App, and `ours`: `(path,
    /// source)` as [`SHADERS`].
    ///
    /// # Panics
    ///
    /// Bevy's libraries not loaded within `PATIENCE`.
    #[must_use]
    pub fn new(ours: &[&[(&str, &'static str)]]) -> Library {
        let mut app = App::new();

        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::log::LogPlugin>()
                .disable::<bevy::audio::AudioPlugin>()
                .set(bevy::render::RenderPlugin {
                    render_creation: bevy::render::settings::WgpuSettings {
                        backends: None,
                        ..default()
                    }
                    .into(),
                    ..default()
                }),
        );
        app.finish();
        app.cleanup();

        // Default settings: no defs.
        let bindings: Handle<Shader> = app.world().resource::<AssetServer>().load(MESH2D_BINDINGS);
        let mesh2d: Handle<Shader> = app.world().resource::<AssetServer>().load(MESH2D);
        let mesh3d: Handle<Shader> = app.world().resource::<AssetServer>().load(MESH3D_BINDINGS);

        // Held: a dropped handle's asset goes on the next update.
        let mut shaders = app.world_mut().resource_mut::<Assets<Shader>>();
        let _ours: Vec<Handle<Shader>> = ours
            .iter()
            .copied()
            .flatten()
            .map(|(path, source)| shaders.add(Shader::from_wesl(*source, *path)))
            .collect();

        // Embedded loads take a few updates; done once nothing new comes.
        let start = Instant::now();
        let mut still = 0;
        let mut count = 0;

        while still < SETTLED {
            assert!(start.elapsed() < PATIENCE, "Bevy's shaders not loaded");

            app.update();
            std::thread::sleep(Duration::from_millis(5));

            let now = app.world().resource::<Assets<Shader>>().len();
            let loaded = [&bindings, &mesh2d, &mesh3d]
                .iter()
                .all(|h| app.world().resource::<Assets<Shader>>().contains(*h));

            still = if now == count && loaded { still + 1 } else { 0 };
            count = now;
        }

        let mut cache = ShaderCache::new((), validate);
        let globals = [
            ShaderDefVal::UInt("AVAILABLE_STORAGE_BUFFER_BINDINGS".into(), STORAGE_BINDINGS),
            ShaderDefVal::Bool(
                "AVAILABLE_STORAGE_BUFFER_BINDINGS__GE_3".into(),
                STORAGE_BINDINGS >= 3,
            ),
            ShaderDefVal::Bool(
                "AVAILABLE_STORAGE_BUFFER_BINDINGS__GE_6".into(),
                STORAGE_BINDINGS >= 6,
            ),
        ];

        // As the pipeline cache's `extract_shaders`: the globals first.
        let ids = app
            .world()
            .resource::<Assets<Shader>>()
            .iter()
            .map(|(id, shader)| {
                let mut shader = shader.clone();

                shader.shader_defs.splice(0..0, globals.iter().cloned());
                cache.set_shader(id, shader.clone());

                (shader.path, id)
            })
            .collect();

        Library { cache, ids }
    }

    /// `path` compiled with `defs`.
    ///
    /// # Errors
    ///
    /// A path not loaded, or the compile's or validation's error.
    pub fn check(&mut self, path: &str, defs: &[ShaderDefVal]) -> Result<(), Failure> {
        let failure = |error: String| Failure {
            path: path.to_owned(),
            defs: defs.to_vec(),
            error,
        };
        let id = self
            .ids
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, id)| *id)
            .ok_or_else(|| failure("not loaded".to_owned()))?;

        self.cache
            .get(usize::MAX, id, defs)
            .map(|_| ())
            .map_err(|e| failure(e.to_string()))
    }

    /// Both stages of `descriptor`, from `vertex` and `fragment`: a
    /// [`ShaderRef::Default`] is Bevy's mesh2d.
    ///
    /// # Errors
    ///
    /// The first stage that fails.
    pub fn pipeline(
        &mut self,
        descriptor: &RenderPipelineDescriptor,
        vertex: &ShaderRef,
        fragment: &ShaderRef,
    ) -> Result<(), Failure> {
        self.check(&path(vertex), &descriptor.vertex.shader_defs)?;

        match &descriptor.fragment {
            Some(stage) => self.check(&path(fragment), &stage.shader_defs),
            None => Ok(()),
        }
    }
}

/// A [`ShaderRef`]'s asset path; Bevy's mesh2d for the default.
fn path(shader: &ShaderRef) -> String {
    match shader {
        ShaderRef::Path(path) => path.to_string(),
        ShaderRef::Default | ShaderRef::Handle(_) => MESH2D.to_owned(),
    }
}

/// The WGSL parsed and validated as wgpu does on the device.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "the shader cache's loader signature"
)]
fn validate(
    _device: &(),
    source: ShaderCacheSource,
    _validate: &ValidateShader,
) -> Result<(), ShaderCacheError> {
    match source {
        ShaderCacheSource::Wgsl(wgsl) => {
            wgsl_valid(&wgsl).map_err(ShaderCacheError::CreateShaderModule)
        }
        ShaderCacheSource::SpirV(_) => Err(ShaderCacheError::CreateShaderModule(
            "SPIR-V is not used".to_owned(),
        )),
    }
}

/// `wgsl` parsed and validated by naga, as wgpu's `create_shader_module`.
///
/// # Errors
///
/// naga's error, with the source's lines.
pub fn wgsl_valid(wgsl: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| e.emit_to_string(wgsl))?;

    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map(|_| ())
    .map_err(|e| e.emit_to_string(wgsl))
}

/// Whether a world material's alpha mode discards fragments.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cutout {
    Discard,
    Keep,
}

impl Cutout {
    #[must_use]
    pub const fn of(discards: bool) -> Self {
        if discards { Self::Discard } else { Self::Keep }
    }
}

/// A world material's pipeline for `mesh`, before the material's
/// `shape`: the defs and state Bevy's 3D mesh pipeline gives it under the
/// world camera (orthographic, no MSAA, no tonemapping, no HDR), its
/// alpha mode `cutout` adding the discard; one sRGB target. **Inferred**
/// from `MeshPipeline::specialize`, which needs a device to call.
#[must_use]
pub fn world(mesh: &Mesh, cutout: Cutout) -> RenderPipelineDescriptor {
    use bevy::render::render_resource::{ColorTargetState, ColorWrites, FragmentState};

    let mut defs: Vec<ShaderDefVal> = vec![
        "MESH_PIPELINE".into(),
        "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
        "VERTEX_POSITIONS".into(),
        "VIEW_PROJECTION_ORTHOGRAPHIC".into(),
        "SHADOW_FILTER_METHOD_GAUSSIAN".into(),
        // Bevy's `experimental_pbr_pcss` feature: the map's linear sampler.
        "PCSS_SAMPLERS_AVAILABLE".into(),
        ShaderDefVal::Int("SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS".into(), 4),
        ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), MATERIAL3D_GROUP),
    ];

    if mesh.contains_attribute(Mesh::ATTRIBUTE_UV_0) {
        defs.extend(["VERTEX_UVS".into(), "VERTEX_UVS_A".into()]);
    }

    if mesh.contains_attribute(Mesh::ATTRIBUTE_UV_1) {
        defs.extend(["VERTEX_UVS".into(), "VERTEX_UVS_B".into()]);
    }

    if mesh.contains_attribute(Mesh::ATTRIBUTE_COLOR) {
        defs.push("VERTEX_COLORS".into());
    }

    if cutout == Cutout::Discard {
        defs.push("MAY_DISCARD".into());
    }

    RenderPipelineDescriptor {
        vertex: bevy::render::render_resource::VertexState {
            shader_defs: defs.clone(),
            ..default()
        },
        fragment: Some(FragmentState {
            shader_defs: defs,
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    }
}

/// The views a mesh2d pipeline is keyed on here: the world camera's
/// sRGB target, plain and as Bevy keys a non-HDR camera's tonemapping
/// (`check_views_need_specialization`).
#[must_use]
pub fn views() -> [Mesh2dPipelineKey; 2] {
    let plain = Mesh2dPipelineKey::from_msaa_samples(1)
        | Mesh2dPipelineKey::from_target_format(TextureFormat::Rgba8UnormSrgb);

    [
        plain,
        plain
            | Mesh2dPipelineKey::TONEMAP_IN_SHADER
            | Mesh2dPipelineKey::TONEMAP_METHOD_LINEAR
            | Mesh2dPipelineKey::DEBAND_DITHER,
    ]
}

/// A material's pipeline for `mesh` under `view`, its defs as Bevy's
/// `Material2dPipelineSpecializer` makes them: mesh2d's, the material
/// bind group, then the material's `specialize`.
///
/// # Panics
///
/// A specialize that fails.
#[must_use]
pub fn material<M: Material2d>(
    mesh: &Mesh,
    view: Mesh2dPipelineKey,
    material: &M,
) -> RenderPipelineDescriptor {
    /// Bevy's material bind group index in 2D (`MATERIAL_2D_BIND_GROUP_INDEX`).
    const MATERIAL_GROUP: u32 = 2;

    let pipeline = Material2dPipeline {
        mesh2d_pipeline: Mesh2dPipeline {
            view_layout: BindGroupLayoutDescriptor::new("view", &[]),
            mesh_layout: BindGroupLayoutDescriptor::new("mesh", &[]),
            shader: Handle::default(),
            per_object_buffer_batch_size: None,
        },
    };
    let layout: MeshVertexBufferLayoutRef =
        mesh.get_mesh_vertex_buffer_layout(&mut MeshVertexBufferLayouts::default());
    let topology = BaseMeshPipelineKey::from_primitive_topology_and_strip_index(
        mesh.primitive_topology(),
        None,
    );
    let mesh_key = view
        | alpha_mode_pipeline_key_2d(material.alpha_mode().into())
        | Mesh2dPipelineKey::from_bits_retain(topology.bits());

    let mut descriptor = pipeline
        .mesh2d_pipeline
        .specialize(mesh_key, &layout)
        .expect("mesh2d's pipeline");
    let group = ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), MATERIAL_GROUP);

    descriptor.vertex.shader_defs.push(group.clone());

    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader_defs.push(group);
    }

    let key = Material2dKey::<M> {
        mesh_key,
        bind_group_data: material.bind_group_data(),
    };

    M::specialize(&pipeline, &mut descriptor, &layout, key).expect("the material's specialize");

    descriptor
}

#[cfg(test)]
mod tests {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::PrimitiveTopology;

    use bevy::pbr::Material;

    use super::*;
    use crate::material::{Band, Body, Key, Look, Painted, Pass};
    use crate::plain::{Alpha, Key as PlainKey, Lay, Plain};
    use crate::present::{EasuPass, RcasPass, Sharp};
    use crate::ring::{Ellipse, Ink};
    use crate::shadow::Shadows;
    use crate::smooth::{Sampling, Sheet};
    use crate::soft::Ramps;
    use crate::sun::bare::Bare;
    use crate::veil::Veil;
    use crate::wind::{Gust, Sway};

    const PASSES: [Pass; 3] = [Pass::Solid, Pass::Glass, Pass::Lift];
    const SWAYS: [Sway; 2] = [Sway::Still, Sway::Tree];
    const SAMPLINGS: [Sampling; 2] = [Sampling::Nearest, Sampling::Sharp];
    const SHADOWS: [Shadows; 2] = [Shadows::Blend, Shadows::Soft];
    const RAMPS: [Ramps; 2] = [Ramps::Band, Ramps::Shadow];
    const PAINTED: [Painted; 2] = [Painted::Kept, Painted::Dropped];
    const INKS: [Ink; 4] = [Ink::Glow, Ink::Lift, Ink::Colour, Ink::Line];

    /// A sprite's quad: `Rectangle`'s positions, normals and uvs.
    fn quad() -> Mesh {
        Mesh::from(Rectangle::new(1.0, 1.0))
    }

    /// A ground patch's: positions and uvs (`map::patch`).
    fn patch() -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);

        mesh
    }

    /// A model's: positions and flat normals (`sun::shape`).
    fn model() -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 3]);

        mesh
    }

    /// A veil chunk's: positions and colours (`veil::mesh`).
    fn chunk() -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);

        mesh
    }

    /// `sway` as a material's: a tree only while the wind blows.
    fn gust(sway: Sway) -> Gust {
        match sway {
            Sway::Still => Gust::STILL,
            Sway::Tree => Gust {
                reach: 1.0,
                ..Gust::STILL
            },
        }
    }

    /// Every body's pipeline: each pass, sway and sampling.
    fn bodies() -> Vec<RenderPipelineDescriptor> {
        let mut out = Vec::new();

        for pass in PASSES {
            for sway in SWAYS {
                for sampling in SAMPLINGS {
                    let body = Body {
                        indices: Handle::default(),
                        palette: Handle::default(),
                        pass,
                        sway,
                        gust: gust(sway),
                        sampling,
                        ..Body::default()
                    };
                    let mut descriptor = world(&quad(), Cutout::of(pass == Pass::Solid));

                    Body::shape(&mut descriptor, Look::from(&body));
                    out.push(descriptor);
                }
            }
        }

        out
    }

    /// Every band's pipeline: each shadow mode, ramps, sway, sampling
    /// and painted.
    fn bands() -> Vec<RenderPipelineDescriptor> {
        let mut out = Vec::new();

        for shadows in SHADOWS {
            for ramps in RAMPS {
                for sway in SWAYS {
                    for sampling in SAMPLINGS {
                        for painted in PAINTED {
                            let band = Band {
                                indices: Handle::default(),
                                rows: Handle::default(),
                                shadows,
                                ramps,
                                sway,
                                gust: gust(sway),
                                feet: Handle::default(),
                                sampling,
                                painted,
                            };
                            let mut descriptor = world(&quad(), Cutout::Keep);

                            Band::shape(&mut descriptor, Key::from(&band));
                            out.push(descriptor);
                        }
                    }
                }
            }
        }

        out
    }

    /// Every plain mesh's pipeline: each alpha and lay, with a picture or
    /// none, on the meshes that take it: the ground's, a quad, the reach's
    /// coloured one.
    fn plains() -> Vec<RenderPipelineDescriptor> {
        let mut out = Vec::new();

        for alpha in [Alpha::Mask, Alpha::MaskFirst, Alpha::Blend] {
            for (mesh, lay) in [
                (patch(), Lay::Ground),
                (quad(), Lay::Card),
                (chunk(), Lay::Card),
            ] {
                for pictured in [false, true] {
                    let mut descriptor = world(&mesh, Cutout::of(alpha == Alpha::Mask));

                    Plain::shape(
                        &mut descriptor,
                        PlainKey {
                            alpha,
                            lay,
                            pictured,
                        },
                    );
                    out.push(descriptor);
                }
            }
        }

        out
    }

    /// Each material's every pipeline, and the shadow resolve's fragment,
    /// compile and validate. A failure lists each stage with its defs.
    #[test]
    #[allow(clippy::too_many_lines, reason = "one list of every pipeline")]
    fn every_shader_compiles_with_the_defs_its_pipelines_use() {
        let mut library = Library::new(&[SHADERS]);
        let mut failures = Vec::new();
        let mut checked = 0;
        let mut check =
            |library: &mut Library, descriptor: RenderPipelineDescriptor, vertex, fragment| {
                checked += 1;

                if let Err(failure) = library.pipeline(&descriptor, &vertex, &fragment) {
                    failures.push(failure);
                }
            };

        for descriptor in bodies() {
            check(
                &mut library,
                descriptor,
                <Body as Material>::vertex_shader(),
                <Body as Material>::fragment_shader(),
            );
        }

        for descriptor in bands() {
            check(
                &mut library,
                descriptor,
                <Band as Material>::vertex_shader(),
                <Band as Material>::fragment_shader(),
            );
        }

        for descriptor in plains() {
            check(
                &mut library,
                descriptor,
                <Plain as Material>::vertex_shader(),
                <Plain as Material>::fragment_shader(),
            );
        }

        for ink in INKS {
            let mut descriptor = world(&quad(), Cutout::Keep);

            Ellipse::shape(&mut descriptor, ink);
            check(
                &mut library,
                descriptor,
                <Ellipse as Material>::vertex_shader(),
                <Ellipse as Material>::fragment_shader(),
            );
        }

        let mut sheet = world(&patch(), Cutout::Keep);
        let mut veil = world(&chunk(), Cutout::Keep);

        Sheet::shape(&mut sheet);
        Veil::shape(&mut veil);

        check(
            &mut library,
            sheet,
            <Sheet as Material>::vertex_shader(),
            <Sheet as Material>::fragment_shader(),
        );
        check(
            &mut library,
            veil,
            <Veil as Material>::vertex_shader(),
            <Veil as Material>::fragment_shader(),
        );
        check(
            &mut library,
            world(&model(), Cutout::Keep),
            <Bare as Material>::vertex_shader(),
            <Bare as Material>::fragment_shader(),
        );

        // The upscale quads: 2D, drawn by the shown and EASU cameras.
        for view in views() {
            let sharp = Sharp {
                mapping: default(),
                picture: Handle::default(),
            };
            let easu = EasuPass {
                con: default(),
                picture: Handle::default(),
            };
            let rcas = RcasPass {
                rcas: default(),
                picture: Handle::default(),
            };

            check(
                &mut library,
                material(&quad(), view, &sharp),
                Sharp::vertex_shader(),
                Sharp::fragment_shader(),
            );
            check(
                &mut library,
                material(&quad(), view, &easu),
                EasuPass::vertex_shader(),
                EasuPass::fragment_shader(),
            );
            check(
                &mut library,
                material(&quad(), view, &rcas),
                RcasPass::vertex_shader(),
                RcasPass::fragment_shader(),
            );
        }

        for shadows in SHADOWS {
            checked += 1;

            let defs = crate::shadow::defs(shadows);

            if let Err(failure) = library.check("embedded://regent_draw/shaders/shadow.wesl", &defs)
            {
                failures.push(failure);
            }
        }

        assert_eq!(checked, 77, "pipeline variants");
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// The art service's shrink, which wgpu compiles itself (`art::gpu`).
    #[test]
    fn the_shrink_shader_is_valid() {
        let checked = wgsl_valid(include_str!("shaders/shrink.wgsl"));

        assert!(checked.is_ok(), "{checked:?}");
    }

    /// A broken shader fails: the check is not vacuous. A `let` in an
    /// `@if` block shares the function's scope, as the one that slipped
    /// through to the log.
    #[test]
    fn a_shader_error_fails_the_check() {
        const BROKEN: &[(&str, &str)] = &[(
            "embedded://regent_draw/shaders/broken.wesl",
            "@fragment fn fragment() -> @location(0) vec4<f32> {
                let here = 1.0;
            @if(TWICE) {
                let here = 2.0;
            }
                return vec4<f32>(here);
            }",
        )];

        let mut library = Library::new(&[BROKEN]);
        let path = BROKEN[0].0;

        assert!(library.check(path, &[]).is_ok());
        assert!(library.check(path, &["TWICE".into()]).is_err());
    }
}
