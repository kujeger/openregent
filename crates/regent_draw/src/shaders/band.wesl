// The other half of the same tile: the pixels that are an operation.
//
// Each index's factor in thousandths, and the alpha a darkening one
// writes, come from `regent_draw::soft` as shader defs, by the tile's draw
// mode. `[enhancements] shadows = "soft"`: the blend keeps the least
// alpha, the most coverage, which `shadow.wesl` resolves; a glow
// multiplies colour; a darkening pixel tests at the depth of the ground
// it lies on (`regent_draw::lie`). "blend": the blend multiplies the target
// by the factor. See `regent_draw::material`.

import bevy_pbr::render::mesh_functions;
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::view_transformations::position_world_to_clip;
import regent_draw::shaders::sway::{Gust, swayed};
@if(SNAP)
import regent_draw::shaders::smooth::{lattice, snapped};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var indices: texture_2d<u32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var rows: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var<uniform> wind: Gust;

const FACTORS = array<u32, 8>(
    constants::FACTOR_247, constants::FACTOR_248, constants::FACTOR_249, constants::FACTOR_250,
    constants::FACTOR_251, constants::FACTOR_252, constants::FACTOR_253, constants::FACTOR_254,
);

@if(SOFT)
const ALPHAS = array<u32, 8>(
    constants::ALPHA_247, constants::ALPHA_248, constants::ALPHA_249, constants::ALPHA_250,
    constants::ALPHA_251, constants::ALPHA_252, constants::ALPHA_253, constants::ALPHA_254,
);

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) uv: vec2<f32>,
    // The ground row at the quad's top, as bits; 0 none (`lie::tag`).
    @location(2) @interpolate(flat) top: u32,
};

struct Drawn {
    @location(0) colour: vec4<f32>,
    @if(SOFT) @builtin(frag_depth) depth: f32,
};

@vertex
fn vertex(vertex: Vertex) -> Out {
    var out: Out;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);

    // A tree's shadow leans with it (`regent_draw::wind`).
@if(SWAY) {
    out.world_position = swayed(world_from_local, vertex.position, wind);
}
@else {
    out.world_position = world_from_local * vec4<f32>(vertex.position, 1.0);
}
    out.position = position_world_to_clip(out.world_position.xyz);

    // Moved with its body (`body.wesl`), so shadow and colour meet on one
    // pixel edge.
@if(SNAP) {
    let top = (world_from_local * vec4<f32>(-0.5, 0.5, 0.0, 1.0)).xyz;
    let point = position_world_to_clip(vec3<f32>(lattice(top.xy, f32(constants::SNAP_LATTICE)), top.z));
    let corner = position_world_to_clip(top);
    let across = position_world_to_clip((world_from_local * vec4<f32>(0.5, 0.5, 0.0, 1.0)).xyz);
    let texels = f32(textureDimensions(indices).x);
    let below = f32(constants::SNAP_BELOW) / 100.0;

    out.position = snapped(out.position, point, corner, across, texels, view.viewport, below);
}

    out.uv = vertex.uv;
    out.top = mesh_functions::get_tag(vertex.instance_index);

    return out;
}

/// Whether index `index`, 247 to 254, darkens: its factor is under one.
fn darkens(index: u32) -> bool {
    var factors = FACTORS;

    return factors[index - 247u] < 1000u;
}

@if(SOFT)
/// The depth darkening pixel `at` of a `size` tile tests at: the nearer
/// of its caster's and that of the ground at its run's foot, the lowest
/// darkening pixel below it before a hole. Its caster's where it has no
/// footing or glows.
fn lying(in: Out, at: vec2<u32>, size: vec2<u32>, index: u32) -> f32 {
    if in.top == 0u || !darkens(index) {
        return in.position.z;
    }

    var foot = at.y;

    for (var y = at.y + 1u; y < size.y; y++) {
        let below = textureLoad(indices, vec2<u32>(at.x, y), 0).r;

        if below == 255u {
            break;
        }

        if below >= 247u && darkens(below) {
            foot = y;
        }
    }

    // The foot's ground row, then the depth a thing standing there draws at.
    let row = bitcast<f32>(in.top) + (f32(foot) + 0.5) * 1000.0 / f32(constants::ROW);
    let count = textureDimensions(rows);
    let last = f32(count.x * count.y - 1u);
    let i = u32(clamp(round(row * f32(constants::PER_CELL)), 0.0, last));
    let ground = textureLoad(rows, vec2<u32>(i % constants::WIDTH, i / constants::WIDTH), 0).r;

    let z = max(ground, in.world_position.z);
    let clip = view.clip_from_world * vec4<f32>(in.world_position.xy, z, 1.0);

    return clip.z / clip.w;
}

@fragment
fn fragment(in: Out) -> Drawn {
    let size = textureDimensions(indices);
    let at = min(vec2<u32>(in.uv * vec2<f32>(size)), size - vec2<u32>(1u));

    let index = textureLoad(indices, at, 0).r;

    // 255 is a hole and anything below 247 is an ordinary colour. Neither is
    // this pass's business.
    if index < 247u || index == 255u {
        discard;
    }

    // A model casts this shadow (`regent_draw::sun`); a glow still draws.
@if(DROPPED) {
    if darkens(index) {
        discard;
    }
}

    // A var: a value array takes no runtime index.
    var factors = FACTORS;
    let factor = f32(factors[index - 247u]) / 1000.0;
    var out: Drawn;

@if(SOFT) {
    // The blend gives dst × (1 + rgb) and the least alpha: a glow's gain
    // in colour, a shadow's coverage in alpha.
    var alphas = ALPHAS;

    out.colour = vec4<f32>(vec3<f32>(max(factor - 1.0, 0.0)), f32(alphas[index - 247u]) / 255.0);
    out.depth = lying(in, at, size, index);
}
@else {
    // The blend gives dst × (rgb + a): a factor under one in alpha, the
    // part over one in colour.
    out.colour = vec4<f32>(vec3<f32>(max(factor - 1.0, 0.0)), min(factor, 1.0));
}

    return out;
}
