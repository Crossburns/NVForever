//! The player knocked down (docs/spec-notes/unconscious.md §5.4): PushActorAway and
//! explosions lay the player down as they do anyone (`fnv_game::unconscious`); here the
//! body drops, the camera watches it, and the player gets up with the game's clip.
//!
//! - **Falling**: the third-person rig is handed to its ragdoll ([`PlayerRig`] and a
//!   [`Corpse`], as for a death, `death_cam.rs`) and thrown with the knockdown's velocity
//!   (`Game::take_shove`); `player_body.rs` leaves the body to the ragdoll meanwhile
//!   ([`PlayerKnockdown::ragdolled`]). A push while the body lies shoves it again.
//! - **The view** [observed: capture `kd_player_knockdown` 2026-10-18, unconscious.md
//!   §5.4]: while the body flies and lies the camera stays exactly where it was when the
//!   player fell, not turning: at the eye in first person, which then shows the body from
//!   outside (the view leaves first person), and where the third-person camera was in third
//!   person. For the getting up the view is the third-person camera behind the player (in
//!   first person too), and a first-person view comes back once the player stands.
//! - **Controls**: all held from the fall until the get-up clip is over
//!   (`nvf_play::script::ScriptState::locks`).
//! - **Lying**: the side the body lies on (the knocked-out NPCs' rule, `knockouts.rs`) is
//!   told to the game (IsFacingUp).
//! - **Getting up**: when the game says the knockdown is over (`KnockOut::rising`), the
//!   ragdoll leaves the world, the character stands where the ragdoll's pelvis lies and
//!   the idle of that side plays on the body (`GetUpFaceUp` / `GetUpFaceDown`, FalloutNV.esm
//!   0x00003BD9 / 0x00003BDA, `MT_KnockDownFaceUp.kf` / `…FaceDown.kf`, as PlayIdle would
//!   ask, `idles.rs`). Once its first pose is on the bones the character is turned and
//!   moved so that the clip's pelvis and pelvis → head line lie where the ragdoll's did
//!   (the NPCs' rule). When the idle has played, the game is told (`finish_getting_up`)
//!   and the view and the controls are the player's again.
//! - Without a body or a ragdoll for its skeleton the player stays where they are, the
//!   controls held until the knockdown's time is over.
//! - The player killed while down: the death camera takes over (`death_cam.rs`).
//!
//! Log lines: `player knockdown: the player falls (…)`, `player knockdown: the player gets
//! up face up|down …`, `player knockdown: laid over the ragdoll: …`, `player knockdown:
//! the player is up …`.

use crate::limbs::ActorRig;
use crate::player_body::PlayerBody;
use crate::ragdoll::{Corpse, PlayerRig, Ragdolls};
use bevy::prelude::*;
use fnv_game::idles::IdleSource;
use fnv_sim::FormId;
use nvf_assets::data::GameData;
use nvf_play::control::{PlayerControl, Pov};
use nvf_play::phys::PhysState;
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::{space, PlayerCamera};

/// Real seconds the getting up may take at most before the game is told it is over
/// anyway (a clip that never started) [design: the clips last 2 s].
pub const RISE_MAX: f32 = 4.0;
/// The get-up idles, by side (face up, face down) [observed: FalloutNV.esm IDLE records,
/// unconscious.md §1].
pub const GETUP_IDLES: [&str; 2] = ["GetUpFaceUp", "GetUpFaceDown"];

/// The player's knockdown on screen.
#[derive(Resource, Default)]
pub struct PlayerKnockdown {
    down: Option<Down>,
    /// The view was from outside (third person, circling) in the last frame the player
    /// was not knocked down. The fall reads this, not the view at the fall: the controls
    /// held while the player is down (`ScriptState::locks`) put the view in first person,
    /// and they may be applied before this notices the knockdown.
    outside_before: bool,
    /// Knockdowns shown and get-up clips played, for the exit report.
    shown: u32,
    clips: u32,
}

