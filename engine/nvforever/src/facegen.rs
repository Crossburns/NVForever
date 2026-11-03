//! FaceGen in the game (workstream 5 part C, docs/spec-notes/actors.md §4): an NPC's
//! FGGS/FGGA coefficients plus its race's morph every part that has an `.egm` (beside its
//! model: head, eyes, mouth, beards, hats, …; or one per shape: the hair models), and its
//! FGTS coefficients plus its race's add the `.egt` colour offset to the face and skin
//! textures. The combination rules are [hypothesis] (fnv_actor::facegen,
//! docs/spec-notes/actors.md §4.2, §4.3).
//!
//! The tint is added in the material's shader (nvf-assets `nif_material.rs`, "FaceGen
//! tint"): per NPC only the `.egt` offset image is made (the face's 256 × 256, the body's
//! and hands' 32 × 32) and given to its face and skin materials; the shader adds it to
//! the diffuse texture as `fnv_actor::facegen::add_offset` adds it to the texture's bytes.
//! Making a tinted copy of each texture per NPC on the CPU was most of the hitch when NPCs
//! came into view [observed: CPU profile of the towns tour, 2026-10-22,
//! docs/design/streaming-hitches.md]; an offset image takes well under a millisecond.
//! Offset images are reused by what they are made of while anything draws them.

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use fnv_actor::facegen::{Egm, Egt};
use fnv_actor::records::FaceGen;
use nvf_assets::actor::{ActorCtx, PreparedPart};
use nvf_assets::{GameData, NifMaterial};
use std::collections::HashMap;
use std::sync::Arc;

/// What an offset image is made of: the `.egt` (archive path) and the texture
/// coefficients' bits.
#[derive(Clone, PartialEq, Eq, Hash)]
struct OffsetKey {
    egt: String,
    fgts: Vec<u64>,
}

/// Parsed FaceGen files and the offset images made from them; `None` remembers that a
/// file is absent or unreadable.
#[derive(Default)]
pub struct FaceGenCache {
    egm: HashMap<String, Option<Arc<Egm>>>,
    egt: HashMap<String, Option<Arc<Egt>>>,
    /// Offset images made, while something holds them.
    made: HashMap<OffsetKey, AssetId<Image>>,
    pub morphed_shapes: usize,
    /// Face offset images made.
    pub faces_tinted: usize,
    /// Body and hand skin offset images made (the race's body and hand `.egt`).
    pub skins_tinted: usize,
    /// Materials given an offset image.
    pub shapes_tinted: usize,
    /// Time spent making offset images (on the main thread).
    pub offset_time: std::time::Duration,
    /// Time spent reading and parsing `.egt` files (on the main thread).
    pub egt_time: std::time::Duration,
    /// EGMs whose vertex count does not cover the shape they sit beside.
    pub mismatched: usize,
}

/// `meshes\a\b.nif` → `meshes\a\b.<ext>`.
pub fn sibling(model: &str, ext: &str) -> String {
    match model.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.{ext}"),
        None => format!("{model}.{ext}"),
    }
}

/// The FaceGen file of one shape of a model: `meshes\a\hairbun.nif`, shape `NoHat` →
/// `meshes\a\hairbunnohat.<ext>` [observed: the hair models, actors.md §4.3].
pub fn shape_sibling(model: &str, shape: &str, ext: &str) -> String {
    let stem = model.rsplit_once('.').map_or(model, |(stem, _)| stem);
    format!("{stem}{}.{ext}", shape.to_ascii_lowercase())
}

/// A FaceGen file read and parsed, for [`FaceGenCache::offer`].
pub enum FaceGenFile {
    Egm(Option<Arc<Egm>>),
    Egt(Option<Arc<Egt>>),
}

/// Read and parse the `.egm` at `path`; `None` when it is absent or unreadable.
fn read_egm(data: &GameData, path: &str) -> Option<Arc<Egm>> {
    read_parsed(data, path, |b| Egm::parse(b).map_err(|e| e.to_string()))
}

/// Read and parse the `.egt` at `path`; `None` when it is absent or unreadable.
fn read_egt(data: &GameData, path: &str) -> Option<Arc<Egt>> {
    read_parsed(data, path, |b| Egt::parse(b).map_err(|e| e.to_string()))
}

/// The file at `path` parsed by `parse`, under a guard: a broken file (an error or a
/// panic) is logged and counts as absent. The caches keep the result per path, so each
/// is logged once.
fn read_parsed<T>(data: &GameData, path: &str, parse: impl FnOnce(&[u8]) -> Result<T, String>) -> Option<Arc<T>> {
    let b = data.vfs.exists(path).then(|| data.read(path)).flatten()?;
    match nvf_assets::guard::guarded(|| parse(&b)) {
        Ok(v) => Some(Arc::new(v)),
        Err(e) => {
            warn!("facegen file {path} not used: {e}");
            None
        }
    }
}

