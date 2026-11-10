// A selection ring drawn as its band (`regent_draw::ring`): an elliptic
// strip whose weight falls off across it, the outer edge smoothed over a
// screen pixel. The same arithmetic as `regent_core::view::map::ring::band`.

import regent_draw::shaders::card::{Card, Vertex, place};

struct Shape {
    size: vec2<f32>,
    radii: vec2<f32>,
    half: f32,
    peak: f32,
    core: f32,
    knee: f32,
    edge: f32,
    soft: f32,
    colour: vec4<f32>,
    line: f32,
    reach: f32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> shape: Shape;

/// How far `p` is from the middle line, art pixels: the ellipse's level
/// over its gradient (`Band::distance`).
fn distance(p: vec2<f32>) -> f32 {
    let r = shape.radii;
    let k = length(p / r);
    let slope = length(p / (r * r));

    return (k - 1.0) * k / max(slope, 1e-6);
}

/// The share of the peak `t` half widths from the middle line, inside
/// the band (`Profile::at`, before its stop at 1).
fn profile(t: f32) -> f32 {
    if t <= shape.core {
        return 1.0;
    }

    if t >= shape.knee {
        return shape.edge;
    }

    return mix(1.0, shape.edge, (t - shape.core) / (shape.knee - shape.core));
}

/// `Rings::Modern`'s alpha at `p`: a line `shape.line` screen pixels
/// wide on the middle line, and inside it a glow of `shape.peak` falling
/// off by a smoothstep over `shape.reach` half widths.
fn modern(p: vec2<f32>) -> f32 {
    let d = distance(p);
    let pixel = max(fwidth(d), 1e-4);
    let s = d / pixel;
    let h = shape.half / pixel;
    let line = clamp(shape.line * 0.5 - abs(s) + 0.5, 0.0, 1.0);
    let glow = shape.peak * (1.0 - smoothstep(0.0, 1.0, -s / (shape.reach * h)));

    return max(line, select(0.0, glow, s < 0.0));
}

@vertex
fn vertex(vertex: Vertex) -> Card {
    return place(vertex);
}

@fragment
fn fragment(in: Card) -> @location(0) vec4<f32> {
    let p = (in.uv - vec2<f32>(0.5)) * shape.size;

@if(LINE) {
    let alpha = modern(p);

    // Too faint to show.
    if alpha <= 0.003 {
        discard;
    }

    return vec4<f32>(shape.colour.rgb, alpha);
}
@else {
    let d = abs(distance(p));

    // Art pixels a screen pixel, so the edge is smooth at any zoom.
    let pixel = max(fwidth(d), 1e-4) * shape.soft;
    let inside = clamp((shape.half - d) / pixel + 0.5, 0.0, 1.0);
    let weight = shape.peak * profile(d / shape.half) * inside;

    if weight <= 0.0 {
        discard;
    }

@if(COLOUR)
    return vec4<f32>(shape.colour.rgb, weight);
@else
    return vec4<f32>(vec3<f32>(weight), 1.0);
}
}