impl PlayerKnockdown {
    /// The body lies as a ragdoll: `player_body.rs` leaves it alone.
    pub fn ragdolled(&self) -> bool {
        self.down.as_ref().is_some_and(|d| matches!(d.phase, Phase::Lying) && d.body.is_some())
    }

    /// The player is knocked down and this has the camera.
    pub fn active(&self) -> bool {
        self.down.is_some()
    }
}

struct Down {
    /// The view was first person when the player fell.
    was_first: bool,
    /// The view was from outside when the player fell: the third-person view comes back
    /// once the player is up (the controls held while down put it in first person).
    outside: bool,
    /// The third-person rig's root and the rig, when its ragdoll took it.
    body: Option<(Entity, ActorRig)>,
    phase: Phase,
    /// Real seconds since the fall.
    real: f32,
    /// The camera as it was when the player fell: it stays there while the body lies.
    held: Option<Transform>,
    /// The side the body was seen lying on.
    face_up: Option<bool>,
}

impl Down {
    /// Give the player the view they had before the fall.
    fn view_back(&self, control: &mut PlayerControl) {
        if self.was_first {
            control.pov = Pov::First;
        } else if self.outside {
            control.pov = Pov::Third;
        }
    }
}

enum Phase {
    Lying,
    /// The get-up idle is asked for: the ragdoll's pelvis and head (game space), whether
    /// the clip's first pose was laid over them, when it began (real seconds), and the
    /// idle once it is seen playing.
    Rising { idle: Option<FormId>, lay: Option<([f32; 3], [f32; 3])>, aligned: bool, frames: u32, since: f32, seen: bool },
}

pub struct PlayerKnockdownPlugin;

impl Plugin for PlayerKnockdownPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerKnockdown>()
            .add_systems(Update, watch.before(nvf_play::player::BodySet).after(crate::death_cam::watch))
            .add_systems(Update, follow.after(crate::ragdoll::pose).run_if(resource_exists::<PhysState>))
            .add_systems(Last, report);
    }
}

/// A bone's world position (Bevy space) by name, from the propagated transforms.
fn bone_at(rig: &ActorRig, globals: &Query<&GlobalTransform>, name: &str) -> Option<Vec3> {
    let &i = rig.skel.by_name.get(name)?;
    globals.get(*rig.nodes.get(i)?).ok().map(|g| g.translation())
}

/// The pelvis and the head, game space.
fn pelvis_head(rig: &ActorRig, globals: &Query<&GlobalTransform>) -> Option<([f32; 3], [f32; 3])> {
    Some((space::to_game(bone_at(rig, globals, "Bip01 Pelvis")?), space::to_game(bone_at(rig, globals, "Bip01 Head")?)))
}

/// Whether the body lies face up (`knockouts.rs`'s rule): its front, (left upper arm −
/// right upper arm) × (head − pelvis) in game space, points up.
fn face_up(rig: &ActorRig, globals: &Query<&GlobalTransform>) -> Option<bool> {
    let g = |n: &str| bone_at(rig, globals, n).map(|p| Vec3::from(space::to_game(p)));
    let (l, r) = (g("Bip01 L UpperArm")?, g("Bip01 R UpperArm")?);
    let (p, h) = (g("Bip01 Pelvis")?, g("Bip01 Head")?);
    Some((l - r).cross(h - p).z > 0.0)
}

