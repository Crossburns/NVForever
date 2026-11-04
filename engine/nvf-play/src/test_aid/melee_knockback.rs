//! `NVF_PLAY_TEST_MELEE_KNOCKBACK=<target FormID hex>:<x>:<y>:<z>`: one player power blow on
//! one standing actor, gated on readiness rather than on fixed seconds (#1386; smoke
//! `melee-knockback`). This half holds the keys: it draws the weapon, waits until the
//! other half (nvforever `residents::melee_knockback`) has the target in place and nothing
//! here stops a swing, aims at that reference, holds Use until the real power swing starts
//! and lets go, once, with no retry. The blow itself takes the normal input and physics
//! paths. Its clock is the world aids' (`crate::pause::AidTime::world_secs`).
//!
//! Every transition is logged once (`melee knockback aid: …`). An invalid value, a timeout
//! or a setup that goes wrong logs `melee knockback aid: FAIL …`, which the scenario forbids.

use super::aim_at_ref;
use crate::combat::{Combat, Knockback, WeaponAnim};
use crate::control::{read_controls, ControlLocks, PlayerControl};
use crate::pause::AidTime;
use crate::phys::PhysState;
use crate::player::Player;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_sim::FormId;

pub const ENV: &str = "NVF_PLAY_TEST_MELEE_KNOCKBACK";

/// From the world aids' first moment to the press, at most: past this the setup failed
/// [design: above what a scripts start, a landing and the settling take].
const SETUP_SECONDS: f32 = 20.0;
/// From the press to the power swing, and from there to the blow's knockback, at most
/// [design: a power hold and a swing are each well under this].
const SWING_SECONDS: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    Waiting,
    Holding { since: f32 },
    Released { since: f32 },
    Landed,
    Failed,
}

/// The state both halves share.
#[derive(Resource, Debug)]
pub struct MeleeKnockbackAid {
    pub target: FormId,
    /// Where the target goes, from the player (MoveTo's offsets).
    pub offset: [f32; 3],
    /// The world half's verdict, each frame until the press: in place, or what it waits for.
    pub arranged: Result<(), &'static str>,
    stage: Stage,
    began: Option<f32>,
    told: Option<&'static str>,
}

impl MeleeKnockbackAid {
    pub fn parse(v: &str) -> Result<MeleeKnockbackAid, String> {
        let fields: Vec<&str> = v.split(':').map(str::trim).collect();
        let [target, x, y, z] = fields.as_slice() else { return Err(format!("{v:?}: expected <target FormID hex>:<x>:<y>:<z>")) };
        let target = u32::from_str_radix(target.trim_start_matches("0x").trim_start_matches("0X"), 16).map_err(|_| format!("{target:?} is not a FormID in hex"))?;
        if target == FormId::PLAYER_REF.0 {
            return Err("the target cannot be the player".into());
        }
        let num = |s: &str| s.parse::<f32>().map_err(|_| format!("{s:?} is not a number"));
        Ok(MeleeKnockbackAid { target: FormId(target), offset: [num(x)?, num(y)?, num(z)?], arranged: Err("the scripts"), stage: Stage::Waiting, began: None, told: None })
    }

    /// Before the press: the world half still arranges the target.
    pub fn waiting(&self) -> bool {
        self.stage == Stage::Waiting
    }

    /// Log the failure, once; nothing more is pressed.
    pub fn fail(&mut self, why: &str) {
        if self.stage != Stage::Failed {
            warn!("melee knockback aid: FAIL target={:08X} reason={why}", self.target.0);
            self.stage = Stage::Failed;
        }
    }

    /// What the keys wait for, logged when it changes.
    fn wait_for(&mut self, why: &'static str) {
        if self.told != Some(why) {
            info!("melee knockback aid: waiting for {why}");
            self.told = Some(why);
        }
    }
}

/// What stops the press, as far as the keys can tell.
fn keys_gate(playing: bool, fighting_locked: bool, lowered: bool, weapon_ready: bool, melee: bool, swinging: bool) -> Result<(), &'static str> {
    if !playing {
        Err("the menus to close")
    } else if fighting_locked {
        Err("the attack controls to unlock")
    } else if lowered {
        Err("the weapon to be raised")
    } else if !weapon_ready {
        Err("the weapon to be ready")
    } else if !melee {
        Err("a melee weapon or the fists")
    } else if swinging {
        Err("the swing playing to end")
    } else {
        Ok(())
    }
}

