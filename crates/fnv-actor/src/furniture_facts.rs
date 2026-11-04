//! What the assets say about who can use which furniture (docs/spec-notes/furniture.md
//! §9): the answers to [`FurnitureFacts`], built once for a run, the same way for the game
//! and for `fnvtool ai run`.
//!
//! The rule for clips is interim [design]. An actor's clips are the ones under the folder
//! of its skeleton. A folder has clips for furniture when the furniture part of the IDLE
//! tree names one there, or when a `.kf` there has a chair or bed in its file name. A file
//! list lacking a name is weak proof, and it says nothing of what the original does: the
//! question is open (docs/handoff/observer-checks.md, issue #1522). A skeleton that cannot be
//! found leaves the answer unknown.

use crate::anims::{actor_folder, file_name, parent, SkeletonIndex};
use crate::furniture::{markers, Furniture, Marker, UseIdles};
use crate::idle::meshes_path;
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_sim::furniture_facts::{Fit, FurnitureFacts};
use fnv_sim::FormId;
use rayon::prelude::*;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Seat,
    Bed,
    /// Neither flag (a work spot, a dance marker).
    Other,
}

/// What a skeleton folder has clips for.
#[derive(Debug, Default, Clone, Copy)]
struct Clips {
    tree: bool,
    seat: bool,
    bed: bool,
}

/// Words in a `.kf` file name that say what the clip is used on. Not `sit` or `sleep`:
/// those are also idles on the ground [observed: `creatures\dog\idleanims\specialidle_sit.kf`
/// and `creatures\radscorpion\idleanims\specialidle_sleep.kf`, whose IDLE conditions ask
/// nothing about furniture].
const SEAT_WORDS: [&str; 3] = ["chair", "stool", "bench"];
const BED_WORDS: [&str; 1] = ["bed"];

/// The facts of a run.
#[derive(Debug, Default)]
pub struct AssetFacts {
    marker_ids: HashMap<FormId, Vec<u8>>,
    kinds: HashMap<FormId, Kind>,
    skeletons: SkeletonIndex,
    folders: HashMap<String, Clips>,
}

/// The markers of a model's bytes, `None` when it does not parse.
pub fn read_markers(bytes: &[u8], nif: &fnv_nif::Schema) -> Option<Vec<Marker>> {
    let file = NifFile::parse(bytes, Some(nif)).ok()?;
    let dec = Decoded::new(&file, bytes, nif);
    Some(markers(&file, &dec))
}

impl AssetFacts {
    /// `paths`: every path of the VFS that may matter (`.kf` and `.nif`); `read` gives a
    /// file's bytes. A model that does not read, or panics while it is read, is left out:
    /// its markers stay unknown.
    pub fn build<'a>(paths: impl IntoIterator<Item = &'a str>, tree: &'a UseIdles, bases: impl IntoIterator<Item = &'a Furniture>, read: impl Fn(&str) -> Option<Vec<u8>> + Sync, nif: &fnv_nif::Schema) -> AssetFacts {
        let bases: Vec<&Furniture> = bases.into_iter().collect();
        let models: BTreeSet<&str> = bases.iter().filter_map(|b| b.model.as_deref()).collect();
        let models: Vec<&str> = models.into_iter().collect();
        let read = &read;
        let read_model = move |m: &&str| {
            let one = std::panic::AssertUnwindSafe(|| read_markers(&read(m)?, nif));
            Some((m.to_string(), std::panic::catch_unwind(one).ok().flatten()?))
        };
        let markers_read: HashMap<String, Vec<Marker>> = models.par_iter().filter_map(read_model).collect();
        Self::assemble(paths, tree.clips(), bases, &markers_read)
    }

    /// [`AssetFacts::build`] with the models' markers already read, by model path.
    pub fn assemble<'a>(paths: impl IntoIterator<Item = &'a str>, tree_clips: impl IntoIterator<Item = &'a str>, bases: impl IntoIterator<Item = &'a Furniture>, models: &HashMap<String, Vec<Marker>>) -> AssetFacts {
        let paths: Vec<&str> = paths.into_iter().collect();
        let skeletons = SkeletonIndex::from_paths(paths.iter().copied());
        let mut folders: HashMap<String, Clips> = HashMap::new();
        for clip in tree_clips {
            folders.entry(skeletons.skeleton_for(clip).folder).or_default().tree = true;
        }
        for p in paths.iter().filter(|p| p.ends_with(".kf")) {
            let name = file_name(p);
            let (seat, bed) = (SEAT_WORDS.iter().any(|w| name.contains(w)), BED_WORDS.iter().any(|w| name.contains(w)));
            if seat || bed {
                let e = folders.entry(skeletons.skeleton_for(p).folder).or_default();
                e.seat |= seat;
                e.bed |= bed;
            }
        }
        let (mut marker_ids, mut kinds) = (HashMap::new(), HashMap::new());
        for b in bases {
            kinds.insert(b.id, if b.sleep() { Kind::Bed } else if b.sit() { Kind::Seat } else { Kind::Other });
            if let Some(m) = b.model.as_ref().and_then(|m| models.get(m)) {
                let enabled = m.iter().enumerate().filter(|(i, _)| b.marker_enabled(*i)).map(|(_, m)| m.ref_1).collect();
                marker_ids.insert(b.id, enabled);
            }
        }
        AssetFacts { marker_ids, kinds, skeletons, folders }
    }

    /// For the run's log: furniture bases, of those with their markers read, and the
    /// skeleton folders with furniture clips.
    pub fn summary(&self) -> (usize, usize, usize) {
        (self.kinds.len(), self.marker_ids.len(), self.folders.values().filter(|c| c.tree || c.seat || c.bed).count())
    }
}