/// How the character is to stand so that a pose whose pelvis and head are `got` lies
/// with them on `want` (game space): the new heading (compass, radians) and feet. The
/// pose turns with the character about its feet; heights are left alone.
pub fn lay_over(feet: [f32; 3], heading: f32, got: ([f32; 3], [f32; 3]), want: ([f32; 3], [f32; 3])) -> Option<(f32, [f32; 3])> {
    let flat = |a: [f32; 3], b: [f32; 3]| Vec2::new(b[0] - a[0], b[1] - a[1]);
    let (g, w) = (flat(got.0, got.1), flat(want.0, want.1));
    if g.length() < 1.0 || w.length() < 1.0 {
        return None;
    }
    // Counter-clockwise from above (game z up); a compass heading turns the other way.
    let angle = g.angle_to(w);
    let rel = Vec2::new(got.0[0] - feet[0], got.0[1] - feet[1]);
    let moved = Vec2::from_angle(angle).rotate(rel);
    let shift = Vec2::new(want.0[0] - feet[0], want.0[1] - feet[1]) - moved;
    Some(((heading - angle).rem_euclid(std::f32::consts::TAU), [feet[0] + shift.x, feet[1] + shift.y, feet[2]]))
}

/// Start, carry on and end the player's knockdown.
#[allow(clippy::too_many_arguments)]
fn watch(
    mut commands: Commands,
    real: Res<Time<Real>>,
    mut script: Option<ResMut<ScriptState>>,
    mut control: ResMut<PlayerControl>,
    body: Res<PlayerBody>,
    data: Option<Res<GameData>>,
    mut ragdolls: ResMut<Ragdolls>,
    (mut phys, mut player, death): (Option<ResMut<PhysState>>, ResMut<Player>, Res<crate::death_cam::DeathCam>),
    (globals, cams): (Query<&GlobalTransform>, Query<&Transform, With<PlayerCamera>>),
    mut kd: ResMut<PlayerKnockdown>,
    mut pushes: MessageWriter<nvf_play::combat::ActorPush>,
    actor_world: Option<Res<crate::actors::ActorWorld>>,
) {
    let me = FormId::PLAYER_REF;
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let knocked = rt.game.knockout(me);
    let kd = &mut *kd;
    // Killed while down, or the knockdown gone (a load, ResurrectActor): let go.
    if kd.down.is_some() && (rt.game.is_dead(me) || death.down() || knocked.is_none()) {
        let d = kd.down.take().expect("checked");
        if !rt.game.is_dead(me) {
            d.view_back(&mut control);
            if let Some((root, _)) = d.body.filter(|_| matches!(d.phase, Phase::Lying)) {
                commands.entity(root).remove::<(PlayerRig, Corpse)>();
                if let Some(p) = phys.as_deref_mut() {
                    p.world.remove_ragdoll(me.0);
                }
                ragdolls.rest.remove(&me.0);
            }
        }
        info!("player knockdown: over without getting up ({})", if rt.game.is_dead(me) { "the player died" } else { "the knockdown ended" });
        return;
    }
    let Some(k) = knocked else {
        // The player's own view, not a dialogue close-up's (`talk_camera`), which reads as
        // first person past its close point: a knockdown there gets up in the view the
        // player chose.
        kd.outside_before = PlayerControl { talk_camera: 0.0, ..*control }.third_person_view();
        return;
    };
    if rt.game.is_dead(me) || death.down() {
        return;
    }
    if kd.down.is_none() {
        if !k.lying() {
            // Up already (a save made while getting up): nothing to show.
            rt.game.finish_getting_up(me);
            return;
        }
        let was_first = !kd.outside_before;
        ragdolls.rest.remove(&me.0);
        let templates = crate::ragdoll::worn_templates(actor_world.as_deref(), body.worn());
        let mut corpse = body.third_rig().and_then(|(root, rig)| Some((root, data.as_deref().and_then(|d| ragdolls.corpse(d, &rig, &templates))?, rig)));
        let shove = rt.game.take_shove(me);
        if let (Some((_, c, _)), Some(v)) = (corpse.as_mut(), shove) {
            c.velocity = v;
        }
        let body = corpse.map(|(root, c, rig)| {
            commands.entity(root).insert((PlayerRig(rig.clone()), c));
            (root, rig)
        });
        if was_first && body.is_some() {
            control.pov = Pov::Third;
        }
        info!(
            "player knockdown: the player falls ({}, {} person{}){}",
            k.cause.name(),
            if was_first { "first" } else { "third" },
            if was_first && body.is_some() { ", the view leaves first person" } else { "" },
            match (&body, shove) {
                (Some(_), Some(v)) => format!(": ragdoll thrown at ({:.0}, {:.0}, {:.0}) units/s", v[0], v[1], v[2]),
                (Some(_), None) => ": ragdoll".into(),
                (None, _) => ": no ragdoll, the player stays where they are".into(),
            }
        );
        kd.shown += 1;
        // The camera as the player fell (the eye in first person): it stays there.
        let held = cams.single().ok().copied();
        kd.down = Some(Down { was_first: was_first && body.is_some(), outside: !was_first, body, phase: Phase::Lying, real: 0.0, held, face_up: None });
        return;
    }
    let d = kd.down.as_mut().expect("checked");
    d.real += real.delta_secs();
    match &mut d.phase {
        Phase::Lying if k.lying() => {
            if let Some(dv) = rt.game.take_shove(me) {
                if let Some((pelvis, _)) = d.body.as_ref().and_then(|(_, rig)| pelvis_head(rig, &globals)) {
                    pushes.write(nvf_play::combat::ActorPush { form_id: me.0, point: pelvis, dv });
                }
            }
            if let Some(up) = d.body.as_ref().and_then(|(_, rig)| face_up(rig, &globals)) {
                if d.face_up != Some(up) {
                    d.face_up = Some(up);
                    rt.game.set_knocked_side(me, up);
                }
            }
        }
        Phase::Lying => {
            // Its time is over: the ragdoll goes and the get-up clip plays.
            let lay = d.body.as_ref().and_then(|(_, rig)| pelvis_head(rig, &globals));
            if let Some((root, _)) = d.body.as_ref() {
                commands.entity(*root).remove::<(PlayerRig, Corpse)>();
                if let Some(p) = phys.as_deref_mut() {
                    p.world.remove_ragdoll(me.0);
                }
                ragdolls.rest.remove(&me.0);
            }
            let side = usize::from(d.face_up.or(k.facing_up) == Some(false));
            let idle = d.body.as_ref().and_then(|_| rt.game.data.by_edid(GETUP_IDLES[side]).or_else(|| rt.game.data.by_edid(GETUP_IDLES[1 - side])));
            if let Some((pelvis, head)) = lay {
                // Stand on the ground under the pelvis, facing along the body (the clip's
                // own turn is laid over it once it plays).
                if let Some(p) = phys.as_deref_mut() {
                    p.place_player(pelvis, 60.0, 1000.0);
                    player.feet = p.player.as_ref().map_or(pelvis, |c| c.feet);
                }
                let dir = Vec2::new(head[0] - pelvis[0], head[1] - pelvis[1]);
                if dir.length() > 1.0 {
                    player.heading = dir.x.atan2(dir.y).rem_euclid(std::f32::consts::TAU);
                }
            }
            let asked = idle.is_some_and(|i| rt.game.ask_idle(me, i, IdleSource::Script));
            info!(
                "player knockdown: the player gets up face {} {:.1} s after the fall{}",
                if side == 0 { "up" } else { "down" },
                d.real,
                if asked { format!(" ({})", GETUP_IDLES[side]) } else { ", no get-up clip".into() }
            );
            if !asked {
                rt.game.finish_getting_up(me);
                d.view_back(&mut control);
                kd.down = None;
                info!("player knockdown: the player is up (no clip)");
                return;
            }
            kd.clips += 1;
            d.phase = Phase::Rising { idle, lay, aligned: false, frames: 0, since: d.real, seen: false };
        }
        Phase::Rising { idle, lay, aligned, frames, since, seen } => {
            let playing = rt.game.idle_playing(me);
            if playing.is_some() && playing == *idle {
                *seen = true;
                *frames += 1;
            }
            // Two frames into the clip its first pose is on the propagated bones: lay it
            // over where the ragdoll lay.
            if *seen && !*aligned && *frames >= 2 {
                *aligned = true;
                let got = body.third_rig().and_then(|(_, rig)| pelvis_head(&rig, &globals));
                if let (Some(got), Some(want)) = (got, *lay) {
                    if let Some((heading, feet)) = lay_over(player.feet, player.heading, got, want) {
                        let turned = (heading - player.heading).to_degrees();
                        let moved = Vec2::new(feet[0] - player.feet[0], feet[1] - player.feet[1]).length();
                        player.heading = heading;
                        if let Some(p) = phys.as_deref_mut() {
                            p.place_player(feet, 60.0, 1000.0);
                            player.feet = p.player.as_ref().map_or(feet, |c| c.feet);
                        }
                        info!("player knockdown: laid over the ragdoll: turned {turned:.0}°, moved {moved:.0} units");
                    }
                }
            }
            let over = (*seen && playing != *idle) || d.real - *since > RISE_MAX;
            if over {
                rt.game.finish_getting_up(me);
                let was_first = d.was_first;
                info!("player knockdown: the player is up {:.1} s after the fall; the controls{} are theirs again", d.real, if was_first { " and the first-person view" } else { "" });
                d.view_back(&mut control);
                kd.down = None;
            }
        }
    }
}

