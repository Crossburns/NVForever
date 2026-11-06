//! Screen-space ambient occlusion of the enhanced graphics (`graphics.rs`,
//! docs/design/graphics-enhanced.md §3.1): creases, corners and the ground under things
//! are darkened by how much of the space around them the depth prepass shows filled.
//!
//! Bevy's own SSAO needs a normal prepass and a view without multisampling; the game's
//! views are multisampled (alpha-tested plants are smoothed with it) and the NIF, grass,
//! tree and water prepasses write depth only. So this pass works from the depth alone:
//! normals from the neighbouring depths, a 12-sample spiral turned by a 4 x 4 pattern,
//! worked out at half resolution (one pixel of each 2 x 2 block, a quarter of the
//! work: about 1.0-1.25 ms a frame at 1920x1080 at full resolution, #1370), the result
//! blurred over those 4 x 4 texels at about the same distance and multiplied
//! into the picture after tonemapping, before the image-space grade and its blur
//! (imagespace.rs). It darkens all light, not only the ambient light as the lighting
//! itself could [engineering choice: the strength is kept low for that].
//!
//! On the main camera only, while it carries [`AmbientOcclusion`].

use crate::fly::FlyCam;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode,
    FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderStages, ShaderType, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

/// Radius of the occlusion around a point, in game units (about 0.9 m) [engineering
/// choice: the size of a doorstep or a crate's foot; larger darkened whole walls].
const RADIUS: f32 = 64.0;
/// How strongly a fully closed point is darkened (0..1) [engineering choice, by eye].
const STRENGTH: f32 = 0.55;
/// Largest radius on the screen, in pixels (near things, e.g. the player's arms).
const MAX_SCREEN_RADIUS: f32 = 80.0;
/// The effect fades out between these distances (game units) [engineering choice: beyond
/// them the radius is a pixel or two and the fog takes over].
const FADE_START: f32 = 4000.0;
const FADE_END: f32 = 7000.0;

/// Ambient occlusion on a camera; the projection part is kept up to date by
/// [`follow_projection`].
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
#[extract_app(bevy::render::RenderApp)]
pub struct AmbientOcclusion {
    /// Radius, strength, near plane, fade end.
    pub params: Vec4,
    /// Projection x and y scale, largest screen radius, fade start.
    pub proj: Vec4,
    /// The size the depth prepass was drawn at (x, y) and the picture's size (z, w): all 1
    /// here; set in the render world while FSR upscales (`fsr.rs` `prepare`), which expands
    /// that depth to the picture's size, so the normals are taken from the texels as drawn
    /// (ssao.wesl `normal_at`). The extraction puts 1 back every frame.
    pub render_scale: Vec4,
}

impl Default for AmbientOcclusion {
    fn default() -> Self {
        AmbientOcclusion {
            params: Vec4::new(RADIUS, STRENGTH, 5.0, FADE_END),
            proj: Vec4::new(1.0, 1.0, MAX_SCREEN_RADIUS, FADE_START),
            render_scale: Vec4::ONE,
        }
    }
}

pub struct SsaoPlugin;

impl Plugin for SsaoPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "ssao.wesl");
        app.add_plugins((ExtractComponentPlugin::<AmbientOcclusion>::default(), UniformComponentPlugin::<AmbientOcclusion>::default()))
            .add_systems(PostUpdate, follow_projection.after(bevy::camera::CameraUpdateSystems));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(Core3d, ao_pass.in_set(Core3dSystems::PostProcess).after(tonemapping).before(bevy::anti_alias::fxaa::fxaa).before(bevy::anti_alias::smaa::smaa).before(crate::imagespace::blur_pass));
    }
}

/// The camera's projection into the uniform (the aspect follows the window).
fn follow_projection(mut cams: Query<(&Projection, &mut AmbientOcclusion), With<FlyCam>>) {
    for (proj, mut ao) in &mut cams {
        let Projection::Perspective(p) = proj else { continue };
        let y = 1.0 / (p.fov * 0.5).tan();
        let x = y / p.aspect_ratio.max(1e-3);
        let params = Vec4::new(RADIUS, STRENGTH, p.near, FADE_END);
        let projv = Vec4::new(x, y, MAX_SCREEN_RADIUS, FADE_START);
        if ao.params != params || ao.proj != projv {
            ao.params = params;
            ao.proj = projv;
        }
    }
}

#[derive(Resource)]
pub(crate) struct AoPipelines {
    /// The occlusion pass's layouts with a single-sampled and a multisampled depth.
    occlusion_layouts: [BindGroupLayoutDescriptor; 2],
    /// The apply pass's.
    apply_layouts: [BindGroupLayoutDescriptor; 2],
    sampler: Sampler,
    occlusion_base: RenderPipelineDescriptor,
    apply_base: RenderPipelineDescriptor,
    /// Occlusion pipeline by multisampling; apply pipeline by target format and it.
    occlusion: HashMap<bool, CachedRenderPipelineId>,
    apply: HashMap<(TextureFormat, bool), CachedRenderPipelineId>,
}

/// A view's pipelines, whether its depth is multisampled, and its occlusion texture.
#[derive(Component)]
pub(crate) struct AoView {
    occlusion: CachedRenderPipelineId,
    apply: CachedRenderPipelineId,
    multi: bool,
    /// The picture's format the apply pipeline was built for.
    format: TextureFormat,
    texture: CachedTexture,
}

/// How open a texel is, and its distance (the apply pass weighs texels by it).
const OCCLUSION_FORMAT: TextureFormat = TextureFormat::Rg16Float;

