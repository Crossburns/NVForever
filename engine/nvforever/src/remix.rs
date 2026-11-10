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
// foldhash, not std's SipHash: the frame looks shapes up by asset id thousands of times
// [observed 2026-11-10: SipHash about 0.4 s of 19 s on the main thread].
use bevy::platform::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::Arc;

/// Where the Remix runtime is: `--remix` or `NVF_REMIX`.
#[derive(Resource, Clone)]
pub struct RemixConfig {
    pub dll: PathBuf,
    /// The game folder (the data folder's parent): under MO2 the merged view, where the
    /// installed Remix mods appear (`rtx-remix\mods`).
    pub game_dir: Option<PathBuf>,
}

pub struct RemixPlugin(pub RemixConfig);

impl Plugin for RemixPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().insert_non_send(RemixStateCell::default());
        app.insert_resource(RemixDraws);
        app.insert_resource(self.0.clone())
            .init_resource::<CpuMeshes>()
            .init_resource::<nvf_play::player::OverlayMenu>()
            .add_systems(PreUpdate, |mut m: ResMut<nvf_play::player::OverlayMenu>| {
                let open = nvf_remix::window::menu_open();
                if m.0 != open {
                    m.0 = open;
                    info!("remix: the runtime's menu {}", if open { "open: the pointer is free" } else { "closed" });
                }
            })
            .add_systems(Startup, setup_ui_overlay)
            .add_systems(PostUpdate, resize_ui_overlay)
            .init_resource::<LiveTextures>()
            .add_systems(PostUpdate, ((take_new_meshes, take_new_images, take_changed_materials), watch_live_textures, pace_live_readbacks).chain().after(AssetEventSystems))
            .add_systems(Last, (frame, shut_down_on_exit).chain());
        // Bevy's own 3D pass under the Remix window is never seen: its main pass is drawn
        // at a few pixels (NVF_REMIX_BEVY_FULL=1 keeps it whole). Visibility, which Remix's
        // scene is chosen by, is Bevy's culling of the camera and is left as it was.
        if std::env::var_os("NVF_REMIX_BEVY_FULL").is_none() {
            if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
                render_app.add_systems(
                    bevy::render::Render,
                    shrink_hidden_pass.in_set(bevy::render::RenderSystems::PrepareViews).before(bevy::render::view::prepare_view_targets),
                );
            }
        }
    }
}

/// The window's 3D views get a main pass of `HIDDEN_PASS` pixels (render world, each frame,
/// as FSR sets its own [documented: bevy_camera MainPassResolutionOverride]).
fn shrink_hidden_pass(mut commands: Commands, views: Query<(Entity, &bevy::render::camera::ExtractedCamera, &bevy::render::view::ExtractedView)>) {
    const HIDDEN_PASS: UVec2 = UVec2::new(64, 36);
    for (e, cam, view) in &views {
        if !matches!(cam.target, Some(bevy::camera::NormalizedRenderTarget::Window(_))) {
            continue;
        }
        if view.viewport.z > HIDDEN_PASS.x && view.viewport.w > HIDDEN_PASS.y {
            commands.entity(e).insert(bevy::camera::MainPassResolutionOverride(HIDDEN_PASS));
        }
    }
}

/// Present while the Remix runtime draws the world: what Bevy draws under its window is
/// not seen (`pipboy_arm` does not wait for it).
#[derive(Resource)]
pub struct RemixDraws;

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
    /// World textures whose pixels went to Remix and whose Bevy image was cut down to one
    /// 4x4 level (`take_new_images`).
    shrunk: HashSet<AssetId<Image>>,
    /// Images this adapter just cut down (one Modified event of its own each, skipped);
    /// a later change of the image is a real one and goes to Remix again.
    own_change: HashSet<AssetId<Image>>,
    /// Meshes this adapter just emptied (`take_new_meshes`), their own Modified skipped.
    own_mesh_change: HashSet<AssetId<Mesh>>,
    /// Meshes, images and NIF materials the game let go (Removed or Unused): their Remix
    /// counterparts go too (`release_gone`).
    gone_meshes: Vec<AssetId<Mesh>>,
    gone_images: Vec<AssetId<Image>>,
    gone_materials: Vec<UntypedAssetId>,
}

/// A vertex as kept between frames: Remix's vertex without its 28 bytes of padding, widened
/// only when it goes to the runtime [observed 2026-11-10: 3688 meshes held 1372 MB at 64
/// bytes a vertex in Goodsprings].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    texcoord: [f32; 2],
    color: u32,
}

impl CpuVertex {
    fn hw(&self) -> HardcodedVertex {
        HardcodedVertex { position: self.position, normal: self.normal, texcoord: self.texcoord, color: self.color, ..Default::default() }
    }
}

/// The runtime's vertices of `v`.
fn wide(v: &[CpuVertex]) -> Vec<HardcodedVertex> {
    v.iter().map(CpuVertex::hw).collect()
}

/// A mesh's triangles, and for a skinned one four joint weights and indices a vertex.
struct CpuMesh {
    verts: Vec<CpuVertex>,
    idx: Vec<u32>,
    skin: Option<(Vec<f32>, Vec<u32>)>,
    /// The key the original's draw of this geometry has for Remix mods (`geometry_key`).
    asset_key: u64,
}

/// The mesh key New Vegas RTX gives a static geometry, from its positions in its own space
/// and its triangles in their stored order: XXH3 over the positions the triangles use
/// (lowest index first, each seeded with the hash so far), XXH3 of the indices, the
/// runtime's descriptor hash (index and vertex counts, triangle list, 32-bit indices),
/// chained with XXH64 [documented: the wrapper's api_asset_keys.cpp of() and
/// descriptor_hash; the runtime's rtx_hashing.cpp].
fn geometry_key(positions: &[[f32; 3]], indices: &[u32]) -> u64 {
    use twox_hash::{XxHash3_64, XxHash64};
    if positions.is_empty() || indices.is_empty() {
        return 0;
    }
    let mut used = vec![false; positions.len()];
    for &i in indices {
        if let Some(u) = used.get_mut(i as usize) {
            *u = true;
        }
    }
    let mut pos = 0u64;
    for (v, p) in positions.iter().enumerate() {
        if used[v] {
            let b: Vec<u8> = p.iter().flat_map(|f| f.to_le_bytes()).collect();
            pos = XxHash3_64::oneshot_with_seed(pos, &b);
        }
    }
    let ib: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
    let idx = XxHash3_64::oneshot(&ib);
    let mut desc = XxHash3_64::oneshot_with_seed(0, &(indices.len() as u32).to_le_bytes());
    desc = XxHash3_64::oneshot_with_seed(desc, &(positions.len() as u32).to_le_bytes());
    desc = XxHash3_64::oneshot_with_seed(desc, &3u32.to_le_bytes());
    desc = XxHash3_64::oneshot_with_seed(desc, &1u32.to_le_bytes());
    let key = XxHash64::oneshot(pos, &idx.to_le_bytes());
    let key = XxHash64::oneshot(key, &desc.to_le_bytes());
    key.max(1)
}

/// A geometry's name scoped by its diffuse (shared_mesh_keys.txt names meshes so)
/// [documented: the wrapper's shared_mesh_keys.hpp scoped_name].
fn scoped_name(key: u64, diffuse: u64) -> u64 {
    twox_hash::XxHash64::oneshot(key, &diffuse.to_le_bytes()).max(1)
}

/// The names a Remix mod's shared_mesh_keys.txt gives geometry by diffuse: lines of
/// `<key hex16> <diffuse hex16>` [documented: the wrapper's shared_mesh_keys.hpp
/// parse_scoped_line; api_asset_keys.cpp load_table].
fn shared_mesh_names(mods: &std::path::Path) -> HashSet<u64> {
    let Ok(text) = std::fs::read_to_string(mods.join("ConvertedMods").join("shared_mesh_keys.txt")) else { return HashSet::new() };
    text.lines()
        .filter_map(|l| {
            let l = l.trim_end_matches('\r');
            (l.len() == 33 && l.as_bytes()[16] == b' ').then(|| Some(scoped_name(u64::from_str_radix(&l[..16], 16).ok()?, u64::from_str_radix(&l[17..], 16).ok()?))).flatten()
        })
        .collect()
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
    /// Its sampler's addressing, U and V, in Remix's terms (`remix_wrap`).
    wrap: [u8; 2],
    /// New pixels of an image Remix may have already: replaced in place.
    modified: bool,
}

