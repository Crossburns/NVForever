//! RTX Remix as NVForever's renderer (`--remix <Remix d3d9.dll>`, or `NVF_REMIX`): the
//! Remix runtime runs inside NVForever (crate `nvf_remix`) and path-traces the scene
//! NVForever hands it each frame, into a child window over NVForever's own
//! (docs/design/remix.md).
//!
//! - Meshes and images are copied as they arrive, in the frame they are added, before
//!   Bevy keeps only their GPU copies; each image becomes a Remix texture at once.
//! - Each frame: the main 3D camera, every visible mesh with a NIF material as an
//!   instance (one Remix mesh per mesh and material), point lights as spheres,
//!   directional lights as the sun, then Present.
//! - The runtime starts in the folder of its `rtx.conf` (the Remix install's root, the
//!   parent of the DLL's `trex` folder), so a mod's settings and replacement assets
//!   (`rtx-remix\mods`) apply; the axis options are set for Bevy's space (Y up,
//!   right-handed) after it.

use bevy::asset::{AssetEventSystems, UntypedAssetId};
use bevy::camera::Projection;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::camera::visibility::RenderLayers;
use bevy::window::{PrimaryWindow, RawHandleWrapper};
use nvf_assets::NifMaterial;
use nvf_remix::ffi::{self, HardcodedVertex};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

/// Where the Remix runtime is: `--remix` or `NVF_REMIX`.
#[derive(Resource, Clone)]
pub struct RemixConfig {
    pub dll: PathBuf,
}

pub struct RemixPlugin(pub RemixConfig);

impl Plugin for RemixPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().insert_non_send(RemixStateCell::default());
        app.insert_resource(self.0.clone())
            .init_resource::<CpuMeshes>()
            .add_systems(Startup, setup_ui_overlay)
            .add_systems(PostUpdate, resize_ui_overlay)
            .init_resource::<LiveTextures>()
            .add_systems(PostUpdate, ((take_new_meshes, take_new_images, take_changed_materials), watch_live_textures).chain().after(AssetEventSystems))
            .add_systems(Last, (frame, shut_down_on_exit).chain());
    }
}

/// Vertex data copied from meshes as they arrive.
#[derive(Resource, Default)]
struct CpuMeshes {
    by_id: HashMap<AssetId<Mesh>, Arc<CpuMesh>>,
    /// Images whose pixels arrived before the runtime started.
    pending_images: Vec<(AssetId<Image>, Arc<PendingImage>)>,
    /// Images that cannot become Remix textures (format, no pixels).
    skipped_images: Vec<AssetId<Image>>,
    /// Images drawn into on the GPU (screens: the Pip-Boy's, terminals'), not yet watched.
    new_live: Vec<AssetId<Image>>,
    /// Meshes changed after Remix had them (particles, lips, dismembered bodies).
    changed: Vec<AssetId<Mesh>>,
    /// NIF materials changed (glows by the hour, fades, swapped textures).
    changed_materials: Vec<UntypedAssetId>,
}

/// A mesh's triangles, and for a skinned one four joint weights and indices a vertex.
struct CpuMesh {
    verts: Vec<HardcodedVertex>,
    idx: Vec<u32>,
    skin: Option<(Vec<f32>, Vec<u32>)>,
}

struct PendingImage {
    width: u32,
    height: u32,
    mips: u32,
    format: TextureFormat,
    data: Vec<u8>,
    normal: bool,
    /// The image hash the original's D3D9 draw of this texture has (XXH3-64 of its top
    /// level as stored), for block-compressed textures [documented: New Vegas RTX's
    /// api_rtxconf.cpp d3d9_image_hash, the top level packed without pitch padding].
    d3d9: Option<u64>,
}

/// XXH3-64 of a block-compressed texture's top level (the bytes at the start of a DDS's
/// data), as the runtime hashes the D3D9 texture the original uploads.
fn d3d9_hash(f: TextureFormat, w: u32, h: u32, data: &[u8]) -> Option<u64> {
    let block = match f {
        TextureFormat::Bc1RgbaUnorm | TextureFormat::Bc1RgbaUnormSrgb => 8usize,
        TextureFormat::Bc2RgbaUnorm | TextureFormat::Bc2RgbaUnormSrgb | TextureFormat::Bc3RgbaUnorm | TextureFormat::Bc3RgbaUnormSrgb => 16,
        _ => return None,
    };
    let n = (w.div_ceil(4).max(1) as usize) * (h.div_ceil(4).max(1) as usize) * block;
    (data.len() >= n).then(|| twox_hash::XxHash3_64::oneshot(&data[..n]))
}

/// What New Vegas RTX's rtx.conf texture lists say, by D3D9 image hash: the instance
/// categories a draw of the texture carries, whether its alpha is dropped
/// (`rtx.ignoreAlphaOnTextures`), and its glow scale [documented: the wrapper's
/// api_rtxconf.cpp kLists; the runtime's list options].
#[derive(Default)]
struct RtxLists {
    by_hash: HashMap<u64, (u32, bool)>,
    glow_intensity: f32,
}

impl RtxLists {
    fn read(text: &str) -> RtxLists {
        // The lists that tag an API draw, and the category each becomes; the runtime's
        // D3D9-only lists (uiTextures, lightConverter, …) are left out.
        const LISTS: [(&str, u32, bool); 23] = [
            ("rtx.ignoreTextures", 1 << 3, false),
            ("rtx.hideInstanceTextures", 1 << 8, false),
            ("rtx.skyBoxTextures", 1 << 2, false),
            ("rtx.particleTextures", 1 << 9, false),
            ("rtx.particleEmitterTextures", 1 << 23, false),
            ("rtx.beamTextures", 1 << 10, false),
            ("rtx.decalTextures", 1 << 11, false),
            ("rtx.dynamicDecalTextures", 1 << 12, false),
            ("rtx.singleOffsetDecalTextures", 1 << 13, false),
            ("rtx.nonOffsetDecalTextures", 1 << 14, false),
            ("rtx.terrainTextures", 1 << 16, false),
            ("rtx.animatedWaterTextures", 1 << 17, false),
            ("rtx.worldSpaceUiTextures", 1 << 0, false),
            ("rtx.worldSpaceUiBackgroundTextures", 1 << 1, false),
            ("rtx.playerModelTextures", 1 << 18, false),
            ("rtx.playerModelBodyTextures", 1 << 19, false),
            ("rtx.hairCardTextures", 1 << 25, false),
            ("rtx.smoothNormalsTextures", 1 << 24, false),
            ("rtx.ignoreLights", 1 << 4, false),
            ("rtx.antiCulling.antiCullingTextures", 1 << 5, false),
            ("rtx.postfx.motionBlurMaskOutTextures", 1 << 6, false),
            ("rtx.opacityMicromapIgnoreTextures", 1 << 7, false),
            ("rtx.ignoreAlphaOnTextures", 1 << 21, true),
        ];
        let mut out = RtxLists { glow_intensity: 1.0, ..Default::default() };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let k = k.trim();
            if k == "rtx.fnvProtocol.glowEmissiveIntensity" {
                out.glow_intensity = v.trim().parse().unwrap_or(1.0);
                continue;
            }
            let Some(&(_, cat, drop)) = LISTS.iter().find(|l| l.0 == k) else { continue };
            for item in v.split(',') {
                let item = item.trim();
                // A negative entry ("-0x…") takes the hash out of this list.
                let (neg, hex) = match item.strip_prefix('-') {
                    Some(r) => (true, r),
                    None => (false, item),
                };
                let Some(hex) = hex.strip_prefix("0x").or_else(|| hex.strip_prefix("0X")) else { continue };
                let Ok(h) = u64::from_str_radix(hex, 16) else { continue };
                let e = out.by_hash.entry(h).or_default();
                if neg {
                    e.0 &= !cat;
                } else {
                    e.0 |= cat;
                    e.1 |= drop;
                }
            }
        }
        out
    }
}

/// The runtime and what NVForever made in it (main thread only).
struct RemixState {
    remix: nvf_remix::Remix,
    child: nvf_remix::window::Child,
    textures: HashSet<u64>,
    failed_textures: HashSet<u64>,
    material_wait: HashMap<UntypedAssetId, u64>,
    /// Land base meshes drawn with their layers' terrain material, and the layer meshes
    /// that material holds.
    terrain: HashMap<AssetId<Mesh>, ffi::MeshHandle>,
    layered: HashSet<AssetId<Mesh>>,
    materials: HashMap<UntypedAssetId, ffi::MaterialHandle>,
    meshes: HashMap<(AssetId<Mesh>, UntypedAssetId), ffi::MeshHandle>,
    lights: HashMap<Entity, (u64, ffi::LightHandle)>,
    frames: u64,
    errors: u64,
    light_scale: f32,
    sun_scale: f32,
    started: std::time::Instant,
    why: [u32; 4],
    shot_taken: bool,
    /// The quadratic attenuation an engine light carries (NVF_REMIX_LIGHT_A2).
    light_a2: f32,
    /// The D3D9 image hash of each uploaded texture that has one (by NVForever's hash).
    d3d9: HashMap<u64, u64>,
    /// New Vegas RTX's rtx.conf lists.
    lists: RtxLists,
    /// The 1x1 weight textures of a one-layer material (layer 0 at full weight, none).
    one_layer: Option<(u64, u64)>,
    /// The sky and weather options last sent, by name.
    pushed: HashMap<&'static str, String>,
    /// How many times each changed mesh was made again (in its Remix hash).
    mesh_gen: HashMap<AssetId<Mesh>, u32>,
    /// How many times each changed material was made again.
    mat_gen: HashMap<UntypedAssetId, u32>,
    /// Changed meshes made again since the last stats line.
    remade: u64,
    /// Interior fill light's strength (NVF_REMIX_FILL_SCALE).
    fill_scale: f32,
    /// The UI overlay's last picture and its size.
    ui_last: Option<(Vec<u8>, (u32, u32))>,
    /// Shapes left out (test aid: NVF_REMIX_TRACE_HIDE).
    hidden: HashSet<(AssetId<Mesh>, UntypedAssetId)>,
    /// Time in `frame` building the scene and in `present`, since the last stats line.
    build_time: std::time::Duration,
    present_time: std::time::Duration,
}

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    // Never 0 (no key in the API).
    h.finish() | 1
}

