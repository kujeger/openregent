// FSR 1's EASU (Edge Adaptive Spatial Upsampling), `FsrEasuF`, the
// 32-bit path, in WGSL: a 12-texel Lanczos-like kernel stretched along
// the local edge. RCAS follows (`rcas.wesl`). `regent_draw::present`.
//
// Ported from AMD's `ffx_fsr1.h` and `ffx_a.h`, FidelityFX-FSR v1.0.2
// (commit a21ffb8f6c13233ba336352bdff293894c706575,
// https://github.com/GPUOpen-Effects/FidelityFX-FSR):
//
// Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files(the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and / or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions :
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.
//
// Where WGSL differs from the header:
// - `FsrEasuCon` runs on the CPU (`present::EasuCon`); its words arrive
//   as `vec4<u32>` and are bit cast, as `AF2_AU2`.
// - The picture is `Rgba8UnormSrgb`, so a gather returns linear light;
//   EASU wants display-encoded input, so the callbacks encode sRGB and
//   the output is decoded again for the sRGB target.
// - The callbacks gather with `textureGather`, the sampler clamping to
//   the edge.
// - `ip` is the fragment's pixel, `floor(position.xy)`, not a thread id.
// - The 16-bit packed path (`FsrEasuH`) is left out: no packed half.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import regent_draw::shaders::srgb::{to_srgb4, from_srgb3};

