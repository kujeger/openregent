//! Pictures upscaled by AMD FSR 1 into egui textures, on the GPU: EASU
//! to the size they show at, then RCAS (`[enhancements] video_upscale`,
//! openregent's). The shaders are [`present`]'s, run by raw passes.
//!
//! ```text
//!  user (main world)     a Target image, lent to egui; each new
//!    │                   picture or size an Ask in Asks
//!  extract               the asks to the render world, joining any
//!    │                   whose target is not on the GPU yet
//!  draw (Render, Queue)  picture ──upload──> source ──EASU──> easu image
//!                          ──RCAS──> target, before egui's pass
//! ```
//!
//! The source and easu image are kept per target, made again when a
//! size changes, freed when the target goes.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems};
use bevy_egui::egui::{ColorImage, TextureId};
use bevy_egui::{EguiTextureHandle, EguiUserTextures};
use wgpu::util::DeviceExt;

use crate::present::{self, EasuCon};

/// How a video reaches the screen.
#[derive(
    Resource, Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum VideoUpscale {
    /// egui's linear sampler, as drawn so far.
    #[default]
    Linear,
    /// AMD FSR 1, EASU then RCAS, where it shows larger than it is.
    Fsr,
}

/// Every texture's format: sampled and written as sRGB, so the passes
/// see linear light, as [`present`]'s images.
const FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// Frames an ask waits for its target to reach the GPU, then is dropped.
const TRIES: u8 = 8;

/// The size `source` is upscaled to, to show at `wanted` pixels: none
/// unless larger on both sides, and within the GPU's `side`.
#[must_use]
pub fn fitted(source: UVec2, wanted: UVec2, side: u32) -> Option<UVec2> {
    let larger = wanted.cmpgt(source).all();
    let fits = wanted.cmple(UVec2::splat(side)).all();

    (larger && fits).then_some(wanted)
}

/// An upscale's target, lent to egui: held while shown.
pub struct Target {
    /// Its image: dropped, egui frees the id.
    handle: Handle<Image>,
    pub id: TextureId,
    pub size: UVec2,
}

impl Target {
    /// A target `size` pixels, its image added to `images` and lent to
    /// egui through `textures`.
    pub fn new(images: &mut Assets<Image>, textures: &mut EguiUserTextures, size: UVec2) -> Target {
        let handle = images.add(target(size));
        let id = textures.add_image(EguiTextureHandle::Weak(handle.id()));

        Target { handle, id, size }
    }

    /// Its image, for an [`Ask`].
    #[must_use]
    pub fn image(&self) -> AssetId<Image> {
        self.handle.id()
    }
}

/// An image `size` pixels for the passes to draw into and egui to show,
/// sampled as egui's own: linear, clamped.
fn target(size: UVec2) -> Image {
    let mut image = Image::new_uninit(
        extent(size),
        TextureDimension::D2,
        FORMAT,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });

    image
}

/// One upscale: `picture` into all of `target`.
#[derive(Clone)]
pub struct Ask {
    pub picture: Arc<ColorImage>,
    pub target: AssetId<Image>,
}

/// This frame's asks, cleared as the next begins.
#[derive(Resource, Default)]
pub struct Asks(pub Vec<Ask>);

/// Clears, extracts and draws.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Asks>().add_systems(First, clear);
    }

    fn finish(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<Waiting>()
                .init_resource::<Held>()
                .add_systems(RenderStartup, init)
                .add_systems(ExtractSchedule, extract)
                .add_systems(Render, draw.in_set(RenderSystems::Queue));
        }
    }
}

/// Last frame's asks gone; untouched when none, so a game's frames
/// leave it unchanged.
fn clear(mut asks: ResMut<Asks>) {
    if !asks.0.is_empty() {
        asks.0.clear();
    }
}

/// Asks not drawn yet, and the frames each has waited.
#[derive(Resource, Default)]
struct Waiting(Vec<(Ask, u8)>);