fn convert_mesh(m: &Mesh) -> Option<CpuMesh> {
    if m.primitive_topology() != bevy::mesh::PrimitiveTopology::TriangleList {
        return None;
    }
    let pos = match m.try_attribute_option(Mesh::ATTRIBUTE_POSITION).ok()?? {
        VertexAttributeValues::Float32x3(p) => p,
        _ => return None,
    };
    let normals = match m.try_attribute_option(Mesh::ATTRIBUTE_NORMAL).ok().flatten() {
        Some(VertexAttributeValues::Float32x3(n)) => Some(n),
        _ => None,
    };
    let uvs = match m.try_attribute_option(Mesh::ATTRIBUTE_UV_0).ok().flatten() {
        Some(VertexAttributeValues::Float32x2(u)) => Some(u),
        _ => None,
    };
    let colors = match m.try_attribute_option(Mesh::ATTRIBUTE_COLOR).ok().flatten() {
        Some(VertexAttributeValues::Float32x4(c)) => Some(c),
        _ => None,
    };
    let to8 = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    let verts = (0..pos.len())
        .map(|i| HardcodedVertex {
            position: pos[i],
            normal: normals.and_then(|n| n.get(i)).copied().unwrap_or([0.0, 1.0, 0.0]),
            texcoord: uvs.and_then(|u| u.get(i)).copied().unwrap_or([0.0, 0.0]),
            // D3DCOLOR order: A R G B from the top byte [documented: remix_c.h's
            // HardcodedVertex.color, a D3D9 vertex colour].
            color: colors.and_then(|c| c.get(i)).map_or(0xFFFF_FFFF, |c| to8(c[3]) << 24 | to8(c[0]) << 16 | to8(c[1]) << 8 | to8(c[2])),
            _pad: [0; 7],
        })
        .collect::<Vec<_>>();
    let mut idx: Vec<u32> = match m.try_indices_option().ok()? {
        Some(Indices::U16(i)) => i.iter().map(|&x| u32::from(x)).collect(),
        Some(Indices::U32(i)) => i.clone(),
        None => (0..verts.len() as u32).collect(),
    };
    // Bevy's front faces wind counter-clockwise, Remix's the other way (Direct3D's)
    // [observed: single-sided interior walls traced only from outside until flipped].
    for t in idx.as_chunks_mut::<3>().0 {
        t.swap(1, 2);
    }
    if idx.len() < 3 || verts.is_empty() {
        return None;
    }
    let joints = match m.try_attribute_option(Mesh::ATTRIBUTE_JOINT_INDEX).ok().flatten() {
        Some(VertexAttributeValues::Uint16x4(j)) => Some(j.iter().flat_map(|x| x.map(u32::from)).collect::<Vec<u32>>()),
        _ => None,
    };
    let weights = match m.try_attribute_option(Mesh::ATTRIBUTE_JOINT_WEIGHT).ok().flatten() {
        Some(VertexAttributeValues::Float32x4(w)) => Some(w.iter().flat_map(|x| *x).collect::<Vec<f32>>()),
        _ => None,
    };
    let skin = match (weights, joints) {
        (Some(w), Some(j)) if w.len() == verts.len() * 4 && j.len() == verts.len() * 4 => Some((w, j)),
        _ => None,
    };
    Some(CpuMesh { verts, idx, skin })
}

/// NIF materials NVForever changed: their Remix materials are made again (window glows
/// follow the hour) [observed 2026-11-09: lit windows at 10:00 under Remix].
fn take_changed_materials(mut ev: MessageReader<AssetEvent<NifMaterial>>, mut cpu: ResMut<CpuMeshes>) {
    for e in ev.read() {
        if let AssetEvent::Modified { id } = e {
            cpu.changed_materials.push(id.untyped());
        }
    }
}

fn take_new_meshes(mut ev: MessageReader<AssetEvent<Mesh>>, meshes: Res<Assets<Mesh>>, mut cpu: ResMut<CpuMeshes>) {
    for e in ev.read() {
        match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                if let Some(c) = meshes.get(*id).and_then(convert_mesh) {
                    if cpu.by_id.insert(*id, Arc::new(c)).is_some() {
                        cpu.changed.push(*id);
                    }
                }
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                cpu.by_id.remove(id);
            }
            _ => {}
        }
    }
}

/// A Bevy texture format as Remix takes it: (format, Vulkan format override).
fn remix_format(f: TextureFormat) -> Option<(u32, u32)> {
    Some(match f {
        TextureFormat::Rgba8Unorm => (ffi::FORMAT_R8G8B8A8_UNORM, 0),
        TextureFormat::Rgba8UnormSrgb => (ffi::FORMAT_R8G8B8A8_SRGB, 0),
        TextureFormat::Bgra8Unorm => (ffi::FORMAT_B8G8R8A8_UNORM, 0),
        TextureFormat::Bgra8UnormSrgb => (ffi::FORMAT_B8G8R8A8_SRGB, 0),
        TextureFormat::Bc1RgbaUnorm => (ffi::FORMAT_BC1_RGB_UNORM, ffi::VK_BC1_RGBA_UNORM),
        TextureFormat::Bc1RgbaUnormSrgb => (ffi::FORMAT_BC1_RGB_SRGB, ffi::VK_BC1_RGBA_SRGB),
        TextureFormat::Bc2RgbaUnorm => (ffi::FORMAT_BC3_UNORM, ffi::VK_BC2_UNORM),
        TextureFormat::Bc2RgbaUnormSrgb => (ffi::FORMAT_BC3_SRGB, ffi::VK_BC2_SRGB),
        TextureFormat::Bc3RgbaUnorm => (ffi::FORMAT_BC3_UNORM, 0),
        TextureFormat::Bc3RgbaUnormSrgb => (ffi::FORMAT_BC3_SRGB, 0),
        TextureFormat::Bc4RUnorm => (ffi::FORMAT_BC3_UNORM, ffi::VK_BC4_UNORM),
        TextureFormat::Bc5RgUnorm => (ffi::FORMAT_BC5_UNORM, 0),
        TextureFormat::Bc7RgbaUnorm => (ffi::FORMAT_BC7_UNORM, 0),
        TextureFormat::Bc7RgbaUnormSrgb => (ffi::FORMAT_BC7_SRGB, 0),
        _ => return None,
    })
}

fn take_new_images(mut ev: MessageReader<AssetEvent<Image>>, images: Res<Assets<Image>>, names: Option<Res<nvf_assets::texture::TextureCache>>, mut cpu: ResMut<CpuMeshes>, ui: Option<Res<UiOverlay>>) {
    for e in ev.read() {
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = e else { continue };
        if ui.as_ref().is_some_and(|u| u.image == *id) {
            continue;
        }
        let Some(img) = images.get(*id) else { continue };
        let d = &img.texture_descriptor;
        // A render target (a screen drawn every frame: the Pip-Boy's, a terminal's), with
        // or without first pixels: read back each frame (live_textures).
        if d.usage.contains(bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT)
            && d.dimension == bevy::render::render_resource::TextureDimension::D2
            && matches!(d.format, TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb | TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb)
        {
            if matches!(e, AssetEvent::Added { .. }) {
                cpu.new_live.push(*id);
            }
            continue;
        }
        if d.dimension != bevy::render::render_resource::TextureDimension::D2 || d.size.depth_or_array_layers != 1 || remix_format(d.format).is_none() {
            cpu.skipped_images.push(*id);
            continue;
        }
        let Some(data) = img.data.clone() else {
            // A render target in a format Remix takes: read back each frame (live_textures).
            let rt = d.usage.contains(bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT);
            if rt && matches!(d.format, TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb | TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb) {
                cpu.new_live.push(*id);
            } else {
                cpu.skipped_images.push(*id);
            }
            continue;
        };
        // A normal map: game textures have no asset path, so by the texture cache's name
        // (`…_n.dds`); NVForever loads normal maps linear and pictures sRGB, so a linear
        // texture the cache does not name is taken as one too [observed 2026-11-09: by
        // path none was found, every normal map went as a picture and rocks and ground
        // shaded flat].
        let normal = !d.format.is_srgb()
            && names.as_ref().and_then(|n| n.name_of(*id)).is_none_or(|n| {
                let n = n.to_ascii_lowercase();
                n.ends_with("_n.dds") || n.ends_with("_n.png")
            });
        let d3d9 = d3d9_hash(d.format, d.size.width, d.size.height, &data);
        cpu.pending_images.push((*id, Arc::new(PendingImage { width: d.size.width, height: d.size.height, mips: d.mip_level_count, format: d.format, data, normal, d3d9 })));
    }
}

type CameraParts = (&'static Camera, &'static GlobalTransform, &'static Projection, Option<&'static RenderLayers>, Has<nvf_play::PlayerCamera>);
type ShapeParts<M> = (
    Entity,
    &'static Mesh3d,
    &'static MeshMaterial3d<M>,
    &'static GlobalTransform,
    &'static ViewVisibility,
    Option<&'static bevy::mesh::skinning::SkinnedMesh>,
    Option<&'static RenderLayers>,
    Has<crate::terrain::LandMesh>,
    Option<&'static ChildOf>,
);

/// The shapes of every material type built on Bevy's standard material: the NIF
/// material (statics, actors, terrain) and the trees', grass's and billboards' own.
#[derive(bevy::ecs::system::SystemParam)]
struct Shapes<'w, 's> {
    nif: Query<'w, 's, ShapeParts<NifMaterial>>,
    nif_m: Res<'w, Assets<NifMaterial>>,
    leaf: Query<'w, 's, ShapeParts<crate::tree::LeafMaterial>>,
    leaf_m: Res<'w, Assets<crate::tree::LeafMaterial>>,
    bark: Query<'w, 's, ShapeParts<crate::tree::BarkMaterial>>,
    bark_m: Res<'w, Assets<crate::tree::BarkMaterial>>,
    bill: Query<'w, 's, ShapeParts<crate::tree::BillboardMaterial>>,
    bill_m: Res<'w, Assets<crate::tree::BillboardMaterial>>,
    grass: Query<'w, 's, ShapeParts<crate::grass::GrassMaterial>>,
    grass_m: Res<'w, Assets<crate::grass::GrassMaterial>>,
    water: Query<'w, 's, (&'static Mesh3d, &'static MeshMaterial3d<crate::water::WaterMaterial>, &'static GlobalTransform, &'static ViewVisibility)>,
    water_m: Res<'w, Assets<crate::water::WaterMaterial>>,
}

