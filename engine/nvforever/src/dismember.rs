//! Dismemberment as drawn (docs/spec-notes/dismemberment.md §5–§6). The rules decide in
//! `nvf_play::dismember` and keep the gone parts in the game; here:
//!
//! - **Skins** ([`apply_skins`]): every drawn shape of an actor (`LimbShape`) is drawn
//!   from its partitions: an exploded part's partitions (and the torso sections that
//!   go with it) leave the body, a gone part's torso cap shows, a severed part's section
//!   cap shows (it leaves with the severed bones). A shape skinned to bones on both sides of
//!   a severed part's cut is cut apart ([`sever_groups`]), each side's triangles weighted
//!   to its own bones only, so none stretches from the body to the piece. A shape without partitions whose bones
//!   all lie in an exploded part's bone tree (head, hair, eyes, hat) is not drawn. Hidden
//!   shapes are given a mesh with no triangles, so the visibility other systems set
//!   (`RefVisual`) does not bring them back. Done again whenever shapes are spawned
//!   (dressing, a cell coming back, a loaded game), and when another system gave a
//!   redrawn shape a mesh again ([`CutMesh`]): the moving face (`crate::lips`) hands a
//!   dying actor's head, eyes and hair their own meshes back as it lets the face go,
//!   which put an exploded head back on the body (the owner's playtest list 7 item 26).
//! - **Ragdolls** ([`cut_ragdolls`]): a severed part's ragdoll bodies are cut from the rest.
//! - **Gore** ([`throw_gore`], [`move_pieces`]): at an exploded part's gore bone, its
//!   explosion and blood, and the pieces of its limb replacement model and its debris
//!   flying off as small physics bodies (`fnv_phys::gore`) owned by the corpse, so
//!   activating one searches it.

use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_combat::data::BodyPartData;
use fnv_combat::rng::SplitMix;
use fnv_phys::gore::GoreStart;
use fnv_sim::FormId;
use nvf_assets::actor::{dismember_mesh, sever_mesh, skin_weights, LimbShape};
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_play::combat::{Combat, ExplosionFx, ExplosionLook, ImpactFx};
use nvf_play::dismember::Dismembered;
use nvf_play::phys::PhysState;
use nvf_play::script::ScriptState;
use nvf_play::sfx::{PlaySound, Sfx};
use nvf_play::{space, RefVisual};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use crate::limbs::{ActorRig, LimbRig};
use crate::ragdoll::Corpse;
use crate::world::WorldRoot;

/// Gore pieces at once, most (`iDebrisMaxCount` when the masters set it; they do not
/// [observed]) [hypothesis: 64].
pub const MAX_PIECES: usize = 64;

/// A piece's start speed range (units per second), its spin (radians per second) and its
/// mass (kg) [design].
pub const PIECE_SPEED: (f32, f32) = (150.0, 350.0);
pub const PIECE_SPIN: f32 = 12.0;
pub const PIECE_MASS: f32 = 0.5;

/// Blood impacts shown for one part, most [design: the BPTD's decal counts are 4 and 6].
pub const MAX_BLOOD: u32 = 6;

/// A gore piece's box: half extents and centre in its frame.
type PieceBox = ([f32; 3], [f32; 3]);

/// A piece of a gore model: its top node, its transform in the model, its shapes with their
/// transforms relative to it.
type Piece = (Option<usize>, Affine3A, Vec<(Affine3A, usize)>);

/// The mesh [`apply_skins`] drew a shape with; a shape whose mesh is no longer this one
/// was given another by some other system and is drawn again.
#[derive(Component)]
struct CutMesh(AssetId<Mesh>);

/// A flying piece of gore.
#[derive(Component)]
struct GorePiece {
    id: u32,
    actor: u32,
    /// It has come to rest (and been measured, [`move_pieces`]).
    settled: bool,
}

#[derive(Resource)]
struct Gore {
    /// The gone parts drawn per actor reference.
    applied: HashMap<u32, Vec<(i32, bool)>>,
    next: u32,
    /// Pieces in the order they came.
    pieces: Vec<(u32, Entity)>,
    rng: SplitMix,
    /// Explosions waiting for their models (first asked for this frame).
    waiting: Vec<(Dismembered, u8)>,
    /// Box half extents and centre of each piece of a model, by path and node.
    boxes: HashMap<(String, Option<usize>), PieceBox>,
    shapes_redrawn: u32,
    joints_cut: u32,
    thrown: u32,
    /// Per actor, the pieces thrown and the distances from its corpse those at rest lie
    /// at, for the log line that tells where the gore ended up.
    landed: HashMap<u32, (u32, Vec<f32>)>,
}

impl Default for Gore {
    fn default() -> Gore {
        Gore { applied: HashMap::new(), next: 1, pieces: Vec::new(), rng: SplitMix(0x60AE), waiting: Vec::new(), boxes: HashMap::new(), shapes_redrawn: 0, joints_cut: 0, thrown: 0, landed: HashMap::new() }
    }
}

pub struct DismemberPlugin;

