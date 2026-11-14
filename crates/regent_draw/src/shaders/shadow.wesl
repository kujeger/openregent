// Put every pixel on the 16-bit frame the original drew into: its present
// shifts each 565 channel up, low bits zero (`regent_core::view::lit::present`).
// With `[enhancements] shadows = "soft"` (`regent_draw::soft`), first darken
// each pixel by the coverage the bands left in alpha.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import regent_draw::shaders::srgb::{to_srgb, from_srgb};
@group(0) @binding(0) var screen: texture_2d<f32>;
// Whole pixels each output pixel reads back by: its pixel p takes the
// screen's p − shift (`shadow::Written::Under`), else 0.
@group(0) @binding(1) var<uniform> shift: vec4<i32>;

/// A colour as the original's screen shows it: 565, shifted up.
fn present(rgb: vec3<f32>) -> vec4<f32> {
    let srgb = vec3<f32>(to_srgb(rgb.r), to_srgb(rgb.g), to_srgb(rgb.b));
    let byte = vec3<u32>(round(srgb * 255.0)) & vec3<u32>(0xf8u, 0xfcu, 0xf8u);
    let out = vec3<f32>(byte) / 255.0;

    return vec4<f32>(from_srgb(out.r), from_srgb(out.g), from_srgb(out.b), 1.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let there = textureLoad(screen, vec2<i32>(in.position.xy) - shift.xy, 0);

@if(SOFT) {
    // Alpha holds the most coverage any shadow gave, 255 none, 1 full;
    // the clear's 0 none too (`soft::coverage`).
    let a = u32(round(there.a * 255.0));
    let c = select(f32(255u - a) / 254.0, 0.0, a == 0u);

    // One strength in linear light: hue kept (`soft::darken`).
    let strength = f32(constants::STRENGTH) / 1000.0;

    return present(there.rgb * (1.0 - c * (1.0 - strength)));
}
@else {
    return present(there.rgb);
}
}
