//! Destroyed objects' loose parts (docs/spec-notes/destruction-stages.md §4, issue #1410):
//! a car's hulk (`vehicles\carhulk01.nif`, DMDL of `Car01`'s last stage) has its doors,
//! hood, trunk and chassis as dynamic Havok bodies without constraints, modelled where
//! the blast throws them: the doors out to the sides, the hood raised. Drawn still, they
//! hung in the air.
//!
//! - **Drawn**: a reference drawn with its destruction model (destroyed.rs) whose model
//!   has such loose parts gets its own nodes and a [`WreckRig`] (world.rs;
//!   `nvf_assets::model::PreparedModel::loose`). Its collision is built from the same
//!   model and held where it is drawn (`nvf_play::phys::PhysState::set_destruction_models`,
//!   `hold_jointed`).
//! - **Fall**: once that collision is in, the held bodies stop taking part
//!   (`set_ref_keyframed_solid`) and the model's bodies become an object ragdoll without
//!   joints (`PhysWorld::add_object_ragdoll`) at their nodes' poses, awake; each moving
//!   body's pose is written to its node from then on (`swing_traps::follow_bodies`)
//!   [hypothesis: the original hands them to Havok when the hulk swaps in, which the
//!   missing constraints and the thrown-out poses suggest; not seen in the original yet].
//! - The parts start still and do not hurt actors [design]; the stage's explosion
//!   pushes them as it pushes loose objects (`combat: explosion … loose object(s)
//!   pushed`). Calm for `ragdoll::CALM_TIME` as a corpse is, or still moving after
//!   [`MAX_FALLING`], the wreck is held at rest where it lies
//!   (`PhysWorld::set_ragdoll_at_rest`) until a blast or a push rouses it.
//! - A wreck whose rig leaves the world (its cell unloads, ClearDestruction) loses its
//!   ragdoll; drawn again, its parts fall again from the file's poses [design: where they
//!   came to rest is not kept]. A change of drawn model (a later stage with another
//!   DMDL) draws the reference anew (destroyed.rs despawns its entities), so its rig is
//!   a new one: the old ragdoll goes with the old rig and the new model's parts fall.
//!
//! Log: `wrecks: <ref> <model>: <n> parts fall (<m> moving)`, `wrecks: <ref> falling:
//! fastest part <v> units/s, <w> rad/s` (once a second), `wrecks: <ref> came to rest
//! after <s> s; farthest part <node> <d> units from its start (<dz> in height)` (or
//! `held at rest still moving after`).

use crate::ragdoll::game_iso;
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
use nvf_play::script::ScriptState;
use std::collections::HashMap;
use std::sync::Arc;

/// A wreck still moving this long after its parts fell is held at rest where it lies,
/// seconds [engineering choice, a cost guard as `ragdoll::MAX_AWAKE`].
const MAX_FALLING: f32 = 20.0;

/// A wreck's fall is logged once a second for this long.
const FALL_LOG_SECONDS: f32 = 15.0;

/// On the root of a drawn wreck: its reference, model and nodes by name.
#[derive(Component)]
pub struct WreckRig {
    pub form_id: u32,
    pub path: String,
    pub nodes: HashMap<String, Entity>,
}

/// A wreck whose parts fell.
struct Fallen {
    root: Entity,
    /// Per body, the node entity it moves (dynamic bodies only).
    nodes: Vec<Option<Entity>>,
    scale: f32,
    /// Seconds since its parts started to fall, until they come to rest.
    falling: Option<f32>,
    /// Seconds to the next log line of its fall.
    log_in: f32,
    /// Where each body's node started (game space), for the distance it fell.
    start: Vec<[f32; 3]>,
    /// Each body's node name, for the log.
    names: Vec<String>,
    /// `PhysState::refs_built` when its held collision was last set aside.
    built: u64,
    /// The bodies' poses at the start of the current calm window
    /// (`ragdoll::CALM_WINDOW`), seconds into it, and how long it has been calm.
    since: Vec<Option<Iso>>,
    window: f32,
    calm: f32,
}

#[derive(Resource, Default)]
struct Wrecks {
    fallen: HashMap<u32, Fallen>,
    /// The models' bodies, by model path (read once; `None`: not read).
    cache: HashMap<String, Option<Arc<Ragdoll>>>,
    count: u32,
}

pub struct WrecksPlugin;

impl Plugin for WrecksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wrecks>().add_systems(Update, (fall, follow).chain().after(nvf_play::PlaySet::Physics)).add_systems(Last, report);
    }
}

