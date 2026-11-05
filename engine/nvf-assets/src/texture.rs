//! DDS → GPU textures. Block-compressed data is uploaded as is (BC1/BC2/BC3 with its
//! mip chain); the three uncompressed layouts are converted to RGBA8.

use crate::data::GameData;
use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormatSupport, CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
    ImageSamplerDescriptor,
};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::asset::AssetHandleProvider;
use fnv_dds::{to_rgba8, Dds, Format};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

/// Loaded textures by (VFS path, sRGB); `None` remembers a failure.
#[derive(Resource, Default)]
pub struct TextureCache {
    map: HashMap<(String, bool), Option<Handle<Image>>>,
    /// Copies sampled with clamped edges ([`Self::get_clamped`]), by the same key.
    clamped: HashMap<(String, bool), Option<Handle<Image>>>,
    /// Files read ahead on a worker thread ([`Self::offer`]), by resolved path, with when
    /// they came in.
    ahead: HashMap<String, (Vec<u8>, f64)>,
    /// Textures decoded on worker threads for models prepared there
    /// (`ModelCtx::models_ready`), shared with them.
    pub(crate) workers: Arc<WorkerTextures>,
    /// Counts changes to `map`, so a copy of it for worker threads is made again only
    /// when it changed ([`Self::snapshot`]).
    generation: u64,
    pub failures: Vec<String>,
}

/// Seconds a texture read ahead waits to be loaded before it is dropped.
const AHEAD_KEEP_SECS: f64 = 30.0;

/// The textures worker threads decoded, by (resolved path, sRGB): the handle reserved for
/// each, its format, and the image until the main thread puts it in the image store
/// ([`TextureCache::take_workers`]). A worker that needs a texture another one decoded
/// takes the same handle. Entries stay a while after their image went in, so workers with
/// an older copy of the cache's list ([`TextureCache::snapshot`]) still find them.
#[derive(Default)]
pub struct WorkerTextures {
    entries: Mutex<HashMap<(String, bool), WorkerTexture>>,
    /// Textures a worker could not load, with why, until the cache remembers them.
    failed: Mutex<HashMap<(String, bool), String>>,
}

struct WorkerTexture {
    handle: Handle<Image>,
    format: TextureFormat,
    image: Option<Image>,
    /// When the image went into the store (real seconds); `None` while it waits.
    stored_at: Option<f64>,
}

/// Seconds a worker texture's entry stays after its image went into the store.
const WORKER_KEEP_SECS: f64 = 30.0;

/// The textures the cache holds, with their formats, as a worker thread sees them.
pub type TextureSnapshot = HashMap<(String, bool), (Handle<Image>, TextureFormat)>;

/// A texture for a worker thread: from the cache's list, from another worker, or read and
/// decoded here under a handle reserved from `images` (the image store's handle provider).
/// `None`: not in any archive, or not decodable (a load on the main thread reports why).
pub(crate) fn worker_texture(
    raw_path: &str,
    srgb: bool,
    data: &GameData,
    cached: &TextureSnapshot,
    workers: &WorkerTextures,
    images: &AssetHandleProvider,
    bc_supported: bool,
) -> Option<(Handle<Image>, TextureFormat)> {
    let key = (resolved_path(raw_path, |p| data.vfs.exists(p)), srgb);
    if let Some(found) = cached.get(&key) {
        return Some(found.clone());
    }
    let shared = |w: &WorkerTexture| (w.handle.clone(), w.format);
    if let Some(w) = workers.entries.lock().expect("worker textures").get(&key) {
        return Some(shared(w));
    }
    if workers.failed.lock().expect("worker textures").contains_key(&key) {
        return None;
    }
    let image = match data.read(&key.0).ok_or_else(|| "not in any archive".to_string()).and_then(|bytes| dds_image(&bytes, srgb, bc_supported)) {
        Ok(mut image) => {
            image.texture_descriptor.label = Some(folder_label(&key.0));
            image
        }
        Err(e) => {
            workers.failed.lock().expect("worker textures").insert(key, e);
            return None;
        }
    };
    let mut entries = workers.entries.lock().expect("worker textures");
    // Another worker may have decoded it meanwhile: its handle is the one to use.
    let w = entries.entry(key).or_insert_with(|| WorkerTexture {
        handle: images.reserve_handle().typed::<Image>(),
        format: image.texture_descriptor.format,
        image: Some(image),
        stored_at: None,
    });
    Some(shared(w))
}

