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
//! - The parts start at rest [design: the blast's push on them is not modelled] and do
//!   not hurt actors [design].
//! - A wreck whose rig leaves the world (its cell unloads, ClearDestruction) loses its
//!   ragdoll; drawn again, its parts fall again from the file's poses [design: where they
//!   came to rest is not kept].
//!
//! Log: `wrecks: <ref> <model>: <n> parts fall (<m> moving)`, `wrecks: <ref> came to
//! rest after <s> s`.

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
        if let Some(f) = wrecks.fallen.get(&r) {
            if f.root == root {
                // Collision built again (a cell loaded under it) is held solid again.
                phys.world.set_ref_keyframed_solid(r, false);
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
        wrecks.fallen.insert(r, Fallen { root, nodes, scale, falling: Some(0.0) });
    }
}

/// The fallen wrecks' bodies move their nodes.
fn follow(
    time: Res<Time>,
    mut wrecks: ResMut<Wrecks>,
    phys: Option<Res<PhysState>>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(phys) = phys else { return };
    let dt = time.delta_secs();
    for (&id, f) in wrecks.fallen.iter_mut() {
        let Some(bodies) = phys.world.ragdoll_nodes(id) else { continue };
        crate::swing_traps::follow_bodies(&bodies, &f.nodes, f.scale, f.root, &parents, &globals, &mut transforms);
        if let Some(t) = f.falling.as_mut() {
            *t += dt;
            if phys.world.ragdoll_asleep(id) {
                info!("wrecks: {id:08X} came to rest after {:.1} s", *t);
                f.falling = None;
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
