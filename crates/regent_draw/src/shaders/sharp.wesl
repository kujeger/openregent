// A picture scaled sharp-bilinear: each texel a flat square, and a
// linear blend one output pixel wide where two meet, so pixels stay even
// at any factor. `regent_draw::present`.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

struct Mapping {
    origin: vec2<f32>,
    start: vec2<f32>,
    factor: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> mapping: Mapping;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var picture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var picture_sampler: sampler;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(picture));
    let texel = (mesh.position.xy - mapping.origin) / mapping.factor + mapping.start;

    // Within `flat` of a texel's middle, its middle; past it, the blend
    // to the next, squeezed into the last output pixel. A factor of 1 is
    // a copy.
    let factor = max(mapping.factor, 1.0);
    let flat = 0.5 - 0.5 / factor;
    let from_middle = fract(texel) - 0.5;
    let past = from_middle - clamp(from_middle, vec2<f32>(-flat), vec2<f32>(flat));
    let at = floor(texel) + 0.5 + past * factor;

    return textureSampleLevel(picture, picture_sampler, at / size, 0.0);
}