/// The archive path a NIF texture path loads from: normalised, the PC's own form of it if
/// any ([`pc_texture`]).
pub fn resolved_path(raw_path: &str, exists: impl Fn(&str) -> bool) -> String {
    let path = fnv_vfs::texture_path(raw_path);
    pc_texture(&path, exists).unwrap_or(path)
}

/// Anisotropic filtering of the game's textures made from now on (8 unless the
/// renderer's graphics settings say otherwise: nvforever `graphics.rs`).
static ANISOTROPY: AtomicU16 = AtomicU16::new(8);

/// Set the anisotropic filtering (1, 2, 4, 8 or 16) of textures made from now on.
pub fn set_anisotropy(clamp: u16) {
    ANISOTROPY.store(clamp.clamp(1, 16), Ordering::Relaxed);
}

pub fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: ANISOTROPY.load(Ordering::Relaxed),
        ..ImageSamplerDescriptor::linear()
    })
}

/// Build a GPU image from DDS bytes; `None` for cube maps, unsupported formats, or BC
/// textures the device cannot sample.
pub fn dds_image(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    // A panic while decoding is this texture's failure, not the game's (guard.rs).
    crate::guard::guarded(|| dds_image_unguarded(bytes, srgb, bc_supported, false))
}

/// [`dds_image`] with the largest level only, its smaller ones left out: for a sheet of
/// pictures cut out by rectangles (a font's glyphs), where a smaller level blends each
/// picture with its neighbours. The base game's font textures have one level
/// [observed: fonts.md §2]; a localized or modded one may have more (#1439).
pub fn dds_image_top(bytes: &[u8], srgb: bool, bc_supported: bool) -> Result<Image, String> {
    crate::guard::guarded(|| dds_image_unguarded(bytes, srgb, bc_supported, true))
}

fn dds_image_unguarded(bytes: &[u8], srgb: bool, bc_supported: bool, top_only: bool) -> Result<Image, String> {
    let dds = Dds::parse(bytes).map_err(|e| e.to_string())?;
    if dds.header.is_cubemap() {
        return Err("cube map (not used by the viewer)".into());
    }
    let (w, h) = (dds.header.width, dds.header.height);
    let levels = if top_only { &dds.levels[0][..1] } else { &dds.levels[0][..] };
    let (format, data) = match dds.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            if !bc_supported {
                return Err("GPU lacks BC texture support".into());
            }
            if w % 4 != 0 || h % 4 != 0 {
                return Err(format!("{w}x{h} block-compressed texture is not a multiple of 4"));
            }
            let f = match (dds.format, srgb) {
                (Format::Dxt1, true) => TextureFormat::Bc1RgbaUnormSrgb,
                (Format::Dxt1, false) => TextureFormat::Bc1RgbaUnorm,
                (Format::Dxt3, true) => TextureFormat::Bc2RgbaUnormSrgb,
                (Format::Dxt3, false) => TextureFormat::Bc2RgbaUnorm,
                (_, true) => TextureFormat::Bc3RgbaUnormSrgb,
                (_, false) => TextureFormat::Bc3RgbaUnorm,
            };
            (f, levels.concat())
        }
        other => {
            let mut out = Vec::new();
            for l in levels {
                out.extend(to_rgba8(other, l).ok_or("unsupported layout")?);
            }
            (if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm }, out)
        }
    };
    let mut img = checked_image(w, h, levels.len() as u32, format, data)?;
    img.sampler = sampler();
    Ok(img)
}

/// The largest side of a texture taken from a file. Above every texture the game ships
/// (4096 at most [observed]) and within what desktop GPUs take; a larger size in a file
/// is a broken header.
pub const MAX_TEXTURE_SIDE: u32 = 16384;