/// The lights Remix gets: Bevy's point and directional lights, and what the interior fill
/// light is made from.
#[derive(bevy::ecs::system::SystemParam)]
struct Lights<'w, 's> {
    points: Query<'w, 's, (Entity, &'static PointLight, &'static GlobalTransform, &'static InheritedVisibility)>,
    // Not the arms' shade light: a Bevy stand-in that lights only the first-person arms
    // for their shadows, at a constant strength (it lit the world as a second sun at
    // night) [observed 2026-11-09].
    suns: Query<'w, 's, (Entity, &'static DirectionalLight, &'static GlobalTransform), Without<crate::player_body::ArmsShade>>,
    sky: Option<Res<'w, crate::sky::SkyState>>,
    ambient: Res<'w, GlobalAmbientLight>,
}

/// One shape, whatever its material type.
struct ShapeIn<'a> {
    entity: Entity,
    mesh: AssetId<Mesh>,
    mat: UntypedAssetId,
    base: &'a StandardMaterial,
    gt: &'a GlobalTransform,
    vis: bool,
    skinned: Option<&'a bevy::mesh::skinning::SkinnedMesh>,
    layers: Option<&'a RenderLayers>,
    /// A mesh of a cell's land (its base or a blended texture layer), and its cell.
    land: bool,
    parent: Option<Entity>,
    /// The shape lies on another surface (a NIF decal: roads on the land, posters).
    decal: bool,
}

impl<'w, 's> Shapes<'w, 's> {
    fn all(&self) -> Vec<ShapeIn<'_>> {
        fn take<'a, E: bevy::pbr::MaterialExtension>(
            q: &'a Query<ShapeParts<bevy::pbr::ExtendedMaterial<StandardMaterial, E>>>,
            m: &'a Assets<bevy::pbr::ExtendedMaterial<StandardMaterial, E>>,
            out: &mut Vec<ShapeIn<'a>>,
            decal: fn(&E) -> bool,
        ) {
            for (entity, mesh, mat, gt, vis, skinned, layers, land, parent) in q {
                let Some(material) = m.get(mat.id()) else { continue };
                out.push(ShapeIn { entity, mesh: mesh.id(), mat: mat.id().untyped(), base: &material.base, gt, vis: vis.get(), skinned, layers, land, parent: parent.map(|p| p.parent()), decal: decal(&material.extension) });
            }
        }
        let mut out = Vec::new();
        take(&self.nif, &self.nif_m, &mut out, |e| e.decal);
        take(&self.leaf, &self.leaf_m, &mut out, |_| false);
        take(&self.bark, &self.bark_m, &mut out, |_| false);
        take(&self.bill, &self.bill_m, &mut out, |_| false);
        take(&self.grass, &self.grass_m, &mut out, |_| false);
        out
    }
}

