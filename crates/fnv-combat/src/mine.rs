//! Mines (docs/spec-notes/combat-rules.md §11a): placed proximity explosives, those the
//! plugins put in the world (PGRE) and those the player puts down.
//!
//! - **What is a mine.** A lobber PROJ with Explosion, Alt. Trigger and a proximity
//!   radius (`MineFragProjectile`: proximity 100, timer 3) [observed: the PROJ records].
//! - **Trigger.** An actor that comes within the proximity radius starts the fuse; out
//!   of doors the radius is multiplied by `fMineExteriorRadiusMult` (1.6 in
//!   FalloutNV.esm) [hypothesis: from the setting's name; the Fallout 3 tutorial's 100
//!   indoors and 160 outdoors, level-design.md, are this product]. The owner's numbers
//!   agree: about 2 m before it beeps, 1.4 m quoted for the frag mine [owner
//!   2026-10-19].
//! - **Fuse.** Once started it burns down whoever stays, ticking and blinking, and the
//!   mine goes off at its end. Its length is the PROJ's timer scaled by the Explosives
//!   skill of whoever set it off, never below `fMinesDelayMin` (0.2 s) [owner
//!   2026-10-19: "the player's explosive skill determines how long you have", 3 s
//!   nominal]. The curve is timer × ([`FUSE_BASE`] + (1 − [`FUSE_BASE`]) × skill / 100):
//!   about a third of the timer at Explosives 0 (1.05 s for the frag mine), the whole
//!   timer at 100 [design, #1659: the earlier linear timer × skill / 100, a hypothesis,
//!   gave a starting character under half a second, too short to step back or crouch
//!   and disarm; the capture `cl_d_mines_live`, a mine that went off between 0.5 and
//!   1.5 s after the player of that save stepped on it (simulation.md §1), still fits at
//!   Explosives up to 23, and that save's skill was not recorded].
//! - **Who sets it off.** Anyone but its owner's side: not a member of the owning
//!   faction nor of a faction allied or friendly to it; a mine the player put down is
//!   never set off by the player [owner 2026-10-19]. The engine decides that
//!   (`nvf_play::mines`); this module only keeps the fuse.
//! - **Tick and light.** While the fuse burns the model's `##LightOn` blinks, lit for
//!   [`TICK_SECS`] and dark for as long, and the PROJ's countdown sound plays each time
//!   it lights [design: the period; the owner: the light blinks once the mine is set
//!   off].

use crate::data::{CombatForms, ProjectileStats};

/// How often a burning fuse ticks and its light changes, seconds [design].
pub const TICK_SECS: f32 = 0.25;

/// The share of the PROJ's timer a mine's fuse has at Explosives 0; the rest comes with
/// the skill, up to the whole timer at 100 [design, #1659: 1.05 s for the frag mine,
/// a human reaction and a step back].
pub const FUSE_BASE: f32 = 0.35;

/// The game settings the mines read, with FalloutNV.esm's values as defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MineRules {
    /// `fMinesDelayMin`: the shortest fuse, seconds (0.2).
    pub delay_min: f32,
    /// `fMineExteriorRadiusMult`: the trigger radius out of doors (1.6).
    pub exterior_radius_mult: f32,
}

impl Default for MineRules {
    fn default() -> Self {
        MineRules { delay_min: 0.2, exterior_radius_mult: 1.6 }
    }
}

impl MineRules {
    pub fn from_forms(forms: &CombatForms) -> MineRules {
        let d = MineRules::default();
        MineRules {
            delay_min: forms.setting("fMinesDelayMin").unwrap_or(d.delay_min),
            exterior_radius_mult: forms.setting("fMineExteriorRadiusMult").unwrap_or(d.exterior_radius_mult),
        }
    }

    /// The distance from the mine within which an actor starts its fuse.
    pub fn trigger_radius(&self, p: &ProjectileStats, exterior: bool) -> f32 {
        p.alt_trigger_proximity * if exterior { self.exterior_radius_mult } else { 1.0 }
    }

    /// The fuse a mine gets when an actor with this Explosives skill sets it off:
    /// timer × (`FUSE_BASE` + (1 − `FUSE_BASE`) × skill / 100), at least `delay_min`.
    pub fn fuse(&self, p: &ProjectileStats, explosives: f32) -> f32 {
        let share = FUSE_BASE + (1.0 - FUSE_BASE) * explosives.clamp(0.0, 100.0) / 100.0;
        (p.alt_trigger_timer * share).max(self.delay_min)
    }
}

/// A PROJ that is a mine: a lobber that explodes on a proximity trigger.
pub fn is_mine(p: &ProjectileStats) -> bool {
    p.kind.as_deref() == Some("Lobber") && p.has("Explosion") && p.has("Alt. Trigger") && p.alt_trigger_proximity > 0.0
}

