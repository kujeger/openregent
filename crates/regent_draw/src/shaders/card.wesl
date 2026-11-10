// A world mesh's vertex stage under the 3D world camera (`regent_draw::frame`):
// Bevy's own, bar the ground's depth. `plain.wesl`, `sheet.wesl`,
// `veil.wesl` and `ring.wesl` call it from their `vertex`.
//
// Under `GROUND` a vertex sits at its metric point, its depth far off the
// camera's range; it draws at its rung instead, the translation's z, as
// the 2D ground did. `world_position` keeps the metric point.

import bevy_pbr::render::mesh_functions;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Bevy's 3D attribute locations (`MeshPipeline::specialize`).
struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @if(VERTEX_UVS_A) @location(2) uv: vec2<f32>,
    @if(VERTEX_COLORS) @location(5) color: vec4<f32>,
};

struct Card {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @if(VERTEX_COLORS) @location(2) color: vec4<f32>,
};

fn place(vertex: Vertex) -> Card {
    var out: Card;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world = world_from_local * vec4<f32>(vertex.position, 1.0);

@if(GROUND) {
    // x and y as given; the depth the rung's.
    let rung = world_from_local[3].z;

    out.position = position_world_to_clip(vec3<f32>(world.xy, rung));
    out.world_position = vec4<f32>(world.xy, world.z - rung, 1.0);
}
@else {
    out.position = position_world_to_clip(world.xyz);
    out.world_position = world;
}

@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_COLORS)
    out.color = vertex.color;

    return out;
}