#[allow(clippy::too_many_arguments)]
fn frame(
    world_state: Option<NonSendMut<RemixStateCell>>,
    config: Res<RemixConfig>,
    windows: Query<&RawHandleWrapper, With<PrimaryWindow>>,
    mut cpu: ResMut<CpuMeshes>,
    cameras: Query<CameraParts, With<Camera3d>>,
    body: Option<Res<crate::player_body::PlayerBody>>,
    children: Query<&Children>,
    shapes: Shapes,
    joints: Query<&GlobalTransform>,
    ui: Option<Res<UiOverlay>>,
    live: Res<LiveTextures>,
    bindposes: Res<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
    meshes_all: Res<Assets<Mesh>>,
    lights: Lights,
    names: Option<Res<nvf_assets::texture::TextureCache>>,
) {
    let Some(mut cell) = world_state else { return };
    if cell.failed {
        return;
    }
    if cell.state.is_none() {
        let Ok(h) = windows.single() else { return };
        let raw_window_handle::RawWindowHandle::Win32(w) = h.get_window_handle() else {
            error!("remix: the window is not a Win32 window");
            cell.failed = true;
            return;
        };
        match start(&config, w.hwnd.get() as *mut std::ffi::c_void) {
            Ok(s) => cell.state = Some(s),
            Err(e) => {
                error!("remix: {e}");
                cell.failed = true;
                return;
            }
        }
    }
    let Some(st) = cell.state.as_mut() else { return };
    let t0 = std::time::Instant::now();
    st.child.follow();

    // Meshes changed since Remix got them: their Remix meshes are made again, under a
    // new generation's hash (one hash made again after its destroy did not show, as
    // with lights) [observed 2026-11-09: particles kept their first shape].
    let changed: HashSet<AssetId<Mesh>> = std::mem::take(&mut cpu.changed).into_iter().collect();
    if !changed.is_empty() {
        for id in &changed {
            *st.mesh_gen.entry(*id).or_default() += 1;
        }
        st.remade += changed.len() as u64;
        let stale: Vec<_> = st.meshes.keys().filter(|k| changed.contains(&k.0)).copied().collect();
        for k in stale {
            if let Some(h) = st.meshes.remove(&k) {
                st.remix.destroy_mesh(h);
            }
        }
    }
    // Changed materials: dropped with the meshes made with them, made again under a new
    // generation's hash (as changed meshes).
    let changed_mats: HashSet<UntypedAssetId> = std::mem::take(&mut cpu.changed_materials).into_iter().filter(|m| st.materials.contains_key(m)).collect();
    if !changed_mats.is_empty() {
        for m in &changed_mats {
            *st.mat_gen.entry(*m).or_default() += 1;
            if let Some(h) = st.materials.remove(m) {
                st.remix.destroy_material(h);
            }
            st.material_wait.remove(m);
        }
        let stale: Vec<_> = st.meshes.keys().filter(|k| changed_mats.contains(&k.1)).copied().collect();
        for k in stale {
            if let Some(h) = st.meshes.remove(&k) {
                st.remix.destroy_mesh(h);
            }
        }
        st.remade += changed_mats.len() as u64;
    }
    for id in std::mem::take(&mut cpu.skipped_images) {
        st.failed_textures.insert(hash_of(&id));
    }
    // New textures.
    for (id, img) in std::mem::take(&mut cpu.pending_images) {
        let hash = hash_of(&id);
        if st.textures.contains(&hash) {
            continue;
        }
        let Some((format, vk)) = remix_format(img.format) else {
            st.failed_textures.insert(hash);
            continue;
        };
        // Normal maps as stored, alpha (FNV's gloss) included: materials read them as RGB
        // tangent-space layers, as New Vegas RTX makes them [documented: the wrapper's
        // api_material.cpp, normalsRgbTangentSpace and the per-layer gloss].
        let _ = img.normal;
        let t = nvf_remix::TextureData { hash, width: img.width, height: img.height, mips: img.mips, format, data: &img.data, vk_format: vk, encoding: ffi::ENCODING_NONE, replace: false };
        if let Some(h) = img.d3d9 {
            st.d3d9.insert(hash, h);
        }
        match st.remix.create_texture(&t) {
            Ok(_) => {
                st.textures.insert(hash);
            }
            Err(e) => {
                st.errors += 1;
                st.failed_textures.insert(hash);
                if st.errors < 20 {
                    warn!("remix: {e} ({}x{} {:?})", img.width, img.height, img.format);
                }
            }
        }
    }

    // Screens drawn every frame: their latest pixels, in place after the first upload.
    if let Ok(mut live) = live.0.lock() {
        for (id, l) in live.iter_mut() {
            let Some(px) = l.pixels.take() else { continue };
            let hash = hash_of(id);
            let t = nvf_remix::TextureData { hash, width: l.width, height: l.height, mips: 1, format: l.format, data: &px, vk_format: 0, encoding: 0, replace: st.textures.contains(&hash) };
            let first = !st.textures.contains(&hash);
            match st.remix.create_texture(&t) {
                Ok(_) => {
                    if first {
                        info!("remix: live texture {id:?} {}x{} uploaded", l.width, l.height);
                    }
                    st.textures.insert(hash);
                    st.failed_textures.remove(&hash);
                }
                Err(e) => {
                    st.errors += 1;
                    if first {
                        warn!("remix: live texture {id:?}: {e}");
                    }
                }
            }
        }
    }

    // The camera.
    // The player's eyes; another 3D camera (a reflection's) only without one.
    let cam = cameras.iter().filter(|(c, ..)| c.is_active).max_by_key(|(c, _, _, _, player)| (*player, -c.order));
    let cam_layers = cam.and_then(|(_, _, _, l, _)| l.cloned()).unwrap_or_default();
    let eye = cam.map(|(_, gt, ..)| gt.translation());
    if let Some((_, gt, proj, _, _)) = cam {
        let (fov, aspect, near, far) = match proj {
            Projection::Perspective(p) => (p.fov.to_degrees(), p.aspect_ratio, p.near, p.far),
            _ => (60.0, 16.0 / 9.0, 1.0, 100_000.0),
        };
        let t = gt.compute_transform();
        let f: Vec3 = t.forward().into();
        let u: Vec3 = t.up().into();
        let r: Vec3 = t.right().into();
        let (sw, sh) = st.child.size();
        let aspect = if sh > 0 { sw as f32 / sh as f32 } else { aspect };
        if st.remix.setup_camera(false, t.translation.to_array(), f.to_array(), u.to_array(), r.to_array(), fov, aspect, near, far.max(near + 1.0)).is_err() {
            st.errors += 1;
        }
    }

    // Shapes.
    // What the camera sees, as Bevy culls it: an interior hides the exterior around it
    // that way [hypothesis: enough for the path tracer's reflections and shadows near the
    // view; wider sets later].
    let mut why = [0u32; 4];
    let shadow_only = RenderLayers::layer(crate::sky::SHADOW_LAYER);
    let (first_person, body_shapes) = body.as_ref().map(|b| b.third_person_shapes()).unwrap_or((false, Vec::new()));
    let mut body_shapes: HashSet<Entity> = body_shapes.into_iter().collect();
    // Everything under the third-person rig too: the head's add-ons (eyes, teeth, brows,
    // hair) are their own entities below its nodes.
    if first_person {
        if let Some(root) = body.as_ref().and_then(|b| b.third_root()) {
            let mut stack = vec![root];
            while let Some(e) = stack.pop() {
                if body_shapes.len() > 4096 {
                    break;
                }
                body_shapes.insert(e);
                if let Ok(c) = children.get(e) {
                    stack.extend(c.iter());
                }
            }
        }
    }
    let mut own = 0u32;
    let all = shapes.all();
    terrain_materials(st, &cpu, &all);
    for ShapeIn { entity, mesh, mat, base, gt, vis, skinned, layers, land, decal, .. } in all {
        // Land: a blended layer already in its base's terrain material is not drawn alone.
        // (Test aid: NVF_REMIX_HIDE_LAND leaves the land out.)
        if land && (st.layered.contains(&mesh) || std::env::var_os("NVF_REMIX_HIDE_LAND").is_some()) {
            continue;
        }
        let layers = layers.cloned().unwrap_or_default();
        // In first person, also anything at the eye that is not the first-person arms
        // rig (the face's parts the rig's lists miss) [design: the camera sits inside the
        // head; nothing else is that close].
        let at_eye = first_person
            && eye.is_some_and(|e| gt.translation().distance(e) < 30.0)
            && !layers.intersects(&RenderLayers::layer(crate::sky::ARMS_LAYER));
        let own_body = first_person && (body_shapes.contains(&entity) || at_eye);
        // The head itself, around the eye, is not sent in first person: Remix still traced
        // it from the camera as the player model [observed: its face parts in view; the
        // runtime's player-model masking follows Portal's camera]; the body below keeps
        // the player-model categories for its shadow.
        // The whole third-person body likewise: its skinned head's entity sits at the rig's
        // root, far from the eye. The player casts no Remix shadow in first person yet.
        if at_eye || own_body {
            continue;
        }
        own += u32::from(own_body);
        // The player's body the camera does not see, drawn for its shadow: Remix's
        // third-person body, which casts shadows and reflections but is not seen
        // [documented: remix_c.h, INSTANCE_CATEGORY_BIT_THIRD_PERSON_PLAYER_BODY].
        // The player model (MODEL: kept out of the camera's own rays, in shadows and
        // reflections; BODY: what the runtime places it by) [documented: the runtime's
        // rtx_instance_manager.cpp, m_isPlayerModel from ThirdPersonPlayerModel].
        let player = ffi::CATEGORY_THIRD_PERSON_PLAYER_MODEL | ffi::CATEGORY_THIRD_PERSON_PLAYER_BODY;
        let category = if own_body {
            player
        } else if !layers.intersects(&cam_layers) {
            if !layers.intersects(&shadow_only) {
                continue;
            }
            player
        } else {
            if !vis {
                continue;
            }
            // A decal: Remix lifts it off the surface it lies on [documented: remix_c.h,
            // INSTANCE_CATEGORY_BIT_DECAL_STATIC]; traced level with it, the surface below
            // showed through.
            if decal {
                ffi::CATEGORY_DECAL_STATIC
            } else {
                0
            }
        };
        why[0] += 1;
        let key = (mesh, mat);
        let handle = match st.terrain.get(&mesh).or_else(|| st.meshes.get(&key)) {
            Some(h) => *h,
            None => {
                let Some(data) = cpu.by_id.get(&mesh).cloned() else {
                    why[1] += 1;
                    if why[1] <= 3 && st.frames % 600 == 0 {
                        if let Some(m) = meshes_all.get(mesh) {
                            let names: Vec<&str> = m.try_attributes().map(|a| a.map(|(a, _)| a.name).collect()).unwrap_or_default();
                            info!("remix: no CPU copy of a mesh: topology {:?}, attributes {:?}", m.primitive_topology(), names);
                        }
                    }
                    continue;
                };
                let Some(rm) = material(st, mat, base) else {
                    why[3] += 1;
                    continue;
                };
                if trace_shape(names.as_deref(), base, &data, land, decal) && std::env::var_os("NVF_REMIX_TRACE_HIDE").is_some() {
                    st.hidden.insert(key);
                }
                let skin = data.skin.as_ref().map(|(w, j)| (4u32, w.as_slice(), j.as_slice()));
                // The material's UV transform, baked in: Remix's materials have none (the
                // Pip-Boy's screen shows a part of its canvas through one).
                let t = base.uv_transform;
                let moved: Vec<HardcodedVertex>;
                let verts: &[HardcodedVertex] = if t == bevy::math::Affine2::IDENTITY {
                    &data.verts
                } else {
                    moved = data.verts.iter().map(|v| HardcodedVertex { texcoord: t.transform_point2(Vec2::from(v.texcoord)).to_array(), ..*v }).collect();
                    &moved
                };
                match st.remix.create_mesh_skinned(hash_of(&(key, st.mesh_gen.get(&mesh).copied().unwrap_or(0), st.mat_gen.get(&mat).copied().unwrap_or(0))), verts, &data.idx, rm, skin) {
                    Ok(h) => {
                        st.meshes.insert(key, h);
                        h
                    }
                    Err(_) => {
                        st.errors += 1;
                        continue;
                    }
                }
            }
        };
        if st.hidden.contains(&key) {
            continue;
        }
        // The categories New Vegas RTX's rtx.conf lists give the albedo [documented: the
        // wrapper's api_material.cpp, m.categories from rtxconf::lookup].
        let category = category | listing(st, &base.base_color_texture).0;
        let two_sided = base.double_sided || base.cull_mode.is_none();
        // A skinned mesh: posed by its joints (joint × inverse bind pose, world space, as
        // Bevy skins it) taken into the instance's space, the instance where its entity is
        // [observed: Remix finds the player's body by its instance's position, so an
        // instance left at the origin had its parts made scenery].
        let to_instance = gt.to_matrix().inverse();
        let bones: Vec<ffi::Transform> = match skinned.and_then(|s| bindposes.get(&s.inverse_bindposes).map(|b| (s, b))) {
            Some((s, inv)) if cpu.by_id.get(&mesh).is_some_and(|d| d.skin.is_some()) => s
                .joints
                .iter()
                .zip(inv.iter())
                .take(ffi::MAX_BONES)
                .map(|(j, ib)| nvf_remix::transform_from_cols((to_instance * joints.get(*j).map(|g| g.to_matrix()).unwrap_or(Mat4::IDENTITY) * *ib).to_cols_array_2d()))
                .collect(),
            _ => Vec::new(),
        };
        // LOD terrain (lod.rs gives its material a depth bias of -50 to keep it behind
        // the loaded land where both exist): Remix has no depth bias, so it is sunk a
        // little instead [design: 200 units, unseen at LOD distances; drawn level, its
        // edges stood through the loaded land as streaked sheets, observed 2026-11-09].
        let m = if base.depth_bias <= -50.0 { Mat4::from_translation(Vec3::new(0.0, -LOD_SINK, 0.0)) * gt.to_matrix() } else { gt.to_matrix() };
        let tr = nvf_remix::transform_from_cols(m.to_cols_array_2d());
        if st.remix.draw_instance_skinned(handle, &tr, category, two_sided, &bones, instance_alpha(base.alpha_mode)).is_err() {
            st.errors += 1;
        }
    }

    // Water: a translucent surface in the water type's colours [design: Remix's own
    // translucency stands in for NVForever's screen-space water shader].
    for (mesh, mat, gt, vis) in &shapes.water {
        if !vis.get() {
            continue;
        }
        let key = (mesh.id(), mat.id().untyped());
        let handle = match st.meshes.get(&key) {
            Some(h) => *h,
            None => {
                let Some(data) = cpu.by_id.get(&mesh.id()).cloned() else { continue };
                let Some(w) = shapes.water_m.get(mat.id()) else { continue };
                let rm = match st.materials.get(&key.1) {
                    Some(h) => *h,
                    None => {
                        // The shallow colour's hue as what the water lets through over
                        // 3000 units (about 40 m) [hypothesis: tuned by eye against
                        // NVForever's own water; brightest channel 1, so only the hue
                        // tints].
                        let c = w.params.shallow;
                        let m = c.x.max(c.y).max(c.z).max(1e-3);
                        info!("remix: water {:?} shallow {:?}", key.1, c);
                        // The water's noise (its normal map) gives the surface its waves; Remix
                        // moves it on animated water [documented: rtx_options.h
                        // animatedWaterTextures, instance category ANIMATED_WATER].
                        let waves = w.noise.as_ref().map(|h| hash_of(&h.id())).filter(|x| st.textures.contains(x));
                        match st.remix.create_translucent_material(hash_of(&key.1), [c.x / m, c.y / m, c.z / m], 3000.0, 1.33, waves) {
                            Ok(h) => {
                                st.materials.insert(key.1, h);
                                h
                            }
                            Err(_) => {
                                st.errors += 1;
                                continue;
                            }
                        }
                    }
                };
                match st.remix.create_mesh(hash_of(&key), &data.verts, &data.idx, rm) {
                    Ok(h) => {
                        st.meshes.insert(key, h);
                        h
                    }
                    Err(_) => {
                        st.errors += 1;
                        continue;
                    }
                }
            }
        };
        let tr = nvf_remix::transform_from_cols(gt.to_matrix().to_cols_array_2d());
        if st.remix.draw_instance(handle, &tr, ffi::CATEGORY_ANIMATED_WATER, true).is_err() {
            st.errors += 1;
        }
    }

    // Lights.
    let mut seen = HashSet::new();
    for (e, l, gt, vis) in &lights.points {
        if !vis.get() || l.intensity <= 0.0 {
            continue;
        }
        let p = gt.translation().to_array();
        let radius = 4.0f32;
        // As New Vegas RTX sends an engine light: a D3D9 point light of diffuse colour ×
        // dimmer (display values: the light's colour, its fade, NVForever's lumens over
        // 4π² light units, crate::lights::lumens), attenuation (0, 0, a2) and range its
        // radius, which the runtime converts to a sphere light [documented: the wrapper's
        // game.cpp update_lights; the runtime's rtx_light_utils.cpp calculateRadiance].
        let fade = l.intensity / (4.0 * std::f32::consts::PI * std::f32::consts::PI * crate::sky::light_unit());
        let shown = l.color.to_srgba();
        let diffuse = [shown.red * fade, shown.green * fade, shown.blue * fade];
        let rad = legacy_radiance(diffuse, [0.0, 0.0, st.light_a2], l.range, radius).map(|v| v * st.light_scale);
        let key = hash_of(&(p.map(f32::to_bits), rad.map(f32::to_bits)));
        light(st, e, key, |r| r.create_sphere_light(key, p, radius, rad));
        seen.insert(e);
    }
    // No directional light goes to Remix: its atmosphere is the sun and the moon, placed by
    // the game's clock below, as New Vegas RTX sends point lights only [documented: the
    // plugin's lights.cpp, "only point lights are sent"; the wrapper's comp.cpp sky drive].
    for (e, l, gt) in lights.suns.iter().filter(|_| false) {
        let d: Vec3 = gt.forward().into();
        let c = l.color.to_linear();
        // The game's sun (π × share at full day) in Remix's radiance [hypothesis: tuned
        // by eye against the original with New Vegas RTX; NVF_REMIX_SUN_SCALE].
        let k = l.illuminance / crate::sky::light_unit() * st.sun_scale;
        if st.frames % 600 == 1 {
            info!("remix: sun illuminance {} (unit {}), radiance {k}, first person {first_person}, own body shapes {own} of {}", l.illuminance, crate::sky::light_unit(), body_shapes.len());
        }
        let rad = [c.red * k, c.green * k, c.blue * k];
        let key = hash_of(&(d.to_array().map(f32::to_bits), rad.map(f32::to_bits)));
        light(st, e, key, |r| r.create_distant_light(key, d.to_array(), 0.53, rad));
        seen.insert(e);
    }
    // Interiors: a fill light at the eye in the cell's ambient colour. Bevy lights an
    // interior largely by its ambient term and the cell's directional light, which a
    // path tracer has no counterpart for inside closed walls [design: the easy stand-in;
    // NVF_REMIX_FILL_SCALE, observed 2026-11-09: rooms near black without it].
    if let (Some(sky), Some(eye)) = (lights.sky.as_deref(), eye) {
        if sky.interior.is_some() {
            let c = lights.ambient.color.to_linear();
            let r = 16.0f32;
            let k = lights.ambient.brightness / crate::sky::light_unit() * st.fill_scale / (r * r);
            let rad = [c.red * k, c.green * k, c.blue * k];
            let p = (eye + Vec3::Y * 24.0).to_array();
            if st.frames % 600 == 1 {
                info!("remix: interior fill light {rad:?} (ambient {:?} x {})", lights.ambient.color, lights.ambient.brightness);
            }
            // Keyed by place and colour, as the point lights: made again when the eye moves
            // (re-made under one hash and destroyed, it never showed) [observed].
            let key = hash_of(&("fill", p.map(f32::to_bits), rad.map(f32::to_bits)));
            let e = Entity::PLACEHOLDER;
            light(st, e, key, |r_| r_.create_sphere_light(key, p, r, rad));
            seen.insert(e);
        }
    }
    // Remix's sky (its atmosphere) places its sun by its own settings, fixed in rtx.conf:
    // it showed a day sky at night. The game's sun, set there whenever it moves
    // [observed 2026-11-09: 23:00 in Goodsprings under a blue sky]. Game space: x east,
    // y north, z up; rotation as a compass bearing [hypothesis: the runtime's sunRotation
    // is one].
    if let Some(sky) = lights.sky.as_deref().filter(|s| s.interior.is_none()) {
        sky_drive(st, sky, eye);
    }
    let gone: Vec<Entity> = st.lights.keys().filter(|e| !seen.contains(e)).copied().collect();
    for e in gone {
        if let Some((_, h)) = st.lights.remove(&e) {
            st.remix.destroy_light(h);
        }
    }
    for (_, h) in st.lights.values() {
        if st.remix.draw_light(*h).is_err() {
            st.errors += 1;
        }
    }

    st.why = why;
    // The HUD and menus over the traced image.
    // Every frame, with the last picture read back until a newer one comes (a frame
    // without it showed no HUD: flicker) [observed 2026-11-09].
    if let Some(ui) = &ui {
        if let Some(px) = ui.pixels.lock().ok().and_then(|mut p| p.take()) {
            st.ui_last = Some((px, ui.size));
        }
        if let Some((px, (w, h))) = &st.ui_last {
            if (*w, *h) == ui.size && st.remix.draw_overlay(px, *w, *h, 1.0).is_err() {
                st.errors += 1;
            }
        }
    }
    let t1 = std::time::Instant::now();
    st.build_time += t1 - t0;
    if st.remix.present().is_err() {
        st.errors += 1;
    }
    st.present_time += t1.elapsed();
    st.frames += 1;
    // Test aid: NVF_REMIX_SHOT=<seconds>:<file.bmp> saves what the Remix window shows,
    // that long after the runtime started: the window's own picture, so it need not be in
    // front (nothing over it is taken, and the player's focus is left alone).
    if let Some((secs, path)) = std::env::var("NVF_REMIX_SHOT").ok().and_then(|v| v.split_once(':').map(|(a, b)| (a.parse::<f32>().ok(), b.to_string()))).and_then(|(a, b)| a.map(|a| (a, b))) {
        let t = st.started.elapsed().as_secs_f32();
        if !st.shot_taken && t >= secs {
            st.shot_taken = true;
            match std::fs::write(&path, st.child.capture_bmp()) {
                Ok(()) => info!("remix: saved the window to {path}"),
                Err(e) => warn!("remix: {path}: {e}"),
            }
        }
    }
    if st.frames % 600 == 1 {
        info!("remix: frame {}, {} textures, {} materials, {} meshes, {} lights, {} errors; visible shapes {}, without a CPU mesh {}, without a material {}, waiting for textures {}", st.frames, st.textures.len(), st.materials.len(), st.meshes.len(), st.lights.len(), st.errors, st.why[0], st.why[1], st.why[2], st.why[3]);
        let n = 600.0;
        info!("remix: {:.2} ms a frame building the scene, {:.2} ms in present, {} changed meshes made again", st.build_time.as_secs_f64() * 1000.0 / n, st.present_time.as_secs_f64() * 1000.0 / n, st.remade);
        st.remade = 0;
        st.build_time = Default::default();
        st.present_time = Default::default();
    }
}

