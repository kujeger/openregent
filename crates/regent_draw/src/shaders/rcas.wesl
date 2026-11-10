// FSR 1's RCAS (Robust Contrast Adaptive Sharpening), `FsrRcasF`, the
// 32-bit path, in WGSL: the EASU image sharpened as much as its 3x3
// neighbourhood allows without clipping, times `con.x`.
// `regent_draw::present`.
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
// - `FsrRcasCon` runs on the CPU (`present::rcas_con`).
// - The EASU image is `Rgba8UnormSrgb`, so a load returns linear light;
//   `FsrRcasInputF` encodes sRGB, as EASU's output was, and the output
//   is decoded again for the sRGB target.
// - `FsrRcasLoadF` clamps to the image's edge.
// - `ip` is the fragment's pixel less the map rect's corner.
// - No `FSR_RCAS_DENOISE` or `FSR_RCAS_PASSTHROUGH_ALPHA`, the header's
//   defaults; no 16-bit path.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
import regent_draw::shaders::srgb::{to_srgb, from_srgb3};

struct Rcas {
    con: vec4<u32>,
    // The map rect's corner, in the window's pixels.
    corner: vec2<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> rcas: Rcas;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var picture: texture_2d<f32>;

// This is set at the limit of providing unnatural results for sharpening.
const FSR_RCAS_LIMIT: f32 = 0.25 - (1.0 / 16.0);

// The input callbacks.
fn FsrRcasLoadF(p: vec2<i32>) -> vec4<f32> {
    let most = vec2<i32>(textureDimensions(picture)) - vec2<i32>(1);

    return textureLoad(picture, clamp(p, vec2<i32>(0), most), 0);
}

fn FsrRcasInputF(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(to_srgb(c.r), to_srgb(c.g), to_srgb(c.b));
}

// ffx_a.h.
fn APrxMedRcpF1(a: f32) -> f32 {
    let b = bitcast<f32>(0x7ef19fffu - bitcast<u32>(a));

    return b * (-b * a + 2.0);
}

fn ARcpF1(x: f32) -> f32 {
    return 1.0 / x;
}

fn AMin3F1(x: f32, y: f32, z: f32) -> f32 {
    return min(x, min(y, z));
}

fn AMax3F1(x: f32, y: f32, z: f32) -> f32 {
    return max(x, max(y, z));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let ip = vec2<u32>(mesh.position.xy - rcas.corner);

    // Algorithm uses minimal 3x3 pixel neighborhood.
    //    b
    //  d e f
    //    h
    let sp = vec2<i32>(ip);
    let b = FsrRcasInputF(FsrRcasLoadF(sp + vec2<i32>(0, -1)).rgb);
    let d = FsrRcasInputF(FsrRcasLoadF(sp + vec2<i32>(-1, 0)).rgb);
    let e = FsrRcasInputF(FsrRcasLoadF(sp).rgb);
    let f = FsrRcasInputF(FsrRcasLoadF(sp + vec2<i32>(1, 0)).rgb);
    let h = FsrRcasInputF(FsrRcasLoadF(sp + vec2<i32>(0, 1)).rgb);
    // Rename (32-bit).
    let bR = b.r;
    let bG = b.g;
    let bB = b.b;
    let dR = d.r;
    let dG = d.g;
    let dB = d.b;
    let eR = e.r;
    let eG = e.g;
    let eB = e.b;
    let fR = f.r;
    let fG = f.g;
    let fB = f.b;
    let hR = h.r;
    let hG = h.g;
    let hB = h.b;
    // Min and max of ring.
    let mn4R = min(AMin3F1(bR, dR, fR), hR);
    let mn4G = min(AMin3F1(bG, dG, fG), hG);
    let mn4B = min(AMin3F1(bB, dB, fB), hB);
    let mx4R = max(AMax3F1(bR, dR, fR), hR);
    let mx4G = max(AMax3F1(bG, dG, fG), hG);
    let mx4B = max(AMax3F1(bB, dB, fB), hB);
    // Immediate constants for peak range.
    let peakC = vec2<f32>(1.0, -1.0 * 4.0);
    // Limiters, these need to be high precision RCPs.
    let hitMinR = min(mn4R, eR) * ARcpF1(4.0 * mx4R);
    let hitMinG = min(mn4G, eG) * ARcpF1(4.0 * mx4G);
    let hitMinB = min(mn4B, eB) * ARcpF1(4.0 * mx4B);
    let hitMaxR = (peakC.x - max(mx4R, eR)) * ARcpF1(4.0 * mn4R + peakC.y);
    let hitMaxG = (peakC.x - max(mx4G, eG)) * ARcpF1(4.0 * mn4G + peakC.y);
    let hitMaxB = (peakC.x - max(mx4B, eB)) * ARcpF1(4.0 * mn4B + peakC.y);
    let lobeR = max(-hitMinR, hitMaxR);
    let lobeG = max(-hitMinG, hitMaxG);
    let lobeB = max(-hitMinB, hitMaxB);
    let lobe = max(-FSR_RCAS_LIMIT, min(AMax3F1(lobeR, lobeG, lobeB), 0.0)) * bitcast<f32>(rcas.con.x);
    // Resolve, which needs the medium precision rcp approximation to avoid visible tonality changes.
    let rcpL = APrxMedRcpF1(4.0 * lobe + 1.0);
    let pixR = (lobe * bR + lobe * dR + lobe * hR + lobe * fR + eR) * rcpL;
    let pixG = (lobe * bG + lobe * dG + lobe * hG + lobe * fG + eG) * rcpL;
    let pixB = (lobe * bB + lobe * dB + lobe * hB + lobe * fB + eB) * rcpL;

    return vec4<f32>(from_srgb3(vec3<f32>(pixR, pixG, pixB)), 1.0);
}
