//! Body parts posed from the animated skeleton (docs/design/combat.md §5.9, step C8):
//! each drawn actor's body part data (BPTD) is matched to its skeleton once, and every
//! frame the parts' bone chains are published to `nvf_play::hitvol::ActorParts` in game
//! space, where they become one hit capsule per bone segment.
//!
//! - **Which BPTD** (`CombatForms::body_parts_for`): a creature's own (CREA `PNAM`), else
//!   the one whose model is the actor's skeleton (`DefaultBodyPartData` for
//!   `characters\_male\skeleton.nif`) [hypothesis, combat.md §5.9].
//! - **A part's bones** [design]: from the part's node down to its end. When the part's
//!   own bones (its subtree, without other parts' subtrees) lead on to other parts, the
//!   end is the farthest of those connecting bones (the torso runs from `Bip01` up to the
//!   neck); otherwise the farthest `Bip01` bone below it (an arm to the fingers, the head
//!   to the top of the head). The chain does not start on the skeleton's root bone or its
//!   `NonAccum` node: `Bip01`, the torso's node in `DefaultBodyPartData`, stands at the
//!   feet [observed: posed segments, 2026-10-09], so the torso starts at the next joint
//!   (the pelvis). Joints closer than [`MIN_SEGMENT`] to the last one kept are merged.
//! - **The top of the head** [design]: a head part whose chain ends on a head bone (its
//!   name ends in "Head") with no bone below it stops at the base of the skull. The human skeleton is so:
//!   `Bip01 Neck1` (z 107.4) → `Bip01 Head` (z 112.8) and nothing above, while the head
//!   mesh reaches 15.1 above `Bip01 Head` [observed: `characters\_male\skeleton.nif` and
//!   `characters\head\headhuman.nif` decoded, 2026-10-24]. Its capsule shrank to a ball
//!   at the neck, so the face and crown were no head (the owner's playtest, 2026-10-24:
//!   a VATS mark on the head needed the crosshair on the neck). Such a chain's last joint
//!   is moved on along its last segment to [`SKULL_SHARE`] of the bone's height above
//!   the skeleton's root ([`skull_tip`]), a point carried by the head bone, so the
//!   capsule runs from the neck joint to the crown.
//! - **The weapon in the hand** (docs/spec-notes/combat-rules.md §17) [design]: an actor
//!   with its weapon drawn also gets one segment along the weapon model's longest side
//!   ([`weapon_segment`]), numbered `hitvol::WEAPON_PART`, so a shot can strike the
//!   weapon. A holstered weapon has none: a shot at the hip or the back hits the actor.

use bevy::camera::primitives::Aabb;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_combat::data::Limb;
use fnv_sim::FormId;
use nvf_assets::actor::PreparedSkeleton;
use nvf_play::combat::Combat;
use nvf_play::hitvol::{ActorParts, PartSegment, PosedParts, WEAPON_PART};
use std::collections::HashMap;
use std::sync::Arc;

use crate::weapons::{HeldWeapon, OnBack, WeaponHold};

/// Joints closer than this to the previous kept joint are merged, game units [design].
pub const MIN_SEGMENT: f32 = 4.0;

/// A part's side branch is covered when it reaches this far from the part's chain, game
/// units, at most [`MAX_BRANCHES`] of them, the longest first [design: long enough to
/// leave out fingers and twist bones, short enough for a bloatfly's legs].
pub const BRANCH_MIN: f32 = 12.0;
pub const MAX_BRANCHES: usize = 8;

/// How far past a head bone with no bone below it the skull reaches, as a share of the
/// bone's height above the skeleton's root [observed: the human head mesh's top is 15.1
/// above `Bip01 Head`, at z 112.8, about 15.6 along the neck's line: 0.138].
pub const SKULL_SHARE: f32 = 0.135;

/// An actor's skeleton as spawned, until its parts are matched ([`LimbRig`]).
#[derive(Component, Clone)]
pub struct ActorRig {
    pub reference: u32,
    pub base: u32,
    /// The base its body comes from: a creature's Model/Animation source (a template's
    /// pick), else `base`. Its body part data goes with the skeleton
    /// (`CombatForms::body_parts_for`).
    pub body: u32,
    pub skeleton: String,
    pub skel: Arc<PreparedSkeleton>,
    /// The skeleton's node entities, in `skel` order.
    pub nodes: Vec<Entity>,
}

