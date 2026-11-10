// The models' shadows on a point (`regent_draw::sun`), as soft shadows'
// coverage in alpha: Bevy's directional light, its shadow map and bias,
// the sun light 0. Bodies (`body.wesl`) and the ground (`plain.wesl`,
// `sheet.wesl`) write it.
//
// A sun of no size: Bevy's own fixed filter. A sized sun (its spread as
// the light's soft shadow size): our own percentage-closer soft shadows,
// as Bevy's PCSS assumes a perspective depth:
//
//   search   the map's depths within reach of the point, nearer the
//            sun than it: blockers, their mean depth
//   width    world units along the light from them to the point, times
//            the spread: the filter's radius, sharp at a caster's foot
//   filter   a grid of the map's 2×2 compares across that radius, tent
//            weighted, its taps never far apart: smooth, no noise

import bevy_pbr::render::mesh_view_bindings::{
    directional_shadow_textures, directional_shadow_textures_comparison_sampler,
    directional_shadow_textures_linear_sampler, lights,
};
import bevy_pbr::render::shadows::{fetch_directional_shadow, world_to_directional_light_local};

// The art camera's elevation's tangent (`regent_draw::frame::ELEVATION`; a
// test there checks it).
const TAN_ELEVATION: f32 = 0.7002075;

// How far along the light from its caster a penumbra keeps widening,
// world units (`regent_draw::sun::cast::ALONG`).
const ALONG: f32 = 400.0;

// The filter's least radius, texels: about Bevy's Gaussian filter's, so
// a contact is as sharp as under a sun of no size.
const LEAST: f32 = 1.5;

// The search's gathers a side, each four texels.
const SEARCH: i32 = 4;

// The filter's taps a side: at most, and their widest spacing, texels,
// below which the 2×2 compares blend smoothly.
const TAPS: i32 = 12;
const STEP: f32 = 1.5;

/// Coverage of world point `p`, 0 to 1, drawn at pixel `pixel`. None in
/// a view the sun does not light, or while it casts nothing (no
/// cascade). The normal is the way to the sun, so
/// Bevy's normal bias moves the point toward it, as its depth bias does.
fn covered(p: vec3<f32>, pixel: vec2<f32>) -> f32 {
    if lights.n_directional_lights == 0u {
        return 0.0;
    }

    let light = &lights.directional_lights[0];
    let toward = (*light).direction_to_light;

    if (*light).soft_shadow_size > 0.0 {
        return penumbral(p, (*light).soft_shadow_size);
    }

    // One cascade: a view depth of 0 picks it.
    let lit = fetch_directional_shadow(0u, vec4<f32>(p, 1.0), toward, 0.0, pixel);

    return 1.0 - lit;
}

/// Coverage of world point `p` under a sun spreading `spread` a world
/// unit along the light: percentage-closer soft shadows on the one
/// cascade (see the top). None while it casts nothing.
fn penumbral(p: vec3<f32>, spread: f32) -> f32 {
    let light = &lights.directional_lights[0];

    if (*light).num_cascades == 0u {
        return 0.0;
    }

    let cascade = &(*light).cascades[0];
    let texel = (*cascade).texel_size;
    let toward = (*light).direction_to_light;

    // Bevy's biases, as its own path takes them, the normal toward the sun.
    let bias = (*light).shadow_normal_bias * texel + (*light).shadow_depth_bias;
    let local = world_to_directional_light_local(0u, 0u, vec4<f32>(p + toward * bias, 1.0));

    if local.w == 0.0 {
        return 0.0;
    }

    let layer = i32((*light).depth_texture_base_index);
    let m = (*cascade).clip_from_world;
    // The map's uv a world unit across the light (clip's x and y over two),
    // and world units from depth 0 to 1, along it.
    let uv = vec2<f32>(length(vec3<f32>(m[0].x, m[1].x, m[2].x)), length(vec3<f32>(m[0].y, m[1].y, m[2].y))) * 0.5;
    let depths = 1.0 / length(vec3<f32>(m[0].z, m[1].z, m[2].z));
    let texels = vec2<f32>(textureDimensions(directional_shadow_textures).xy);
    let least = LEAST * texel;
    let reach = max(spread * ALONG, least);

    // The search: reverse Z, a blocker's depth the greater.
    var sum = 0.0;
    var count = 0.0;

    for (var i = 0; i < SEARCH; i++) {
        for (var j = 0; j < SEARCH; j++) {
            let at = (vec2<f32>(f32(i), f32(j)) + 0.5) / f32(SEARCH) * 2.0 - 1.0;
            let d = textureGather(
                directional_shadow_textures,
                directional_shadow_textures_linear_sampler,
                local.xy + at * reach * uv,
                layer,
            );
            let near = select(vec4<f32>(0.0), vec4<f32>(1.0), d >= vec4<f32>(local.z));

            sum += dot(d, near);
            count += dot(near, vec4<f32>(1.0));
        }
    }

    // None in reach: lit. All: the filter, within reach, is covered too.
    if count == 0.0 {
        return 0.0;
    }

    if count == f32(SEARCH * SEARCH * 4) {
        return 1.0;
    }

    // The width, and taps enough that none lie over STEP texels apart.
    let gap = (sum / count - local.z) * depths;
    let radius = clamp(gap * spread, least, reach);
    let across = radius * uv;
    let wide = max(across.x * texels.x, across.y * texels.y);
    let taps = clamp(i32(ceil(2.0 * wide / STEP)) + 1, 3, TAPS);
    var lit = 0.0;
    var weights = 0.0;

    for (var i = 0; i < taps; i++) {
        for (var j = 0; j < taps; j++) {
            let at = (vec2<f32>(f32(i), f32(j)) + 0.5) / f32(taps) * 2.0 - 1.0;
            let w = (1.0 - abs(at.x)) * (1.0 - abs(at.y));

            lit += w * textureSampleCompareLevel(
                directional_shadow_textures,
                directional_shadow_textures_comparison_sampler,
                local.xy + at * across,
                layer,
                local.z,
            );
            weights += w;
        }
    }

    return 1.0 - lit / weights;
}

/// World point `p` moved `by` toward the sun: none in a view the sun
/// does not light.
fn sunward(p: vec3<f32>, by: f32) -> vec3<f32> {
    if lights.n_directional_lights == 0u {
        return p;
    }

    return p + lights.directional_lights[0].direction_to_light * by;
}

/// The world point of a body's pixel drawn at `drawn`, its footing's flat
/// ground at screen height `flat`: upright over the footing, its lift
/// the height over that (`regent_draw::frame::point`).
fn upright(drawn: vec2<f32>, flat: f32) -> vec3<f32> {
    let lift = drawn.y - flat;

    return vec3<f32>(drawn, lift * TAN_ELEVATION - flat / TAN_ELEVATION);
}

/// Coverage `c` as the alpha soft shadows read (`regent_draw::soft`): 1
/// none, 1/255 full.
fn shade(c: f32) -> f32 {
    return (255.0 - c * 254.0) / 255.0;
}
