// A mesh's own colour (`regent_draw::plain::Plain`): a colour, times its
// vertex colours and its picture where it has them, as 2D's
// `ColorMaterial` draws it. While models cast (`CAST`), the ground's
// alpha is their shadows on it (`lit.wesl`).

import regent_draw::shaders::card::{Card, Vertex, place};
@if(CAST)
import regent_draw::shaders::lit::{covered, shade};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> colour: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var picture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var picture_sampler: sampler;

// `regent_draw::material::CUTOFF`.
const CUTOFF: f32 = 0.5;

@vertex
fn vertex(vertex: Vertex) -> Card {
    return place(vertex);
}

@fragment
fn fragment(in: Card) -> @location(0) vec4<f32> {
    var out = colour;

@if(VERTEX_COLORS)
    out = out * in.color;
@if(PICTURE)
    out = out * textureSample(picture, picture_sampler, in.uv);

    // Drawn whole at the cutoff and over, else not at all.
@if(MASK) {
    if out.a < CUTOFF {
        discard;
    }

    out.a = 1.0;
}

    // The ground: the models' shadows on its metric point, as coverage.
@if(CAST)
    out.a = shade(covered(in.world_position.xyz, in.position.xy));

    return out;
}
