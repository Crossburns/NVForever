// Grass (grass.rs): vertex shader for the main pass and the prepass (depth), so both see
// the same motion, and the main pass's fragment shader.
//
// A grass mesh holds every plant of one type in a quarter of a cell. UV_1 holds, per
// vertex, the sway weight (the model's vertex alpha: 0 at the foot, more towards the
// tips) and the height above the plant's foot (game units).
//
// Wind: a vertex moves along the wind by its sway weight × the wind's magnitude, in waves
// that travel across the ground, so neighbouring plants move nearly together and distant
// ones do not. The wave speed comes from the grass type's wave period and the magnitude
// from the weather's wind speed (grass.md §5).
//
// Distance: beyond the grass distance the plants sink into the ground, each vertex by its
// own height (so the plant flattens) and a little more, over the fade range. The game
// fades its grass out over that range; how it does so is not known (grass.md §5).
//
// Edges (fragment, GRASS_COVERAGE: the view is multisampled): the texture's alpha is cut
// at the model's alpha-test threshold, as in the depth prepass, but texels below the
// threshold are not dropped: they cover a share of the pixel's samples, alpha / threshold
// (alpha to coverage). Strands end softly instead of in hard steps, and thin strands
// whose alpha the smaller mip levels have averaged down stay visible at a distance. The
// share is 1 wherever the prepass wrote depth, so nothing behind is wrongly hidden.
// Without multisampling the alpha is tested as before.
//
// Light (fragment): lit with the ground's normal (grass.rs), but with the ambient light
// and the sun weighted by `light` (grass.md §5).

import bevy_pbr::{
    render::mesh_functions,
    render::mesh_view_bindings::view,
    render::view_transformations::position_world_to_clip,
};
import bevy_render::globals::Globals;
@if(PREPASS_PIPELINE) import bevy_pbr::prepass::io::{Vertex, VertexOutput};
@if(!PREPASS_PIPELINE) import bevy_pbr::render::forward_io::{Vertex, VertexOutput, FragmentOutput};
@if(!PREPASS_PIPELINE) import bevy_pbr::render::mesh_view_bindings::globals;
@if(!PREPASS_PIPELINE) import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;
@if(!PREPASS_PIPELINE) import bevy_pbr::render::pbr_functions::{nvf_apply_pbr_lighting, main_pass_post_lighting_processing};
@if(!PREPASS_PIPELINE && VISIBILITY_RANGE_DITHER) import bevy_pbr::render::pbr_functions::visibility_range_dither;
@if(PREPASS_PIPELINE && MOTION_VECTOR_PREPASS) import bevy_pbr::prepass::bindings::previous_view_uniforms;
// The prepass view bind group holds the globals at binding 1 (the main pass at 11).
@if(PREPASS_PIPELINE) @group(0) @binding(1) var<uniform> globals: Globals;

