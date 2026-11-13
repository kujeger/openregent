//! Pictures shrunk on the GPU: an upscaled picture's part area-shrunk
//! to the window pixels it covers, as `shrink::part` does on the CPU,
//! drawn into an egui texture the frame it is asked.
//!
//! ```text
//!  lend (PreUpdate, PostUpdate)    egui textures lent ahead: a reserved
//!    │                             image and its id ([`Gpu`])
//!  Art::get (egui's pass)          takes one: the big picture, its part,
//!    │                             the size ([`Gpu::shrunk`])
//!  lend (PostUpdate)               the big picture and the target as
//!    │                             images; the asks sent on
//!  draw (render world, Queue)      one pass an ask: the target drawn
//!                                  from the big picture, before egui's
//! ```
//!
//! The big picture lives one frame; the target lives while its entry is
//! held, freed by egui when the image goes. A picture not shrunk (a plain
//! one, one shrunk on the CPU) is uploaded to a lent texture as it is
//! ([`Gpu::uploaded`]): no copy stays in RAM, where `bevy_egui` keeps two of
//! each egui texture (its `ColorImage` and a main-world `Image`). Without
//! a GPU (tests, the headless runs) none is lent: the CPU shrinks, and
//! egui takes the textures.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems};
use bevy_egui::egui::{ColorImage, Rect, TextureId};
use bevy_egui::{EguiPostUpdateSet, EguiTextureHandle, EguiUserTextures};
use wgpu::util::DeviceExt;

use super::Art;
use super::scale::Edge;
use super::side::Side;

/// Textures kept lent: more than a page's first pass makes (the main
/// menu's, about 50 at 3840 × 2160). Past them a picture is shrunk on
/// the CPU, and egui takes it.
const POOL: usize = 256;

/// The big picture's format: read as stored.
const SOURCE: TextureFormat = TextureFormat::Rgba8Unorm;

/// The target's format, as egui's own textures: sampled as sRGB.
const TARGET: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// The target as drawn into: written as stored.
const DRAWN: TextureFormat = TextureFormat::Rgba8Unorm;

/// An egui texture to draw into: an image reserved, not yet made.
pub struct Lent {
    handle: Handle<Image>,
    id: TextureId,
}

/// A texture shrunk on the GPU: held while its entry is.
pub struct Drawn {
    /// Its image: dropped, egui frees the id.
    _handle: Handle<Image>,
    pub id: TextureId,
    pub size: [usize; 2],
}

/// One texture asked: `target`, `size` pixels, filled as `how` says.
struct Ask {
    how: How,
    size: [usize; 2],
    edge: Edge,
    target: Handle<Image>,
}

/// How a lent texture is filled.
enum How {
    /// `big`'s part `cut` (its pixels) shrunk into it.
    Shrunk { big: ColorImage, cut: Rect },
    /// `image` as it is, uploaded: unlike egui's own textures, of which
    /// `bevy_egui` keeps two copies in RAM, none is kept.
    Uploaded(ColorImage),
}

impl How {
    /// The picture it reads.
    const fn image(&self) -> &ColorImage {
        match self {
            How::Shrunk { big, .. } => big,
            How::Uploaded(image) => image,
        }
    }

    fn into_image(self) -> ColorImage {
        match self {
            How::Shrunk { big, .. } => big,
            How::Uploaded(image) => image,
        }
    }
}

/// The art service's GPU side: textures lent, shrinks asked.
pub struct Gpu {
    free: Vec<Lent>,
    asked: Vec<Ask>,
    /// The GPU's largest texture side.
    side: usize,
}

impl Gpu {
    /// `big`'s part `cut` (its pixels) shrunk to `size` this frame, its
    /// edges as `edge`; `big` back with none lent, or past the GPU's side.
    ///
    /// # Errors
    ///
    /// `big`, to shrink on the CPU.
    pub fn shrunk(
        &mut self,
        big: ColorImage,
        cut: Rect,
        (size, edge): ([usize; 2], Edge),
    ) -> Result<Drawn, ColorImage> {
        self.ask(How::Shrunk { big, cut }, (size, edge))
            .map_err(How::into_image)
    }

