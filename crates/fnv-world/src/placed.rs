//! A placed reference and the base object it places.

use crate::fields::{global, num, vec3, winner, Fields};
use fnv_esm::{LoadOrder, Schema};

/// Record header flags of references [documented: xEdit wbDefinitionsFNV.pas REFR].
pub mod ref_flags {
    pub const DELETED: u32 = 0x20;
    pub const PERSISTENT: u32 = 0x400;
    pub const INITIALLY_DISABLED: u32 = 0x800;
    pub const VISIBLE_WHEN_DISTANT: u32 = 0x8000;
}

/// XSRF bit 1, "Imposter" [documented: xEdit wbDefinitionsFNV.pas REFR]. The other bits
/// of the stored value vary from reference to reference (0x0A080002, 0x30002, ...) with no
/// pattern [observed: the 301 references with XSRF in FalloutNV.esm].
const IMPOSTER: u32 = 0x2;

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub form_id: u32,
    pub sig: [u8; 4],
    pub flags: u32,
    pub base: Option<u32>,
    pub position: [f32; 3],
    /// Euler angles in radians, as stored (convention: [`crate::transform`]).
    pub rotation: [f32; 3],
    pub scale: f32,
    /// XESP: the reference whose enabled state this one follows, and whether it is the
    /// opposite of the parent's.
    pub enable_parent: Option<(u32, bool)>,
    /// ONAM "Open by Default" (doors) [documented: xEdit wbDefinitionsFNV.pas REFR].
    pub open_by_default: bool,
    /// XTEL present: a door that leads elsewhere (a teleport door) rather than one that
    /// opens in place [documented: xEdit wbDefinitionsFNV.pas REFR "Teleport Destination"].
    pub teleport: bool,
    /// XEMI "Emittance": a region (REGN) or light (LIGH) whose colour drives the
    /// reference's externally emitted light [documented: xEdit wbDefinitionsFNV.pas REFR;
    /// see crate::lighting::emittance].
    pub emittance: Option<u32>,
    /// XSRF "Special Rendering Flags" bit 1, Imposter: drawn while the player is in a
    /// region whose imposter list names it, at any distance, rather than with its cell
    /// [documented: xEdit wbDefinitionsFNV.pas REFR; GECK wiki "Reference"]; see
    /// [`crate::imposter`].
    pub imposter: bool,
    /// XPRM present: the reference is a primitive (a box, sphere or plane drawn only in
    /// the editor) rather than an object, a trigger volume when its base is an activator
    /// (docs/spec-notes/level-design.md §4.1). Such a reference shows nothing and offers
    /// nothing in game, whatever model its base names ([`Self::draws_base_model`]).
    pub primitive: bool,
}

impl Placement {
    pub fn deleted(&self) -> bool {
        self.flags & ref_flags::DELETED != 0
    }
    pub fn initially_disabled(&self) -> bool {
        self.flags & ref_flags::INITIALLY_DISABLED != 0
    }
    /// Whether the base object's model is drawn, collides and can be aimed at for this
    /// reference: not for a primitive. A trigger made by copying an activator keeps the
    /// copied model: 68 references with XPRM in the base game and DLC have a base with a
    /// model, all activators, 66 naming `WaterTowerPipes01.NIF` and 2 a vault control panel
    /// [observed: survey of the six ESMs], and primitives show only with marker viewing on [documented: GECK wiki
    /// Creating Primitives; ToggleAllPrimitives] (level-design.md §4.1).
    pub fn draws_base_model(&self) -> bool {
        !self.primitive
    }
}

