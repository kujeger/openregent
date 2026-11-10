//! egui's texture bind groups, kept from frame to frame. `bevy_egui`'s
//! `queue_bind_groups_system` makes them anew every frame, for every
//! registered texture: with the UI's art, a few percent of a drawn
//! frame's CPU. This system takes its place: each texture keeps a place
//! in a chunk (`bevy_egui`'s bindless array size, or one without) while it
//! lives, and a chunk whose textures stay the same keeps its bind group.
//!
//! ```text
//!  Render, Queue: egui's textures
//!    gone: its place freed     new: the lowest free place
//!    each chunk's views and samplers ── as before? ──> its bind group again
//!                                     └─ no ─────────> made
//! ```
//!
//! What egui draws is the same: each texture is drawn from its own view
//! and sampler, wherever it sits.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZero;

use bevy::ecs::schedule::ScheduleCleanupPolicy;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    BindGroup, BindGroupEntry, BindGroupLayout, BindingResource, PipelineCache, SamplerId,
    TextureViewId,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy_egui::render::EguiPipeline;
use bevy_egui::render::systems::{
    EguiTextureBindGroups, EguiTextureId, ExtractedEguiTextures, queue_bind_groups_system,
};

/// A texture, as a key: egui's own by window and id, or a user one's id.
type Key = (u8, u64, u64);

/// A chunk's textures: each one's view and sampler, in place order.
type Chunk = Vec<(TextureViewId, SamplerId)>;

/// Each texture's place, and each chunk's bind group as last made.
#[derive(Default)]
struct Kept {
    /// Each texture's place: chunk `place / size`.
    places: HashMap<Key, usize>,
    /// Places freed, lowest first.
    free: BTreeSet<usize>,
    /// Each chunk's textures and bind group.
    chunks: HashMap<usize, (Chunk, BindGroup)>,
}

/// Puts `queue` in place of `bevy_egui`'s system: after `EguiPlugin`,
/// whose `finish` adds it.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, _: &mut App) {}

    fn finish(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            let policy = ScheduleCleanupPolicy::RemoveSystemsOnly;

            // Only in its place: without either, egui draws nothing.
            match render.remove_systems_in_set(Render, queue_bind_groups_system, policy) {
                Ok(removed) if removed > 0 => {
                    render.add_systems(Render, queue.in_set(RenderSystems::Queue));
                }
                found => warn!("art: bevy_egui's bind groups kept as they are: {found:?}"),
            }
        }
    }
}

/// Each egui texture's bind group, as `queue_bind_groups_system` makes
/// it, kept while its chunk's textures stay the same.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn queue(
    mut commands: Commands,
    textures: ExtractedEguiTextures,
    device: Res<RenderDevice>,
    images: Res<RenderAssets<GpuImage>>,
    cache: Res<PipelineCache>,
    pipeline: Res<EguiPipeline>,
    mut kept: Local<Kept>,
) {
    let layout = cache.get_bind_group_layout(&pipeline.texture_bind_group_layout);
    let size = pipeline
        .bindless
        .and_then(|n| usize::try_from(n.get()).ok())
        .unwrap_or(1);
    let live: Vec<_> = textures
        .handles()
        .filter_map(|(texture, id)| Some((key(&texture), texture, images.get(id)?)))
        .collect();
    let mut chunks: BTreeMap<usize, Vec<_>> = BTreeMap::new();

    kept.place(live.iter().map(|(key, ..)| *key));

    for (key, texture, image) in live {
        let place = kept.places[&key];

        chunks
            .entry(place / size)
            .or_default()
            .push((place, texture, image));
    }

    let mut groups = HashMap::new();
    let mut made = HashMap::new();

    for (at, mut chunk) in chunks {
        chunk.sort_unstable_by_key(|(place, ..)| *place);

        let now: Chunk = chunk
            .iter()
            .map(|(_, _, image)| (image.texture_view.id(), image.sampler.id()))
            .collect();
        let group = match kept.chunks.remove(&at) {
            Some((was, group)) if was == now => group,
            _ => {
                let images: Vec<&GpuImage> = chunk.iter().map(|(_, _, image)| *image).collect();

                bind(&device, &layout, &images, pipeline.bindless)
            }
        };

        // Bindless, each texture is its place in the array.
        for (offset, (_, texture, _)) in (0u32..).zip(chunk) {
            let offset = pipeline.bindless.map(|_| offset);

            groups.insert(texture, (group.clone(), offset));
        }

        made.insert(at, (now, group));
    }

    kept.chunks = made;
    commands.insert_resource(EguiTextureBindGroups(groups));
}

impl Kept {
    /// Places for textures `live`: those gone freed, each new one given
    /// the lowest free.
    fn place(&mut self, live: impl Iterator<Item = Key>) {
        let live: HashSet<Key> = live.collect();
        let free = &mut self.free;

        self.places.retain(|key, place| {
            let stays = live.contains(key);

            if !stays {
                free.insert(*place);
            }

            stays
        });

        for key in live {
            if !self.places.contains_key(&key) {
                let next = self.places.len() + self.free.len();
                let place = self.free.pop_first().unwrap_or(next);

                self.places.insert(key, place);
            }
        }
    }
}

/// Texture `texture` as a key.
fn key(texture: &EguiTextureId) -> Key {
    match texture {
        EguiTextureId::Managed(window, id) => (0, window.id().to_bits(), *id),
        EguiTextureId::User(id) => (1, 0, *id),
    }
}

/// One bind group for `images`: an array of each, bindless; else the one
/// image's view and sampler.
fn bind(
    device: &RenderDevice,
    layout: &BindGroupLayout,
    images: &[&GpuImage],
    bindless: Option<NonZero<u32>>,
) -> BindGroup {
    let views: Vec<_> = images.iter().map(|i| &*i.texture_view).collect();
    let samplers: Vec<_> = images.iter().map(|i| &*i.sampler).collect();
    let (view, sampler) = match (bindless, views.first(), samplers.first()) {
        (None, Some(view), Some(sampler)) => (
            BindingResource::TextureView(view),
            BindingResource::Sampler(sampler),
        ),
        _ => (
            BindingResource::TextureViewArray(&views),
            BindingResource::SamplerArray(&samplers),
        ),
    };

    device.create_bind_group(
        None,
        layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: view,
            },
            BindGroupEntry {
                binding: 1,
                resource: sampler,
            },
        ],
    )
}