/// An actor's body parts on its node entities.
#[derive(Component, Clone, Debug)]
pub struct LimbRig {
    pub data: FormId,
    /// (part index, kind, the chain's node entities from the part's node to its end,
    /// the top of the skull in the end node's own frame when the chain stops short of it:
    /// [`skull_tip`]).
    pub chains: Vec<(u8, Limb, Vec<Entity>, Option<Vec3>)>,
    /// Per chain, the widest ragdoll shape on its bones in the skeleton's units
    /// ([`BoneRadii`]); `None` without one.
    pub radii: Vec<Option<f32>>,
}

pub struct LimbsPlugin;

impl Plugin for LimbsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoneRadii>().add_systems(Update, (rig_limbs, crate::weapons::drop_broken).run_if(resource_exists::<Combat>)).add_systems(
            PostUpdate,
            publish_limbs.after(bevy::transform::TransformSystems::Propagate).run_if(resource_exists::<ActorParts>),
        );
    }
}

/// The node chains of a skeleton's body parts (see the module notes): for each part
/// whose node the skeleton has, its index and the nodes from its node to its end. `bind`
/// holds the nodes' bind-pose positions.
pub fn part_chains(names: &[Option<String>], parents: &[Option<usize>], bind: &[Vec3], part_nodes: &[Option<&str>]) -> Vec<(usize, Vec<usize>)> {
    let find = |n: &str| names.iter().position(|x| x.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(n)));
    let nodes: Vec<Option<usize>> = part_nodes.iter().map(|n| n.and_then(find)).collect();
    let is_part = |i: usize| nodes.contains(&Some(i));
    let is_bone = |i: usize| names[i].as_deref().is_some_and(|s| s.to_ascii_lowercase().starts_with("bip01"));
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); names.len()];
    for (i, p) in parents.iter().enumerate() {
        if let Some(p) = p {
            children[*p].push(i);
        }
    }
    let is_ancestor = |a: usize, mut b: usize| {
        while let Some(p) = parents[b] {
            if p == a {
                return true;
            }
            b = p;
        }
        false
    };
    let mut out = Vec::new();
    for (k, n) in nodes.iter().enumerate() {
        let Some(n) = *n else { continue };
        // The part's own bones: its subtree without other parts' subtrees.
        let mut own = Vec::new();
        let mut stack = vec![n];
        while let Some(i) = stack.pop() {
            own.push(i);
            stack.extend(children[i].iter().copied().filter(|&c| !is_part(c)));
        }
        let others: Vec<usize> = nodes.iter().flatten().copied().filter(|&o| o != n && is_ancestor(n, o)).collect();
        let connecting: Vec<usize> = own.iter().copied().filter(|&i| i != n && others.iter().any(|&o| is_ancestor(i, o))).collect();
        let candidates: Vec<usize> = if connecting.is_empty() {
            own.iter().copied().filter(|&i| i != n && is_bone(i)).collect()
        } else {
            connecting
        };
        let end = candidates.into_iter().max_by(|&a, &b| bind[a].distance(bind[n]).total_cmp(&bind[b].distance(bind[n])));
        let mut path = vec![];
        if let Some(mut i) = end {
            while i != n {
                path.push(i);
                i = parents[i].expect("below the part's node");
            }
        }
        path.push(n);
        path.reverse();
        let full = path.clone();
        // Not from the root bone or the accumulation node (they stand at the feet).
        let root_like = |i: usize| {
            parents[i].is_none_or(|p| parents[p].is_none()) || names[i].as_deref().is_some_and(|s| s.to_ascii_lowercase().contains("nonaccum"))
        };
        while path.len() > 1 && root_like(path[0]) {
            path.remove(0);
        }
        out.push((k, merged(&path, bind)));
        // Side branches: the part's own bones off its chain that reach far (a creature's
        // legs that are no part of their own, a radscorpion's head) are its body too
        // (the owner's playtest list 5 item 16: shots passed through a bloatfly's lower
        // body) [design]. Each branch from where it leaves the chain to its farthest bone.
        let on_path = |i: usize| full.contains(&i);
        let mut branches: Vec<(f32, Vec<usize>)> = Vec::new();
        for &first in own.iter().filter(|&&i| !on_path(i) && parents[i].is_some_and(on_path)) {
            let from = parents[first].expect("off the chain");
            // The farthest bone of this branch, its own bones only.
            let mut far = (first, bind[first].distance(bind[from]));
            let mut stack = vec![first];
            while let Some(i) = stack.pop() {
                let d = bind[i].distance(bind[from]);
                if d > far.1 && is_bone(i) {
                    far = (i, d);
                }
                stack.extend(children[i].iter().copied().filter(|&c| !is_part(c) && !on_path(c)));
            }
            if far.1 < BRANCH_MIN || !is_bone(far.0) || others.iter().any(|&o| is_ancestor(first, o) || o == first) {
                continue;
            }
            let mut b = vec![far.0];
            let mut i = far.0;
            while i != from {
                i = parents[i].expect("below the branch's start");
                b.push(i);
            }
            b.reverse();
            // Not from the root bone or the accumulation node at the feet.
            while b.len() > 2 && root_like(b[0]) {
                b.remove(0);
            }
            branches.push((far.1, b));
        }
        branches.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, b) in branches.into_iter().take(MAX_BRANCHES) {
            out.push((k, merged(&b, bind)));
        }
    }
    out
}