/// Read and parse the FaceGen file at `path` (an `.egm` or `.egt`), on a worker thread
/// (`readahead.rs`).
pub fn read_file(data: &GameData, path: &str) -> FaceGenFile {
    if path.ends_with(".egt") {
        FaceGenFile::Egt(read_egt(data, path))
    } else {
        FaceGenFile::Egm(read_egm(data, path))
    }
}

impl FaceGenCache {
    pub(crate) fn egm(&mut self, data: &GameData, path: &str) -> Option<Arc<Egm>> {
        self.egm.entry(path.to_string()).or_insert_with(|| read_egm(data, path)).clone()
    }

    /// Whether the FaceGen file at `path` has been read (or found absent).
    pub fn has(&self, path: &str) -> bool {
        self.egm.contains_key(path) || self.egt.contains_key(path)
    }

    /// `morph` tries shape-specific files only after the whole model's EGM is known
    /// absent [design: `morph`]. Read-ahead discovers those names from prepared parts.
    pub fn needs_shape_egms(&self, model: &str) -> bool {
        matches!(self.egm.get(&sibling(model, "egm")), Some(None))
    }

    /// A FaceGen file read ahead on a worker thread ([`read_file`]); kept unless it has
    /// been read here meanwhile.
    pub fn offer(&mut self, path: String, file: FaceGenFile) {
        match file {
            FaceGenFile::Egm(f) => {
                self.egm.entry(path).or_insert(f);
            }
            FaceGenFile::Egt(f) => {
                self.egt.entry(path).or_insert(f);
            }
        }
    }

    /// Per shape of `part` (model `model`), a mesh morphed by `fg`'s FGGS and FGGA
    /// (`Appearance::shape_facegen`) where an `.egm` covers the shape's vertices. With an
    /// `.egm` beside the model: a one-shape part, or a shape with exactly the EGM's
    /// vertex count. Without one: a shape with an `.egm` of its own ([`shape_sibling`])
    /// of exactly its vertex count.
    pub fn morph(&mut self, ctx: &mut ActorCtx, part: &PreparedPart, model: &str, fg: &FaceGen) -> Vec<Option<Handle<Mesh>>> {
        let mut out = vec![None; part.shapes.len()];
        let whole = self.egm(&ctx.models.data, &sibling(model, "egm"));
        for (i, s) in part.shapes.iter().enumerate() {
            let n = s.cpu.count_vertices();
            let (egm, fits) = match (&whole, s.name.as_deref()) {
                (Some(egm), _) => (egm.clone(), n <= egm.vertices && (part.shapes.len() == 1 || n == egm.vertices)),
                (None, Some(name)) => match self.egm(&ctx.models.data, &shape_sibling(model, name, "egm")) {
                    Some(egm) => (egm.clone(), n == egm.vertices),
                    None => continue,
                },
                (None, None) => continue,
            };
            if !fits {
                self.mismatched += 1;
                continue;
            }
            let mut mesh = (*s.cpu).clone();
            let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) else { continue };
            egm.apply(p, &fg.fggs, &fg.fgga);
            out[i] = Some(ctx.models.meshes.add(mesh));
            self.morphed_shapes += 1;
        }
        out
    }

    fn egt(&mut self, data: &GameData, path: &str) -> Option<Arc<Egt>> {
        let time = &mut self.egt_time;
        self.egt
            .entry(path.to_string())
            .or_insert_with(|| {
                let start = std::time::Instant::now();
                let egt = read_egt(data, path);
                *time += start.elapsed();
                egt
            })
            .clone()
    }

    /// The face offset of an NPC: the `.egt` beside `model` for the texture coefficients
    /// `fgts` (`Appearance::skin_fgts`), added to the head's texture in the shader. Put on
    /// a material with [`Self::tinted`].
    pub fn face_offset(&mut self, ctx: &mut ActorCtx, model: &str, fgts: &[f64]) -> Option<Handle<Image>> {
        self.offset(ctx, false, sibling(model, "egt"), fgts)
    }

    /// An NPC's body skin offset: the race's "Upper Body Texture" `.egt` (`egt`, archive
    /// path) for the texture coefficients `fgts`, combined as for the face [hypothesis,
    /// actors.md §4.1]; the hands' likewise.
    pub fn skin_offset(&mut self, ctx: &mut ActorCtx, egt: &str, fgts: &[f64]) -> Option<Handle<Image>> {
        self.offset(ctx, true, egt.to_string(), fgts)
    }

    /// The offset image of `egt` for `fgts`: the one made already, else a new one.
    /// `None`: the `.egt` is missing or unreadable.
    fn offset(&mut self, ctx: &mut ActorCtx, skin: bool, egt: String, fgts: &[f64]) -> Option<Handle<Image>> {
        let key = OffsetKey { egt, fgts: fgts.iter().map(|v| v.to_bits()).collect() };
        if let Some(image) = self.made.get(&key).and_then(|id| ctx.models.images.get_strong_handle(*id)) {
            return Some(image);
        }
        let file = self.egt(&ctx.models.data, &key.egt)?;
        let start = std::time::Instant::now();
        let image = offset_image(file.columns, file.rows, &offset_planes(&file, fgts));
        self.offset_time += start.elapsed();
        let image = match image {
            Ok(i) => i,
            Err(e) => {
                warn!("facegen offset of {} not used: {e}", key.egt);
                return None;
            }
        };
        let images = &mut ctx.models.images;
        let image = images.add(image);
        self.made.retain(|_, id| images.contains(*id));
        self.made.insert(key, image.id());
        *(if skin { &mut self.skins_tinted } else { &mut self.faces_tinted }) += 1;
        Some(image)
    }

    /// A copy of `material` whose shader adds the FaceGen offset `offset` to its texture.
    pub fn tinted(&mut self, materials: &mut Assets<NifMaterial>, material: &Handle<NifMaterial>, offset: &Handle<Image>) -> Option<Handle<NifMaterial>> {
        let mut m = materials.get(material)?.clone();
        m.extension.facegen_offset = Some(offset.clone());
        self.shapes_tinted += 1;
        Some(materials.add(m))
    }
}