/// Each target's source and easu image.
#[derive(Resource, Default)]
struct Held(HashMap<AssetId<Image>, Textures>);

struct Textures {
    source: wgpu::Texture,
    easu: wgpu::Texture,
}

/// The frame's asks joined to those waiting: a newer one for a target
/// takes the older's place.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn extract(asks: Extract<Res<Asks>>, mut waiting: ResMut<Waiting>) {
    for ask in &asks.0 {
        waiting.0.retain(|(old, _)| old.target != ask.target);
        waiting.0.push((ask.clone(), 0));
    }
}

/// The pipelines, made at once: no frame waits on them.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn init(mut commands: Commands, device: Res<RenderDevice>) {
    commands.insert_resource(Fsr::new(device.wgpu_device()));
}

/// The asks whose targets are on the GPU drawn, submitted before the
/// frame's own passes; the rest wait. Textures of targets gone freed.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn draw(
    (mut waiting, mut held): (ResMut<Waiting>, ResMut<Held>),
    images: Res<RenderAssets<GpuImage>>,
    (device, queue): (Res<RenderDevice>, Res<RenderQueue>),
    fsr: Option<Res<Fsr>>,
) {
    held.0.retain(|target, _| images.get(*target).is_some());

    if let Some(fsr) = fsr.filter(|_| !waiting.0.is_empty()) {
        let device = device.wgpu_device();
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fsr") });

        waiting.0.retain_mut(|(ask, tries)| {
            if let Some(target) = images.get(ask.target) {
                let size = target.texture.size();
                let textures = held
                    .0
                    .entry(ask.target)
                    .and_modify(|t| t.fit(device, &ask.picture, size))
                    .or_insert_with(|| Textures::new(device, &ask.picture, size));

                fsr.encode(
                    device,
                    &queue,
                    &mut encoder,
                    (&ask.picture, textures, &target.texture),
                );
                false
            } else {
                *tries += 1;
                *tries < TRIES
            }
        });

        queue.submit([encoder.finish()]);
    }
}

impl Textures {
    /// A source for `picture`, an easu image `size`.
    fn new(device: &wgpu::Device, picture: &ColorImage, size: Extent3d) -> Textures {
        Textures {
            source: texture(device, picture_extent(picture), TextureUsages::COPY_DST),
            easu: texture(device, size, TextureUsages::RENDER_ATTACHMENT),
        }
    }

    /// Each made again where its size is not `picture`'s or `size`.
    fn fit(&mut self, device: &wgpu::Device, picture: &ColorImage, size: Extent3d) {
        if self.source.size() != picture_extent(picture) {
            self.source = texture(device, picture_extent(picture), TextureUsages::COPY_DST);
        }

        if self.easu.size() != size {
            self.easu = texture(device, size, TextureUsages::RENDER_ATTACHMENT);
        }
    }
}

/// A texture `size`, sampled, and `usage`.
fn texture(device: &wgpu::Device, size: Extent3d, usage: TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fsr"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: FORMAT,
        usage: usage | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn picture_extent(picture: &ColorImage) -> Extent3d {
    let pixels = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);

    extent(UVec2::new(pixels(picture.size[0]), pixels(picture.size[1])))
}

const fn extent(size: UVec2) -> Extent3d {
    Extent3d {
        width: size.x,
        height: size.y,
        depth_or_array_layers: 1,
    }
}

/// One triangle over the target, for [`present`]'s fragment shaders,
/// whose only input is the fragment's position.
const VERTEX: &str = "
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vertex(@builtin(vertex_index) i: u32) -> VertexOutput {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));

    return VertexOutput(vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0));
}
";

/// The sRGB curve the passes import, and the import's start.
const SRGB: &str = include_str!("../shaders/srgb.wesl");
const SRGB_IMPORT: &str = "import regent_draw::shaders::srgb::";