    /// `image` uploaded as it is this frame, its edges as `edge`: no copy
    /// kept in RAM; `image` back with none lent, or past the GPU's side.
    ///
    /// # Errors
    ///
    /// `image`, for egui to take.
    pub fn uploaded(&mut self, image: ColorImage, edge: Edge) -> Result<Drawn, ColorImage> {
        let size = image.size;

        self.ask(How::Uploaded(image), (size, edge))
            .map_err(How::into_image)
    }

    /// A lent texture `size` pixels, filled as `how` says this frame.
    fn ask(&mut self, how: How, (size, edge): ([usize; 2], Edge)) -> Result<Drawn, How> {
        let fits = how
            .image()
            .size
            .iter()
            .chain(&size)
            .all(|&n| n <= self.side);

        match self.free.pop() {
            Some(lent) if fits => {
                self.asked.push(Ask {
                    how,
                    size,
                    edge,
                    target: lent.handle.clone(),
                });

                Ok(Drawn {
                    _handle: lent.handle,
                    id: lent.id,
                    size,
                })
            }
            lent => {
                self.free.extend(lent);
                Err(how)
            }
        }
    }
}

/// The asks of a frame, as the render world takes them; their big
/// pictures held to the next.
#[derive(Resource, Default)]
struct Sent(Vec<(Handle<Image>, Job)>);

/// One pass: `source`'s part `cut` into `target`.
#[derive(Clone, Copy)]
struct Job {
    source: AssetId<Image>,
    target: AssetId<Image>,
    cut: Rect,
}

/// The render world's jobs this frame.
#[derive(Resource, Default)]
struct Jobs(Vec<Job>);

/// Lends, sends and draws.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        // Lent before the first pass, sent after each.
        app.init_resource::<Sent>()
            .add_plugins(super::binds::Plugin)
            .add_systems(PreUpdate, lend)
            .add_systems(PostUpdate, lend.after(EguiPostUpdateSet::EndPass));
    }

    fn finish(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<Jobs>()
                .add_systems(RenderStartup, init)
                .add_systems(ExtractSchedule, extract)
                .add_systems(Render, draw.in_set(RenderSystems::Queue));
        }
    }
}

/// The art service's asks made images and sent; its lent textures
/// topped up to [`POOL`]. None where there is no GPU.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn lend(
    art: Option<ResMut<Art>>,
    side: Option<Res<Side>>,
    (mut images, textures): (ResMut<Assets<Image>>, Option<ResMut<EguiUserTextures>>),
    mut sent: ResMut<Sent>,
) {
    // `textures` taken mutably only to add: else egui's user textures
    // read as changed each frame, and `kept` draws all of egui again.
    if let (Some(mut art), Some(side), Some(mut textures)) = (art, side, textures) {
        let gpu = art.gpu();

        gpu.side = side.0;
        sent.0 = gpu
            .asked
            .drain(..)
            .filter_map(|ask| {
                // A shrink is a job for the render world; an upload is
                // the image alone.
                let (image, sent) = match ask.how {
                    How::Shrunk { big, cut } => {
                        let source = images.add(source(&big));
                        let job = Job {
                            source: source.id(),
                            target: ask.target.id(),
                            cut,
                        };

                        (target(ask.size, ask.edge), Some((source, job)))
                    }
                    How::Uploaded(image) => (uploaded(&image, ask.edge), None),
                };

                if let Err(e) = images.insert(&ask.target, image) {
                    warn!("art: a lent texture: {e}");
                }

                sent
            })
            .collect();

        while gpu.free.len() < POOL {
            let handle = images.reserve_handle();
            // Weak: the entry holds the image, and egui frees the id when
            // it goes.
            let id = textures.add_image(EguiTextureHandle::Weak(handle.id()));

            gpu.free.push(Lent { handle, id });
        }
    }
}