impl Wrecks {
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
                    Ok(r) if r.bodies.iter().any(|b| b.object.motion == Motion::Dynamic) => Some(Arc::new(r)),
                    Ok(_) => None,
                    Err(e) => {
                        warn!("wrecks: {key}: not read ({e})");
                        None
                    }
                }
            })
            .clone()
    }
}

/// Let the parts of the wrecks drawn with their destruction model fall, once their
/// collision is built from it.
#[allow(clippy::too_many_arguments)]
fn fall(
    mut wrecks: ResMut<Wrecks>,
    script: Option<Res<ScriptState>>,
    data: Option<Res<GameData>>,
    phys: Option<ResMut<PhysState>>,
    rigs: Query<(Entity, &WreckRig)>,
    globals: Query<&GlobalTransform>,
) {
    let (Some(script), Some(data), Some(mut phys)) = (script, data, phys) else { return };
    let Some(rt) = script.rt.as_ref() else { return };
    // Rigs gone with their cells (or drawn intact again) take their ragdolls along.
    let gone: Vec<u32> = wrecks.fallen.iter().filter(|(_, f)| rigs.get(f.root).is_err()).map(|(id, _)| *id).collect();
    for id in gone {
        info!("wrecks: {id:08X} is no longer drawn: its ragdoll goes");
        phys.world.remove_ragdoll(id);
        wrecks.fallen.remove(&id);
    }
    for (root, rig) in &rigs {
        let r = rig.form_id;
        if let Some(f) = wrecks.fallen.get_mut(&r) {
            if f.root == root {
                // Collision built again (a cell loaded under it) is held solid: set aside
                // again, looked at only after physics built some.
                if f.built != phys.refs_built {
                    phys.world.set_ref_keyframed_solid(r, false);
                    f.built = phys.refs_built;
                }
                continue;
            }
        }
        // The model the game has it destroyed to, and physics built its collision from.
        let model = |m: Option<&str>| m.and_then(fnv_world::placed::archive_path).map(|p| fnv_vfs::normalize(&p));
        let drawn = Some(fnv_vfs::normalize(&rig.path));
        if model(rt.game.destruction_model(FormId(r))) != drawn || model(phys.built_model(r)) != drawn {
            continue;
        }
        let Some(rag) = wrecks.data(&data, &rig.path) else { continue };
        let Ok(root_g) = globals.get(root) else { continue };
        let scale = root_g.affine().matrix3.x_axis.length();
        let node = |b: &fnv_collide::ragdoll::RagdollBody| b.object.node.as_ref().and_then(|n| rig.nodes.get(n)).copied();
        let poses: Option<Vec<Iso>> = rag.bodies.iter().map(|b| node(b).and_then(|e| globals.get(e).ok()).map(|g| game_iso(&g.affine()))).collect();
        let Some(poses) = poses else {
            warn!("wrecks: {r:08X} {}: a body's node is not drawn; its parts stay", rig.path);
            wrecks.cache.insert(fnv_vfs::normalize(&rig.path), None);
            continue;
        };
        if !phys.ready_at(poses[0].translation) {
            continue;
        }
        phys.world.set_ref_keyframed_solid(r, false);
        let start = RagdollStart { form_id: r, ragdoll: &rag, scale, nodes: &poses, velocity: [0.0; 3], parts: &[], asleep: false };
        let built = match guarded(|| Ok(phys.world.add_object_ragdoll(&start))) {
            Ok(b) => b,
            Err(e) => {
                warn!("wrecks: {r:08X}: not built ({e})");
                phys.world.remove_ragdoll(r);
                phys.world.set_ref_keyframed_solid(r, true);
                wrecks.cache.insert(fnv_vfs::normalize(&rig.path), None);
                continue;
            }
        };
        let moving = rag.bodies.iter().filter(|b| b.object.motion == Motion::Dynamic).count();
        info!("wrecks: {r:08X} {}: {} parts fall ({moving} moving)", rig.path, built.bodies);
        wrecks.count += 1;
        let nodes = rag.bodies.iter().map(|b| node(b).filter(|_| b.object.motion == Motion::Dynamic)).collect();
        let start = poses.iter().map(|p| p.translation).collect();
        let names = rag.bodies.iter().map(|b| b.object.node.clone().unwrap_or_default()).collect();
        wrecks.fallen.insert(r, Fallen { root, nodes, scale, falling: Some(0.0), log_in: 1.0, start, names, since: Vec::new(), window: 0.0, calm: 0.0, built: phys.refs_built });
    }
}

