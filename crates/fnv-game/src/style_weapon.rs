//! The weapon a fighting NPC holds, by its combat style's Weapon Restrictions
//! (docs/spec-notes/companion-wheel.md §4).
//!
//! - **The rule** [documented: GECK wiki Combat Style, Simple tab, "Weapon Restrictions -
//!   Either None, Melee Only, or Ranged Only. If Melee Only is selected, the combatant will
//!   always use melee/hand-to-hand as their primary weapon. If Ranged Only is selected, the
//!   combatant will always use ranged weapons as their primary weapon"]: while an NPC
//!   fights, a weapon held that its style rules out is swapped for the carried weapon its
//!   style allows, the one with the highest damage as the outfitting's own choice counts
//!   it (a gun's explosion counted, the first carried on ties; `fnv_sim::World::equip_default`)
//!   [hypothesis: which of the allowed weapons the original takes is not documented]. With none allowed it keeps
//!   what it holds (a Melee Only actor holding only a gun fights with its fists,
//!   `npc_combat::arm_of`).
//! - **Melee and ranged** by the weapon's animation type [hypothesis: the style's words
//!   "melee/hand-to-hand"]: Hand to Hand, Melee (1 Hand) and Melee (2 Hand) are melee; the
//!   pistols, rifles, Handle and Launcher are ranged. Grenades, mines and thrown weapons are
//!   never taken up as the primary weapon [hypothesis].
//! - **No weapon held**: an NPC that fights with nothing equipped while it carries a weapon
//!   its style allows takes that one up as well [hypothesis: the GECK wiki's UnequipItem
//!   notes that companions do not stay unequipped, "NoEquipFlag … ignored by companions";
//!   generalised to every NPC in combat]: the best by the re-arming rank
//!   (`crate::rearm`, docs/design/combat-ai.md §22: its own class, damage per second, a gun
//!   only with ammunition) [design: owner 2026-10-25]. A weapon a script locked off (UnequipItem's
//!   NoEquip flag) and a broken one are not taken; one locked on is not swapped out
//!   (`fnv_sim::World::equip`).
//! - **Who**: NPCs only (creatures fight with their bodies' attacks), alive, in combat
//!   (`Game::fighting`), every frame, so a companion switched to melee or ranged on the
//!   companion wheel mid-fight (SetCombatStyle FollowersCombatStyleMelee /
//!   FollowersCombatStyleRanged, Melee Only / Ranged Only [observed: `fnvtool esm decode`])
//!   changes weapon at once. Out of combat nothing changes [hypothesis]. The swap is an
//!   ordinary equip: OnUnequip and OnEquip run, and the engine draws what the rules say
//!   the actor holds (`outfits::follow_inventory`).
//! - **Before re-arming**: re-arming (`fnv_ai::rearm`, which runs in every combat state,
//!   Alert and Lost too) counts a weapon its style rules out as no usable weapon; it asks
//!   this rule first ([`Game::arm_restricted_by_style`]), so a held weapon the style rules
//!   out is swapped here, by this rule's pick, and logged with the style as its reason
//!   (#1721). Re-arming keeps the empty hand and the better weapon.

use crate::game::{event, Game};
use fnv_script::vm::Value;
use fnv_sim::forms::ItemKind;
use fnv_sim::FormId;

/// A weapon's place under a style's Weapon Restrictions (module notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Melee,
    Ranged,
    /// Grenades, mines, thrown weapons, and weapons of no known type: never a primary weapon.
    Other,
}

/// The reach of a weapon of WEAP DNAM animation type `code` (0 Hand to Hand … 13 Thrown).
pub fn reach_of(code: Option<i64>) -> Reach {
    match code {
        Some(0..=2) => Reach::Melee,
        Some(3..=9) => Reach::Ranged,
        _ => Reach::Other,
    }
}

/// Whether a weapon of reach `r` may be the primary weapon under restriction `restriction`
/// (0 None, 1 Melee Only, 2 Ranged Only).
pub fn allowed(restriction: u32, r: Reach) -> bool {
    match restriction {
        1 => r == Reach::Melee,
        2 => r == Reach::Ranged,
        _ => r != Reach::Other,
    }
}

/// Of the `carried` weapons (id, base damage, reach), the one to take up: the allowed one
/// with the highest damage, the first on ties.
pub fn pick(carried: &[(FormId, f32, Reach)], restriction: u32) -> Option<FormId> {
    let mut best: Option<(f32, FormId)> = None;
    for &(id, damage, r) in carried {
        if allowed(restriction, r) && best.is_none_or(|(d, _)| damage > d) {
            best = Some((damage, id));
        }
    }
    best.map(|b| b.1)
}

