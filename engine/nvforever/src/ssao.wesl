// Screen-space ambient occlusion from the depth prepass (ssao.rs; enhanced graphics,
// docs/design/graphics-enhanced.md §3.1). Entry `occlusion` runs at half resolution: for
// each 2 x 2 block of the picture it works out how open the block's top-left pixel is
// (1 open, 0 closed) and writes that with the pixel's distance into a texture of its own;
// entry `apply` blurs that over the 4 x 4 texels its sample pattern repeats over, keeping
// to texels at about this pixel's distance (so nothing bleeds across a depth edge), and
// multiplies the picture by it. An engineering choice, not the original game's.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import nvf_assets::first_person_depth::unsqueeze;

struct Ao {
    // x: radius (game units), y: strength, z: the projection's near plane, w: the
    // distance (game units) beyond which the effect has faded out.
    params: vec4<f32>,
    // x, y: the projection's x and y scale (clip_from_view[0][0], [1][1]); z: the
    // largest radius on the screen (pixels); w: the distance where the fade starts.
    proj: vec4<f32>,
    // x, y: the size the depth prepass was drawn at; z, w: the picture's size. They differ
    // under FSR's upscaling (fsr.rs `prepare`), whose depth is expanded to the picture's
    // size by nearest texels (fsr_depth.wgsl `expand_depth`); all 1 otherwise.
    render_scale: vec4<f32>,
}

