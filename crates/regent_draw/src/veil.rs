//! The fog's veil: black over unexplored ground ([`regent_core::view::map::fog`]).
//! openregent decides what the player sees and when a chunk is redrawn
//! (`map::fog`); this is the material, its shader and the chunks' meshes.
//!
//! ```text
//!  Explored ──> changed: the chunks a reveal touches
//!           └─> mesh: a chunk's fogged cells, Gouraud from the corners
//!                 └─> Veil (veil.wesl): black by 1 − the corners' share
//! ```

use std::collections::HashSet;

use bevy::asset::{AssetPath, RenderAssetUsages, embedded_asset, embedded_path};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor,
    SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use regent_core::sim::world::reveal::Explored;
use regent_core::view::map::cut::Cut;
use regent_core::view::map::fog::{self, UNSEEN_SHADE};

use crate::map::Maps;
use crate::map::depth::FURTHEST;
use crate::plain;

/// Cells a side of one veil mesh: a reveal redraws only its chunks.
pub const CHUNK: i32 = 16;

/// Where the veil draws: over the ground (−950), under every object.
pub const DEPTH: f32 = FURTHEST - 25.0;

/// Registers the veil's material.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/veil.wesl");

        app.add_plugins(MaterialPlugin::<Veil>::default());
    }
}

/// The veil: black where the ground is unexplored. The blend keeps the
/// shadow's coverage in alpha.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct Veil {}

impl Material for Veil {
    fn vertex_shader() -> ShaderRef {
        Veil::fragment_shader()
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/veil.wesl")).with_source("embedded"),
        )
    }

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
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        Veil::shape(descriptor);

        Ok(())
    }
}

impl Veil {
    /// [`Material::specialize`]'s work, without Bevy's pipeline: the
    /// compile tests call it.
    pub fn shape(descriptor: &mut RenderPipelineDescriptor) {
        plain::unculled(descriptor);

        let keep = BlendComponent {
            src_factor: BlendFactor::Zero,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        };

        if let Ok(fragment) = descriptor.fragment_mut() {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState {
                    color: BlendState::ALPHA_BLENDING.color,
                    alpha: keep,
                });
            }
        }
    }
}

/// Every chunk of a map `side` cells across.
pub fn chunks(side: i32) -> impl Iterator<Item = (i32, i32)> {
    let across = (side + CHUNK - 1) / CHUNK;

    (0..across).flat_map(move |y| (0..across).map(move |x| (x, y)))
}

/// The chunks holding a cell whose corner changed from `was` to `now`,
/// as `mask` sees them.
#[must_use]
pub fn changed(was: &Explored, now: &Explored, mask: u8) -> HashSet<(i32, i32)> {
    let cells = now.cells();
    let bit = |flags: u8| fog::explored(flags, mask);

    vertices(was, now, |a, b| bit(a) != bit(b))
        .flat_map(corners)
        .filter(|(x, y)| (0..cells).contains(x) && (0..cells).contains(y))
        .map(|(x, y)| (x / CHUNK, y / CHUNK))
        .collect()
}

/// Each vertex whose flags went from `was` to `now` as `differ` tells:
/// runs of flags still shared, most of them on a reveal, passed over
/// unread ([`Explored::differing`]).
pub fn vertices<'a>(
    was: &'a Explored,
    now: &'a Explored,
    differ: impl Fn(u8, u8) -> bool + 'a,
) -> impl Iterator<Item = (i32, i32)> + 'a {
    let across = now.cells() + 1;

    now.differing(was)
        .flat_map(|(first, a, b)| a.iter().zip(b.iter()).zip(first..))
        .filter(move |((a, b), _)| differ(**a, **b))
        .filter_map(|(_, i)| i32::try_from(i).ok())
        .map(move |i| (i % across, i / across))
}

/// The four cells vertex `(x, y)` is a corner of.
#[must_use]
pub const fn corners((x, y): (i32, i32)) -> [(i32, i32); 4] {
    [(x - 1, y - 1), (x, y - 1), (x - 1, y), (x, y)]
}

/// Chunk `at`'s veil: each cell with a corner unexplored by `mask`, two
/// triangles cut as the ground's, each corner's share of its shade in its
/// colour. `None` where every corner is explored.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "a map is at most 512 cells"
)]
#[must_use]
pub fn mesh(maps: &Maps, explored: &Explored, at: (i32, i32), mask: u8) -> Option<Mesh> {
    let side = explored.cells();
    let across = side + 1;
    let (x0, y0) = (at.0 * CHUNK, at.1 * CHUNK);
    let (x1, y1) = ((x0 + CHUNK).min(side), (y0 + CHUNK).min(side));
    let lit = |x: i32, y: i32| {
        explored
            .flag((y * across + x) as usize)
            .is_some_and(|f| fog::explored(f, mask))
    };
    let fogged: Vec<(i32, i32)> = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|(x, y)| !(lit(*x, *y) && lit(x + 1, *y) && lit(*x, y + 1) && lit(x + 1, y + 1)))
        .collect();

    // The projection is affine: a point lifted by its height.
    let origin = maps.aloft_at(Vec2::ZERO, 0.0);
    let east = maps.aloft_at(Vec2::X, 0.0) - origin;
    let south = maps.aloft_at(Vec2::Y, 0.0) - origin;
    let up = maps.aloft_at(Vec2::ZERO, 1.0) - origin;
    let terrain = maps.terrain();
    let wide = x1 - x0 + 1;

    let (positions, colours): (Vec<[f32; 3]>, Vec<[f32; 4]>) = (y0..=y1)
        .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
        .map(|(x, y)| {
            let height = terrain
                .as_ref()
                .and_then(|t| t.vertex(x as u32, y as u32))
                .map_or(0.0, |v| v.height);
            let p = origin + east * x as f32 + south * y as f32 + up * height;
            let kept = if lit(x, y) { 1.0 } else { UNSEEN_SHADE };

            ([p.x, p.y, 0.0], [kept, kept, kept, 1.0])
        })
        .unzip();

    let corner = |x: i32, y: i32| ((y - y0) * wide + (x - x0)) as u32;
    let indices: Vec<u32> = fogged
        .iter()
        .flat_map(|&(x, y)| {
            let (tl, tr) = (corner(x, y), corner(x + 1, y));
            let (bl, br) = (corner(x, y + 1), corner(x + 1, y + 1));

            // The ground's cut.
            Cut::of(x.cast_unsigned(), y.cast_unsigned()).indices([tl, tr, bl, br])
        })
        .collect();

    (!fogged.is_empty()).then(|| {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Revealing vertex (16, 3) touches cells (15, 2) to (16, 3): chunks
    /// (0, 0) and (1, 0).
    #[test]
    fn a_revealed_vertex_redraws_the_chunks_of_its_four_cells() {
        let was = Explored::blank(64);
        let mut now = was.clone();

        now.disc((16, 3), 1, 1);

        let mut dirty: Vec<(i32, i32)> =
            changed(&was, &now, fog::mask(regent_core::sim::world::CROWN))
                .into_iter()
                .collect();

        dirty.sort_unstable();
        assert_eq!(dirty, [(0, 0), (1, 0)]);
    }

    #[test]
    fn the_chunks_cover_the_map() {
        assert_eq!(chunks(128).count(), 64);
        assert_eq!(chunks(20).count(), 4);
    }
}
