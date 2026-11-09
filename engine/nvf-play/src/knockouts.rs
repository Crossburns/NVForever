//! Unconscious actors in play (docs/spec-notes/unconscious.md §4): one log line per
//! knockout, knockdown and getting up (`fnv_game::unconscious`), a count at exit, and the test
//! aid `NVF_PLAY_TEST_HIT`. The corner message of an essential knockout goes the script
//! effects' way (`crate::script_events::unconscious_line`); the bodies falling and getting
//! up are the renderer's (nvforever `knockouts.rs`).
//!
//! `NVF_PLAY_TEST_HIT=<seconds>:<reference>:<damage>[:<weapon>][:limb=<condition>][,…]`: at
//! that time the player hits the reference (an Editor ID, a hex FormID) for that much
//! health damage, with that weapon (its OnHit block runs, as the boxing gloves' does), on
//! the limb of that condition actor value (`limb=RightAttackCondition`: the damage is
//! also limb damage on a part holding 1 % of the actor's health, so a hit of a few
//! points cripples it; docs/spec-notes/crippled-limbs.md), and with `:crit` as a critical
//! hit, which gives the weapon's critical effect (an energy weapon's disintegration:
//! docs/spec-notes/ash-piles.md); with `:by=<reference>` that actor hits instead of the
//! player (`player` as the reference hits the player; the difficulty's smoke,
//! docs/spec-notes/difficulty.md §4) [test aid].

use crate::activate::HudMessage;
use crate::combat::{apply_hit, ActorHit, CombatStats, HitFeedback};
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::PlayState;
use bevy::prelude::*;
use fnv_game::combat::HitInfo;
use fnv_game::unconscious::{Cause, Happened};
use fnv_sim::FormId;

pub struct KnockoutsPlugin;

impl Plugin for KnockoutsPlugin {
    fn build(&self, app: &mut App) {
        let hits = match std::env::var("NVF_PLAY_TEST_HIT") {
            Ok(v) => parse_hits(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_HIT: {e}");
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        app.init_resource::<KnockoutStats>()
            .insert_resource(TestHits(hits))
            .add_systems(Update, (test_hits.run_if(in_state(PlayState::Playing)), report_news).chain().after(crate::PlaySet::Activation))
            .add_systems(Last, report);
    }
}

/// Knockouts seen in this run.
#[derive(Resource, Default, Debug)]
pub struct KnockoutStats {
    pub essential: u32,
    pub fatigue: u32,
    /// Knockdowns: PushActorAway and explosions.
    pub knocked_down: u32,
    /// Paralyses (`fnv_game::magic_archetypes`).
    pub paralyzed: u32,
    pub got_up: u32,
}

/// One `NVF_PLAY_TEST_HIT` item.
#[derive(Debug, Clone, PartialEq)]
pub struct TestHit {
    pub at: f32,
    pub target: String,
    pub damage: f32,
    pub weapon: Option<String>,
    /// The limb hit: its condition actor value's name.
    pub limb: Option<String>,
    /// `:crit`: a critical hit, which gives the weapon's critical effect.
    pub critical: bool,
    /// `:by=<reference>`: the attacker, else the player.
    pub by: Option<String>,
}

/// The aid's hits still to come, soonest first.
#[derive(Resource, Default)]
struct TestHits(Vec<TestHit>);

/// Parse `<seconds>:<reference>:<damage>[:<weapon>]` items separated by commas.
pub fn parse_hits(v: &str) -> Result<Vec<TestHit>, String> {
    let mut out = v
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let mut parts: Vec<&str> = item.split(':').map(str::trim).collect();
            let limb = parts.iter().position(|p| p.starts_with("limb=")).map(|i| parts.remove(i)["limb=".len()..].to_string());
            let critical = parts.iter().position(|p| p.eq_ignore_ascii_case("crit")).map(|i| parts.remove(i)).is_some();
            let by = parts.iter().position(|p| p.starts_with("by=")).map(|i| parts.remove(i)["by=".len()..].to_string());
            let (t, target, damage, weapon) = match parts.as_slice() {
                [t, r, d] => (t, r, d, None),
                [t, r, d, w] => (t, r, d, Some(w.to_string())),
                _ => return Err(format!("{item:?}: expected <seconds>:<reference>:<damage>[:<weapon>][:limb=<condition>][:crit]")),
            };
            let at = t.parse::<f32>().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            let damage = damage.parse::<f32>().map_err(|_| format!("{item:?}: bad damage {damage:?}"))?;
            Ok(TestHit { at, target: target.to_string(), damage, weapon, limb, critical, by })
        })
        .collect::<Result<Vec<_>, String>>()?;
    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    Ok(out)
}

