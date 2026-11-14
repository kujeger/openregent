// A tile drawn from its own palette indices.
//
// The blitter's branch, minus the shadow half: 255 draws nothing and 247 to
// 254 are an operation rather than a colour, so this pass leaves both to
// somebody else. See `regent_core::view::shade`. Under `SHARP` and `SNAP`, read
// sharp-bilinear on the pixels (`regent_draw::smooth`). An opaque one
// writes the models' shadows on it as coverage (`lit.wesl`); one of a
// building with a model at its point on the model, never in its own
// shadow (`regent_draw::sun::relief`). Each instance reads its row of
// `table` by its tag (`regent_draw::instance`): its footing, and under
// `SHEET` its frame's rectangle in the page (`regent_draw::map::sheet`);
// else the frame is the whole texture.

import bevy_pbr::render::mesh_functions;
import bevy_pbr::render::view_transformations::position_world_to_clip;
import regent_draw::shaders::lit::{covered, shade, sunward, upright};
import regent_draw::shaders::sway::{Gust, swayed};
@if(SNAP)
import bevy_pbr::render::mesh_view_bindings::view;
@if(SNAP)
import regent_draw::shaders::smooth::{lattice, snapped};
@if(SHARP)
import regent_draw::shaders::smooth::{blend, mixed};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var indices: texture_2d<u32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var palette: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var<uniform> wind: Gust;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var stands: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var relief: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(5) var table: texture_2d<u32>;

// `regent_draw::instance::WIDTH`: rows a texture row of `table`.
const ROWS_ACROSS: u32 = 1024u;

// `regent_draw::sun::relief`'s constants (a test there checks them): the
// texel whose corner is the anchor, a tag's slot marker, and a texel the
// model shades itself.
const ANCHOR_COLUMN: i32 = 192;
const ANCHOR_ROW: i32 = 224;
const SLOT_TAG: u32 = 0x7F800000u;
const SHADED: f32 = 1000000000.0;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) uv: vec2<f32>,
    // Its footing's flat ground height, as bits (`regent_draw::sun::stand`),
    // or its building's slot (`relief::model_tag`); 0 none: no models'
    // shadows.
    @location(2) @interpolate(flat) footing: u32,
    // The frame's texels in the texture: x, y, width, height.
    @location(3) @interpolate(flat) frame: vec4<u32>,
};

// Instance `at`'s row: its tag, rectangle x | y << 16, width |
// height << 16.
fn row(at: u32) -> vec4<u32> {
    return textureLoad(table, vec2<u32>(at % ROWS_ACROSS * 2u, at / ROWS_ACROSS), 0);
}

// Instance `at`'s tree lean: `wind` with its foot and bend.
fn leaning(at: u32, wind: Gust) -> Gust {
    let lean = textureLoad(table, vec2<u32>(at % ROWS_ACROSS * 2u + 1u, at / ROWS_ACROSS), 0);
    var own = wind;

    own.foot = bitcast<f32>(lean.x);
    own.bend = bitcast<f32>(lean.y);
    return own;
}

// The frame's rectangle in the texture from its row: the sheet's, else
// all of it.
fn framed(row: vec4<u32>) -> vec4<u32> {
@if(SHEET) {
    return vec4<u32>(row.y & 0xFFFFu, row.y >> 16u, row.z & 0xFFFFu, row.z >> 16u);
}
@else {
    return vec4<u32>(vec2<u32>(0u), textureDimensions(indices));
}
}

// Bevy's mesh vertex stage, bar a tree's lean under `SWAY` and the
// pixel snap under `SNAP`.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let tag = mesh_functions::get_tag(vertex.instance_index);
    let row = row(tag);

@if(SWAY) {
    out.world_position = swayed(world_from_local, vertex.position, leaning(tag, wind));
}
@else {
    out.world_position = world_from_local * vec4<f32>(vertex.position, 1.0);
}

    out.position = position_world_to_clip(out.world_position.xyz);

@if(SNAP) {
    // The unit quad's top corners, unswayed: uv (0, 0) and (1, 0). A
    // tree's foot snaps; its lean stays smooth.
    let top = (world_from_local * vec4<f32>(-0.5, 0.5, 0.0, 1.0)).xyz;
    let point = position_world_to_clip(vec3<f32>(lattice(top.xy, f32(constants::SNAP_LATTICE)), top.z));
    let corner = position_world_to_clip(top);
    let across = position_world_to_clip((world_from_local * vec4<f32>(0.5, 0.5, 0.0, 1.0)).xyz);
    let texels = f32(framed(row).z);
    let below = f32(constants::SNAP_BELOW) / 100.0;

    out.position = snapped(out.position, point, corner, across, texels, view.viewport, below);
}

    out.uv = vertex.uv;
    out.footing = row.x;
    out.frame = framed(row);

    return out;
}

