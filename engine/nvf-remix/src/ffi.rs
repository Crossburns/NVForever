//! The RTX Remix C API as `remix_c.h` declares it (Remix Plus, version 0.1000.1, the
//! runtime High and Dry's New Vegas RTX 1.2.0-rc6 ships), the part NVForever uses. Every
//! struct is laid out field for field as the header's [documented: remix_c.h, MIT,
//! NVIDIA and the New Vegas RTX authors]; fields NVForever never sets are still declared
//! so the sizes match. The interface's unused entries are kept as opaque pointers in
//! their places.
#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::ffi::c_void;

pub type Bool = u32;
pub type ErrorCode = i32;
pub type Path = *const u16;
pub type Hwnd = *mut c_void;
pub type MaterialHandle = *mut c_void;
pub type MeshHandle = *mut c_void;
pub type LightHandle = *mut c_void;
pub type TextureHandle = *mut c_void;

pub const VERSION_MAJOR: u64 = 0;
pub const VERSION_MINOR: u64 = 1000;
pub const VERSION_PATCH: u64 = 1;
pub const fn version_make(major: u64, minor: u64, patch: u64) -> u64 {
    (major << 48) | (minor << 16) | patch
}

pub const SUCCESS: ErrorCode = 0;

// remixapi_StructType
pub const ST_INITIALIZE_LIBRARY_INFO: u32 = 1;
pub const ST_MATERIAL_INFO: u32 = 2;
pub const ST_MATERIAL_INFO_TRANSLUCENT_EXT: u32 = 4;
pub const ST_MATERIAL_INFO_OPAQUE_EXT: u32 = 5;
pub const ST_LIGHT_INFO: u32 = 6;
pub const ST_LIGHT_INFO_DISTANT_EXT: u32 = 7;
pub const ST_LIGHT_INFO_SPHERE_EXT: u32 = 11;
pub const ST_MESH_INFO: u32 = 12;
pub const ST_INSTANCE_INFO: u32 = 13;
pub const ST_INSTANCE_INFO_BONE_TRANSFORMS_EXT: u32 = 14;
pub const ST_INSTANCE_INFO_BLEND_EXT: u32 = 15;
pub const MAX_BONES: usize = 256;
pub const ST_CAMERA_INFO: u32 = 16;
pub const ST_CAMERA_INFO_PARAMETERIZED_EXT: u32 = 17;
pub const ST_STARTUP_INFO: u32 = 22;
pub const ST_PRESENT_INFO: u32 = 23;
pub const ST_TEXTURE_INFO: u32 = 25;
pub const ST_TEXTURE_INFO_HOST_IMPORT_EXT: u32 = 33;
pub const ST_MATERIAL_INFO_TERRAIN_LAYERS_EXT: u32 = 0x7F00_0001;
pub const TERRAIN_MAX_LAYERS: usize = 8;

// REMIXAPI_TEXTURE_ENCODING_*
pub const ENCODING_NONE: u32 = 0;
pub const ENCODING_NORMAL_TANGENT_XY: u32 = 1;

// VkFormat values the host-import extension takes as they are [documented: Vulkan's
// VkFormat; the runtime checks them with imageFormatInfo].
pub const VK_BC1_RGBA_UNORM: u32 = 133;
pub const VK_BC1_RGBA_SRGB: u32 = 134;
pub const VK_BC2_UNORM: u32 = 135;
pub const VK_BC2_SRGB: u32 = 136;
pub const VK_BC3_UNORM: u32 = 137;
pub const VK_BC3_SRGB: u32 = 138;
pub const VK_BC4_UNORM: u32 = 139;
pub const VK_BC5_UNORM: u32 = 141;

// remixapi_Format (VkFormat values)
pub const FORMAT_R8G8B8A8_UNORM: u32 = 37;
pub const FORMAT_R8G8B8A8_SRGB: u32 = 43;
pub const FORMAT_B8G8R8A8_UNORM: u32 = 44;
pub const FORMAT_B8G8R8A8_SRGB: u32 = 50;
pub const FORMAT_BC1_RGB_UNORM: u32 = 131;
pub const FORMAT_BC1_RGB_SRGB: u32 = 132;
pub const FORMAT_BC3_UNORM: u32 = 135;
pub const FORMAT_BC3_SRGB: u32 = 136;
pub const FORMAT_BC5_UNORM: u32 = 139;
pub const FORMAT_BC7_UNORM: u32 = 145;
pub const FORMAT_BC7_SRGB: u32 = 146;

// remixapi_CameraType
pub const CAMERA_TYPE_WORLD: u32 = 0;
pub const CAMERA_TYPE_SKY: u32 = 1;
pub const CAMERA_TYPE_VIEW_MODEL: u32 = 2;