/// The top of the skull, bind space, for a head chain (joint indices) that ends on a head
/// bone (named "… Head") with no bone below it (see the module notes): the last segment
/// carried on by [`SKULL_SHARE`] of the end bone's height above the skeleton's root.
/// `None` for a chain of one joint, another end (a "HeadNub" already at the top), an end
/// with bones below it, or an end not above the root.
pub fn skull_tip(names: &[Option<String>], parents: &[Option<usize>], bind: &[Vec3], chain: &[usize]) -> Option<Vec3> {
    let [.., prev, end] = *chain else { return None };
    let is_bone = |i: usize| names[i].as_deref().is_some_and(|s| s.to_ascii_lowercase().starts_with("bip01"));
    let below = |i: usize| {
        let mut p = parents[i];
        while let Some(x) = p {
            if x == end {
                return true;
            }
            p = parents[x];
        }
        false
    };
    let head = names[end].as_deref().is_some_and(|s| s.to_ascii_lowercase().ends_with("head"));
    if !head || (0..names.len()).any(|i| is_bone(i) && below(i)) {
        return None;
    }
    let mut root = end;
    while let Some(p) = parents[root] {
        root = p;
    }
    let height = bind[end].z - bind[root].z;
    let dir = (bind[end] - bind[prev]).normalize_or_zero();
    (height > 0.0 && dir != Vec3::ZERO).then(|| bind[end] + dir * SKULL_SHARE * height)
}

/// A path of joints with those too close to the last one kept merged (the end always
/// stays).
fn merged(path: &[usize], bind: &[Vec3]) -> Vec<usize> {
    let mut chain = vec![path[0]];
    for (j, &i) in path.iter().enumerate().skip(1) {
        let last = *chain.last().expect("start");
        if bind[i].distance(bind[last]) >= MIN_SEGMENT || (j + 1 == path.len() && chain.len() == 1) {
            chain.push(i);
        }
    }
    chain
}

/// One drawn shape of a weapon model: its transform and its local bounds (centre, half
/// extents).
pub type ShapeBox = (Affine3A, Vec3, Vec3);