/// A 2D image of `w`×`h` with `mips` levels whose bytes are `data`, or why the GPU would
/// refuse it. Bevy closes the game on a texture the GPU refuses (a zero or oversized
/// side, more levels than the size allows), and the render thread panics when the data
/// is shorter than the size says; so every image made from a file's numbers is built
/// here (docs/design/broken-files.md).
pub fn checked_image(w: u32, h: u32, mips: u32, format: TextureFormat, data: Vec<u8>) -> Result<Image, String> {
    if w == 0 || h == 0 || w > MAX_TEXTURE_SIDE || h > MAX_TEXTURE_SIDE {
        return Err(format!("size {w}x{h} out of range (1 to {MAX_TEXTURE_SIDE})"));
    }
    let most = 32 - w.max(h).leading_zeros();
    if mips == 0 || mips > most {
        return Err(format!("{mips} mip levels for a {w}x{h} texture (at most {most})"));
    }
    let (bw, bh) = format.block_dimensions();
    let block = format.block_copy_size(None).ok_or("format has no single block size")? as usize;
    let expected: usize = (0..mips).map(|m| ((w >> m).max(1).div_ceil(bw) * (h >> m).max(1).div_ceil(bh)) as usize * block).sum();
    if data.len() != expected {
        return Err(format!("{} bytes of data for a {w}x{h} texture of {mips} levels ({expected} expected)", data.len()));
    }
    let mut img = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, data, format, RenderAssetUsages::RENDER_WORLD);
    img.texture_descriptor.mip_level_count = mips;
    img.texture_descriptor.label = Some("dds texture");
    Ok(img)
}

/// The GPU label of a game texture: its folder under `textures\` (`textures\clutter`),
/// so the GPU allocator's report (nvforever `stress_sample.rs`) tells what kinds of
/// texture it holds. One string per folder, made once.
fn folder_label(path: &str) -> &'static str {
    static LABELS: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let mut parts = path.split('\\');
    let folder = match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(_)) => format!("{a}\\{b}"),
        _ => "textures".to_string(),
    };
    let mut labels = LABELS.lock().expect("texture labels");
    labels.get_or_insert_with(HashMap::new).entry(folder).or_insert_with_key(|k| Box::leak(k.clone().into_boxed_str()))
}

/// The PC's own form of a texture a model names in a console's folder: the same path
/// with `\pc\` for `\xbox\`, when that file exists. [observed: the Pip-Boy models
/// (`pipboy3000\pipboyarm.nif` and the rest) name `textures\pipboy3000\xbox\english\
/// PipBoyArm01.dds`, whose buttons are labelled with the console's LT and RT, and
/// `pipboy3000\pc\english\pipboyarm01.dds` lies beside it, as do `ps3` and `ps3o`
/// forms; the owner's screenshots of the PC game show no such labels. hypothesis: the
/// PC game reads the `pc` folder's file; docs/spec-notes/pipboy.md §1]
pub fn pc_texture(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    let i = lower.find("\\xbox\\")?;
    let pc = format!("{}\\pc\\{}", &path[..i], &path[i + "\\xbox\\".len()..]);
    exists(&pc).then_some(pc)
}

/// The first-person form of a texture a model names: the same folder with `1stperson`
/// before the file name, when that file exists. [observed: `meshes\weapons\2handrifle\
/// laserrifle.nif`, the only laser rifle model, names `LaserRifle01.dds` (128x128), and
/// `1stpersonlaserrifle01.dds` (1024x1024) lies beside it; 413 of the 455 `1stperson`
/// textures in the archives, all weapons', have such a plain twin and no model names
/// them; the owner's screenshot of the game shows the gun's small print sharp in first
/// person. hypothesis: the game draws the first-person weapon with these;
/// docs/spec-notes/first-person-textures.md §3]
pub fn first_person_texture(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let path = fnv_vfs::texture_path(path);
    if is_first_person_texture(&path) {
        return None;
    }
    let file = path.rfind('\\').map_or(0, |i| i + 1);
    let first = format!("{}1stperson{}", &path[..file], &path[file..]);
    exists(&first).then_some(first)
}