impl Plugin for DismemberPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gore>()
            .add_systems(
                Update,
                (apply_skins, cut_ragdolls, throw_gore.before(crate::shot_fx::ShotFxSet).before(nvf_play::sfx::SfxSet), throw_object_debris, move_pieces.after(nvf_play::PlaySet::Physics))
                    .after(nvf_play::PlaySet::Activation)
                    .run_if(resource_exists::<Combat>)
                    .run_if(resource_exists::<PhysState>),
            )
            .add_systems(Last, report);
    }
}

/// The body part data an actor's parts come from.
fn body_parts<'a>(combat: &'a Combat, rig: &ActorRig, limb: Option<&LimbRig>) -> Option<&'a BodyPartData> {
    let id = limb.map(|l| l.data).or_else(|| combat.forms.body_parts_for(FormId(rig.body), &rig.skeleton))?;
    combat.forms.body_parts.get(&id)
}

/// The skeleton node named `name` (any case).
fn node(rig: &ActorRig, name: &str) -> Option<usize> {
    rig.skel.by_name.get(name).copied().or_else(|| rig.skel.names.iter().position(|n| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))))
}

/// Node `root` and every node below it.
fn subtree(rig: &ActorRig, root: usize) -> Vec<usize> {
    let p = &rig.skel.parents;
    (0..p.len())
        .filter(|&i| {
            let mut cur = Some(i);
            let mut steps = 0;
            while let Some(c) = cur {
                if c == root {
                    return true;
                }
                cur = p[c];
                steps += 1;
                if steps > 256 {
                    break;
                }
            }
            false
        })
        .collect()
}

/// The node names (lower case) of the part at `location`.
fn part_names(rig: &ActorRig, data: &BodyPartData, location: u16) -> HashSet<String> {
    part_nodes(rig, data, location as i32).into_iter().filter_map(|n| rig.skel.names[n].as_deref().map(str::to_ascii_lowercase)).collect()
}

/// The steps from the skeleton's root to node `n`.
fn depth(rig: &ActorRig, n: usize) -> usize {
    let mut d = 0;
    let mut cur = rig.skel.parents[n];
    while let Some(c) = cur {
        d += 1;
        cur = rig.skel.parents[c];
        if d > 256 {
            break;
        }
    }
    d
}

/// The severed piece (side 1, 2, …, `sides`) a partition of body part `bp` leaves with:
/// the part itself, its torso section and its section cap.
pub fn piece_of(bp: u16, cap: bool, sides: &[u16]) -> Option<u8> {
    let location = match (cap, bp) {
        (true, 101..=113) => bp - 100,
        (false, 0..=13) => bp,
        (false, 1000..) => bp / 1000,
        _ => return None,
    };
    sides.iter().position(|&l| l == location).map(|k| k as u8 + 1)
}

/// How a skinned shape carried by bones both of the body and of a severed piece is cut
/// apart (nvf_assets `sever_mesh`): its drawn triangles by side, the side of each of its
/// joints and the joint carrying a vertex left with no weight on its side (for a piece,
/// its bone nearest the cut; for the body, the nearest bone above a cut). A shape with
/// partitions goes by them (`piece_of`, else `partition_drawn`); one without, triangle
/// by triangle, with the side most of its corners' weight lies on. None for a shape all
/// on one side.
type Sever = (Vec<(u8, Vec<u32>)>, Vec<u8>, Vec<Option<u16>>);
fn sever_groups(rig: &ActorRig, shape: &LimbShape, sides: &[(u16, HashSet<String>)], exploded: &BTreeSet<u16>, severed: &BTreeSet<u16>) -> Option<Sever> {
    let (joints, weights) = skin_weights(&shape.cpu)?;
    let side: Vec<u8> = shape
        .bones
        .iter()
        .map(|b| {
            let b = b.to_ascii_lowercase();
            sides.iter().position(|(_, names)| names.contains(&b)).map_or(0, |k| k as u8 + 1)
        })
        .collect();
    if !side.contains(&0) || side.iter().all(|&s| s == 0) {
        return None;
    }
    let n = sides.len() + 1;
    let joint_depth = |j: usize| node(rig, &shape.bones[j]).map(|i| depth(rig, i));
    let mut fallback = vec![None; n];
    for (k, f) in fallback.iter_mut().enumerate().skip(1) {
        *f = (0..side.len()).filter(|&j| side[j] as usize == k).filter_map(|j| Some((joint_depth(j)?, j))).min().map(|(_, j)| j as u16);
    }
    // The body's: the bone above the first piece's top, or the one above that, …
    let top = sides.iter().flat_map(|(_, names)| names.iter()).filter_map(|b| node(rig, b)).min_by_key(|&i| depth(rig, i));
    let mut cur = top.and_then(|t| rig.skel.parents[t]);
    while let Some(c) = cur {
        if let Some(j) = shape.bones.iter().position(|b| rig.skel.names[c].as_deref().is_some_and(|n| n.eq_ignore_ascii_case(b))) {
            fallback[0] = Some(j as u16);
            break;
        }
        cur = rig.skel.parents[c];
    }
    let mut groups: Vec<(u8, Vec<u32>)> = (0..n as u8).map(|k| (k, Vec::new())).collect();
    if shape.partitions.is_empty() {
        let tris: Vec<u32> = match shape.cpu.indices() {
            Some(i) => i.iter().map(|i| i as u32).collect(),
            None => (0..shape.cpu.count_vertices() as u32).collect(),
        };
        for t in tris.as_chunks::<3>().0 {
            let mut on = vec![0.0f32; n];
            for &v in t {
                let (Some(j), Some(w)) = (joints.get(v as usize), weights.get(v as usize)) else { continue };
                for c in 0..4 {
                    on[side.get(j[c] as usize).copied().unwrap_or(0) as usize] += w[c];
                }
            }
            let k = (0..n).max_by(|&a, &b| on[a].total_cmp(&on[b])).unwrap_or(0);
            groups[k].1.extend_from_slice(t);
        }
    } else {
        let locations: Vec<u16> = sides.iter().map(|s| s.0).collect();
        for p in shape.partitions.iter() {
            match piece_of(p.body_part, p.cap, &locations).filter(|&k| fallback[k as usize].is_some()) {
                Some(k) => groups[k as usize].1.extend_from_slice(&p.indices),
                None if partition_drawn(p.body_part, p.cap, exploded, severed) => groups[0].1.extend_from_slice(&p.indices),
                None => {}
            }
        }
    }
    // Nothing leaves and no body triangle reaches into a piece: drawn as before.
    let reaches = |v: &u32| joints[*v as usize].iter().zip(weights[*v as usize]).any(|(&j, w)| w > 0.0 && side.get(j as usize) != Some(&0));
    if groups.iter().skip(1).all(|g| g.1.is_empty()) && !groups[0].1.iter().any(reaches) {
        return None;
    }
    Some((groups, side, fallback))
}

