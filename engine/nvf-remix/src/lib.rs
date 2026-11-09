//! NVForever's RTX Remix renderer: the Remix runtime (`d3d9.dll` of a Remix install, the
//! `.trex` folder) loaded into NVForever's own process and driven through its C API
//! (`remix_c.h`): NVForever hands it meshes, materials, textures, lights and the camera
//! each frame, and Remix path-traces them into a window. No Direct3D 9, no bridge, no
//! game executable is involved (docs/design/remix.md).
//!
//! Every call goes to the thread that started the runtime; [`Remix`] is not `Send`.

// The API's handles are opaque tokens the runtime made; this crate passes them back and
// never dereferences them.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod ffi;
#[cfg(windows)]
pub mod window;

use std::ffi::{c_void, CString};
use std::path::{Path, PathBuf};

use ffi::*;

/// A Remix error code and the call that returned it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub call: &'static str,
    pub code: i32,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Remix {} failed: error code {} (0x{:X})", self.call, self.code, self.code as u32)
    }
}

impl std::error::Error for Error {}

fn check(call: &'static str, code: i32) -> Result<(), Error> {
    if code == SUCCESS {
        Ok(())
    } else {
        Err(Error { call, code })
    }
}

/// A wide, NUL-terminated string for the API's paths.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The path string a material gives for a texture made with [`Remix::create_texture`]:
/// its hash as `0x%016llX` [documented: New Vegas RTX's api_common.cpp, the form the
/// runtime looks textures up by].
pub fn texture_ref(hash: u64) -> Vec<u16> {
    wide(&format!("0x{hash:016X}"))
}

/// The loaded runtime.
pub struct Remix {
    api: Interface,
    _lib: libloading::Library,
    pub dll: PathBuf,
    started: bool,
    _not_send: std::marker::PhantomData<*mut ()>,
}

/// A texture's pixels for [`Remix::create_texture`].
pub struct TextureData<'a> {
    pub hash: u64,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    /// A `FORMAT_*` of [`ffi`].
    pub format: u32,
    /// Every mip level, largest first.
    pub data: &'a [u8],
    /// A Vulkan format in place of `format` (0: `format`), for the block formats the
    /// API's own list lacks (BC1 with alpha, BC2, BC4).
    pub vk_format: u32,
    /// `ENCODING_*`: how a normal map stores its vector.
    pub encoding: u32,
    /// Swap the pixels of the texture already registered under `hash` in place (a screen
    /// drawn every frame) [documented: remix_c.h, REMIXAPI_TEXTURE_HOST_IMPORT_FLAG_REPLACE].
    pub replace: bool,
}

/// What a material is made of.
#[derive(Clone, Debug, Default)]
pub struct MaterialDesc {
    pub hash: u64,
    pub albedo: Option<u64>,
    pub normal: Option<u64>,
    pub emissive: Option<u64>,
    pub emissive_intensity: f32,
    pub emissive_color: [f32; 3],
    pub albedo_constant: [f32; 3],
    pub opacity: f32,
    pub roughness: f32,
    pub metallic: f32,
    /// The alpha test and blend come from each instance's [`InstanceAlpha`] (alpha the
    /// texture's times the vertex colour's) instead of `alpha_test` and `blend`.
    pub draw_call_alpha: bool,
    /// Alpha-tested at this reference (0-255), or `None` for opaque.
    pub alpha_test: Option<u8>,
    /// Blended over what is behind, by this blend type (`BLEND_*`), or `None` for not.
    pub blend: Option<i32>,
    /// A layered material's tint texture (RGB multiplied over the blended layers, read at
    /// the weight textures' coordinates): a land quadrant's vertex colours.
    pub tint: Option<u64>,
}

/// An instance's alpha state (see [`Remix::draw_instance_skinned`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceAlpha {
    /// Alpha-tested at this reference (0-255).
    pub test: Option<u8>,
    /// Blended over what is behind by its alpha.
    pub blend: bool,
    /// The vertex colour is baked lighting: the runtime keeps its hue and drops its
    /// brightness (`rtx.vertexColorIsBakedLighting`). False for a real colour (unlit
    /// effects, particles).
    pub baked: bool,
}

