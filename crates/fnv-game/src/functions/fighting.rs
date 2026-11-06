//! Script functions over the combat AI (`crate::awareness`, `crate::combat_group`,
//! `crate::combat_style`, `crate::combat`): combat styles, combat groups, combat
//! targets, fleeing, knockouts, causes of death, armour and scripted shots
//! (docs/design/script-stubs.md §3.6).

use super::{none, yes, Args, Ctx};
use crate::awareness::State;
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::forms::ItemKind;
use fnv_sim::FormId;
use std::collections::BTreeSet;

/// `actor.SetCombatStyle style`: the reference fights with that combat style from now
/// on; the base form's style is not changed [documented: catalogue notes]. Kept in
/// `RefExtra::combat_style`, which [`Game::combat_style`] reads first. A form that is not
/// a combat style is refused.
pub fn set_combat_style(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let style = a.form(0)?;
    if !g.data.combat_styles.contains_key(&style) {
        return Err(format!("{style} is not a combat style"));
    }
    g.extra.entry(r).or_default().combat_style = Some(style);
    none()
}

/// `actor.GetGroupMemberCount`: how many are in the actor's combat group, itself
/// counted; 0 when it is not in combat [documented: catalogue summary, "0 if not an
/// actor or not in combat, 1 if fighting alone"]. The group is [`Game::combat_group`]
/// (every actor in a combat state that is its comrade and has a foe in common with it);
/// in combat is a combat state (Alert, Combat, Lost), as IsInCombat counts it
/// [hypothesis].
pub fn get_group_member_count(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if !g.awareness.state(r).in_combat() {
        return Ok(Value::Num(0.0));
    }
    Ok(Value::Num(g.combat_group(r).len() as f64))
}

/// `actor.GetGroupTargetCount`: how many targets the actor's combat group has; 0 when it
/// is not in combat or the group has none [documented: catalogue summary]. The targets
/// are every member's targets and the owners of its alarms ([`Game::foes_of`]), each
/// counted once [hypothesis: an alarm's owner, searched for, is still the group's
/// target].
pub fn get_group_target_count(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if !g.awareness.state(r).in_combat() {
        return Ok(Value::Num(0.0));
    }
    let foes: BTreeSet<FormId> = g.combat_group(r).into_iter().flat_map(|m| g.foes_of(m)).collect();
    Ok(Value::Num(foes.len() as f64))
}

/// `actor.IsCombatTarget target`: 1 when `target` is one of the actor's combat targets
/// [documented: catalogue summary]: on its target list while it is in a combat state
/// (`crate::awareness::Awareness::targets`) [hypothesis: a target it lost and searches
/// for still counts, the list keeping it].
pub fn is_combat_target(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let target = a.form(0)?;
    yes(g.awareness.get(r).is_some_and(|x| x.state.in_combat() && x.targets.contains_key(&target)))
}

/// `actor.GetShouldAttack target`: 1 when the actor would attack `target` on detecting
/// it. No wiki page [documented: script-functions.md §4, one of eight functions without
/// one]; read from its name and its six uses, each `GetShouldAttack player == 0` before a
/// scene goes on (observed: decompiled FalloutNV.esm) [hypothesis]. Answered by
/// [`Game::would_attack`] (faction reactions, aggression, StartCombat, grudges).
pub fn get_should_attack(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let target = a.opt_form(0).unwrap_or(FormId::PLAYER_REF);
    yes(g.would_attack(r, target))
}

/// `actor.ForceFlee [cell] [reference]`: the actor assesses the threats and flees from
/// one if there is one; with none it goes on with its package; a conversation it is in
/// ends at once [documented: catalogue summary and notes]. The threat is its combat
/// target, else the first of its foes ([`Game::foes_of`]) [engineering choice]. The
/// flight is the combat AI's ([`Game::start_flight`]); given a reference, its spot is
/// that reference's place, flown to as Away [hypothesis: the AI runs there rather than
/// to a door or cover]; a cell is not used (the AI picks the spot) [engineering choice:
/// no way to a cell yet]. Whether the actor stops at the spot is the AI's.
pub fn force_flee(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if g.dialogue.as_ref().is_some_and(|d| d.actor == r) {
        g.end_dialogue();
    }
    let threat = g.awareness.get(r).and_then(|x| x.combat_target().map(|(t, _)| t)).or_else(|| g.foes_of(r).into_iter().next());
    let Some(from) = threat else { return none() };
    let at = position(g, from).unwrap_or_default();
    g.start_flight(r, from, at);
    if let Some(spot) = a.opt_form(1).and_then(|d| position(g, d)) {
        if let Some(f) = g.awareness.tactics.flights.get_mut(&r) {
            f.spot = Some((spot, crate::combat_group::FleeBy::Away));
        }
    }
    none()
}