struct Con {
    con0: vec4<u32>,
    con1: vec4<u32>,
    con2: vec4<u32>,
    con3: vec4<u32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> con: Con;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var picture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var picture_sampler: sampler;

// The input callbacks: one channel of four texels about `p`.
fn FsrEasuRF(p: vec2<f32>) -> vec4<f32> {
    return to_srgb4(textureGather(0, picture, picture_sampler, p));
}

fn FsrEasuGF(p: vec2<f32>) -> vec4<f32> {
    return to_srgb4(textureGather(1, picture, picture_sampler, p));
}

fn FsrEasuBF(p: vec2<f32>) -> vec4<f32> {
    return to_srgb4(textureGather(2, picture, picture_sampler, p));
}

// ffx_a.h.
fn APrxLoRcpF1(a: f32) -> f32 {
    return bitcast<f32>(0x7ef07ebbu - bitcast<u32>(a));
}

fn APrxLoRsqF1(a: f32) -> f32 {
    return bitcast<f32>(0x5f347d74u - (bitcast<u32>(a) >> 1u));
}

fn ARcpF1(x: f32) -> f32 {
    return 1.0 / x;
}

fn AMin3F3(x: vec3<f32>, y: vec3<f32>, z: vec3<f32>) -> vec3<f32> {
    return min(x, min(y, z));
}

fn AMax3F3(x: vec3<f32>, y: vec3<f32>, z: vec3<f32>) -> vec3<f32> {
    return max(x, max(y, z));
}

// Filtering for a given tap for the scalar. `acc` is `aC` and `aW`.
fn FsrEasuTapF(
    acc: ptr<function, vec4<f32>>,
    off: vec2<f32>,
    dir: vec2<f32>,
    len: vec2<f32>,
    lob: f32,
    clp: f32,
    c: vec3<f32>,
) {
    // Rotate offset by direction.
    var v: vec2<f32>;
    v.x = (off.x * (dir.x)) + (off.y * dir.y);
    v.y = (off.x * (-dir.y)) + (off.y * dir.x);
    // Anisotropy.
    v *= len;
    // Compute distance^2.
    var d2 = v.x * v.x + v.y * v.y;
    // Limit to the window as at corner, 2 taps can easily be outside.
    d2 = min(d2, clp);
    // Approximation of lancos2 without sin() or rcp(), or sqrt() to get x.
    //  (25/16 * (2/5 * x^2 - 1)^2 - (25/16 - 1)) * (1/4 * x^2 - 1)^2
    //  |_______________________________________|   |_______________|
    //                   base                             window
    var wB = (2.0 / 5.0) * d2 + (-1.0);
    var wA = lob * d2 + (-1.0);
    wB *= wB;
    wA *= wA;
    wB = (25.0 / 16.0) * wB + (-(25.0 / 16.0 - 1.0));
    let w = wB * wA;
    // Do weighted average.
    *acc += vec4<f32>(c * w, w);
}

// Accumulate direction and length. `bi` the bilinear weight, picked by
// the header's `biS`..`biV`; `acc` is `dir` and `len`.
fn FsrEasuSetF(acc: ptr<function, vec3<f32>>, w: f32, lA: f32, lB: f32, lC: f32, lD: f32, lE: f32) {
    // Direction is the '+' diff.
    //    a
    //  b c d
    //    e
    // Then takes magnitude from abs average of both sides of 'c'.
    // Length converts gradient reversal to 0, smoothly to non-reversal at 1, shaped, then adding horz and vert terms.
    let dc = lD - lC;
    let cb = lC - lB;
    var lenX = max(abs(dc), abs(cb));
    lenX = APrxLoRcpF1(lenX);
    let dirX = lD - lB;
    (*acc).x += dirX * w;
    lenX = clamp(abs(dirX) * lenX, 0.0, 1.0);
    lenX *= lenX;
    (*acc).z += lenX * w;
    // Repeat for the y axis.
    let ec = lE - lC;
    let ca = lC - lA;
    var lenY = max(abs(ec), abs(ca));
    lenY = APrxLoRcpF1(lenY);
    let dirY = lE - lA;
    (*acc).y += dirY * w;
    lenY = clamp(abs(dirY) * lenY, 0.0, 1.0);
    lenY *= lenY;
    (*acc).z += lenY * w;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let ip = vec2<u32>(mesh.position.xy);

    // Get position of 'f'.
    var pp = vec2<f32>(ip) * bitcast<vec2<f32>>(con.con0.xy) + bitcast<vec2<f32>>(con.con0.zw);
    let fp = floor(pp);
    pp -= fp;

    // 12-tap kernel.
    //    b c
    //  e f g h
    //  i j k l
    //    n o
    // Gather 4 ordering.
    //  a b
    //  r g
    let p0 = fp * bitcast<vec2<f32>>(con.con1.xy) + bitcast<vec2<f32>>(con.con1.zw);
    // These are from p0 to avoid pulling two constants on pre-Navi hardware.
    let p1 = p0 + bitcast<vec2<f32>>(con.con2.xy);
    let p2 = p0 + bitcast<vec2<f32>>(con.con2.zw);
    let p3 = p0 + bitcast<vec2<f32>>(con.con3.xy);
    let bczzR = FsrEasuRF(p0);
    let bczzG = FsrEasuGF(p0);
    let bczzB = FsrEasuBF(p0);
    let ijfeR = FsrEasuRF(p1);
    let ijfeG = FsrEasuGF(p1);
    let ijfeB = FsrEasuBF(p1);
    let klhgR = FsrEasuRF(p2);
    let klhgG = FsrEasuGF(p2);
    let klhgB = FsrEasuBF(p2);
    let zzonR = FsrEasuRF(p3);
    let zzonG = FsrEasuGF(p3);
    let zzonB = FsrEasuBF(p3);

    // Simplest multi-channel approximate luma possible (luma times 2, in 2 FMA/MAD).
    let bczzL = bczzB * 0.5 + (bczzR * 0.5 + bczzG);
    let ijfeL = ijfeB * 0.5 + (ijfeR * 0.5 + ijfeG);
    let klhgL = klhgB * 0.5 + (klhgR * 0.5 + klhgG);
    let zzonL = zzonB * 0.5 + (zzonR * 0.5 + zzonG);
    // Rename.
    let bL = bczzL.x;
    let cL = bczzL.y;
    let iL = ijfeL.x;
    let jL = ijfeL.y;
    let fL = ijfeL.z;
    let eL = ijfeL.w;
    let kL = klhgL.x;
    let lL = klhgL.y;
    let hL = klhgL.z;
    let gL = klhgL.w;
    let oL = zzonL.z;
    let nL = zzonL.w;
    // Accumulate for bilinear interpolation.
    //  s t
    //  u v
    var dirLen = vec3<f32>(0.0);
    FsrEasuSetF(&dirLen, (1.0 - pp.x) * (1.0 - pp.y), bL, eL, fL, gL, jL);
    FsrEasuSetF(&dirLen, pp.x * (1.0 - pp.y), cL, fL, gL, hL, kL);
    FsrEasuSetF(&dirLen, (1.0 - pp.x) * pp.y, fL, iL, jL, kL, nL);
    FsrEasuSetF(&dirLen, pp.x * pp.y, gL, jL, kL, lL, oL);
    var dir = dirLen.xy;
    var len = dirLen.z;

    // Normalize with approximation, and cleanup close to zero.
    let dir2 = dir * dir;
    var dirR = dir2.x + dir2.y;
    let zro = dirR < (1.0 / 32768.0);
    dirR = APrxLoRsqF1(dirR);
    dirR = select(dirR, 1.0, zro);
    dir.x = select(dir.x, 1.0, zro);
    dir *= vec2<f32>(dirR);
    // Transform from {0 to 2} to {0 to 1} range, and shape with square.
    len = len * 0.5;
    len *= len;
    // Stretch kernel {1.0 vert|horz, to sqrt(2.0) on diagonal}.
    let stretch = (dir.x * dir.x + dir.y * dir.y) * APrxLoRcpF1(max(abs(dir.x), abs(dir.y)));
    // Anisotropic length after rotation,
    //  x := 1.0 lerp to 'stretch' on edges
    //  y := 1.0 lerp to 2x on edges
    let len2 = vec2<f32>(1.0 + (stretch - 1.0) * len, 1.0 + (-0.5) * len);
    // Based on the amount of 'edge',
    // the window shifts from +/-{sqrt(2.0) to slightly beyond 2.0}.
    let lob = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * len;
    // Set distance^2 clipping point to the end of the adjustable window.
    let clp = APrxLoRcpF1(lob);

    // Accumulation mixed with min/max of 4 nearest.
    //    b c
    //  e f g h
    //  i j k l
    //    n o
    let min4 = min(
        AMin3F3(vec3<f32>(ijfeR.z, ijfeG.z, ijfeB.z), vec3<f32>(klhgR.w, klhgG.w, klhgB.w), vec3<f32>(ijfeR.y, ijfeG.y, ijfeB.y)),
        vec3<f32>(klhgR.x, klhgG.x, klhgB.x),
    );
    let max4 = max(
        AMax3F3(vec3<f32>(ijfeR.z, ijfeG.z, ijfeB.z), vec3<f32>(klhgR.w, klhgG.w, klhgB.w), vec3<f32>(ijfeR.y, ijfeG.y, ijfeB.y)),
        vec3<f32>(klhgR.x, klhgG.x, klhgB.x),
    );
    // Accumulation: `aC` in xyz, `aW` in w.
    var acc = vec4<f32>(0.0);
    FsrEasuTapF(&acc, vec2<f32>(0.0, -1.0) - pp, dir, len2, lob, clp, vec3<f32>(bczzR.x, bczzG.x, bczzB.x)); // b
    FsrEasuTapF(&acc, vec2<f32>(1.0, -1.0) - pp, dir, len2, lob, clp, vec3<f32>(bczzR.y, bczzG.y, bczzB.y)); // c
    FsrEasuTapF(&acc, vec2<f32>(-1.0, 1.0) - pp, dir, len2, lob, clp, vec3<f32>(ijfeR.x, ijfeG.x, ijfeB.x)); // i
    FsrEasuTapF(&acc, vec2<f32>(0.0, 1.0) - pp, dir, len2, lob, clp, vec3<f32>(ijfeR.y, ijfeG.y, ijfeB.y)); // j
    FsrEasuTapF(&acc, vec2<f32>(0.0, 0.0) - pp, dir, len2, lob, clp, vec3<f32>(ijfeR.z, ijfeG.z, ijfeB.z)); // f
    FsrEasuTapF(&acc, vec2<f32>(-1.0, 0.0) - pp, dir, len2, lob, clp, vec3<f32>(ijfeR.w, ijfeG.w, ijfeB.w)); // e
    FsrEasuTapF(&acc, vec2<f32>(1.0, 1.0) - pp, dir, len2, lob, clp, vec3<f32>(klhgR.x, klhgG.x, klhgB.x)); // k
    FsrEasuTapF(&acc, vec2<f32>(2.0, 1.0) - pp, dir, len2, lob, clp, vec3<f32>(klhgR.y, klhgG.y, klhgB.y)); // l
    FsrEasuTapF(&acc, vec2<f32>(2.0, 0.0) - pp, dir, len2, lob, clp, vec3<f32>(klhgR.z, klhgG.z, klhgB.z)); // h
    FsrEasuTapF(&acc, vec2<f32>(1.0, 0.0) - pp, dir, len2, lob, clp, vec3<f32>(klhgR.w, klhgG.w, klhgB.w)); // g
    FsrEasuTapF(&acc, vec2<f32>(1.0, 2.0) - pp, dir, len2, lob, clp, vec3<f32>(zzonR.z, zzonG.z, zzonB.z)); // o
    FsrEasuTapF(&acc, vec2<f32>(0.0, 2.0) - pp, dir, len2, lob, clp, vec3<f32>(zzonR.w, zzonG.w, zzonB.w)); // n

    // Normalize and dering.
    let pix = min(max4, max(min4, acc.xyz * vec3<f32>(ARcpF1(acc.w))));

    return vec4<f32>(from_srgb3(pix), 1.0);
}