/// `Egt::offset` for the symmetric coefficients `fgts`, one plane per colour: the same
/// sums, in an order the compiler can vectorise (a 256 × 256 face file has 50 modes).
fn offset_planes(egt: &Egt, fgts: &[f64]) -> [Vec<f32>; 3] {
    let n = egt.rows * egt.columns;
    let mut planes = [vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]];
    for (m, (scale, img)) in egt.modes.iter().enumerate().take(egt.symmetric) {
        let w = fgts.get(m).copied().unwrap_or(0.0) as f32 * scale;
        if w == 0.0 {
            continue;
        }
        for (plane, channel) in planes.iter_mut().zip(img) {
            for (o, &v) in plane.iter_mut().zip(channel) {
                *o += w * f32::from(v);
            }
        }
    }
    planes
}

/// The texture holding a FaceGen offset (`rows` × `columns` colour offsets per plane, the
/// file's first row first) for the shader, which reads it unfiltered: 16-bit floats,
/// which keep an offset of the observed −60…+20 units (actors.md §4) to 1/32 of a unit.
/// `Err`: sizes the GPU would refuse (a broken file).
fn offset_image(columns: usize, rows: usize, planes: &[Vec<f32>; 3]) -> Result<Image, String> {
    let n = rows * columns;
    let mut data = Vec::with_capacity(n * 8);
    for ((&r, &g), &b) in planes[0].iter().zip(&planes[1]).zip(&planes[2]).take(n) {
        for v in [r, g, b, 0.0] {
            data.extend_from_slice(&half::f16::from_f32(v).to_le_bytes());
        }
    }
    let side = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    nvf_assets::texture::checked_image(side(columns), side(rows), 1, TextureFormat::Rgba16Float, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_whole_egm_discovers_shape_files_and_missing_files_finish_readiness() {
        let mut cache = FaceGenCache::default();
        assert!(!cache.needs_shape_egms("hair.nif"));
        cache.offer("hair.egm".into(), FaceGenFile::Egm(None));
        assert!(cache.has("hair.egm"));
        assert!(cache.needs_shape_egms("hair.nif"));
        let shape = shape_sibling("hair.nif", "NoHat", "egm");
        assert!(!cache.has(&shape));
        cache.offer(shape.clone(), FaceGenFile::Egm(None));
        assert!(cache.has(&shape));
        cache.offer("head.egt".into(), FaceGenFile::Egt(None));
        assert!(cache.has("head.egt"));
    }

    #[test]
    fn siblings() {
        assert_eq!(sibling(r"meshes\characters\head\headhuman.nif", "egm"), r"meshes\characters\head\headhuman.egm");
        assert_eq!(sibling("x", "egt"), "x.egt");
        assert_eq!(shape_sibling(r"meshes\characters\hair\hairbun.nif", "NoHat", "egm"), r"meshes\characters\hair\hairbunnohat.egm");
        assert_eq!(shape_sibling("x", "Hat", "egm"), "xhat.egm");
    }

    /// CPU mirror of the shader's `facegen_offset_at` (nif_material.wesl).
    fn offset_at(offset: &[[f32; 3]], rows: usize, columns: usize, u: f32, v: f32) -> [f32; 3] {
        let (u, v) = (u.rem_euclid(1.0), v.rem_euclid(1.0));
        let x = (u * columns as f32 - 0.5).clamp(0.0, (columns - 1) as f32);
        let y = ((1.0 - v) * rows as f32 - 0.5).clamp(0.0, (rows - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(columns - 1), (y0 + 1).min(rows - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |xx: usize, yy: usize| offset[yy * columns + xx];
        std::array::from_fn(|k| {
            let top = at(x0, y0)[k] + (at(x1, y0)[k] - at(x0, y0)[k]) * fx;
            let bottom = at(x0, y1)[k] + (at(x1, y1)[k] - at(x0, y1)[k]) * fx;
            top + (bottom - top) * fy
        })
    }

    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn linear_to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    /// CPU mirror of the shader's `facegen_tint`: a texel as sampled (linear) plus an
    /// offset in stored units, held to 0..255, linear again.
    fn tint_texel(texel: f32, offset: f32) -> f32 {
        let stored = linear_to_srgb(texel.clamp(0.0, 1.0)) * 255.0;
        srgb_to_linear((stored + offset).clamp(0.0, 255.0) / 255.0)
    }

    /// The shader's add, read at each texel's centre, gives what `add_offset` stores in
    /// that texel, to its rounding: same flip, same upsampling, same hold to 0..255.
    #[test]
    fn shader_add_matches_the_cpu_add() {
        let (w, h, rows, columns) = (64usize, 48usize, 8usize, 6usize);
        let px: Vec<u8> = (0..w * h * 4).map(|i| ((i * 37 + i / 7) % 256) as u8).collect();
        // Up to about ±90 units, rows and columns different, so a flip or a transpose shows.
        let offset: Vec<[f32; 3]> = (0..rows * columns)
            .map(|i| {
                let (r, c) = ((i / columns) as f32, (i % columns) as f32);
                [r * 12.0 - 40.0, c * 15.0 - 30.0, (r - c) * 9.0]
            })
            .collect();
        let mut cpu = px.clone();
        fnv_actor::facegen::add_offset(&mut cpu, w, h, &offset, rows, columns);
        let mut worst = 0.0f32;
        for y in 0..h {
            for x in 0..w {
                let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                let o = offset_at(&offset, rows, columns, u, v);
                for k in 0..3 {
                    let texel = srgb_to_linear(f32::from(px[(y * w + x) * 4 + k]) / 255.0);
                    let shader = linear_to_srgb(tint_texel(texel, o[k])) * 255.0;
                    worst = worst.max((shader - f32::from(cpu[(y * w + x) * 4 + k])).abs());
                }
            }
        }
        assert!(worst <= 0.5 + 1e-3, "worst difference {worst} units");
    }

    /// The planes hold `Egt::offset`'s sums.
    #[test]
    fn planes_match_egt_offset() {
        let n = 12;
        let image = |seed: i32| -> [Vec<i8>; 3] { std::array::from_fn(|k| (0..n).map(|i| ((i * 7 + seed * 13 + k as i32 * 29) % 255 - 127) as i8).collect()) };
        let egt = Egt { rows: 3, columns: 4, symmetric: 3, asymmetric: 0, basis_version: 0, modes: vec![(0.5, image(1)), (0.25, image(2)), (1.5, image(3))] };
        let fgts = [1.0, -2.0, 0.75];
        let planes = offset_planes(&egt, &fgts);
        for (i, o) in egt.offset(&fgts, &[]).iter().enumerate() {
            for k in 0..3 {
                assert!((planes[k][i] - o[k]).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn offset_texture_layout() {
        use bevy::render::render_resource::Extent3d;
        let img = offset_image(2, 3, &[vec![1.0; 6], vec![-2.0; 6], vec![0.5; 6]]).unwrap();
        assert_eq!(img.texture_descriptor.size, Extent3d { width: 2, height: 3, depth_or_array_layers: 1 });
        let data = img.data.as_ref().unwrap();
        assert_eq!(data.len(), 2 * 3 * 8);
        assert_eq!(half::f16::from_le_bytes([data[2], data[3]]).to_f32(), -2.0);
    }

    /// A broken file's sizes give no texture rather than one the GPU refuses.
    #[test]
    fn offset_texture_of_no_size_is_refused() {
        assert!(offset_image(0, 3, &[vec![], vec![], vec![]]).is_err());
        assert!(offset_image(2, 3, &[vec![1.0; 2], vec![1.0; 2], vec![1.0; 2]]).is_err());
    }
}
