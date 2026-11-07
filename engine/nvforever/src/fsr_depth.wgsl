// NVForever: convert its first-person depth encoding for FSR, then expand the
// original encoding for post effects. Source/output are reverse-Z Depth32Float.
@group(0) @binding(0) var source: texture_depth_2d;
@group(0) @binding(1) var<uniform> resolution: vec4<u32>;

// The share of the depth range first-person shapes are squeezed into: nvf_assets
// nif_material.rs `FIRST_PERSON_DEPTH_SHARE` and first_person_depth.wesl. This plain WGSL
// cannot import that module; fsr.rs `fsr_depth_unsqueezes_with_the_first_person_share`
// fails if the copies differ.
const FIRST_PERSON_DEPTH_SHARE: f32 = 0.015625;

@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn prepare_depth(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {
    let z = textureLoad(source, vec2<i32>(p.xy), 0);
    // nvf_assets::first_person_depth::unsqueeze: a value in the squeezed share is a
    // first-person shape, returned to the depth it was squeezed from.
    if z > 1.0 - FIRST_PERSON_DEPTH_SHARE { return clamp(1.0 - (1.0 - z) / FIRST_PERSON_DEPTH_SHARE, 0.0, 1.0); }
    return z;
}

@fragment fn expand_depth(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {
    let at = min(vec2<u32>(p.xy * vec2<f32>(resolution.xy) / vec2<f32>(resolution.zw)), resolution.xy - vec2<u32>(1u));
    return textureLoad(source, vec2<i32>(at), 0);
}