@if(SHARP)
// The colour at `at` of frame `frame`, or `fallback` off the frame,
// where it draws nothing or an operation, or (translucent) no share:
// the art's edges stay nearest (`regent_draw::smooth`).
fn tap(at: vec2<i32>, fallback: vec4<f32>, frame: vec4<u32>) -> vec4<f32> {
    let size = vec2<i32>(frame.zw);

    if any(at < vec2<i32>(0)) || any(at >= size) {
        return fallback;
    }

    let index = textureLoad(indices, vec2<i32>(frame.xy) + at, 0).r;

    if index >= 247u {
        return fallback;
    }

    let colour = textureLoad(palette, vec2<u32>(index, 0u), 0);

@if(GLASS)
    return select(fallback, colour, colour.a > 0.0);
@else
    return colour;
}

// The models' shadow on the pixel drawn at `drawn` (`pixel` on the
// target) of a body tagged `tag`: upright over its footing, or on its
// building's model, none where the model shades itself.
fn received(tag: u32, drawn: vec2<f32>, pixel: vec2<f32>) -> f32 {
    if (tag & SLOT_TAG) != SLOT_TAG {
        return covered(upright(drawn, bitcast<f32>(tag)), pixel);
    }

    // The slot: anchor x, y, the footing's flat ground, the layer.
    let stand = textureLoad(stands, vec2<u32>(tag & ~SLOT_TAG, 0u), 0);
    let local = floor(drawn - stand.xy);
    let at = vec2<i32>(i32(local.x) + ANCHOR_COLUMN, ANCHOR_ROW - 1 - i32(local.y));
    let size = vec2<i32>(textureDimensions(relief));
    // Depth over the upright point, and lift toward the sun; off the
    // grid upright, as a pixel the model does not cover.
    var texel = vec2<f32>(0.0);

    if all(at >= vec2<i32>(0)) && all(at < size) {
        texel = textureLoad(relief, at, i32(stand.w), 0).rg;
    }

    if texel.x >= SHADED {
        return 0.0;
    }

    let point = upright(drawn, stand.z) + vec3<f32>(0.0, 0.0, texel.x);

    return covered(sunward(point, texel.y), pixel);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
@if(SHEET)
    let size = mesh.frame.zw;
@else
    let size = textureDimensions(indices);
@if(SHARP)
    let texel = mesh.uv * vec2<f32>(size);
    // Before any discard: derivatives need every pixel of the quad.
@if(SHARP)
    let b = blend(texel, fwidth(texel));
    // Clamped, because a uv of exactly one lands a texel past the edge.
    let at = min(vec2<u32>(mesh.uv * vec2<f32>(size)), size - vec2<u32>(1u));

    let index = textureLoad(indices, mesh.frame.xy + at, 0).r;

    if index >= 247u {
        discard;
    }

    let colour = textureLoad(palette, vec2<u32>(index, 0u), 0);
@if(SHARP)
    let here = vec2<i32>(at);
@if(SHARP)
    let shown = mixed(
        b,
        colour,
        tap(here + vec2<i32>(b.step.x, 0), colour, mesh.frame),
        tap(here + vec2<i32>(0, b.step.y), colour, mesh.frame),
        tap(here + b.step, colour, mesh.frame),
    );
@else
    let shown = colour;

@if(GLASS) {
    // Translucent: the palette's alpha is the share, worked out in Rust
    // (`regent_core::view::blend`); the blend keeps the target's alpha.
    if colour.a <= 0.0 {
        discard;
    }

    return shown;
}
@elif(LIFT) {
    // A translucent tile's twin: only alpha is written, the shadow coverage
    // under its drawn pixels cleared (`regent_draw::soft::lifts`).
    if colour.a <= 0.0 {
        discard;
    }

    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
@else {
    // Opaque, and the alpha matters: it is the channel soft shadows write
    // their coverage into, so anything drawn sets it: none, or the
    // models' shadow on the pixel (`received`).
    var alpha = 1.0;

    if mesh.footing != 0u {
        alpha = shade(received(mesh.footing, mesh.world_position.xy, mesh.position.xy));
    }

    return vec4<f32>(shown.rgb, alpha);
}
}