// remixapi_InstanceCategoryBit
pub const CATEGORY_WORLD_UI: u32 = 1 << 0;
pub const CATEGORY_WORLD_MATTE: u32 = 1 << 1;
pub const CATEGORY_SKY: u32 = 1 << 2;
pub const CATEGORY_IGNORE: u32 = 1 << 3;
pub const CATEGORY_IGNORE_LIGHTS: u32 = 1 << 4;
pub const CATEGORY_DECAL_STATIC: u32 = 1 << 11;
pub const CATEGORY_ANIMATED_WATER: u32 = 1 << 17;
/// Blend types [documented: the runtime's surface_shared.h, BlendType]: alpha over what
/// is behind; colour times alpha added to it (a glow); multiplied with it.
pub const BLEND_ALPHA: i32 = 0;
pub const BLEND_ALPHA_EMISSIVE: i32 = 1;
pub const BLEND_MULTIPLICATIVE: i32 = 7;
pub const CATEGORY_THIRD_PERSON_PLAYER_MODEL: u32 = 1 << 18;
pub const CATEGORY_THIRD_PERSON_PLAYER_BODY: u32 = 1 << 19;
pub const CATEGORY_VIEW_MODEL: u32 = 1 << 26;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Float2D {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Float3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl From<[f32; 3]> for Float3D {
    fn from(v: [f32; 3]) -> Self {
        Float3D { x: v[0], y: v[1], z: v[2] }
    }
}

/// Row-major 3x4: rotation-scale in the first three columns, translation in the fourth.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub matrix: [[f32; 4]; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Transform { matrix: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]] }
    }
}

#[repr(C)]
pub struct InitializeLibraryInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub version: u64,
}

#[repr(C)]
pub struct StartupInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub hwnd: Hwnd,
    pub disableSrgbConversionForOutput: Bool,
    pub forceNoVkSwapchain: Bool,
    pub editorModeEnabled: Bool,
    pub combineGuiInFinalColor: Bool,
}

#[repr(C)]
pub struct MaterialInfoOpaqueEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub roughnessTexture: Path,
    pub metallicTexture: Path,
    pub anisotropy: f32,
    pub albedoConstant: Float3D,
    pub opacityConstant: f32,
    pub roughnessConstant: f32,
    pub metallicConstant: f32,
    pub thinFilmThickness_hasvalue: Bool,
    pub thinFilmThickness_value: f32,
    pub alphaIsThinFilmThickness: Bool,
    pub heightTexture: Path,
    pub displaceIn: f32,
    pub useDrawCallAlphaState: Bool,
    pub blendType_hasvalue: Bool,
    pub blendType_value: i32,
    pub invertedBlend: Bool,
    pub alphaTestType: i32,
    pub alphaReferenceValue: u8,
    pub displaceOut: f32,
    pub enableDlssControlMask: Bool,
    pub dlssControlMaskIntensity: f32,
    pub dlssControlMaskToneStrength: f32,
    pub dlssControlMaskStructuralStrength: f32,
}

/// Remix Plus: up to eight terrain layers blended per hit by two RGBA weight textures,
/// chained after the opaque extension.
#[repr(C)]
pub struct MaterialInfoTerrainLayersEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub layerCount: u32,
    pub albedoTextures: [Path; TERRAIN_MAX_LAYERS],
    pub normalTextures: [Path; TERRAIN_MAX_LAYERS],
    pub weightTextures: [Path; 2],
    pub tintTexture: Path,
    pub weightUvScale: Float2D,
    pub weightUvOffset: Float2D,
    pub normalsRgbTangentSpace: Bool,
    pub heightTextures: [Path; TERRAIN_MAX_LAYERS],
}

#[repr(C)]
pub struct MaterialInfoTranslucentEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub transmittanceTexture: Path,
    pub refractiveIndex: f32,
    pub transmittanceColor: Float3D,
    pub transmittanceMeasurementDistance: f32,
    pub thinWallThickness_hasvalue: Bool,
    pub thinWallThickness_value: f32,
    pub useDiffuseLayer: Bool,
}

#[repr(C)]
pub struct MaterialInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub hash: u64,
    pub albedoTexture: Path,
    pub normalTexture: Path,
    pub tangentTexture: Path,
    pub emissiveTexture: Path,
    pub emissiveIntensity: f32,
    pub emissiveColorConstant: Float3D,
    pub spriteSheetRow: u8,
    pub spriteSheetCol: u8,
    pub spriteSheetFps: u8,
    pub filterMode: u8,
    pub wrapModeU: u8,
    pub wrapModeV: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct HardcodedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub texcoord: [f32; 2],
    pub color: u32,
    pub _pad: [u32; 7],
}

