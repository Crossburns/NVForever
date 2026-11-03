//! Swinging traps (docs/spec-notes/traps.md §4): the swinging log, I-beams, engine block
//! and brahmin corpses hang their weight from fixed anchors on chains of Havok bodies
//! joined by the model's constraints, held where the file draws them until the trap is
//! let go.
//!
//! - **Drawn**: an activator whose model joins its bodies gets its own nodes and a
//!   [`TrapRig`] (world.rs; `nvf_assets::model::PreparedModel::jointed`). Until it is let
//!   go its collision is the streamer's, held in place (physics.md §4.3.1).
//! - **Let go** when the game has it live (its script's first `trapUpdate`,
//!   `fnv_game::swing_traps`): the held collision stops taking part
//!   (`set_ref_keyframed_solid`) and the model's bodies become an object ragdoll
//!   (`PhysWorld::add_object_ragdoll`) at their nodes' poses: the anchors fixed, the rest
//!   falling and swinging on the constraints. From then on each moving body's pose is
//!   written to its node.
//! - **Hits**: a body moving faster than [`MIN_REPORTED_SPEED`] whose bounding sphere
//!   meets an actor's hit capsule (or the player's) is reported ([`TrapHit`]); the trap's
//!   own variables decide what it does (`nvf_play::swing_traps`) [design: the overlap
//!   test stands in for Havok's contact; the ragdoll does not push characters].
//! - A trap whose rig leaves the world (its cell unloads) loses its ragdoll; drawn again,
//!   it hangs where the file draws it [design].
//!
//! Log: `swing traps: <ref> let go: <n> bodies (<m> moving), <j> joints`.

use crate::ragdoll::{bevy_affine, game_iso};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_collide::ragdoll::Ragdoll;
use fnv_collide::{Iso, Motion};
use fnv_nif::scene::Decoded;
use fnv_nif::NifFile;
use fnv_phys::RagdollStart;
use fnv_sim::FormId;
use nvf_assets::data::GameData;
use nvf_assets::guard::guarded;
use nvf_play::phys::PhysState;
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::swing_traps::TrapHit;
use std::collections::HashMap;
use std::sync::Arc;

/// Bodies slower than this are not looked at for hits, game units a second [design: below
/// any trap's `fTrapMinVelocity`, 120 in the masters].
pub const MIN_REPORTED_SPEED: f32 = 60.0;
/// The player's body as a capsule for the hits: radius, game units [design].
const PLAYER_RADIUS: f32 = 20.0;
/// A trap's swing is logged this many times, a second apart.
const SWING_LOGS: u32 = 8;

/// On the root of a drawn jointed activator: its reference, model and nodes by name.
#[derive(Component)]
pub struct TrapRig {
    pub form_id: u32,
    pub path: String,
    pub nodes: HashMap<String, Entity>,
}

/// A trap let go.
struct Swinging {
    root: Entity,
    data: Arc<Ragdoll>,
    /// Per body, the node entity it moves (dynamic bodies only).
    nodes: Vec<Option<Entity>>,
    /// Per body, its bounding sphere in its node's frame (centre, radius), unscaled.
    spheres: Vec<([f32; 3], f32)>,
    scale: f32,
    /// The heaviest moving body (the log, the beam): logged as it swings.
    weight: usize,
    /// Seconds to the next log line of its swing.
    log_in: f32,
    /// Lines left to log.
    logs: u32,
}

#[derive(Resource, Default)]
struct SwingTraps {
    swinging: HashMap<u32, Swinging>,
    /// The models' jointed bodies, by model path (read once).
    cache: HashMap<String, Option<Arc<Ragdoll>>>,
    let_go: u32,
    hits: u32,
}

pub struct SwingTrapsPlugin;

impl Plugin for SwingTrapsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwingTraps>()
            .add_systems(Update, (let_go, swing).chain().after(nvf_play::PlaySet::Scripts))
            .add_systems(Last, report);
    }
}