/// `actor.GetConcussed`: 1 while the actor is concussed [documented: GECK wiki
/// GetConcussed; the rule: `crate::magic_archetypes`, magic-archetypes.md §2].
pub fn get_concussed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(if g.is_concussed(r) { 1.0 } else { 0.0 }))
}

/// `actor.GetKnockedState`: −1 knocked out by fatigue, 0 normal, 1 knocked out
/// (unconscious essentials, paralysis, negative fatigue), 2 knocked down [documented:
/// GECK wiki GetKnockedState]. Here −1 for a fatigue knockout, 1 for an essential one
/// (`crate::unconscious`), a paralysis [documented] and SetUnconscious [hypothesis for
/// SetUnconscious], 2 for an explosion's knockdown, else 0. An actor PushActorAway laid down reads 0 until it is
/// up, also when it is knocked out meanwhile [documented: the page's notes]. Getting up
/// reads 0 [observed: an essential RingoRef knocked out by KillActor read 1 for 10 s and 0
/// from the sample his get-up began, AI observer campaign `functions_round` 2026-10-19,
/// docs/spec-notes/unconscious.md §2.10; the other causes' getting up assumed the same].
pub fn get_knocked_state(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    use crate::unconscious::Cause;
    let r = ctx.subject()?;
    let state = match g.knockout(r) {
        Some(k) if !k.lying() => 0.0,
        Some(k) if k.pushed => 0.0,
        Some(k) => match k.cause {
            Cause::Fatigue => -1.0,
            Cause::Essential | Cause::Paralyzed => 1.0,
            Cause::KnockedDown => 2.0,
            Cause::Pushed => 0.0,
        },
        None if g.extra.get(&r).is_some_and(|e| e.unconscious) => 1.0,
        None => 0.0,
    };
    Ok(Value::Num(state))
}

/// `source.PushActorAway target force`: knocks the target down and throws it away from
/// the source with the force (towards it when negative; straight up without a source); no
/// effect on the dead, immobile creatures or actors flagged No Knockdowns [documented:
/// GECK wiki PushActorAway]. The rules are [`Game::push_actor_away`]; the player is
/// knocked down too (docs/spec-notes/unconscious.md §5.4).
pub fn push_actor_away(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let target = a.form(0)?;
    let force = a.num_or(1, 0.0);
    g.push_actor_away(ctx.subject().ok(), target, force);
    none()
}

/// The GetCauseofDeath code of a death by a weapon of WEAP DNAM animation type
/// `record_type`, or by a hit with no weapon (`None`, a creature's attack or fists).
/// [documented: catalogue notes, the codes −1 none, 0 explosion, 2 gun, 3 blunt weapon, 4
/// hand to hand, 5 object impact, 6 poison, 7 radiation; code 1 is not on the page.]
/// Which weapon types give which code is [hypothesis]: hand to hand (0) and no weapon
/// 4; melee (1, 2) 3, the page naming no blade; pistols, rifles and Handle (3–8) 2; the
/// launcher, grenades and mines (9–12) 0; thrown weapons (13) 5.
pub fn cause_code(record_type: Option<i64>) -> i32 {
    match record_type {
        None | Some(0) => 4,
        Some(1 | 2) => 3,
        Some(3..=8) => 2,
        Some(9..=12) => 0,
        Some(13) => 5,
        Some(_) => -1,
    }
}

/// `actor.GetCauseofDeath`: how the actor died, by [`cause_code`] of the hit that killed it
/// (`RefExtra::killing_hit`; an earlier wound or a hit on the corpse does not count), unless
/// KillActor named a cause (`RefExtra::death_cause`); −1 while it lives, or dead with no
/// killing hit (KillActor, an effect's or a script's damage, dead from the start)
/// [hypothesis: "none"].
/// Poison and radiation deaths are not told apart yet.
pub fn get_cause_of_death(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if !g.is_dead(r) {
        return Ok(Value::Num(-1.0));
    }
    if let Some(cause) = g.extra.get(&r).and_then(|e| e.death_cause) {
        return Ok(Value::Num(cause as f64));
    }
    let Some(hit) = g.extra.get(&r).and_then(|e| e.killing_hit) else { return Ok(Value::Num(-1.0)) };
    let record_type = match hit.weapon {
        Some(w) => Some(g.data.weapon_anim_types.get(&w).copied().unwrap_or(-1)),
        None => None,
    };
    Ok(Value::Num(cause_code(record_type) as f64))
}

