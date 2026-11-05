//! Creatures' body weapons (docs/design/combat-ai.md §19): a creature fights with the
//! "Embedded Weapon" records it carries (Mister Handy's buzzsaw and flamer, Mister
//! Gutsy's plasma gun and flamer, a protectron's lasers, a fire gecko's flame) and
//! switches between them by its target's distance.
//!
//! - **Which arms** (once per creature, [`attach`]): each embedded weapon in its
//!   inventory (not "Player Only" or "Not Used In Normal Combat") whose stance its
//!   skeleton has (`<prefix>aim.kf` or `holster`, at the folder's root or in its
//!   `specialanims` folder: `weapons::Stance::load`), and its own attack where its folder
//!   has `h2hattack…` clips (a fire gecko's bite). Creatures with a drawable weapon in
//!   hand (`outfits::arm_creatures`) are left to that.
//! - **Using one**: a body weapon is drawn as a weapon without a model: a
//!   [`WeaponHold`] with its stance (its draw cycle plays the stance's equip, aim and
//!   unequip clips; `alert.rs` draws it in a fight) and a [`BodyWeapon`] on the root, which
//!   `npc_combat` reads as the weapon drawn: a gun fires its projectile, anything else
//!   strikes with the weapon's damage and reach. Its shots leave from the weapon's node
//!   (WEAP NNAM: `ProjectileNode_HeadLaser` for the flamers, `…HeadLaser01` for the
//!   plasma gun; `npc_combat`'s embedded-node rule, as for any embedded gun). Its blows and shots play the stance's
//!   attack clips (`attack_anims`: `1hmattackright_chop.kf`, `1hpattackright.kf`,
//!   `2hlattackspin.kf`). The own attack is the creature's stance (`creature_stance.rs`)
//!   and its own `h2h` attack clips, as before.
//! - **Switching** ([`switch`], the rule: `fnv_combat::body_weapon`): while it fights,
//!   every [`body_weapon::SWITCH_SECONDS`] at most it takes up the arm the rule chooses
//!   for its target's distance; the new arm's equip clip plays and it attacks once that
//!   is drawn. A gun whose projectile explodes suits no target within its blast (the
//!   rule's `Arm::blast`, `npc_combat` holding fire there), unless the creature's combat
//!   style has Ignore Damaging Self: a sentry bot keeps its minigun inside its missile
//!   launcher's 1000 units (#1621). Its first choice in a fight is made at once (the arm drawn before the fight
//!   was chosen for [`FIRST_DISTANCE`]), so a Mister Handy that fights at its target's
//!   feet takes up its saw rather than stepping back with its flamer; and a creature with
//!   a melee arm ([`BodyMelee`]) does not step back from a target too near
//!   (`npc_combat`, `combat_tactics::BACK_OFF_WITHIN`) [design]: it strikes. An arm whose limb (WEAP "Embedded Weapon Actor Value": Mister Handy's
//!   flamer Left Attack, buzzsaw Right Attack) is crippled to 0 is left out
//!   [hypothesis: the wiki's "the condition value affected when the weapon is targeted"];
//!   with none left it fights with the creature's own damage and no clip.
//! - Body weapons do not wear (`nvf_play::combat::wear_weapon`) or reload (`npc_combat`:
//!   no magazine) [design], and slow nothing (the two-handed run penalty is for weapons
//!   in hand).

use crate::actors::ActorWorld;
use crate::creature_stance::{CreatureClips, CreatureStance, NoStance, StanceAttach};
use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::npc_combat::{gun_reach_of, is_gun, Fighters};
use crate::outfits::CreatureArms;
use crate::weapons::{Stance, WeaponHold, WeaponMode};
use bevy::prelude::*;
use fnv_actor::weapon::AnimType;
use fnv_combat::body_weapon::{self, Arm};
use fnv_sim::FormId;
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::anim_play::BlendPlayer;
use nvf_assets::model::ModelCtx;
use nvf_play::combat::Combat;
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub struct BodyWeaponsPlugin;

