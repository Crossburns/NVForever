//! NPCs throwing grenades (docs/design/combat-ai.md §20). No page says when the
//! original's actors throw one: the combat style has no grenade field and FalloutNV.esm
//! sets no such setting [observed: CSTY's fields, the GMSTs named Grenade or Throw];
//! combat is not a parity target (docs/design/combat.md), so the rules are ours [design]:
//!
//! - **What**: a weapon of animation type "Grenade Throw (1 Hand)" whose projectile
//!   explodes (frag, plasma and pulse grenades, dynamite), from the actor's inventory, one
//!   used up per throw: unlike guns, which load rounds from nowhere (owner 2026-10-17),
//!   a grenade thrown is gone, so a fighter carries as many throws as grenades.
//! - **Who**: only an actor whose held weapon is the grenade. A gunman does not throw the
//!   grenades it carries [observed: observer check 20, o13_airport 2026-11-01: Powder
//!   Gangers carrying 4 and 5 dynamite threw none in 90 s at 426-1,595 units; one holding
//!   its dynamite threw once at about 1,225 units, then drew a gun]. In ours a gun or a
//!   melee weapon is always held before a thrown one (`fnv_actor::weapon::choose_weapon`)
//!   and re-arming passes grenades over, so it draws a grenade only when no preferred
//!   held weapon is available.
//! - **When**: holding it, not fleeing, ready on a target it sees, first [`FIRST_AFTER`]
//!   seconds into seeing it, then every
//!   [`BETWEEN`] seconds (both from observer check 20); only at a target beyond
//!   [`MIN_THROW`] and within its throw's reach (the low arc's, under
//!   the projectile's gravity, no farther than [`MAX_THROW`]); held, as an explosive shot
//!   is, when a comrade or a bystander would be caught (combat-ai.md §17.6).
//! - **How**: from its eyes on the low arc that lands on the target's feet (as VATS
//!   mode's throws, `fnv_combat::vats::lob_dir`), the grenade's throw clip played, its
//!   FireExplosive line tried (`fnv_game::combat_lines`), the shot's own explosion and
//!   the others' running from it as for the player's grenades (§17.7). No firearm cycle
//!   runs while it holds a grenade, even while a throw waits or is denied [design].

use fnv_combat::data::CombatForms;
use fnv_sim::FormId;

/// The animation type of grenades [documented: GECK wiki Weapons, Animation Type].
pub const GRENADE_ANIM: &str = "Grenade Throw (1 Hand)";
/// The first throw of a fight comes this many seconds (lowest, highest) into seeing the
/// target [observed: observer check 20, captures o18_air and o24_air 2026-11-01: an NCR
/// trooper holding a frag grenade threw within a second of combat, from 800 and from
/// 1,200 units (#1351). It was 4 to 10 s, a design number].
pub const FIRST_AFTER: (f32, f32) = (0.0, 1.0);
/// Then one every this many seconds (lowest, highest) [observed: the same captures, throws
/// at 1.8, 5.4, 7.2 and 9.2 s, and at 0, 1.6, 4.0, 5.7 and 9.3 s: 1.6 to 3.6 s apart. It
/// was 12 to 25 s].
pub const BETWEEN: (f32, f32) = (1.6, 3.6);
/// No throw at a target nearer than this, game units [design, between the observed: the
/// trooper threw from 800 units, and two troopers 300 units off threw none and closed in
/// to strike (observer check 20); the original's own floor is not pinned. It was 1.1 times
/// the blast's radius (990 for a frag), which kept ours from the 800-unit throws].
pub const MIN_THROW: f32 = 550.0;
/// No throw farther than this, game units (about 21 m) [design].
pub const MAX_THROW: f32 = 1500.0;

/// A grenade an actor carries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grenade {
    pub weapon: FormId,
    /// The projectile's speed (units/s) and gravity multiplier.
    pub speed: f32,
    pub gravity_mult: f32,
    /// Its explosion's radius.
    pub blast: f32,
}