impl Remix {
    /// Load the runtime from `dll` (a Remix `d3d9.dll`) with its folder on the DLL search
    /// path, and initialise the API at the version NVForever was written against.
    pub fn load(dll: &Path) -> Result<Remix, String> {
        let dir = dll.parent().ok_or("the Remix runtime path has no folder")?;
        // The runtime's own DLLs (DLSS, NRD, …) sit beside it.
        #[cfg(windows)]
        window::add_dll_dir(dir);
        // SAFETY: loading a library runs its initialisers; the Remix runtime is a regular
        // DLL with no requirements on the loading thread.
        let lib = unsafe { libloading::Library::new(dll) }.map_err(|e| format!("{}: {e}", dll.display()))?;
        // SAFETY: the symbol's type is remix_c.h's PFN_remixapi_InitializeLibrary.
        let init: libloading::Symbol<InitializeLibrary> =
            unsafe { lib.get(b"remixapi_InitializeLibrary\0") }.map_err(|e| format!("remixapi_InitializeLibrary: {e}"))?;
        let info = InitializeLibraryInfo { sType: ST_INITIALIZE_LIBRARY_INFO, pNext: std::ptr::null_mut(), version: version_make(VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH) };
        // SAFETY: Interface is all pointers, valid zeroed (None / null).
        let mut api: Interface = unsafe { std::mem::zeroed() };
        // SAFETY: both pointers are valid for the call.
        let code = unsafe { init(&info, &mut api) };
        check("InitializeLibrary", code).map_err(|e| e.to_string())?;
        if api.Startup.is_none() || api.Present.is_none() || api.CreateMesh.is_none() || api.DrawInstance.is_none() {
            return Err("the Remix runtime lacks the scene API (Startup, Present, CreateMesh, DrawInstance)".into());
        }
        Ok(Remix { api, _lib: lib, dll: dll.to_path_buf(), started: false, _not_send: std::marker::PhantomData })
    }

    /// Start rendering into `hwnd`.
    pub fn startup(&mut self, hwnd: *mut c_void) -> Result<(), Error> {
        let info = StartupInfo {
            sType: ST_STARTUP_INFO,
            pNext: std::ptr::null_mut(),
            hwnd,
            disableSrgbConversionForOutput: 0,
            forceNoVkSwapchain: 0,
            editorModeEnabled: 0,
            combineGuiInFinalColor: 0,
        };
        // SAFETY: checked present in load; info valid for the call.
        check("Startup", unsafe { (self.api.Startup.unwrap_unchecked())(&info) })?;
        self.started = true;
        Ok(())
    }

    pub fn started(&self) -> bool {
        self.started
    }