#[repr(C)]
pub struct MeshInfoSkinning {
    pub bonesPerVertex: u32,
    pub blendWeights_values: *const f32,
    pub blendWeights_count: u32,
    pub blendIndices_values: *const u32,
    pub blendIndices_count: u32,
}

#[repr(C)]
pub struct MeshInfoSurfaceTriangles {
    pub vertices_values: *const HardcodedVertex,
    pub vertices_count: u64,
    pub indices_values: *const u32,
    pub indices_count: u64,
    pub skinning_hasvalue: Bool,
    pub skinning_value: MeshInfoSkinning,
    pub material: MaterialHandle,
}

#[repr(C)]
pub struct MeshInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub hash: u64,
    pub surfaces_values: *const MeshInfoSurfaceTriangles,
    pub surfaces_count: u32,
}

#[repr(C)]
pub struct CameraInfoParameterizedEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub position: Float3D,
    pub forward: Float3D,
    pub up: Float3D,
    pub right: Float3D,
    pub fovYInDegrees: f32,
    pub aspect: f32,
    pub nearPlane: f32,
    pub farPlane: f32,
}

#[repr(C)]
pub struct CameraInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub type_: u32,
    pub view: [[f32; 4]; 4],
    pub projection: [[f32; 4]; 4],
}

#[repr(C)]
pub struct InstanceInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub categoryFlags: u32,
    pub mesh: MeshHandle,
    pub transform: Transform,
    pub doubleSided: Bool,
}

#[repr(C)]
pub struct InstanceInfoBoneTransformsEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub boneTransforms_values: *const Transform,
    pub boneTransforms_count: u32,
}

/// An instance's fixed-function alpha state (D3D9's alpha test, blend and texture stage),
/// used by a material made with `useDrawCallAlphaState` [documented: remix_c.h].
#[repr(C)]
pub struct InstanceInfoBlendEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub alphaTestEnabled: u32,
    pub alphaTestReferenceValue: u8,
    pub alphaTestCompareOp: u32,
    pub alphaBlendEnabled: u32,
    pub srcColorBlendFactor: u32,
    pub dstColorBlendFactor: u32,
    pub colorBlendOp: u32,
    pub textureColorArg1Source: u32,
    pub textureColorArg2Source: u32,
    pub textureColorOperation: u32,
    pub textureAlphaArg1Source: u32,
    pub textureAlphaArg2Source: u32,
    pub textureAlphaOperation: u32,
    pub tFactor: u32,
    pub isTextureFactorBlend: u32,
    pub srcAlphaBlendFactor: u32,
    pub dstAlphaBlendFactor: u32,
    pub alphaBlendOp: u32,
    pub writeMask: u32,
    pub isVertexColorBakedLighting: u32,
}

/// Texture-stage argument sources and operations [documented: the runtime's
/// surface_shared.h, RtTextureArgSource, DxvkRtTextureOperation]; compare ops and blend
/// factors are Vulkan's.
pub const TA_TEXTURE: u32 = 1;
pub const TA_VERTEX_COLOR: u32 = 2;
pub const TOP_SELECT_ARG1: u32 = 1;
pub const TOP_MODULATE: u32 = 3;
pub const VK_COMPARE_GREATER_OR_EQUAL: u32 = 6;
pub const VK_BLEND_ZERO: u32 = 0;
pub const VK_BLEND_ONE: u32 = 1;
pub const VK_BLEND_DST_COLOR: u32 = 4;
pub const VK_BLEND_SRC_ALPHA: u32 = 6;
pub const VK_BLEND_ONE_MINUS_SRC_ALPHA: u32 = 7;

#[repr(C)]
pub struct LightInfoLightShaping {
    pub direction: Float3D,
    pub coneAngleDegrees: f32,
    pub coneSoftness: f32,
    pub focusExponent: f32,
}

#[repr(C)]
pub struct LightInfoSphereEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub position: Float3D,
    pub radius: f32,
    pub shaping_hasvalue: Bool,
    pub shaping_value: LightInfoLightShaping,
    pub volumetricRadianceScale: f32,
}

#[repr(C)]
pub struct LightInfoDistantEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub direction: Float3D,
    pub angularDiameterDegrees: f32,
    pub volumetricRadianceScale: f32,
}

#[repr(C)]
pub struct LightInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub hash: u64,
    pub radiance: Float3D,
    pub isDynamic: Bool,
    pub ignoreViewModel: Bool,
}

