//! Resurrected actors on screen (`fnv_game::resurrect`, docs/spec-notes/resurrect.md):
//! a corpse that ResurrectActor brought back stands alive where its reference is.
//!
//! - The reference is put where the body lies, its ragdoll's pelvis
//!   (`Game::stand_where_the_body_lies`; capture rs1: the original's reference follows the
//!   ragdoll and the actor stands up there); then the ragdoll leaves the physics world and
//!   the pose it rested in is forgotten (`ragdoll.rs`).
//! - Its body is drawn anew: the corpse (death pose or ragdoll, `deaths.rs`) is taken
//!   away and a fresh actor spawned in its default standing pose, from its record with
//!   the random choices the resurrection rolled again (`ActorWorld::rolls`: a leveled
//!   actor may come back as another entry, a man or a woman; it wears and holds what its
//!   new inventory gives it). A resident (`residents.rs`) is spawned by the residents
//!   where its walker stands, its packages taking it up from there; an actor of a cell
//!   drawn without packages is spawned at its reference's new place.
//! - The animate flag's get-up is not played: it stands at once [design: the owner,
//!   2026-10-17].
//! - An actor that is not drawn needs nothing: it is no corpse any more, and is drawn
//!   alive with its new roll when its cell is.
//! - A living actor that was only healed (`Resurrection::regenerated` false) keeps its
//!   body.
//!
//! One log line per resurrection, and one when the actor is drawn standing again with
//! the package it runs (the smoke test `resurrect-actor` reads them).

use crate::actors::ActorWorld;
use crate::limbs::ActorRig;
use crate::ragdoll::{Corpse, Ragdolls};
use crate::world::WorldData;
use bevy::math::{Affine3A, Mat3};
use bevy::prelude::*;
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::model::ModelCtx;
use nvf_play::phys::PhysState;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use fnv_sim::FormId;

pub struct ResurrectPlugin;

impl Plugin for ResurrectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show_resurrections.after(nvf_play::PlaySet::Activation).before(crate::residents::drive));
    }
}

/// A drawn actor's root: its entity, reference, parent, and whether it is shown dead.
type Root = (Entity, &'static RefVisual, &'static ChildOf, Has<crate::locomotion::Dying>, Has<Corpse>);