fn light(st: &mut RemixState, e: Entity, key: u64, make: impl FnOnce(&nvf_remix::Remix) -> Result<ffi::LightHandle, nvf_remix::Error>) {
    if st.lights.get(&e).is_some_and(|(k, _)| *k == key) {
        return;
    }
    if let Some((_, old)) = st.lights.remove(&e) {
        st.remix.destroy_light(old);
    }
    match make(&st.remix) {
        Ok(h) => {
            st.lights.insert(e, (key, h));
        }
        Err(_) => st.errors += 1,
    }
}

fn material(st: &mut RemixState, id: UntypedAssetId, b: &StandardMaterial) -> Option<ffi::MaterialHandle> {
    if let Some(h) = st.materials.get(&id) {
        return Some(*h);
    }
    // Not yet: a texture the material names has not arrived (images load after the
    // material); made once every one is uploaded or known to fail.
    let first = *st.material_wait.entry(id).or_insert(st.frames);
    // After about 5 s a texture that never came is given up [design].
    if st.frames.saturating_sub(first) < 300 {
        for h in [&b.base_color_texture, &b.normal_map_texture, &b.emissive_texture].into_iter().flatten() {
            let x = hash_of(&h.id());
            if !st.textures.contains(&x) && !st.failed_textures.contains(&x) {
                return None;
            }
        }
    }
    let tex = |h: &Option<Handle<Image>>| h.as_ref().map(|h| hash_of(&h.id())).filter(|x| st.textures.contains(x));
    let c = b.base_color.to_linear();
    let e = b.emissive;
    let (alpha_test, blend) = match b.alpha_mode {
        AlphaMode::Mask(t) => (Some((t * 255.0) as u8), None),
        AlphaMode::Blend | AlphaMode::Premultiplied => (None, Some(ffi::BLEND_ALPHA)),
        // Glows, fire, flashes: added over the picture as emission [documented:
        // surface_shared.h, kAlphaEmissive].
        AlphaMode::Add => (None, Some(ffi::BLEND_ALPHA_EMISSIVE)),
        AlphaMode::Multiply => (None, Some(ffi::BLEND_MULTIPLICATIVE)),
        _ => (None, None),
    };
    // What New Vegas RTX's lists say of the albedo: `rtx.ignoreAlphaOnTextures` drops the
    // alpha test and blend [documented: the wrapper's api_material.cpp, alpha_dropped].
    let listed = listing(st, &b.base_color_texture);
    let (alpha_test, blend) = if listed.1 { (None, None) } else { (alpha_test, blend) };
    let glow = st.lists.glow_intensity;
    let desc = nvf_remix::MaterialDesc {
        hash: hash_of(&(id, st.mat_gen.get(&id).copied().unwrap_or(0))),
        albedo: tex(&b.base_color_texture),
        // LOD terrain's normal maps are in model space (lod.rs), not the tangent space
        // Remix reads: left out.
        normal: if b.depth_bias <= -50.0 { None } else { tex(&b.normal_map_texture) },
        // An unlit surface (a screen) glows with its own picture [design: Remix lights
        // every surface; emission is how a picture shows unlit].
        emissive: if b.unlit { tex(&b.base_color_texture) } else { tex(&b.emissive_texture) },
        // A glow map at New Vegas RTX's glow intensity (rtx.fnvProtocol.glowEmissiveIntensity)
        // [documented: the wrapper's api_material.cpp, glow_intensity].
        emissive_intensity: if b.unlit { 1.0 } else if e.red + e.green + e.blue > 0.0 { if b.emissive_texture.is_some() { glow } else { 1.0 } } else { 0.0 },
        emissive_color: if b.unlit { [c.red, c.green, c.blue] } else { [e.red, e.green, e.blue] },
        albedo_constant: [c.red, c.green, c.blue],
        opacity: c.alpha,
        roughness: b.perceptual_roughness,
        metallic: b.metallic,
        // Alpha-tested and alpha-blended: by the texture's alpha times the vertex
        // colour's, as Bevy draws them (the sand drifting over roads fades out by its
        // vertex alpha) [documented: remix_c.h useDrawCallAlphaState]; the state comes
        // with each instance (`instance_alpha`).
        draw_call_alpha: !listed.1 && instance_alpha(b.alpha_mode).is_some(),
        alpha_test,
        blend,
    };
    // A normal map goes as a one-layer material (layer 0 at full weight everywhere): the
    // layer reads it as RGB tangent space with its alpha as gloss, where a plain normal
    // texture would drop the alpha [documented: the wrapper's api_material.cpp, "Layers at
    // weight 1", weightUvScale 0 and offset 0.5].
    let made = match (desc.albedo, desc.normal) {
        (Some(albedo), Some(normal)) => match one_layer(st) {
            Some(weights) => st.remix.create_terrain_material(&nvf_remix::MaterialDesc { normal: None, ..desc.clone() }, &[(albedo, Some(normal))], [weights.0, weights.1], [0.0, 0.0], [0.5, 0.5]),
            None => st.remix.create_material(&desc),
        },
        _ => st.remix.create_material(&desc),
    };
    match made {
        Ok(h) => {
            st.materials.insert(id, h);
            Some(h)
        }
        Err(_) => {
            st.errors += 1;
            None
        }
    }
}

