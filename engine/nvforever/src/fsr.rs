//! FSR 3.1.4 temporal upscaling through the MIT-licensed wgpu-ffx source port.
//! See docs/design/fsr-upscaling.md for the render contract, measurements and limits.
//!
//! The scene is rendered into the top-left, lower-resolution part of Bevy's HDR
//! target. FSR reconstructs the entire target before bloom, tonemapping and UI.
//! Opaque/transparent colour differences supply a conservative reactive mask; the sky's
//! blended parts and the terrain's blended texture layers count as opaque there
//! ([`reference_sky_and_terrain_layers`]).
//! The depth prepass is expanded afterwards for NVForever's screen-space effects.

use std::{borrow::Cow, ops::{Deref, Range}, sync::{atomic::{AtomicBool, Ordering}, Mutex}};

use bevy::camera::{CameraMainTextureUsages, Hdr, MainPassResolutionOverride, Viewport};
use bevy::core_pipeline::{
    core_3d::{main_opaque_pass_3d, main_transparent_pass_3d, Transparent3d},
    prepass::{DepthPrepass, MotionVectorPrepass, ViewPrepassTextures},
    Core3d, Core3dSystems,
};
use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::render::{
    camera::{ExtractedCamera, MipBias, TemporalJitter},
    diagnostic::RecordDiagnostics,
    render_phase::ViewSortedRenderPhases,
    renderer::{RenderAdapter, RenderContext, RenderDevice, RenderQueue, ViewQuery},
    sync_world::RenderEntity,
    view::{prepare_view_targets, ExtractedView, ViewDepthStencilTexture, ViewTarget},
    Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
};
use wgpu::util::DeviceExt;
use wgpu_ffx::{FsrContext, FsrContextFlags, FsrContextInfo, FsrDispatchFlags, FsrDispatchInfo, FsrView};

/// AMD's standard fixed-resolution modes. Native AA reconstructs at 1:1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum FsrQuality {
    NativeAa,
    #[default]
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
}

impl FsrQuality {
    pub fn ratio(self) -> f32 {
        match self {
            Self::NativeAa => 1.0,
            Self::Quality => 1.5,
            Self::Balanced => 1.7,
            Self::Performance => 2.0,
            Self::UltraPerformance => 3.0,
        }
    }

    pub fn render_size(self, output: UVec2) -> UVec2 {
        (output.as_vec2() / self.ratio()).as_uvec2().max(UVec2::ONE)
    }
}

/// Enable FSR on a perspective HDR camera. The graphics settings enforce Msaa::Off
/// and remove competing temporal/spatial anti-aliasing components.
#[derive(Component, Clone, Copy, Debug)]
#[require(Hdr, DepthPrepass, MotionVectorPrepass, TemporalJitter, MipBias)]
pub struct FsrUpscaling {
    pub quality: FsrQuality,
    pub sharpness: f32,
}

impl Default for FsrUpscaling {
    fn default() -> Self {
        Self { quality: FsrQuality::Quality, sharpness: 0.2 }
    }
}

#[derive(Component, Clone, Copy)]
struct FsrCamera {
    settings: FsrUpscaling,
    translation: Vec3,
    rotation: Quat,
    near: f32,
    fov: f32,
    frame: u32,
    delta_ms: f32,
    loading_generation: u32,
    play_state: Option<nvf_play::PlayState>,
}

/// The most the field of view may magnify or widen the view in one frame (the ratio of
/// tan(fov / 2) to the previous frame's) while FSR keeps its history. Bevy computes the
/// motion vectors from this and the previous frame's projections, so a smooth zoom, such as
/// the aim zoom's 0.25 s ease (`nvf_play::aim_zoom`), reprojects like any camera move.
/// Resetting on every changed frame threw the history away for the whole zoom, in and out
/// [observed 2026-11-06: in an aim zoom capture all 18 frames whose field of view changed
/// reset]. A jump (the console's `fov`, a cut to another lens) still resets. [engineering
/// choice: 2 keeps the history through the aim zoom of a 20° sight at 15 frames a second
/// and up, `an_aim_zoom_keeps_the_history_and_a_lens_jump_resets_it`.]
const LENS_CUT_RATIO: f32 = 2.0;

impl FsrCamera {
    fn cut_from(&self, previous: &Self) -> bool {
        let zoom = (self.fov * 0.5).tan() / (previous.fov * 0.5).tan();
        self.frame != previous.frame.wrapping_add(1)
            || self.translation.distance_squared(previous.translation) >= 400.0 * 400.0
            || self.rotation.angle_between(previous.rotation).abs() > std::f32::consts::FRAC_PI_4
            || !(1.0 / LENS_CUT_RATIO..=LENS_CUT_RATIO).contains(&zoom)
            || self.near != previous.near
            || self.settings.quality != previous.settings.quality
            || self.delta_ms > 250.0
            || self.loading_generation != previous.loading_generation
            || self.play_state != previous.play_state
    }
}

pub struct FsrPlugin;

