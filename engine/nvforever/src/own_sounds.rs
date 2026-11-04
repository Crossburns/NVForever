//! Sounds of things that are drawn here: add-on nodes, projectiles in flight and the
//! weapons in actors' hands (docs/spec-notes/audio.md §15.2).
//!
//! - **Add-on nodes** [hypothesis: the fire add-ons' `FXFireLP` is the fire's crackle]:
//!   an add-on node in the world whose ADDN names a sound (`SNAM`) is a [`SoundSource`]
//!   where it is while it shows; the ambient plays it as it plays a sound marker
//!   (sounds with neither Loop nor Play At Random, the bullet-impact add-ons', are not:
//!   the impact plays its IPCT sound).
//! - **Projectiles** (PROJ DATA Sound), by the PROJ's Supersonic flag: "If checked, the
//!   sound emanates from the point of the projectile's path closest to the listener
//!   (Think bullets whizzing by your head). If unchecked, the sound emitter follows the
//!   projectile around until it is destroyed (Think rockets flying through the air)"
//!   [documented: GECK wiki Projectile]. A supersonic one plays its sound once, at the
//!   point of its path nearest the listener ([`nearest_on`]), when it has passed it: the
//!   frame it starts to draw away after coming closer, when that point was within the
//!   sound's maximum distance [design: when]. A shot that only draws away (the player's
//!   own) never whizzes, nor one that ends on the listener (its impact sounds instead).
//!   Any other sounds its sound where it flies, going round until it is gone
//!   [hypothesis: their sounds are loops by name, `WPNPlasmaRifleProjectileLP`,
//!   `WPNPulseGunProjectileLP`, though only `FXMissileFlyBy` has the Loop flag].
//!   48 PROJ of the masters are supersonic (the bullets, `FXBulletWhiz`), 30 of the
//!   others name a sound [observed].
//! - **Idle** (WEAP `UNAM`): an actor with its weapon out sounds the weapon's idle sound
//!   at the actor, going round whatever its flags [hypothesis: 12 of the 16 have neither
//!   flag, their files are loops by name]. The player's is `nvf_play::combat`'s.
//!
//! An `own sounds summary` line at the end.

use crate::particles::AddonEmitter;
use crate::weapons::HeldWeapon;
use crate::world::WorldRoot;
use bevy::prelude::*;
use nvf_play::combat::ShotVisuals;
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::sfx::{PlaySound, Sfx, SoundSource};
use nvf_play::sim::SimState;
use fnv_sim::FormId;
use std::collections::{HashMap, HashSet};

/// The [`SoundSource::key`] kinds used here.
const ADDON_KIND: u8 = 10;
const PROJECTILE_KIND: u8 = 11;
const IDLE_KIND: u8 = 12;

#[derive(Resource, Default)]
struct OwnSounds {
    /// Supersonic projectiles seen, by their number (`None` once it whizzed).
    shots: HashMap<u64, Option<Passing>>,
    addons: HashSet<Entity>,
    projectile_loops: HashSet<u64>,
    whizzes: u32,
    idles: HashSet<u32>,
}

/// A supersonic projectile on its way past the listener: its distance last frame, whether
/// it has come closer, and where it was the last two frames.
#[derive(Clone, Copy, Debug)]
struct Passing {
    last: f32,
    closer: bool,
    at: [f32; 3],
    before: [f32; 3],
}

/// The point of the stretch from `a` to `b` nearest `to`.
fn nearest_on(a: [f32; 3], b: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    if len2 < 1e-6 {
        return a;
    }
    let t = (((to[0] - a[0]) * ab[0] + (to[1] - a[1]) * ab[1] + (to[2] - a[2]) * ab[2]) / len2).clamp(0.0, 1.0);
    [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t]
}

fn distance2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Add-on nodes' sounds (see the module notes).
fn addons(
    mut st: ResMut<OwnSounds>,
    sfx: Option<Res<Sfx>>,
    root: Query<Entity, With<WorldRoot>>,
    emitters: Query<(Entity, &AddonEmitter, Option<&InheritedVisibility>)>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
    mut out: MessageWriter<SoundSource>,
) {
    let (Some(sfx), Ok(root)) = (sfx, root.single()) else { return };
    for (e, em, v) in &emitters {
        if v.is_some_and(|v| !v.get()) {
            continue;
        }
        let Some(sound) = sfx.forms.addons.get(&em.index).and_then(|a| a.sound) else { continue };
        let at = crate::particles::game_affine(e, &transforms, &parents, root).translation;
        st.addons.insert(e);
        out.write(SoundSource { key: SoundSource::key(ADDON_KIND, e.to_bits() as u32), sound, at: at.to_array(), looping: false });
    }
}