impl Game<'_> {
    /// Every fighting NPC takes up the weapon its combat style allows (module notes).
    pub fn arm_by_style(&mut self) {
        let fighters: Vec<FormId> = self.fighting.iter().copied().collect();
        for actor in fighters {
            self.arm_one_by_style(actor);
        }
    }

    /// The weapon `actor` should swap to now, if any (module notes).
    pub fn style_weapon_swap(&mut self, actor: FormId) -> Option<FormId> {
        if actor == FormId::PLAYER_REF || self.is_dead(actor) {
            return None;
        }
        let base = crate::functions::base_of(self, actor)?;
        if self.data.sig(base) != Some(*b"NPC_") {
            return None;
        }
        let restriction = self.combat_style(actor).map_or(0, |s| s.weapon_restrictions);
        let forms = self.world.forms;
        let inv = self.world.peek(actor)?.inventory.as_ref()?;
        let reach = |w: FormId| reach_of(self.data.weapon_anim_types.get(&w).copied());
        let held = inv.equipped().into_iter().find(|&i| forms.item(i).is_some_and(|it| it.kind == ItemKind::Weapon));
        if held.is_some_and(|w| allowed(restriction, reach(w))) {
            return None;
        }
        // An unrestricted actor keeps whatever it holds; only an empty hand looks, by the
        // re-arming rank.
        if held.is_some() && restriction == 0 {
            return None;
        }
        if held.is_none() {
            return self.best_carried(actor).map(|(w, _, _)| w);
        }
        let carried: Vec<(FormId, f32, Reach)> = inv
            .iter()
            .filter(|&(i, n)| n > 0 && Some(i) != held && inv.has_sound(i))
            .filter_map(|(i, _)| {
                let it = forms.item(i).filter(|it| it.kind == ItemKind::Weapon && it.drawable && !it.not_for_npcs)?;
                Some((i, it.weapon_damage.unwrap_or(0.0), reach(i)))
            })
            .collect();
        pick(&carried, restriction)
    }

    /// `actor`, holding a weapon its combat style rules out, takes up the one the style
    /// allows (module notes, "Before re-arming"). Returns the weapon taken up.
    pub fn arm_restricted_by_style(&mut self, actor: FormId) -> Option<FormId> {
        self.held_weapon(actor)?;
        self.arm_one_by_style(actor)
    }

    fn arm_one_by_style(&mut self, actor: FormId) -> Option<FormId> {
        let weapon = self.style_weapon_swap(actor)?;
        let e = self.world.equip(actor, weapon).ok()?;
        let why = match self.combat_style(actor).map_or(0, |s| s.weapon_restrictions) {
            1 => "its combat style is Melee Only",
            2 => "its combat style is Ranged Only",
            _ => "it held no weapon",
        };
        let off = e.unequipped.iter().map(|w| self.data.name(*w)).collect::<Vec<_>>().join(", ");
        tracing::info!(
            "style weapon: {} {actor} takes up {} {weapon}{} ({why})",
            self.data.name(actor),
            self.data.name(weapon),
            if off.is_empty() { String::new() } else { format!(" in place of {off}") },
        );
        for u in e.unequipped {
            self.item_event(actor, u, event::ON_UNEQUIP, Some(Value::Ref(actor.0)));
        }
        self.equip_events(actor, weapon, false);
        self.sync_actor_effects(actor);
        Some(weapon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_by_anim_type() {
        assert_eq!(reach_of(Some(0)), Reach::Melee);
        assert_eq!(reach_of(Some(2)), Reach::Melee);
        assert_eq!(reach_of(Some(3)), Reach::Ranged);
        assert_eq!(reach_of(Some(9)), Reach::Ranged);
        assert_eq!(reach_of(Some(10)), Reach::Other);
        assert_eq!(reach_of(Some(13)), Reach::Other);
        assert_eq!(reach_of(None), Reach::Other);
    }

    #[test]
    fn picks_the_strongest_allowed() {
        let (gun, knife, machete, grenade) = (FormId(1), FormId(2), FormId(3), FormId(4));
        let carried = [(gun, 30.0, Reach::Ranged), (knife, 10.0, Reach::Melee), (machete, 12.0, Reach::Melee), (grenade, 100.0, Reach::Other)];
        assert_eq!(pick(&carried, 1), Some(machete));
        assert_eq!(pick(&carried, 2), Some(gun));
        assert_eq!(pick(&carried, 0), Some(gun));
        assert_eq!(pick(&carried[..1], 1), None);
        // Ties: the first carried.
        assert_eq!(pick(&[(knife, 10.0, Reach::Melee), (machete, 10.0, Reach::Melee)], 1), Some(knife));
    }
}
