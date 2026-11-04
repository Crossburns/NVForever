//! What the assets say about who can use which furniture (docs/spec-notes/furniture.md
//! §9): the answers to [`FurnitureFacts`], built once for a run, the same way for the game
//! and for `fnvtool ai run`.
//!
//! The rule is interim [design]. A creature fits a piece when the furniture part of the
//! IDLE tree can give a clip for it from the folder of the creature's own skeleton, and
//! not otherwise: file names prove nothing, since nothing plays a clip the tree does not
//! reach. A person is never ruled out for what a folder lacks: that stays unknown. A
//! skeleton that cannot be found leaves the answer unknown for anyone.

use crate::anims::{actor_folder, file_name, parent, SkeletonIndex};
use crate::furniture::{markers, Furniture, Marker, UseIdles};
use crate::idle::meshes_path;
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_sim::furniture_facts::{ActorKind, Fit, FurnitureFacts};
use fnv_sim::FormId;
use rayon::prelude::*;
use std::collections::{BTreeSet, HashMap};

/// What is known of a furniture base.
#[derive(Debug, Default)]
struct Base {
    /// The IDs of the markers its flags enable; `None`: its model was not read.
    marker_ids: Option<Vec<u8>>,
    /// The skeleton folders of the clips the tree can give for getting on it.
    clip_folders: BTreeSet<String>,
}

/// The facts of a run.
#[derive(Debug, Default)]
pub struct AssetFacts {
    bases: HashMap<FormId, Base>,
    skeletons: SkeletonIndex,
    /// The skeleton folders of every clip of the furniture part of the tree.
    tree_folders: BTreeSet<String>,
}

/// The markers of a model's bytes, `None` when it does not parse.
pub fn read_markers(bytes: &[u8], nif: &fnv_nif::Schema) -> Option<Vec<Marker>> {
    let file = NifFile::parse(bytes, Some(nif)).ok()?;
    let dec = Decoded::new(&file, bytes, nif);
    Some(markers(&file, &dec))
}

impl AssetFacts {
    /// `paths`: the skeleton models among the VFS's paths (others are passed over); `read`
    /// gives a file's bytes. A model that does not read, or panics while it is read, is
    /// left out: its markers stay unknown.
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
        Self::assemble(paths, tree.clips(), |marker, base| tree.clips_for_getting_on(marker, base), bases, &markers_read)
    }

    /// [`AssetFacts::build`] with the models' markers already read, by model path.
    /// `tree_clips`: every clip of the tree; `clips_for`: those it can give for getting on
    /// a marker ID of a base.
    pub fn assemble<'a, 'c>(paths: impl IntoIterator<Item = &'a str>, tree_clips: impl IntoIterator<Item = &'c str>, clips_for: impl Fn(u8, FormId) -> Vec<&'c str>, bases: impl IntoIterator<Item = &'a Furniture>, models: &HashMap<String, Vec<Marker>>) -> AssetFacts {
        let skeletons = SkeletonIndex::from_paths(paths);
        let tree_folders = tree_clips.into_iter().map(|c| skeletons.skeleton_for(c).folder).collect();
        let mut known = HashMap::new();
        for b in bases {
            let mut base = Base::default();
            if let Some(m) = b.model.as_ref().and_then(|m| models.get(m)) {
                let ids: Vec<u8> = m.iter().enumerate().filter(|(i, _)| b.marker_enabled(*i)).map(|(_, m)| m.ref_1).collect();
                base.clip_folders = ids.iter().flat_map(|id| clips_for(*id, b.id)).map(|c| skeletons.skeleton_for(c).folder).collect();
                base.marker_ids = Some(ids);
            }
            known.insert(b.id, base);
        }
        AssetFacts { bases: known, skeletons, tree_folders }
    }

    /// For the run's log: furniture bases, of those with their markers read, and the
    /// skeleton folders the tree has clips in.
    pub fn summary(&self) -> (usize, usize, usize) {
        (self.bases.len(), self.bases.values().filter(|b| b.marker_ids.is_some()).count(), self.tree_folders.len())
    }
}

impl FurnitureFacts for AssetFacts {
    fn marker_ids(&self, furniture_base: FormId) -> Option<&[u8]> {
        self.bases.get(&furniture_base)?.marker_ids.as_deref()
    }

