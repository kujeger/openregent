// A picture's part shrunk by area, as `art::shrink::part` does it on the
// CPU: each new pixel the mean of the source area it covers, a
// part-covered pixel by its share. Both textures are read and written
// as plain unorm, so the bytes are averaged as stored: premultiplied,
// sRGB-encoded, as egui keeps them.

struct Part {
    // The part's corner, source pixels.
    start: vec2<f32>,
    // Source pixels a new pixel covers.
    step: vec2<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> part: Part;

// One triangle covering the target.
@vertex
fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));

    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

// The source pixels new pixel `at` covers, each by its share: rows
// narrowed, then summed down, as the CPU does.
@fragment
fn fragment(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let side = vec2<i32>(textureDimensions(source));
    let i = floor(at.xy);
    let start = part.start + i * part.step;
    let end = part.start + (i + 1.0) * part.step;
    let first = vec2<i32>(floor(max(start, vec2<f32>(0.0))));
    let last = min(vec2<i32>(ceil(end)), side);
    var sum = vec4<f32>(0.0);

    for (var y = first.y; y < last.y; y++) {
        let down = (min(f32(y + 1), end.y) - max(f32(y), start.y)) / part.step.y;
        var row = vec4<f32>(0.0);

        for (var x = first.x; x < last.x; x++) {
            let across = (min(f32(x + 1), end.x) - max(f32(x), start.x)) / part.step.x;

            row += textureLoad(source, vec2<i32>(x, y), 0) * across;
        }

        sum += row * down;
    }

    return sum;
}