/// Whether this device can execute the audited Vulkan SPIR-V shader path.
#[derive(Resource, Clone, Copy, Default)]
pub struct FsrSupport(pub bool);

impl Plugin for FsrPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<FsrRenderer>()
            .add_systems(ExtractSchedule, extract)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareViews).before(prepare_view_targets))
            .add_systems(
                Core3d,
                capture_opaque
                    .in_set(Core3dSystems::MainPass)
                    .after(main_opaque_pass_3d)
                    .before(bevy::pbr::main_transmissive_pass_3d)
                    .before(main_transparent_pass_3d),
            )
            .add_systems(
                Core3d,
                reference_sky_and_terrain_layers
                    .in_set(Core3dSystems::MainPass)
                    .after(capture_opaque)
                    .after(bevy::pbr::main_transmissive_pass_3d)
                    .before(main_transparent_pass_3d),
            )
            .add_systems(
                Core3d,
                fsr_pass
                    .in_set(Core3dSystems::EarlyPostProcess)
                    .before(bevy::post_process::bloom::bloom)
                    .before(crate::post::bright_pass_hdr),
            );
    }

    fn finish(&self, app: &mut App) {
        let supported = app.get_sub_app(RenderApp).is_some_and(|render| {
            let world = render.world();
            let (Some(adapter), Some(device)) = (world.get_resource::<RenderAdapter>(), world.get_resource::<RenderDevice>()) else { return false };
            let features = wgpu::Features::PASSTHROUGH_SHADERS | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
                | wgpu::Features::FLOAT32_FILTERABLE | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM | wgpu::Features::CLEAR_TEXTURE;
            adapter.get_info().backend == wgpu::Backend::Vulkan && device.features().contains(features) && supports_limits(&device.limits())
                && [wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::R16Float, wgpu::TextureFormat::Rg16Float,
                    wgpu::TextureFormat::Rgba16Float, wgpu::TextureFormat::Rgba32Float]
                    .into_iter().all(|format| {
                        let properties = adapter.get_texture_format_features(format);
                        properties.allowed_usages.contains(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING)
                            && properties.flags.contains(wgpu::TextureFormatFeatureFlags::STORAGE_READ_WRITE)
                    })
        });
        app.insert_resource(FsrSupport(supported));
        if !supported {
            info!("FSR 3.1 unavailable: Vulkan native SPIR-V and the required storage/filterable texture formats are needed; using native rendering");
        }
    }
}

/// Maxima from the pinned port's pass.rs and full-precision SPD shader: 256
/// threads; four 16x16 float shared arrays plus a uint; eight storage textures;
/// nine sampled textures; fifteen total bindings; 240 bytes of constants.
fn supports_limits(limits: &wgpu::Limits) -> bool {
    limits.max_compute_invocations_per_workgroup >= 256
        && limits.max_compute_workgroup_size_x >= 256
        && limits.max_compute_workgroup_size_y >= 8
        && limits.max_compute_workgroup_size_z >= 1
        && limits.max_compute_workgroup_storage_size >= 4100
        && limits.max_storage_textures_per_shader_stage >= 8
        && limits.max_storage_buffers_per_shader_stage >= 1
        && limits.max_sampled_textures_per_shader_stage >= 9
        && limits.max_samplers_per_shader_stage >= 2
        && limits.max_uniform_buffers_per_shader_stage >= 1
        && limits.max_uniform_buffer_binding_size >= 240
        && limits.max_bindings_per_bind_group >= 15
        && limits.max_bind_groups >= 1
}