/// The grenade among `items` (an inventory's items and counts) a fighter throws: of the
/// grenades it carries, the one of lowest FormID [design: one kind at a time].
pub fn carried(forms: &CombatForms, items: impl Iterator<Item = (FormId, i32)>) -> Option<Grenade> {
    items
        .filter(|(_, n)| *n > 0)
        .filter_map(|(id, _)| {
            let w = forms.weapons.get(&id)?;
            if w.anim_type.as_deref() != Some(GRENADE_ANIM) {
                return None;
            }
            let proj = forms.chain(id, None)?.projectile?;
            let p = forms.projectiles.get(&proj)?;
            let blast = forms.explosions.get(&p.explodes()?)?.radius;
            (blast > 0.0 && p.speed > 0.0).then_some(Grenade { weapon: id, speed: p.speed, gravity_mult: p.gravity, blast })
        })
        .min_by_key(|g| g.weapon)
}

/// How far `g` is thrown under `gravity` (units/s²): the low arc's reach, at most
/// [`MAX_THROW`].
pub fn reach(g: &Grenade, gravity: f32) -> f32 {
    fnv_combat::vats::throw_reach(g.speed, g.gravity_mult * gravity).min(MAX_THROW)
}

/// Whether a target `distance` away is one to throw `g` at: beyond [`MIN_THROW`], within
/// the throw's reach.
pub fn in_band(g: &Grenade, distance: f32, gravity: f32) -> bool {
    distance > MIN_THROW && distance <= reach(g, gravity)
}

/// The seconds until the next throw, from `lo`..`hi` by `u` in [0, 1).
pub fn wait(range: (f32, f32), u: f32) -> f32 {
    range.0 + (range.1 - range.0) * u.clamp(0.0, 1.0)
}

/// The inputs shared by every decision of a held-grenade controller. `ready` includes
/// the person's sight, draw, reaction and attack holds, and excludes fleeing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ThrowInput {
    pub target: FormId,
    pub clock: f32,
    pub ready: bool,
    pub origin: [f32; 3],
    pub target_at: [f32; 3],
    pub gravity: f32,
}

/// A prepared throw; it becomes an emission only after [`commit`] spends its item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Throw {
    pub grenade: Grenade,
    pub origin: [f32; 3],
    pub aim: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Decision {
    Waiting,
    Empty,
    OutOfBand(Grenade),
    Held { grenade: Grenade, by: FormId },
    Throw(Throw),
}

/// Advance only the grenade controller, retaining the established target deadlines
/// and retry policy [design]. Selection and protection checks run only when needed;
/// neither a waiting nor a denied throw has any emission or inventory side effect.
pub(crate) fn plan<C>(
    deadline: &mut Option<(FormId, f32)>,
    input: ThrowInput,
    context: &mut C,
    select: impl FnOnce(&mut C) -> Option<Grenade>,
    protected: impl FnOnce(&mut C, &Grenade) -> Option<FormId>,
    mut random: impl FnMut() -> f32,
) -> Decision {
    if !input.ready {
        return Decision::Waiting;
    }
    let due = match *deadline {
        Some((target, at)) if target == input.target => at,
        _ => {
            let at = input.clock + wait(FIRST_AFTER, random());
            *deadline = Some((input.target, at));
            at
        }
    };
    if input.clock < due {
        return Decision::Waiting;
    }
    let Some(grenade) = select(context) else {
        *deadline = Some((input.target, input.clock + wait(BETWEEN, random())));
        return Decision::Empty;
    };
    let distance = fnv_combat::vec::len(fnv_combat::vec::sub(input.target_at, input.origin));
    if !in_band(&grenade, distance, input.gravity) {
        *deadline = Some((input.target, input.clock + 1.0));
        return Decision::OutOfBand(grenade);
    }
    let caught = protected(context, &grenade);
    *deadline = Some((input.target, input.clock + wait(BETWEEN, random())));
    if let Some(by) = caught {
        return Decision::Held { grenade, by };
    }
    let gravity = grenade.gravity_mult * input.gravity;
    let aim = fnv_combat::vats::lob_dir(input.origin, input.target_at, grenade.speed, gravity).unwrap_or_else(|| fnv_combat::vats::lob_far(input.origin, input.target_at));
    Decision::Throw(Throw { grenade, origin: input.origin, aim })
}