/// An image's addressing as Remix's wrap modes (0 clamp, 1 repeat, 2 mirrored, 3 clip): the
/// sampler NVForever gave it, which carries the NIF's clamp mode (texture.rs loads a
/// clamped copy for a clamping shape), else the app's default, repeat (main.rs ImagePlugin)
/// [documented: the runtime's mdl_helpers.h WrapMode; the wrapper's api_material.cpp passes
/// each axis].
fn remix_wrap(sampler: &bevy::image::ImageSampler) -> [u8; 2] {
    use bevy::image::{ImageAddressMode as A, ImageSampler};
    let one = |m: A| match m {
        A::ClampToEdge => 0,
        A::Repeat => 1,
        A::MirrorRepeat => 2,
        A::ClampToBorder => 3,
    };
    match sampler {
        ImageSampler::Descriptor(d) => [one(d.address_mode_u), one(d.address_mode_v)],
        ImageSampler::Default => [1, 1],
    }
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
    /// The lists as the runtime resolves its layers: each file in order (rtx.conf, then
    /// user.conf) adds its positive entries to a list and takes its negative ("-0x…")
    /// entries out of that list only; a texture's categories and alpha drop come from the
    /// lists it ends up in [documented: the wrapper's api_rtxconf.cpp, "each list on its
    /// own: its positive entries minus its own negative entries"].
    fn read(texts: &[String]) -> RtxLists {
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
        let mut sets: Vec<HashSet<u64>> = vec![HashSet::new(); LISTS.len()];
        for text in texts {
            for line in text.lines() {
                let Some((k, v)) = line.split_once('=') else { continue };
                let k = k.trim();
                if k == "rtx.fnvProtocol.glowEmissiveIntensity" {
                    out.glow_intensity = v.trim().parse().unwrap_or(out.glow_intensity);
                    continue;
                }
                let Some(n) = LISTS.iter().position(|l| l.0 == k) else { continue };
                for item in v.split(',') {
                    let item = item.trim();
                    let (neg, hex) = match item.strip_prefix('-') {
                        Some(r) => (true, r),
                        None => (false, item),
                    };
                    let Some(hex) = hex.strip_prefix("0x").or_else(|| hex.strip_prefix("0X")) else { continue };
                    let Ok(h) = u64::from_str_radix(hex, 16) else { continue };
                    if neg {
                        sets[n].remove(&h);
                    } else {
                        sets[n].insert(h);
                    }
                }
            }
        }
        for (n, set) in sets.iter().enumerate() {
            let (_, cat, drop) = LISTS[n];
            for h in set {
                let e = out.by_hash.entry(*h).or_default();
                e.0 |= cat;
                e.1 |= drop;
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
    why: [u32; 6],
    shot_taken: bool,
    /// shared_mesh_keys.txt's names of geometry by diffuse.
    mesh_names: HashSet<u64>,
    /// Materials made without a texture, by that texture's hash.
    mat_missing: HashMap<u64, Vec<UntypedAssetId>>,
    /// Each uploaded texture's addressing (Remix wrap modes, U and V).
    wrap: HashMap<u64, [u8; 2]>,
    /// Where the eye was when each falloff shape's vertices were last faded.
    falloff_eye: HashMap<(AssetId<Mesh>, UntypedAssetId), Vec3>,
    /// Raw input is shared with the runtime's menu window.
    input_shared: bool,
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
    /// Changed meshes refreshed in place since the last stats line.
    refreshed: u64,
    /// Each made mesh's vertex count, index count and Remix hash.
    mesh_shape: HashMap<(AssetId<Mesh>, UntypedAssetId), (usize, usize, u64)>,
    /// Interior fill light's strength (NVF_REMIX_FILL_SCALE).
    fill_scale: f32,
    /// How far from the eye shapes the camera does not see stay in the traced scene
    /// (NVF_REMIX_MEMBER_RADIUS, game units).
    member_radius: f32,
    /// Textures waiting for their upload, oldest first, and their hashes: a frame uploads
    /// at most `UPLOAD_BYTES_A_FRAME` of them (the runtime keeps each upload's pixels
    /// until the GPU has them; every texture of a cell in one frame cost gigabytes of
    /// memory at once) [observed 2026-11-09: 6 to 15 GB private memory within 5 s of the
    /// first frame, and a failed allocation when the machine had 10 GB to give].
    upload_queue: std::collections::VecDeque<(AssetId<Image>, Arc<PendingImage>)>,
    queued: HashSet<u64>,
    /// The UI overlay's last picture and its size.
    ui_last: Option<(Vec<u8>, (u32, u32))>,
    /// Shapes left out (test aid: NVF_REMIX_TRACE_HIDE).
    hidden: HashSet<(AssetId<Mesh>, UntypedAssetId)>,
    /// Time in `frame` building the scene and in `present`, since the last stats line.
    build_time: std::time::Duration,
    present_time: std::time::Duration,
    /// Static shapes as retained instances, by entity: registered once, then only moved,
    /// shown or hidden, and replayed by one DrawRetainedInstances a frame
    /// (`NVF_REMIX_RETAINED=0`: a DrawInstance each a frame, as before) [observed
    /// 2026-11-10: about 4300 draws a frame cost the runtime's render thread 14-15 ms a
    /// frame in Goodsprings, rtx.profile.detailedLogging].
    retained: HashMap<Entity, Retained>,
    use_retained: bool,
    /// Meshes that changed after Remix had them (particles, morphs): drawn each frame.
    dynamic: HashSet<AssetId<Mesh>>,
    /// Retained instances made or described again, and moved, since the last stats line.
    ret_made: u64,
    ret_moved: u64,
    /// Uploaded texture contents (format, size, levels, pixels) by their first hash, and the
    /// bytes uploaded again for an image of the same contents (a clamped copy, a second
    /// handle of one file).
    contents: HashMap<u64, u64>,
    same_bytes: u64,
    same_count: u64,
    /// Textures not uploaded because one of the same contents was: their materials name
    /// that one (`remix_tex`), each with its own addressing and D3D9 hash, which stay
    /// under the image's own hash [observed 2026-11-10: 103 textures, 366 MB, in
    /// Goodsprings: clamped copies and second handles of one file].
    alias: HashMap<u64, u64>,
    /// Textures whose image is gone but which images of the same contents still share.
    orphans: HashSet<u64>,
    /// What `release_gone` freed since the last stats line: meshes, materials, textures.
    released: [u64; 3],
}

/// The texture Remix has for the image of hash `h`: its own, or the one of the same
/// contents uploaded first.
fn remix_tex(st: &RemixState, h: u64) -> u64 {
    st.alias.get(&h).copied().unwrap_or(h)
}

/// One retained instance: what it was last described and placed as.
struct Retained {
    handle: u64,
    mesh: ffi::MeshHandle,
    categories: u32,
    two_sided: bool,
    alpha: Option<nvf_remix::InstanceAlpha>,
    transform: ffi::Transform,
    visible: bool,
    /// The frame it was last drawn in.
    seen: u64,
}

/// What the game let go leaves Remix too, in the order the runtime needs: retained
/// instances, then meshes, then materials, then textures. A loaded asset is a new asset id,
/// so nothing freed here is asked for again [observed 2026-11-10: nothing was freed; eight
/// jumps across the map in four minutes took private memory 14.5 -> 22.7 GB and the card
/// 9.0 -> 14.4 GB, Remix meshes 1588 -> 5276, textures 1664 -> 4523].
fn release_gone(st: &mut RemixState, cpu: &mut CpuMeshes) {
    let gone_meshes: HashSet<AssetId<Mesh>> = std::mem::take(&mut cpu.gone_meshes).into_iter().collect();
    let gone_mats: HashSet<UntypedAssetId> = std::mem::take(&mut cpu.gone_materials).into_iter().collect();
    let gone_images = std::mem::take(&mut cpu.gone_images);
    if !gone_meshes.is_empty() || !gone_mats.is_empty() {
        let keys: Vec<_> = st.meshes.keys().filter(|k| gone_meshes.contains(&k.0) || gone_mats.contains(&k.1)).copied().collect();
        for k in keys {
            if let Some(h) = st.meshes.remove(&k) {
                forget_mesh(st, h);
                st.remix.destroy_mesh(h);
                st.released[0] += 1;
            }
            st.mesh_shape.remove(&k);
            st.falloff_eye.remove(&k);
            st.hidden.remove(&k);
        }
        for m in &gone_meshes {
            if let Some(h) = st.terrain.remove(m) {
                forget_mesh(st, h);
                st.remix.destroy_mesh(h);
                st.released[0] += 1;
            }
            st.layered.remove(m);
            st.mesh_gen.remove(m);
            st.dynamic.remove(m);
        }
        for m in &gone_mats {
            if let Some(h) = st.materials.remove(m) {
                st.remix.destroy_material(h);
                st.released[1] += 1;
            }
            st.mat_gen.remove(m);
            st.material_wait.remove(m);
        }
    }
    for id in gone_images {
        let h = hash_of(&id);
        st.queued.remove(&h);
        st.upload_queue.retain(|(i, _)| *i != id);
        st.mat_missing.remove(&h);
        st.failed_textures.remove(&h);
        st.wrap.remove(&h);
        st.d3d9.remove(&h);
        if !st.textures.remove(&h) {
            continue;
        }
        // A duplicate never uploaded its own: the texture it shared may go now.
        if let Some(first) = st.alias.remove(&h) {
            if st.orphans.contains(&first) && !st.alias.values().any(|&f| f == first) {
                st.orphans.remove(&first);
                destroy_texture(st, first);
            }
            continue;
        }
        if st.alias.values().any(|&f| f == h) {
            st.orphans.insert(h);
            continue;
        }
        destroy_texture(st, h);
    }
}

fn destroy_texture(st: &mut RemixState, h: u64) {
    st.remix.destroy_texture(h as ffi::TextureHandle);
    st.contents.retain(|_, v| *v != h);
    st.released[2] += 1;
}

/// Retained instances of `mesh` go before it does [documented: remix_c.h, "Destroy a
/// retained instance before its mesh"].
fn forget_mesh(st: &mut RemixState, mesh: ffi::MeshHandle) {
    let gone: Vec<Entity> = st.retained.iter().filter(|(_, r)| r.mesh == mesh).map(|(e, _)| *e).collect();
    if gone.is_empty() {
        return;
    }
    let handles: Vec<u64> = gone.iter().filter_map(|e| st.retained.remove(e)).map(|r| r.handle).collect();
    st.remix.destroy_retained(&handles);
}

/// After the frame's shapes: hide what was not drawn this frame, drop what has not been
/// drawn for a while, and replay the table.
fn retained_end_frame(st: &mut RemixState) {
    const DROP_AFTER: u64 = 600;
    let frame = st.frames;
    let mut updates = Vec::new();
    let mut dropped = Vec::new();
    for (e, r) in st.retained.iter_mut() {
        if r.seen == frame {
            continue;
        }
        if frame.saturating_sub(r.seen) > DROP_AFTER {
            dropped.push(*e);
        } else if r.visible {
            r.visible = false;
            updates.push(ffi::RetainedInstanceUpdate { handle: r.handle, transform: r.transform, flags: ffi::RETAINED_UPDATE_VISIBILITY, visible: 0 });
        }
    }
    if !updates.is_empty() && st.remix.update_retained(&updates).is_err() {
        st.errors += 1;
    }
    if !dropped.is_empty() {
        let handles: Vec<u64> = dropped.iter().filter_map(|e| st.retained.remove(e)).map(|r| r.handle).collect();
        st.remix.destroy_retained(&handles);
    }
    if !st.retained.is_empty() && st.remix.draw_retained().is_err() {
        st.errors += 1;
    }
}

/// Draw a static shape through its retained instance: made the first time, described
/// again when its mesh, categories or alpha change, moved when its transform changes,
/// shown again when it was hidden. Moves and shows are batched into `moves`.
#[allow(clippy::too_many_arguments)]
fn draw_retained(st: &mut RemixState, entity: Entity, mesh: ffi::MeshHandle, tr: &ffi::Transform, categories: u32, two_sided: bool, alpha: Option<nvf_remix::InstanceAlpha>, moves: &mut Vec<ffi::RetainedInstanceUpdate>) {
    let frame = st.frames;
    if let Some(r) = st.retained.get_mut(&entity) {
        r.seen = frame;
        if r.mesh != mesh || r.categories != categories || r.two_sided != two_sided || r.alpha != alpha {
            if st.remix.redescribe_retained(r.handle, mesh, tr, categories, two_sided, alpha).is_err() {
                st.errors += 1;
            }
            r.mesh = mesh;
            r.categories = categories;
            r.two_sided = two_sided;
            r.alpha = alpha;
            r.transform = *tr;
            st.ret_made += 1;
            if !r.visible {
                r.visible = true;
                moves.push(ffi::RetainedInstanceUpdate { handle: r.handle, transform: *tr, flags: ffi::RETAINED_UPDATE_VISIBILITY, visible: 1 });
            }
            return;
        }
        let mut flags = 0;
        if r.transform != *tr {
            r.transform = *tr;
            flags |= ffi::RETAINED_UPDATE_TRANSFORM;
            st.ret_moved += 1;
        }
        if !r.visible {
            r.visible = true;
            flags |= ffi::RETAINED_UPDATE_VISIBILITY;
        }
        if flags != 0 {
            moves.push(ffi::RetainedInstanceUpdate { handle: r.handle, transform: *tr, flags, visible: 1 });
        }
        return;
    }
    match st.remix.create_retained(mesh, tr, categories, two_sided, alpha) {
        Ok(handle) => {
            st.ret_made += 1;
            st.retained.insert(entity, Retained { handle, mesh, categories, two_sided, alpha, transform: *tr, visible: true, seen: frame });
        }
        Err(_) => {
            st.errors += 1;
            // Drawn this frame anyway.
            if st.remix.draw_instance_skinned(mesh, tr, categories, two_sided, &[], alpha).is_err() {
                st.errors += 1;
            }
        }
    }
}

fn hash_of<T: Hash>(t: &T) -> u64 {
    use std::hash::BuildHasher;
    // Never 0 (no key in the API).
    bevy::platform::hash::FixedHasher.hash_one(t) | 1
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
        .map(|i| CpuVertex {
            position: pos[i],
            normal: normals.and_then(|n| n.get(i)).copied().unwrap_or([0.0, 1.0, 0.0]),
            texcoord: uvs.and_then(|u| u.get(i)).copied().unwrap_or([0.0, 0.0]),
            // D3DCOLOR order: A R G B from the top byte [documented: remix_c.h's
            // HardcodedVertex.color, a D3D9 vertex colour].
            color: colors.and_then(|c| c.get(i)).map_or(0xFFFF_FFFF, |c| to8(c[3]) << 24 | to8(c[0]) << 16 | to8(c[1]) << 8 | to8(c[2])),
        })
        .collect::<Vec<_>>();
    let mut idx: Vec<u32> = match m.try_indices_option().ok()? {
        Some(Indices::U16(i)) => i.iter().map(|&x| u32::from(x)).collect(),
        Some(Indices::U32(i)) => i.clone(),
        None => (0..verts.len() as u32).collect(),
    };
    let asset_key = geometry_key(pos, &idx);
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
    Some(CpuMesh { verts, idx, skin, asset_key })
}

/// NIF materials NVForever changed: their Remix materials are made again (window glows
/// follow the hour) [observed 2026-11-09: lit windows at 10:00 under Remix].
fn take_changed_materials(mut ev: MessageReader<AssetEvent<NifMaterial>>, mut cpu: ResMut<CpuMeshes>) {
    for e in ev.read() {
        match e {
            AssetEvent::Modified { id } => cpu.changed_materials.push(id.untyped()),
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => cpu.gone_materials.push(id.untyped()),
            _ => {}
        }
    }
}

fn take_new_meshes(mut ev: MessageReader<AssetEvent<Mesh>>, mut meshes: ResMut<Assets<Mesh>>, mut cpu: ResMut<CpuMeshes>) {
    static KEEP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let keep_bevy = *KEEP.get_or_init(|| std::env::var_os("NVF_REMIX_KEEP_BEVY_MESHES").is_some());
    let mut emptied = Vec::new();
    for e in ev.read() {
        match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                if cpu.own_mesh_change.remove(id) {
                    continue;
                }
                let Some(m) = meshes.get(*id) else { continue };
                // Bevy's own copy is drawn only by its pass under the Remix window: a mesh
                // only the render world would hold (no main-world reader can have it), not
                // skinned, keeps its bounds (`final_aabb`, which Bevy's culling, and so
                // the shapes Remix gets, use) and loses its vertices, so it is never
                // uploaded [observed 2026-11-10: wgpu's mesh slabs 1.35 GB under Remix,
                // the same geometry Remix and this adapter hold]. NVF_REMIX_KEEP_BEVY_MESHES
                // keeps them.
                let empty = !keep_bevy && m.asset_usage == bevy::asset::RenderAssetUsages::RENDER_WORLD && m.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_none();
                if let Some(c) = convert_mesh(m) {
                    if empty {
                        emptied.push((*id, m.primitive_topology(), m.final_aabb.or_else(|| bounds_of(&c))));
                    }
                    if cpu.by_id.insert(*id, Arc::new(c)).is_some() {
                        cpu.changed.push(*id);
                    }
                }
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                cpu.by_id.remove(id);
                cpu.own_mesh_change.remove(id);
                cpu.gone_meshes.push(*id);
            }
            _ => {}
        }
    }
    for (id, topology, aabb) in emptied {
        let Some(aabb) = aabb else { continue };
        if let Some(mut m) = meshes.get_mut(id) {
            let mut shell = Mesh::new(topology, bevy::asset::RenderAssetUsages::MAIN_WORLD);
            shell.final_aabb = Some(aabb);
            *m = shell;
            cpu.own_mesh_change.insert(id);
        }
    }
}

/// The box around a mesh's vertices, as Bevy keeps it for culling.
fn bounds_of(c: &CpuMesh) -> Option<bevy::shape::Aabb3d> {
    let mut it = c.verts.iter().map(|v| Vec3::from(v.position));
    let first = it.next()?;
    let (lo, hi) = it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p)));
    Some(bevy::shape::Aabb3d { min: lo.into(), max: hi.into() })
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