    /// A runtime option (`rtx.conf` key and value).
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), Error> {
        let Some(f) = self.api.SetConfigVariable else { return Err(Error { call: "SetConfigVariable", code: -1 }) };
        let (k, v) = (CString::new(key).unwrap_or_default(), CString::new(value).unwrap_or_default());
        // SAFETY: NUL-terminated strings valid for the call.
        check("SetConfigVariable", unsafe { f(k.as_ptr(), v.as_ptr()) })
    }

    /// Upload a texture; materials name it by [`texture_ref`] of its hash.
    pub fn create_texture(&self, t: &TextureData<'_>) -> Result<TextureHandle, Error> {
        let Some(f) = self.api.CreateTexture else { return Err(Error { call: "CreateTexture", code: -1 }) };
        let mut host = TextureInfoHostImportEXT { sType: ST_TEXTURE_INFO_HOST_IMPORT_EXT, pNext: std::ptr::null_mut(), encoding: t.encoding, vkFormat: t.vk_format, flags: u32::from(t.replace) };
        let ext = if t.vk_format != 0 || t.encoding != 0 || t.replace { (&mut host as *mut TextureInfoHostImportEXT).cast() } else { std::ptr::null_mut() };
        let info = TextureInfo {
            sType: ST_TEXTURE_INFO,
            pNext: ext,
            hash: t.hash,
            width: t.width,
            height: t.height,
            depth: 1,
            mipLevels: t.mips.max(1),
            format: t.format,
            data: t.data.as_ptr().cast(),
            dataSize: t.data.len() as u64,
        };
        let mut h = std::ptr::null_mut();
        // SAFETY: info and its data valid for the call; the runtime copies the pixels.
        check("CreateTexture", unsafe { f(&info, &mut h) })?;
        Ok(h)
    }

    pub fn destroy_texture(&self, h: TextureHandle) {
        if let Some(f) = self.api.DestroyTexture {
            // SAFETY: a handle this runtime made.
            unsafe { f(h) };
        }
    }

    pub fn create_material(&self, m: &MaterialDesc) -> Result<MaterialHandle, Error> {
        self.create_material_layered(m, None)
    }

    /// A terrain material: `layers` (albedo, normal texture hashes, bottom first) blended
    /// per hit by the two RGBA weight textures `weights` (layers 0-3, 4-7), read at
    /// `uv × scale + offset` [documented: remix_c.h, MaterialInfoTerrainLayersEXT].
    pub fn create_terrain_material(&self, m: &MaterialDesc, layers: &[(u64, Option<u64>)], weights: [u64; 2], scale: [f32; 2], offset: [f32; 2]) -> Result<MaterialHandle, Error> {
        self.create_material_layered(m, Some((layers, weights, scale, offset)))
    }

    #[allow(clippy::type_complexity)]
    fn create_material_layered(&self, m: &MaterialDesc, terrain: Option<(&[(u64, Option<u64>)], [u64; 2], [f32; 2], [f32; 2])>) -> Result<MaterialHandle, Error> {
        let Some(f) = self.api.CreateMaterial else { return Err(Error { call: "CreateMaterial", code: -1 }) };
        let none = [0u16];
        let albedo = m.albedo.map(texture_ref);
        let normal = m.normal.map(texture_ref);
        let emissive = m.emissive.map(texture_ref);
        let p = |v: &Option<Vec<u16>>| v.as_ref().map_or(none.as_ptr(), |w| w.as_ptr());
        let mut opaque = MaterialInfoOpaqueEXT {
            sType: ST_MATERIAL_INFO_OPAQUE_EXT,
            pNext: std::ptr::null_mut(),
            roughnessTexture: none.as_ptr(),
            metallicTexture: none.as_ptr(),
            anisotropy: 0.0,
            albedoConstant: m.albedo_constant.into(),
            opacityConstant: m.opacity,
            roughnessConstant: m.roughness,
            metallicConstant: m.metallic,
            thinFilmThickness_hasvalue: 0,
            thinFilmThickness_value: 0.0,
            alphaIsThinFilmThickness: 0,
            heightTexture: none.as_ptr(),
            displaceIn: 0.0,
            useDrawCallAlphaState: u32::from(m.draw_call_alpha),
            blendType_hasvalue: u32::from(m.blend.is_some()),
            blendType_value: m.blend.unwrap_or(BLEND_ALPHA),
            invertedBlend: 0,
            // AlphaTestType 6 greater-or-equal, 7 always (no test) [documented: the
            // runtime's surface_shared.h, AlphaTestType, the D3D compare order less 1].
            alphaTestType: if m.alpha_test.is_some() { 6 } else { 7 },
            alphaReferenceValue: m.alpha_test.unwrap_or(0),
            displaceOut: 0.0,
            enableDlssControlMask: 0,
            dlssControlMaskIntensity: 0.0,
            dlssControlMaskToneStrength: 0.0,
            dlssControlMaskStructuralStrength: 0.0,
        };
        // The terrain extension's strings, kept alive for the call.
        let layer_refs: Vec<(Vec<u16>, Option<Vec<u16>>)> = terrain.map(|t| t.0.iter().take(TERRAIN_MAX_LAYERS).map(|(a, n)| (texture_ref(*a), n.map(texture_ref))).collect()).unwrap_or_default();
        let weight_refs: Vec<Vec<u16>> = terrain.map(|t| t.1.iter().map(|w| texture_ref(*w)).collect()).unwrap_or_default();
        let tint_ref = m.tint.map(texture_ref);
        let mut layered = terrain.map(|(_, _, scale, offset)| {
            let mut albedo = [none.as_ptr(); TERRAIN_MAX_LAYERS];
            let mut normal = [none.as_ptr(); TERRAIN_MAX_LAYERS];
            for (i, (a, n)) in layer_refs.iter().enumerate() {
                albedo[i] = a.as_ptr();
                normal[i] = n.as_ref().map_or(none.as_ptr(), |w| w.as_ptr());
            }
            MaterialInfoTerrainLayersEXT {
                sType: ST_MATERIAL_INFO_TERRAIN_LAYERS_EXT,
                pNext: std::ptr::null_mut(),
                layerCount: layer_refs.len() as u32,
                albedoTextures: albedo,
                normalTextures: normal,
                weightTextures: [weight_refs[0].as_ptr(), weight_refs[1].as_ptr()],
                tintTexture: tint_ref.as_ref().map_or(none.as_ptr(), |w| w.as_ptr()),
                weightUvScale: Float2D { x: scale[0], y: scale[1] },
                weightUvOffset: Float2D { x: offset[0], y: offset[1] },
                normalsRgbTangentSpace: 1,
                heightTextures: [none.as_ptr(); TERRAIN_MAX_LAYERS],
            }
        });
        if let Some(l) = layered.as_mut() {
            opaque.pNext = (l as *mut MaterialInfoTerrainLayersEXT).cast();
        }
        let info = MaterialInfo {
            sType: ST_MATERIAL_INFO,
            pNext: (&mut opaque as *mut MaterialInfoOpaqueEXT).cast(),
            hash: m.hash,
            albedoTexture: p(&albedo),
            normalTexture: p(&normal),
            tangentTexture: none.as_ptr(),
            emissiveTexture: p(&emissive),
            emissiveIntensity: m.emissive_intensity,
            emissiveColorConstant: m.emissive_color.into(),
            spriteSheetRow: 1,
            spriteSheetCol: 1,
            spriteSheetFps: 0,
            filterMode: 1,
            // 1 repeat (0 is clamp) [documented: the runtime's lssusd/mdl_helpers.h,
            // WrapMode]: game textures tile; clamped, every surface whose texture
            // coordinates leave 0..1 showed its edge texel smeared (flat ground, smooth
            // rocks, streaked roads) [observed 2026-11-09].
            wrapModeU: 1,
            wrapModeV: 1,
        };
        let mut h = std::ptr::null_mut();
        // SAFETY: info chain and strings valid for the call.
        check("CreateMaterial", unsafe { f(&info, &mut h) })?;
        Ok(h)
    }

    /// A translucent material (water, glass): light passes through tinted by `color` over
    /// `distance` scene units, bent by `ior`; `normal` an optional normal map.
    pub fn create_translucent_material(&self, hash: u64, color: [f32; 3], distance: f32, ior: f32, normal: Option<u64>) -> Result<MaterialHandle, Error> {
        let Some(f) = self.api.CreateMaterial else { return Err(Error { call: "CreateMaterial", code: -1 }) };
        let none = [0u16];
        let normal = normal.map(texture_ref);
        let mut ext = MaterialInfoTranslucentEXT {
            sType: ST_MATERIAL_INFO_TRANSLUCENT_EXT,
            pNext: std::ptr::null_mut(),
            transmittanceTexture: none.as_ptr(),
            refractiveIndex: ior,
            transmittanceColor: color.into(),
            transmittanceMeasurementDistance: distance,
            thinWallThickness_hasvalue: 0,
            thinWallThickness_value: 0.0,
            useDiffuseLayer: 0,
        };
        let info = MaterialInfo {
            sType: ST_MATERIAL_INFO,
            pNext: (&mut ext as *mut MaterialInfoTranslucentEXT).cast(),
            hash,
            albedoTexture: none.as_ptr(),
            normalTexture: normal.as_ref().map_or(none.as_ptr(), |w| w.as_ptr()),
            tangentTexture: none.as_ptr(),
            emissiveTexture: none.as_ptr(),
            emissiveIntensity: 0.0,
            emissiveColorConstant: Float3D::default(),
            spriteSheetRow: 1,
            spriteSheetCol: 1,
            spriteSheetFps: 0,
            filterMode: 1,
            // 1 repeat (0 is clamp) [documented: the runtime's lssusd/mdl_helpers.h,
            // WrapMode]: game textures tile; clamped, every surface whose texture
            // coordinates leave 0..1 showed its edge texel smeared (flat ground, smooth
            // rocks, streaked roads) [observed 2026-11-09].
            wrapModeU: 1,
            wrapModeV: 1,
        };
        let mut h = std::ptr::null_mut();
        // SAFETY: info chain and strings valid for the call.
        check("CreateMaterial", unsafe { f(&info, &mut h) })?;
        Ok(h)
    }

    pub fn destroy_material(&self, h: MaterialHandle) {
        if let Some(f) = self.api.DestroyMaterial {
            // SAFETY: a handle this runtime made.
            unsafe { f(h) };
        }
    }

    /// A mesh of one surface.
    pub fn create_mesh(&self, hash: u64, vertices: &[HardcodedVertex], indices: &[u32], material: MaterialHandle) -> Result<MeshHandle, Error> {
        self.create_mesh_skinned(hash, vertices, indices, material, None)
    }

    /// A mesh of one surface, skinned when `skin` gives (bones per vertex, weights,
    /// bone indices), `bones per vertex` of each per vertex [documented: the runtime's
    /// rtx_remix_api.cpp reads `vertex * bonesPerVertex`].
    pub fn create_mesh_skinned(&self, hash: u64, vertices: &[HardcodedVertex], indices: &[u32], material: MaterialHandle, skin: Option<(u32, &[f32], &[u32])>) -> Result<MeshHandle, Error> {
        let skinning_value = match skin {
            Some((n, w, i)) => MeshInfoSkinning { bonesPerVertex: n, blendWeights_values: w.as_ptr(), blendWeights_count: w.len() as u32, blendIndices_values: i.as_ptr(), blendIndices_count: i.len() as u32 },
            None => MeshInfoSkinning { bonesPerVertex: 0, blendWeights_values: std::ptr::null(), blendWeights_count: 0, blendIndices_values: std::ptr::null(), blendIndices_count: 0 },
        };
        let surface = MeshInfoSurfaceTriangles {
            vertices_values: vertices.as_ptr(),
            vertices_count: vertices.len() as u64,
            indices_values: indices.as_ptr(),
            indices_count: indices.len() as u64,
            skinning_hasvalue: u32::from(skin.is_some()),
            skinning_value,
            material,
        };
        let info = MeshInfo { sType: ST_MESH_INFO, pNext: std::ptr::null_mut(), hash, surfaces_values: &surface, surfaces_count: 1 };
        let mut h = std::ptr::null_mut();
        // SAFETY: checked present in load; arrays valid for the call (the runtime copies).
        check("CreateMesh", unsafe { (self.api.CreateMesh.unwrap_unchecked())(&info, &mut h) })?;
        Ok(h)
    }

    /// New vertices for the live mesh made under `hash`, its triangles the same: the
    /// handle and its instances stay and the runtime refits its acceleration structure
    /// [documented: remix_c.h, MeshInfoRefreshGeometryEXT]. False when the runtime did
    /// not take it as that mesh (make it again then).
    pub fn refresh_mesh(&self, hash: u64, mesh: MeshHandle, vertices: &[HardcodedVertex], indices: &[u32], material: MaterialHandle) -> bool {
        #[repr(C)]
        struct Refresh {
            s_type: u32,
            p_next: *mut c_void,
        }
        let mut ext = Refresh { s_type: 29, p_next: std::ptr::null_mut() };
        let surface = MeshInfoSurfaceTriangles {
            vertices_values: vertices.as_ptr(),
            vertices_count: vertices.len() as u64,
            indices_values: indices.as_ptr(),
            indices_count: indices.len() as u64,
            skinning_hasvalue: 0,
            skinning_value: MeshInfoSkinning { bonesPerVertex: 0, blendWeights_values: std::ptr::null(), blendWeights_count: 0, blendIndices_values: std::ptr::null(), blendIndices_count: 0 },
            material,
        };
        let info = MeshInfo { sType: ST_MESH_INFO, pNext: (&mut ext as *mut Refresh).cast(), hash, surfaces_values: &surface, surfaces_count: 1 };
        let mut h = std::ptr::null_mut();
        // SAFETY: checked present in load; arrays valid for the call (the runtime copies).
        let code = unsafe { (self.api.CreateMesh.unwrap_unchecked())(&info, &mut h) };
        code == SUCCESS && h == mesh
    }

    pub fn destroy_mesh(&self, h: MeshHandle) {
        if let Some(f) = self.api.DestroyMesh {
            // SAFETY: a handle this runtime made.
            unsafe { f(h) };
        }
    }

    /// The frame's camera: position and axes in the scene's space, vertical field of view
    /// in degrees.
    #[allow(clippy::too_many_arguments)]
    pub fn setup_camera(&self, sky: bool, position: [f32; 3], forward: [f32; 3], up: [f32; 3], right: [f32; 3], fov_y: f32, aspect: f32, near: f32, far: f32) -> Result<(), Error> {
        let Some(f) = self.api.SetupCamera else { return Err(Error { call: "SetupCamera", code: -1 }) };
        let mut p = CameraInfoParameterizedEXT {
            sType: ST_CAMERA_INFO_PARAMETERIZED_EXT,
            pNext: std::ptr::null_mut(),
            position: position.into(),
            forward: forward.into(),
            up: up.into(),
            right: right.into(),
            fovYInDegrees: fov_y,
            aspect,
            nearPlane: near,
            farPlane: far,
        };
        let info = CameraInfo {
            sType: ST_CAMERA_INFO,
            pNext: (&mut p as *mut CameraInfoParameterizedEXT).cast(),
            type_: if sky { CAMERA_TYPE_SKY } else { CAMERA_TYPE_WORLD },
            view: [[0.0; 4]; 4],
            projection: [[0.0; 4]; 4],
        };
        // SAFETY: info chain valid for the call.
        check("SetupCamera", unsafe { f(&info) })
    }

    pub fn draw_instance(&self, mesh: MeshHandle, transform: &Transform, categories: u32, double_sided: bool) -> Result<(), Error> {
        self.draw_instance_skinned(mesh, transform, categories, double_sided, &[], None)
    }

    /// An instance posed by `bones` (object space, at most [`MAX_BONES`]; none: unposed).
    ///
    /// `alpha`: the instance's alpha state, for a material made with
    /// [`MaterialDesc::draw_call_alpha`]: alpha is the texture's times the vertex colour's.
    pub fn draw_instance_skinned(&self, mesh: MeshHandle, transform: &Transform, categories: u32, double_sided: bool, bones: &[Transform], alpha: Option<InstanceAlpha>) -> Result<(), Error> {
        // As New Vegas RTX's vertex_color_blend and alpha_blend [documented: the wrapper's
        // api_material.cpp]: colour texture x vertex colour; alpha the texture's, times the
        // vertex colour's when blended or tested; D3D9 blends alpha by the colour factors.
        let mut blend = alpha.map(|a| InstanceInfoBlendEXT {
            sType: ST_INSTANCE_INFO_BLEND_EXT,
            pNext: std::ptr::null_mut(),
            alphaTestEnabled: u32::from(a.test.is_some()),
            alphaTestReferenceValue: a.test.unwrap_or(0),
            alphaTestCompareOp: if a.test.is_some() { VK_COMPARE_GREATER_OR_EQUAL } else { 7 },
            alphaBlendEnabled: u32::from(a.blend),
            srcColorBlendFactor: if a.blend { VK_BLEND_SRC_ALPHA } else { VK_BLEND_ONE },
            dstColorBlendFactor: if a.blend { VK_BLEND_ONE_MINUS_SRC_ALPHA } else { 0 },
            colorBlendOp: 0,
            textureColorArg1Source: TA_TEXTURE,
            textureColorArg2Source: TA_VERTEX_COLOR,
            textureColorOperation: TOP_MODULATE,
            textureAlphaArg1Source: TA_TEXTURE,
            textureAlphaArg2Source: TA_VERTEX_COLOR,
            textureAlphaOperation: if a.blend || a.test.is_some() { TOP_MODULATE } else { TOP_SELECT_ARG1 },
            tFactor: 0xFFFF_FFFF,
            isTextureFactorBlend: 0,
            srcAlphaBlendFactor: if a.blend { VK_BLEND_SRC_ALPHA } else { VK_BLEND_ONE },
            dstAlphaBlendFactor: if a.blend { VK_BLEND_ONE_MINUS_SRC_ALPHA } else { 0 },
            alphaBlendOp: 0,
            writeMask: 0xF,
            isVertexColorBakedLighting: u32::from(a.baked),
        });
        let blend_ptr: *mut c_void = blend.as_mut().map_or(std::ptr::null_mut(), |b| (b as *mut InstanceInfoBlendEXT).cast());
        let mut ext = InstanceInfoBoneTransformsEXT { sType: ST_INSTANCE_INFO_BONE_TRANSFORMS_EXT, pNext: blend_ptr, boneTransforms_values: bones.as_ptr(), boneTransforms_count: bones.len().min(MAX_BONES) as u32 };
        let next = if bones.is_empty() { blend_ptr } else { (&mut ext as *mut InstanceInfoBoneTransformsEXT).cast() };
        let info = InstanceInfo { sType: ST_INSTANCE_INFO, pNext: next, categoryFlags: categories, mesh, transform: *transform, doubleSided: u32::from(double_sided) };
        // SAFETY: checked present in load.
        check("DrawInstance", unsafe { (self.api.DrawInstance.unwrap_unchecked())(&info) })
    }

    /// A sphere light (point light of `radius`), radiance per channel.
    pub fn create_sphere_light(&self, hash: u64, position: [f32; 3], radius: f32, radiance: [f32; 3]) -> Result<LightHandle, Error> {
        let mut s = LightInfoSphereEXT {
            sType: ST_LIGHT_INFO_SPHERE_EXT,
            pNext: std::ptr::null_mut(),
            position: position.into(),
            radius,
            shaping_hasvalue: 0,
            shaping_value: LightInfoLightShaping { direction: Float3D::default(), coneAngleDegrees: 180.0, coneSoftness: 0.0, focusExponent: 0.0 },
            volumetricRadianceScale: 1.0,
        };
        self.light(hash, radiance, (&mut s as *mut LightInfoSphereEXT).cast())
    }

    /// A distant light (the sun) shining along `direction`.
    pub fn create_distant_light(&self, hash: u64, direction: [f32; 3], angular_diameter: f32, radiance: [f32; 3]) -> Result<LightHandle, Error> {
        // The runtime refuses a direction that is not of unit length (INVALID_ARGUMENTS)
        // [observed: the smoke example, 2026-11-09].
        let l = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
        if l.is_nan() || l <= 1e-6 {
            return Err(Error { call: "CreateLight", code: 3 });
        }
        let direction = [direction[0] / l, direction[1] / l, direction[2] / l];
        let mut d = LightInfoDistantEXT { sType: ST_LIGHT_INFO_DISTANT_EXT, pNext: std::ptr::null_mut(), direction: direction.into(), angularDiameterDegrees: angular_diameter, volumetricRadianceScale: 1.0 };
        self.light(hash, radiance, (&mut d as *mut LightInfoDistantEXT).cast())
    }

    fn light(&self, hash: u64, radiance: [f32; 3], ext: *mut c_void) -> Result<LightHandle, Error> {
        let Some(f) = self.api.CreateLight else { return Err(Error { call: "CreateLight", code: -1 }) };
        let info = LightInfo { sType: ST_LIGHT_INFO, pNext: ext, hash, radiance: radiance.into(), isDynamic: 1, ignoreViewModel: 0 };
        let mut h = std::ptr::null_mut();
        // SAFETY: info chain valid for the call.
        check("CreateLight", unsafe { f(&info, &mut h) })?;
        Ok(h)
    }

    pub fn draw_light(&self, h: LightHandle) -> Result<(), Error> {
        let Some(f) = self.api.DrawLightInstance else { return Err(Error { call: "DrawLightInstance", code: -1 }) };
        // SAFETY: a handle this runtime made.
        check("DrawLightInstance", unsafe { f(h) })
    }

    pub fn destroy_light(&self, h: LightHandle) {
        if let Some(f) = self.api.DestroyLight {
            // SAFETY: a handle this runtime made.
            unsafe { f(h) };
        }
    }

    /// An RGBA8 image over the frame (the HUD and menus), straight alpha.
    pub fn draw_overlay(&self, rgba: &[u8], width: u32, height: u32, opacity: f32) -> Result<(), Error> {
        let Some(f) = self.api.DrawScreenOverlay else { return Err(Error { call: "DrawScreenOverlay", code: -1 }) };
        if rgba.len() < (width * height * 4) as usize {
            return Err(Error { call: "DrawScreenOverlay", code: 3 });
        }
        // SAFETY: the pixels are valid for the call; the runtime copies them.
        check("DrawScreenOverlay", unsafe { f(rgba.as_ptr().cast(), width, height, FORMAT_R8G8B8A8_UNORM, opacity) })
    }

    pub fn has_overlay(&self) -> bool {
        self.api.DrawScreenOverlay.is_some()
    }

    /// End the frame: Remix traces what was drawn and shows it.
    pub fn present(&self) -> Result<(), Error> {
        let info = PresentInfo { sType: ST_PRESENT_INFO, pNext: std::ptr::null_mut(), hwndOverride: std::ptr::null_mut() };
        // SAFETY: checked present in load.
        check("Present", unsafe { (self.api.Present.unwrap_unchecked())(&info) })
    }
}

impl Drop for Remix {
    fn drop(&mut self) {
        if let Some(f) = self.api.Shutdown {
            // SAFETY: the runtime is loaded until `_lib` drops after this.
            unsafe { f() };
        }
    }
}

/// A row-major 3x4 transform from a column-major 4x4 (glam's `to_cols_array_2d`).
pub fn transform_from_cols(c: [[f32; 4]; 4]) -> Transform {
    Transform { matrix: [[c[0][0], c[1][0], c[2][0], c[3][0]], [c[0][1], c[1][1], c[2][1], c[3][1]], [c[0][2], c[1][2], c[2][2], c[3][2]]] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_refs_are_the_hash_in_hex() {
        assert_eq!(String::from_utf16_lossy(&texture_ref(0xAB)[..18]), "0x00000000000000AB");
    }

    #[test]
    fn transforms_are_row_major_with_translation_last() {
        let t = transform_from_cols([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [5.0, 6.0, 7.0, 1.0]]);
        assert_eq!(t.matrix[0][3], 5.0);
        assert_eq!(t.matrix[2][3], 7.0);
    }
}