/// What a burning fuse did in a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuseStep {
    Burning,
    /// A tick: play the countdown sound.
    Tick,
    /// Burnt down: the mine goes off.
    Done,
}

/// A mine's fuse, from the moment it is set off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fuse {
    pub left: f32,
    since_tick: f32,
    /// The light is lit now (it blinks, lit on each tick).
    pub lit: bool,
}

impl Fuse {
    /// A fuse of `secs`; it ticks at once.
    pub fn new(secs: f32) -> Fuse {
        Fuse { left: secs, since_tick: TICK_SECS, lit: false }
    }

    pub fn step(&mut self, dt: f32) -> FuseStep {
        self.left -= dt;
        if self.left <= 0.0 {
            return FuseStep::Done;
        }
        self.since_tick += dt;
        if self.since_tick >= TICK_SECS {
            self.since_tick %= TICK_SECS;
            self.lit = !self.lit;
            if self.lit {
                return FuseStep::Tick;
            }
        }
        FuseStep::Burning
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frag() -> ProjectileStats {
        ProjectileStats {
            kind: Some("Lobber".into()),
            flags: vec!["Explosion".into(), "Alt. Trigger".into(), "Can Be Disabled".into(), "Can Be Picked Up".into()],
            alt_trigger_timer: 3.0,
            alt_trigger_proximity: 100.0,
            ..Default::default()
        }
    }

    #[test]
    fn frag_mine_is_a_mine_and_a_grenade_is_not() {
        assert!(is_mine(&frag()));
        let grenade = ProjectileStats { alt_trigger_proximity: 0.0, ..frag() };
        assert!(!is_mine(&grenade));
    }

    #[test]
    fn radius_grows_out_of_doors() {
        let r = MineRules::default();
        assert_eq!(r.trigger_radius(&frag(), false), 100.0);
        assert!((r.trigger_radius(&frag(), true) - 160.0).abs() < 1e-3);
    }

    #[test]
    fn fuse_scales_with_explosives_from_a_third_of_the_timer() {
        let r = MineRules::default();
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(near(r.fuse(&frag(), 100.0), 3.0));
        assert!(near(r.fuse(&frag(), 50.0), 2.025));
        assert!(near(r.fuse(&frag(), 20.0), 1.44));
        // A starting character has about a second to react (#1659).
        assert!(near(r.fuse(&frag(), 0.0), 1.05));
        assert!(near(r.fuse(&frag(), -10.0), 1.05));
        assert!(near(r.fuse(&frag(), 250.0), 3.0));
        // Rising with the skill all the way.
        let fuses: Vec<f32> = (0..=100).step_by(10).map(|s| r.fuse(&frag(), s as f32)).collect();
        assert!(fuses.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn fuse_never_below_delay_min() {
        let r = MineRules::default();
        // A short timer: the share of it falls under fMinesDelayMin at low skill.
        let short = ProjectileStats { alt_trigger_timer: 0.3, ..frag() };
        assert_eq!(r.fuse(&short, 0.0), 0.2);
        assert!((r.fuse(&short, 100.0) - 0.3).abs() < 1e-5);
        let long_floor = MineRules { delay_min: 2.0, ..r };
        assert_eq!(long_floor.fuse(&frag(), 0.0), 2.0);
    }

    #[test]
    fn fuse_blinks_all_the_way_down() {
        // The light keeps blinking until the fuse is out, at the shortest and longest fuse.
        let r = MineRules::default();
        for skill in [0.0, 100.0] {
            let secs = r.fuse(&frag(), skill);
            let mut f = Fuse::new(secs);
            let dt = 1.0 / 64.0;
            let mut last_tick = 0.0;
            let mut t = 0.0;
            loop {
                t += dt;
                match f.step(dt) {
                    FuseStep::Tick => last_tick = t,
                    FuseStep::Done => break,
                    FuseStep::Burning => {}
                }
            }
            assert!(t + 1e-3 >= secs);
            assert!(t - last_tick <= 2.0 * TICK_SECS + dt, "skill {skill}: last tick {last_tick} of {t}");
        }
    }

    #[test]
    fn fuse_ticks_then_goes_off() {
        let mut f = Fuse::new(1.0);
        let mut ticks = 0;
        let mut steps = 0;
        loop {
            steps += 1;
            match f.step(0.0625) {
                FuseStep::Tick => ticks += 1,
                FuseStep::Done => break,
                FuseStep::Burning => {}
            }
        }
        // Lit at once and after 0.5 s: one tick per two periods.
        assert_eq!(ticks, 2);
        assert_eq!(steps, 16);
    }
}