fn init_pipelines(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let occlusion_layout = |multi: bool| {
        let depth = if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new("ssao_occlusion_layout", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (depth, uniform_buffer::<AmbientOcclusion>(true))))
    };
    let apply_layout = |multi: bool| {
        let depth = if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new(
            "ssao_apply_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    depth,
                    uniform_buffer::<AmbientOcclusion>(true),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                ),
            ),
        )
    };
    let shader = assets.load("embedded://nvforever/ssao.wesl");
    let base = |label: &'static str, entry: &'static str| RenderPipelineDescriptor {
        label: Some(label.into()),
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: shader.clone(), entry_point: Some(entry.into()), ..default() }),
        ..default()
    };
    let sampler = device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() });
    commands.insert_resource(AoPipelines {
        occlusion_layouts: [occlusion_layout(false), occlusion_layout(true)],
        apply_layouts: [apply_layout(false), apply_layout(true)],
        sampler,
        occlusion_base: base("ssao_occlusion_pipeline", "occlusion"),
        apply_base: base("ssao_apply_pipeline", "apply"),
        occlusion: HashMap::default(),
        apply: HashMap::default(),
    });
}

fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<AoPipelines>,
    mut textures: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    views: Query<(Entity, &ExtractedView, &Msaa), With<AmbientOcclusion>>,
) {
    for (entity, view, msaa) in &views {
        let multi = msaa.samples() > 1;
        let format = view.target_format;
        let occlusion = match pipelines.occlusion.get(&multi) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.occlusion_base.clone();
                d.layout = vec![pipelines.occlusion_layouts[multi as usize].clone()];
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format: OCCLUSION_FORMAT, blend: None, write_mask: ColorWrites::ALL })];
                    if multi {
                        f.shader_defs.push("MULTISAMPLED".into());
                    }
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.occlusion.insert(multi, id);
                id
            }
        };
        let apply = match pipelines.apply.get(&(format, multi)) {
            Some(id) => *id,
            None => {
                let mut d = pipelines.apply_base.clone();
                d.layout = vec![pipelines.apply_layouts[multi as usize].clone()];
                if let Some(f) = d.fragment.as_mut() {
                    f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
                    if multi {
                        f.shader_defs.push("MULTISAMPLED".into());
                    }
                }
                let id = cache.queue_render_pipeline(d);
                pipelines.apply.insert((format, multi), id);
                id
            }
        };
        // Half resolution, rounded up (texel q stands for the picture's pixel 2q).
        let size = ((view.viewport.zw() + UVec2::ONE) / 2).max(UVec2::ONE);
        let texture = textures.get(
            &device,
            TextureDescriptor {
                label: Some("ssao_openness"),
                size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: OCCLUSION_FORMAT,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        commands.entity(entity).insert(AoView { occlusion, apply, multi, format, texture });
    }
}

/// Whether a pipeline built for `format` may draw into the view's picture now. In the
/// frame the view turns HDR or back (the light glow switched, `graphics.rs`) the view's
/// format and its main texture can disagree for a frame; a pipeline set on a pass of
/// the other format is a validation error that ends the run, so the pass is skipped.
pub(crate) fn format_matches(target: &ViewTarget, format: TextureFormat) -> bool {
    target.main_texture_format() == format
}

/// Whether the view's depth prepass texture is multisampled this frame. Read from the
/// texture itself: in the frame the multisampling changes (temporal anti-aliasing switched
/// on or off, `graphics.rs`) the view's `Msaa` and its prepass texture may disagree, and a
/// pass bound to the other kind of texture is a validation error that ends the run.
pub(crate) fn depth_multisampled(prepass: &ViewPrepassTextures) -> Option<bool> {
    prepass.depth.as_ref().map(|d| d.texture.texture.sample_count() > 1)
}

/// The occlusion into the view's texture, then the picture multiplied by it.
pub(crate) fn ao_pass(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<AmbientOcclusion>, &AoView, &ViewPrepassTextures)>,
    pipelines: Res<AoPipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<AmbientOcclusion>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, index, ao, prepass) = view.into_inner();
    if depth_multisampled(prepass) != Some(ao.multi) || !format_matches(target, ao.format) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let (Some(occlusion), Some(apply)) = (cache.get_render_pipeline(ao.occlusion), cache.get_render_pipeline(ao.apply)) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    // GPU time of the pass (recorded with --log-frame-times, graphics.rs).
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_ambient_occlusion");
    let layout = cache.get_bind_group_layout(&pipelines.occlusion_layouts[ao.multi as usize]);
    let bind_group = device.create_bind_group("ssao_occlusion_bind_group", &layout, &BindGroupEntries::sequential((depth, binding.clone())));
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("ssao_occlusion_pass"),
            color_attachments: &[Some(RenderPassColorAttachment { view: &ao.texture.default_view, depth_slice: None, resolve_target: None, ops: Operations::default() })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(occlusion);
        pass.set_bind_group(0, &bind_group, &[index.index()]);
        pass.draw(0..3, 0..1);
    }
    let post = target.post_process_write();
    let layout = cache.get_bind_group_layout(&pipelines.apply_layouts[ao.multi as usize]);
    let bind_group = device.create_bind_group(
        "ssao_apply_bind_group",
        &layout,
        &BindGroupEntries::sequential((depth, binding, post.source, &pipelines.sampler, &ao.texture.default_view)),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("ssao_apply_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(apply);
    pass.set_bind_group(0, &bind_group, &[index.index()]);
    pass.draw(0..3, 0..1);
    drop(pass);
    span.end(ctx.command_encoder());
}