/// A [`present`] material shader as plain WGSL: its import lines gone,
/// the sRGB curve's text in place of its import, the material's group
/// 0, [`VERTEX`] before it.
fn plain(wesl: &str) -> String {
    let body: Vec<&str> = wesl
        .lines()
        .map(|line| {
            if line.starts_with(SRGB_IMPORT) {
                SRGB
            } else {
                line
            }
        })
        .filter(|line| !line.starts_with("import "))
        .collect();

    format!("{VERTEX}\n{}", body.join("\n")).replace("constants::MATERIAL_BIND_GROUP", "0")
}

/// RCAS's uniform: its constants, and the corner, 0: the pass covers
/// the target alone.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RcasUniform {
    con: [u32; 4],
    corner: [f32; 2],
    pad: [f32; 2],
}

/// The two passes' pipelines and EASU's sampler.
#[derive(Resource)]
pub struct Fsr {
    easu: Pass,
    rcas: Pass,
    sampler: wgpu::Sampler,
}

struct Pass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl Fsr {
    /// The pipelines on `device`.
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Fsr {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fsr easu"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..default()
        });

        Fsr {
            easu: Pass::new(
                device,
                "fsr easu",
                include_str!("../shaders/easu.wesl"),
                Sampled::Yes,
            ),
            rcas: Pass::new(
                device,
                "fsr rcas",
                include_str!("../shaders/rcas.wesl"),
                Sampled::No,
            ),
            sampler,
        }
    }

    /// `picture` uploaded to the source, EASU into the easu image, RCAS
    /// into `target`.
    fn encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        (picture, textures, target): (&ColorImage, &Textures, &wgpu::Texture),
    ) {
        let size = picture_extent(picture);

        queue.write_texture(
            textures.source.as_image_copy(),
            bytemuck::cast_slice(&picture.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.width * 4),
                rows_per_image: None,
            },
            size,
        );

        // FsrEasuCon: the whole picture is the viewport.
        #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
        let input = Vec2::new(size.width as f32, size.height as f32);
        #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
        let output = Vec2::new(target.width() as f32, target.height() as f32);
        let con = EasuCon::offset(input, input, output, Vec2::ZERO);
        let words: [UVec4; 4] = [con.con0, con.con1, con.con2, con.con3];
        let rcas = RcasUniform {
            con: present::rcas_con(present::SHARPNESS).to_array(),
            corner: [0.0; 2],
            pad: [0.0; 2],
        };

        let source = textures.source.create_view(&default());
        let easu = textures.easu.create_view(&default());
        let shown = target.create_view(&default());
        let easu_words: Vec<u32> = words.iter().flat_map(UVec4::to_array).collect();

        self.easu.run(
            device,
            encoder,
            (
                bytemuck::cast_slice(&easu_words),
                &source,
                Some(&self.sampler),
            ),
            &easu,
        );
        self.rcas.run(
            device,
            encoder,
            (bytemuck::bytes_of(&rcas), &easu, None),
            &shown,
        );
    }
}

/// Whether a pass samples its picture: EASU gathers, RCAS loads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sampled {
    Yes,
    No,
}