/// What the hit aid needs to dismember (`crate::dismember`).
type DismemberAid<'w> = (Option<Res<'w, crate::combat::Combat>>, Option<Res<'w, crate::hitvol::ActorParts>>, Option<ResMut<'w, crate::dismember::Dismemberment>>);

#[allow(clippy::too_many_arguments)]
fn test_hits(
    time: crate::pause::AidTime,
    mut hits: ResMut<TestHits>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut stats: ResMut<CombatStats>,
    mut feedback: ResMut<HitFeedback>,
    mut hud: Option<ResMut<HudMessage>>,
    mut messages: MessageWriter<ActorHit>,
    (combat, parts, mut dm): DismemberAid,
) {
    let Some(now) = time.world_secs() else { return };
    while hits.0.first().is_some_and(|h| h.at <= now) {
        // A hit due before the scripts run waits for them (a slow start-up dropped it).
        let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
        let h = hits.0.remove(0);
        let found = with_world(rt, &mut sim, |rt| {
            let g = &rt.game;
            let target = fnv_game::runner::resolve(g, &h.target)?;
            let weapon = h.weapon.as_deref().map(|w| fnv_game::runner::resolve(g, w)).transpose()?;
            let by = h.by.as_deref().map(|b| fnv_game::runner::resolve(g, b)).transpose()?.unwrap_or(FormId::PLAYER_REF);
            Ok::<_, String>((target, weapon, by))
        });
        let (target, weapon, by) = match found {
            Ok(x) => x,
            Err(e) => {
                warn!("test aid: hit {}: {e}", h.target);
                continue;
            }
        };
        let limb = match h.limb.as_deref().map(|l| fnv_game::combat::limb_condition(l).ok_or(l)) {
            Some(Ok(condition)) => Some(fnv_game::combat::LimbHit { condition, health_percent: 1.0, damage: h.damage as f64, points: false }),
            Some(Err(l)) => {
                warn!("test aid: hit {}: no limb condition {l:?}", h.target);
                continue;
            }
            None => None,
        };
        let crit = combat.as_deref().filter(|_| h.critical).and_then(|c| crate::combat::crit_effect(c, weapon));
        let info = HitInfo { attacker: Some(by), weapon, location: 0, critical: h.critical, limb, penetrated: None };
        let r = apply_hit(script.as_deref_mut(), &mut sim, target, h.damage, info, &mut stats, &mut feedback, hud.as_deref_mut(), &mut messages, &[], false, crit);
        // A hit on a limb that kills, or lands on the corpse, may dismember it as a shot
        // there would (docs/spec-notes/dismemberment.md §4).
        if let (Some(r), Some(l), Some(c), Some(parts), Some(dm)) = (r.as_ref(), limb, combat.as_deref(), parts.as_deref(), dm.as_deref_mut()) {
            let part = parts.data_of(target.0).and_then(|d| c.forms.body_parts.get(&d)).and_then(|d| {
                d.parts.iter().position(|p| p.actor_value.as_deref().and_then(fnv_game::combat::limb_condition) == Some(l.condition))
            });
            let w = weapon.and_then(|w| c.forms.weapons.get(&w)).cloned().unwrap_or_default();
            let at = sim.world.peek(target).map_or([0.0; 3], |s| [s.pos[0], s.pos[1], s.pos[2] + 40.0]);
            // The blow goes from the player to the target, level.
            let from = sim.world.peek(FormId::PLAYER_REF).map_or(at, |s| s.pos);
            let (dx, dy) = (at[0] - from[0], at[1] - from[1]);
            let len = dx.hypot(dy);
            let dir = if len > 1.0 { [dx / len, dy / len, 0.0] } else { [0.0, 1.0, 0.0] };
            if let Some(check) = part.and_then(|p| crate::combat::dismember_check(target, Some(p as u8), &w, r, Some(by), at, dir)) {
                dm.checks.push(check);
            }
        }
        match r {
            Some(r) => info!("test aid: {} hits {target} for {:.0}{}: health {:.0}/{:.0}", if by == FormId::PLAYER_REF { "the player".to_string() } else { by.to_string() }, r.dealt, weapon.map_or(String::new(), |w| format!(" with {w}")), r.health, r.max_health),
            None => warn!("test aid: hit {target}: no scripts"),
        }
    }
}