fn supports_size(limits: &wgpu::Limits, render: UVec2, output: UVec2) -> bool {
    // The port binds six SPD mip levels from a half-render-sized texture.
    let reconstruction_bytes = render.x as u64 * render.y as u64 * 4;
    render.min_element() > 0 && render.max_element() >= 64
        && output.max_element() <= limits.max_texture_dimension_2d
        && reconstruction_bytes <= limits.max_buffer_size
        && reconstruction_bytes <= limits.max_storage_buffer_binding_size
        && output.max_element().div_ceil(8) <= limits.max_compute_workgroups_per_dimension
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn extract(
    mut commands: Commands,
    cameras: Extract<Query<(RenderEntity, &Camera, &Projection, &GlobalTransform, Option<&FsrUpscaling>)>>,
    existing: Query<Entity, With<FsrCamera>>,
    frame: Extract<Res<FrameCount>>,
    time: Extract<Res<Time<Real>>>,
    support: Extract<Res<FsrSupport>>,
    loading: Extract<Option<Res<nvf_play::loading::Loading>>>,
    play_state: Extract<Option<Res<State<nvf_play::PlayState>>>>,
) {
    let mut active = Vec::new();
    for (entity, camera, projection, transform, fsr) in &cameras {
        let (Some(fsr), Projection::Perspective(projection)) = (fsr, projection) else { continue };
        if !support.0 || !camera.is_active || camera.physical_viewport_size().is_none_or(|s| s.x < 32 || s.y < 32) {
            continue;
        }
        active.push(entity);
        commands.entity(entity).insert(FsrCamera {
            settings: *fsr,
            translation: transform.translation(),
            rotation: transform.rotation(),
            near: projection.near,
            fov: projection.fov,
            frame: frame.0,
            delta_ms: (time.delta_secs() * 1000.0).max(1.0),
            loading_generation: loading.as_ref().map_or(0, |loading| loading.shown),
            play_state: play_state.as_ref().map(|state| *state.get()),
        });
    }
    for entity in &existing {
        if !active.contains(&entity) {
            commands.entity(entity).remove::<(FsrCamera, PreparedFsr, MainPassResolutionOverride, UnsuitableViewWarned)>();
        }
    }
}

/// A view FSR cannot use (multisampled, not HDR, or offset) whose warning has been
/// logged: [`prepare`] meets it every frame and says so once, until the view is usable
/// again or FSR is switched off.
#[derive(Component)]
struct UnsuitableViewWarned;

#[derive(Resource, Default)]
pub(crate) struct FsrRenderer(Option<(FsrContext, AuxiliaryPipelines)>);

#[derive(Component)]
pub(crate) struct PreparedFsr {
    view: Mutex<FsrView>,
    render_size: UVec2,
    output_size: UVec2,
    camera: FsrCamera,
    reset: bool,
    opaque: wgpu::Texture,
    reactive: wgpu::Texture,
    fsr_depth: wgpu::Texture,
    expanded_depth: wgpu::Texture,
    dilated_depth: wgpu::Texture,
    dilated_motion: wgpu::Texture,
    reconstructed_depth: wgpu::Buffer,
    resolution: wgpu::Buffer,
    dispatch_logged: AtomicBool,
    reference_logged: AtomicBool,
    sky_logged: AtomicBool,
    history_valid: AtomicBool,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn prepare(
    mut commands: Commands,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut renderer: ResMut<FsrRenderer>,
    mut views: Query<(
        Entity, &ExtractedView, &FsrCamera, &Msaa, &mut CameraMainTextureUsages,
        &mut TemporalJitter, &mut MipBias, Option<&mut PreparedFsr>, Option<&mut crate::ssao::AmbientOcclusion>,
        Has<UnsuitableViewWarned>,
    )>,
) {
    for (entity, view, camera, msaa, mut usages, mut jitter, mut bias, old, ao, warned) in &mut views {
        if *msaa != Msaa::Off || view.target_format != wgpu::TextureFormat::Rgba16Float || view.viewport.xy() != UVec2::ZERO {
            // Met every frame while it lasts: said once. The view renders natively, without
            // FSR's jitter and mip bias, as for an unsupported size below.
            if !warned {
                warn!("FSR 3.1 requires a single-sampled HDR view with no viewport offset; rendering natively until it is one");
                commands.entity(entity).insert(UnsuitableViewWarned);
            }
            jitter.offset = Vec2::ZERO;
            bias.0 = 0.0;
            commands.entity(entity).remove::<(PreparedFsr, MainPassResolutionOverride)>();
            continue;
        }
        if warned {
            commands.entity(entity).remove::<UnsuitableViewWarned>();
        }
        let output_size = view.viewport.zw();
        let render_size = camera.settings.quality.render_size(output_size);
        if !supports_size(&device.limits(), render_size, output_size) {
            jitter.offset = Vec2::ZERO;
            bias.0 = 0.0;
            commands.entity(entity).remove::<(PreparedFsr, MainPassResolutionOverride)>();
            continue;
        }
        let (context, _) = renderer.0.get_or_insert_with(|| {
            let context = FsrContext::new(FsrContextInfo {
                device: device.wgpu_device().clone(),
                flags: FsrContextFlags::HIGH_DYNAMIC_RANGE | FsrContextFlags::DEPTH_INVERTED
                    | FsrContextFlags::DEPTH_INFINITE | FsrContextFlags::AUTO_EXPOSURE,
            });
            (context, AuxiliaryPipelines::new(device.wgpu_device()))
        });
        usages.0 |= wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC;
        let phases = wgpu_ffx::get_jitter_phase_count(render_size.x as i32, output_size.x as i32);
        let offset = Vec2::from_array(wgpu_ffx::get_jitter_offset((camera.frame % i32::MAX as u32) as i32, phases));
        // Bevy applies its projection jitter with the opposite displacement sign.
        jitter.offset = -offset;
        bias.0 = (render_size.x as f32 / output_size.x as f32).log2() - 1.0;
        // The depth prepass is drawn at the render size and expanded by nearest texels
        // ([`AuxiliaryPipelines::expand_depth`]); the ambient occlusion takes its normals
        // from neighbouring texels as drawn (ssao.wesl `normal_at`). Before the uniforms are
        // written (RenderSystems::PrepareResources); the extraction resets it every frame.
        if let Some(mut ao) = ao.filter(|_| render_size != output_size) {
            ao.render_scale = Vec4::new(render_size.x as f32, render_size.y as f32, output_size.x as f32, output_size.y as f32);
        }
        match old {
            Some(mut old) if old.render_size == render_size && old.output_size == output_size => {
                old.reset = camera.cut_from(&old.camera);
                old.camera = *camera;
            }
            _ => {
                info!("FSR 3.1.4: {:?}, {}x{} -> {}x{}, frame generation off", camera.settings.quality, render_size.x, render_size.y, output_size.x, output_size.y);
                commands.entity(entity).insert(PreparedFsr::new(context, &device, &queue, *camera, render_size, output_size));
            }
        }
        commands.entity(entity).insert(MainPassResolutionOverride(render_size));
    }
}

impl PreparedFsr {
    fn new(context: &FsrContext, device: &RenderDevice, queue: &RenderQueue, camera: FsrCamera, render_size: UVec2, output_size: UVec2) -> Self {
        let device = device.wgpu_device();
        let texture = |label, size: UVec2, format, usage| device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label), size: wgpu::Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[],
        });
        let sampled = wgpu::TextureUsages::TEXTURE_BINDING;
        let storage = sampled | wgpu::TextureUsages::STORAGE_BINDING;
        let attachment = sampled | wgpu::TextureUsages::RENDER_ATTACHMENT;
        let resolution_bytes: Vec<u8> = [render_size.x, render_size.y, output_size.x, output_size.y].into_iter().flat_map(u32::to_ne_bytes).collect();
        Self {
            view: Mutex::new(context.create_view(queue, render_size.to_array(), output_size.to_array())),
            render_size, output_size, camera, reset: true,
            dispatch_logged: AtomicBool::new(false),
            reference_logged: AtomicBool::new(false),
            sky_logged: AtomicBool::new(false),
            history_valid: AtomicBool::new(false),
            // Display-sized like the depth attachment it is drawn with ([`reference_terrain_layers`]);
            // only the render rectangle is used.
            opaque: texture("fsr_opaque", output_size, wgpu::TextureFormat::Rgba16Float, attachment | wgpu::TextureUsages::COPY_DST),
            reactive: texture("fsr_reactive", render_size, wgpu::TextureFormat::R8Unorm, attachment),
            fsr_depth: texture("fsr_linear_camera_depth", render_size, wgpu::TextureFormat::Depth32Float, attachment),
            expanded_depth: texture("fsr_full_resolution_depth", output_size, wgpu::TextureFormat::Depth32Float, attachment | wgpu::TextureUsages::COPY_SRC),
            dilated_depth: texture("fsr_dilated_depth", render_size, wgpu::TextureFormat::R32Float, storage),
            dilated_motion: texture("fsr_dilated_motion", render_size, wgpu::TextureFormat::Rg16Float, storage),
            reconstructed_depth: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fsr_reconstructed_depth"), size: render_size.x as u64 * render_size.y as u64 * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
            }),
            resolution: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fsr_resolution"), contents: &resolution_bytes, usage: wgpu::BufferUsages::UNIFORM,
            }),
        }
    }
}