pub(super) fn setup(app: &mut App) {
    let Ok(v) = std::env::var(ENV) else { return };
    match MeleeKnockbackAid::parse(&v) {
        Ok(aid) => {
            app.insert_resource(aid).add_systems(Update, press.in_set(PlaySet::Input).after(read_controls));
        }
        Err(e) => warn!("melee knockback aid: FAIL invalid {ENV}: {e}"),
    }
}

/// The keys, after the real ones were read.
#[allow(clippy::too_many_arguments)]
fn press(
    time: AidTime,
    mut aid: ResMut<MeleeKnockbackAid>,
    state: Res<State<PlayState>>,
    locks: Res<ControlLocks>,
    anim: Res<WeaponAnim>,
    combat: Option<Res<Combat>>,
    phys: Res<PhysState>,
    mut player: ResMut<Player>,
    mut control: ResMut<PlayerControl>,
    mut knocks: MessageReader<Knockback>,
) {
    let target = aid.target.0;
    let blows: Vec<Knockback> = knocks.read().filter(|k| k.form_id == target).copied().collect();
    let Some(now) = time.world_secs() else { return };
    let began = *aid.began.get_or_insert(now);
    let (melee, swing) = combat.as_deref().map_or((false, None), Combat::melee_swing);
    match aid.stage {
        Stage::Waiting => {
            if !blows.is_empty() {
                aid.fail("a knockback on the target before the blow");
                return;
            }
            let playing = *state.get() == PlayState::Playing;
            if playing && !locks.fighting && !control.weapon_out {
                control.weapon_out = true;
                info!("melee knockback aid: the weapon drawn");
            }
            let gate = keys_gate(playing, locks.fighting, control.lowered, anim.ready, melee, swing.is_some());
            let Err(why) = gate.and(aid.arranged) else {
                info!("melee knockback aid: ready target={target:08X}");
                aim_at_ref(&phys, &mut player, control.crosshair.as_ref(), target);
                info!("melee knockback aid: Use held");
                control.trigger = control.weapon_out;
                aid.stage = Stage::Holding { since: now };
                return;
            };
            if now - began > SETUP_SECONDS {
                aid.fail(&format!("timed out after {SETUP_SECONDS} s waiting for {why}"));
                return;
            }
            aid.wait_for(why);
        }
        Stage::Holding { since } => {
            if !blows.is_empty() {
                aid.fail("a knockback on the target before the power swing");
            } else if swing == Some(false) {
                aid.fail("a normal swing started instead of a power attack");
            } else if swing == Some(true) {
                // Not held any more: `read_controls` set the trigger to the real key's state.
                info!("melee knockback aid: the power swing started, Use let go");
                aid.stage = Stage::Released { since: now };
            } else if now - since > SWING_SECONDS {
                aid.fail("no power swing started while Use was held");
            } else {
                control.trigger = control.weapon_out;
            }
        }
        Stage::Released { since } => match blows.as_slice() {
            [] if now - since > SWING_SECONDS => aid.fail("the swing landed no knockback"),
            [] => {}
            [k] if k.attacker == FormId::PLAYER_REF.0 && k.power => {
                info!("melee knockback aid: blow={} landed target={target:08X} asked={:.3} units", k.blow, k.distance());
                aid.stage = Stage::Landed;
            }
            _ => aid.fail("the first knockback on the target is not one player power blow"),
        },
        Stage::Landed => {
            if !blows.is_empty() {
                aid.fail("a second knockback on the target");
            }
        }
        Stage::Failed => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_value_names_a_target_and_its_place() {
        let a = MeleeKnockbackAid::parse("000F36F5:0:90:0").unwrap();
        assert_eq!((a.target, a.offset), (FormId(0x000F_36F5), [0.0, 90.0, 0.0]));
        assert!(a.waiting());
        assert_eq!(MeleeKnockbackAid::parse("0x10:1:-2:3.5").unwrap().offset, [1.0, -2.0, 3.5]);
        for bad in ["", "000F36F5", "000F36F5:0:90", "nothex:0:90:0", "000F36F5:0:north:0", "14:0:90:0"] {
            assert!(MeleeKnockbackAid::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_keys_wait_for_everything_that_stops_a_swing() {
        assert_eq!(keys_gate(true, false, false, true, true, false), Ok(()));
        assert!(keys_gate(false, false, false, true, true, false).is_err());
        assert!(keys_gate(true, true, false, true, true, false).is_err());
        assert!(keys_gate(true, false, true, true, true, false).is_err());
        assert!(keys_gate(true, false, false, false, true, false).is_err());
        assert!(keys_gate(true, false, false, true, false, false).is_err());
        assert!(keys_gate(true, false, false, true, true, true).is_err());
    }
}