@if(MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> ao: Ao;
// `apply` only.
@group(0) @binding(2) var screen: texture_2d<f32>;
@group(0) @binding(3) var screen_sampler: sampler;
// Half resolution: r how open, g the distance (game units).
@group(0) @binding(4) var openness: texture_2d<f32>;

const SAMPLES: i32 = 12;
// Turns of the sample spiral.
const TURNS: f32 = 3.0;
// The distance written for depth 0, and the most any texel holds: half floats end at
// 65,504, and the sky dome lies in the depth prepass at 175,000 units or more. Far beyond
// the fade, so nothing compares against it but other far texels.
const SKY: f32 = 60000.0;

// What `occlusion` writes: how open, and the distance held to SKY.
fn texel(open: f32, dist: f32) -> vec4<f32> {
    return vec4(open, min(dist, SKY), 0.0, 0.0);
}
// Below this cosine between the normal and a sample a surface does not occlude itself.
const BIAS: f32 = 0.12;
const TAU: f32 = 6.2831853;

fn dims() -> vec2<i32> {
    return vec2<i32>(textureDimensions(depth));
}

// The depth prepass's value at a pixel (reverse z: 0 is the far plane, the sky), the
// first-person arms and weapon at the depth they were squeezed from (nvf_assets
// nif_material.rs "First-person depth").
fn depth_at(p: vec2<i32>) -> f32 {
    return unsqueeze(textureLoad(depth, clamp(p, vec2(0), dims() - 1), 0));
}

// The view-space position of pixel `p` at depth value `d` (reverse infinite z:
// distance = near / d), looking down −z.
fn view_pos(p: vec2<f32>, d: f32) -> vec3<f32> {
    let size = vec2<f32>(dims());
    let ndc = vec2(p.x / size.x * 2.0 - 1.0, 1.0 - p.y / size.y * 2.0);
    let z = ao.params.z / max(d, 1e-7);
    return vec3(ndc.x * z / ao.proj.x, ndc.y * z / ao.proj.y, -z);
}

// The texel of the depth as drawn that the pixel centred at `p` shows, as fsr_depth.wgsl
// `expand_depth` picks it; without upscaling, the pixel itself.
fn drawn_texel(p: vec2<f32>) -> vec2<f32> {
    return floor(p * ao.render_scale.xy / ao.render_scale.zw);
}

// The centre of drawn texel `t`, in pixels of the picture.
fn texel_centre(t: vec2<f32>) -> vec2<f32> {
    return (t + 0.5) * ao.render_scale.zw / ao.render_scale.xy;
}

// The view-space position at the centre of drawn texel `t`, its depth read at a pixel
// that shows it.
fn texel_pos(t: vec2<f32>) -> vec3<f32> {
    let c = texel_centre(t);
    return view_pos(c, depth_at(vec2<i32>(floor(c))));
}

// The view-space normal at pixel `px`, whose depth is `d`: from the neighbouring texels of
// the depth as drawn, on each axis the side with the smaller step in depth, so an edge
// does not bend it. Without upscaling a texel is a pixel. Under FSR's upscaling a texel
// covers 1.5 to 3 pixels, so neighbouring pixels often held this pixel's own depth: the
// normal faced the camera and every surface seen at an angle occluded itself, a grey veil
// over the ground and walls [observed 2026-11-06, FSR Performance at Goodsprings: mean
// darkening 4.2%, against 0.7% without FSR].
fn normal_at(px: vec2<i32>, d: f32) -> vec3<f32> {
    let t = drawn_texel(vec2<f32>(px) + 0.5);
    let p = view_pos(texel_centre(t), d);
    let l = texel_pos(t - vec2(1.0, 0.0));
    let r = texel_pos(t + vec2(1.0, 0.0));
    let u = texel_pos(t - vec2(0.0, 1.0));
    let b = texel_pos(t + vec2(0.0, 1.0));
    let dx = select(p - l, r - p, abs(r.z - p.z) < abs(p.z - l.z));
    let dy = select(p - u, b - p, abs(b.z - p.z) < abs(p.z - u.z));
    var n = normalize(cross(dx, dy));
    if dot(n, -p) < 0.0 {
        n = -n;
    }
    return n;
}

// The 4 x 4 ordered pattern that turns each pixel's spiral (0..1).
fn pattern(p: vec2<i32>) -> f32 {
    var b = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let i = (p.y & 3) * 4 + (p.x & 3);
    return (b[i] + 0.5) / 16.0;
}

@fragment
fn occlusion(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let half = vec2<i32>(floor(in.position.xy));
    let px = half * 2;
    let d = depth_at(px);
    if d <= 0.0 {
        return texel(1.0, SKY);
    }
    let c = vec2<f32>(px) + 0.5;
    let p = view_pos(c, d);
    let dist = -p.z;
    let fade = 1.0 - smoothstep(ao.proj.w, ao.params.w, dist);
    if fade <= 0.0 {
        return texel(1.0, dist);
    }
    let n = normal_at(px, d);
    let radius = ao.params.x;
    let size = vec2<f32>(dims());
    let on_screen = min(radius * ao.proj.y * 0.5 * size.y / dist, ao.proj.z);
    if on_screen < 1.0 {
        return texel(1.0, dist);
    }
    // Turned by the half-resolution texel, so the 4 x 4 blur in `apply` sees every turn.
    let turn = pattern(half) * TAU;
    var sum = 0.0;
    for (var i = 0; i < SAMPLES; i++) {
        let a = (f32(i) + 0.5) / f32(SAMPLES);
        let angle = a * TURNS * TAU + turn;
        let q = c + vec2(cos(angle), sin(angle)) * a * on_screen;
        let qd = depth_at(vec2<i32>(floor(q)));
        if qd <= 0.0 {
            continue;
        }
        let v = view_pos(floor(q) + 0.5, qd) - p;
        let vv = dot(v, v);
        let cosine = dot(v, n) * inverseSqrt(max(vv, 1e-4));
        // Nothing from beyond the radius, full weight within half of it.
        let range = 1.0 - smoothstep(0.25, 1.0, vv / (radius * radius));
        sum += max(cosine - BIAS, 0.0) * range;
    }
    let open = clamp(1.0 - ao.params.y * sum / f32(SAMPLES) * 2.0, 0.0, 1.0);
    return texel(mix(1.0, open, fade), dist);
}

@fragment
fn apply(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(screen, screen_sampler, in.uv);
    let px = vec2<i32>(floor(in.position.xy));
    let d = depth_at(px);
    if d <= 0.0 {
        return src;
    }
    let z = ao.params.z / d;
    let size = vec2<i32>(textureDimensions(openness));
    // The 4 x 4 texels around this pixel: texel q stands for full pixel 2q.
    let first = (px >> vec2(1u)) - 2 + (px & vec2(1));
    var total = 0.0;
    var weight = 0.0;
    // The texel nearest this pixel's distance, for a pixel no texel shares it with (a
    // pole or a grass blade a pixel wide, a silhouette pixel in an odd row or column).
    var nearest = 1.0;
    var nearest_gap = 3.4e38;
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let t = textureLoad(openness, clamp(first + vec2(x, y), vec2(0), size - 1), 0);
            let gap = abs(t.g - z);
            // Only texels within a few per cent of this pixel's distance.
            let w = max(1.0 - gap / (0.05 * z + 1.0), 0.0);
            total += t.r * w;
            weight += w;
            if gap < nearest_gap {
                nearest_gap = gap;
                nearest = t.r;
            }
        }
    }
    let open = select(nearest, total / weight, weight > 0.0);
    return vec4(src.rgb * open, src.a);
}
