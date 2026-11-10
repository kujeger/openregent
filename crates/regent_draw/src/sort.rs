//! `Transparent3d`'s and `Transparent2d`'s mesh items keyed by where
//! they are each frame.
//!
//! Bevy 0.20 keeps a 3D mesh's transparent item, its mesh's centre in
//! the world, from when it queues it: on a material or mesh change
//! (`queue_material_meshes`). Each frame it measures that stale centre's
//! distance again (`recalculate_sort_keys`). A glass sprite or a band
//! that moves keeps its old place in the order, so it can draw under one
//! it has passed. 2D keeps the queue's z: the shown camera's hit point
//! bars ([`rekey_2d`]).
//!
//! ```text
//!  queued      A z 1    B z 2       order A, B
//!  A moves     A z 3    B z 2       centres stale: still A, B
//!  rekeyed     A 3      B 2         order B, A
//! ```

use bevy::core_pipeline::core_2d::Transparent2d;
use bevy::core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d};
use bevy::math::FloatOrd;
use bevy::pbr::{
    MeshInputUniform, MeshUniform, RenderMeshInstances, get_mesh_instance_world_from_local,
};
use bevy::prelude::*;
use bevy::render::batching::gpu_preprocessing::BatchedInstanceBuffers;
use bevy::render::erased_render_asset::ErasedRenderAssets;
use bevy::render::mesh::RenderMesh;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::sync_world::MainEntity;
use bevy::sprite_render::{PreparedMaterial2d, RenderMaterial2dInstances, RenderMesh2dInstances};

/// Each mesh item's centre as Bevy's queue takes it, from this frame's
/// transform; its depth bias kept. Items of other kinds are left alone.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub(crate) fn rekey(
    instances: Res<RenderMeshInstances>,
    buffers: Option<Res<BatchedInstanceBuffers<MeshUniform, MeshInputUniform>>>,
    meshes: Res<RenderAssets<RenderMesh>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
) {
    let centre = |entity: MainEntity| {
        let queued = instances.render_mesh_queue_data(entity)?;
        let mesh = meshes.get(queued.mesh_asset_id())?;
        let world_from_local = get_mesh_instance_world_from_local(
            entity,
            queued.current_uniform_index,
            &instances,
            buffers.as_deref(),
        );

        Some(world_from_local.transform_point3(mesh.aabb_center))
    };

    for phase in phases.values_mut() {
        recentre(phase.items.values_mut(), centre);
    }
}

/// Each `Transparent2d` mesh item keyed by its z this frame, its
/// material's depth bias kept, as `queue_material2d_meshes` keys it; items
/// of other kinds are left alone. Bevy 0.20 keeps the queue's z (its
/// `recalculate_sort_keys` does nothing), and queues a part as it comes
/// in view. A bar's frame, in view before its short fill, kept its key
/// while the bar's z dropped (`hp_bars` ranks every bar on the map); the
/// fill, queued later, sorted under it: a black bar till both queued again
/// (off the screen and back). Two bars' parts interleaved likewise
/// (`tests/bar_order.rs`).
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub(crate) fn rekey_2d(
    instances: Res<RenderMesh2dInstances>,
    materials: Res<RenderMaterial2dInstances>,
    prepared: Res<ErasedRenderAssets<PreparedMaterial2d>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent2d>>,
) {
    let key = |entity: MainEntity| {
        let z = instances
            .get(&entity)?
            .transforms
            .world_from_local
            .translation
            .z;
        let bias = prepared
            .get(*materials.get(&entity)?)?
            .properties
            .depth_bias;

        Some(FloatOrd(z + bias))
    };

    for phase in phases.values_mut() {
        rezed(phase.items.values_mut(), key);
    }
}

/// Each of `items`' keys as `key` gives it now.
fn rezed<'a>(
    items: impl Iterator<Item = &'a mut Transparent2d>,
    key: impl Fn(MainEntity) -> Option<FloatOrd>,
) {
    for item in items {
        if let Some(now) = key(item.entity.1) {
            item.sort_key = now;
        }
    }
}