    fn fit(&self, kind: ActorKind, skeleton: &str, furniture_base: FormId) -> Fit {
        let path = meshes_path(skeleton.trim());
        let (Some(base), Some(folder)) = (self.bases.get(&furniture_base), parent(&path)) else { return Fit::Unknown };
        if !self.skeletons.skeletons_in(folder).iter().any(|s| s == file_name(&path)) {
            return Fit::Unknown;
        }
        match kind {
            ActorKind::Unresolved => Fit::Unknown,
            // What the tree does with this piece stays the engine's, as before: a folder
            // says nothing against a person, whose clips may be shared.
            ActorKind::Person if self.tree_folders.contains(folder) => Fit::Supported,
            ActorKind::Person => Fit::Unknown,
            ActorKind::Creature if base.marker_ids.is_none() => Fit::Unknown,
            ActorKind::Creature if base.clip_folders.contains(folder) => Fit::Supported,
            ActorKind::Creature => Fit::Unsupported,
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
    use ActorKind::{Creature, Person, Unresolved};

    const HUMAN: &str = r"Characters\_Male\Skeleton.nif";
    /// A person's skeleton in a folder the tree has no clip in.
    const ELSEWHERE: &str = r"Characters\_Elsewhere\Skeleton.nif";
    const HOUND: &str = r"Creatures\Hound\Skeleton.nif";
    const PARROT: &str = r"Creatures\Parrot\Skeleton.nif";
    const SEAT: FormId = FormId(0x10);
    const BED: FormId = FormId(0x11);
    const SPOT: FormId = FormId(0x12);
    const CROSS: FormId = FormId(0x13);
    const UNREAD: FormId = FormId(0x14);
    const PERCH: FormId = FormId(0x15);
    const DOWN: &str = r"meshes\characters\_male\idleanims\downwego.kf";
    const PERCHING: &str = r"meshes\creatures\parrot\idleanims\ontoperch.kf";

    fn base(id: FormId, model: &str, flags: u32) -> Furniture {
        Furniture { id, edid: None, name: None, model: Some(model.to_string()), marker_flags: Some(flags), has_script: false }
    }

    fn marker(id: u8) -> Marker {
        Marker { offset: [0.0; 3], orientation: 0, ref_1: id, ref_2: id }
    }

    /// Synthetic: the tree gives a person's clip for every marker but work spots, and a
    /// parrot's clip for one base. The hound has a clip whose file name says chair, and the
    /// parrot one that says bed: the tree gives neither.
    fn facts() -> AssetFacts {
        let paths = [
            r"meshes\characters\_male\skeleton.nif",
            r"meshes\characters\_elsewhere\skeleton.nif",
            DOWN,
            r"meshes\creatures\hound\skeleton.nif",
            r"meshes\creatures\hound\idleanims\specialidle_chairenter.kf",
            r"meshes\creatures\parrot\skeleton.nif",
            r"meshes\creatures\parrot\idleanims\bedenter.kf",
            PERCHING,
        ];
        let bases = [
            base(SEAT, "meshes\\seat.nif", 0x4000_0003),
            base(BED, "meshes\\bed.nif", 0x8000_0001),
            base(SPOT, "meshes\\spot.nif", 1),
            base(CROSS, "meshes\\cross.nif", 0x4000_0001),
            base(UNREAD, "meshes\\gone.nif", 0x4000_0001),
            base(PERCH, "meshes\\perch.nif", 0x4000_0001),
        ];
        let models = HashMap::from([
            // The third marker is off: the base enables two.
            ("meshes\\seat.nif".to_string(), vec![marker(11), marker(12), marker(10)]),
            ("meshes\\bed.nif".to_string(), vec![marker(1)]),
            ("meshes\\spot.nif".to_string(), vec![marker(22)]),
            ("meshes\\cross.nif".to_string(), vec![marker(10)]),
            ("meshes\\perch.nif".to_string(), vec![marker(11)]),
        ]);
        let clips_for = |id: u8, b: FormId| {
            let mut out = Vec::new();
            if id != 22 {
                out.push(DOWN);
            }
            if b == PERCH {
                out.push(PERCHING);
            }
            out
        };
        AssetFacts::assemble(paths, [DOWN, PERCHING], clips_for, &bases, &models)
    }

    #[test]
    fn a_person_whose_folder_the_tree_has_clips_in_is_supported() {
        let f = facts();
        for b in [SEAT, BED, SPOT, CROSS, UNREAD] {
            assert_eq!(f.fit(Person, HUMAN, b), Fit::Supported);
        }
    }

    /// The worst regression this rule could bring: people off every chair.
    #[test]
    fn a_person_whose_folder_has_no_clips_is_unknown_never_unsupported() {
        let f = facts();
        for b in [SEAT, BED, SPOT, CROSS, UNREAD, PERCH] {
            assert_eq!(f.fit(Person, ELSEWHERE, b), Fit::Unknown);
            // A person on a creature's skeleton, by a template: the same.
            assert_eq!(f.fit(Person, HOUND, b), Fit::Unknown);
        }
    }

    #[test]
    fn a_file_name_makes_no_creature_supported() {
        let f = facts();
        for b in [SEAT, BED, SPOT, CROSS] {
            assert_eq!(f.fit(Creature, HOUND, b), Fit::Unsupported, "{b:?}");
        }
        assert_eq!(f.fit(Creature, PARROT, BED), Fit::Unsupported);
    }

    #[test]
    fn a_creature_is_supported_only_where_the_tree_gives_a_clip_from_its_folder() {
        let f = facts();
        assert_eq!(f.fit(Creature, PARROT, PERCH), Fit::Supported);
        assert_eq!(f.fit(Creature, PARROT, SEAT), Fit::Unsupported);
        // The tree's clip for this base is the person's, not the hound's.
        assert_eq!(f.fit(Creature, HOUND, PERCH), Fit::Unsupported);
    }

    #[test]
    fn what_cannot_be_resolved_is_unknown() {
        let f = facts();
        for kind in [Creature, Person, Unresolved] {
            // Not resolved; a placeholder model; a folder with no such skeleton; a form
            // that is not furniture.
            assert_eq!(f.fit(kind, "", SEAT), Fit::Unknown);
            assert_eq!(f.fit(kind, "marker_placeholder.nif", SEAT), Fit::Unknown);
            assert_eq!(f.fit(kind, r"Creatures\Nobody\Skeleton.nif", SEAT), Fit::Unknown);
            assert_eq!(f.fit(kind, HOUND, FormId(0x99)), Fit::Unknown);
            // A run without any assets answers the same.
            assert_eq!(AssetFacts::default().fit(kind, HUMAN, SEAT), Fit::Unknown);
        }
        // Neither what the actor is, nor a model that was not read.
        assert_eq!(f.fit(Unresolved, HOUND, SEAT), Fit::Unknown);
        assert_eq!(f.fit(Creature, HOUND, UNREAD), Fit::Unknown);
    }

    #[test]
    fn marker_ids_are_the_enabled_ones_and_unknown_when_the_model_was_not_read() {
        let f = facts();
        assert_eq!(f.marker_ids(SEAT), Some(&[11, 12][..]));
        assert_eq!(f.marker_ids(CROSS), Some(&[10][..]));
        assert_eq!(f.marker_ids(UNREAD), None);
        assert_eq!(f.marker_ids(FormId(0x99)), None);
        assert_eq!(f.summary(), (6, 5, 2));
    }

    #[test]
    fn a_clip_from_elsewhere_is_refused_only_for_body_tracks_without_a_node() {
        let hound = ["Bip01", "Bip01 Pelvis"];
        let has = |n: &str| hound.contains(&n);
        let tracks = ["Bip01", "Bip01 Pelvis", "Bip01 L Thumb3", "Weapon", "##Latch"];
        // The hound lacks a body node the clip drives; the other names do not count.
        assert_eq!(missing_body_nodes(HOUND, DOWN, tracks, has), ["Bip01 L Thumb3"]);
        // All body nodes there: not refused, though the folder differs.
        assert!(missing_body_nodes(HOUND, DOWN, ["Bip01", "Bip01 Pelvis", "Weapon"], has).is_empty());
        // Its own folder: whatever it names.
        assert!(missing_body_nodes(HOUND, r"meshes\creatures\hound\idleanims\x.kf", tracks, has).is_empty());
        assert!(missing_body_nodes(HUMAN, DOWN, tracks, |_| false).is_empty());
    }
}