/// `actor.IsLimbGone limb [last]`: 1 when the body part at location `limb` (GetHitLocation's
/// numbering), or any from `limb` to `last`, has been severed or exploded [documented: GECK
/// wiki IsLimbGone] (`RefExtra::limbs_gone`, docs/spec-notes/dismemberment.md §5).
pub fn is_limb_gone(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let first = a.int(0)?;
    let last = a.int(1).unwrap_or(first);
    let (lo, hi) = (first.min(last), first.max(last));
    yes(g.extra.get(&r).is_some_and(|e| e.limbs_gone.range(lo..=hi).next().is_some()))
}

/// `actor.GetKillingBlowLimb`: the location of the hit that killed the actor, −1 while it
/// lives or when no hit killed it [documented: GECK wiki GetKillingBlowLimb]
/// (`RefExtra::killing_hit`).
pub fn get_killing_blow_limb(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if !g.is_dead(r) {
        return Ok(Value::Num(-1.0));
    }
    Ok(Value::Num(g.extra.get(&r).and_then(|e| e.killing_hit).map_or(-1, |h| h.location) as f64))
}

/// `actor.GetArmorRating`: the total damage resistance of the armour the actor has
/// equipped [documented: catalogue summary]: the sum of the equipped ARMO's DNAM DR
/// (stored × 100, [documented: xEdit wbDiv(100)]), rounded down [hypothesis: an
/// integer, as the page's return type]. New Vegas armour protects by its DT, and most
/// armour has DR 0 [observed: `fnvtool esm decode`]; the one shipped use scales the
/// target's rating by 0.15 (FalloutNV.esm `SCPT 0x0011D9EF`) [observed]. Equipped is the
/// world's equipped flags; the condition of the armour is not counted [hypothesis].
pub fn get_armor_rating(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let forms = g.world.forms;
    let equipped = g.world.get(r).ok().and_then(|s| s.inventory.as_ref()).map(|i| i.equipped()).unwrap_or_default();
    let dr: f32 = equipped.into_iter().filter_map(|i| forms.item(i)).filter(|it| it.kind == ItemKind::Armor).filter_map(|it| it.armor_dr).sum();
    Ok(Value::Num(dr.floor() as f64))
}

/// `ref.FireWeapon weapon`: the reference fires one shot of the weapon, from its centre
/// straight ahead; it need not be an actor [documented: catalogue summary and notes].
/// The shot is the engine's: the request waits in `Game::combat_requests`
/// (`CombatRequest::Fire`). Not done [engineering choice, noted]: the model's projectile
/// node, the player's ammunition and spread; the fired weapon's enchantments and on-hit
/// blocks do not take effect [documented], and none run here.
pub fn fire_weapon(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let from = ctx.subject()?;
    let weapon = a.form(0)?;
    g.combat_requests.push(crate::combat::CombatRequest::Fire { from, weapon });
    none()
}

/// `Exists [target]` (combat dialogue conditions): 1 when the condition's subject and
/// target are different references [documented: catalogue summary]. In combat dialogue
/// the target is a near member of the speaker's combat group, or the speaker itself when
/// none is near [documented: catalogue notes]; NVForever's combat lines take the foe as
/// the target (`Game::detection_bark`), so for a subject in a combat state the answer is
/// whether its combat group has another member ([`Game::combat_group`]) [hypothesis:
/// "near" not measured]. Elsewhere the condition's target (`Game::condition_target`).
pub fn exists(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if g.awareness.state(r) != State::Normal {
        return yes(g.combat_group(r).len() > 1);
    }
    yes(g.condition_target.is_some_and(|t| t != r))
}

fn position(g: &Game, r: FormId) -> Option<[f32; 3]> {
    g.world.peek(r).map(|s| s.pos).or_else(|| g.world.placed.get(r).map(|p| p.pos))
}
