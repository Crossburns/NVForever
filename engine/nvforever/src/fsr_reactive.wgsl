// NVForever's conservative translucency mask. Compare opaque-only HDR with the
// completed scene; water/refraction/particles need less temporal accumulation.
@group(0) @binding(0) var opaque: texture_2d<f32>;
@group(0) @binding(1) var scene: texture_2d<f32>;

@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn fragment(@builtin(position) p: vec4<f32>) -> @location(0) f32 {
    let at = vec2<i32>(p.xy);
    let a = max(textureLoad(opaque, at, 0).rgb, vec3<f32>(0.0));
    let b = max(textureLoad(scene, at, 0).rgb, vec3<f32>(0.0));
    let d = abs(a - b) / max(max(a, b), vec3<f32>(0.05));
    return clamp(max(d.r, max(d.g, d.b)), 0.0, 0.9);
}