impl SwingTraps {
    fn data(&mut self, data: &GameData, path: &str) -> Option<Arc<Ragdoll>> {
        let key = fnv_vfs::normalize(path);
        self.cache
            .entry(key.clone())
            .or_insert_with(|| {
                let bytes = data.read(&key)?;
                let read = guarded(|| {
                    let file = NifFile::parse(&bytes, Some(&data.schema)).map_err(|e| e.to_string())?;
                    let dec = Decoded::new(&file, &bytes, &data.schema);
                    Ok(fnv_collide::ragdoll::ragdoll(&file, &dec))
                });
                match read {
                    Ok(r) if !r.joints.is_empty() => Some(Arc::new(r)),
                    Ok(_) => None,
                    Err(e) => {
                        warn!("swing traps: {key}: not read ({e})");
                        None
                    }
                }
            })
            .clone()
    }
}

/// The bounding sphere of a body's shape in its node's frame.
fn sphere(b: &fnv_collide::ragdoll::RagdollBody) -> ([f32; 3], f32) {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    b.object.shape.bound_points(&b.object.body_transform, &mut |p| {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    });
    if !lo[0].is_finite() {
        return ([0.0; 3], 0.0);
    }
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let r = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() / 2.0;
    (c, r)
}

/// Let go the traps the game has live.
#[allow(clippy::too_many_arguments)]
fn let_go(
    mut traps: ResMut<SwingTraps>,
    script: Option<Res<ScriptState>>,
    data: Option<Res<GameData>>,
    phys: Option<ResMut<PhysState>>,
    rigs: Query<(Entity, &TrapRig)>,
    globals: Query<&GlobalTransform>,
) {
    let (Some(script), Some(data), Some(mut phys)) = (script, data, phys) else { return };
    let Some(rt) = script.rt.as_ref() else { return };
    let live = rt.game.live_traps();
    // Rigs gone with their cells take their ragdolls along.
    let gone: Vec<u32> = traps.swinging.iter().filter(|(_, s)| rigs.get(s.root).is_err()).map(|(id, _)| *id).collect();
    for id in gone {
        info!("swing traps: {id:08X} is no longer drawn: its ragdoll goes");
        phys.world.remove_ragdoll(id);
        traps.swinging.remove(&id);
    }
    for t in live {
        if traps.swinging.contains_key(&t.0) {
            continue;
        }
        let Some((root, rig)) = rigs.iter().find(|(_, r)| r.form_id == t.0) else { continue };
        let Some(rag) = traps.data(&data, &rig.path) else { continue };
        let Ok(root_g) = globals.get(root) else { continue };
        let scale = root_g.affine().matrix3.x_axis.length();
        let node = |b: &fnv_collide::ragdoll::RagdollBody| b.object.node.as_ref().and_then(|n| rig.nodes.get(n)).copied();
        let poses: Option<Vec<Iso>> = rag.bodies.iter().map(|b| node(b).and_then(|e| globals.get(e).ok()).map(|g| game_iso(&g.affine()))).collect();
        let Some(poses) = poses else {
            warn!("swing traps: {:08X} {}: a body's node is not drawn; not let go", t.0, rig.path);
            traps.cache.insert(fnv_vfs::normalize(&rig.path), None);
            continue;
        };
        if !phys.ready_at(poses[0].translation) {
            continue;
        }
        phys.world.set_ref_keyframed_solid(t.0, false);
        let start = RagdollStart { form_id: t.0, ragdoll: &rag, scale, nodes: &poses, velocity: [0.0; 3], parts: &[], asleep: false };
        let r = match guarded(|| Ok(phys.world.add_object_ragdoll(&start))) {
            Ok(r) => r,
            Err(e) => {
                warn!("swing traps: {:08X}: not built ({e})", t.0);
                phys.world.remove_ragdoll(t.0);
                phys.world.set_ref_keyframed_solid(t.0, true);
                continue;
            }
        };
        let moving = rag.bodies.iter().filter(|b| b.object.motion == Motion::Dynamic).count();
        let name = |i: usize| rag.bodies.get(i).and_then(|b| b.object.node.clone()).unwrap_or_default();
        let freed: Vec<String> = r.freed.iter().filter_map(|&k| rag.joints.get(k)).map(|j| format!("{} - {}", name(j.a), name(j.b))).collect();
        let freed = if freed.is_empty() { String::new() } else { format!(" ({} started outside its limits and swings free of them: {})", freed.len(), freed.join(", ")) };
        info!("swing traps: {:08X} let go: {} bodies ({moving} moving), {} joints{freed}", t.0, r.bodies, r.joints);
        traps.let_go += 1;
        let nodes = rag.bodies.iter().map(|b| node(b).filter(|_| b.object.motion == Motion::Dynamic)).collect();
        let spheres = rag.bodies.iter().map(sphere).collect();
        let weight = rag.bodies.iter().enumerate().filter(|(_, b)| b.object.motion == Motion::Dynamic).max_by(|a, b| a.1.object.mass.total_cmp(&b.1.object.mass)).map_or(0, |(i, _)| i);
        traps.swinging.insert(t.0, Swinging { root, data: rag.clone(), nodes, spheres, scale, weight, log_in: 0.0, logs: SWING_LOGS });
    }
}

