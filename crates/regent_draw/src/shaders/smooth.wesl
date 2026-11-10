// Sharp-bilinear in a picture's own texels: flat inside a texel, a blend
// half a physical pixel each side of its edges; and each quad's corner
// put on a pixel. `regent_draw::smooth`.
//
//   texel 3.9, footprint 0.5 (2 pixels a texel): 0.1 to the edge, the
//   next texel weighs 0.5 - 0.1 / 0.5 = 0.3; at 3.5, the middle, 0.

// Toward which neighbour each axis blends (-1 or 1), and by how much.
struct Blend {
    step: vec2<i32>,
    weight: vec2<f32>,
}

// `texel` the point in texels (texel i spans i .. i + 1), `footprint`
// texels a physical pixel, as `fwidth` gives them.
fn blend(texel: vec2<f32>, footprint: vec2<f32>) -> Blend {
    let from_middle = fract(texel) - 0.5;
    let to_edge = 0.5 - abs(from_middle);
    let weight = clamp(0.5 - to_edge / max(footprint, vec2<f32>(1e-6)), vec2<f32>(0.0), vec2<f32>(0.5));
    let step = select(vec2<i32>(-1), vec2<i32>(1), from_middle >= vec2<f32>(0.0));

    return Blend(step, weight);
}

// The four texels' colours put together by `b`: `here` the texel's own,
// `across` its neighbour in x, `down` in y, `diagonal` in both.
fn mixed(b: Blend, here: vec4<f32>, across: vec4<f32>, down: vec4<f32>, diagonal: vec4<f32>) -> vec4<f32> {
    return mix(mix(here, across, b.weight.x), mix(down, diagonal, b.weight.x), b.weight.y);
}

// Physical pixels half the clip range spans in `viewport` (x, y, width,
// height): y counts down.
fn half(viewport: vec4<f32>) -> vec2<f32> {
    return viewport.zw * vec2<f32>(0.5, -0.5);
}

// Clip point `clip` in framebuffer pixels.
fn pixel(clip: vec4<f32>, viewport: vec4<f32>) -> vec2<f32> {
    return clip.xy / clip.w * half(viewport) + viewport.xy + viewport.zw * 0.5;
}

// `smooth::SNAP_TIE`: a half between pixels rounds up, whatever the noise.
const SNAP_TIE: f32 = 0.00390625;

// The world point whose pixel decides how a quad with its corner at world
// point `corner` moves, `steps` a world unit: `smooth::lattice`. A piece's
// frames' corners lie whole units apart, so all share it and move alike.
fn lattice(corner: vec2<f32>, steps: f32) -> vec2<f32> {
    return corner - floor(floor(corner * steps + 0.5) / steps);
}

// Vertex `position` (clip) of a quad, moved so its `lattice` point `point`
// (clip) lands on a pixel corner where a texel is under `below` pixels:
// `smooth::moved`. `corner` and `across` are the quad's top corners (clip),
// `texels` its texture's width: the pixels a texel.
fn snapped(
    position: vec4<f32>,
    point: vec4<f32>,
    corner: vec4<f32>,
    across: vec4<f32>,
    texels: f32,
    viewport: vec4<f32>,
    below: f32,
) -> vec4<f32> {
    let at = pixel(point, viewport);
    let factor = distance(pixel(across, viewport), pixel(corner, viewport)) / texels;
    let shift = select(vec2<f32>(0.0), floor(at + 0.5 + SNAP_TIE) - at, factor < below);

    return position + vec4<f32>(shift / half(viewport) * position.w, 0.0, 0.0);
}