/// The weight textures of a one-layer material: layer 0 at 1, the second set at 0.
fn one_layer(st: &mut RemixState) -> Option<(u64, u64)> {
    if let Some(w) = st.one_layer {
        return Some(w);
    }
    let (a, b) = (hash_of(&"nvforever one layer"), hash_of(&"nvforever no layer"));
    for (h, px) in [(a, [255u8, 0, 0, 0]), (b, [0u8, 0, 0, 0])] {
        let t = nvf_remix::TextureData { hash: h, width: 1, height: 1, mips: 1, format: ffi::FORMAT_R8G8B8A8_UNORM, data: &px, vk_format: 0, encoding: 0, replace: false };
        if st.remix.create_texture(&t).is_err() {
            st.errors += 1;
            return None;
        }
    }
    st.one_layer = Some((a, b));
    st.one_layer
}

/// Remix's sky from the game's, as New Vegas RTX drives it each frame: the sun and the moon
/// (opposite it, in the climate's phase), the camera as the clouds' anchor, and the weather
/// as its class for the weather presets [documented: the wrapper's comp.cpp sky_drive and
/// weather.cpp; option names from the runtime]. Values are sent when they change.
fn sky_drive(st: &mut RemixState, sky: &crate::sky::SkyState, eye: Option<Vec3>) {
    let [x, y, z] = fnv_world::sky::sun_direction(&sky.climate, sky.hour, crate::sky::SUN_SOUTH);
    let elevation = z.clamp(-1.0, 1.0).asin().to_degrees();
    let rotation = x.atan2(y).to_degrees().rem_euclid(360.0);
    let moon_rotation = (-x).atan2(-y).to_degrees().rem_euclid(360.0);
    // The game's phase index runs 0 full .. 4 new; the sky's phase 0 new, 0.5 full.
    let phase = ((sky.moon_phase() + 4) & 7) as f32 / 8.0;
    let mut values: Vec<(&'static str, String)> = vec![
        ("rtx.atmosphere.sunElevation", format!("{elevation:.2}")),
        ("rtx.atmosphere.sunRotation", format!("{rotation:.2}")),
        ("rtx.atmosphere.moon0.enabled", "1".into()),
        ("rtx.atmosphere.moon0.elevation", format!("{:.2}", -elevation)),
        ("rtx.atmosphere.moon0.rotation", format!("{moon_rotation:.2}")),
        ("rtx.atmosphere.moon0.phase", format!("{phase:.3}")),
        ("rtx.weather.gameWeather", weather_id(&sky.weather)),
        ("rtx.weather.gameWeatherOutgoing", sky.prev.as_ref().map(weather_id).unwrap_or_default()),
        ("rtx.weather.gameWeatherBlend", if sky.prev.is_some() { format!("{:.2}", sky.fade.clamp(0.0, 1.0)) } else { "1".into() }),
    ];
    if let Some(e) = eye {
        values.push(("rtx.atmosphere.useCameraWorldOverride", "True".into()));
        values.push(("rtx.atmosphere.cameraWorldOverride", format!("{:.1}, {:.1}, {:.1}", e.x, e.y, e.z)));
    }
    for (k, v) in values {
        if st.pushed.get(k) == Some(&v) {
            continue;
        }
        if st.remix.set_config(k, &v).is_err() {
            st.errors += 1;
        }
        if k == "rtx.weather.gameWeather" || (k == "rtx.atmosphere.sunElevation" && st.frames % 600 == 1) {
            info!("remix: {k} = {v} (game hour {:.2})", sky.hour);
        }
        st.pushed.insert(k, v);
    }
}

/// A weather as New Vegas RTX names it to the weather presets: its FormID and its class
/// (fnv.clear, fnv.haze, …) from the record's flags, fog, wind and thunder, with the
/// wrapper's thresholds [documented: its weather.cpp classify and remix-comp.ini
/// [Weather]: FogFarMax 20000, HazeFogFarMax 60000, DustWindMin 100, StormThunderMax 200].
fn weather_id(w: &fnv_world::sky::Weather) -> String {
    let fog = if w.fog[1] > 0.0 { w.fog[1] } else { w.fog[3].max(0.0) };
    let windy = w.wind_speed >= 100;
    let cloudy = w.classification & 2 != 0;
    let class = if w.classification & 4 != 0 {
        if w.lightning_frequency <= 200 { "storm" } else { "rain" }
    } else if w.classification & 8 != 0 {
        "snow"
    } else if fog > 0.0 && fog <= 20000.0 {
        if windy { "dust" } else { "fog" }
    } else if fog > 0.0 && fog <= 60000.0 {
        if windy {
            "dust"
        } else if cloudy {
            "overcast"
        } else {
            "haze"
        }
    } else if cloudy {
        "cloudy"
    } else {
        "clear"
    };
    format!("{:08X}, fnv.{class}", w.form_id)
}

/// The install's weather presets in NVForever's folder, each bound to the weather classes
/// the install's remix-comp.ini gives it ([Weather] Clear=Fair, …), as the wrapper binds
/// them at start [documented: its weather.cpp bind_presets]. Returns the folder.
fn bind_weather_presets(root: &std::path::Path, work: &std::path::Path) -> Option<std::path::PathBuf> {
    let src = root.join("rtx-remix").join("weather");
    let dst = work.join("weather");
    std::fs::create_dir_all(&dst).ok()?;
    let ini = std::fs::read_to_string(root.join("remix-comp.ini")).unwrap_or_default();
    let mut section = false;
    let mut binds: HashMap<String, Vec<String>> = HashMap::new();
    for line in ini.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.eq_ignore_ascii_case("[Weather]");
            continue;
        }
        if !section || t.starts_with(';') {
            continue;
        }
        let Some((k, v)) = t.split_once('=') else { continue };
        let class = k.trim().to_ascii_lowercase();
        if ["clear", "haze", "cloudy", "overcast", "fog", "dust", "rain", "storm", "snow"].contains(&class.as_str()) {
            binds.entry(v.trim().to_ascii_lowercase()).or_default().push(format!("fnv.{class}"));
        }
    }
    for e in std::fs::read_dir(&src).ok()?.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("conf") {
            continue;
        }
        let stem = path.file_stem().and_then(|x| x.to_str()).unwrap_or_default().to_ascii_lowercase();
        let ids = binds.get(&stem).map(|v| v.join(", ")).unwrap_or_default();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let out: String = text
            .lines()
            .map(|l| if l.trim_start().starts_with("weather.bindings") { format!("weather.bindings = {ids}") } else { l.to_string() })
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(dst.join(path.file_name()?), out + "\n");
    }
    Some(dst)
}

/// The runtime's conversion of a D3D9 point light (diffuse colour, attenuation a0 a1 a2,
/// range) to the radiance of a sphere light of `sphere` radius, with least squares on
/// (its default) [documented: the runtime's rtx_light_utils.cpp leastSquareIntensity,
/// calculateIntensity, calculateRadiance; kLegacyLightEndValue 1/255, kNewLightEndValue
/// 0.01, lightConversionIntensityFactor 1].
fn legacy_radiance(diffuse: [f32; 3], att: [f32; 3], range: f32, sphere: f32) -> [f32; 3] {
    const EPS: f32 = 1e-6;
    const LEGACY_END: f32 = 1.0 / 255.0;
    const NEW_END: f32 = 0.01;
    let [a0, a1, a2] = att;
    let b = diffuse[0].max(diffuse[1]).max(diffuse[2]);
    if b <= 0.0 {
        return [0.0; 3];
    }
    let least_squares = |range: f32| -> f32 {
        let mut low = 0.0f32;
        if a2 < EPS {
            if a1 > EPS {
                low = (1.0 / 0.1 - a0) / a1;
            }
        } else {
            let c = a0 - 1.0 / 0.1;
            let disc = a1 * a1 - 4.0 * a2 * c;
            if disc >= 0.0 {
                let r = disc.sqrt();
                for root in [(-a1 + r) / (2.0 * a2), (-a1 - r) / (2.0 * a2)] {
                    if root > 0.0 {
                        low = root;
                    }
                }
            }
        }
        let range = if low > 0.0 { range.min(low) } else { range };
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for i in 0..5 {
            let x = (i + 1) as f32 / 5.0 * range;
            let x2 = x * x;
            let ii = b / (a2 * x2 + a1 * x + a0);
            num += ii / x2;
            den += 1.0 / (x2 * x2);
        }
        num / den
    };
    let end = if a0 > 0.0 && b / a0 < LEGACY_END {
        0.0
    } else if a2 < EPS && a1 <= EPS {
        range
    } else {
        (least_squares(range) / LEGACY_END).sqrt()
    };
    let intensity = NEW_END / (std::f32::consts::PI * sphere * sphere) * end * end;
    diffuse.map(|d| d / b * intensity)
}

/// What New Vegas RTX's rtx.conf lists say of a texture: instance categories and whether
/// its alpha is dropped.
fn listing(st: &RemixState, t: &Option<Handle<Image>>) -> (u32, bool) {
    t.as_ref().and_then(|h| st.d3d9.get(&hash_of(&h.id()))).and_then(|d| st.lists.by_hash.get(d)).copied().unwrap_or((0, false))
}

#[derive(Default)]
struct RemixStateCell {
    state: Option<RemixState>,
    failed: bool,
}