/// The let-go traps' bodies move their nodes, and meet actors.
#[allow(clippy::too_many_arguments)]
fn swing(
    time: Res<Time>,
    mut traps: ResMut<SwingTraps>,
    phys: Option<Res<PhysState>>,
    player: Option<Res<Player>>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
    mut hits: MessageWriter<TrapHit>,
) {
    let Some(phys) = phys else { return };
    let mut found = 0;
    let dt = time.delta_secs();
    for (&id, s) in traps.swinging.iter_mut() {
        let Some(bodies) = phys.world.ragdoll_nodes(id) else {
            if s.logs > 0 {
                info!("swing traps: {id:08X}: no ragdoll in the physics world");
                s.logs = 0;
            }
            continue;
        };
        // Its swing in the log: the heaviest body's place and speed, once a second.
        s.log_in -= dt;
        if s.logs > 0 && s.log_in <= 0.0 {
            if let Some(Some(b)) = bodies.get(s.weight) {
                let v = (b.linvel[0].powi(2) + b.linvel[1].powi(2) + b.linvel[2].powi(2)).sqrt();
                // How far each joint's two pivots have come apart (its stretch).
                let iso = |i: usize| bodies.get(i).copied().flatten().map(|b| Iso { translation: b.translation, rotation: b.rotation });
                let gaps: Vec<(f32, usize)> = s.data.joints.iter().enumerate().filter_map(|(k, j)| Some((j.pivot_gap(&iso(j.a)?, &iso(j.b)?), k))).collect();
                let (worst, wj) = gaps.iter().copied().fold((0.0f32, 0), |a, b| if b.0 > a.0 { b } else { a });
                let name = |i: usize| s.data.bodies.get(i).and_then(|b| b.object.node.clone()).unwrap_or_default();
                let wj = s.data.joints.get(wj).map(|j| format!("{} - {}", name(j.a), name(j.b))).unwrap_or_default();
                info!("swing traps: {id:08X} swings: body {} at ({:.0}, {:.0}, {:.0}), {v:.0} units/s; joints apart by at most {worst:.1} units ({wj})", s.weight, b.translation[0], b.translation[1], b.translation[2]);
            }
            s.log_in = 1.0;
            s.logs -= 1;
        }
        follow_bodies(&bodies, &s.nodes, s.scale, s.root, &parents, &globals, &mut transforms);
        for (i, b) in bodies.iter().enumerate() {
            let Some(b) = b else { continue };
            // A moving body meets actors.
            if s.nodes.get(i).copied().flatten().is_none() {
                continue;
            }
            let speed = (b.linvel[0].powi(2) + b.linvel[1].powi(2) + b.linvel[2].powi(2)).sqrt();
            if speed < MIN_REPORTED_SPEED {
                continue;
            }
            let (c, r) = s.spheres[i];
            let node = Iso { translation: b.translation, rotation: b.rotation };
            let centre = node.apply([c[0] * s.scale, c[1] * s.scale, c[2] * s.scale]);
            let reach = r * s.scale;
            for actor in phys.world.actor_volumes() {
                let Some(v) = phys.world.actor_volume(actor) else { continue };
                if v.dead {
                    continue;
                }
                let h = v.height.max(2.0 * v.radius);
                if capsule_gap(centre, v.feet, v.radius, h) <= reach {
                    hits.write(TrapHit { trap: FormId(id), actor: FormId(actor), speed });
                    found += 1;
                }
            }
            if let Some(p) = player.as_deref() {
                if capsule_gap(centre, p.feet, PLAYER_RADIUS, p.eye_height + PLAYER_RADIUS) <= reach {
                    hits.write(TrapHit { trap: FormId(id), actor: FormId::PLAYER_REF, speed });
                    found += 1;
                }
            }
        }
    }
    traps.hits += found;
}

