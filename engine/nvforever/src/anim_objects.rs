//! Animated objects (docs/spec-notes/idles.md §10): the props actors hold while an idle
//! plays, such as Chet's broom while he sweeps (`LooseSweeping`, `aoSweepingBroom`) or a
//! cigarette while smoking. `fnv_actor::idle::anim_objects` reads the ANIO records; each
//! names the IDLE it goes with and a model whose `Prn` extra data names the bone it hangs
//! from. While an actor's idle slot plays an IDLE that has animated objects (`idles.rs`,
//! [`ActorIdle`]), their models are drawn under those bones of its skeleton, rigid, as
//! a rigid body part is (`nvf_assets::actor::rigid_local`); when the idle ends or another
//! starts they are taken away [hypothesis for the game's rule, idles.md §10].
//!
//! A node of the object's model that the idle's clip has a track for is moved by that
//! track, every frame (idles.md §10): Doc Mitchell's mirror (`nvpropreflectron.nif`,
//! whose node `##Dummy01_Reflectron01` is stored about 200 units from the hand) is
//! brought into his hand and handed over by the track of that name in
//! `SpecialIdle_NVCG01DocGiveMirror`.
//!
//! Log: `anim object: <actor> holds <model> on <bone> (<idle>)` per object drawn,
//! `anim object: <actor> <ANIO> node <name> follows the idle's track` per node moved, and
//! at the end `anim objects summary: …`.
//!
//! Not done: the player's idles (none of the player's known idles has an object), an
//! object's own controllers (`aobroom.nif` carries a transform controller on its root,
//! not played), and objects whose model has no `Prn` (drawn at `Bip01 Head`, the rigid
//! parts' default).

use crate::idles::{ActorIdle, Playing};
use crate::limbs::ActorRig;
use crate::world::WorldData;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_nif::anim::TransformTrack;
use nvf_assets::actor::{rigid_local, ActorCtx, ActorParts};
use nvf_assets::anim_play::BoundClip;
use nvf_assets::model::ModelCtx;
use std::collections::HashMap;
use std::sync::Arc;