/// Log the knockouts and the getting up since last frame.
fn report_news(mut script: Option<ResMut<ScriptState>>, sim: Res<SimState>, mut stats: ResMut<KnockoutStats>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    // What the magic effect archetypes did (docs/spec-notes/magic-archetypes.md §3).
    for n in rt.game.take_magic_news() {
        use fnv_game::magic_archetypes::MagicNews;
        let name = |r: FormId| sim.ref_name(Some(&rt.game), r.0).map_or_else(|| r.to_string(), |n| format!("{n} {r}"));
        match n {
            MagicNews::Paralyzed { actor, effect, seconds } => info!("magic: {} paralyzed by {} for {seconds:.0} s", name(actor), rt.game.data.name(effect)),
            MagicNews::Cured { actor, what, count } => info!("magic: {what} on {}: {count} effects ended", name(actor)),
            MagicNews::Concussed { actor, seconds } => info!("magic: {} concussed for {seconds:.0} s", name(actor)),
            MagicNews::Palm { target } => info!("magic: Paralyzing Palm strikes {}", name(target)),
        }
    }
    for n in rt.game.take_knockout_news() {
        let name = sim.ref_name(Some(&rt.game), n.actor.0).map_or_else(|| n.actor.to_string(), str::to_string);
        match n.what {
            Happened::Up => {
                stats.got_up += 1;
                info!("knockout: {name} {} gets up ({} knockout over)", n.actor, n.cause.name());
            }
            Happened::Held => info!("knockout: {name} {} stays down: a fight goes on near it", n.actor),
            Happened::Down => {
                match n.cause {
                    Cause::Essential => stats.essential += 1,
                    Cause::Fatigue => stats.fatigue += 1,
                    Cause::Pushed | Cause::KnockedDown => stats.knocked_down += 1,
                    Cause::Paralyzed => stats.paralyzed += 1,
                }
                let how = if n.cause.awake() { "knocked down" } else { "knocked out" };
                info!("knockout: {name} {} {how} ({})", n.actor, n.cause.name());
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<KnockoutStats>) {
    if exit.read().last().is_none() || stats.essential + stats.fatigue + stats.knocked_down + stats.paralyzed == 0 {
        return;
    }
    info!(
        "knockouts: {} essential, {} by fatigue, {} knocked down, {} paralyzed, {} got up",
        stats.essential, stats.fatigue, stats.knocked_down, stats.paralyzed, stats.got_up
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_parse_in_time_order() {
        let h = parse_hits("6:GSSunnySmilesREF:30:WeapNVBoxingGloves, 4:0x00104E87:500").unwrap();
        assert_eq!(h[0], TestHit { at: 4.0, target: "0x00104E87".into(), damage: 500.0, weapon: None, limb: None, critical: false, by: None });
        assert_eq!(h[1].weapon.as_deref(), Some("WeapNVBoxingGloves"));
        let h = parse_hits("5:GSChetRef:1000:limb=RightAttackCondition,6:GSChetRef:1:Weap10mmPistol:limb=LeftMobilityCondition").unwrap();
        assert_eq!((h[0].weapon.as_deref(), h[0].limb.as_deref()), (None, Some("RightAttackCondition")));
        assert_eq!((h[1].weapon.as_deref(), h[1].limb.as_deref()), (Some("Weap10mmPistol"), Some("LeftMobilityCondition")));
        let h = parse_hits("5:GSChetRef:500:WeapLaserRifle:crit").unwrap();
        assert!(h[0].critical && h[0].weapon.as_deref() == Some("WeapLaserRifle"));
        let h = parse_hits("7:player:10:by=GSSunnySmilesREF").unwrap();
        assert_eq!((h[0].target.as_str(), h[0].by.as_deref(), h[0].damage), ("player", Some("GSSunnySmilesREF"), 10.0));
        assert!(parse_hits("4:x").is_err());
        assert!(parse_hits("a:x:1").is_err());
    }
}
