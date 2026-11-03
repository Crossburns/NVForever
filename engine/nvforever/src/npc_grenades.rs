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
//!   and re-arming passes grenades over, so throws come only once drawing a grenade is
//!   built.
//! - **When**: holding it, not fleeing, ready on a target it sees, first [`FIRST_AFTER`]
//!   seconds into seeing it, then every
//!   [`BETWEEN`] seconds; only at a target beyond [`MIN_BLAST_SHARE`] times the blast's
//!   radius (it keeps clear of its own blast) and its throw's reach (the low arc's, under
//!   the projectile's gravity, no farther than [`MAX_THROW`]); held, as an explosive shot
//!   is, when a comrade or a bystander would be caught (combat-ai.md §17.6).
//! - **How**: from its eyes on the low arc that lands on the target's feet (as VATS
//!   mode's throws, `fnv_combat::vats::lob_dir`), the grenade's throw clip played, its
//!   FireExplosive line tried (`fnv_game::combat_lines`), the shot's own explosion and
//!   the others' running from it as for the player's grenades (§17.7). The gun stays in
//!   its hand meanwhile [design: no weapon swap for one throw].

use fnv_combat::data::CombatForms;
use fnv_sim::FormId;

/// The animation type of grenades [documented: GECK wiki Weapons, Animation Type].
pub const GRENADE_ANIM: &str = "Grenade Throw (1 Hand)";
/// The first throw of a fight comes this many seconds (lowest, highest) into seeing the
/// target [design].
pub const FIRST_AFTER: (f32, f32) = (4.0, 10.0);
/// Then one every this many seconds (lowest, highest) [design].
pub const BETWEEN: (f32, f32) = (12.0, 25.0);
/// No throw at a target nearer than this many times the blast's radius [design: a frag
/// grenade's blast is 900 units, so 990; the thrower stays outside its own blast].
pub const MIN_BLAST_SHARE: f32 = 1.1;
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

/// Whether a target `distance` away is one to throw `g` at: clear of the blast, within
/// the throw's reach.
pub fn in_band(g: &Grenade, distance: f32, gravity: f32) -> bool {
    distance > g.blast * MIN_BLAST_SHARE && distance <= reach(g, gravity)
}

/// The seconds until the next throw, from `lo`..`hi` by `u` in [0, 1).
pub fn wait(range: (f32, f32), u: f32) -> f32 {
    range.0 + (range.1 - range.0) * u.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frag() -> Grenade {
        Grenade { weapon: FormId(0x4330), speed: 1000.0, gravity_mult: 1.0, blast: 256.0 }
    }

    #[test]
    fn a_grenade_is_thrown_clear_of_its_blast_and_within_its_reach() {
        let g = frag();
        let gravity = 686.0;
        // 1000² / 686 ≈ 1458: within MAX_THROW.
        assert!((reach(&g, gravity) - 1457.7).abs() < 1.0);
        assert!(!in_band(&g, 270.0, gravity), "within 1.1 x its blast");
        assert!(in_band(&g, 300.0, gravity));
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
        assert!((wait(FIRST_AFTER, 0.5) - 7.0).abs() < 1e-5);
    }
}