impl FurnitureFacts for AssetFacts {
    fn marker_ids(&self, furniture_base: FormId) -> Option<&[u8]> {
        self.marker_ids.get(&furniture_base).map(Vec::as_slice)
    }

    fn fit(&self, skeleton: &str, furniture_base: FormId) -> Fit {
        let path = meshes_path(skeleton.trim());
        let (Some(kind), Some(folder)) = (self.kinds.get(&furniture_base), parent(&path)) else { return Fit::Unknown };
        if !self.skeletons.skeletons_in(folder).iter().any(|s| s == file_name(&path)) {
            return Fit::Unknown;
        }
        let clips = self.folders.get(folder).copied().unwrap_or_default();
        let has = clips.tree
            || match kind {
                Kind::Seat => clips.seat,
                Kind::Bed => clips.bed,
                Kind::Other => false,
            };
        if has {
            Fit::Supported
        } else {
            Fit::Unsupported
        }
    }
}

/// The last check before a clip is bound to an actor's skeleton: the body tracks of `clip`
/// that have no node, when the clip is from another actor's folder. A clip from the
/// skeleton's own folder fits whatever it names, and a clip from elsewhere whose body
/// tracks all find a node may fit: a folder alone is not proof, and a share of matching
/// names would be a number nobody observed [design]. A body track is one named `Bip01` or
/// `Bip01 …` (animation.md); other names (`Weapon`, `##Latch`) can be missing on anyone.
/// `has_node` counts every node of the skeleton, the root too.
pub fn missing_body_nodes<'a>(skeleton: &str, clip: &str, tracks: impl IntoIterator<Item = &'a str>, has_node: impl Fn(&str) -> bool) -> Vec<&'a str> {
    if actor_folder(&meshes_path(skeleton.trim())) == actor_folder(&meshes_path(clip)) {
        return Vec::new();
    }
    tracks.into_iter().filter(|t| (*t == "Bip01" || t.starts_with("Bip01 ")) && !has_node(t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUMAN: &str = r"Characters\_Male\Skeleton.nif";
    const HOUND: &str = r"Creatures\Hound\Skeleton.nif";
    const PARROT: &str = r"Creatures\Parrot\Skeleton.nif";
    const SEAT: FormId = FormId(0x10);
    const BED: FormId = FormId(0x11);
    const SPOT: FormId = FormId(0x12);
    const CROSS: FormId = FormId(0x13);
    const UNREAD: FormId = FormId(0x14);

    fn base(id: FormId, model: &str, flags: u32) -> Furniture {
        Furniture { id, edid: None, name: None, model: Some(model.to_string()), marker_flags: Some(flags), has_script: false }
    }

    fn marker(id: u8) -> Marker {
        Marker { offset: [0.0; 3], orientation: 0, ref_1: id, ref_2: id }
    }

    /// Synthetic: a tree naming a clip under `_male`; a hound with idles and no furniture
    /// clip; a parrot with a clip on a chair and none on a bed.
    fn facts() -> AssetFacts {
        let paths = [
            r"meshes\characters\_male\skeleton.nif",
            r"meshes\characters\_male\idleanims\downwego.kf",
            r"meshes\creatures\hound\skeleton.nif",
            r"meshes\creatures\hound\idleanims\specialidle_sit.kf",
            r"meshes\creatures\parrot\skeleton.nif",
            r"meshes\creatures\parrot\idleanims\chairperch.kf",
        ];
        let bases = [base(SEAT, "meshes\\seat.nif", 0x4000_0003), base(BED, "meshes\\bed.nif", 0x8000_0001), base(SPOT, "meshes\\spot.nif", 1), base(CROSS, "meshes\\cross.nif", 0x4000_0001), base(UNREAD, "meshes\\gone.nif", 0x4000_0001)];
        let models = HashMap::from([
            // The third marker is off: the base enables two.
            ("meshes\\seat.nif".to_string(), vec![marker(11), marker(12), marker(10)]),
            ("meshes\\bed.nif".to_string(), vec![marker(1)]),
            ("meshes\\spot.nif".to_string(), vec![marker(22)]),
            ("meshes\\cross.nif".to_string(), vec![marker(10)]),
        ]);
        AssetFacts::assemble(paths, [r"meshes\characters\_male\idleanims\downwego.kf"], &bases, &models)
    }

    #[test]
    fn people_have_the_trees_clips_for_everything() {
        let f = facts();
        for b in [SEAT, BED, SPOT, CROSS] {
            assert_eq!(f.fit(HUMAN, b), Fit::Supported);
        }
    }

    #[test]
    fn a_creature_without_furniture_clips_is_unsupported_and_an_idle_on_the_ground_does_not_count() {
        let f = facts();
        for b in [SEAT, BED, SPOT, CROSS] {
            assert_eq!(f.fit(HOUND, b), Fit::Unsupported);
        }
    }

    #[test]
    fn a_creature_with_its_own_clip_is_supported_for_that_kind_only() {
        let f = facts();
        assert_eq!(f.fit(PARROT, SEAT), Fit::Supported);
        assert_eq!(f.fit(PARROT, BED), Fit::Unsupported);
        assert_eq!(f.fit(PARROT, SPOT), Fit::Unsupported);
    }

    #[test]
    fn what_cannot_be_resolved_is_unknown() {
        let f = facts();
        // Not resolved; a placeholder model; a folder with no such skeleton; a form that
        // is not furniture.
        assert_eq!(f.fit("", SEAT), Fit::Unknown);
        assert_eq!(f.fit("marker_placeholder.nif", SEAT), Fit::Unknown);
        assert_eq!(f.fit(r"Creatures\Nobody\Skeleton.nif", SEAT), Fit::Unknown);
        assert_eq!(f.fit(HUMAN, FormId(0x99)), Fit::Unknown);
        // A run without any assets answers the same.
        assert_eq!(AssetFacts::default().fit(HUMAN, SEAT), Fit::Unknown);
    }

    #[test]
    fn marker_ids_are_the_enabled_ones_and_unknown_when_the_model_was_not_read() {
        let f = facts();
        assert_eq!(f.marker_ids(SEAT), Some(&[11, 12][..]));
        assert_eq!(f.marker_ids(CROSS), Some(&[10][..]));
        assert_eq!(f.marker_ids(UNREAD), None);
        assert_eq!(f.summary(), (5, 4, 2));
    }

    #[test]
    fn a_clip_from_elsewhere_is_refused_only_for_body_tracks_without_a_node() {
        let human_clip = r"meshes\characters\_male\idleanims\downwego.kf";
        let hound = ["Bip01", "Bip01 Pelvis"];
        let has = |n: &str| hound.contains(&n);
        let tracks = ["Bip01", "Bip01 Pelvis", "Bip01 L Thumb3", "Weapon", "##Latch"];
        // The hound lacks a body node the clip drives; the other names do not count.
        assert_eq!(missing_body_nodes(HOUND, human_clip, tracks, has), ["Bip01 L Thumb3"]);
        // All body nodes there: not refused, though the folder differs.
        assert!(missing_body_nodes(HOUND, human_clip, ["Bip01", "Bip01 Pelvis", "Weapon"], has).is_empty());
        // Its own folder: whatever it names.
        assert!(missing_body_nodes(HOUND, r"meshes\creatures\hound\idleanims\x.kf", tracks, has).is_empty());
        assert!(missing_body_nodes(HUMAN, human_clip, tracks, |_| false).is_empty());
    }
}