/// Commit exactly one selected item before the caller publishes a shot, animation,
/// success count or bark. A stale inventory or a failed removal cannot emit a throw.
pub(crate) fn commit<E>(throw: Throw, consume: impl FnOnce(FormId) -> Result<i32, E>) -> Option<Throw> {
    matches!(consume(throw.grenade.weapon), Ok(1)).then_some(throw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_combat::data::{ExplosionStats, ProjectileStats, WeaponStats};
    use fnv_sim::inventory::Inventory;

    fn input(clock: f32) -> ThrowInput {
        ThrowInput { target: FormId(9), clock, ready: true, origin: [0.0, 0.0, 64.0], target_at: [1000.0, 0.0, 0.0], gravity: 686.0 }
    }

    fn forms() -> CombatForms {
        let mut forms = CombatForms::default();
        for (id, speed, blast) in [(1, 1000.0, 256.0), (2, 1200.0, 300.0)] {
            forms.weapons.insert(FormId(id), WeaponStats { anim_type: Some(GRENADE_ANIM.into()), projectile: Some(FormId(id + 10)), projectile_count: 1, ..Default::default() });
            forms.projectiles.insert(FormId(id + 10), ProjectileStats { kind: Some("Lobber".into()), speed, gravity: 1.0, flags: vec!["Explosion".into()], explosion: Some(FormId(id + 20)), ..Default::default() });
            forms.explosions.insert(FormId(id + 20), ExplosionStats { radius: blast, ..Default::default() });
        }
        forms
    }

    #[test]
    fn held_grenade_deadline_starts_only_when_ready_and_is_not_rearmed_each_frame() {
        let mut deadline = None;
        assert_eq!(plan(&mut deadline, ThrowInput { ready: false, ..input(5.0) }, &mut (), |_| panic!("not ready"), |_, _| panic!("not ready"), || panic!("not ready")), Decision::Waiting);
        assert_eq!(deadline, None);
        assert_eq!(plan(&mut deadline, input(10.0), &mut (), |_| panic!("before first deadline"), |_, _| panic!("before first deadline"), || 0.5), Decision::Waiting);
        assert_eq!(deadline, Some((FormId(9), 10.5)));
        for clock in [10.0, 10.25, 10.499] {
            assert_eq!(plan(&mut deadline, input(clock), &mut (), |_| panic!("before deadline"), |_, _| panic!("before deadline"), || panic!("must not reroll")), Decision::Waiting);
            assert_eq!(deadline, Some((FormId(9), 10.5)));
        }
        assert!(matches!(plan(&mut deadline, input(10.5), &mut (), |_| Some(frag()), |_, _| None, || 0.0), Decision::Throw(_)));
        assert_eq!(deadline, Some((FormId(9), 12.1)));
        // A new target rolls its first wait; a temporary hold does not erase a wait.
        assert_eq!(plan(&mut deadline, ThrowInput { target: FormId(8), ..input(20.0) }, &mut (), |_| panic!("new target wait"), |_, _| None, || 1.0), Decision::Waiting);
        assert_eq!(deadline, Some((FormId(8), 21.0)));
        assert_eq!(plan(&mut deadline, ThrowInput { ready: false, ..input(40.0) }, &mut (), |_| panic!("held"), |_, _| None, || panic!("held")), Decision::Waiting);
        assert_eq!(deadline, Some((FormId(8), 21.0)));
    }

    #[test]
    fn held_grenade_denials_keep_their_existing_retry_delays() {
        let mut deadline = Some((FormId(9), 1.0));
        assert_eq!(plan(&mut deadline, input(1.0), &mut (), |_| None, |_, _| panic!("no item"), || 1.0), Decision::Empty);
        assert_eq!(deadline, Some((FormId(9), 4.6)));
        for target_at in [[100.0, 0.0, 0.0], [2000.0, 0.0, 0.0]] {
            deadline = Some((FormId(9), 1.0));
            assert_eq!(plan(&mut deadline, ThrowInput { target_at, ..input(1.0) }, &mut (), |_| Some(frag()), |_, _| panic!("out of band"), || panic!("range retry has no roll")), Decision::OutOfBand(frag()));
            assert_eq!(deadline, Some((FormId(9), 2.0)));
        }
        deadline = Some((FormId(9), 1.0));
        assert_eq!(plan(&mut deadline, input(1.0), &mut (), |_| Some(frag()), |_, _| Some(FormId(7)), || 0.0), Decision::Held { grenade: frag(), by: FormId(7) });
        assert_eq!(deadline, Some((FormId(9), 2.6)));
    }

    #[test]
    fn held_grenade_callbacks_borrow_one_context_only_when_needed() {
        let mut calls = Vec::new();
        let mut deadline = None;
        for (clock, ready) in [(0.0, false), (0.0, true), (0.4, true)] {
            assert_eq!(plan(
                &mut deadline, ThrowInput { ready, ..input(clock) }, &mut calls,
                |calls| { calls.push("select"); Some(frag()) },
                |calls, _| { calls.push("protect"); None },
                || 0.5,
            ), Decision::Waiting);
            assert!(calls.is_empty());
        }
        assert!(matches!(plan(
            &mut deadline, input(0.5), &mut calls,
            |calls| { calls.push("select"); Some(frag()) },
            |calls, _| { assert_eq!(calls.as_slice(), ["select"]); calls.push("protect"); None },
            || 0.0,
        ), Decision::Throw(_)));
        assert_eq!(calls, ["select", "protect"]);
        for grenade in [None, Some(frag())] {
            calls.clear();
            deadline = Some((FormId(9), 0.0));
            let decision = plan(
                &mut deadline, ThrowInput { target_at: [100.0, 0.0, 0.0], ..input(1.0) }, &mut calls,
                |calls| { calls.push("select"); grenade },
                |calls, _| { calls.push("protect"); None },
                || 0.0,
            );
            assert_eq!(decision, grenade.map_or(Decision::Empty, Decision::OutOfBand));
            assert_eq!(calls, ["select"]);
        }
    }

    #[test]
    fn held_grenade_commit_rejects_zero_and_failed_consumption() {
        let mut deadline = Some((FormId(9), 0.0));
        let Decision::Throw(throw) = plan(&mut deadline, input(1.0), &mut (), |_| Some(frag()), |_, _| None, || 0.0) else { panic!("valid throw") };
        assert_eq!(commit(throw, |_| Ok::<_, ()>(0)), None);
        assert_eq!(commit(throw, |_| Err::<i32, _>("inventory unavailable")), None);
        let accepted = commit(throw, |weapon| {
            assert_eq!(weapon, frag().weapon);
            Ok::<_, ()>(1)
        });
        assert_eq!(accepted, Some(throw));
    }

    #[test]
    fn held_grenade_selection_keeps_lowest_id_and_rejects_bad_or_empty_items() {
        let mut forms = forms();
        let low = FormId(1);
        let high = FormId(2);
        // Holding high enables grenade execution, but selection deliberately remains
        // lowest FormID among carried grenades (the existing §20 policy).
        assert_eq!(carried(&forms, [(high, 2), (low, 1)].into_iter()).unwrap().weapon, low);
        assert_eq!(carried(&forms, [(low, 0), (high, 2)].into_iter()).unwrap().weapon, high);
        forms.projectiles.remove(&FormId(11));
        assert_eq!(carried(&forms, [(low, 2), (high, 2)].into_iter()).unwrap().weapon, high);
        for anim in ["Land Mine (1 Hand)", "Mine Drop (1 Hand)", "Thrown (1 Hand)", "Launcher (2 Hand)"] {
            forms.weapons.get_mut(&high).unwrap().anim_type = Some(anim.into());
            assert_eq!(carried(&forms, [(high, 2)].into_iter()), None);
        }
    }

    #[test]
    fn held_grenade_two_items_make_two_lobbed_projectiles_then_stay_empty() {
        let forms = forms();
        let mut inventory = Inventory::new();
        inventory.add(FormId(1), 2);
        let mut deadline = None;
        let mut fired = Vec::new();
        // Run the production controller and consumption boundary on ordinary
        // inventory, then the receiver's projectile constructor with zero spread.
        for clock in 0..80 {
            let decision = plan(&mut deadline, input(clock as f32), &mut inventory, |inventory| carried(&forms, inventory.iter()), |_, _| None, || 0.0);
            if let Decision::Throw(throw) = decision {
                let before = inventory.count(throw.grenade.weapon);
                let throw = commit(throw, |weapon| Ok::<_, ()>(inventory.remove(weapon, 1))).unwrap();
                assert_eq!(inventory.count(throw.grenade.weapon), before - 1);
                let chain = forms.chain(throw.grenade.weapon, None).unwrap();
                let projectiles = fnv_combat::shot::fire(&forms, &chain, throw.origin, throw.aim, 0.0, None, input(0.0).gravity, &Default::default(), &mut || 0.5);
                assert_eq!(projectiles.len(), 1);
                let p = &projectiles[0];
                assert_eq!(p.gravity, throw.grenade.gravity_mult * input(0.0).gravity);
                assert!(p.vel[2] > 0.0, "throw has an upward ballistic arc, not direct aim at the lower feet");
                assert!((fnv_combat::vec::len(p.vel) - throw.grenade.speed).abs() < 0.01);
                let expected = fnv_combat::vats::lob_dir(throw.origin, input(0.0).target_at, throw.grenade.speed, throw.grenade.gravity_mult * input(0.0).gravity).unwrap();
                for (actual, want) in p.vel.into_iter().zip(expected) {
                    assert!((actual - want * throw.grenade.speed).abs() < 0.01);
                }
                fired.push((clock, throw.grenade.weapon));
            }
        }
        // The first at once, the second 1.6 s on (the next whole second).
        assert_eq!(fired, [(0, FormId(1)), (2, FormId(1))]);
        assert_eq!(inventory.count(FormId(1)), 0);
    }

    #[test]
    fn held_grenade_mixed_inventory_uses_one_selected_identity_for_throw_and_consumption() {
        let forms = forms();
        let mut inventory = Inventory::new();
        inventory.add(FormId(1), 1);
        inventory.add(FormId(2), 2);
        let mut deadline = Some((FormId(9), 0.0));
        let Decision::Throw(throw) = plan(&mut deadline, input(1.0), &mut inventory, |inventory| carried(&forms, inventory.iter()), |_, _| None, || 0.0) else { panic!("valid throw") };
        let throw = commit(throw, |weapon| Ok::<_, ()>(inventory.remove(weapon, 1))).unwrap();
        assert_eq!(throw.grenade.weapon, FormId(1));
        assert_eq!(throw.grenade.speed, 1000.0);
        assert_eq!(throw.grenade.blast, 256.0);
        assert_eq!(inventory.count(FormId(1)), 0);
        assert_eq!(inventory.count(FormId(2)), 2);
        let chain = forms.chain(throw.grenade.weapon, None).unwrap();
        assert_eq!(chain.projectile, Some(FormId(11)));
        assert_eq!(chain.explosion, Some(FormId(21)));
        assert_eq!(carried(&forms, inventory.iter()).unwrap().weapon, FormId(2));
    }

    fn frag() -> Grenade {
        Grenade { weapon: FormId(0x4330), speed: 1000.0, gravity_mult: 1.0, blast: 256.0 }
    }

    #[test]
    fn a_grenade_is_thrown_beyond_the_floor_and_within_its_reach() {
        let g = frag();
        let gravity = 686.0;
        // 1000² / 686 ≈ 1458: within MAX_THROW.
        assert!((reach(&g, gravity) - 1457.7).abs() < 1.0);
        assert!(!in_band(&g, 300.0, gravity), "the observed 300 units: no throw");
        assert!(in_band(&g, 800.0, gravity), "the observed 800 units: thrown");
        assert!(in_band(&g, 1400.0, gravity));
        assert!(!in_band(&g, 1500.0, gravity), "beyond its arc");
        // A fast one is held to MAX_THROW.
        let far = Grenade { speed: 3000.0, ..g };
        assert_eq!(reach(&far, gravity), MAX_THROW);
    }

    #[test]
    fn waits_fall_within_their_range() {
        assert_eq!(wait(BETWEEN, 0.0), BETWEEN.0);
        assert_eq!(wait(BETWEEN, 1.0), BETWEEN.1);
        assert!((wait(FIRST_AFTER, 0.5) - 0.5).abs() < 1e-5);
    }
}