/// Each of `items`' centres as `centre` gives it now.
fn recentre<'a>(
    items: impl Iterator<Item = &'a mut Transparent3d>,
    centre: impl Fn(MainEntity) -> Option<Vec3>,
) {
    for item in items {
        if let TransparentSortingInfo3d::Sorted { mesh_center, .. } = &mut item.sorting_info
            && let Some(now) = centre(item.entity.1)
        {
            *mesh_center = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::render::render_phase::{
        DrawFunctionId, PhaseItemExtraIndex, SortedPhaseItem, ViewRangefinder3d,
    };
    use bevy::render::render_resource::CachedRenderPipelineId;

    use super::*;

    /// A glass sprite queued behind another, then moved in front of it
    /// with no material change, draws after it.
    #[test]
    fn a_moving_glass_sprite_passing_another_draws_in_front() {
        let mut world = World::new();
        let (moving, still) = (
            MainEntity::from(world.spawn_empty().id()),
            MainEntity::from(world.spawn_empty().id()),
        );
        // As `queue_material_meshes` keys it: the centre then, no bias.
        let item = |entity: MainEntity, z: f32| Transparent3d {
            sorting_info: TransparentSortingInfo3d::Sorted {
                mesh_center: Vec3::new(0.0, 0.0, z),
                depth_bias: 0.0,
            },
            distance: 0.0,
            entity: (Entity::PLACEHOLDER, entity),
            pipeline: CachedRenderPipelineId::INVALID,
            draw_function: DrawFunctionId(0),
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            indexed: true,
        };
        let mut items = vec![item(moving, 1.0), item(still, 2.0)];

        // Moved in front, as extraction leaves it: the item not requeued.
        recentre(items.iter_mut(), |entity| {
            (entity == moving).then_some(Vec3::new(0.0, 0.0, 3.0))
        });

        // An unturned camera at the origin, as the world camera; then the
        // phase's own measure and order.
        let rangefinder = ViewRangefinder3d::from_world_from_view(&bevy::math::Affine3A::IDENTITY);

        for item in &mut items {
            item.distance = item.sorting_info.sort_distance(&rangefinder);
        }

        items.sort_by_key(SortedPhaseItem::sort_key);

        let order: Vec<MainEntity> = items.iter().map(|item| item.entity.1).collect();

        assert_eq!(order, [still, moving]);
    }

    /// Two hit point bars, frame and fill each, where they overlap. A's
    /// queued first at z 0; a bar ordered before it comes, so A moves a
    /// bar's step nearer, and B is queued where A was. A, the nearer,
    /// draws over B, frame and fill.
    #[test]
    fn a_bar_moved_nearer_draws_over_one_queued_where_it_was() {
        let mut world = World::new();
        let mut part = || MainEntity::from(world.spawn_empty().id());
        let (a_frame, a_fill, b_frame, b_fill) = (part(), part(), part(), part());
        // As `queue_material2d_meshes` keys it: the z then, no bias.
        let item = |entity: MainEntity, z: f32| Transparent2d {
            sort_key: FloatOrd(z),
            entity: (Entity::PLACEHOLDER, entity),
            pipeline: CachedRenderPipelineId::INVALID,
            draw_function: DrawFunctionId(0),
            batch_range: 0..1,
            extracted_index: usize::MAX,
            extra_index: PhaseItemExtraIndex::None,
            indexed: true,
        };
        let (fill, step) = (0.003, 0.004);
        let mut items = [
            item(a_frame, 0.0),
            item(a_fill, fill),
            item(b_frame, 0.0),
            item(b_fill, fill),
        ];

        // A moved, as extraction leaves it: its items not requeued.
        rezed(items.iter_mut(), |entity| {
            let z = match entity {
                e if e == a_frame => step,
                e if e == a_fill => step + fill,
                e if e == b_frame => 0.0,
                _ => fill,
            };

            Some(FloatOrd(z))
        });

        items.sort_by_key(SortedPhaseItem::sort_key);

        let order: Vec<MainEntity> = items.iter().map(|item| item.entity.1).collect();

        assert_eq!(order, [b_frame, b_fill, a_frame, a_fill]);
    }
}