#[allow(clippy::too_many_arguments)]
fn show_resurrections(
    mut commands: Commands,
    script: Option<ResMut<ScriptState>>,
    mut ragdolls: ResMut<Ragdolls>,
    mut phys: Option<ResMut<PhysState>>,
    actor_world: Option<Res<ActorWorld>>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    mut residents: Option<ResMut<crate::residents::Residents>>,
    world: Res<WorldData>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    roots: Query<Root, With<ActorRig>>,
    ragdoll_bodies: Query<(&ActorRig, &Corpse)>,
    transforms: Query<&Transform>,
    time: Res<Time>,
    (mut synced, mut waiting): (Local<bool>, Local<Vec<(u32, f32)>>),
) {
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let Some(aw) = actor_world else { return };
    let mut spawn_stats = crate::actors::ActorStats::default();
    // The rolls of a loaded game: actors resurrected before it was saved.
    if !*synced {
        if let Ok(mut rolls) = aw.rolls.lock() {
            rolls.extend(rt.game.extra.iter().filter(|(_, e)| e.rolls > 0).map(|(r, e)| (r.0, e.rolls)));
        }
        *synced = true;
    }
    // Those redrawn earlier: standing once their new body is there.
    waiting.retain(|&(id, since)| {
        let Some(_) = roots.iter().find(|r| r.1 .0 == id && !r.3 && !r.4) else { return time.elapsed_secs() - since < 30.0 };
        let package = rt.game.current_package(FormId(id)).map_or_else(|| "none".to_string(), |p| rt.game.data.name(p));
        info!("resurrect: {} drawn standing again {:.1} s after, package {package}", FormId(id), time.elapsed_secs() - since);
        false
    });
    for res in rt.game.take_resurrections() {
        let id = res.actor.0;
        let name = rt.game.data.name(rt.game.world.placed.get(res.actor).map_or(res.actor, |p| p.base));
        let root = roots.iter().find(|r| r.1 .0 == id);
        if !res.regenerated {
            info!("resurrect: {} ({name}) healed, alive already", res.actor);
            continue;
        }
        if let Ok(mut rolls) = aw.rolls.lock() {
            rolls.insert(id, res.rolls);
        }
        // Where the body lies: its ragdoll's first body (the pelvis), in the world or at
        // rest (capture rs1: the reference follows the ragdoll and stands up there).
        let pelvis = ragdoll_bodies.iter().find(|(r, _)| r.reference == id).map_or(0, |(_, c)| pelvis_body(c));
        let live = phys.as_deref().and_then(|p| p.world.ragdoll_nodes(id)).and_then(|n| n.get(pelvis).copied().flatten().map(|b| b.translation));
        let body = live.or_else(|| ragdolls.rest.get(&id).and_then(|r| r.get(pelvis).copied().flatten().map(|i| i.translation)));
        if let Some(at) = body {
            rt.game.stand_where_the_body_lies(res.actor, at);
        }
        ragdolls.rest.remove(&id);
        if let Some(p) = phys.as_deref_mut() {
            p.world.remove_ragdoll(id);
        }
        let flag = if res.animate { "1, the get-up not played" } else { "0" };
        let Some((e, _, parent, dying, corpse)) = root else {
            info!("resurrect: {} ({name}) alive again (animate {flag}), not drawn: drawn alive when its cell is", res.actor);
            continue;
        };
        let was = if corpse { "ragdoll" } else if dying { "death pose" } else { "body" };
        let state = rt.game.world.peek(res.actor).map(|s| (s.pos, s.rot));
        let (pos, rot) = state.unwrap_or_default();
        let at = format!("({:.0}, {:.0}, {:.0})", pos[0], pos[1], pos[2]);
        if residents.as_deref_mut().is_some_and(|r| r.redraw(&mut commands, id)) {
            info!("resurrect: {} ({name}) alive again (animate {flag}): its {was} taken away, standing at {at}, rolls {}; its packages take it up", res.actor, res.rolls);
            waiting.push((id, time.elapsed_secs()));
            continue;
        }
        // Drawn by its cell: spawned again there, where the game has its reference.
        let hidden = !rt.game.present(res.actor);
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let Some(drawn) = respawn(&mut commands, e, parent.parent(), id, (pos, rot), hidden, &aw, sim.as_deref().map(|s| &s.world), &world, &mut ctx, &transforms, &mut spawn_stats) else { continue };
        info!("resurrect: {} ({name}) alive again (animate {flag}): its {was} taken away, {} at {at}, rolls {}", res.actor, if drawn { "spawned standing" } else { "could not be drawn" }, res.rolls);
        if drawn {
            waiting.push((id, time.elapsed_secs()));
        }
    }
    crate::profile::actor_spawns(spawn_stats.equipment_spawns, spawn_stats.record_spawns);
}

/// Draw again the actor `id` that its cell drew as `root` under `parent`, at the game's
/// place for it (`pos`, `rot`): `root` is taken away and the actor spawned anew from its
/// record (with `ActorWorld`'s rolls and changes). `None` when it has no placement (a
/// reference created at run time), else whether anything was drawn. Shared with
/// `crate::appearance`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn respawn(
    commands: &mut Commands,
    root: Entity,
    parent: Entity,
    id: u32,
    (pos, rot): ([f32; 3], [f32; 3]),
    hidden: bool,
    aw: &ActorWorld,
    sim: Option<&fnv_sim::World<'_>>,
    world: &WorldData,
    ctx: &mut ActorCtx,
    transforms: &Query<&Transform>,
    stats: &mut crate::actors::ActorStats,
) -> Option<bool> {
    let p = fnv_world::placed::placement(world.lo, world.schema, id)?;
    let base = p.base.and_then(|b| fnv_world::placed::base_object(world.lo, world.schema, b))?;
    let to_parent = transforms.get(parent).map_or(Affine3A::IDENTITY, |t| t.compute_affine()).inverse();
    let m = Mat3::from_cols_array(&fnv_world::transform::ref_rotation(rot)).transpose();
    let rt_affine = to_parent * Affine3A::from_mat3_translation(m * p.scale, Vec3::from_array(pos));
    commands.entity(root).despawn();
    Some(match &base.sig {
        b"NPC_" => crate::actors::spawn(commands, parent, rt_affine, Vec3::from_array(pos), id, base.form_id, aw, sim, ctx, stats, hidden),
        b"CREA" => crate::actors::spawn_creature(commands, parent, rt_affine, id, base.form_id, aw, ctx, stats, hidden),
        _ => false,
    })
}

/// The ragdoll body on the pelvis (the body the actor's place is read from), else the
/// first.
fn pelvis_body(c: &Corpse) -> usize {
    c.data.bodies.iter().position(|b| b.object.node.as_deref().is_some_and(|n| n.to_ascii_lowercase().contains("pelvis"))).unwrap_or(0)
}
