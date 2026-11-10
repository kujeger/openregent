// The fog over unexplored ground: black, as much as the corners say.
//
// The vertex colour's red is the share of its shade a corner keeps, 1
// explored and near 0 not, interpolated over the cell's triangles as the
// engine interpolates shade. The engine scales the screen's bytes; the
// blend here is in linear colour, so the share goes through the sRGB curve
// first. The blend keeps the target's alpha: the shadow's coverage.

import regent_draw::shaders::card::{Card, Vertex, place};
import regent_draw::shaders::srgb::from_srgb;

@vertex
fn vertex(vertex: Vertex) -> Card {
    return place(vertex);
}

@fragment
fn fragment(mesh: Card) -> @location(0) vec4<f32> {
@if(VERTEX_COLORS)
    let corners = clamp(mesh.color.r, 0.0, 1.0);
@else
    let corners = 1.0;

    return vec4<f32>(0.0, 0.0, 0.0, 1.0 - from_srgb(corners));
}
