// The composed ground read sharp-bilinear (`regent_draw::smooth::Sheet`):
// one linear tap at a uv warped as `sharp.wesl`'s, so each texel is flat
// and the blend to the next one physical pixel wide. A cell without art
// is a hole, as with its `plain::Plain`. While models cast
// (`CAST`), its alpha is their shadows on it (`lit.wesl`).

import regent_draw::shaders::card::{Card, Vertex, place};
@if(CAST)
import regent_draw::shaders::lit::{covered, shade};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var picture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var picture_sampler: sampler;

// `regent_draw::material::CUTOFF`.
const CUTOFF: f32 = 0.5;

@vertex
fn vertex(vertex: Vertex) -> Card {
    return place(vertex);
}

@fragment
fn fragment(mesh: Card) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(picture));
    let texel = mesh.uv * size;
    // Texels a physical pixel, at most one (below, a copy). Before any
    // discard: derivatives need every pixel of the quad.
    let footprint = clamp(fwidth(texel), vec2<f32>(1e-6), vec2<f32>(1.0));

    // Within `flat` of a texel's middle, its middle; past it, the blend
    // to the next, squeezed into the last physical pixel.
    let flat = 0.5 - 0.5 * footprint;
    let from_middle = fract(texel) - 0.5;
    let past = from_middle - clamp(from_middle, -flat, flat);
    let at = floor(texel) + 0.5 + past / footprint;
    let colour = textureSampleLevel(picture, picture_sampler, at / size, 0.0);

    let here = textureLoad(picture, vec2<u32>(min(texel, size - 1.0)), 0);

    if here.a < CUTOFF {
        discard;
    }

    // A hole's texels are 0 (`Sheet::blank`): over the weight of the rest,
    // the blend of those alone.
    // Alpha: the models' shadows on its metric point, as coverage.
    var alpha = 1.0;

@if(CAST)
    alpha = shade(covered(mesh.world_position.xyz, mesh.position.xy));

    return vec4<f32>(colour.rgb / max(colour.a, 1e-6), alpha);
}
