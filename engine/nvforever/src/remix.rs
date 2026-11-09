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
            .add_systems(PostUpdate, (take_new_meshes, take_new_images).after(AssetEventSystems))
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
}

/// The runtime and what NVForever made in it (main thread only).
struct RemixState {
    remix: nvf_remix::Remix,
    child: nvf_remix::window::Child,
    textures: HashSet<u64>,
    failed_textures: HashSet<u64>,
    material_wait: HashMap<UntypedAssetId, u64>,
    materials: HashMap<UntypedAssetId, ffi::MaterialHandle>,
    meshes: HashMap<(AssetId<Mesh>, UntypedAssetId), ffi::MeshHandle>,
    lights: HashMap<Entity, (u64, ffi::LightHandle)>,
    frames: u64,
    errors: u64,
    light_scale: f32,
    sun_scale: f32,
    started: std::time::Instant,
    why: [u32; 4],
    shot_raised: bool,
    shot_taken: bool,
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

fn take_new_meshes(mut ev: MessageReader<AssetEvent<Mesh>>, meshes: Res<Assets<Mesh>>, mut cpu: ResMut<CpuMeshes>) {
    for e in ev.read() {
        match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                if let Some(c) = meshes.get(*id).and_then(convert_mesh) {
                    cpu.by_id.insert(*id, Arc::new(c));
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

fn take_new_images(mut ev: MessageReader<AssetEvent<Image>>, images: Res<Assets<Image>>, server: Res<AssetServer>, mut cpu: ResMut<CpuMeshes>) {
    for e in ev.read() {
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = e else { continue };
        let Some(img) = images.get(*id) else { continue };
        let d = &img.texture_descriptor;
        if d.dimension != bevy::render::render_resource::TextureDimension::D2 || d.size.depth_or_array_layers != 1 || remix_format(d.format).is_none() {
            cpu.skipped_images.push(*id);
            continue;
        }
        let Some(data) = img.data.clone() else {
            cpu.skipped_images.push(*id);
            continue;
        };
        let normal = server.get_path(*id).is_some_and(|p| {
            let s = p.path().to_string_lossy().to_ascii_lowercase();
            s.ends_with("_n.dds") || s.ends_with("_n.png")
        });
        cpu.pending_images.push((*id, Arc::new(PendingImage { width: d.size.width, height: d.size.height, mips: d.mip_level_count, format: d.format, data, normal })));
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
}

impl<'w, 's> Shapes<'w, 's> {
    fn all(&self) -> Vec<ShapeIn<'_>> {
        fn take<'a, E: bevy::pbr::MaterialExtension>(
            q: &'a Query<ShapeParts<bevy::pbr::ExtendedMaterial<StandardMaterial, E>>>,
            m: &'a Assets<bevy::pbr::ExtendedMaterial<StandardMaterial, E>>,
            out: &mut Vec<ShapeIn<'a>>,
        ) {
            for (entity, mesh, mat, gt, vis, skinned, layers) in q {
                let Some(material) = m.get(mat.id()) else { continue };
                out.push(ShapeIn { entity, mesh: mesh.id(), mat: mat.id().untyped(), base: &material.base, gt, vis: vis.get(), skinned, layers });
            }
        }
        let mut out = Vec::new();
        take(&self.nif, &self.nif_m, &mut out);
        take(&self.leaf, &self.leaf_m, &mut out);
        take(&self.bark, &self.bark_m, &mut out);
        take(&self.bill, &self.bill_m, &mut out);
        take(&self.grass, &self.grass_m, &mut out);
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
    bindposes: Res<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
    meshes_all: Res<Assets<Mesh>>,
    points: Query<(Entity, &PointLight, &GlobalTransform, &InheritedVisibility)>,
    suns: Query<(Entity, &DirectionalLight, &GlobalTransform)>,
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
    st.child.follow();

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
        let t = nvf_remix::TextureData { hash, width: img.width, height: img.height, mips: img.mips, format, data: &img.data, vk_format: vk, encoding: if img.normal { ffi::ENCODING_NORMAL_TANGENT_XY } else { ffi::ENCODING_NONE } };
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
    for ShapeIn { entity, mesh, mat, base, gt, vis, skinned, layers } in shapes.all() {
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
            0
        };
        why[0] += 1;
        // Additive shapes (light shafts, glows) are no surfaces to the path tracer
        // [design: they would trace as lit sheets; their light comes from the lights].
        if matches!(base.alpha_mode, AlphaMode::Add) {
            continue;
        }
        let key = (mesh, mat);
        let handle = match st.meshes.get(&key) {
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
                let skin = data.skin.as_ref().map(|(w, j)| (4u32, w.as_slice(), j.as_slice()));
                match st.remix.create_mesh_skinned(hash_of(&key), &data.verts, &data.idx, rm, skin) {
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
        let tr = nvf_remix::transform_from_cols(gt.to_matrix().to_cols_array_2d());
        if st.remix.draw_instance_skinned(handle, &tr, category, two_sided, &bones).is_err() {
            st.errors += 1;
        }
    }

    // Lights.
    let mut seen = HashSet::new();
    for (e, l, gt, vis) in &points {
        if !vis.get() || l.intensity <= 0.0 {
            continue;
        }
        let p = gt.translation().to_array();
        let c = l.color.to_linear();
        let radius = 4.0f32;
        // Radiance of a sphere of `radius` giving the light's lumens [hypothesis: Bevy's
        // lumens over 4π sr and the disc's π r², times a scale for Remix's exposure].
        // In the game's own scale: Bevy's lumens over the camera exposure NVForever lights
        // with (`sky::light_unit`).
        let k = l.intensity / crate::sky::light_unit() / (4.0 * std::f32::consts::PI * std::f32::consts::PI * radius * radius) * st.light_scale;
        let rad = [c.red * k, c.green * k, c.blue * k];
        let key = hash_of(&(p.map(f32::to_bits), rad.map(f32::to_bits)));
        light(st, e, key, |r| r.create_sphere_light(key, p, radius, rad));
        seen.insert(e);
    }
    for (e, l, gt) in &suns {
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
    if let Some(ui) = &ui {
        if let Some(px) = ui.pixels.lock().ok().and_then(|mut p| p.take()) {
            let (w, h) = ui.size;
            if st.remix.draw_overlay(&px, w, h, 1.0).is_err() {
                st.errors += 1;
            }
        }
    }
    if st.remix.present().is_err() {
        st.errors += 1;
    }
    st.frames += 1;
    // Test aid: NVF_REMIX_SHOT=<seconds>:<file.bmp> saves what the Remix window shows,
    // that long after the runtime started (the window is brought to the front first).
    if let Some((secs, path)) = std::env::var("NVF_REMIX_SHOT").ok().and_then(|v| v.split_once(':').map(|(a, b)| (a.parse::<f32>().ok(), b.to_string()))).and_then(|(a, b)| a.map(|a| (a, b))) {
        let t = st.started.elapsed().as_secs_f32();
        if !st.shot_raised && t >= secs - 1.0 {
            st.child.raise();
            st.shot_raised = true;
        }
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
        AlphaMode::Mask(t) => (Some((t * 255.0) as u8), false),
        AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add | AlphaMode::Multiply => (None, true),
        _ => (None, false),
    };
    let desc = nvf_remix::MaterialDesc {
        hash: hash_of(&id),
        albedo: tex(&b.base_color_texture),
        normal: tex(&b.normal_map_texture),
        emissive: tex(&b.emissive_texture),
        emissive_intensity: if e.red + e.green + e.blue > 0.0 { 1.0 } else { 0.0 },
        emissive_color: [e.red, e.green, e.blue],
        albedo_constant: [c.red, c.green, c.blue],
        opacity: c.alpha,
        roughness: b.perceptual_roughness,
        metallic: b.metallic,
        alpha_test,
        blend,
    };
    match st.remix.create_material(&desc) {
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
            text.push_str("\n# NVForever\nrtx.automation.disableBlockingDialogBoxes = True\n");
            let _ = std::fs::write(&conf, text);
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
    let child = nvf_remix::window::Child::new(parent).ok_or("could not make the Remix child window")?;
    let mut remix = nvf_remix::Remix::load(&config.dll)?;
    remix.startup(child.hwnd.cast()).map_err(|e| e.to_string())?;
    // Bevy's space: Y up, right-handed [design: NVForever's scene].
    for (k, v) in [("rtx.zUp", "False"), ("rtx.leftHandedCoordinateSystem", "False")] {
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
        materials: HashMap::new(),
        meshes: HashMap::new(),
        lights: HashMap::new(),
        frames: 0,
        errors: 0,
        light_scale: scale("NVF_REMIX_LIGHT_SCALE", 400.0),
        sun_scale: scale("NVF_REMIX_SUN_SCALE", 2.0),
        started: std::time::Instant::now(),
        why: [0; 4],
        shot_raised: false,
        shot_taken: false,
    })
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
    size: (u32, u32),
    pixels: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}

fn setup_ui_overlay(mut commands: Commands, mut images: ResMut<Assets<Image>>, window: Query<&Window, With<PrimaryWindow>>) {
    use bevy::asset::RenderAssetUsages;
    use bevy::camera::{ClearColorConfig, RenderTarget};
    use bevy::render::gpu_readback::{Readback, ReadbackComplete};
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureUsages};
    let (w, h) = window.single().map(|w| (w.physical_width().max(1), w.physical_height().max(1))).unwrap_or((1280, 720));
    let mut image = Image::new_uninit(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    commands.spawn((
        Name::new("Remix UI camera"),
        Camera2d,
        Camera { order: 1000, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        RenderTarget::Image(image.clone().into()),
        IsDefaultUiCamera,
        Msaa::Off,
    ));
    let pixels: Arc<std::sync::Mutex<Option<Vec<u8>>>> = Arc::default();
    let sink = pixels.clone();
    // Rows come back padded to 256 bytes [documented: wgpu COPY_BYTES_PER_ROW_ALIGNMENT,
    // bevy_render gpu_readback get_aligned_size].
    let row = (w * 4) as usize;
    let stride = row.div_ceil(256) * 256;
    commands.spawn(Readback::texture(image)).observe(move |ev: On<ReadbackComplete>| {
        let data = &ev.data;
        let out = if stride == row {
            data.clone()
        } else {
            data.chunks(stride).take(h as usize).flat_map(|r| r[..row.min(r.len())].iter().copied()).collect()
        };
        if let Ok(mut p) = sink.lock() {
            *p = Some(out);
        }
    });
    commands.insert_resource(UiOverlay { size: (w, h), pixels });
}