/// Whether a texture's file name is in its first-person form (starts `1stperson`).
pub fn is_first_person_texture(path: &str) -> bool {
    let path = fnv_vfs::texture_path(path);
    let file = path.rfind('\\').map_or(0, |i| i + 1);
    path[file..].starts_with("1stperson")
}

impl TextureCache {
    /// Drop the textures nothing else holds (no live material uses them), after their
    /// grace time (`crate::release`); the number dropped. Keys: (path, sRGB, clamped).
    /// The textures held, for the stress census: an entry is idle when only the cache
    /// holds its image (it waits out its keep time).
    pub fn share(&self) -> crate::release::CacheShare {
        let mut s = crate::release::CacheShare::default();
        for h in self.map.values().chain(self.clamped.values()).flatten() {
            s.entries += 1;
            s.idle += usize::from(!crate::release::held_beyond(h, 1));
            s.images.insert(h.id());
        }
        s
    }

    /// Drop the textures nothing has used for `keep.secs` ([`KeepLimits`](crate::release::KeepLimits)):
    /// counted from when the model or part that last held one went unused, where `hints`
    /// says (release.rs [`ImageHints`](crate::release::ImageHints)).
    pub fn release(&mut self, idle: &mut crate::release::Unused<(String, bool, bool)>, hints: &crate::release::ImageHints, now: f64, keep: crate::release::KeepLimits) -> usize {
        use crate::release::held_beyond;
        let mut ids = HashMap::new();
        let entries: Vec<_> = [(false, &self.map), (true, &self.clamped)]
            .into_iter()
            .flat_map(|(clamp, map)| map.iter().filter_map(move |((p, srgb), h)| h.as_ref().map(|h| ((p.clone(), *srgb, clamp), h))))
            .map(|(k, h)| {
                ids.insert(k.clone(), h.id());
                (k, held_beyond(h, 1))
            })
            .collect();
        self.ahead.retain(|_, (_, at)| now - *at < AHEAD_KEEP_SECS);
        self.workers.entries.lock().expect("worker textures").retain(|_, w| w.stored_at.is_none_or(|t| now - t < WORKER_KEEP_SECS));
        let drop = idle.sweep_timed(now, entries, keep.secs, keep.max, |k| ids.get(k).and_then(|&id| hints.since(id)));
        for ((p, srgb, clamp), _) in &drop {
            (if *clamp { &mut self.clamped } else { &mut self.map }).remove(&(p.clone(), *srgb));
        }
        self.generation += u64::from(!drop.is_empty());
        drop.len()
    }

    /// Put the images worker threads decoded into the image store and the cache (a texture
    /// the main thread loaded meanwhile keeps its own; the worker's copy stays for the
    /// materials made with it); the number put in. `now`: real seconds.
    pub fn take_workers(&mut self, images: &mut Assets<Image>, now: f64) -> usize {
        let mut n = 0;
        let mut entries = self.workers.entries.lock().expect("worker textures");
        for (key, w) in entries.iter_mut() {
            let Some(image) = w.image.take() else { continue };
            if images.insert(w.handle.id(), image).is_ok() {
                self.map.entry(key.clone()).or_insert_with(|| Some(w.handle.clone()));
                n += 1;
            }
            w.stored_at = Some(now);
        }
        drop(entries);
        for (key, e) in self.workers.failed.lock().expect("worker textures").drain() {
            crate::guard::failed(&mut self.failures, "texture", format!("{}: {e}", key.0));
            self.map.entry(key).or_insert(None);
        }
        self.generation += u64::from(n > 0);
        n
    }

    /// The textures held now with their formats, for worker threads (`ModelCtx`
    /// keeps the copy while it is unchanged and in use).
    pub fn snapshot(&self, images: &Assets<Image>) -> TextureSnapshot {
        self.map
            .iter()
            .filter_map(|(k, h)| {
                let h = h.as_ref()?;
                Some((k.clone(), (h.clone(), images.get(h)?.texture_descriptor.format)))
            })
            .collect()
    }