impl Plugin for BodyWeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BodyStats>()
            .add_systems(Update, (attach.after(StanceAttach), switch.after(crate::npc_combat::fight)))
            .add_systems(Last, report);
    }
}

/// On a creature's root: the body weapon it fights with now (`npc_combat` reads it as
/// the weapon drawn on it).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyWeapon {
    pub reference: u32,
    pub weapon: FormId,
}

/// On a creature's root when one of its arms is a melee one (`npc_combat`: it does not
/// step back from a target too near; module notes).
#[derive(Component, Clone, Copy, Debug)]
pub struct BodyMelee;

/// One of a creature's arms.
struct BodyArm {
    /// The body weapon; `None`: its own attack.
    weapon: Option<FormId>,
    name: String,
    stance: Option<Arc<Stance>>,
    /// The limb-condition actor value that disables it at 0.
    limb: Option<u16>,
    /// Its projectile's explosion radius (0: none); the rule's `blast` unless the
    /// creature's combat style ignores damaging itself.
    blast: f32,
    rule: Arm,
}

/// On a creature with body weapons: its arms, the one in use and when it took that up
/// (`None`: not yet in a fight).
#[derive(Component)]
pub struct BodyArms {
    arms: Vec<BodyArm>,
    current: Option<usize>,
    since: Option<f32>,
    /// Its own unarmed stance, given back when it takes up its own attack again.
    own: Option<Arc<CreatureClips>>,
}

/// On a creature [`attach`] has looked at.
#[derive(Component)]
struct BodyChecked;

#[derive(Resource, Default)]
struct BodyStats {
    /// Creatures given body weapons: "name id: arm, arm …".
    armed: BTreeMap<u32, String>,
    /// Switches between arms, by (from, to) arm names.
    switches: BTreeMap<(String, String), usize>,
    /// Arms left out once because their limb was crippled.
    crippled: usize,
    /// Embedded weapons whose stance the skeleton lacks, by "folder prefix".
    no_stance: BTreeMap<String, usize>,
}

/// The distance the first arm is chosen for, before any fight, units [design: a target
/// at a gun's distance, so a creature with a gun shows it first].
const FIRST_DISTANCE: f32 = 1024.0;

type Fresh<'w> = (Entity, &'w ActorRig, &'w CreatureArms, Option<&'w CreatureStance>, Option<&'w mut BlendPlayer>);
type Unchecked = (Without<BodyChecked>, Without<Dying>, Or<(With<CreatureStance>, With<NoStance>)>);