impl Pass {
    /// A pass of `wesl`, [`present`]'s: a uniform, the picture, and a
    /// sampler if `sampled`.
    fn new(device: &wgpu::Device, label: &str, wesl: &str, sampled: Sampled) -> Pass {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(plain(wesl).into()),
        });
        let uniform = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let picture = wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let filter = wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let entries = match sampled {
            Sampled::Yes => vec![uniform, picture, filter],
            Sampled::No => vec![uniform, picture],
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Pass { pipeline, layout }
    }

    /// The pass over all of `target`: `uniform`, `picture` and its
    /// sampler bound.
    fn run(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (uniform, picture, sampler): (&[u8], &wgpu::TextureView, Option<&wgpu::Sampler>),
        target: &wgpu::TextureView,
    ) {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fsr"),
            contents: uniform,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(picture),
            },
        ];

        entries.extend(sampler.map(|s| wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::Sampler(s),
        }));

        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fsr"),
            layout: &self.layout,
            entries: &entries,
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..default()
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::Color32;

    #[test]
    fn a_picture_is_upscaled_only_where_it_shows_larger_and_fits() {
        let video = UVec2::new(640, 480);

        assert_eq!(
            fitted(video, UVec2::new(1600, 1200), 8192),
            Some(UVec2::new(1600, 1200))
        );
        assert_eq!(fitted(video, video, 8192), None, "1:1");
        assert_eq!(fitted(video, UVec2::new(1600, 480), 8192), None);
        assert_eq!(fitted(video, UVec2::new(320, 240), 8192), None);
        assert_eq!(fitted(video, UVec2::new(9000, 6750), 8192), None);
    }

    #[test]
    fn the_shaders_lose_their_import_and_material_group_and_stay_valid() {
        for wesl in [
            include_str!("../shaders/easu.wesl"),
            include_str!("../shaders/rcas.wesl"),
        ] {
            let wgsl = plain(wesl);

            assert!(!wgsl.contains("import "));
            assert!(!wgsl.contains("constants::"));
            assert!(wgsl.contains("@group(0) @binding(1) var picture"));
            crate::compile::wgsl_valid(&wgsl).unwrap();
        }
    }

    /// `picture` upscaled to `size` on this machine's GPU, read back.
    fn on_gpu(picture: &ColorImage, size: UVec2) -> Vec<Color32> {
        let (_, device, queue) = crate::art::gpu::test_gpu();
        let usage = TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
        let target = texture(device, extent(size), usage);
        let textures = Textures::new(device, picture, extent(size));
        // Rows padded as a copy to a buffer wants.
        let width = size.x as usize;
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * size.y as usize) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        Fsr::new(device).encode(device, queue, &mut encoder, (picture, &textures, &target));
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: u32::try_from(row).ok(),
                    rows_per_image: None,
                },
            },
            extent(size),
        );
        queue.submit([encoder.finish()]);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the copy");

        let bytes = read.slice(..).get_mapped_range().expect("the mapping");

        bytes
            .chunks(row)
            .flat_map(|r| bytemuck::cast_slice::<u8, Color32>(&r[..width * 4]).to_vec())
            .collect()
    }

    /// One colour stays that colour, opaque, at every pixel: the passes
    /// give back the sRGB they were given.
    #[test]
    fn a_flat_picture_upscales_to_the_same_colour() {
        let colour = Color32::from_rgb(200, 90, 30);
        let picture = ColorImage::new([64, 48], vec![colour; 64 * 48]);
        let shown = on_gpu(&picture, UVec2::new(160, 120));
        let off = shown
            .iter()
            .filter(|c| {
                let (a, b) = (c.to_array(), colour.to_array());

                (0..4).any(|i| a[i].abs_diff(b[i]) > 1)
            })
            .count();

        assert_eq!(shown.len(), 160 * 120);
        assert_eq!(off, 0);
    }

    /// A hard vertical edge stays where it was, scaled: pixel middles
    /// where `FsrEasuCon` puts them.
    #[test]
    fn an_edge_stays_in_place() {
        let (dark, light) = (Color32::from_gray(20), Color32::from_gray(230));
        let picture = ColorImage::new(
            [32, 8],
            (0..32 * 8)
                .map(|i| if i % 32 < 16 { dark } else { light })
                .collect(),
        );
        let shown = on_gpu(&picture, UVec2::new(80, 20));
        let row = &shown[10 * 80..11 * 80];
        let grey = |c: &Color32| c.to_array()[0];

        assert!(row[..36].iter().all(|c| grey(c) < 60), "{:?}", &row[30..50]);
        assert!(
            row[44..].iter().all(|c| grey(c) > 190),
            "{:?}",
            &row[30..50]
        );
    }
}