impl Art {
    /// The GPU side, made on first use.
    fn gpu(&mut self) -> &mut Gpu {
        self.gpu.get_or_insert_with(|| Gpu {
            free: Vec::new(),
            asked: Vec::new(),
            side: 0,
        })
    }
}

/// `big` as an image to read from, on the GPU alone.
fn source(big: &ColorImage) -> Image {
    let bytes = bytemuck::cast_slice(&big.pixels).to_vec();

    Image::new(
        extent(big.size),
        TextureDimension::D2,
        bytes,
        SOURCE,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// An image `size` pixels to draw into, sampled as egui's own: linear,
/// its edges as `edge`.
fn target(size: [usize; 2], edge: Edge) -> Image {
    let mut image = Image::new_uninit(
        extent(size),
        TextureDimension::D2,
        TARGET,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    image.texture_descriptor.view_formats = &[DRAWN];
    image.sampler = sampler(edge);

    image
}

/// `image` as a texture, on the GPU alone, sampled as egui's own: the
/// bytes egui would upload (`bevy_egui`'s `color_image_as_bevy_image`).
fn uploaded(image: &ColorImage, edge: Edge) -> Image {
    let bytes = bytemuck::cast_slice(&image.pixels).to_vec();
    let mut image = Image::new(
        extent(image.size),
        TextureDimension::D2,
        bytes,
        TARGET,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.sampler = sampler(edge);
    image
}

/// egui's own sampling: linear, its edges as `edge`.
fn sampler(edge: Edge) -> ImageSampler {
    let address = match edge {
        Edge::Clamp => ImageAddressMode::ClampToEdge,
        Edge::Repeat => ImageAddressMode::Repeat,
    };

    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address,
        address_mode_v: address,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    })
}

/// `size` as a texture's extent.
fn extent(size: [usize; 2]) -> Extent3d {
    let pixels = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);

    Extent3d {
        width: pixels(size[0]),
        height: pixels(size[1]),
        depth_or_array_layers: 1,
    }
}

/// The frame's jobs, to the render world.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn extract(mut commands: Commands, sent: Extract<Res<Sent>>) {
    commands.insert_resource(Jobs(sent.0.iter().map(|(_, job)| *job).collect()));
}

/// The pipeline, made at once: no frame waits on it.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn init(mut commands: Commands, device: Res<RenderDevice>) {
    commands.insert_resource(Shrinker::new(device.wgpu_device()));
}

/// The frame's jobs drawn, submitted before the frame's own passes.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn draw(
    jobs: Res<Jobs>,
    images: Res<RenderAssets<GpuImage>>,
    (device, queue): (Res<RenderDevice>, Res<RenderQueue>),
    shrinker: Option<Res<Shrinker>>,
) {
    if let Some(shrinker) = shrinker.filter(|_| !jobs.0.is_empty()) {
        let device = device.wgpu_device();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("art shrink"),
        });

        for job in &jobs.0 {
            if let (Some(source), Some(target)) = (images.get(job.source), images.get(job.target)) {
                let textures = (&*source.texture, &*target.texture);

                shrinker.encode(device, &mut encoder, textures, job.cut);
            } else {
                warn!("art: a shrink's texture is not on the GPU");
            }
        }

        queue.submit([encoder.finish()]);
    }
}

/// The shrink's pipeline.
#[derive(Resource)]
pub struct Shrinker {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

/// The shader's uniform: the part's corner and the source pixels a new
/// pixel covers.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Part {
    start: [f32; 2],
    step: [f32; 2],
}