fn start(config: &RemixConfig, parent: *mut std::ffi::c_void) -> Result<RemixState, String> {
    // The install's root (rtx.conf, dxvk.conf, rtx-remix\mods): the DLL's folder's
    // parent when the DLL is in `trex`. Only read: Remix runs in a folder of NVForever's
    // home with copies of the two settings files, writes its caches, logs and captures
    // there and reads the install's replacement assets where they are (DEFAULT_MODS_DIR;
    // the runtime's util_filesys.h) [design: NVForever writes nothing into a mod or game
    // folder; the first runs wrote caches into the install's root].
    let root = config.dll.parent().map(|d| if d.file_name().is_some_and(|n| n.eq_ignore_ascii_case("trex")) { d.parent().unwrap_or(d) } else { d }).map(std::path::Path::to_path_buf);
    let home = fnv_install::EngineHome::user().map(|h| h.logs().parent().map(|p| p.join("remix")).unwrap_or_default()).filter(|p| !p.as_os_str().is_empty());
    if let Some(work) = &home {
        let _ = std::fs::create_dir_all(work.join("logs"));
        let _ = std::fs::create_dir_all(work.join("captures"));
        if let Some(r) = &root {
            for f in ["rtx.conf", "dxvk.conf"] {
                if r.join(f).is_file() {
                    let _ = std::fs::copy(r.join(f), work.join(f));
                }
            }
            // NVForever's settings over the install's, in NVForever's copy: no welcome
            // window (it would show at every start: the runtime keeps "never show again"
            // under %LOCALAPPDATA%\NVIDIA, which a locked-down folder refuses) [documented:
            // the runtime's first-use guide shows unless
            // rtx.automation.disableBlockingDialogBoxes].
            let conf = work.join("rtx.conf");
            let mut text = std::fs::read_to_string(&conf).unwrap_or_default();
            // No splash banner either ("Welcome to New Vegas RTX … Closing in", across the
            // top for 20 s) [documented: the runtime's dxvk_imgui_splash, hideSplashMessage].
            // No input sink window for the runtime's menu: it registers the process's raw
            // mouse and keyboard to itself (the keyboard RIDEV_NOLEGACY), and a process has
            // one registration per device, so NVForever got neither mouse looks nor keys
            // [documented: the runtime's rtx_overlay_window.cpp and dxvk_imgui.cpp,
            // useNewGuiInputMethod; observed 2026-11-09, the owner could not move or press].
            text.push_str("\n# NVForever\nrtx.automation.disableBlockingDialogBoxes = True\nrtx.hideSplashMessage = True\nrtx.useNewGuiInputMethod = False\n");
            if let Some(dir) = bind_weather_presets(r, work) {
                text.push_str(&format!("rtx.weather.presetDirectory = {}\n", dir.to_string_lossy().replace('\\', "/")));
            }
            let _ = std::fs::write(&conf, text);
            // The player's own settings (user.conf: preset, frame generation, …), read by
            // the runtime from the folder it runs in [documented: rtx_option_layer.cpp,
            // the user layer]: the install's, else NVForever.ini's `[Render] RemixUserConf`
            // or NVF_REMIX_USER_CONF (an MO2 setup keeps it in a mod of its own, `NV RTX
            // tuning`). Copied when newer than the copy, so what the runtime's menu saved
            // there stays [design].
            let user_src = Some(r.join("user.conf")).filter(|p| p.is_file()).or_else(|| {
                let from_ini = fnv_install::EngineHome::user().and_then(|h| std::fs::read_to_string(h.config_file()).ok()).and_then(|t| fnv_install::ini_get(&t, "Render", "RemixUserConf").map(|v| v.trim().to_string()));
                from_ini.or_else(|| std::env::var("NVF_REMIX_USER_CONF").ok()).map(std::path::PathBuf::from).filter(|p| p.is_file())
            });
            if let Some(src) = user_src {
                let dst = work.join("user.conf");
                let newer = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
                if newer(&dst).is_none_or(|d| newer(&src).is_some_and(|s| s > d)) {
                    match std::fs::copy(&src, &dst) {
                        Ok(_) => info!("remix: user settings from {}", src.display()),
                        Err(e) => warn!("remix: {}: {e}", src.display()),
                    }
                }
            }
            let mods = r.join("rtx-remix").join("mods");
            if mods.is_dir() {
                std::env::set_var("DEFAULT_MODS_DIR", &mods);
            }
        }
        std::env::set_var("DXVK_LOG_PATH", work.join("logs"));
        std::env::set_var("DXVK_CAPTURE_PATH", work.join("captures"));
        std::env::set_var("DXVK_CONFIG_FILE", work.join("dxvk.conf"));
        let _ = std::env::set_current_dir(work);
    }
    let lists = home
        .as_ref()
        .map(|w| w.join("rtx.conf"))
        .or_else(|| root.as_ref().map(|r| r.join("rtx.conf")))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| RtxLists::read(&t))
        .unwrap_or(RtxLists { glow_intensity: 1.0, ..Default::default() });
    info!("remix: rtx.conf lists name {} textures; glow intensity {}", lists.by_hash.len(), lists.glow_intensity);
    let child = nvf_remix::window::Child::new(parent).ok_or("could not make the Remix child window")?;
    let mut remix = nvf_remix::Remix::load(&config.dll)?;
    remix.startup(child.hwnd.cast()).map_err(|e| e.to_string())?;
    // Bevy's space: Y up, right-handed [design: NVForever's scene].
    // The sky's own day cycle off: NVForever's clock places its sun (frame).
    for (k, v) in [("rtx.zUp", "False"), ("rtx.leftHandedCoordinateSystem", "False"), ("rtx.atmosphere.timeCycleEnable", "False")] {
        if let Err(e) = remix.set_config(k, v) {
            warn!("remix: {k}: {e}");
        }
    }
    let scale = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    info!("remix: started {} in the window", config.dll.display());
    Ok(RemixState {
        remix,
        child,
        textures: HashSet::new(),
        failed_textures: HashSet::new(),
        material_wait: HashMap::new(),
        terrain: HashMap::new(),
        layered: HashSet::new(),
        materials: HashMap::new(),
        meshes: HashMap::new(),
        lights: HashMap::new(),
        frames: 0,
        errors: 0,
        light_scale: scale("NVF_REMIX_LIGHT_SCALE", 1.0),
        sun_scale: scale("NVF_REMIX_SUN_SCALE", 2.0),
        started: std::time::Instant::now(),
        why: [0; 4],
        shot_taken: false,
        light_a2: scale("NVF_REMIX_LIGHT_A2", 0.001),
        d3d9: HashMap::new(),
        lists,
        one_layer: None,
        pushed: HashMap::new(),
        mesh_gen: HashMap::new(),
        mat_gen: HashMap::new(),
        remade: 0,
        fill_scale: scale("NVF_REMIX_FILL_SCALE", 20000.0),
        ui_last: None,
        hidden: HashSet::new(),
        build_time: Default::default(),
        present_time: Default::default(),
    })
}

/// How far LOD terrain is sunk under the loaded land (game units, Bevy's up is +y).
const LOD_SINK: f32 = 200.0;

/// The alpha state an instance of a material with this alpha mode carries: alpha-tested
/// and alpha-blended ones only (glows and multiplied shapes keep their blend type).
fn instance_alpha(mode: AlphaMode) -> Option<nvf_remix::InstanceAlpha> {
    match mode {
        AlphaMode::Mask(t) => Some(nvf_remix::InstanceAlpha { test: Some((t * 255.0) as u8), blend: false }),
        AlphaMode::Blend | AlphaMode::Premultiplied => Some(nvf_remix::InstanceAlpha { test: None, blend: true }),
        _ => None,
    }
}

/// Test aid: `NVF_REMIX_TRACE=<text>` logs each shape sent whose albedo texture's path
/// holds that text (lower case), or `bias=<n>` for the material's depth bias: its material
/// and mesh as Remix gets them.
fn trace_shape(names: Option<&nvf_assets::texture::TextureCache>, b: &StandardMaterial, data: &CpuMesh, land: bool, decal: bool) -> bool {
    static WANT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let Some(want) = WANT.get_or_init(|| std::env::var("NVF_REMIX_TRACE").ok().map(|s| s.to_lowercase())) else { return false };
    let path = |h: &Option<Handle<Image>>| h.as_ref().and_then(|h| names.and_then(|n| n.name_of(h.id()))).map(str::to_lowercase).unwrap_or_default();
    let albedo = path(&b.base_color_texture);
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    let (mut amin, mut amax) = (255u32, 0u32);
    for v in &data.verts {
        for k in 0..2 {
            lo[k] = lo[k].min(v.texcoord[k]);
            hi[k] = hi[k].max(v.texcoord[k]);
        }
        amin = amin.min(v.color >> 24);
        amax = amax.max(v.color >> 24);
    }
    // Matched against the albedo's path, ` bias=<depth bias>`, ` land=<bool>` and
    // ` faded` (some vertex alpha below 255).
    let faded = if amin < 255 { " faded" } else { "" };
    if !format!("{albedo} bias={} land={land}{faded}", b.depth_bias).contains(want.as_str()) {
        return false;
    }
    info!(
        "remix trace: {albedo} normal {} alpha {:?} land {land} decal {decal} unlit {} uv_transform {:?} colour {:?}; {} verts, uv {lo:?}..{hi:?}, vertex alpha {amin}..{amax}",
        path(&b.normal_map_texture),
        b.alpha_mode,
        b.unlit,
        b.uv_transform,
        b.base_color,
        data.verts.len()
    );
    true
}

/// Remix shut down when the game ends, while its window is still there: dropping it with
/// the rest of the world afterwards crashed on exit [observed: the first HND run,
/// 0xC0000005 after "the game exited normally"].
fn shut_down_on_exit(mut exit: MessageReader<AppExit>, cell: Option<NonSendMut<RemixStateCell>>) {
    if exit.read().next().is_none() {
        return;
    }
    if let Some(mut c) = cell {
        if let Some(st) = c.state.take() {
            // Remix's own Shutdown crashed at exit [observed: 0xC0000005 after "the game
            // exited normally", with the window still up]: the process's exit frees it.
            info!("remix: leaving the runtime to the process's exit after {} frames", st.frames);
            std::mem::forget(st);
        }
        c.failed = true;
    }
}

/// The HUD and menus, drawn by Bevy into an image instead of the window (Remix covers
/// the window) and read back each frame for Remix's screen overlay: a 2D camera that is
/// the default UI camera, with a transparent background.
#[derive(Resource)]
struct UiOverlay {
    image: AssetId<Image>,
    size: (u32, u32),
    pixels: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
    readback: Entity,
}

/// The overlay's readback of `image` at `w`×`h` into `sink`.
fn spawn_ui_readback(commands: &mut Commands, image: Handle<Image>, w: u32, h: u32, sink: Arc<std::sync::Mutex<Option<Vec<u8>>>>) -> Entity {
    use bevy::render::gpu_readback::{Readback, ReadbackComplete};
    // Rows come back padded to 256 bytes [documented: wgpu COPY_BYTES_PER_ROW_ALIGNMENT,
    // bevy_render gpu_readback get_aligned_size].
    let row = (w * 4) as usize;
    let stride = row.div_ceil(256) * 256;
    commands
        .spawn(Readback::texture(image))
        .observe(move |ev: On<ReadbackComplete>| {
            let data = &ev.data;
            // A frame read before a resize has the old size: dropped.
            if data.len() < stride * (h as usize - 1) + row || data.len() > stride * h as usize {
                return;
            }
            let out = if stride == row { data[..row * h as usize].to_vec() } else { data.chunks(stride).take(h as usize).flat_map(|r| r[..row.min(r.len())].iter().copied()).collect() };
            if let Ok(mut p) = sink.lock() {
                *p = Some(out);
            }
        })
        .id()
}