/// The segment along a weapon model: the ends of the longest side of the box around
/// its shapes, through the box's middle. The box is taken in the first shape's frame
/// (the shapes of one model hang rigidly from one node). `None` without shapes.
pub fn weapon_segment(shapes: &[ShapeBox]) -> Option<(Vec3, Vec3)> {
    let frame = shapes.first()?.0;
    let inv = frame.inverse();
    let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
    for (m, c, h) in shapes {
        let to = inv * *m;
        for i in 0..8 {
            let sign = Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
            let p = to.transform_point3(*c + *h * sign);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let (c, h) = ((lo + hi) / 2.0, (hi - lo) / 2.0);
    let axis = if h.x >= h.y && h.x >= h.z {
        Vec3::X * h.x
    } else if h.y >= h.z {
        Vec3::Y * h.y
    } else {
        Vec3::Z * h.z
    };
    Some((frame.transform_point3(c - axis), frame.transform_point3(c + axis)))
}

/// Match spawned actors' skeletons to their body part data, once.
fn rig_limbs(mut commands: Commands, combat: Res<Combat>, data: Option<Res<nvf_assets::GameData>>, mut radii: ResMut<BoneRadii>, rigs: Query<(Entity, &ActorRig), Without<LimbRig>>) {
    for (e, rig) in &rigs {
        let mut limb = LimbRig { data: FormId(0), chains: Vec::new(), radii: Vec::new() };
        let widths = data.as_deref().map(|d| radii.of(d, &rig.skeleton));
        // A body part data FormID that is missing or not a BPTD (a broken mod): no limbs.
        let found = combat.forms.body_parts_for(FormId(rig.body), &rig.skeleton).and_then(|d| match combat.forms.body_parts.get(&d) {
            Some(b) => Some((d, b)),
            None => {
                warn!("actor {:08X}: body part data {d} not found; no limbs", rig.reference);
                None
            }
        });
        if let Some((data, bpd)) = found {
            let parts = &bpd.parts;
            let names: Vec<Option<&str>> = parts.iter().map(|p| p.node.as_deref()).collect();
            let bind: Vec<Vec3> = rig.skel.bind_world.iter().map(|a| Vec3::from(a.translation)).collect();
            limb.data = data;
            for (k, chain) in part_chains(&rig.skel.names, &rig.skel.parents, &bind, &names) {
                let ents: Vec<Entity> = chain.iter().filter_map(|&i| rig.nodes.get(i).copied()).collect();
                let kind = parts[k].limb();
                // The skull's top in the end bone's own frame, so it nods with the head.
                let tip = (kind == Limb::Head)
                    .then(|| skull_tip(&rig.skel.names, &rig.skel.parents, &bind, &chain))
                    .flatten()
                    .zip(chain.last().and_then(|&e| rig.skel.bind_world.get(e)))
                    .map(|(t, end)| end.inverse().transform_point3(t));
                let width = widths.as_ref().and_then(|w| chain.iter().filter_map(|&i| rig.skel.names.get(i).cloned().flatten()).filter_map(|n| w.get(&n.to_ascii_lowercase()).copied()).reduce(f32::max));
                limb.chains.push((k as u8, kind, ents, tip));
                limb.radii.push(width);
            }
            debug!("actor {:08X}: body parts {data} on {} chains", rig.reference, limb.chains.len());
        }
        commands.entity(e).insert(limb);
    }
}

/// The widest ragdoll shape on each bone of a skeleton (lower-case bone name → game
/// units), read once per skeleton from its ragdoll bodies (`fnv_collide::ragdoll`): a
/// capsule's or sphere's radius, a box's middle half-extent [design: the box's second
/// side, so a flat plate does not count as its length]. A file that breaks the reader
/// has none.
#[derive(Resource, Default)]
pub struct BoneRadii(HashMap<String, Arc<HashMap<String, f32>>>);

impl BoneRadii {
    fn of(&mut self, data: &nvf_assets::GameData, skeleton: &str) -> Arc<HashMap<String, f32>> {
        let key = fnv_vfs::normalize(skeleton);
        self.0
            .entry(key.clone())
            .or_insert_with(|| {
                let read = data.read(&key).and_then(|bytes| {
                    nvf_assets::guard::guarded(|| {
                        let file = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)).map_err(|e| e.to_string())?;
                        let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
                        Ok(fnv_collide::ragdoll::ragdoll(&file, &dec))
                    })
                    .ok()
                });
                let mut out = HashMap::new();
                for b in read.map(|r| r.bodies).unwrap_or_default() {
                    let Some(bone) = b.object.node.as_deref() else { continue };
                    if let Some(w) = shape_width(&b.object.shape) {
                        let e = out.entry(bone.to_ascii_lowercase()).or_insert(0.0f32);
                        *e = e.max(w);
                    }
                }
                debug!("limbs: {key}: ragdoll widths {out:?}");
                Arc::new(out)
            })
            .clone()
    }
}

/// A shape's width for [`BoneRadii`].
fn shape_width(s: &fnv_collide::shape::Shape) -> Option<f32> {
    use fnv_collide::shape::Shape;
    match s {
        Shape::Capsule { radius, .. } | Shape::Sphere { radius } => Some(*radius),
        Shape::Box { half_extents } => {
            let mut h = *half_extents;
            h.sort_by(f32::total_cmp);
            Some(h[1])
        }
        Shape::Compound { children } => children.iter().filter_map(|(_, c)| shape_width(c)).reduce(f32::max),
        _ => None,
    }
}

