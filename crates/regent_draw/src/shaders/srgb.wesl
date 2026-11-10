// The sRGB curve both ways, for the shaders that work in linear colour
// (`regent_draw::shaders::srgb`). `shadow::Plugin` loads it; `soft.rs`
// mirrors it in Rust.

/// Linear to the sRGB byte's fraction.
fn to_srgb(c: f32) -> f32 {
    return select(1.055 * pow(c, 1.0 / 2.4) - 0.055, c * 12.92, c <= 0.0031308);
}

fn to_srgb4(c: vec4<f32>) -> vec4<f32> {
    return select(1.055 * pow(c, vec4<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec4<f32>(0.0031308));
}

/// The sRGB byte's fraction to linear.
fn from_srgb(c: f32) -> f32 {
    return select(pow((c + 0.055) / 1.055, 2.4), c / 12.92, c <= 0.04045);
}

fn from_srgb3(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}