/// Give each creature its arms, once (module notes).
#[allow(clippy::too_many_arguments)]
fn attach(
    mut commands: Commands,
    world: Option<Res<ActorWorld>>,
    combat: Option<Res<Combat>>,
    sim: Res<SimState>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut stats: ResMut<BodyStats>,
    mut own_attacks: Local<HashMap<String, bool>>,
    mut fresh: Query<Fresh, Unchecked>,
) {
    let (Some(world), Some(combat)) = (world, combat) else { return };
    if world.weapon_mode == WeaponMode::None || fresh.is_empty() {
        return;
    }
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for (e, rig, held, own_stance, player) in &mut fresh {
        // A weapon in hand is `outfits::arm_creatures`'.
        if held.weapon.is_some() {
            commands.entity(e).insert(BodyChecked);
            continue;
        }
        let me = FormId(rig.reference);
        // Its inventory once the rules have it.
        let Some(inv) = sim.world.peek(me).and_then(|s| s.inventory.as_ref()) else { continue };
        let Some(player) = player else { continue };
        let folder = folder_of(&rig.skeleton);
        let mut arms = Vec::new();
        for (item, count) in inv.iter() {
            let Some(ws) = combat.forms.weapons.get(&item).filter(|w| w.embedded && count > 0) else { continue };
            if ws.flags.iter().any(|f| f == "Player Only" || f == "Not Used In Normal Combat") || arms.iter().any(|a: &BodyArm| a.weapon == Some(item)) {
                continue;
            }
            let Some(prefix) = ws.anim_type.as_deref().and_then(AnimType::from_name).map(AnimType::prefix) else { continue };
            let Some(stance) = world.anims.stance(&mut ctx, &rig.skeleton, &rig.skel, prefix) else {
                *stats.no_stance.entry(format!("{folder} {prefix}")).or_default() += 1;
                continue;
            };
            let gun = is_gun(&combat, ws, item);
            let dps = body_weapon::damage_per_second(ws.base_damage, ws.projectile_count, ws.attack_shots_per_sec, ws.fire_rate);
            let name = ws.edid.clone().unwrap_or_else(|| item.to_string());
            let blast = if gun { gun_reach_of(&combat, item, ws, 1.0).blast } else { 0.0 };
            arms.push(BodyArm { weapon: Some(item), name, stance: Some(stance), limb: ws.embedded_limb, blast, rule: Arm { melee: !gun, min_range: ws.min_range, max_range: ws.max_range, blast, dps, usable: true } });
        }
        commands.entity(e).insert(BodyChecked);
        if arms.is_empty() {
            continue;
        }
        // Its own attack, where its folder has clips for it.
        let has_own = *own_attacks.entry(folder.clone()).or_insert_with(|| {
            let pre = format!("{folder}\\h2hattack");
            !ctx.models.data.vfs.paths(|p| p.starts_with(&pre) && p.ends_with(".kf")).is_empty()
        });
        if has_own {
            arms.push(BodyArm { weapon: None, name: "its own attack".into(), stance: None, limb: None, blast: 0.0, rule: Arm { melee: true, min_range: 0.0, max_range: 0.0, blast: 0.0, dps: 0.0, usable: true } });
        }
        let rules: Vec<Arm> = arms.iter().map(|a| a.rule).collect();
        let first = body_weapon::choose(&rules, FIRST_DISTANCE, None).unwrap_or(0);
        let list = arms.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", ");
        let label = format!("{} {:08X}", rig.skeleton, rig.reference);
        info!("body weapons: {label} fights with {list}; first {}", arms[first].name);
        stats.armed.insert(rig.reference, format!("{:08X}: {list}", rig.reference));
        let melee = arms.iter().any(|a| a.rule.melee);
        let mut body = BodyArms { arms, current: None, since: None, own: own_stance.map(|s| s.clips.clone()) };
        take_up(&mut commands, e, rig, player.into_inner(), &mut body, Some(first), false);
        commands.entity(e).insert(body);
        if melee {
            commands.entity(e).insert(BodyMelee);
        }
    }
}

/// Take up arm `to` (`None`: none at all): its stance and weapon, or the creature's own
/// attack (module notes).
fn take_up(commands: &mut Commands, e: Entity, rig: &ActorRig, player: &mut BlendPlayer, body: &mut BodyArms, to: Option<usize>, drawn: bool) {
    (player.layers[2], player.layers[3]) = (None, None);
    body.current = to;
    let arm = to.and_then(|i| body.arms.get(i));
    match arm.and_then(|a| Some((a.weapon?, a.stance.clone()?))) {
        Some((weapon, stance)) => {
            let parent = rig.skel.parents[stance.weapon_node].unwrap_or(0);
            let mut hold = WeaponHold::new(stance, false, parent);
            hold.body = true;
            hold.drawn = drawn;
            hold.layers(player);
            commands.entity(e).remove::<CreatureStance>().insert((hold, BodyWeapon { reference: rig.reference, weapon }));
        }
        None => {
            commands.entity(e).remove::<(WeaponHold, BodyWeapon)>();
            if let Some(clips) = body.own.clone().filter(|_| arm.is_some()) {
                commands.entity(e).insert(CreatureStance::new(clips, drawn));
            }
        }
    }
}

