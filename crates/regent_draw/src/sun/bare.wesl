// A model drawn plain grey in its building's place (`regent_draw::sun::bare`):
// at its metric points, its depth its piece's draw depth (the mesh tag)
// plus its metric depth squeezed; lit by the sun's angle on each face,
// darkened by the models' shadows.

import bevy_pbr::render::mesh_functions;
import bevy_pbr::render::view_transformations::position_world_to_clip;
import bevy_pbr::render::mesh_view_bindings::lights;
import regent_draw::shaders::lit::{covered, sunward};

// World units of draw depth a world unit of metric depth moves a point:
// the model's span stays within a row or so of its building's depth.
const SQUEEZE: f32 = 0.0001;

// The grey, and the light it takes with no sun and with the sun square on.
const GREY: f32 = 0.6;
const AMBIENT: f32 = 0.2;
const DIRECT: f32 = 0.8;

// How far toward the sun a point's shadow is looked up, world units: off
// the face it lies on.
const OFF_FACE: f32 = 2.0;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> Out {
    var out: Out;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world = (world_from_local * vec4<f32>(vertex.position, 1.0)).xyz;
    let anchor = world_from_local[3].xyz;
    let rung = bitcast<f32>(mesh_functions::get_tag(vertex.instance_index));

    out.position = position_world_to_clip(vec3<f32>(world.xy, rung + (world.z - anchor.z) * SQUEEZE));
    out.world = world;
    out.normal = normalize((world_from_local * vec4<f32>(vertex.normal, 0.0)).xyz);

    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    var light = AMBIENT;

    if lights.n_directional_lights > 0u {
        let toward = lights.directional_lights[0].direction_to_light;
        // The models' faces wind either way: a face the camera sees
        // faces it (the camera looks down -z), so its normal is turned
        // toward +z.
        let normal = in.normal * sign(in.normal.z + 1e-6);
        let facing = max(dot(normal, toward), 0.0);
        let lit = 1.0 - covered(sunward(in.world, OFF_FACE), in.position.xy);

        light = AMBIENT + DIRECT * facing * lit;
    }

    // Alpha 1: no soft-shadow coverage of its own; the shading is above.
    return vec4<f32>(vec3<f32>(GREY * light), 1.0);
}