/// The nodes of the part at `location`.
fn part_nodes(rig: &ActorRig, data: &BodyPartData, location: i32) -> Vec<usize> {
    data.parts.iter().filter(|p| p.location() == location).filter_map(|p| node(rig, p.node.as_deref()?)).flat_map(|n| subtree(rig, n)).collect()
}

/// Whether a partition of body part `bp` is drawn, given the exploded and severed
/// locations: see the module notes.
pub fn partition_drawn(bp: u16, cap: bool, exploded: &BTreeSet<u16>, severed: &BTreeSet<u16>) -> bool {
    let torso_gone = exploded.contains(&0);
    if torso_gone {
        return false;
    }
    if cap {
        return match bp {
            201..=213 => exploded.contains(&(bp - 200)) || severed.contains(&(bp - 200)),
            101..=113 => severed.contains(&(bp - 100)),
            _ => false,
        };
    }
    match bp {
        0..=13 => !exploded.contains(&bp),
        1000.. => !exploded.contains(&(bp / 1000)),
        _ => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_skins(
    mut commands: Commands,
    script: Option<Res<ScriptState>>,
    combat: Res<Combat>,
    rigs: Query<(&ActorRig, Option<&LimbRig>)>,
    added: Query<&RefVisual, Added<LimbShape>>,
    mut shapes: Query<(Entity, &RefVisual, &LimbShape, &mut Mesh3d, Option<&CutMesh>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut gore: ResMut<Gore>,
) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let mut respawned: HashSet<u32> = added.iter().map(|r| r.0).collect();
    // A redrawn shape given another mesh since (`CutMesh`): its actor is drawn again.
    for (_, r, _, mesh, cut) in &shapes {
        if cut.is_some_and(|c| c.0 != mesh.0.id()) && respawned.insert(r.0) {
            info!("dismember: {:08X}'s shape was given its whole mesh back: drawn without its gone parts again", r.0);
        }
    }
    let mut todo = Vec::new();
    for (rig, limb) in &rigs {
        let gone = game.limbs_gone(FormId(rig.reference));
        let changed = gore.applied.get(&rig.reference).map_or(!gone.is_empty(), |a| *a != gone);
        if changed || (respawned.contains(&rig.reference) && !gone.is_empty()) {
            todo.push((rig, limb, gone));
        }
    }
    for (rig, limb, gone) in todo {
        let Some(data) = body_parts(&combat, rig, limb) else { continue };
        let exploded: BTreeSet<u16> = gone.iter().filter(|g| g.1).map(|g| g.0 as u16).collect();
        let severed: BTreeSet<u16> = gone.iter().filter(|g| !g.1).map(|g| g.0 as u16).collect();
        let blown: HashSet<String> = exploded.iter().flat_map(|&l| part_names(rig, data, l)).collect();
        // The severed pieces, side 1, 2, … (0 is the body), and the bones each carries.
        let sides: Vec<(u16, HashSet<String>)> = severed.iter().map(|&l| (l, part_names(rig, data, l))).collect();
        let torso_gone = exploded.contains(&0);
        let mut redrawn = 0;
        let mut cut = (0, 0);
        for (e, r, shape, mut mesh, _) in &mut shapes {
            if r.0 != rig.reference {
                continue;
            }
            if let Some((groups, side, fallback)) = sever_groups(rig, shape, &sides, &exploded, &severed).filter(|_| !torso_gone) {
                cut.0 += 1;
                cut.1 += groups.iter().filter(|g| g.0 > 0).map(|g| g.1.len() / 3).sum::<usize>();
                mesh.0 = meshes.add(sever_mesh(&shape.cpu, &groups, &side, &fallback));
                commands.entity(e).try_insert(CutMesh(mesh.0.id()));
                redrawn += 1;
                continue;
            }
            let new = if shape.partitions.is_empty() {
                let all_blown = !shape.bones.is_empty() && shape.bones.iter().all(|b| blown.contains(&b.to_ascii_lowercase()));
                if !(torso_gone || all_blown) {
                    continue;
                }
                dismember_mesh(&shape.cpu, &[], |_| false)
            } else {
                // By each partition's own flag: a cap partition may share a shape with
                // drawn ones (dismemberment.md §2.5).
                let keep = |p: &nvf_assets::actor::Partition| partition_drawn(p.body_part, p.cap, &exploded, &severed);
                if !shape.partitions.iter().any(|p| p.cap) && shape.partitions.iter().all(keep) {
                    continue;
                }
                dismember_mesh(&shape.cpu, &shape.partitions, keep)
            };
            mesh.0 = meshes.add(new);
            commands.entity(e).try_insert(CutMesh(mesh.0.id()));
            redrawn += 1;
        }
        if cut.0 > 0 {
            info!("dismember: {:08X} cut apart for severed {:?}: {} shapes, {} triangles leave with the pieces", rig.reference, severed.iter().collect::<Vec<_>>(), cut.0, cut.1);
        }
        gore.shapes_redrawn += redrawn;
        debug!("dismember: {:08X} drawn without {:?}, with stumps for {:?}: {redrawn} shapes", rig.reference, exploded, severed);
        gore.applied.insert(rig.reference, gone);
    }
}

/// Cut each severed part's ragdoll bodies from the rest (idempotent; again after the
/// ragdoll is built anew).
fn cut_ragdolls(script: Option<Res<ScriptState>>, combat: Res<Combat>, corpses: Query<(&ActorRig, &Corpse, Option<&LimbRig>)>, mut phys: ResMut<PhysState>, mut gore: ResMut<Gore>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    for (rig, corpse, limb) in &corpses {
        if !corpse.live {
            continue;
        }
        let gone = game.limbs_gone(FormId(rig.reference));
        if !gone.iter().any(|g| !g.1) {
            continue;
        }
        let Some(data) = body_parts(&combat, rig, limb) else { continue };
        for &(location, _) in gone.iter().filter(|g| !g.1) {
            let nodes = part_nodes(rig, data, location);
            let bodies: Vec<usize> = corpse.body_bones().iter().enumerate().filter(|(_, b)| nodes.contains(b)).map(|(i, _)| i).collect();
            if bodies.is_empty() {
                continue;
            }
            let n = phys.world.cut_ragdoll(rig.reference, &bodies);
            if n > 0 {
                gore.joints_cut += n as u32;
                info!("dismember: {:08X} part at location {location} cut from its ragdoll ({n} joint(s))", rig.reference);
            }
        }
    }
}

/// A unit vector in a random direction.
fn random_dir(rng: &mut SplitMix) -> Vec3 {
    let z = rng.next_f32() * 2.0 - 1.0;
    let a = rng.next_f32() * std::f32::consts::TAU;
    let r = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(r * a.cos(), r * a.sin(), z)
}

/// A game-space pose of a node's world transform (Bevy space).
fn game_pose(g: &GlobalTransform) -> (Vec3, Mat3) {
    let a = g.affine();
    let col = |c: bevy::math::Vec3A| Vec3::from(space::to_game(Vec3::from(c))).normalize_or_zero();
    (Vec3::from(space::to_game(Vec3::from(a.translation))), Mat3::from_cols(col(a.matrix3.x_axis), col(a.matrix3.y_axis), col(a.matrix3.z_axis)))
}

/// Row-major 3×3 of a rotation.
fn rows(m: Mat3) -> [f32; 9] {
    m.transpose().to_cols_array()
}

#[allow(clippy::too_many_arguments)]
fn throw_gore(
    mut commands: Commands,
    mut events: MessageReader<Dismembered>,
    combat: Res<Combat>,
    sim: Res<nvf_play::sim::SimState>,
    sfx: Option<Res<Sfx>>,
    rigs: Query<&ActorRig>,
    globals: Query<&GlobalTransform>,
    root: Query<Entity, With<WorldRoot>>,
    mut models: ModelCtx,
    mut phys: ResMut<PhysState>,
    (mut explosions, mut impacts, mut sounds): (MessageWriter<ExplosionFx>, MessageWriter<ImpactFx>, MessageWriter<PlaySound>),
    mut gore: ResMut<Gore>,
    script: Option<Res<ScriptState>>,
) {
    let mut due: Vec<(Dismembered, u8)> = std::mem::take(&mut gore.waiting);
    due.extend(events.read().map(|e| (*e, 0)));
    let Ok(world_root) = root.single() else { return };
    for (e, tries) in due {
        let Some(data) = combat.forms.body_parts.get(&e.data) else { continue };
        let Some(part) = data.parts.get(e.part) else { continue };
        let rig = rigs.iter().find(|r| r.reference == e.form_id);
        // The gore bone (`NAM4`, else the part's node), else the hit's point. The blood
        // spout and the explosion go at the bone's gore offset, in the bone's axes
        // [documented: GECK wiki Body Part, "translation of the blood spurt relative to the
        // Target Bone"]. The replacement model goes at the bone itself, in the actor's own
        // axes as they stand in the bind pose: its pieces are modelled that way, the leg's
        // hanging 15, 41 and 59 units below its origin, the head's just above it
        // [observed: `fnvtool nif decode` of GoreLegGore01.NIF and HeadBits01.NIF]. Turned
        // as the bone has turned from its bind pose, so a lying corpse's leg bits lie along
        // its leg, not down into the floor [design]. A gore bone the skeleton lacks falls
        // back to the part's node: NVGecko's forelegs name a file there,
        // `BASE Meshes\creatures\NVgiantrat\skeleton.nif` [observed: FalloutNV.esm BPTD].
        let bone = rig.and_then(|r| {
            let n = part.gore_target_bone.as_deref().and_then(|b| node(r, b)).or_else(|| node(r, part.node.as_deref()?))?;
            Some((globals.get(r.nodes[n]).ok()?, Mat3::from(r.skel.bind_world.get(n)?.matrix3)))
        });
        let upright = rig.and_then(|r| globals.get(r.nodes[0]).ok()).map_or(Mat3::IDENTITY, |g| game_pose(g).1);
        let (origin, at, frame) = match bone {
            Some((g, bind)) => {
                let (p, m) = game_pose(g);
                let bind = Mat3::from_cols(bind.x_axis.normalize_or_zero(), bind.y_axis.normalize_or_zero(), bind.z_axis.normalize_or_zero());
                let frame = if bind.determinant().abs() > 0.5 { m * bind.inverse() } else { upright };
                (p, p + m * Vec3::from(part.gore_translate), frame)
            }
            None => (Vec3::from(e.point), Vec3::from(e.point), upright),
        };
        let at_a = at.to_array();
        let (explosion, debris, count, scale, set, decals) = if e.exploded {
            (part.explodable_explosion, part.explodable_debris, part.explodable_debris_count, part.explodable_debris_scale, part.explodable_impact_dataset, part.explodable_decal_count)
        } else {
            (part.severable_explosion, part.severable_debris, part.severable_debris_count, part.severable_debris_scale, part.severable_impact_dataset, part.severable_decal_count)
        };
        // The pieces' models first: wait (a few frames at most) for one not loaded yet.
        let replacement = part.limb_replacement_model.as_deref().filter(|_| e.exploded);
        let debris_models: Vec<(u8, String)> = debris.and_then(|d| combat.forms.debris.get(&d)).map(|m| m.iter().map(|m| (m.percent, m.model.clone())).collect()).unwrap_or_default();
        let mut wanted: Vec<(String, f32)> = Vec::new();
        if let Some(m) = replacement {
            wanted.push((m.to_string(), if part.limb_replacement_scale > 0.0 { part.limb_replacement_scale } else { 1.0 }));
        }
        let total: u32 = debris_models.iter().map(|m| m.0 as u32).sum();
        for _ in 0..count.min(16) {
            let mut pick = gore.rng.next_f32() * total as f32;
            if let Some((_, m)) = debris_models.iter().find(|m| {
                pick -= m.0 as f32;
                pick < 0.0
            }) {
                wanted.push((m.clone(), if scale > 0.0 { scale } else { 1.0 }));
            }
        }
        let loaded: Vec<Option<Arc<PreparedModel>>> = wanted.iter().map(|(m, _)| models.model(&fnv_actor::resolve::mesh_path(m))).collect();
        if loaded.iter().any(Option::is_none) && tries < 3 {
            gore.waiting.push((e, tries + 1));
            continue;
        }
        // The explosion (visual and sound) and the blood.
        if let Some(x) = explosion {
            explosions.write(ExplosionFx { explosion: x, point: at_a, cause: None, normal: None, look: ExplosionLook::Own });
            if let Some(ex) = sfx.as_deref().and_then(|s| s.forms.explosions.get(&x)) {
                for sound in ex.sound_1.into_iter().chain(ex.sound_2) {
                    sounds.write(PlaySound { sound, at: Some(at_a) });
                }
            }
        }
        if let (Some(set), Some(s)) = (set, sfx.as_deref()) {
            // The impact material of the actor's Model/Animation source, not the placed
            // base's (docs/spec-notes/actors.md §1.3).
            let r = FormId(e.form_id);
            let base = sim.base_of(e.form_id).map(|b| script.as_deref().and_then(|s| s.rt.as_ref()).map_or(b, |rt| { let (seed, at) = sim.pick(&rt.game, r); rt.game.data.model_base_at(b, seed, at) }));
            let material = base.and_then(|b| s.forms.actor_material(b)).unwrap_or("organic");
            if let Some(impact) = s.forms.impact_id(set, material) {
                for k in 0..decals.min(MAX_BLOOD) {
                    let d = (random_dir(&mut gore.rng) + Vec3::from(e.dir) * 0.5).normalize_or_zero();
                    impacts.write(ImpactFx { impact, shooter: FormId(e.form_id), point: (at + d * 4.0).to_array(), normal: (-d).to_array(), dir: d.to_array(), actor: None, no_spray: false, no_decal: false });
                    if k == 0 {
                        if let Some(i) = s.forms.impacts.get(&impact) {
                            for sound in i.sound_1.into_iter().chain(i.sound_2) {
                                sounds.write(PlaySound { sound, at: Some(at_a) });
                            }
                        }
                    }
                }
            }
        }
        // The pieces, at the size the actor is drawn: the reference's scale times a
        // creature's base scale (#1587: giant ants threw pieces at a full ant's size).
        let actor_scale = sim.ref_scale(e.form_id) * rig.map_or(1.0, |r| r.base_scale);
        let (thrown, farthest, lowest) = throw_pieces(&mut commands, world_root, &models.meshes, &mut phys, &mut gore, e.form_id, origin, frame, actor_scale, &wanted, &loaded, Vec3::from(e.dir));
        gore.thrown += thrown;
        gore.landed.entry(e.form_id).or_default().0 += thrown;
        while gore.pieces.len() > MAX_PIECES {
            let (id, ent) = gore.pieces.remove(0);
            phys.world.remove_gore(id);
            commands.entity(ent).despawn();
        }
        let what = if e.exploded { "exploded" } else { "severed" };
        info!(
            "dismember: gore for {:08X} {} {what}: {thrown} pieces{}{} at ({:.0}, {:.0}, {:.0}); the pieces start within {farthest:.0} units of {} at ({:.0}, {:.0}, {:.0}){}",
            e.form_id,
            part.name.as_deref().unwrap_or("part"),
            replacement.map_or(String::new(), |m| format!(" ({m} and {} debris)", wanted.len() - 1)),
            if explosion.is_some() { ", explosion" } else { "" },
            at.x,
            at.y,
            at.z,
            part.gore_target_bone.as_deref().or(part.node.as_deref()).unwrap_or("the hit"),
            origin.x,
            origin.y,
            origin.z,
            if thrown > 0 { format!(", the lowest at z {lowest:.0}") } else { String::new() }
        );
    }
}

/// Throw the pieces of `wanted` (model path and scale, `loaded` their models) as physics
/// bodies of `owner`'s: placed at `origin` in `frame` times `scale`, flying out along
/// `dir` and up, spinning. The pieces thrown, the farthest one's start from `origin` and
/// the lowest one's height.
#[allow(clippy::too_many_arguments)]
fn throw_pieces(
    commands: &mut Commands,
    world_root: Entity,
    meshes: &Assets<Mesh>,
    phys: &mut PhysState,
    gore: &mut Gore,
    owner: u32,
    origin: Vec3,
    frame: Mat3,
    scale: f32,
    wanted: &[(String, f32)],
    loaded: &[Option<Arc<PreparedModel>>],
    dir: Vec3,
) -> (u32, f32, f32) {
    let mut thrown = 0;
    let mut farthest = 0.0f32;
    let mut lowest = f32::MAX;
    for ((path, s), pm) in wanted.iter().zip(loaded) {
        let Some(pm) = pm else { continue };
        let place = Affine3A::from_mat3_translation(frame * scale * *s, origin);
        for (piece, local, shapes) in pieces(pm) {
            let key = (path.to_ascii_lowercase(), piece);
            let (half, center) = *gore.boxes.entry(key).or_insert_with(|| piece_box(meshes, pm, &shapes));
            let world = place * local;
            let (sc, q, t) = world.to_scale_rotation_translation();
            let k = sc.x.max(0.01);
            let id = gore.next;
            gore.next = gore.next.wrapping_add(1).max(1);
            let d = (random_dir(&mut gore.rng) + dir * 0.6 + Vec3::Z * 0.6).normalize_or_zero();
            let speed = PIECE_SPEED.0 + (PIECE_SPEED.1 - PIECE_SPEED.0) * gore.rng.next_f32();
            let spin = random_dir(&mut gore.rng) * PIECE_SPIN * gore.rng.next_f32();
            phys.world.add_gore(&GoreStart {
                id,
                actor: owner,
                translation: t.to_array(),
                rotation: rows(Mat3::from_quat(q)),
                half_extents: (Vec3::from(half) * k).to_array(),
                center: (Vec3::from(center) * k).to_array(),
                linvel: (d * speed).to_array(),
                angvel: spin.to_array(),
                mass: PIECE_MASS,
            });
            let holder = commands.spawn((Transform::from_translation(t).with_rotation(q).with_scale(Vec3::splat(k)), Visibility::Inherited, ChildOf(world_root), GorePiece { id, actor: owner, settled: false })).id();
            farthest = farthest.max(t.distance(origin));
            lowest = lowest.min(t.z);
            for (rel, si) in shapes {
                let sh = &pm.shapes[si];
                commands.spawn((Mesh3d(sh.mesh.clone()), MeshMaterial3d(sh.material.clone()), Transform::from_matrix(Mat4::from(rel)), Visibility::Inherited, NotShadowCaster, ChildOf(holder)));
            }
            gore.pieces.push((id, holder));
            thrown += 1;
        }
    }
    (thrown, farthest, lowest)
}

/// A destroyed object's debris (a destruction stage's DEBR, `nvf_play::combat::
/// ObjectDebris`): `count` pieces, each model picked by its DEBR percentage, thrown from
/// the object's middle as the gore of a dismembered body is [design: the gore's speeds
/// and spin; docs/spec-notes/destruction-stages.md §4]. A debris model not loaded yet
/// waits a few frames.
#[allow(clippy::too_many_arguments)]
fn throw_object_debris(
    mut commands: Commands,
    mut combat: ResMut<Combat>,
    root: Query<Entity, With<WorldRoot>>,
    mut models: ModelCtx,
    mut phys: ResMut<PhysState>,
    mut gore: ResMut<Gore>,
    mut waiting: Local<Vec<(nvf_play::combat::ObjectDebris, u8)>>,
) {
    let mut due: Vec<(nvf_play::combat::ObjectDebris, u8)> = std::mem::take(&mut *waiting);
    due.extend(std::mem::take(&mut combat.debris).into_iter().map(|d| (d, 0)));
    let Ok(world_root) = root.single() else { return };
    for (d, tries) in due {
        let list: Vec<(u8, String)> = combat.forms.debris.get(&d.debris).map(|m| m.iter().map(|m| (m.percent, m.model.clone())).collect()).unwrap_or_default();
        let total: u32 = list.iter().map(|m| m.0 as u32).sum();
        let mut wanted: Vec<(String, f32)> = Vec::new();
        for _ in 0..d.count.clamp(0, 16) {
            let mut pick = gore.rng.next_f32() * total as f32;
            if let Some((_, m)) = list.iter().find(|m| {
                pick -= m.0 as f32;
                pick < 0.0
            }) {
                wanted.push((m.clone(), 1.0));
            }
        }
        let loaded: Vec<Option<Arc<PreparedModel>>> = wanted.iter().map(|(m, _)| models.model(&fnv_actor::resolve::mesh_path(m))).collect();
        if loaded.iter().any(Option::is_none) && tries < 3 {
            waiting.push((d, tries + 1));
            continue;
        }
        let at = Vec3::from(d.at);
        let (thrown, farthest, _) = throw_pieces(&mut commands, world_root, &models.meshes, &mut phys, &mut gore, d.reference.0, at, Mat3::IDENTITY, 1.0, &wanted, &loaded, Vec3::ZERO);
        gore.thrown += thrown;
        while gore.pieces.len() > MAX_PIECES {
            let (id, ent) = gore.pieces.remove(0);
            phys.world.remove_gore(id);
            commands.entity(ent).despawn();
        }
        info!("destroyed: {} throws {thrown} debris pieces of {} (within {farthest:.0} units of ({:.0}, {:.0}, {:.0}))", d.reference, d.debris, at.x, at.y, at.z);
    }
}

/// The pieces of a gore model: each node right below the root that has shapes under it
/// (`HeadBits01.NIF`'s Eyeball, JawBoneBloody01, …), else the whole model as one. Each
/// piece: its node, its transform in the model (root included), and its shapes with
/// their transforms relative to it.
fn pieces(pm: &PreparedModel) -> Vec<Piece> {
    let n = pm.nodes.len();
    let mut model: Vec<Affine3A> = Vec::with_capacity(n);
    for nd in &pm.nodes {
        let parent = nd.parent.map_or(pm.root, |p| model[p]);
        model.push(parent * nd.local);
    }
    let top = |mut i: usize| {
        while let Some(p) = pm.nodes[i].parent {
            i = p;
        }
        i
    };
    let mut out: Vec<Piece> = Vec::new();
    let mut loose: Vec<(Affine3A, usize)> = Vec::new();
    for (si, s) in pm.shapes.iter().enumerate() {
        match s.node {
            Some(nd) => {
                let t = top(nd);
                let rel = model[t].inverse() * model[nd] * s.own;
                match out.iter_mut().find(|p| p.0 == Some(t)) {
                    Some(p) => p.2.push((rel, si)),
                    None => out.push((Some(t), model[t], vec![(rel, si)])),
                }
            }
            None => loose.push((pm.root * s.own, si)),
        }
    }
    if !loose.is_empty() {
        out.push((None, Affine3A::IDENTITY, loose));
    }
    out
}

/// A piece's box: the bounds of its shapes' vertices in the piece's frame (half extents
/// and centre), at least half a unit; 2 units when no vertex is at hand.
fn piece_box(meshes: &Assets<Mesh>, pm: &PreparedModel, shapes: &[(Affine3A, usize)]) -> PieceBox {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for (rel, si) in shapes {
        let Some(m) = meshes.get(&pm.shapes[*si].mesh) else { continue };
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(ps)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { continue };
        for p in ps {
            let v = rel.transform_point3(Vec3::from(*p));
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    if lo.x > hi.x {
        return ([2.0; 3], [0.0; 3]);
    }
    (((hi - lo) * 0.5).max(Vec3::splat(0.5)).to_array(), ((hi + lo) * 0.5).to_array())
}

/// Pieces follow their bodies; a piece whose actor is no longer loaded goes.
/// When all of an actor's pieces have come to rest, one log line tells how far from its
/// corpse (the mass centre of its ragdoll's trunk) they lie.
fn move_pieces(mut commands: Commands, mut pieces: Query<(Entity, &mut GorePiece, &mut Transform)>, rigs: Query<&ActorRig>, mut phys: ResMut<PhysState>, mut gore: ResMut<Gore>) {
    if gore.pieces.is_empty() {
        return;
    }
    let loaded: HashSet<u32> = rigs.iter().map(|r| r.reference).collect();
    let mut gone = Vec::new();
    for (e, mut p, mut t) in &mut pieces {
        if !loaded.contains(&p.actor) {
            gone.push((p.id, e));
            continue;
        }
        let Some(s) = phys.world.gore_state(p.id) else { continue };
        t.translation = Vec3::from(s.translation);
        t.rotation = Quat::from_mat3(&Mat3::from_cols_array(&s.rotation).transpose());
        let still = s.sleeping || Vec3::from(s.linvel).length() < 1.0;
        if p.settled || !still {
            continue;
        }
        p.settled = true;
        let Some(corpse) = phys.world.ragdoll_center(p.actor) else { continue };
        let entry = gore.landed.entry(p.actor).or_default();
        entry.1.push(t.translation.distance(Vec3::from(corpse)));
        if entry.1.len() as u32 >= entry.0 {
            let mut d = std::mem::take(&mut entry.1);
            d.sort_by(f32::total_cmp);
            info!(
                "dismember: gore of {:08X} at rest: {} pieces lie {:.0} to {:.0} units from the corpse's trunk (median {:.0}) at ({:.0}, {:.0}, {:.0})",
                p.actor,
                d.len(),
                d[0],
                d[d.len() - 1],
                d[d.len() / 2],
                corpse[0],
                corpse[1],
                corpse[2]
            );
            gore.landed.remove(&p.actor);
        }
    }
    for (id, e) in gone {
        phys.world.remove_gore(id);
        commands.entity(e).despawn();
        gore.pieces.retain(|p| p.0 != id);
    }
}

fn report(mut exit: MessageReader<AppExit>, gore: Res<Gore>) {
    if exit.read().next().is_some() && (gore.thrown > 0 || gore.shapes_redrawn > 0) {
        info!("dismember summary: {} shapes redrawn, {} ragdoll joints cut, {} gore pieces thrown, {} still out", gore.shapes_redrawn, gore.joints_cut, gore.thrown, gore.pieces.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitions_follow_gone_parts() {
        let none = BTreeSet::new();
        let left_arm: BTreeSet<u16> = [3].into();
        // An exploded left arm leaves the body with its torso section; its torso cap shows.
        assert!(!partition_drawn(3, false, &left_arm, &none));
        assert!(!partition_drawn(3000, false, &left_arm, &none));
        assert!(partition_drawn(0, false, &left_arm, &none));
        assert!(partition_drawn(5000, false, &left_arm, &none));
        assert!(partition_drawn(203, true, &left_arm, &none));
        assert!(!partition_drawn(103, true, &left_arm, &none));
        assert!(!partition_drawn(205, true, &left_arm, &none));
        // A severed one keeps its skin (it leaves with its bones) and shows both caps.
        assert!(partition_drawn(3, false, &none, &left_arm));
        assert!(partition_drawn(203, true, &none, &left_arm));
        assert!(partition_drawn(103, true, &none, &left_arm));
        // What leaves with a severed head (side 1) and left arm (side 2).
        assert_eq!(piece_of(1, false, &[1, 3]), Some(1));
        assert_eq!(piece_of(3000, false, &[1, 3]), Some(2));
        assert_eq!(piece_of(101, true, &[1, 3]), Some(1));
        assert_eq!(piece_of(201, true, &[1, 3]), None);
        assert_eq!(piece_of(0, false, &[1, 3]), None);
        // An intact actor: no caps.
        assert!(!partition_drawn(201, true, &none, &none));
        // An exploded torso takes everything.
        let torso: BTreeSet<u16> = [0].into();
        assert!(!partition_drawn(5, false, &torso, &none) && !partition_drawn(203, true, &torso, &left_arm));
    }
}