fn take_new_images(mut ev: MessageReader<AssetEvent<Image>>, mut images: ResMut<Assets<Image>>, names: Option<Res<nvf_assets::texture::TextureCache>>, mut cpu: ResMut<CpuMeshes>, ui: Option<Res<UiOverlay>>) {
    static KEEP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let keep_bevy = *KEEP.get_or_init(|| std::env::var_os("NVF_REMIX_KEEP_BEVY_TEXTURES").is_some());
    for e in ev.read() {
        if let AssetEvent::Removed { id } | AssetEvent::Unused { id } = e {
            cpu.shrunk.remove(id);
            cpu.own_change.remove(id);
            cpu.gone_images.push(*id);
        }
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = e else { continue };
        // The change is this system's own (the image cut down below); any later one is
        // new pixels.
        if cpu.own_change.remove(id) {
            continue;
        }
        let modified = matches!(e, AssetEvent::Modified { .. });
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
        let name = names.as_ref().and_then(|n| n.name_of(*id)).map(str::to_ascii_lowercase);
        // A world texture (one the texture cache loaded, not the interface's) is drawn by
        // Remix alone: its pixels go to Remix and Bevy keeps one 4x4 level, so the picture
        // is not held twice, in Bevy's GPU textures under the Remix window and in Remix's
        // [observed 2026-11-09: the driver's 14.4 GB of 16 in use against Remix's own
        // 6.5 GB, and 19 GB of private memory; NVF_REMIX_KEEP_BEVY_TEXTURES keeps Bevy's
        // for comparing with a Bevy screenshot].
        let shrink = !keep_bevy && name.as_deref().is_some_and(|n| !n.contains("interface\\") && !n.contains("interface/")) && img.data.is_some() && d.size.width >= 4 && d.size.height >= 4;
        let (format, width, height, mips) = (d.format, d.size.width, d.size.height, d.mip_level_count);
        let taken = if shrink {
            let small = match format {
                TextureFormat::Bc1RgbaUnorm | TextureFormat::Bc1RgbaUnormSrgb | TextureFormat::Bc4RUnorm => 8,
                TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb | TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => 64,
                _ => 16,
            };
            images.get_mut(*id).and_then(|mut img| {
                let full = img.data.take()?;
                img.data = Some(full[..small.min(full.len())].to_vec());
                img.texture_descriptor.size.width = 4;
                img.texture_descriptor.size.height = 4;
                img.texture_descriptor.mip_level_count = 1;
                Some(full)
            })
        } else {
            None
        };
        if taken.is_some() {
            cpu.shrunk.insert(*id);
            cpu.own_change.insert(*id);
        }
        let Some(img) = images.get(*id) else { continue };
        let d = &img.texture_descriptor;
        let Some(data) = taken.or_else(|| img.data.clone()) else {
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
        let normal = !format.is_srgb() && name.as_deref().is_none_or(|n| n.ends_with("_n.dds") || n.ends_with("_n.png"));
        let d3d9 = d3d9_hash(format, width, height, &data);
        let wrap = remix_wrap(&img.sampler);
        cpu.pending_images.push((*id, Arc::new(PendingImage { width, height, mips, format, data, normal, d3d9, wrap, modified })));
    }
}

type CameraParts = (&'static Camera, &'static GlobalTransform, &'static Projection, Option<&'static RenderLayers>, Has<nvf_play::PlayerCamera>);
type ShapeParts<M> = (
    Entity,
    &'static Mesh3d,
    &'static MeshMaterial3d<M>,
    &'static GlobalTransform,
    &'static ViewVisibility,
    &'static InheritedVisibility,
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
    /// Shown by the game (not hidden, in a drawn cell), in the camera's view or not.
    shown: bool,
    skinned: Option<&'a bevy::mesh::skinning::SkinnedMesh>,
    layers: Option<&'a RenderLayers>,
    /// A mesh of a cell's land (its base or a blended texture layer), and its cell.
    land: bool,
    parent: Option<Entity>,
    /// The shape lies on another surface (a NIF decal: roads on the land, posters).
    decal: bool,
    /// An additive shape that adds its colour whole (NiAlphaProperty ONE / ONE), not by
    /// its alpha (`NifSurface::add_whole`).
    add_whole: bool,
    /// What else the NIF material says that StandardMaterial does not hold.
    nif: NifExtra,
}

/// The NIF material's fields the Remix scene needs beyond StandardMaterial
/// (nvf_assets::nif_material::NifSurface).
#[derive(Clone, Copy, Default)]
struct NifExtra {
    /// Left out of the traced scene, as New Vegas RTX leaves it out (`NifSurface::light_card`).
    light_card: bool,
    /// Its base texture is UV-animated (`NifSurface::uv_animated`).
    uv_animated: bool,
    /// A refracting shell: it only bends the picture behind it (NifSurface::refract).
    refract: bool,
    /// The alpha test of a blended shape (NifSurface::alpha_cut, 0..1; 0 none).
    alpha_cut: f32,
    /// The view-angle opacity falloff (NifSurface::falloff: start and stop cosines, start
    /// and stop opacities), when the shape has one.
    falloff: Option<Vec4>,
}

impl<'w, 's> Shapes<'w, 's> {
    fn all(&self) -> Vec<ShapeIn<'_>> {
        fn take<'a, E: bevy::pbr::MaterialExtension>(
            q: &'a Query<ShapeParts<bevy::pbr::ExtendedMaterial<StandardMaterial, E>>>,
            m: &'a Assets<bevy::pbr::ExtendedMaterial<StandardMaterial, E>>,
            out: &mut Vec<ShapeIn<'a>>,
            flags: fn(&E) -> (bool, bool, NifExtra),
        ) {
            for (entity, mesh, mat, gt, vis, shown, skinned, layers, land, parent) in q {
                let Some(material) = m.get(mat.id()) else { continue };
                out.push(ShapeIn { entity, mesh: mesh.id(), mat: mat.id().untyped(), base: &material.base, gt, vis: vis.get(), shown: shown.get(), skinned, layers, land, parent: parent.map(|p| p.parent()), decal: flags(&material.extension).0, add_whole: flags(&material.extension).1, nif: flags(&material.extension).2 });
            }
        }
        let mut out = Vec::new();
        take(&self.nif, &self.nif_m, &mut out, |e| {
            let extra = NifExtra { light_card: e.light_card, uv_animated: e.uv_animated, refract: e.refract, alpha_cut: e.alpha_cut, falloff: (e.falloff != nvf_assets::nif_material::Falloff::NONE.params).then_some(e.falloff) };
            (e.decal, e.add_whole > 0.5, extra)
        });
        take(&self.leaf, &self.leaf_m, &mut out, |_| (false, false, NifExtra::default()));
        take(&self.bark, &self.bark_m, &mut out, |_| (false, false, NifExtra::default()));
        take(&self.bill, &self.bill_m, &mut out, |_| (false, false, NifExtra::default()));
        take(&self.grass, &self.grass_m, &mut out, |_| (false, false, NifExtra::default()));
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
    // The runtime's menu (Alt+X) gets the keyboard and mouse with the game
    // (Child::share_raw_input), once its window is up.
    if !st.input_shared && st.child.share_raw_input() {
        st.input_shared = true;
        info!("remix: raw input shared with the runtime's menu window (Alt+X)");
    }

    // Meshes changed since Remix got them: their Remix meshes are made again, under a
    // new generation's hash (one hash made again after its destroy did not show, as
    // with lights) [observed 2026-11-09: particles kept their first shape].
    let changed: HashSet<AssetId<Mesh>> = std::mem::take(&mut cpu.changed).into_iter().collect();
    st.dynamic.extend(changed.iter().copied());
    if !changed.is_empty() {
        // The same triangles with new vertices (a particle system's quads, a morph) are
        // refreshed in place; anything else is made again [documented: New Vegas RTX's
        // api_mesh.hpp refresh_mesh]. Made again every frame, each change left its old
        // acceleration structure for the runtime to free later [observed 2026-11-09: 18
        // meshes a frame in Goodsprings, the driver's memory 2 GB up in 300 frames].
        let stale: Vec<_> = st.meshes.keys().filter(|k| changed.contains(&k.0)).copied().collect();
        let mut remake: HashSet<AssetId<Mesh>> = HashSet::new();
        for k in stale {
            let refreshed = match (cpu.by_id.get(&k.0), st.meshes.get(&k), st.materials.get(&k.1), st.mesh_shape.get(&k)) {
                (Some(d), Some(h), Some(m), Some(&(nv, ni, hash))) if d.skin.is_none() && d.verts.len() == nv && d.idx.len() == ni && !remake.contains(&k.0) => st.remix.refresh_mesh(hash, *h, &wide(&d.verts), &d.idx, *m),
                _ => false,
            };
            if refreshed {
                st.refreshed += 1;
            } else {
                remake.insert(k.0);
                st.mesh_shape.remove(&k);
                if let Some(h) = st.meshes.remove(&k) {
                    forget_mesh(st, h);
                    st.remix.destroy_mesh(h);
                }
            }
        }
        for id in &remake {
            *st.mesh_gen.entry(*id).or_default() += 1;
        }
        st.remade += remake.len() as u64;
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
            st.mesh_shape.remove(&k);
            if let Some(h) = st.meshes.remove(&k) {
                forget_mesh(st, h);
                st.remix.destroy_mesh(h);
            }
        }
        st.remade += changed_mats.len() as u64;
    }
    release_gone(st, &mut cpu);
    for id in std::mem::take(&mut cpu.skipped_images) {
        st.failed_textures.insert(hash_of(&id));
    }
    // New textures, within the frame's upload budget (the first always goes).
    for (id, img) in std::mem::take(&mut cpu.pending_images) {
        st.queued.insert(hash_of(&id));
        st.upload_queue.push_back((id, img));
    }
    // What NVForever holds for Remix, once a second for the first two minutes
    // (NVF_REMIX_MEMLOG): CPU mesh copies and textures waiting to go.
    if std::env::var_os("NVF_REMIX_MEMLOG").is_some() && st.frames % 5 == 0 && st.frames < 7200 {
        let mesh_bytes: usize = cpu.by_id.values().map(|m| m.verts.len() * std::mem::size_of::<CpuVertex>() + m.idx.len() * 4 + m.skin.as_ref().map_or(0, |(w, j)| w.len() * 4 + j.len() * 4)).sum();
        let queued: usize = st.upload_queue.iter().map(|(_, i)| i.data.len()).sum();
        info!("remix mem: frame {}: {} CPU meshes {:.0} MB, {} textures queued {:.0} MB, {} uploaded, {} Remix meshes", st.frames, cpu.by_id.len(), mesh_bytes as f64 / 1e6, st.upload_queue.len(), queued as f64 / 1e6, st.textures.len(), st.meshes.len());
    }
    let mut uploaded = 0usize;
    while uploaded < UPLOAD_BYTES_A_FRAME {
        let Some((id, img)) = st.upload_queue.pop_front() else { break };
        let hash = hash_of(&id);
        st.queued.remove(&hash);
        let mut replace = st.textures.contains(&hash);
        if replace && !img.modified {
            continue;
        }
        // New pixels of an image that shared another's texture: its own texture now.
        if replace && st.alias.remove(&hash).is_some() {
            replace = false;
        }
        uploaded += img.data.len();
        let Some((format, vk)) = remix_format(img.format) else {
            st.failed_textures.insert(hash);
            continue;
        };
        // Normal maps as stored, alpha (FNV's gloss) included: materials read them as RGB
        // tangent-space layers, as New Vegas RTX makes them [documented: the wrapper's
        // api_material.cpp, normalsRgbTangentSpace and the per-layer gloss].
        let _ = img.normal;
        let t = nvf_remix::TextureData { hash, width: img.width, height: img.height, mips: img.mips, format, data: &img.data, vk_format: vk, encoding: ffi::ENCODING_NONE, replace };
        st.wrap.insert(hash, img.wrap);
        if let Some(h) = img.d3d9 {
            st.d3d9.insert(hash, h);
        }
        if !replace && !img.modified {
            let content = twox_hash::XxHash3_64::oneshot_with_seed(u64::from(img.width) << 32 | u64::from(img.height) | u64::from(img.mips) << 56, &img.data) ^ format as u64;
            let first = *st.contents.entry(content).or_insert(hash);
            if first != hash && st.textures.contains(&first) && !st.alias.contains_key(&first) {
                st.same_bytes += img.data.len() as u64;
                st.same_count += 1;
                st.alias.insert(hash, first);
                st.textures.insert(hash);
                st.failed_textures.remove(&hash);
                if let Some(mats) = st.mat_missing.remove(&hash) {
                    cpu.changed_materials.extend(mats);
                }
                continue;
            }
        }
        match st.remix.create_texture(&t) {
            Ok(_) => {
                st.textures.insert(hash);
                st.failed_textures.remove(&hash);
                // Materials made without this texture (it came after they gave up on it)
                // are made again with it.
                if let Some(mats) = st.mat_missing.remove(&hash) {
                    cpu.changed_materials.extend(mats);
                }
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
            bevy::render::gpu_readback::recycle_readback_data(px);
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
    let mut why = [0u32; 6];
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
    let mut unseen_built = 0usize;
    let all = shapes.all();
    terrain_materials(st, &cpu, &all);
    let mut moves: Vec<ffi::RetainedInstanceUpdate> = Vec::new();
    for ShapeIn { entity, mesh, mat, base, gt, vis, shown, skinned, layers, land, decal, add_whole, nif, .. } in all {
        // As New Vegas RTX: light cards are not in the traced scene; refracting shells only
        // bend what is behind them, which a path tracer has no use for [documented: the
        // wrapper's scene_statics.cpp light_card_shape; design for refraction].
        // The game's fog, mist and haze cards: unlit, alpha-over, UV-animated, not a
        // particle system (NVForever's particles are not model shapes) [documented: the
        // wrapper's plugin fog.cpp].
        let fog_card = nif.uv_animated && base.unlit && matches!(base.alpha_mode, AlphaMode::Blend);
        if nif.light_card || nif.refract || fog_card {
            why[4] += 1;
            continue;
        }
        if nif.falloff.is_some() {
            why[5] += 1;
        }
        // In the traced scene: what the camera sees, and everything the game shows within
        // `member_radius` of the eye, seen or not: a path tracer needs what is behind and
        // beside the camera for its shadows, reflections and bounced light (New Vegas RTX
        // keeps every loaded static in the scene, drawn or not) [documented: its
        // docs/fnv-api-only.md, "Statics: scene membership, not draws"].
        let seen = vis;
        let vis = vis || (shown && eye.is_some_and(|e| gt.translation().distance_squared(e) < st.member_radius * st.member_radius));
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
                // What the camera does not see is built a little each frame.
                if !seen {
                    if unseen_built >= UNSEEN_VERTS_A_FRAME {
                        continue;
                    }
                    unseen_built += data.verts.len();
                }
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
                let verts: Vec<HardcodedVertex> = if t == bevy::math::Affine2::IDENTITY {
                    wide(&data.verts)
                } else {
                    data.verts.iter().map(|v| HardcodedVertex { texcoord: t.transform_point2(Vec2::from(v.texcoord)).to_array(), ..v.hw() }).collect()
                };
                let verts = verts.as_slice();
                let mesh_hash = hash_of(&(key, st.mesh_gen.get(&mesh).copied().unwrap_or(0), st.mat_gen.get(&mat).copied().unwrap_or(0)));
                // The key mods name the mesh by (static meshes, as the wrapper keys them).
                let asset_key = (skin.is_none() && data.asset_key != 0).then(|| {
                    let diffuse = base.base_color_texture.as_ref().and_then(|h| st.d3d9.get(&hash_of(&h.id()))).copied().unwrap_or(0);
                    let named = scoped_name(data.asset_key, diffuse);
                    if diffuse != 0 && st.mesh_names.contains(&named) { named } else { data.asset_key }
                });
                match st.remix.create_mesh_keyed(mesh_hash, verts, &data.idx, rm, skin, asset_key) {
                    Ok(h) => {
                        st.meshes.insert(key, h);
                        // Refreshed in place later only as sent: not with a baked UV transform.
                        if t == bevy::math::Affine2::IDENTITY {
                            st.mesh_shape.insert(key, (data.verts.len(), data.idx.len(), mesh_hash));
                        }
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
        if let (Some(p), Some(e)) = (nif.falloff, eye) {
            falloff_refresh(st, &cpu, key, handle, gt, p, add_whole, e);
        }
        // The categories New Vegas RTX's rtx.conf lists give the albedo [documented: the
        // wrapper's api_material.cpp, m.categories from rtxconf::lookup].
        let category = category | listing(st, &base.base_color_texture).0;
        // Smoke, dust and steam: unlit alpha-over cards that are not decals carry PARTICLE,
        // so they stay translucent at every preset [documented: the wrapper's api_unlit.cpp,
        // "cards", material::translucent_card].
        let category = if base.unlit && matches!(base.alpha_mode, AlphaMode::Blend) && !decal { category | (1 << 9) | (1 << 7) } else { category };
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
        let alpha = if st.terrain.contains_key(&mesh) { None } else { instance_state(base, add_whole, nif.alpha_cut) };
        // Static: no bones, a mesh that has not changed since Remix got it, no view-angle
        // fade (its vertices follow the eye).
        if st.use_retained && bones.is_empty() && skinned.is_none() && nif.falloff.is_none() && !st.dynamic.contains(&mesh) {
            draw_retained(st, entity, handle, &tr, category, two_sided, alpha, &mut moves);
        } else if st.remix.draw_instance_skinned(handle, &tr, category, two_sided, &bones, alpha).is_err() {
            st.errors += 1;
        }
    }
    if !moves.is_empty() && st.remix.update_retained(&moves).is_err() {
        st.errors += 1;
    }
    if st.use_retained {
        retained_end_frame(st);
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
                        let display = |c: Vec4| [c.x, c.y, c.z].map(|v| crate::lights::linear_to_display(v.max(0.0)));
                        let (colour, distance) = water_medium(display(w.params.shallow), display(w.params.deep));
                        info!("remix: water {:?} transmittance {colour:?} over {distance:.0} units", key.1);
                        // The water's noise (its normal map) gives the surface its waves; Remix
                        // moves it on animated water [documented: rtx_options.h
                        // animatedWaterTextures, instance category ANIMATED_WATER].
                        let waves = w.noise.as_ref().map(|h| hash_of(&h.id())).filter(|x| st.textures.contains(x)).map(|x| remix_tex(st, x));
                        match st.remix.create_translucent_material(hash_of(&key.1), colour, distance, 1.33, waves) {
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
                match st.remix.create_mesh(hash_of(&key), &wide(&data.verts), &data.idx, rm) {
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
        if sky.interior.is_some() && st.fill_scale > 0.0 {
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
    let gone: Vec<Entity> = st.lights.keys().filter(|e| !seen.contains(*e)).copied().collect();
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
        if let Some(px) = ui.pixels.lock().ok().and_then(|mut p| p.latest.take()) {
            // The picture it replaces goes back for a later readback.
            if let Some((old, _)) = st.ui_last.replace((px, ui.size)) {
                bevy::render::gpu_readback::recycle_readback_data(old);
            }
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
        info!("remix: frame {}, {} textures, {} materials, {} meshes, {} lights, {} errors; visible shapes {}, without a CPU mesh {}, without a material {}, waiting for textures {}, light cards and refracting shells left out {}, with a view-angle falloff {}", st.frames, st.textures.len(), st.materials.len(), st.meshes.len(), st.lights.len(), st.errors, st.why[0], st.why[1], st.why[2], st.why[3], st.why[4], st.why[5]);
        let n = 600.0;
        info!("remix: textures sharing the texture of the same contents: {} ({:.0} MB not uploaded); let go with their assets since the last line: {} meshes, {} materials, {} textures", st.same_count, st.same_bytes as f64 / 1e6, st.released[0], st.released[1], st.released[2]);
        st.released = [0; 3];
        info!("remix: {:.2} ms a frame building the scene, {:.2} ms in present, {} changed meshes made again, {} refreshed in place; {} retained instances, {} made or described again, {} moved", st.build_time.as_secs_f64() * 1000.0 / n, st.present_time.as_secs_f64() * 1000.0 / n, st.remade, st.refreshed, st.retained.len(), st.ret_made, st.ret_moved);
        st.ret_made = 0;
        st.ret_moved = 0;
        st.remade = 0;
        st.refreshed = 0;
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
    } else if [&b.base_color_texture, &b.normal_map_texture, &b.emissive_texture].into_iter().flatten().any(|h| st.queued.contains(&hash_of(&h.id()))) {
        // Still in the upload queue: it will come.
        return None;
    }
    // Textures it is made without (given up on, or still queued): when one arrives the
    // material is made again (`mat_missing`).
    for h in [&b.base_color_texture, &b.normal_map_texture, &b.emissive_texture].into_iter().flatten() {
        let x = hash_of(&h.id());
        if !st.textures.contains(&x) {
            st.mat_missing.entry(x).or_default().push(id);
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
        wrap: tex(&b.base_color_texture).and_then(|h| st.wrap.get(&h).copied()).unwrap_or([1, 1]),
        asset_key: tex(&b.base_color_texture).and_then(|h| st.d3d9.get(&h).copied()),
        albedo: tex(&b.base_color_texture).map(|h| remix_tex(st, h)),
        // LOD terrain's normal maps are in model space (lod.rs), not the tangent space
        // Remix reads: left out.
        normal: if b.depth_bias <= -50.0 { None } else { tex(&b.normal_map_texture).map(|h| remix_tex(st, h)) },
        // An unlit surface (a screen) glows with its own picture [design: Remix lights
        // every surface; emission is how a picture shows unlit].
        emissive: if b.unlit { tex(&b.base_color_texture) } else { tex(&b.emissive_texture) }.map(|h| remix_tex(st, h)),
        // A glow map at New Vegas RTX's glow intensity (rtx.fnvProtocol.glowEmissiveIntensity)
        // [documented: the wrapper's api_material.cpp, glow_intensity].
        // A lit shape glows only through a glow map, tinted by the material's emissive
        // colour (display values, at most 1 a channel; black is none); without the map it
        // does not glow at all, whatever its emissive colour [documented: the wrapper's
        // scene_statics.cpp, md.glow from texture slot 2 with glow_scale / glow_tint].
        // Sent as a uniform colour, every shape with an emissive colour shone over its
        // whole surface.
        emissive_intensity: if b.unlit { 1.0 } else if b.emissive_texture.is_some() && e.red + e.green + e.blue > 0.0 { glow } else { 0.0 },
        emissive_color: if b.unlit { [c.red, c.green, c.blue] } else { glow_tint([e.red, e.green, e.blue]) },
        albedo_constant: [c.red, c.green, c.blue],
        opacity: c.alpha,
        roughness: b.perceptual_roughness,
        metallic: b.metallic,
        // Alpha-tested and alpha-blended: by the texture's alpha times the vertex
        // colour's, as Bevy draws them (the sand drifting over roads fades out by its
        // vertex alpha) [documented: remix_c.h useDrawCallAlphaState]; the state comes
        // with each instance (`instance_alpha`).
        draw_call_alpha: !listed.1 && (instance_alpha(b.alpha_mode).is_some() || matches!(b.alpha_mode, AlphaMode::Add | AlphaMode::Multiply)),
        alpha_test,
        blend,
        tint: None,
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

/// A glow map's tint from the material's linear emissive colour: display values held to 1
/// a channel (a brighter colour keeps its hue).
fn glow_tint(e: [f32; 3]) -> [f32; 3] {
    let d = e.map(|v| crate::lights::linear_to_display(v.max(0.0)));
    let m = d[0].max(d[1]).max(d[2]).max(1.0);
    d.map(|v| v / m)
}

/// The water's medium as New Vegas RTX makes it from the water shader's shallow and deep
/// colours (display values): mostly the deep colour's hue, normalised to its brightest
/// channel and scaled by the clarity, over a distance that grows with the colours'
/// brightness [documented: the wrapper's api_water.cpp medium_of; remix-comp.ini
/// WaterMeasurementDistance 256, WaterClarity 0.8].
fn water_medium(shallow: [f32; 3], deep: [f32; 3]) -> ([f32; 3], f32) {
    let mut base = [0, 1, 2].map(|k| 0.35 * shallow[k] + 0.65 * deep[k]);
    let mean = [0, 1, 2].map(|k| 0.5 * (shallow[k] + deep[k]));
    let mut m = base[0].max(base[1]).max(base[2]);
    let mut brightness = mean[0].max(mean[1]).max(mean[2]);
    if m < 1e-3 {
        base = [0.25, 0.40, 0.38];
        m = 0.40;
        brightness = 0.35;
    }
    (base.map(|b| (b / m * 0.8).clamp(0.02, 0.98)), 256.0 * (brightness / 0.35).clamp(0.25, 2.0))
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
    // The sky builds its sun from these in the scene's space, Y up: (cos e sin r, sin e,
    // cos e cos r) [documented: the runtime's numos renderer.cpp, sunDirection]. Bevy's
    // +Z is the game's south (world.rs game_to_bevy: game (x, y, z) is Bevy (x, z, -y)),
    // so r is measured from south: with the original's atan2(x, y) the sun stood mirrored
    // north to south and every shadow fell the wrong way [observed 2026-11-09 against the
    // original's capture at Goodsprings, 14:00].
    let rotation = x.atan2(-y).to_degrees().rem_euclid(360.0);
    let moon_rotation = (-x).atan2(y).to_degrees().rem_euclid(360.0);
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
            text.push_str("\n# NVForever\nrtx.automation.disableBlockingDialogBoxes = True\nrtx.hideSplashMessage = True\n");
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
            // Remix mods: the game folder's rtx-remix\mods first (under MO2 the merged view,
            // where every installed mod's Root shows), else the runtime's install. Found
            // here, not from the DLL's folder alone, which is one MO2 mod.
            let mods = config.game_dir.as_ref().map(|g| g.join("rtx-remix").join("mods")).filter(|m| m.is_dir()).unwrap_or_else(|| r.join("rtx-remix").join("mods"));
            // Off unless asked for (NVForever.ini `[Render] RemixMods=1`, NVF_REMIX_MODS=1):
            // with the HND install's three packs the card ran out of memory in 15 s, Bevy's
            // own meshes still held under the Remix window [observed 2026-11-09: wgpu "Out of
            // Memory" growing a mesh slab; the original with the same packs at 9.8 GB].
            let want_mods = std::env::var("NVF_REMIX_MODS").map(|v| v == "1").unwrap_or_else(|_| {
                fnv_install::EngineHome::user().and_then(|h| std::fs::read_to_string(h.config_file()).ok()).and_then(|t| fnv_install::ini_get(&t, "Render", "RemixMods").map(|v| v.trim() == "1")).unwrap_or(false)
            });
            if !want_mods {
                // An empty folder: the runtime's own default would find the install's packs.
                let none = work.join("no-mods");
                let _ = std::fs::create_dir_all(&none);
                std::env::set_var("DEFAULT_MODS_DIR", &none);
                info!("remix: Remix mods off ([Render] RemixMods=1 turns them on: {} there)", mods.display());
            } else if mods.is_dir() {
                std::env::set_var("DEFAULT_MODS_DIR", &mods);
                info!("remix: Remix mods from {}", mods.display());
            }
        }
        std::env::set_var("DXVK_LOG_PATH", work.join("logs"));
        std::env::set_var("DXVK_CAPTURE_PATH", work.join("captures"));
        std::env::set_var("DXVK_CONFIG_FILE", work.join("dxvk.conf"));
        let _ = std::env::set_current_dir(work);
    }
    let folder = home.clone().or_else(|| root.clone());
    let texts: Vec<String> = ["rtx.conf", "user.conf"].iter().filter_map(|f| folder.as_ref().and_then(|d| std::fs::read_to_string(d.join(f)).ok())).collect();
    let lists = RtxLists::read(&texts);
    let mesh_names = std::env::var_os("DEFAULT_MODS_DIR").map(|m| shared_mesh_names(std::path::Path::new(&m))).unwrap_or_default();
    info!("remix: rtx.conf lists name {} textures; glow intensity {}", lists.by_hash.len(), lists.glow_intensity);
    let child = nvf_remix::window::Child::new(parent).ok_or("could not make the Remix child window")?;
    let mut remix = nvf_remix::Remix::load(&config.dll)?;
    remix.startup(child.hwnd.cast()).map_err(|e| e.to_string())?;
    // Bevy's space: Y up, right-handed [design: NVForever's scene].
    // The sky's own day cycle off: NVForever's clock places its sun (frame).
    // The emissive colour tints a glow map only with rtx.fnvProtocol.apiGlowTint, which the
    // wrapper turns on [documented: the runtime's rtx_remix_api.cpp; the wrapper's
    // api_material.cpp push of apiGlowTint].
    for (k, v) in [("rtx.zUp", "False"), ("rtx.leftHandedCoordinateSystem", "False"), ("rtx.atmosphere.timeCycleEnable", "False"), ("rtx.fnvProtocol.apiGlowTint", "True"), ("rtx.scene.assetKeys", "True")] {
        if let Err(e) = remix.set_config(k, v) {
            warn!("remix: {k}: {e}");
        }
    }
    // Test aid: `NVF_REMIX_SET=key=value;key=value` sets runtime options for one run (the
    // profiler: rtx.profile.detailedLogging=True), leaving every config file alone.
    if let Ok(list) = std::env::var("NVF_REMIX_SET") {
        for (k, v) in list.split(';').filter_map(|kv| kv.split_once('=')) {
            match remix.set_config(k.trim(), v.trim()) {
                Ok(()) => info!("remix: {} = {} (NVF_REMIX_SET)", k.trim(), v.trim()),
                Err(e) => warn!("remix: {k}: {e}"),
            }
        }
    }
    // Static shapes go as retained instances when the runtime has them, owned by its scene
    // directly (built once, moved and shown in place) [documented: the runtime's
    // rtx_scene_interface.h, rtx.scene.directRetained, off by default for API hosts].
    let use_retained = remix.has_retained() && std::env::var("NVF_REMIX_RETAINED").map_or(true, |v| v != "0");
    if use_retained {
        if let Err(e) = remix.set_config("rtx.scene.directRetained", "True") {
            warn!("remix: rtx.scene.directRetained: {e}");
        }
    }
    info!("remix: retained instances {}", if use_retained { "on" } else if remix.has_retained() { "off (NVF_REMIX_RETAINED=0)" } else { "off (the runtime has none)" });
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
        why: [0; 6],
        shot_taken: false,
        mesh_names,
        mat_missing: HashMap::new(),
        wrap: HashMap::new(),
        falloff_eye: HashMap::new(),
        input_shared: false,
        light_a2: scale("NVF_REMIX_LIGHT_A2", 0.001),
        d3d9: HashMap::new(),
        lists,
        one_layer: None,
        pushed: HashMap::new(),
        mesh_gen: HashMap::new(),
        mat_gen: HashMap::new(),
        remade: 0,
        refreshed: 0,
        mesh_shape: HashMap::new(),
        // Off unless asked for: a light following the eye is no part of the original's
        // lighting (NVF_REMIX_FILL_SCALE, e.g. 20000, turns it on as an artistic fill).
        fill_scale: scale("NVF_REMIX_FILL_SCALE", 0.0),
        member_radius: scale("NVF_REMIX_MEMBER_RADIUS", 8192.0),
        upload_queue: Default::default(),
        queued: HashSet::new(),
        ui_last: None,
        hidden: HashSet::new(),
        build_time: Default::default(),
        present_time: Default::default(),
        retained: HashMap::new(),
        use_retained,
        dynamic: HashSet::new(),
        ret_made: 0,
        ret_moved: 0,
        contents: HashMap::new(),
        same_bytes: 0,
        same_count: 0,
        alias: HashMap::new(),
        orphans: HashSet::new(),
        released: [0; 3],
    })
}

/// The most texture bytes a frame hands the runtime (about 3 GB a second at 60 frames).
const UPLOAD_BYTES_A_FRAME: usize = 48 << 20;
/// The most vertices of shapes the camera does not see a frame makes Remix meshes of.
const UNSEEN_VERTS_A_FRAME: usize = 150_000;

/// How far LOD terrain is sunk under the loaded land (game units, Bevy's up is +y).
const LOD_SINK: f32 = 200.0;

/// The alpha state an instance of a material with this alpha mode carries: alpha-tested
/// and alpha-blended ones only (glows and multiplied shapes keep their blend type).
fn instance_alpha(mode: AlphaMode) -> Option<nvf_remix::InstanceAlpha> {
    match mode {
        AlphaMode::Mask(t) => Some(nvf_remix::InstanceAlpha { test: Some((t * 255.0) as u8), blend: false, baked: true, factors: None }),
        AlphaMode::Blend | AlphaMode::Premultiplied => Some(nvf_remix::InstanceAlpha { test: None, blend: true, baked: true, factors: None }),
        _ => None,
    }
}

/// A shape's view-angle falloff in its vertices, as FNV's NoLighting vertex shader fades it
/// and the D3D9 route New Vegas RTX keeps such shapes on hands Remix the result: each
/// vertex's opacity from |N.V| (nif_material.wesl falloff_opacity), times its own alpha,
/// and its colour too for a shape added whole (ONE / ONE ignores alpha). Refreshed in place
/// when the eye has moved 32 units since [design: the cost of a refresh].
#[allow(clippy::too_many_arguments)]
fn falloff_refresh(st: &mut RemixState, cpu: &CpuMeshes, key: (AssetId<Mesh>, UntypedAssetId), handle: ffi::MeshHandle, gt: &GlobalTransform, p: Vec4, add_whole: bool, eye: Vec3) {
    if st.falloff_eye.get(&key).is_some_and(|e| e.distance(eye) < 32.0) {
        return;
    }
    let (Some(data), Some(&(nv, ni, hash)), Some(&mat)) = (cpu.by_id.get(&key.0), st.mesh_shape.get(&key), st.materials.get(&key.1)) else { return };
    if data.skin.is_some() || data.verts.len() != nv || data.idx.len() != ni {
        return;
    }
    let m = gt.affine();
    let verts: Vec<HardcodedVertex> = data
        .verts
        .iter()
        .map(|v| {
            let pos = m.transform_point3(Vec3::from(v.position));
            let n = m.transform_vector3(Vec3::from(v.normal)).normalize_or_zero();
            let cos = n.dot((eye - pos).normalize_or_zero()).abs();
            let t = ((cos - p.y) / (p.x - p.y)).clamp(0.0, 1.0);
            let op = p.w + (p.z - p.w) * t;
            let scale = |c: u32, shift: u32| ((((c >> shift) & 0xFF) as f32 * op).round().clamp(0.0, 255.0) as u32) << shift;
            let c = v.color;
            let color = if add_whole { scale(c, 24) | scale(c, 16) | scale(c, 8) | scale(c, 0) } else { scale(c, 24) | (c & 0x00FF_FFFF) };
            HardcodedVertex { color, ..v.hw() }
        })
        .collect();
    if st.remix.refresh_mesh(hash, handle, &verts, &data.idx, mat) {
        st.falloff_eye.insert(key, eye);
    }
}

/// The texture stage state every instance carries: colour the texture's times the vertex
/// colour's (as baked lighting on lit shapes: the runtime keeps its hue), with the alpha
/// state of `instance_alpha` where the material has one [documented: New Vegas RTX's
/// api_material.cpp vertex_color_blend, on every static]. Glows and multiplied shapes keep
/// their material's blend type and get none.
fn instance_state(b: &StandardMaterial, add_whole: bool, alpha_cut: f32) -> Option<nvf_remix::InstanceAlpha> {
    use nvf_remix::ffi::{VK_BLEND_DST_COLOR, VK_BLEND_ONE, VK_BLEND_SRC_ALPHA, VK_BLEND_ZERO};
    match b.alpha_mode {
        // Glows and multiplied shapes: their own blend factors, colour and alpha the
        // texture's times the vertex colour's, the vertex colour a real colour (a tint, a
        // fade), as New Vegas RTX sends unlit effects [documented: its api_material.cpp,
        // "Unlit FX and particles: the vertex colour is a real colour"]. Without this state
        // the runtime read the texture alone and the vertex fades were lost.
        AlphaMode::Add => Some(nvf_remix::InstanceAlpha {
            test: None,
            blend: true,
            baked: false,
            factors: Some((if add_whole { VK_BLEND_ONE } else { VK_BLEND_SRC_ALPHA }, VK_BLEND_ONE)),
        }),
        AlphaMode::Multiply => Some(nvf_remix::InstanceAlpha { test: None, blend: true, baked: false, factors: Some((VK_BLEND_DST_COLOR, VK_BLEND_ZERO)) }),
        // A blended shape that also cuts (NifSurface::alpha_cut): alpha-tested and blended,
        // as the NIF's NiAlphaProperty sets both.
        AlphaMode::Blend if alpha_cut > 0.0 => Some(nvf_remix::InstanceAlpha { test: Some((alpha_cut.clamp(0.0, 1.0) * 255.0) as u8), blend: true, baked: !b.unlit, factors: None }),
        m => Some(nvf_remix::InstanceAlpha { baked: !b.unlit, ..instance_alpha(m).unwrap_or(nvf_remix::InstanceAlpha { test: None, blend: false, baked: true, factors: None }) }),
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
/// The UI overlay's latest read-back picture. A whole-window picture every frame (14.7 MB
/// at 2560x1440), copied and allocated anew, cost about 4 ms a frame in copies, page faults
/// and frees [observed 2026-11-10, Superluminal: sync_readbacks 2.3 s and
/// NtFreeVirtualMemory 1.1 s of 19 s]: the readback's own buffer is taken, and buffers
/// done with go back to bevy_render's readback pool (vendor/README.md).
#[derive(Default)]
struct PixelSink {
    latest: Option<Vec<u8>>,
}

#[derive(Resource)]
struct UiOverlay {
    image: AssetId<Image>,
    size: (u32, u32),
    pixels: Arc<std::sync::Mutex<PixelSink>>,
    readback: Entity,
}

/// The overlay's readback of `image` at `w`×`h` into `sink`.
fn spawn_ui_readback(commands: &mut Commands, image: Handle<Image>, w: u32, h: u32, sink: Arc<std::sync::Mutex<PixelSink>>) -> Entity {
    use bevy::render::gpu_readback::{Readback, ReadbackComplete};
    // Rows come back padded to 256 bytes [documented: wgpu COPY_BYTES_PER_ROW_ALIGNMENT,
    // bevy_render gpu_readback get_aligned_size].
    let row = (w * 4) as usize;
    let stride = row.div_ceil(256) * 256;
    commands
        .spawn(Readback::texture(image))
        .observe(move |mut ev: On<ReadbackComplete>| {
            let len = ev.data.len();
            // A frame read before a resize has the old size: dropped.
            if len < stride * (h as usize - 1) + row || len > stride * h as usize {
                return;
            }
            let mut data = std::mem::take(&mut ev.event_mut().data);
            // Rows padded to the copy alignment are packed in place.
            if stride != row {
                for y in 1..h as usize {
                    data.copy_within(y * stride..y * stride + row, y * row);
                }
            }
            data.truncate(row * h as usize);
            let Ok(mut p) = sink.lock() else { return };
            if let Some(old) = p.latest.replace(data) {
                bevy::render::gpu_readback::recycle_readback_data(old);
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
        *p = PixelSink::default();
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
    let pixels: Arc<std::sync::Mutex<PixelSink>> = Arc::default();
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
    let tex = |st: &RemixState, h: &Option<Handle<Image>>| h.as_ref().map(|h| hash_of(&h.id())).filter(|x| st.textures.contains(x)).map(|x| remix_tex(st, x));
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
            // In the layers' rank, as the terrain loader composites them (its layer
            // material's depth bias is sort_first(2 x (rank + 1)), terrain.rs), not in query
            // order.
            let mut layers = layers;
            layers.sort_by(|a, b| a.0.base.depth_bias.total_cmp(&b.0.base.depth_bias));
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
            // The land's vertex colours as the material's tint texture, on the weights'
            // grid (display values; terrain.rs stores them linear) [documented: the
            // wrapper's api_terrain.cpp, "the land tint rides in the tint texture"].
            let mut tint = vec![255u8; n * n * 4];
            let mut tinted = false;
            for v in 0..n * n {
                let c = base.verts[v].color;
                for (k, shift) in [16u32, 8, 0].into_iter().enumerate() {
                    let lin = ((c >> shift) & 0xFF) as f32 / 255.0;
                    let d = (crate::lights::linear_to_display(lin).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    tint[v * 4 + k] = d;
                    tinted |= d < 254;
                }
            }
            let tint_hash = hash_of(&(b.mesh, 2u8));
            if tinted {
                let t = nvf_remix::TextureData { hash: tint_hash, width: n as u32, height: n as u32, mips: 1, format: ffi::FORMAT_R8G8B8A8_UNORM, data: &tint, vk_format: 0, encoding: 0, replace: false };
                if st.remix.create_texture(&t).is_err() {
                    st.errors += 1;
                    tinted = false;
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
                // Land has no specular [documented: remix-comp.ini TerrainRoughness 0.85].
                roughness: 0.85,
                metallic: 0.0,
                tint: tinted.then_some(tint_hash),
                // Land textures tile.
                wrap: [1, 1],
                ..Default::default()
            };
            let white: Vec<HardcodedVertex> = base.verts.iter().map(|v| HardcodedVertex { color: 0xFFFF_FFFF, ..v.hw() }).collect();
            let Ok(mat) = st.remix.create_terrain_material(&desc, &list, hashes, [su, sv], [ou, ov]) else {
                st.errors += 1;
                continue;
            };
            match st.remix.create_mesh(hash_of(&(b.mesh, "terrain-mesh")), &white, &base.idx, mat) {
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
    /// The entity reading it back, and the image (`pace_live_readbacks`).
    readback: Entity,
    handle: Handle<Image>,
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
        let sink = live.0.clone();
        // Rows come back padded to 256 bytes (as for the UI overlay).
        let row = (w * 4) as usize;
        let stride = row.div_ceil(256) * 256;
        // The readback's own buffer is taken and its rows packed in place, as the UI
        // overlay's [observed 2026-11-10, Superluminal: copying a screen every frame, page
        // faults and frees included, about 2.3 ms a frame].
        let readback = commands
            .spawn(Readback::texture(handle.clone()))
            .observe(move |mut ev: On<ReadbackComplete>| {
                if ev.data.len() < stride * (h as usize - 1) + row {
                    return;
                }
                let mut data = std::mem::take(&mut ev.event_mut().data);
                if stride != row {
                    for y in 1..h as usize {
                        data.copy_within(y * stride..y * stride + row, y * row);
                    }
                }
                data.truncate(row * h as usize);
                if let Ok(mut l) = sink.lock() {
                    if let Some(t) = l.get_mut(&id) {
                        if let Some(old) = t.pixels.replace(data) {
                            bevy::render::gpu_readback::recycle_readback_data(old);
                        }
                    }
                }
            })
            .id();
        if let Ok(mut l) = live.0.lock() {
            l.insert(id, LiveTexture { width: w, height: h, format, pixels: None, readback, handle });
        }
    }
}

/// A screen is read back only while a camera that draws into it is on (the Pip-Boy's while
/// it shows, a terminal's while it is open): with none on, Remix keeps its last picture. A
/// screen no camera draws into is read back always.
fn pace_live_readbacks(mut commands: Commands, live: Res<LiveTextures>, cameras: Query<(&Camera, &bevy::camera::RenderTarget)>, mut reading: Local<HashMap<AssetId<Image>, bool>>) {
    use bevy::render::gpu_readback::Readback;
    let Ok(l) = live.0.lock() else { return };
    for (id, t) in l.iter() {
        let drawn_by: Vec<bool> = cameras
            .iter()
            .filter(|(_, target)| matches!(target, bevy::camera::RenderTarget::Image(i) if i.handle.id() == *id))
            .map(|(c, _)| c.is_active)
            .collect();
        let want = drawn_by.is_empty() || drawn_by.iter().any(|&on| on);
        let now = reading.entry(*id).or_insert(true);
        if *now != want {
            *now = want;
            if want {
                commands.entity(t.readback).insert(Readback::texture(t.handle.clone()));
            } else {
                commands.entity(t.readback).remove::<Readback>();
            }
        }
    }
}