/// The winning definition of a placed reference.
pub fn placement(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<Placement> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let base = f.formid(b"NAME", lo);
    let (position, rotation) = match f.get(b"DATA") {
        Some(d) => (
            d.get("position").and_then(vec3).unwrap_or_default(),
            d.get("rotation_radians").and_then(vec3).unwrap_or_default(),
        ),
        None => ([0.0; 3], [0.0; 3]),
    };
    let scale = f.get(b"XSCL").and_then(|v| v.as_f64()).map_or(1.0, |v| v as f32);
    let enable_parent = f.get(b"XESP").and_then(|v| {
        let parent = global(lo, pi, crate::fields::formid_of(v.get("reference")?)?)?;
        let opposite = v.get("flags").and_then(|f| num(f, "value")).is_some_and(|b| b as u32 & 1 != 0);
        Some((parent, opposite))
    });
    let open_by_default = f.get(b"ONAM").is_some();
    let teleport = f.raw(b"XTEL").is_some();
    let emittance = f.formid(b"XEMI", lo).filter(|&e| e != 0);
    let imposter = f.get(b"XSRF").and_then(|v| num(&v, "value")).is_some_and(|b| b as u32 & IMPOSTER != 0);
    let primitive = f.raw(b"XPRM").is_some();
    Some(Placement {
        form_id: fid,
        sig: r.header.sig,
        flags: r.header.flags,
        base,
        position,
        rotation,
        scale,
        enable_parent,
        open_by_default,
        teleport,
        emittance,
        imposter,
        primitive,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaseObject {
    pub form_id: u32,
    pub sig: [u8; 4],
    pub edid: Option<String>,
    /// MODL as stored: relative to `meshes\`.
    pub model: Option<String>,
    /// The item's own world model as stored, when its record names one: ARMO MOD2, WEAP
    /// MOD4 ([`world_model_sub`]).
    pub world_model: Option<String>,
    /// Alternate textures of the model [`Self::world_model_path`] names: MODS for MODL,
    /// MO2S for ARMO MOD2, MO4S for WEAP MOD4 (`crate::alt_textures`).
    pub world_alt_textures: Vec<crate::alt_textures::AltTexture>,
}

/// The subrecord naming the model an item shows lying in the world, for the record types
/// whose MODL is something else (docs/spec-notes/simulation.md §3 "Item models"):
/// - ARMO: MODL is the worn (biped) model, MOD2 the "Male world model" [documented: xEdit
///   `wbDefinitionsFNV.pas` ARMO], usually a `...GO.NIF` ground object; armour on the
///   ground shows MOD2 [observed in game, male player].
/// - WEAP: MODL is the model in the hand, MOD4 the "World Model" (MOD2 is the "Shell
///   Casing Model", MOD3 the "Scope Model") [documented: xEdit `wbDefinitionsFNV.pas`
///   WEAP]; where MOD4 differs from MODL it is a `...GO.NIF` or `...WorldObject.NIF`, and
///   eight fist weapons have it and no MODL [observed in the records]. That the game
///   draws it for a weapon on the ground is not captured [hypothesis].
///
/// Every other item type has MODL only [observed].
pub fn world_model_sub(sig: &[u8; 4]) -> Option<&'static [u8; 4]> {
    match sig {
        b"ARMO" => Some(b"MOD2"),
        b"WEAP" => Some(b"MOD4"),
        _ => None,
    }
}

/// Archive path of a model stored relative to `meshes\`, lower case: `meshes\...`, except
/// SpeedTree models (`.spt`, used by TREE), which live under `trees\` [observed: all ten
/// `.spt` files in the archives are in `trees\`, and TREE MODL values name them without a
/// folder].
pub fn archive_path(stored: &str) -> Option<String> {
    let m = stored.trim();
    if m.is_empty() {
        return None;
    }
    // TREE models are stored with a leading separator, e.g. "\WastelandShrub01.spt"
    // [observed]; the file is trees\wastelandshrub01.spt.
    let m = m.replace('/', "\\").to_ascii_lowercase();
    let m = m.trim_start_matches('\\');
    let root = if m.ends_with(".spt") { "trees\\" } else { "meshes\\" };
    Some(if m.starts_with(root) { m.to_string() } else { format!("{root}{m}") })
}

impl BaseObject {
    /// Archive path of MODL ([`archive_path`]).
    pub fn model_path(&self) -> Option<String> {
        archive_path(self.model.as_deref()?)
    }

    /// Archive path of the model the object shows (and collides with) in the world,
    /// placed or dropped: an item's world model when its record names one
    /// ([`world_model_sub`]), else MODL.
    pub fn world_model_path(&self) -> Option<String> {
        self.world_model.as_deref().and_then(archive_path).or_else(|| self.model_path())
    }

    pub fn is_speedtree(&self) -> bool {
        self.model_path().is_some_and(|m| m.ends_with(".spt"))
    }
}

pub fn base_object(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<BaseObject> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let world_sub = world_model_sub(&r.header.sig).filter(|s| f.string(s).as_deref().and_then(archive_path).is_some());
    let world_model = world_sub.and_then(|s| f.string(s));
    let world_alt_textures = crate::alt_textures::alt_textures(lo, schema, &f, world_sub.unwrap_or(b"MODL"));
    Some(BaseObject { form_id: fid, sig: r.header.sig, edid: f.string(b"EDID"), model: f.string(b"MODL"), world_model, world_alt_textures })
}

/// A TREE's billboard: its size (TREE BNAM width, height) and texture,
/// `textures\trees\billboards\<model file name>.dds` [observed: one billboard per `.spt`
/// with the same stem].
pub fn tree_billboard(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<([f32; 2], String)> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let b = f.get(b"BNAM")?;
    let size = [num(&b, "width")? as f32, num(&b, "height")? as f32];
    let model = f.string(b"MODL").as_deref().and_then(archive_path)?;
    let stem = model.rsplit('\\').next()?.strip_suffix(".spt")?;
    Some((size, format!("textures\\trees\\billboards\\{stem}.dds")))
}

/// What the renderer needs from a TREE record besides its `.spt` model
/// (docs/spec-notes/speedtree.md §6).
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRecord {
    /// `.spt` archive path ([`BaseObject::model_path`]).
    pub model: String,
    /// Combined leaf texture, `textures\trees\leaves\<ICON>` lower case [observed].
    pub leaf_texture: Option<String>,
    /// Size of the finished tree: the larger of its width and height, taken from the
    /// billboard size (BNAM; the billboard is a square side view fitted to the tree
    /// [observed: the opaque part of WastelandShrub01's billboard spans 0.89 of its
    /// width and 0.62 of its height, WhiteOak01's 0.61 and 0.99]), else OBND max z.
    pub size: Option<f32>,
    /// CNAM leaf and branch dimming [documented: xEdit field names; meaning
    /// speedtree.md §6, hypothesis].
    pub leaf_dimming: Option<f32>,
    pub branch_dimming: Option<f32>,
    /// CNAM rock and rustle speeds [documented: xEdit field names; use: tree.rs, by eye].
    pub rock_speed: Option<f32>,
    pub rustle_speed: Option<f32>,
}

pub fn tree_record(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<TreeRecord> {
    let (pi, r) = winner(lo, fid)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let model = f.string(b"MODL").as_deref().and_then(archive_path)?;
    let leaf_texture = f.string(b"ICON").map(|i| i.trim().replace('/', "\\").to_ascii_lowercase()).filter(|i| !i.is_empty());
    let leaf_texture = leaf_texture.map(|i| format!("textures\\trees\\leaves\\{}", i.trim_start_matches('\\')));
    let obnd = f.get(b"OBND").and_then(|b| num(b.get("max")?, "z")).map(|z| z as f32).filter(|&z| z > 0.0);
    let bnam = f.get(b"BNAM").and_then(|b| Some(num(&b, "width")?.max(num(&b, "height")?) as f32)).filter(|&s| s > 0.0);
    let cnam = f.get(b"CNAM");
    let dim = |k: &str| cnam.as_ref().and_then(|c| num(c, k)).map(|v| v as f32);
    Some(TreeRecord { model, leaf_texture, size: bnam.or(obnd), leaf_dimming: dim("leaf_dimming"), branch_dimming: dim("branch_dimming"),
        rock_speed: dim("rock_speed"), rustle_speed: dim("rustle_speed") })
}

/// DOOR FNAM flag bits [documented: xEdit wbDefinitionsFNV.pas DOOR; schemas/fnv
/// objects.json numbers them from bit 0].
pub mod door_flags {
    pub const AUTOMATIC: u8 = 0x02;
    pub const HIDDEN: u8 = 0x04;
    pub const MINIMAL_USE: u8 = 0x08;
    pub const SLIDING: u8 = 0x10;
}

/// What a DOOR record holds besides its model (docs/spec-notes/doors.md).
#[derive(Debug, Clone, PartialEq)]
pub struct DoorRecord {
    pub model: Option<String>,
    /// SNAM, ANAM, BNAM: open, close and looping sounds (SOUN).
    pub open_sound: Option<u32>,
    pub close_sound: Option<u32>,
    pub loop_sound: Option<u32>,
    /// FNAM ([`door_flags`]).
    pub flags: u8,
    /// OBND, in the model's space (min, max).
    pub bounds: Option<([f32; 3], [f32; 3])>,
}

pub fn door_record(lo: &LoadOrder, schema: &Schema, fid: u32) -> Option<DoorRecord> {
    let (pi, r) = winner(lo, fid)?;
    if &r.header.sig != b"DOOR" {
        return None;
    }
    let f = Fields::new(r, pi, schema).ok()?;
    let model = f.string(b"MODL").as_deref().and_then(archive_path);
    let sound = |s: &[u8; 4]| f.formid(s, lo).filter(|&v| v != 0);
    let flags = f.get(b"FNAM").and_then(|v| num(&v, "value")).map_or(0, |v| v as u8);
    let corner = |v: &serde_json::Value, k: &str| -> Option<[f32; 3]> {
        let c = v.get(k)?;
        Some([num(c, "x")? as f32, num(c, "y")? as f32, num(c, "z")? as f32])
    };
    let bounds = f.get(b"OBND").and_then(|b| Some((corner(&b, "min")?, corner(&b, "max")?)));
    Some(DoorRecord { model, open_sound: sound(b"SNAM"), close_sound: sound(b"ANAM"), loop_sound: sound(b"BNAM"), flags, bounds })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_paths() {
        let b = |m: &str| BaseObject { form_id: 1, sig: *b"STAT", edid: None, model: Some(m.into()), world_model: None, world_alt_textures: Vec::new() };
        assert_eq!(b("Clutter\\Junk\\Can.NIF").model_path().unwrap(), "meshes\\clutter\\junk\\can.nif");
        assert_eq!(b("meshes/a.nif").model_path().unwrap(), "meshes\\a.nif");
        assert_eq!(b("  ").model_path(), None);
        assert_eq!(b("WastelandShrub01.spt").model_path().unwrap(), "trees\\wastelandshrub01.spt");
        assert_eq!(b("\\WastelandShrub01.spt").model_path().unwrap(), "trees\\wastelandshrub01.spt");
        assert!(b("WastelandShrub01.spt").is_speedtree() && !b("a.nif").is_speedtree());
    }

    #[test]
    fn primitives_draw_no_model() {
        // level-design.md §4.1: a trigger copied from an activator with a model.
        let p = |primitive| Placement {
            form_id: 1,
            sig: *b"REFR",
            flags: 0,
            base: Some(2),
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: 1.0,
            enable_parent: None,
            open_by_default: false,
            teleport: false,
            emittance: None,
            imposter: false,
            primitive,
        };
        assert!(p(false).draws_base_model());
        assert!(!p(true).draws_base_model());
    }

    #[test]
    fn world_model_paths() {
        let b = |sig: &[u8; 4], model: Option<&str>, world: Option<&str>| BaseObject {
            form_id: 1,
            sig: *sig,
            edid: None,
            model: model.map(Into::into),
            world_model: world.map(Into::into),
            world_alt_textures: Vec::new(),
        };
        // Armour: the ground object, not the worn suit; MODL stays what `model_path` gives.
        let leather = b(b"ARMO", Some("Armor\\LeatherArmor\\M\\OutfitM.NIF"), Some("Armor\\LeatherArmor\\M\\GO.NIF"));
        assert_eq!(leather.world_model_path().unwrap(), "meshes\\armor\\leatherarmor\\m\\go.nif");
        assert_eq!(leather.model_path().unwrap(), "meshes\\armor\\leatherarmor\\m\\outfitm.nif");
        // A world model and no MODL (fist weapons).
        let knuckles = b(b"WEAP", None, Some("Weapons\\Hand2Hand\\BrassKnucklesWorldObject.NIF"));
        assert_eq!(knuckles.world_model_path().unwrap(), "meshes\\weapons\\hand2hand\\brassknucklesworldobject.nif");
        assert_eq!(knuckles.model_path(), None);
        // No world model, or an empty one: MODL.
        assert_eq!(b(b"WEAP", Some("Weapons\\2HandMelee\\BaseballBat.NIF"), None).world_model_path().unwrap(), "meshes\\weapons\\2handmelee\\baseballbat.nif");
        assert_eq!(b(b"ARMO", Some("a.nif"), Some(" ")).world_model_path().unwrap(), "meshes\\a.nif");
        assert_eq!(b(b"MISC", None, None).world_model_path(), None);
        assert_eq!((world_model_sub(b"ARMO"), world_model_sub(b"WEAP"), world_model_sub(b"MISC")), (Some(b"MOD2"), Some(b"MOD4"), None));
    }
}