/// Projectiles' sounds in flight (see the module notes).
fn projectiles(mut st: ResMut<OwnSounds>, sfx: Option<Res<Sfx>>, visuals: Res<ShotVisuals>, player: Option<Res<Player>>, mut out: MessageWriter<SoundSource>, mut sounds: MessageWriter<PlaySound>) {
    let (Some(sfx), Some(player)) = (sfx, player) else { return };
    let st = &mut *st;
    let eye = player.eye();
    let mut now: HashSet<u64> = HashSet::new();
    for v in &visuals.flying {
        now.insert(v.id);
        let Some(sound) = sfx.forms.projectiles.get(&v.projectile).and_then(|p| p.sound) else { continue };
        let Some(s) = sfx.forms.sounds.get(&sound) else { continue };
        let supersonic = sfx.forms.projectiles.get(&v.projectile).is_some_and(|p| p.supersonic);
        if !supersonic {
            st.projectile_loops.insert(v.id);
            out.write(SoundSource { key: SoundSource::key(PROJECTILE_KIND, v.id as u32), sound, at: v.pos, looping: true });
            continue;
        }
        let d = distance2(v.pos, eye).sqrt();
        let Some(seen) = st.shots.entry(v.id).or_insert(Some(Passing { last: d, closer: false, at: v.pos, before: v.pos })) else { continue };
        let p = *seen;
        if d < p.last {
            *seen = Passing { last: d, closer: true, before: p.at, at: v.pos };
        } else if p.closer && d > p.last {
            // The nearest point lies between where it was two frames ago and now.
            let at = nearest_on(p.before, v.pos, eye);
            let nearest = distance2(at, eye).sqrt();
            st.shots.insert(v.id, None);
            if nearest < s.max_distance {
                st.whizzes += 1;
                debug!("own sounds: projectile {} whizzes by, {nearest:.0} units from the listener: {}", v.id, s.edid.as_deref().unwrap_or("?"));
                sounds.write(PlaySound { sound, at: Some(at) });
            }
        } else {
            *seen = Passing { last: d, closer: p.closer, before: p.at, at: v.pos };
        }
    }
    st.shots.retain(|id, _| now.contains(id));
}

/// Actors' weapons' idle sounds (see the module notes).
fn idles(mut st: ResMut<OwnSounds>, sfx: Option<Res<Sfx>>, script: Option<Res<ScriptState>>, sim: Res<SimState>, held: Query<&HeldWeapon>, mut out: MessageWriter<SoundSource>) {
    let (Some(sfx), Some(rt)) = (sfx, script.as_deref().and_then(|s| s.rt.as_ref())) else { return };
    let mut done: HashSet<u32> = HashSet::new();
    for h in &held {
        if h.reference == FormId::PLAYER_REF.0 || !done.insert(h.reference) {
            continue;
        }
        let Some(sound) = sfx.forms.weapons.get(&h.weapon).and_then(|w| w.idle) else { continue };
        let me = FormId(h.reference);
        if !rt.game.extra.get(&me).is_some_and(|e| e.weapon_out) || rt.game.is_dead(me) {
            continue;
        }
        let Some(at) = sim.world.peek(me).map(|s| s.pos) else { continue };
        st.idles.insert(h.reference);
        out.write(SoundSource { key: SoundSource::key(IDLE_KIND, h.reference), sound, at, looping: true });
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<OwnSounds>) {
    if exit.read().last().is_some() {
        info!(
            "own sounds summary: {} add-on nodes with a sound shown; {} projectiles with a looping sound, {} whizzed by; {} actors' weapons idled",
            st.addons.len(),
            st.projectile_loops.len(),
            st.whizzes,
            st.idles.len()
        );
    }
}

pub struct OwnSoundsPlugin;

impl Plugin for OwnSoundsPlugin {
    fn build(&self, app: &mut App) {
        // Before sfx and the ambient play what they send (nvf_play::sfx::SfxSet).
        app.init_resource::<OwnSounds>().add_systems(Update, (addons, projectiles, idles).before(nvf_play::sfx::SfxSet)).add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_point_of_a_pass() {
        // A shot along x passing 50 units from a listener at the origin.
        assert_eq!(nearest_on([-100.0, 50.0, 0.0], [100.0, 50.0, 0.0], [0.0; 3]), [0.0, 50.0, 0.0]);
        // Wholly before the listener: the end nearer it.
        assert_eq!(nearest_on([-300.0, 50.0, 0.0], [-100.0, 50.0, 0.0], [0.0; 3]), [-100.0, 50.0, 0.0]);
        assert_eq!(nearest_on([5.0; 3], [5.0; 3], [0.0; 3]), [5.0; 3]);
    }
}
