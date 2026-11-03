//! Diagnostic (#1309): where the hands sit against each other and against
//! the `Weapon` node (the left hand also in its frame) when the skeleton is posed from given
//! clips, each at weight 1, blended by track priority (no rest pose for the `Weapon` node).
//!
//! cargo run --release -p nvf-assets --example hand_probe -- <Data dir> <nif schema dir> <time s> <clip>...

use bevy::math::Affine3A;
use bevy::prelude::{Entity, Transform};
use nvf_assets::actor::{bind_clip, ClipReader, ClipSource};
use nvf_assets::anim_play::{BlendPlayer, Layer};
use nvf_assets::data::GameData;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = GameData::open(a[1].as_ref(), a[2].as_ref()).expect("game data");
    let time: f32 = a[3].parse().unwrap();
    let skel = nvf_assets::actor::probe_skeleton(&data, r"meshes\characters\_male\skeleton.nif").expect("skeleton");
    let mut reader = ClipReader::new(data);
    let n = skel.names.len();
    let nodes = (0..n as u32).map(|i| Entity::from_raw_u32(i + 1).unwrap()).collect();
    let mut player = BlendPlayer::new(nodes, Arc::new(skel.locals.clone()));
    for (slot, path) in a[4..].iter().enumerate() {
        let clip = reader.clip(path).unwrap_or_else(|| panic!("{path}"));
        let len = clip.length().max(1e-3);
        player.layers[slot] = Some(Layer { clip: Arc::new(bind_clip(&skel, clip)), phase: (time / len).min(0.999), weight: 1.0 });
    }
    let mut out = Vec::new();
    player.sample(&mut out);
    let mut local: Vec<Transform> = skel.locals.clone();
    for (i, t) in out {
        local[i] = t;
    }
    let mut world = vec![Affine3A::IDENTITY; n];
    for i in 0..n {
        let l = Affine3A::from_scale_rotation_translation(local[i].scale, local[i].rotation, local[i].translation);
        world[i] = match skel.parents[i] {
            Some(p) => world[p] * l,
            None => l,
        };
    }
    let at = |name: &str| world[skel.by_name[name]].translation;
    let (l, r, w) = (at("Bip01 L Hand"), at("Bip01 R Hand"), at("Weapon"));
    let wi = world[skel.by_name["Weapon"]].inverse();
    println!("L hand in the Weapon frame {:?}", wi.transform_point3a(l));
    let head = at("Bip01 Head");
    let pelvis = at("Bip01 Pelvis");
    println!(
        "{:?} at {time} s: L-R hand {:.1}, L hand-Weapon {:.1}, R hand-Weapon {:.1}, above pelvis: L {:.1} R {:.1} Weapon {:.1} head {:.1}",
        &a[4..],
        l.distance(r),
        l.distance(w),
        r.distance(w),
        l.z - pelvis.z,
        r.z - pelvis.z,
        w.z - pelvis.z,
        head.z - pelvis.z
    );
}