/// The overlay follows the window's size.
fn resize_ui_overlay(mut commands: Commands, mut images: ResMut<Assets<Image>>, window: Query<&Window, With<PrimaryWindow>>, ui: Option<ResMut<UiOverlay>>) {
    let Some(mut ui) = ui else { return };
    let Ok(win) = window.single() else { return };
    let (w, h) = (win.physical_width().max(1), win.physical_height().max(1));
    if (w, h) == ui.size {
        return;
    }
    let Some(handle) = images.get_strong_handle(ui.image) else { return };
    if let Some(mut img) = images.get_mut(ui.image) {
        img.resize(bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
    }
    commands.entity(ui.readback).despawn();
    if let Ok(mut p) = ui.pixels.lock() {
        *p = None;
    }
    ui.readback = spawn_ui_readback(&mut commands, handle, w, h, ui.pixels.clone());
    info!("remix: UI overlay {}x{} -> {w}x{h}", ui.size.0, ui.size.1);
    ui.size = (w, h);
}

fn setup_ui_overlay(mut commands: Commands, mut images: ResMut<Assets<Image>>, window: Query<&Window, With<PrimaryWindow>>) {
    use bevy::asset::RenderAssetUsages;
    use bevy::camera::{ClearColorConfig, RenderTarget};
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureUsages};
    let (w, h) = window.single().map(|w| (w.physical_width().max(1), w.physical_height().max(1))).unwrap_or((1280, 720));
    let mut image = Image::new_uninit(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    let image_id = image.id();
    commands.spawn((
        Name::new("Remix UI camera"),
        Camera2d,
        Camera { order: 1000, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        RenderTarget::Image(image.clone().into()),
        IsDefaultUiCamera,
        Msaa::Off,
    ));
    let pixels: Arc<std::sync::Mutex<Option<Vec<u8>>>> = Arc::default();
    let readback = spawn_ui_readback(&mut commands, image, w, h, pixels.clone());
    commands.insert_resource(UiOverlay { image: image_id, size: (w, h), pixels, readback });
}

/// Each land quadrant's base mesh with the blended layer meshes over it (same vertex grid:
/// NVForever's terrain.rs builds a layer as the base's grid with the layer's opacity in the
/// vertex alpha) as one mesh with a terrain-layers material: up to eight layers (the base
/// and the top seven), weighted per vertex as the layers composite over each other, the
/// weights in two RGBA textures over the quadrant [documented: remix_c.h,
/// MaterialInfoTerrainLayersEXT]. Made once a quadrant's textures are all in Remix.
fn terrain_materials(st: &mut RemixState, cpu: &CpuMeshes, all: &[ShapeIn<'_>]) {
    let mut by_cell: HashMap<Entity, Vec<usize>> = HashMap::new();
    for (i, s) in all.iter().enumerate() {
        if s.land {
            if let Some(p) = s.parent {
                by_cell.entry(p).or_default().push(i);
            }
        }
    }
    let tex = |st: &RemixState, h: &Option<Handle<Image>>| h.as_ref().map(|h| hash_of(&h.id())).filter(|x| st.textures.contains(x));
    for shapes in by_cell.values() {
        for &bi in shapes {
            let b = &all[bi];
            if matches!(b.base.alpha_mode, AlphaMode::Blend) || st.terrain.contains_key(&b.mesh) {
                continue;
            }
            let Some(base) = cpu.by_id.get(&b.mesh) else { continue };
            let n = (base.verts.len() as f32).sqrt().round() as usize;
            if n < 2 || n * n != base.verts.len() {
                continue;
            }
            let first = base.verts[0].position;
            let layers: Vec<(&ShapeIn<'_>, &CpuMesh)> = shapes
                .iter()
                .map(|&li| &all[li])
                .filter(|l| matches!(l.base.alpha_mode, AlphaMode::Blend))
                .filter_map(|l| cpu.by_id.get(&l.mesh).map(|d| (l, d.as_ref())))
                .filter(|(_, d)| d.verts.len() == base.verts.len() && d.verts[0].position == first)
                .collect();
            if layers.is_empty() {
                continue;
            }
            // The top seven layers over the base.
            let top = &layers[layers.len().saturating_sub(ffi::TERRAIN_MAX_LAYERS - 1)..];
            let Some(base_albedo) = tex(st, &b.base.base_color_texture) else { continue };
            let mut list: Vec<(u64, Option<u64>)> = vec![(base_albedo, tex(st, &b.base.normal_map_texture))];
            let mut ok = true;
            for (l, _) in top {
                match tex(st, &l.base.base_color_texture) {
                    Some(a) => list.push((a, tex(st, &l.base.normal_map_texture))),
                    None => ok = false,
                }
            }
            if !ok {
                continue;
            }
            // Weights: layer k shows by its alpha times what the layers above leave.
            let mut w = [vec![0u8; n * n * 4], vec![0u8; n * n * 4]];
            for v in 0..n * n {
                let alpha: Vec<f32> = top.iter().map(|(_, d)| (d.verts[v].color >> 24) as f32 / 255.0).collect();
                let mut above = 1.0f32;
                let mut weights = vec![0.0f32; alpha.len() + 1];
                for k in (0..alpha.len()).rev() {
                    weights[k + 1] = alpha[k] * above;
                    above *= 1.0 - alpha[k];
                }
                weights[0] = above;
                for (k, wk) in weights.iter().enumerate() {
                    w[k / 4][v * 4 + k % 4] = (wk.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
            let hashes = [hash_of(&(b.mesh, 0u8)), hash_of(&(b.mesh, 1u8))];
            for (h, data) in hashes.iter().zip(w.iter()) {
                let t = nvf_remix::TextureData { hash: *h, width: n as u32, height: n as u32, mips: 1, format: ffi::FORMAT_R8G8B8A8_UNORM, data, vk_format: 0, encoding: 0, replace: false };
                if st.remix.create_texture(&t).is_err() {
                    st.errors += 1;
                    ok = false;
                }
            }
            if !ok {
                continue;
            }
            // Vertex (c, r) at weight texel centre ((c + 1/2) / n, (r + 1/2) / n): the vertex
            // grid's texcoords mapped linearly onto the weight texture.
            let uv0 = base.verts[0].texcoord;
            let uv1 = base.verts[n * n - 1].texcoord;
            let axis = |a: usize| {
                let d = (uv1[a] - uv0[a]) / (n - 1) as f32;
                if d.abs() < 1e-9 {
                    (1.0, 0.0)
                } else {
                    let scale = 1.0 / (n as f32 * d);
                    (scale, 0.5 / n as f32 - uv0[a] * scale)
                }
            };
            let (su, ou) = axis(0);
            let (sv, ov) = axis(1);
            let c = b.base.base_color.to_linear();
            let desc = nvf_remix::MaterialDesc {
                hash: hash_of(&(b.mesh, "terrain")),
                albedo: Some(base_albedo),
                albedo_constant: [c.red, c.green, c.blue],
                opacity: 1.0,
                roughness: b.base.perceptual_roughness,
                metallic: b.base.metallic,
                ..Default::default()
            };
            let Ok(mat) = st.remix.create_terrain_material(&desc, &list, hashes, [su, sv], [ou, ov]) else {
                st.errors += 1;
                continue;
            };
            match st.remix.create_mesh(hash_of(&(b.mesh, "terrain-mesh")), &base.verts, &base.idx, mat) {
                Ok(h) => {
                    st.terrain.insert(b.mesh, h);
                    for (l, _) in top {
                        st.layered.insert(l.mesh);
                    }
                }
                Err(_) => st.errors += 1,
            }
        }
    }
}

/// Render-target images read back each frame for Remix (`take_new_images` finds them).
#[derive(Resource, Default)]
struct LiveTextures(Arc<std::sync::Mutex<HashMap<AssetId<Image>, LiveTexture>>>);

struct LiveTexture {
    width: u32,
    height: u32,
    format: u32,
    pixels: Option<Vec<u8>>,
}

fn watch_live_textures(mut commands: Commands, mut cpu: ResMut<CpuMeshes>, mut images: ResMut<Assets<Image>>, live: Res<LiveTextures>) {
    use bevy::render::gpu_readback::{Readback, ReadbackComplete};
    for id in std::mem::take(&mut cpu.new_live) {
        // A GPU copy out of the image needs it marked copyable (Bevy's render targets are
        // not) [observed: wgpu validation, COPY_SRC missing].
        if let Some(mut img) = images.get_mut(id) {
            img.texture_descriptor.usage |= bevy::render::render_resource::TextureUsages::COPY_SRC;
        }
        let Some(img) = images.get(id) else { continue };
        let d = &img.texture_descriptor;
        let (w, h) = (d.size.width, d.size.height);
        let format = match d.format {
            TextureFormat::Rgba8UnormSrgb => ffi::FORMAT_R8G8B8A8_SRGB,
            TextureFormat::Bgra8Unorm => ffi::FORMAT_B8G8R8A8_UNORM,
            TextureFormat::Bgra8UnormSrgb => ffi::FORMAT_B8G8R8A8_SRGB,
            _ => ffi::FORMAT_R8G8B8A8_UNORM,
        };
        let Some(handle) = images.get_strong_handle(id) else { continue };
        if let Ok(mut l) = live.0.lock() {
            l.insert(id, LiveTexture { width: w, height: h, format, pixels: None });
        }
        let sink = live.0.clone();
        // Rows come back padded to 256 bytes (as for the UI overlay).
        let row = (w * 4) as usize;
        let stride = row.div_ceil(256) * 256;
        commands.spawn(Readback::texture(handle)).observe(move |ev: On<ReadbackComplete>| {
            let data = &ev.data;
            let out: Vec<u8> = if stride == row { data.clone() } else { data.chunks(stride).take(h as usize).flat_map(|r| r[..row.min(r.len())].iter().copied()).collect() };
            if let Ok(mut l) = sink.lock() {
                if let Some(t) = l.get_mut(&id) {
                    t.pixels = Some(out);
                }
            }
        });
    }
}