fn capture_opaque(view: ViewQuery<(&ViewTarget, &PreparedFsr)>, mut ctx: RenderContext) {
    let (target, fsr) = view.into_inner();
    ctx.command_encoder().copy_texture_to_texture(
        target.main_texture().as_image_copy(), fsr.opaque.as_image_copy(),
        wgpu::Extent3d { width: fsr.render_size.x, height: fsr.render_size.y, depth_or_array_layers: 1 },
    );
}

/// The first transparent draws go into the reactive mask's opaque reference: the same
/// draws, with the same pipelines and depth buffer, over the opaque copy, so FSR does not
/// count them as translucent.
///
/// The terrain's texture layers (terrain.rs) are blended copies of opaque ground, drawn
/// first in the transparent phase after the sky (`nif_material::sort_first`). They move with
/// the ground's depth and motion. Without them every pixel of ground a layer textures
/// counted as translucent, FSR discarded most of its history there, and the edges of
/// grass, bushes and trees standing on that ground were left nearly unaccumulated: they
/// shimmered under the jitter [observed 2026-11-05, docs/design/fsr-upscaling.md
/// "The reactive mask"].
///
/// The sky's blended and additive parts (sky.rs `sorts_with_sky`: the cloud layers, the
/// night's starfield layer, the stars and the moon) are drawn over the opaque sky dome,
/// which is in the depth and motion prepass: their pixels carry the dome's motion, which a
/// turn of the camera moves them by as it does the dome (both are around the camera, far
/// beyond the world). Left reactive, they kept FSR from accumulating the clouds and stars
/// and everything standing against them [observed 2026-11-06, docs/design/fsr-upscaling.md
/// "The reactive mask"]. Every other blended mesh stays reactive.
fn reference_sky_and_terrain_layers(
    world: &World,
    view: ViewQuery<(&ExtractedCamera, &ExtractedView, &ViewDepthStencilTexture, &PreparedFsr, Option<&MainPassResolutionOverride>)>,
    phases: Res<ViewSortedRenderPhases<Transparent3d>>,
    mut ctx: RenderContext,
) {
    let view_entity = view.entity();
    let (camera, extracted, depth, fsr, resolution) = view.into_inner();
    let Some(phase) = phases.get(&extracted.retained_view_entity) else { return };
    let sky = sky_range(phase.items.values().map(|item| item.distance));
    let layers = sort_first_range(phase.items.values().map(|item| item.distance));
    if sky.is_empty() && layers.is_empty() {
        return;
    }
    let target = fsr.opaque.create_view(&default());
    let mut pass = ctx.begin_tracked_render_pass(wgpu::RenderPassDescriptor {
        label: Some("fsr_reactive_reference_sky_and_terrain_layers"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
        // Blended parts test against the depth buffer but write none; store keeps it.
        depth_stencil_attachment: Some(depth.get_attachment(wgpu::StoreOp::Store)),
        timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
    });
    if let Some(viewport) = Viewport::from_viewport_and_override(camera.viewport.as_ref(), resolution) {
        pass.set_camera_viewport(&viewport);
    }
    // In the phase's order: the sky's band, then the terrain layers' band right after it.
    for (range, logged, one, all) in [(sky, &fsr.sky_logged, "sky", "sky parts"), (layers, &fsr.reference_logged, "terrain layer", "terrain layers")] {
        if range.is_empty() {
            continue;
        }
        let draws = range.len();
        if let Err(error) = phase.render_range(&mut pass, world, view_entity, range) {
            error!("FSR reactive reference: {all} not drawn: {error:?}");
        } else if !logged.swap(true, Ordering::Relaxed) {
            info!("FSR reactive reference: {draws} {one} draws over the opaque copy");
        }
    }
}

/// The indices of the sorted transparent items whose sort keys `in_band` takes. The phase
/// is sorted by these keys, ascending, so a band is one run.
fn band_range(sort_keys: impl Iterator<Item = f32>, in_band: impl Fn(f32) -> bool) -> Range<usize> {
    let mut range: Option<Range<usize>> = None;
    for (index, key) in sort_keys.enumerate() {
        if in_band(key) {
            range.get_or_insert(index..index).end = index + 1;
        } else if range.is_some() {
            break;
        }
    }
    range.unwrap_or(0..0)
}

/// The `sort_first` band (terrain layers).
fn sort_first_range(sort_keys: impl Iterator<Item = f32>) -> Range<usize> {
    band_range(sort_keys, nvf_assets::nif_material::sorts_first)
}

/// The sky's band (sky.rs `sorts_with_sky`), the first transparent items.
fn sky_range(sort_keys: impl Iterator<Item = f32>) -> Range<usize> {
    band_range(sort_keys, crate::sky::sorts_with_sky)
}

/// Reconstruction is before every effect expecting the full display-sized image.
#[allow(clippy::type_complexity)]
pub(crate) fn fsr_pass(
    view: ViewQuery<(&ViewTarget, &PreparedFsr, Option<&ViewPrepassTextures>, &TemporalJitter)>,
    renderer: Res<FsrRenderer>, device: Res<RenderDevice>, mut ctx: RenderContext,
) {
    let (target, prepared, prepass, jitter) = view.into_inner();
    let Some((context, auxiliary)) = &renderer.0 else { return };
    let inputs = prepass.and_then(|prepass| prepass.depth.as_ref().zip(prepass.motion_vectors.as_ref()));
    let Some((depth, motion)) = inputs else {
        prepared.history_valid.store(false, Ordering::Relaxed);
        auxiliary.fallback(device.wgpu_device(), ctx.command_encoder(), prepared, target);
        if let Some(depth) = prepass.and_then(|prepass| prepass.depth.as_ref()) {
            auxiliary.expand_depth(device.wgpu_device(), ctx.command_encoder(), prepared, &depth.texture.texture);
        }
        return;
    };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_fsr_3_1");
    let device = device.wgpu_device();
    let encoder = ctx.command_encoder();
    auxiliary.prepare_inputs(device, encoder, prepared, target.main_texture(), &depth.texture.texture);
    let source = target.main_texture().deref().clone();
    let destination = target.main_texture_other().deref().clone();
    let info = FsrDispatchInfo {
        color: source, depth: prepared.fsr_depth.clone(), motion_vectors: motion.texture.texture.deref().clone(),
        exposure: None, reactive_mask: Some(prepared.reactive.clone()), transparency_and_composition: Some(prepared.reactive.clone()),
        dilated_depth: prepared.dilated_depth.clone(), dilated_motion_vectors: prepared.dilated_motion.clone(),
        reconstructed_previous_depth: prepared.reconstructed_depth.clone(), output: destination,
        jitter_offset: (-jitter.offset).to_array(), motion_vector_scale: (-prepared.render_size.as_vec2()).to_array(),
        render_size: prepared.render_size.to_array(), upscale_size: prepared.output_size.to_array(),
        enable_sharpening: prepared.camera.settings.sharpness > 0.0, sharpness: prepared.camera.settings.sharpness.clamp(0.0, 1.0),
        frame_time_delta: prepared.camera.delta_ms, pre_exposure: 1.0,
        reset_history: prepared.reset || !prepared.history_valid.load(Ordering::Relaxed),
        camera_near: f32::MAX, camera_far: prepared.camera.near, camera_fov_y: prepared.camera.fov,
        // NVForever's world coordinates follow the game's units (about 70 units/metre).
        view_space_to_meters_factor: 1.0 / 70.0, flags: FsrDispatchFlags::empty(),
    };
    match context.dispatch(&mut prepared.view.lock().unwrap(), encoder, &info) {
        Ok(()) => {
            auxiliary.restore_alpha(device, encoder, prepared, target);
            target.post_process_write();
            prepared.history_valid.store(true, Ordering::Relaxed);
            if !prepared.dispatch_logged.swap(true, Ordering::Relaxed) {
                info!("FSR dispatch active: 3.1.4, {}x{} -> {}x{}, Vulkan source shaders, frame generation off", prepared.render_size.x, prepared.render_size.y, prepared.output_size.x, prepared.output_size.y);
            }
        }
        Err(error) => {
            prepared.history_valid.store(false, Ordering::Relaxed);
            error!("FSR dispatch failed: {error}; presenting spatial fallback");
            auxiliary.fallback(device, encoder, prepared, target);
        }
    }
    auxiliary.expand_depth(device, encoder, prepared, &depth.texture.texture);
    span.end(ctx.command_encoder());
}

struct AuxiliaryPipelines {
    reactive_layout: wgpu::BindGroupLayout,
    depth_layout: wgpu::BindGroupLayout,
    fallback_layout: wgpu::BindGroupLayout,
    reactive: wgpu::RenderPipeline,
    prepare_depth: wgpu::RenderPipeline,
    expand_depth: wgpu::RenderPipeline,
    fallback: wgpu::RenderPipeline,
    alpha: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl AuxiliaryPipelines {
    fn new(device: &wgpu::Device) -> Self {
        let texture_entry = |binding, depth| wgpu::BindGroupLayoutEntry {
            binding, visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: if depth { wgpu::TextureSampleType::Depth } else { wgpu::TextureSampleType::Float { filterable: true } }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
        };
        let uniform_entry = wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let reactive_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fsr_reactive_layout"), entries: &[texture_entry(0, false), texture_entry(1, false)] });
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fsr_depth_layout"), entries: &[texture_entry(0, true), uniform_entry] });
        let fallback_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fsr_fallback_layout"), entries: &[
            texture_entry(0, false), uniform_entry,
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        ] });
        let make = |label, source: &'static str, layout: &wgpu::BindGroupLayout, entry, color: Option<(wgpu::TextureFormat, wgpu::ColorWrites)>| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)) });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
            let targets: Vec<_> = color.into_iter().map(|(format, write_mask)| Some(wgpu::ColorTargetState { format, blend: None, write_mask })).collect();
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label), layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vertex"), compilation_options: default(), buffers: &[] },
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(entry), compilation_options: default(), targets: &targets }),
                primitive: default(), depth_stencil: color.is_none().then_some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Always), stencil: default(), bias: default() }),
                multisample: default(), multiview_mask: None, cache: None,
            })
        };
        Self {
            reactive: make("fsr_reactive", include_str!("fsr_reactive.wgsl"), &reactive_layout, "fragment", Some((wgpu::TextureFormat::R8Unorm, wgpu::ColorWrites::ALL))),
            prepare_depth: make("fsr_prepare_depth", include_str!("fsr_depth.wgsl"), &depth_layout, "prepare_depth", None),
            expand_depth: make("fsr_expand_depth", include_str!("fsr_depth.wgsl"), &depth_layout, "expand_depth", None),
            fallback: make("fsr_spatial_fallback", include_str!("fsr_fallback.wgsl"), &fallback_layout, "fragment", Some((wgpu::TextureFormat::Rgba16Float, wgpu::ColorWrites::ALL))),
            alpha: make("fsr_restore_alpha", include_str!("fsr_fallback.wgsl"), &fallback_layout, "alpha", Some((wgpu::TextureFormat::Rgba16Float, wgpu::ColorWrites::ALPHA))),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..default() }),
            reactive_layout, depth_layout, fallback_layout,
        }
    }

    fn depth_group(&self, device: &wgpu::Device, fsr: &PreparedFsr, source: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr_depth_group"), layout: &self.depth_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
            wgpu::BindGroupEntry { binding: 1, resource: fsr.resolution.as_entire_binding() },
        ] })
    }

    fn draw_depth(encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr_depth_conversion"), color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: target, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn prepare_inputs(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, fsr: &PreparedFsr, color: &wgpu::Texture, depth: &wgpu::Texture) {
        let depth_view = depth.create_view(&default());
        let group = self.depth_group(device, fsr, &depth_view);
        Self::draw_depth(encoder, &fsr.fsr_depth.create_view(&default()), &self.prepare_depth, &group);
        let color = color.create_view(&default());
        let opaque = fsr.opaque.create_view(&default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr_reactive_group"), layout: &self.reactive_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&opaque) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&color) },
        ] });
        let target = fsr.reactive.create_view(&default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr_reactive_mask"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(&self.reactive);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn expand_depth(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, fsr: &PreparedFsr, depth: &wgpu::Texture) {
        if fsr.render_size == fsr.output_size { return; }
        let source = depth.create_view(&default());
        let group = self.depth_group(device, fsr, &source);
        Self::draw_depth(encoder, &fsr.expanded_depth.create_view(&default()), &self.expand_depth, &group);
        encoder.copy_texture_to_texture(fsr.expanded_depth.as_image_copy(), depth.as_image_copy(), fsr.expanded_depth.size());
    }

    fn fallback(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, fsr: &PreparedFsr, target: &ViewTarget) {
        let post = target.post_process_write();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr_fallback_group"), layout: &self.fallback_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(post.source) },
            wgpu::BindGroupEntry { binding: 1, resource: fsr.resolution.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
        ] });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr_spatial_fallback"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(&self.fallback);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// FSR's output has alpha 1: the scene's own alpha is copied over from the nearest
    /// render-resolution texel (the main texture, before the flip), so the marks drawn into
    /// it (the Pip-Boy screen's, `imagespace.wesl` `SCREEN_MARK`) reach the post passes.
    fn restore_alpha(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, fsr: &PreparedFsr, target: &ViewTarget) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr_restore_alpha_group"), layout: &self.fallback_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(target.main_texture_view()) },
            wgpu::BindGroupEntry { binding: 1, resource: fsr.resolution.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
        ] });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr_restore_alpha"), color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target.main_texture_other_view(), depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        pass.set_pipeline(&self.alpha);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_modes_have_standard_ratios() {
        let output = UVec2::new(3840, 2160);
        assert_eq!(FsrQuality::NativeAa.render_size(output), output);
        assert_eq!(FsrQuality::Quality.render_size(output), UVec2::new(2560, 1440));
        assert_eq!(FsrQuality::Balanced.render_size(output), UVec2::new(2258, 1270));
        assert_eq!(FsrQuality::Performance.render_size(output), UVec2::new(1920, 1080));
        assert_eq!(FsrQuality::UltraPerformance.render_size(output), UVec2::new(1280, 720));
    }

    #[test]
    fn history_survives_motion_and_zoom_but_resets_on_cuts_and_lens_jumps() {
        let before = FsrCamera { settings: default(), translation: Vec3::ZERO, rotation: Quat::IDENTITY, near: 5.0, fov: 1.0, frame: 10, delta_ms: 16.0, loading_generation: 0, play_state: None };
        let after = FsrCamera { frame: 11, translation: Vec3::X * 20.0, ..before };
        assert!(!after.cut_from(&before));
        assert!(FsrCamera { translation: Vec3::X * 401.0, ..after }.cut_from(&before));
        assert!(FsrCamera { rotation: Quat::from_rotation_y(1.0), ..after }.cut_from(&before));
        assert!(!FsrCamera { fov: 0.95, ..after }.cut_from(&before), "a zoom step keeps the history");
        assert!(FsrCamera { fov: 0.3, ..after }.cut_from(&before), "a lens jump resets it");
        assert!(FsrCamera { frame: 14, ..after }.cut_from(&before));
        assert!(FsrCamera { delta_ms: 500.0, ..after }.cut_from(&before));
        assert!(FsrCamera { loading_generation: 1, ..after }.cut_from(&before));
        assert!(FsrCamera { play_state: Some(nvf_play::PlayState::Loading), ..after }.cut_from(&before));
    }

    #[test]
    fn fsr_depth_unsqueezes_with_the_first_person_share() {
        // fsr_depth.wgsl is plain WGSL without imports: its copy of the share has to be the
        // one the NIF shaders squeeze first-person depth into.
        let line = format!("const FIRST_PERSON_DEPTH_SHARE: f32 = {:?};", nvf_assets::nif_material::FIRST_PERSON_DEPTH_SHARE);
        assert!(include_str!("fsr_depth.wgsl").contains(&line), "fsr_depth.wgsl must declare `{line}`");
    }

    #[test]
    fn an_aim_zoom_keeps_the_history_and_a_lens_jump_resets_it() {
        use nvf_play::aim_zoom::{zoomed_fov, ZOOM_SECONDS};
        use nvf_play::free_cam::vertical_fov;
        let base = vertical_fov(75.0);
        let start = FsrCamera { settings: default(), translation: Vec3::ZERO, rotation: Quat::IDENTITY, near: 5.0, fov: base, frame: 0, delta_ms: 16.0, loading_generation: 0, play_state: None };
        // The 9mm pistol's iron sights (65°), a closer sight and a 20° scope, zoomed in and
        // out again a frame at a time as `aim_zoom::zoom` steps it.
        for sight_deg in [65.0, 55.0, 20.0] {
            let sight = vertical_fov(sight_deg);
            for fps in [15.0, 30.0, 60.0, 144.0] {
                let step = (1.0f32 / fps).min(0.25) / ZOOM_SECONDS;
                let mut blends = Vec::new();
                let mut blend = 0.0f32;
                while blend < 1.0 {
                    blend = (blend + step).min(1.0);
                    blends.push(blend);
                }
                while blend > 0.0 {
                    blend = (blend - step).max(0.0);
                    blends.push(blend);
                }
                let mut camera = FsrCamera { delta_ms: 1000.0 / fps, ..start };
                for blend in blends {
                    let next = FsrCamera { frame: camera.frame + 1, fov: zoomed_fov(base, sight, blend), ..camera };
                    assert!(!next.cut_from(&camera), "{sight_deg}° sight at {fps} fps: {:.2}° -> {:.2}° reset", camera.fov.to_degrees(), next.fov.to_degrees());
                    camera = next;
                }
            }
        }
        // A jump to the 20° scope's angle (the console's `fov`) and back resets.
        let jumped = FsrCamera { frame: 1, fov: vertical_fov(20.0), ..start };
        assert!(jumped.cut_from(&start));
        assert!(FsrCamera { frame: 2, fov: base, ..jumped }.cut_from(&jumped));
    }

    #[test]
    fn reactive_reference_takes_only_the_terrain_layer_band() {
        use nvf_assets::nif_material::sort_first;
        // Sorted keys as Bevy draws them: sky parts, terrain layers near and far, then
        // ordinary blended meshes (particles, glass, decals) near and far.
        let sky = [-1.0e6 - 30.0, -1.0e6 - 20.0, -1.0e6 + 1.0];
        let layers = [sort_first(2.0) - 9_000.0, sort_first(4.0) - 300.0, sort_first(2.0) + 2_500.0];
        let others = [-12_000.0, -40.0, 3.0];
        let keys: Vec<f32> = sky.iter().chain(&layers).chain(&others).copied().collect();
        assert_eq!(sort_first_range(keys.iter().copied()), 3..6);
        assert_eq!(sort_first_range(sky.iter().chain(&others).copied()), 0..0, "no layers in view");
        assert_eq!(sort_first_range(layers.iter().copied()), 0..3);
        assert_eq!(sort_first_range(std::iter::empty()), 0..0);
    }

    #[test]
    fn reactive_reference_takes_the_sky_band_then_the_terrain_layers() {
        use nvf_assets::nif_material::sort_first;
        // Sorted keys as Bevy draws them: the stars (-30) at the world's origin up to
        // 200,000 units ahead, the moon (-20) 140,000 units out, the cloud layers (0-7) at
        // the camera, the stars again with the origin 200,000 units behind; then terrain
        // layers, then ordinary blended meshes.
        let sky = [-1.0e6 - 30.0 - 200_000.0, -1.0e6 - 20.0 - 140_000.0, -1.0e6, -1.0e6 + 7.0, -1.0e6 - 30.0 + 200_000.0];
        let layers = [sort_first(2.0) - 9_000.0, sort_first(4.0) - 300.0, sort_first(2.0) + 2_500.0];
        let others = [-60_000.0, -40.0, 3.0];
        let keys: Vec<f32> = sky.iter().chain(&layers).chain(&others).copied().collect();
        let (sky_band, layer_band) = (sky_range(keys.iter().copied()), sort_first_range(keys.iter().copied()));
        assert_eq!(sky_band, 0..5);
        assert_eq!(layer_band, 5..8);
        assert_eq!(sky_band.end, layer_band.start, "the two bands are drawn as one run, in order");
        assert_eq!(sky_range(layers.iter().chain(&others).copied()), 0..0, "no sky parts in view (clear sky by day)");
        assert_eq!(sky_range(others.iter().copied()), 0..0);
    }

    #[test]
    fn unsupported_limits_and_tiny_views_do_not_create_fsr_contexts() {
        assert!(!supports_limits(&wgpu::Limits::default()), "WebGPU's four-storage-texture baseline is insufficient");
        let limits = wgpu::Limits { max_storage_textures_per_shader_stage: 8, ..default() };
        assert!(supports_limits(&limits));
        assert!(!supports_limits(&wgpu::Limits { max_storage_textures_per_shader_stage: 7, ..limits.clone() }));
        assert!(!supports_limits(&wgpu::Limits { max_compute_workgroup_size_x: 128, ..limits.clone() }));
        assert!(!supports_limits(&wgpu::Limits { max_compute_workgroup_storage_size: 4096, ..limits.clone() }));
        assert!(supports_size(&limits, UVec2::new(853, 480), UVec2::new(1280, 720)));
        assert!(!supports_size(&limits, UVec2::splat(32), UVec2::splat(96)));
        assert!(!supports_size(&limits, UVec2::splat(16384), UVec2::splat(16384)));
    }
}