struct GrassParams {
    // xyz: wind direction (world, horizontal, unit); w: how far a vertex with sway
    // weight 1 moves (game units).
    wind: vec4<f32>,
    // x: wave angular speed (rad/s); y: distance at which the grass starts to go; z:
    // distance over which it goes; w: unused.
    motion: vec4<f32>,
    // x: factor on the ambient light; y: factor on every other light (the sun).
    light: vec4<f32>,
    previous_wind: vec4<f32>,
    previous_motion: vec4<f32>,
    change_frame: vec4<u32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> grass: GrassParams;

// How far below the ground a plant's foot ends up once it is gone (game units).
const SINK: f32 = 24.0;
// Wave length of the wind's waves along and across the wind (game units) [chosen].
const WAVE_ALONG: f32 = 900.0;
const WAVE_ACROSS: f32 = 2300.0;

// Evaluate the identical deformation at either frame's model, time and camera.
@if(VERTEX_UVS_B)
fn deform(base: vec3<f32>, uv_b: vec2<f32>, wind: vec4<f32>, motion: vec4<f32>, time: f32, camera: vec3<f32>) -> vec3<f32> {
    var world = base;
    let along = wind.xyz;
    let across = vec3<f32>(-along.z, 0.0, along.x);
    let phase = 6.2831853 * (dot(world, along) / WAVE_ALONG + dot(world, across) / WAVE_ACROSS);
    let t = time * motion.x;
    let swing = 0.5 + 0.35 * sin(t - phase) + 0.15 * sin(2.31 * t - 1.7 * phase);
    world += along * (wind.w * uv_b.x * swing);
    let gone = clamp((distance(camera, world) - motion.y) / motion.z, 0.0, 1.0);
    world.y -= gone * (uv_b.y + SINK);
    return world;
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0)).xyz;

    @if(VERTEX_UVS_B) world = deform(world, vertex.uv_b, grass.wind, grass.motion, globals.time, view.world_position.xyz);
    @if(VERTEX_UVS_B) out.uv_b = vertex.uv_b;

    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
    @if(VERTEX_UVS_A) out.uv = vertex.uv;

    @if(PREPASS_PIPELINE && UNCLIPPED_DEPTH_ORTHO_EMULATION) out.unclipped_depth = out.position.z;
    @if(PREPASS_PIPELINE && UNCLIPPED_DEPTH_ORTHO_EMULATION) out.position.z = min(out.position.z, 1.0);
    @if(PREPASS_PIPELINE && NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS) out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
    @if(PREPASS_PIPELINE && NORMAL_PREPASS_OR_DEFERRED_PREPASS && !VERTEX_NORMALS) out.world_normal = view.world_from_view[2].xyz;
    @if(PREPASS_PIPELINE && MOTION_VECTOR_PREPASS) {
        let previous_model = mesh_functions::get_previous_world_from_local(vertex.instance_index);
        var previous_world = mesh_functions::mesh_position_local_to_world(previous_model, vec4(vertex.position, 1.0)).xyz;
        @if(VERTEX_UVS_B) {
            // The view transform is rigid: its transposed rotation and translation
            // recover the previous camera position without a matrix inverse.
            let v = previous_view_uniforms.view_from_world;
            let rotation = transpose(mat3x3(v[0].xyz, v[1].xyz, v[2].xyz));
            let camera = -(rotation * v[3].xyz);
            let changed = globals.frame_count == grass.change_frame.x;
            let wind = select(grass.wind, grass.previous_wind, changed);
            let motion = select(grass.motion, grass.previous_motion, changed);
            let previous_time_unwrapped = globals.time - globals.delta_time;
            let previous_time = select(previous_time_unwrapped, previous_time_unwrapped + 3600.0, previous_time_unwrapped < 0.0);
            previous_world = deform(previous_world, vertex.uv_b, wind, motion, previous_time, camera);
        }
        out.previous_world_position = vec4(previous_world, 1.0);
    }
    @if(!PREPASS_PIPELINE && VERTEX_NORMALS) out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);

    @if(VERTEX_COLORS) out.color = vertex.color;
    @if(VERTEX_OUTPUT_INSTANCE_INDEX) out.instance_index = vertex.instance_index;
    @if(VISIBILITY_RANGE_DITHER) out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, world_from_local[3]);
    return out;
}

@if(!PREPASS_PIPELINE) @fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    @if(!PREPASS_PIPELINE && VISIBILITY_RANGE_DITHER) visibility_range_dither(in.position, in.visibility_range_dither);
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let cutoff = max(pbr_input.material.alpha_cutoff, 0.01);
    let alpha = pbr_input.material.base_color.a;
    @if(!PREPASS_PIPELINE && GRASS_COVERAGE) let coverage = saturate(alpha / cutoff);
    @if(!PREPASS_PIPELINE && GRASS_COVERAGE) if coverage < 0.03 {
        discard;
    }
    @if(!PREPASS_PIPELINE && !GRASS_COVERAGE) let coverage = 1.0;
    @if(!PREPASS_PIPELINE && !GRASS_COVERAGE) if alpha < cutoff {
        discard;
    }
    pbr_input.material.base_color.a = 1.0;
    var out: FragmentOutput;
    // Grass weighs the ambient light and the sun differently from other surfaces
    // (grass.md §5): the two parts of the display-value lighting sum (nvforever lights.rs
    // DISPLAY_HELPERS) are scaled, each by its factor's display value, so that either part
    // alone is scaled by the factor as light.
    let lit = nvf_apply_pbr_lighting(pbr_input, pow(max(grass.light.xy, vec2(0.0)), vec2(1.0 / 2.2))).rgb;
    out.color = vec4(lit, 1.0);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    out.color.a = coverage;
    return out;
}