/// The fallen wrecks' bodies move their nodes.
fn follow(
    time: Res<Time>,
    mut wrecks: ResMut<Wrecks>,
    phys: Option<ResMut<PhysState>>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(mut phys) = phys else { return };
    let dt = time.delta_secs();
    for (&id, f) in wrecks.fallen.iter_mut() {
        let Some(bodies) = phys.world.ragdoll_nodes(id) else { continue };
        crate::swing_traps::follow_bodies(&bodies, &f.nodes, f.scale, f.root, &parents, &globals, &mut transforms);
        if f.falling.is_none() && !phys.world.ragdoll_asleep(id) {
            // Roused by a blast or a push: it falls again.
            (f.falling, f.calm, f.window) = (Some(0.0), 0.0, 0.0);
            f.since.clear();
        }
        // Calm as a corpse is (`ragdoll::CALM_SPEED`, `CALM_SPIN`, `CALM_TIME`): the parts'
        // net motion over short windows, not their velocities. A hubcap (`CarPort01`, a
        // thin 15-sided disc) rocked on its edges at up to 1.7 rad/s for 45 s without the
        // physics engine calling it asleep [observed: smoke wreck-car-parts-fall].
        let now: Vec<Option<Iso>> = bodies.iter().map(|b| b.as_ref().map(|b| Iso { translation: b.translation, rotation: b.rotation })).collect();
        if f.falling.is_some() {
            f.window += dt;
            if f.since.len() != now.len() {
                (f.since, f.window) = (now, 0.0);
            } else if f.window >= crate::ragdoll::CALM_WINDOW {
                let moved = crate::ragdoll::net_motion(&f.since, &now, f.window);
                let calm = moved.iter().all(|&(v, w)| v < crate::ragdoll::CALM_SPEED && w < crate::ragdoll::CALM_SPIN);
                f.calm = if calm { f.calm + f.window } else { 0.0 };
                (f.since, f.window) = (now, 0.0);
            }
        }
        if let Some(t) = f.falling.as_mut() {
            *t += dt;
            f.log_in -= dt;
            let held = *t >= MAX_FALLING;
            if f.calm >= crate::ragdoll::CALM_TIME || held || phys.world.ragdoll_asleep(id) {
                phys.world.set_ragdoll_at_rest(id);
                // The part that ended farthest from where it started.
                let moved = bodies.iter().zip(&f.start).enumerate().filter_map(|(i, (b, s))| {
                    let b = b.as_ref()?;
                    let d = (0..3).map(|k| (b.translation[k] - s[k]).powi(2)).sum::<f32>().sqrt();
                    Some((d, b.translation[2] - s[2], i))
                });
                let (d, dz, i) = moved.fold((0.0f32, 0.0f32, 0), |a, m| if m.0 > a.0 { m } else { a });
                let name = f.names.get(i).map_or("", String::as_str);
                let how = if held { "held at rest still moving" } else { "came to rest" };
                info!("wrecks: {id:08X} {how} after {:.1} s; farthest part {name} {d:.0} units from its start ({dz:+.0} in height)", *t);
                f.falling = None;
            } else if f.log_in <= 0.0 && *t < FALL_LOG_SECONDS {
                // Its fall in the log: the fastest part, once a second.
                if let Some((v, w)) = phys.world.ragdoll_speed(id) {
                    let speed = |b: &fnv_phys::BodyState| (b.linvel[0].powi(2) + b.linvel[1].powi(2) + b.linvel[2].powi(2)).sqrt();
                    let fastest = bodies.iter().enumerate().filter_map(|(i, b)| Some((speed(b.as_ref()?), i, b.as_ref()?.translation))).max_by(|a, b| a.0.total_cmp(&b.0));
                    let (name, at) = fastest.map_or((String::new(), [0.0; 3]), |(_, i, t)| (f.names.get(i).cloned().unwrap_or_default(), t));
                    info!("wrecks: {id:08X} falling: fastest part {v:.0} units/s, {w:.1} rad/s ({name} at {:.0}, {:.0}, {:.0})", at[0], at[1], at[2]);
                }
                f.log_in = 1.0;
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, wrecks: Res<Wrecks>) {
    if exit.read().next().is_none() {
        return;
    }
    info!("wrecks summary: {} wrecks' parts fell", wrecks.count);
}