/// While the body lies, hold the camera where it was when the player fell (see the
/// module notes); while the player gets up, leave it to the third-person camera.
fn follow(mut kd: ResMut<PlayerKnockdown>, mut cams: Query<&mut Transform, With<PlayerCamera>>) {
    let Some(d) = kd.down.as_mut() else { return };
    if !matches!(d.phase, Phase::Lying) {
        return;
    }
    let Ok(mut t) = cams.single_mut() else { return };
    match d.held {
        Some(h) => *t = h,
        None => d.held = Some(*t),
    }
}

fn report(mut exit: MessageReader<AppExit>, kd: Res<PlayerKnockdown>) {
    if exit.read().last().is_none() || kd.shown == 0 {
        return;
    }
    info!("player knockdown: {} knockdown(s) shown, {} get-up clip(s){}", kd.shown, kd.clips, if kd.active() { ", the player down at exit" } else { "" });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..2).all(|i| (a[i] - b[i]).abs() < 1e-3)
    }

    #[test]
    fn a_pose_is_laid_over_the_ragdoll() {
        // The clip's pelvis 10 north of the feet, head further north; the ragdoll lies
        // with its pelvis at (100, 0) and its head to the east.
        let feet = [0.0, 0.0, 5.0];
        let got = ([0.0, 10.0, 0.0], [0.0, 60.0, 0.0]);
        let want = ([100.0, 0.0, 0.0], [150.0, 0.0, 0.0]);
        let (heading, at) = lay_over(feet, 0.0, got, want).unwrap();
        // North to east: a quarter turn clockwise.
        assert!((heading - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "{heading}");
        // The pelvis, turned with the feet, lands on the ragdoll's: 10 east of the feet.
        assert!(close(at, [90.0, 0.0, 5.0]), "{at:?}");
        assert_eq!(at[2], 5.0);
        // Already lying right: nothing moves.
        let (h, at) = lay_over(feet, 1.0, want, want).unwrap();
        assert!((h - 1.0).abs() < 1e-5 && close(at, feet));
        // A body standing on end gives no line along the ground.
        assert!(lay_over(feet, 0.0, ([0.0; 3], [0.0, 0.0, 50.0]), want).is_none());
    }

    #[test]
    fn the_getup_idles_are_named_by_side() {
        assert_eq!(GETUP_IDLES, ["GetUpFaceUp", "GetUpFaceDown"]);
        assert!(!PlayerKnockdown::default().active() && !PlayerKnockdown::default().ragdolled());
    }
}
