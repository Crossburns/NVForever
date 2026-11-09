//! A swinging trap's hits (docs/spec-notes/traps.md §4): nvforever's `swing_traps.rs`
//! finds the actors a trap's moving bodies meet ([`TrapHit`]); here the trap's script
//! variables decide the hit (`fnv_game::Game::trap_contact`), which takes health as a fall
//! does (no attacker, no body part) and pushes the actor away from the trap as
//! PushActorAway does, at the trap's push speed (`fTrapPushBack` read as game units a
//! second [hypothesis]).
//!
//! Log: `swing traps: <trap> hit <actor>: <n> damage, <d> dealt, health <h>/<max>[, dead]`.

use crate::activate::HudMessage;
use crate::combat::{apply_hit, ActorHit, CombatStats, HitFeedback};
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_sim::FormId;

/// A body of `trap` moving at `speed` (game units a second) met `actor`.
#[derive(Message, Clone, Copy, Debug)]
pub struct TrapHit {
    pub trap: FormId,
    pub actor: FormId,
    pub speed: f32,
}

pub struct SwingTrapsPlugin;

impl Plugin for SwingTrapsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TrapHit>().add_systems(Update, strike.in_set(PlaySet::Activation).run_if(resource_exists::<SimState>).run_if(resource_exists::<CombatStats>).run_if(resource_exists::<HitFeedback>));
    }
}

#[allow(clippy::too_many_arguments)]
fn strike(
    mut hits: MessageReader<TrapHit>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    (mut stats, mut feedback): (ResMut<CombatStats>, ResMut<HitFeedback>),
    mut hud: Option<ResMut<HudMessage>>,
    mut messages: MessageWriter<ActorHit>,
) {
    for h in hits.read() {
        let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
        let Some(s) = with_world(rt, &mut sim, |rt| rt.game.trap_contact(h.trap, h.actor, f64::from(h.speed))) else { continue };
        let info = fnv_game::combat::HitInfo { attacker: None, weapon: None, location: -1, critical: false, limb: None, penetrated: None };
        let r = apply_hit(script.as_deref_mut(), &mut sim, h.actor, s.damage as f32, info, &mut stats, &mut feedback, hud.as_deref_mut(), &mut messages, &[], false, None);
        if s.push > 0.0 {
            if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                let force = s.push / f64::from(fnv_game::unconscious::PUSH_SPEED);
                with_world(rt, &mut sim, |rt| rt.game.push_actor_away(Some(h.trap), h.actor, force));
            }
        }
        if let Some(r) = r {
            info!(
                "swing traps: {} hit {}: {:.0} damage, {:.1} dealt, health {:.0}/{:.0}{}",
                h.trap,
                h.actor,
                s.damage,
                r.dealt,
                r.health,
                r.max_health,
                if r.immune { " (immune: god mode)" } else if r.died { ", dead" } else { "" }
            );
        }
    }
}