#[repr(C)]
pub struct TextureInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub hash: u64,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mipLevels: u32,
    pub format: u32,
    pub data: *const c_void,
    pub dataSize: u64,
}

#[repr(C)]
pub struct TextureInfoHostImportEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub encoding: u32,
    pub vkFormat: u32,
    pub flags: u32,
}

#[repr(C)]
pub struct PresentInfo {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub hwndOverride: Hwnd,
}

type Fp = *const c_void;

/// `remixapi_Interface`, entry for entry.
#[repr(C)]
pub struct Interface {
    pub Shutdown: Option<unsafe extern "system" fn() -> ErrorCode>,
    pub CreateMaterial: Option<unsafe extern "system" fn(*const MaterialInfo, *mut MaterialHandle) -> ErrorCode>,
    pub DestroyMaterial: Option<unsafe extern "system" fn(MaterialHandle) -> ErrorCode>,
    pub CreateMesh: Option<unsafe extern "system" fn(*const MeshInfo, *mut MeshHandle) -> ErrorCode>,
    pub CreateMeshBatched: Fp,
    pub DestroyMesh: Option<unsafe extern "system" fn(MeshHandle) -> ErrorCode>,
    pub SetupCamera: Option<unsafe extern "system" fn(*const CameraInfo) -> ErrorCode>,
    pub DrawInstance: Option<unsafe extern "system" fn(*const InstanceInfo) -> ErrorCode>,
    pub CreateLight: Option<unsafe extern "system" fn(*const LightInfo, *mut LightHandle) -> ErrorCode>,
    pub CreateLightBatched: Fp,
    pub DestroyLight: Option<unsafe extern "system" fn(LightHandle) -> ErrorCode>,
    pub DrawLightInstance: Option<unsafe extern "system" fn(LightHandle) -> ErrorCode>,
    pub SetConfigVariable: Option<unsafe extern "system" fn(*const i8, *const i8) -> ErrorCode>,
    pub AddTextureHash: Fp,
    pub RemoveTextureHash: Fp,
    pub CreateTexture: Option<unsafe extern "system" fn(*const TextureInfo, *mut TextureHandle) -> ErrorCode>,
    pub DestroyTexture: Option<unsafe extern "system" fn(TextureHandle) -> ErrorCode>,
    pub dxvk_CreateD3D9: Fp,
    pub dxvk_RegisterD3D9Device: Fp,
    pub dxvk_GetExternalSwapchain: Fp,
    pub dxvk_GetVkImage: Fp,
    pub dxvk_CopyRenderingOutput: Fp,
    pub dxvk_SetDefaultOutput: Fp,
    pub dxvk_GetTextureHash: Fp,
    pub dxvk_GetSharedD3D11TextureHandle: Fp,
    pub pick_RequestObjectPicking: Fp,
    pub pick_HighlightObjects: Fp,
    pub Startup: Option<unsafe extern "system" fn(*const StartupInfo) -> ErrorCode>,
    pub Present: Option<unsafe extern "system" fn(*const PresentInfo) -> ErrorCode>,
    pub SetCameraMediumMaterial: Fp,
    pub GetUIState: Fp,
    pub SetUIState: Fp,
    pub RegisterCallbacks: Fp,
    pub AutoInstancePersistentLights: Fp,
    pub UpdateLightDefinition: Fp,
    pub DrawScreenOverlay: Option<unsafe extern "system" fn(*const c_void, u32, u32, u32, f32) -> ErrorCode>,
    pub SetGameValue: Fp,
    pub RequestVramCompaction: Fp,
    pub GetVramStats: Fp,
    pub RequestTextureVramFree: Fp,
    pub GetGameValue: Fp,
}

pub type InitializeLibrary = unsafe extern "system" fn(*const InitializeLibraryInfo, *mut Interface) -> ErrorCode;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    // Sizes from the header's layout on x64 (natural alignment).
    #[test]
    fn structs_have_the_headers_sizes() {
        assert_eq!(size_of::<HardcodedVertex>(), 64);
        assert_eq!(size_of::<Transform>(), 48);
        assert_eq!(size_of::<InstanceInfo>(), 8 + 8 + 4 + 4 + 8 + 48 + 4 + 4);
        assert_eq!(size_of::<Interface>(), 41 * 8);
        assert_eq!(size_of::<MeshInfoSurfaceTriangles>(), 8 + 8 + 8 + 8 + 4 + 4 + 40 + 8);
        assert_eq!(size_of::<StartupInfo>(), 4 + 4 + 8 + 8 + 16);
    }
}