/// Put the nodes the bodies move (`nodes`, per body; `None` for a body that moves none)
/// where their bodies are (`bodies`, `PhysWorld::ragdoll_nodes`), the rig drawn at
/// `scale` under `root`. A chain's links can be nodes inside each other: each one's place
/// under its parent is taken from where the parent is put this frame, not from its drawn
/// place of the last frame (which lags a frame, and the lags add up down the chain).
pub(crate) fn follow_bodies(
    bodies: &[Option<fnv_phys::BodyState>],
    nodes: &[Option<Entity>],
    scale: f32,
    root: Entity,
    parents: &Query<&ChildOf>,
    globals: &Query<&GlobalTransform>,
    transforms: &mut Query<&mut Transform>,
) {
    let wanted: HashMap<Entity, Affine3A> = bodies
        .iter()
        .enumerate()
        .filter_map(|(i, b)| Some((nodes.get(i).copied().flatten()?, bevy_affine(b.as_ref()?.translation, &b.as_ref()?.rotation, scale))))
        .collect();
    let mut placed: HashMap<Entity, Affine3A> = HashMap::new();
    for (&e, &g) in &wanted {
        let Some(parent) = parents.get(e).ok().map(|p| p.parent()) else { continue };
        let Some(pg) = global_now(parent, root, &wanted, &mut placed, parents, globals, transforms, 0) else { continue };
        if let Ok(mut t) = transforms.get_mut(e) {
            let (_, rotation, translation) = (pg.inverse() * g).to_scale_rotation_translation();
            if rotation.is_finite() && translation.is_finite() {
                t.translation = translation;
                t.rotation = rotation.normalize();
            }
        }
    }
}

/// Where node `e` is this frame (Bevy space): a node a body moves where the body puts
/// it, the trap's root where it is drawn, any other node under its parent's place this
/// frame by its own transform.
#[allow(clippy::too_many_arguments)]
fn global_now(
    e: Entity,
    root: Entity,
    wanted: &HashMap<Entity, Affine3A>,
    placed: &mut HashMap<Entity, Affine3A>,
    parents: &Query<&ChildOf>,
    globals: &Query<&GlobalTransform>,
    transforms: &Query<&mut Transform>,
    depth: u32,
) -> Option<Affine3A> {
    if let Some(&g) = wanted.get(&e).or(placed.get(&e)) {
        return Some(g);
    }
    let parent = parents.get(e).ok().map(|p| p.parent());
    let g = match parent {
        Some(p) if e != root && depth < 64 => global_now(p, root, wanted, placed, parents, globals, transforms, depth + 1)? * transforms.get(e).ok()?.compute_affine(),
        _ => globals.get(e).ok()?.affine(),
    };
    placed.insert(e, g);
    Some(g)
}

/// How far `p` is from the surface of an upright capsule standing on `feet` (negative
/// inside), game units.
pub fn capsule_gap(p: [f32; 3], feet: [f32; 3], radius: f32, height: f32) -> f32 {
    let lo = feet[2] + radius;
    let hi = (feet[2] + height - radius).max(lo);
    let z = p[2].clamp(lo, hi);
    let d = ((p[0] - feet[0]).powi(2) + (p[1] - feet[1]).powi(2) + (p[2] - z).powi(2)).sqrt();
    d - radius
}

fn report(mut exit: MessageReader<AppExit>, traps: Res<SwingTraps>) {
    if exit.read().next().is_none() {
        return;
    }
    info!("swing traps summary: {} let go, {} contacts reported", traps.let_go, traps.hits);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gap_to_an_upright_capsule() {
        // A capsule of radius 20, 120 high, on the origin.
        assert!((capsule_gap([50.0, 0.0, 60.0], [0.0; 3], 20.0, 120.0) - 30.0).abs() < 1e-4);
        // Above its top: from the upper cap's centre (z 100).
        assert!((capsule_gap([0.0, 0.0, 150.0], [0.0; 3], 20.0, 120.0) - 30.0).abs() < 1e-4);
        // Inside.
        assert!(capsule_gap([0.0, 0.0, 60.0], [0.0; 3], 20.0, 120.0) < 0.0);
    }
}