impl Shrinker {
    /// The pipeline on `device`.
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Shrinker {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("art shrink"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shrink.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("art shrink"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("art shrink"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("art shrink"),
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
                    format: DRAWN,
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

        Shrinker { pipeline, layout }
    }

    /// `source`'s part `cut` (its pixels) drawn into all of `target`.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (source, target): (&wgpu::Texture, &wgpu::Texture),
        cut: Rect,
    ) {
        #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
        let size = [target.width() as f32, target.height() as f32];
        let part = Part {
            start: [cut.min.x, cut.min.y],
            step: [cut.width() / size[0], cut.height() / size[1]],
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("art shrink"),
            contents: bytemuck::bytes_of(&part),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let read = source.create_view(&wgpu::TextureViewDescriptor {
            format: Some(SOURCE),
            ..default()
        });
        let drawn = target.create_view(&wgpu::TextureViewDescriptor {
            format: Some(DRAWN),
            ..default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("art shrink"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&read),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("art shrink"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &drawn,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
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

/// This machine's GPU for the tests: one instance and device a process,
/// never dropped. The Vulkan loader's `SetDebugUtilsObjectNameEXT` walks its
/// instance list unlocked (`loader_get_icd_and_device`), so a test making an
/// instance while another names an object crashed the run with SIGSEGV,
/// about one run in thirty.
#[cfg(test)]
pub(crate) fn test_gpu() -> &'static (wgpu::Instance, wgpu::Device, wgpu::Queue) {
    static GPU: std::sync::LazyLock<(wgpu::Instance, wgpu::Device, wgpu::Queue)> =
        std::sync::LazyLock::new(|| {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = bevy::tasks::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            )
            .expect("a GPU");
            let (device, queue) =
                bevy::tasks::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                    .expect("a device");

            (instance, device, queue)
        });

    &GPU
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::shrink;
    use bevy_egui::egui::{Color32, pos2};

    /// A picture of `size` noisy, part-transparent pixels, premultiplied.
    fn noise(size: [usize; 2]) -> ColorImage {
        let mut seed = 11_u32;
        let pixels = (0..size[0] * size[1])
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let [r, g, b, a] = seed.to_le_bytes();

                Color32::from_rgba_unmultiplied(r, g, b, a | 0x40)
            })
            .collect();

        ColorImage::new(size, pixels)
    }

    /// `big`'s part `cut` shrunk to `size` on this machine's GPU, as
    /// the game draws it, read back.
    fn on_gpu(big: &ColorImage, cut: Rect, size: [usize; 2]) -> Vec<Color32> {
        let (_, device, queue) = test_gpu();
        let texture = |size: [usize; 2], format, usage, view_formats| wgpu::TextureDescriptor {
            label: None,
            size: extent(size),
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage,
            view_formats,
        };
        let source = device.create_texture_with_data(
            queue,
            &texture(big.size, SOURCE, TextureUsages::TEXTURE_BINDING, &[]),
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&big.pixels),
        );
        let usage = TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
        let target = device.create_texture(&texture(size, TARGET, usage, &[DRAWN]));
        // Rows padded as a copy to a buffer wants.
        let row = (size[0] * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * size[1]) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        Shrinker::new(device).encode(device, &mut encoder, (&source, &target), cut);
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
            .flat_map(|r| bytemuck::cast_slice::<u8, Color32>(&r[..size[0] * 4]).to_vec())
            .collect()
    }

    /// The GPU's shrink is the CPU's (`shrink::part`), within a step of
    /// rounding: whole, cut at fractions as laid over another
    /// (`Snap::Over`), and nearly unshrunk.
    #[test]
    fn the_gpu_shrinks_as_the_cpu_does() {
        let big = noise([97, 61]);
        let whole = shrink::whole(&big);

        for (cut, size) in [
            (whole, [53, 40]),
            (whole, [96, 60]),
            (
                Rect::from_min_max(pos2(3.25, 7.5), pos2(80.75, 59.0)),
                [41, 29],
            ),
            (
                Rect::from_min_max(pos2(0.5, 0.5), pos2(96.5, 60.5)),
                [70, 45],
            ),
        ] {
            let cpu = shrink::part(&big, cut, size);
            let gpu = on_gpu(&big, cut, size);
            let off = cpu
                .pixels
                .iter()
                .zip(&gpu)
                .filter(|(a, b)| {
                    let (a, b) = (a.to_array(), b.to_array());

                    (0..4).any(|c| a[c].abs_diff(b[c]) > 1)
                })
                .count();

            assert_eq!(gpu.len(), cpu.pixels.len());
            assert_eq!(off, 0, "{cut:?} to {size:?}");
        }
    }
}