/// Publish every rigged actor's posed parts (game space) for the hit volumes.
fn publish_limbs(
    mut out: ResMut<ActorParts>,
    rigs: Query<(&ActorRig, &LimbRig, &InheritedVisibility, Option<&WeaponHold>)>,
    nodes: Query<&GlobalTransform>,
    held: Query<(&HeldWeapon, &GlobalTransform, &Aabb), Without<OnBack>>,
) {
    out.by_ref.clear();
    // The shapes of the weapons in hand, by actor.
    let mut weapons: HashMap<u32, (FormId, Vec<ShapeBox>)> = HashMap::new();
    for (h, g, aabb) in &held {
        weapons.entry(h.reference).or_insert_with(|| (h.weapon, Vec::new())).1.push((g.affine(), Vec3::from(aabb.center), Vec3::from(aabb.half_extents)));
    }
    for (rig, limb, vis, hold) in &rigs {
        if !vis.get() || limb.chains.is_empty() {
            continue;
        }
        let mut posed = PosedParts { data: limb.data, segments: Vec::new(), weapon: None };
        for (n, (part, kind, chain, tip)) in limb.chains.iter().enumerate() {
            // The ragdoll shape's width at the actor's scale (its bone's).
            let scale = chain.first().and_then(|&e| nodes.get(e).ok()).map_or(1.0, |g| g.affine().matrix3.x_axis.length());
            let radius = limb.radii.get(n).copied().flatten().map(|r| r * scale);
            let mut pts: Vec<[f32; 3]> = chain.iter().filter_map(|&e| nodes.get(e).ok()).map(|g| nvf_play::space::to_game(g.translation())).collect();
            // A head that stops at the base of the skull ends at its crown instead.
            if let (Some(tip), Some(end)) = (tip, chain.last().and_then(|&e| nodes.get(e).ok())) {
                if pts.len() == chain.len() && pts.len() >= 2 {
                    *pts.last_mut().expect("two points") = nvf_play::space::to_game(end.transform_point(*tip));
                }
            }
            match pts.len() {
                0 => {}
                1 => posed.segments.push(PartSegment { part: *part, limb: *kind, a: pts[0], b: pts[0], radius }),
                _ => posed.segments.extend(pts.windows(2).map(|w| PartSegment { part: *part, limb: *kind, a: w[0], b: w[1], radius })),
            }
        }
        if let Some((weapon, shapes)) = weapons.get(&rig.reference).filter(|_| hold.is_some_and(|h| h.drawn)) {
            if let Some((a, b)) = weapon_segment(shapes) {
                posed.segments.push(PartSegment { part: WEAPON_PART, limb: Limb::Other, radius: None, a: nvf_play::space::to_game(a), b: nvf_play::space::to_game(b) });
                posed.weapon = Some(*weapon);
            }
        }
        out.by_ref.insert(rig.reference, posed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small biped as the game's skeletons are laid out: the scene root, Bip01 and Bip01
    /// NonAccum at the feet, then Pelvis → (L Thigh → L Calf → L Foot), Spine → Neck →
    /// (Neck1 → Head → HeadNub), (L UpperArm → L Forearm → L Hand → Weapon).
    fn biped() -> (Vec<Option<String>>, Vec<Option<usize>>, Vec<Vec3>) {
        let nodes: [(&str, Option<usize>, [f32; 3]); 16] = [
            ("skeleton.nif", None, [0.0, 0.0, 0.0]),
            ("Bip01", Some(0), [0.0, 0.0, 0.0]),
            ("Bip01 NonAccum", Some(1), [0.0, 0.0, 0.0]),
            ("Bip01 Pelvis", Some(2), [0.0, 0.0, 70.0]),
            ("Bip01 L Thigh", Some(3), [-8.0, 0.0, 66.0]),
            ("Bip01 L Calf", Some(4), [-8.0, 0.0, 36.0]),
            ("Bip01 L Foot", Some(5), [-8.0, 0.0, 6.0]),
            ("Bip01 Spine", Some(3), [0.0, 0.0, 78.0]),
            ("Bip01 Neck", Some(7), [0.0, 0.0, 108.0]),
            ("Bip01 Neck1", Some(8), [0.0, 0.0, 112.0]),
            ("Bip01 Head", Some(9), [0.0, 0.0, 115.0]),
            ("Bip01 HeadNub", Some(10), [0.0, 0.0, 128.0]),
            ("Bip01 L UpperArm", Some(8), [-14.0, 0.0, 104.0]),
            ("Bip01 L Forearm", Some(12), [-14.0, 0.0, 80.0]),
            ("Bip01 L Hand", Some(13), [-14.0, 0.0, 58.0]),
            ("Weapon", Some(14), [-14.0, 0.0, 40.0]),
        ];
        (nodes.iter().map(|n| Some(n.0.to_string())).collect(), nodes.iter().map(|n| n.1).collect(), nodes.iter().map(|n| Vec3::from(n.2)).collect())
    }

    #[test]
    fn chains_follow_the_bones_between_parts() {
        let (names, parents, bind) = biped();
        let parts = [Some("Bip01"), Some("bip01 neck1"), Some("Bip01 L UpperArm"), Some("Bip01 L Thigh"), Some("Bip01 R Thigh")];
        let chains = part_chains(&names, &parents, &bind, &parts);
        // The torso runs from the pelvis (not Bip01 and NonAccum at the feet) up to the neck;
        // the head to its top (Head merged, too close to Neck1); the arm to the hand (Weapon
        // is no bone); the leg to the foot. The missing right thigh has no chain.
        assert_eq!(chains, vec![(0, vec![3, 7, 8]), (1, vec![9, 11]), (2, vec![12, 13, 14]), (3, vec![4, 5, 6])]);
    }

    #[test]
    fn a_weapon_segment_runs_along_the_model() {
        // A barrel 20 long on local Y and a grip below it, hanging from a node moved to
        // (100, 0, 0) and turned a quarter turn about Z (local Y to world -X).
        let node = Affine3A::from_rotation_translation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2), Vec3::new(100.0, 0.0, 0.0));
        let barrel = (node, Vec3::new(0.0, 10.0, 0.0), Vec3::new(1.0, 10.0, 1.0));
        let grip = (node * Affine3A::from_translation(Vec3::new(0.0, 0.0, -4.0)), Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0));
        let (a, b) = weapon_segment(&[barrel, grip]).unwrap();
        // The box spans local y from -2 (the grip's back) to 20 and z from -7 to 1: the
        // segment runs along y through z = -3.
        assert!(a.distance(Vec3::new(102.0, 0.0, -3.0)) < 1e-3, "{a}");
        assert!(b.distance(Vec3::new(80.0, 0.0, -3.0)) < 1e-3, "{b}");
        assert_eq!(weapon_segment(&[]), None);
    }

    /// A bloatfly as its skeleton is laid out [observed: `creatureslowfly\skeleton.nif`
    /// names; places made up]: flying, Bip01 at the body; the abdomen along the tail to
    /// the stinger (a part), a leg hanging below that is no part, and the head and brain
    /// in front.
    fn bloatfly() -> (Vec<Option<String>>, Vec<Option<usize>>, Vec<Vec3>) {
        let nodes: [(&str, Option<usize>, [f32; 3]); 13] = [
            ("Scene Root", None, [0.0, 0.0, 0.0]),
            ("Bip01", Some(0), [0.0, 0.0, 136.0]),
            ("Bip01 NonAccum", Some(1), [0.0, 0.0, 136.0]),
            ("Bip01 Spine", Some(2), [0.0, 2.0, 136.0]),
            ("Bip01 neck01", Some(3), [0.0, 12.0, 138.0]),
            ("Bip01 Head", Some(4), [0.0, 18.0, 138.0]),
            ("Bip01 L UpperArm", Some(3), [-6.0, 4.0, 142.0]),
            ("Bip01 L Thigh 01", Some(3), [-4.0, 4.0, 132.0]),
            ("Bip01 L Calf 01", Some(7), [-6.0, 6.0, 118.0]),
            ("Bip01 L Foot 01", Some(8), [-6.0, 8.0, 108.0]),
            ("Bip01 tail", Some(3), [0.0, -6.0, 134.0]),
            ("Bip01 tail01", Some(10), [0.0, -20.0, 128.0]),
            ("Bip01 tail04", Some(11), [0.0, -34.0, 118.0]),
        ];
        (nodes.iter().map(|n| Some(n.0.to_string())).collect(), nodes.iter().map(|n| n.1).collect(), nodes.iter().map(|n| Vec3::from(n.2)).collect())
    }

    #[test]
    fn a_bloatfly_body_covers_its_abdomen_and_legs() {
        let (names, parents, bind) = bloatfly();
        // BloatFlyBodyPartData's nodes [observed]: Body, a wing, Head, Brain, Stinger.
        let parts = [Some("Bip01"), Some("Bip01 L UpperArm"), Some("Bip01 Head"), Some("Bip01 neck01"), Some("Bip01 tail04")];
        let chains = part_chains(&names, &parents, &bind, &parts);
        // The body runs from the spine along the abdomen towards the stinger, and the leg
        // hanging below it is the body's too (playtest list 5 item 16); the wing, head,
        // brain and stinger as before.
        assert_eq!(chains, vec![(0, vec![3, 10, 11]), (0, vec![3, 7, 8, 9]), (1, vec![6]), (2, vec![5]), (3, vec![4]), (4, vec![12])]);
    }

    /// The human head as `characters\_male\skeleton.nif` lays it out [observed, 2026-10-24]:
    /// `Bip01 Head` has no bone below it (only `HeadAnims`), and the head mesh reaches
    /// 15.1 above it.
    fn human_head() -> (Vec<Option<String>>, Vec<Option<usize>>, Vec<Vec3>) {
        let nodes: [(&str, Option<usize>, [f32; 3]); 6] = [
            ("Scene Root", None, [0.0, 0.0, 0.0]),
            ("Bip01", Some(0), [0.0, 0.0, 0.0]),
            ("Bip01 Neck", Some(1), [0.0, -1.0, 104.0]),
            ("Bip01 Neck1", Some(2), [0.0, -1.0, 107.4]),
            ("Bip01 Head", Some(3), [0.0, 0.4, 112.8]),
            ("HeadAnims", Some(4), [0.0, 0.4, 112.8]),
        ];
        (nodes.iter().map(|n| Some(n.0.to_string())).collect(), nodes.iter().map(|n| n.1).collect(), nodes.iter().map(|n| Vec3::from(n.2)).collect())
    }

    #[test]
    fn a_head_without_a_top_bone_reaches_the_crown() {
        let (names, parents, bind) = human_head();
        let chains = part_chains(&names, &parents, &bind, &[Some("Bip01 Neck1")]);
        assert_eq!(chains, vec![(0, vec![3, 4])]);
        let tip = skull_tip(&names, &parents, &bind, &chains[0].1).expect("the head stops at the skull's base");
        // The mesh's top is at 127.9.
        assert!((126.5..129.0).contains(&tip.z) && tip.y > 0.4, "{tip}");
        // The capsule (a 128-unit actor) now holds the face and the crown, not only the
        // neck: it once shrank to a ball at z 110.
        let seg = nvf_play::hitvol::PartSegment { part: 0, limb: Limb::Head, radius: None, a: bind[3].to_array(), b: tip.to_array() };
        let cap = nvf_play::hitvol::part_capsules(&[seg], 128.0)[0];
        let inside = |p: Vec3| {
            let (a, b) = (Vec3::from(cap.a), Vec3::from(cap.b));
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            p.distance(a + (b - a) * t) <= cap.radius
        };
        for (what, p) in [("eyes", Vec3::new(0.0, 6.0, 119.0)), ("forehead", Vec3::new(0.0, 5.0, 124.0)), ("crown", Vec3::new(0.0, 0.0, 126.5)), ("chin", Vec3::new(0.0, 4.0, 109.0))] {
            assert!(inside(p), "{what} {p} outside {cap:?}");
        }
        // A head with a bone to its top (the made-up biped's HeadNub) is left as it is,
        // and so is a lone joint.
        let (names, parents, bind) = biped();
        assert_eq!(skull_tip(&names, &parents, &bind, &[9, 11]), None);
        assert_eq!(skull_tip(&names, &parents, &bind, &[10]), None);
    }

    #[test]
    fn a_lone_part_node_is_a_point() {
        let (names, parents, bind) = biped();
        let chains = part_chains(&names, &parents, &bind, &[Some("Bip01 L Foot")]);
        assert_eq!(chains, vec![(0, vec![6])]);
    }
}