    /// Changes to the cache so far ([`Self::snapshot`]).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The bytes of the texture file at `path` (resolved: [`resolved_path`]), read ahead on
    /// a worker thread, for the first load of it; kept unless it is loaded already.
    pub fn offer(&mut self, path: String, bytes: Vec<u8>, now: f64) {
        let loaded = [true, false].iter().any(|&srgb| self.map.contains_key(&(path.clone(), srgb)) || self.clamped.contains_key(&(path.clone(), srgb)));
        if !loaded {
            self.ahead.insert(path, (bytes, now));
        }
    }

    /// Textures loaded and held now (failures not counted).
    pub fn len(&self) -> usize {
        self.map.values().chain(self.clamped.values()).filter(|h| h.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Load (once) the texture at a NIF texture path (the PC's own form of it, if any:
    /// [`pc_texture`]).
    pub fn get(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.load(raw_path, srgb, false, data, images, formats)
    }

    /// Like [`Self::get`], a copy of the texture whose sampler clamps at its edges
    /// instead of repeating (the grass atlas, nvforever `grass.rs`).
    pub fn get_clamped(
        &mut self,
        raw_path: &str,
        srgb: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        self.load(raw_path, srgb, true, data, images, formats)
    }

    fn load(
        &mut self,
        raw_path: &str,
        srgb: bool,
        clamp: bool,
        data: &GameData,
        images: &mut Assets<Image>,
        formats: Option<&CompressedImageFormatSupport>,
    ) -> Option<Handle<Image>> {
        let path = resolved_path(raw_path, |p| data.vfs.exists(p));
        if let Some(h) = (if clamp { &self.clamped } else { &self.map }).get(&(path.clone(), srgb)) {
            return h.clone();
        }
        // Decoded on a worker thread: its handle (and its image, if still waiting).
        if !clamp {
            let mut entries = self.workers.entries.lock().expect("worker textures");
            if let Some(w) = entries.get_mut(&(path.clone(), srgb)) {
                if let Some(image) = w.image.take() {
                    let _ = images.insert(w.handle.id(), image);
                    w.stored_at = Some(0.0);
                }
                let h = w.handle.clone();
                drop(entries);
                self.map.insert((path, srgb), Some(h.clone()));
                self.generation += 1;
                return Some(h);
            }
        }
        let bc = formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC));
        let read = self.ahead.remove(&path).map(|(bytes, _)| bytes).or_else(|| data.read(&path));
        let result = match read {
            None => Err("not in any archive".to_string()),
            Some(bytes) => dds_image(&bytes, srgb, bc),
        };
        let handle = match result {
            Ok(mut img) => {
                img.texture_descriptor.label = Some(folder_label(&path));
                if clamp {
                    if let ImageSampler::Descriptor(d) = &mut img.sampler {
                        d.address_mode_u = ImageAddressMode::ClampToEdge;
                        d.address_mode_v = ImageAddressMode::ClampToEdge;
                    }
                }
                Some(images.add(img))
            }
            Err(e) => {
                crate::guard::failed(&mut self.failures, "texture", format!("{path}: {e}"));
                None
            }
        };
        (if clamp { &mut self.clamped } else { &mut self.map }).insert((path, srgb), handle.clone());
        self.generation += 1;
        handle
    }
}

#[cfg(test)]
mod tests {
    use super::first_person_texture;

    #[test]
    fn the_first_person_twin_is_used_when_it_exists() {
        let twin = r"textures\weapons\2handrifle\1stpersonlaserrifle01.dds";
        let exists = |p: &str| p == twin;
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\LaserRifle01.dds", exists).as_deref(), Some(twin));
        // A path as models write it, without the `textures` folder.
        assert_eq!(first_person_texture(r"weapons\2handrifle\LaserRifle01.dds", exists).as_deref(), Some(twin));
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\LaserRifle02.dds", exists), None);
        // Already a first-person texture: kept.
        assert_eq!(first_person_texture(r"textures\weapons\2handrifle\1stPersonLaserRifle01.dds", |_| true), None);
    }
}
