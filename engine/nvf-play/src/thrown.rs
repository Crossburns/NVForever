//! Thrown weapons land and can be picked up again (docs/spec-notes/combat-rules.md §11b).
//!
//! - **Which** [documented: GECK wiki Projectile, "Can be Picked Up: If checked,
//!   projectile can be picked up by the player character after it has been disabled, or
//!   regardless of it if Can be Disabled isn't enabled"]: a projectile whose PROJ has the
//!   flag and neither "Can Be Disabled" nor an explosion
//!   (`fnv_combat::data::ProjectileStats::lands_as_weapon`): the throwing knife, hatchet,
//!   spear and tomahawk of the masters. Mines and C-4 have both flags and stay
//!   `crate::mines`'s.
//! - **As what**: the weapon thrown (the shot's weapon, when that weapon fires this PROJ),
//!   else the PROJ's Default Weapon Source ("if they can be picked up by an NPC or the
//!   player they will be added to inventory as that weapon" [documented: GECK wiki
//!   Projectile]).
//! - **Where** [hypothesis: the game's own placement is not documented]: one that struck
//!   an actor goes into that actor's inventory, to be taken from the body; one that struck
//!   anything else lies where it struck, [`OFF_SURFACE`] out from the surface, turned the
//!   way it flew; one that flew its whole range lies where it ended. A created reference
//!   of the weapon (`fnv_sim::World::place`) in the thrower's cell, drawn and given
//!   collision by `crate::dropped` (it falls and settles under its model's collision) and
//!   taken by activation like any loose item. It belongs to nobody [design].
//! - The GECK wiki's Projectile page notes the flag "doesn't appear to work with Missile
//!   type projectiles"; the masters' thrown weapons are Missiles that set it, so it is
//!   followed here (docs/owner-review.md asks the owner what the game does).
//!
//! A `thrown summary` line at the end; one `thrown:` line per landing.

use crate::combat::Combat;
use crate::sim::SimState;
use crate::RefChanged;
use bevy::prelude::*;
use fnv_combat::vec::V3;
use fnv_sim::FormId;

/// How far from the surface it struck a landed weapon is put, units [design: clear of it,
/// for its collision to settle it].
pub const OFF_SURFACE: f32 = 4.0;

/// Landings so far.
#[derive(Resource, Default, Debug)]
pub struct ThrownStats {
    /// Put down in the world.
    pub landed: u32,
    /// Kept by the actor they struck.
    pub kept: u32,
    pub failed: u32,
}

pub struct ThrownPlugin;

impl Plugin for ThrownPlugin {
    fn build(&self, app: &mut App) {
        // `land` runs in `crate::combat`'s chain, after the projectiles fly.
        app.init_resource::<ThrownStats>().add_systems(Last, report);
    }
}

/// The z rotation of a reference facing `dir` (0 faces +Y, clockwise seen from above).
fn heading(dir: V3) -> f32 {
    if dir[0].abs() + dir[1].abs() < 1e-4 {
        0.0
    } else {
        dir[0].atan2(dir[1])
    }
}

/// Where a landed weapon is put: out from the surface it struck, or where it ended.
pub fn landing_point(point: V3, normal: Option<V3>) -> V3 {
    match normal {
        Some(n) => [point[0] + n[0] * OFF_SURFACE, point[1] + n[1] * OFF_SURFACE, point[2] + n[2] * OFF_SURFACE],
        None => point,
    }
}

/// Put down (or give to the actor struck) the thrown weapons that landed (module notes).
pub(crate) fn land(mut c: ResMut<Combat>, mut sim: ResMut<SimState>, mut stats: ResMut<ThrownStats>, mut changed: MessageWriter<RefChanged>) {
    for l in c.take_landed() {
        let name = sim.forms.get(l.weapon).and_then(|b| b.edid.clone()).unwrap_or_else(|| l.weapon.to_string());
        if let Some(actor) = l.struck {
            match sim.world.add_item(actor, l.weapon, 1) {
                Ok(_) => {
                    stats.kept += 1;
                    info!("thrown: {} thrown by {} struck {actor}: it keeps it", name, l.by);
                    continue;
                }
                Err(e) => warn!("thrown: {name} could not go to {actor} ({e}); it is put down instead"),
            }
        }
        let cell = sim.world.peek(l.by).and_then(|a| a.cell).or_else(|| sim.world.peek(FormId::PLAYER_REF).and_then(|a| a.cell));
        let at = landing_point(l.point, l.normal);
        match sim.world.place(l.weapon, cell, at, [0.0, 0.0, heading(l.dir)], 1) {
            Ok(id) => {
                stats.landed += 1;
                changed.write(RefChanged::new(id.0));
                info!("thrown: {name} ({}) thrown by {} landed at ({:.0}, {:.0}, {:.0}) as reference {id}, to be picked up", l.proj, l.by, at[0], at[1], at[2]);
            }
            Err(e) => {
                stats.failed += 1;
                warn!("thrown: {name} could not be put down: {e}");
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<ThrownStats>) {
    if exit.read().next().is_some() {
        info!("thrown summary: {} thrown weapons landed to be picked up, {} kept by the actor they struck, {} could not be put down", stats.landed, stats.kept, stats.failed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lands_off_the_surface_it_struck() {
        assert_eq!(landing_point([10.0, 0.0, 0.0], Some([0.0, 0.0, 1.0])), [10.0, 0.0, OFF_SURFACE]);
        assert_eq!(landing_point([10.0, 0.0, 5.0], None), [10.0, 0.0, 5.0]);
        assert!((heading([1.0, 0.0, 0.0]) - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }
}