/// The animated objects by the IDLE they go with: (editor ID, model) of each.
#[derive(Resource, Default)]
pub struct AnimObjects {
    by_idle: HashMap<u32, Vec<(String, String)>>,
    stats: Stats,
    startup: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Stats {
    /// Objects drawn on actors, idles that had objects, objects whose model did not load
    /// or whose bone the skeleton lacks.
    drawn: u32,
    idles: u32,
    failed: u32,
}

impl AnimObjects {
    pub fn build(world: &WorldData) -> AnimObjects {
        let mut issues = fnv_sim::extract::Issues::default();
        let all = fnv_actor::idle::anim_objects(world.lo, world.schema, &mut issues);
        let mut by_idle: HashMap<u32, Vec<(String, String)>> = HashMap::new();
        let mut usable = 0;
        for a in &all {
            if let (Some(idle), Some(model)) = (a.idle, a.model.clone()) {
                by_idle.entry(idle.0).or_default().push((a.edid.clone().unwrap_or_else(|| a.id.to_string()), model));
                usable += 1;
            }
        }
        let startup = format!("anim objects: {} records, {usable} with an idle and a model, for {} idles", all.len(), by_idle.len());
        AnimObjects { by_idle, stats: Stats::default(), startup: Some(startup) }
    }
}

/// The animated objects drawn on an actor now (on its root): for which idle (its slot
/// serial), their entities, and their nodes the idle's clip moves.
#[derive(Component)]
pub struct HeldObjects {
    serial: u32,
    entities: Vec<Entity>,
    moved: Vec<Moved>,
}

/// A node of an object's model moved by a track of the idle's clip.
struct Moved {
    /// The node's entity, under the bone.
    entity: Entity,
    bone: usize,
    /// The transform above the node in the model: the part's root and the node's
    /// ancestors.
    above: Affine3A,
    /// The node's stored transform, for what the track has no channel for.
    rest: Affine3A,
    track: Arc<TransformTrack>,
}

impl Moved {
    /// The node's transform under its bone with the track sampled at `time`.
    fn local(&self, rig: &ActorRig, time: f32) -> Transform {
        let (s0, r0, t0) = self.rest.to_scale_rotation_translation();
        let t = self.track.translation.sample(time).map_or(t0, Vec3::from_array);
        let r = self.track.rotation_at(time).map_or(r0, |[w, x, y, z]| Quat::from_xyzw(x, y, z, w).normalize());
        let s = self.track.scale.sample(time).map_or(s0.x, |[s]| s);
        let node = Affine3A::from_scale_rotation_translation(Vec3::splat(s), r, t);
        Transform::from_matrix(Mat4::from(rigid_local(&rig.skel, self.bone, &self.above, &node)))
    }
}

/// The clip time an idle shows, as [`ActorIdle::layers`] gives it to the blend player.
fn clip_time(p: &Playing<Arc<BoundClip>>) -> f32 {
    let len = p.play.keys.length.max(1e-3);
    p.clip.time_at((p.play.time() / len).min(0.999))
}

/// Read the records once the world data is there.
pub fn read(mut a: ResMut<AnimObjects>, world: Res<WorldData>) {
    *a = AnimObjects::build(&world);
    if let Some(line) = a.startup.take() {
        info!("{line}");
    }
}

/// Draw the objects of each actor's idle while it plays; take them away after. Nodes
/// the idle's clip has tracks for follow them.
pub fn hold(
    mut commands: Commands,
    mut objects: ResMut<AnimObjects>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    actors: Query<(Entity, &ActorRig, Option<&ActorIdle>, Option<&HeldObjects>)>,
    mut transforms: Query<&mut Transform>,
) {
    let objects = &mut *objects;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for (e, rig, idle, held) in &actors {
        // The idle playing (also while it fades out at its end), not a script's group; else
        // the one before it while it still shows under the next one's fade-in, so an
        // idle brought back by the line that asks for it again keeps its objects
        // (#1355: Doc's mirror, a chair fidget starting as the hand-over ended).
        let has = |p: &&Playing<Arc<BoundClip>>| p.group.is_none() && objects.by_idle.contains_key(&p.idle);
        let playing = idle.and_then(|i| i.0.cur.as_ref().filter(has).or(i.0.prev.as_ref().filter(has)));
        let want = playing.map(|p| (p.idle, p.serial));
        if held.map(|h| h.serial) == want.map(|w| w.1) {
            if let (Some(h), Some(p)) = (held, playing) {
                let time = clip_time(p);
                for m in &h.moved {
                    if let Ok(mut t) = transforms.get_mut(m.entity) {
                        *t = m.local(rig, time);
                    }
                }
            }
            continue;
        }
        if let Some(h) = held {
            for &x in &h.entities {
                commands.entity(x).despawn();
            }
            commands.entity(e).remove::<HeldObjects>();
        }
        let (Some((idle, serial)), Some(p)) = (want, playing) else { continue };
        objects.stats.idles += 1;
        let time = clip_time(p);
        let mut entities = Vec::new();
        let mut moved: Vec<Moved> = Vec::new();
        for (edid, model) in objects.by_idle[&idle].clone() {
            let Some(part) = ctx.part(&model) else {
                objects.stats.failed += 1;
                warn!("anim object: {edid} ({model}) does not load");
                continue;
            };
            let bone_name = part.attach_bone().to_string();
            let Some(&b) = rig.skel.by_name.get(&bone_name) else {
                objects.stats.failed += 1;
                warn!("anim object: {:08X} has no {bone_name} for {edid}", rig.reference);
                continue;
            };
            // The entity of each node the clip moves, shared by the shapes below it.
            let mut nodes: HashMap<String, Entity> = HashMap::new();
            for s in part.shapes.iter().filter(|s| !s.cap) {
                // The outermost ancestor the clip has a track for (`ancestors` runs from
                // the parent outwards).
                let found = s.ancestors.iter().enumerate().rev().find_map(|(k, (name, own))| {
                    let name = name.as_deref()?;
                    let (_, track) = p.clip.clip.tracks.iter().find(|(n, _)| n == name)?;
                    Some((k, name.to_string(), *own, track.clone()))
                });
                let Some((k, name, rest, track)) = found else {
                    let local = rigid_local(&rig.skel, b, &part.root, &s.transform);
                    let x = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(local)))).id();
                    commands.entity(rig.nodes[b]).add_child(x);
                    entities.push(x);
                    continue;
                };
                let node = match nodes.get(&name) {
                    Some(&x) => x,
                    None => {
                        let above = s.ancestors[k + 1..].iter().rev().fold(part.root, |acc, (_, t)| acc * *t);
                        let mut m = Moved { entity: Entity::PLACEHOLDER, bone: b, above, rest, track };
                        let x = commands.spawn((m.local(rig, time), Visibility::default())).id();
                        commands.entity(rig.nodes[b]).add_child(x);
                        entities.push(x);
                        m.entity = x;
                        moved.push(m);
                        nodes.insert(name.clone(), x);
                        info!("anim object: {:08X} {edid} node {name} follows the idle's track", rig.reference);
                        x
                    }
                };
                let below = s.ancestors[..k].iter().rev().fold(Affine3A::IDENTITY, |acc, (_, t)| acc * *t) * s.own;
                let x = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(below)))).id();
                commands.entity(node).add_child(x);
            }
            objects.stats.drawn += 1;
            info!("anim object: {:08X} holds {model} on {bone_name} ({edid}, idle {idle:08X})", rig.reference);
        }
        commands.entity(e).insert(HeldObjects { serial, entities, moved });
    }
}

pub fn report(mut exit: MessageReader<AppExit>, objects: Res<AnimObjects>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = objects.stats;
    info!("anim objects summary: {} objects drawn for {} idles played, {} could not be drawn", s.drawn, s.idles, s.failed);
}