type Armed<'w> = (Entity, &'w ActorRig, &'w mut BodyArms, &'w mut BlendPlayer, Option<&'w WeaponHold>, Option<&'w CreatureStance>);

/// Switch arms in a fight by the target's distance (module notes).
#[allow(clippy::too_many_arguments)]
fn switch(
    mut commands: Commands,
    time: Res<Time>,
    fighters: Res<Fighters>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut stats: ResMut<BodyStats>,
    mut armed: Query<Armed, Without<Dying>>,
) {
    let now = time.elapsed_secs();
    for (e, rig, mut body, mut player, hold, own) in &mut armed {
        let id = rig.reference;
        let Some(d) = fighters.distance(id) else { continue };
        // The first choice in a fight at once, later ones apart (module notes).
        if body.since.is_some_and(|t| now - t < body_weapon::SWITCH_SECONDS) {
            continue;
        }
        // Arms on a crippled limb are left out; a blast counts unless its combat style
        // ignores damaging itself.
        let limbs: Vec<Option<u16>> = body.arms.iter().map(|a| a.limb).collect();
        let (usable, ignores_self): (Vec<bool>, bool) = match script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            Some(rt) => with_world(rt, &mut sim, |rt| {
                let usable = limbs.iter().map(|l| l.is_none_or(|code| !rt.game.is_crippled(FormId(id), code))).collect();
                (usable, rt.game.combat_style(FormId(id)).is_some_and(|s| s.ignore_damaging_self))
            }),
            None => (vec![true; limbs.len()], false),
        };
        for (a, u) in body.arms.iter_mut().zip(usable) {
            if a.rule.usable && !u {
                stats.crippled += 1;
                info!("body weapons: {id:08X} can no longer use {} (its limb is crippled)", a.name);
            }
            a.rule.usable = u;
            a.rule.blast = if ignores_self { 0.0 } else { a.blast };
        }
        let rules: Vec<Arm> = body.arms.iter().map(|a| a.rule).collect();
        let to = body_weapon::choose(&rules, d, body.current);
        if to == body.current {
            body.since.get_or_insert(now);
            continue;
        }
        let name = |i: Option<usize>| i.and_then(|i| body.arms.get(i)).map_or("nothing".to_string(), |a| a.name.clone());
        let (from_name, to_name) = (name(body.current), name(to));
        info!("body weapons: {id:08X} switches from {from_name} to {to_name}, its target {d:.0} units away");
        *stats.switches.entry((from_name, to_name)).or_default() += 1;
        let drawn = hold.is_some_and(|h| h.drawn) || own.is_some_and(|s| s.drawn);
        body.since = Some(now);
        take_up(&mut commands, e, rig, &mut player, &mut body, to, drawn);
    }
}

/// The folder of a skeleton path (`creatures\mistergutsy\skeleton.nif` →
/// `meshes\creatures\mistergutsy`).
fn folder_of(skeleton: &str) -> String {
    let s = fnv_vfs::normalize(&fnv_actor::resolve::mesh_path(skeleton));
    s.rfind('\\').map_or(String::new(), |i| s[..i].to_string())
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<BodyStats>) {
    if exit.read().next().is_none() {
        return;
    }
    let switches: Vec<String> = stats.switches.iter().map(|((a, b), n)| format!("{a} -> {b} x{n}")).collect();
    info!(
        "body weapons summary: {} creatures armed ({}); {} switches ({}); {} arms lost to crippled limbs; no stance for: {:?}",
        stats.armed.len(),
        stats.armed.values().cloned().collect::<Vec<_>>().join("; "),
        stats.switches.values().sum::<usize>(),
        switches.join(", "),
        stats.crippled,
        stats.no_stance,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skeletons_folder() {
        assert_eq!(folder_of(r"Creatures\MisterGutsy\Skeleton.nif"), r"meshes\creatures\mistergutsy");
    }
}
