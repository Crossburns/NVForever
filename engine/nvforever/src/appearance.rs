//! Race and sex changes on screen (`fnv_game::appearance`, docs/spec-notes/
//! appearance-changes.md): an actor that AgeRace or SexChange changed is drawn again with
//! its new race and sex.
//!
//! - The changes are kept for the resolver (`ActorWorld::changes`): every body drawn from
//!   then on has them, an actor drawn later (its cell loaded) included. The table is
//!   rebuilt from the game every [`RESYNC_FRAMES`] frames, so a loaded game's changes
//!   reach it too.
//! - **An NPC on screen** is drawn anew at once: a resident (`residents.rs`) by the
//!   residents where its walker stands, else spawned again where its cell drew it
//!   (`resurrect::respawn`). One not drawn needs nothing.
//! - **The player**: its body is rebuilt when the change differs from the one it was built
//!   with (`player_body.rs`); with SexChange's reset the character creator's look becomes
//!   the race's first starting face for the new sex (`nvf_play::creator::preset_look`)
//!   and is kept as the player's look (`fnv_game::chargen::apply_look`).
//!
//! One log line per change (`appearance: …`; the smoke tests `agerace-sexchange-npc` and
//! `sexchange-player-reset` read them).

use crate::actors::ActorWorld;
use crate::limbs::ActorRig;
use crate::world::WorldData;
use bevy::prelude::*;
use fnv_actor::Change;
use fnv_game::Game;
use fnv_sim::FormId;
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::model::ModelCtx;
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;

pub struct AppearancePlugin;

impl Plugin for AppearancePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show_changes.after(nvf_play::PlaySet::Activation).before(crate::residents::drive));
    }
}

/// The change `actor` has in the game now, for the resolver: the player's only while the
/// character creator has given no look (the look carries race and sex then).
fn change_of(g: &Game, actor: FormId) -> Change {
    if actor == FormId::PLAYER_REF {
        if g.player.look.is_some() {
            return Change::default();
        }
        return Change { race: g.player.race, female: g.player.female };
    }
    g.extra.get(&actor).map_or_else(Change::default, |e| Change { race: e.race, female: e.female })
}

/// Frames between rebuilds of `ActorWorld::changes` from the game [design: cheap, and
/// soon enough for a loaded game's actors, which are drawn after the load].
const RESYNC_FRAMES: u32 = 120;

/// Every change the game holds now, for the resolver.
fn all_changes(g: &Game) -> std::collections::HashMap<u32, Change> {
    let refs = g.extra.keys().copied().chain(std::iter::once(FormId::PLAYER_REF));
    refs.map(|r| (r.0, change_of(g, r))).filter(|(_, c)| !c.is_none()).collect()
}

#[allow(clippy::too_many_arguments)]
fn show_changes(
    mut commands: Commands,
    script: Option<ResMut<ScriptState>>,
    actor_world: Option<Res<ActorWorld>>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    mut residents: Option<ResMut<crate::residents::Residents>>,
    world: Res<WorldData>,
    creator: Option<Res<nvf_play::creator::CreatorData>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    roots: Query<(Entity, &RefVisual, &ChildOf), With<ActorRig>>,
    transforms: Query<&Transform>,
    mut since_sync: Local<Option<u32>>,
) {
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let Some(aw) = actor_world else { return };
    let mut spawn_stats = crate::actors::ActorStats::default();
    // The changes of a loaded game (or of scripts run before the actors were read).
    let due = since_sync.is_none_or(|n| n >= RESYNC_FRAMES);
    *since_sync = Some(if due { 0 } else { since_sync.unwrap_or(0) + 1 });
    if due {
        let now = all_changes(&rt.game);
        if let Ok(mut changes) = aw.changes.lock() {
            if *changes != now {
                *changes = now;
            }
        }
    }
    for c in rt.game.take_appearance_changes() {
        let id = c.actor.0;
        let g = &mut rt.game;
        let name = g.data.name(g.world.placed.get(c.actor).map_or(c.actor, |p| p.base));
        let race = g.actor_race(c.actor).map_or_else(|| "none".to_string(), |r| g.data.name(r));
        let sex = if g.actor_female(c.actor) { "female" } else { "male" };
        if c.actor == FormId::PLAYER_REF && c.reset {
            // SexChange's reset: the race's first starting face for the new sex.
            if let Some(data) = creator.as_deref() {
                let start = g.player.look.as_ref().map(nvf_play::creator::actor_look).or_else(|| data.0.player.clone());
                if let Some(mut start) = start {
                    start.race = g.actor_race(c.actor).unwrap_or(start.race);
                    start.female = g.actor_female(c.actor);
                    let look = nvf_play::creator::preset_look(data, start);
                    fnv_game::chargen::apply_look(g, nvf_play::creator::game_look(&look));
                    info!("appearance: the player's face reset to the first starting face of {race} {sex}");
                }
            }
        }
        let change = change_of(g, c.actor);
        if let Ok(mut changes) = aw.changes.lock() {
            if change.is_none() {
                changes.remove(&id);
            } else {
                changes.insert(id, change);
            }
        }
        if c.actor == FormId::PLAYER_REF {
            info!("appearance: the player is now {race} {sex}; the body is rebuilt");
            continue;
        }
        let Some((e, _, parent)) = roots.iter().find(|r| r.1 .0 == id) else {
            info!("appearance: {} ({name}) is now {race} {sex}, not drawn: drawn so when its cell is", c.actor);
            continue;
        };
        if residents.as_deref_mut().is_some_and(|r| r.redraw(&mut commands, id)) {
            info!("appearance: {} ({name}) is now {race} {sex}: drawn anew by the residents", c.actor);
            continue;
        }
        let Some(state) = g.world.peek(c.actor).map(|s| (s.pos, s.rot)) else { continue };
        let hidden = !g.present(c.actor);
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        let drawn = crate::resurrect::respawn(&mut commands, e, parent.parent(), id, state, hidden, &aw, sim.as_deref().map(|s| &s.world), &world, &mut ctx, &transforms, &mut spawn_stats);
        let how = match drawn {
            Some(true) => "drawn anew",
            Some(false) => "could not be drawn",
            None => "kept (no placement to draw it again from)",
        };
        info!("appearance: {} ({name}) is now {race} {sex}: {how}", c.actor);
    }
    crate::profile::actor_spawns(spawn_stats.equipment_spawns, spawn_stats.record_spawns);
}
