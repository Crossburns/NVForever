//! The view narrows onto the speaker in a conversation (docs/design/dialogue-play.md
//! §8.8): the same conversations `crate::dialogue_look` turns the view for
//! (`Game::dialogue`). In the original the speaker's head comes out the same size from any
//! distance, about [`HEAD_SHARE`] of the view's height with the face's centre about
//! [`FACE_FROM_TOP`] of it from the top, so it is a framing and not a fixed zoom [observed:
//! the original, 2026-11-06, Doc Mitchell from about 90 and about 300 units].
//!
//! - **Field of view.** Worked out from the distance to the face and an assumed head
//!   ([`framed_vfov`]); never wider than the view the player has, never narrower than
//!   [`MIN_VFOV`]. It follows the speaker's distance slowly ([`FOLLOW_SECONDS`]), not the
//!   mouse: the player still looks around while zoomed.
//! - **Framing.** The opening turn (`crate::dialogue_look`) aims [`face_drop`] under the
//!   face, so the face lands at [`FACE_FROM_TOP`], and stays on it for
//!   [`crate::dialogue_look::ZOOM_HOLD_SECONDS`] while the speaker finishes turning; after
//!   it, and at the first move of the mouse, the view is the player's [owner decision
//!   2026-10-10].
//! - **Looking around** is scaled with the zoom ([`DialogueZoom::look_scale`]), so the
//!   mouse moves the picture as far as it does unzoomed.
//! - **Who owns the field of view** (as `chargen/vigor.rs`'s `fov_before`). [`DialogueZoom`]
//!   keeps what was on the camera underneath its own value and gives it back. A value on
//!   the camera that is not the one it last wrote is someone else's (the settings slider,
//!   the console's SetCameraFOV, the sights' zoom): the new base, zoomed from at once. Who
//!   else reads a base asks [`DialogueZoom::underlying`] first, so a zoomed value never
//!   becomes one (`crate::aim_zoom`, `crate::free_cam`, `crate::chargen::vigor`).
//! - **Third person** ends in the same close-up: the camera travels to the first-person eye
//!   with the zoom and the body goes on the way ([`crate::control::TALK_CAMERA_CLOSE`]),
//!   and comes back when the conversation ends [observed: the original, 2026-11-06, the
//!   same framing and no body]. `PlayerControl::pov` is not touched: the player's own view
//!   is still what scripts and saves see.
//! - **No zoom**: through a talking activator (not in this version), a dead speaker, one
//!   that is not drawn, the free camera, the character creator, a kill camera; the Vigor
//!   Tester has the field of view for itself and gets it at once. A conversation that ends
//!   or stops being zoomable eases out.
//! - **Pausing**: the eases run on the virtual clock, so the Esc pause menu holds them.
//!   With `--menu-pause original` the picture stands still, so they complete at once.
//!
//! The depth of field that goes with it is nvforever's `imagespace.rs` ([`DialogueZoom::blend`],
//! [`DialogueZoom::face`]).
//!
//! Log, starting `dialogue zoom:`: one line as each ease ends, with the distance and the
//! angle reached; one line per conversation that gets no zoom, and why.

use crate::combat::is_dead;
use crate::control::PlayerControl;
use crate::dialogue_look::{smooth, FaceFrom, SpeakerFace};
use crate::free_cam::FreeCam;
use crate::pause::{MenuPause, Paused};
use crate::phys::PhysState;
use crate::player::{Player, MAX_PITCH};
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::PlayerCamera;
use bevy::prelude::*;
use fnv_game::game::menus;
use fnv_sim::FormId;

/// The speaker's head, crown to chin, in game units [observed: our run of `dialogue-zoom`,
/// 2026-11-06: Chet's head as drawn came out about 185 of 720 px from 394 units at 8.7°, in
/// a view 59.9 units tall there, so about 15.4; one head measured by eye]. Creatures and
/// robots get the same number: the rigs have bones, not the head mesh's size, so a head
/// measured as drawn would need the skinned shape's bounds.
pub const HEAD_HEIGHT: f32 = 15.5;
/// The head's share of the view's height [observed: about 215 of 720 px in the original,
/// near and far].
pub const HEAD_SHARE: f32 = 0.30;
/// Where the point the view aims at (`SpeakerFace`, the head bone as drawn) lands, as a share
/// of the view's height from the top. The original has the face's centre at about 0.32
/// [observed: captures of 2026-11-06]; the bone lies about 0.06 of the view under that centre
/// at this framing [observed: Chet at 0.32 showed the face's centre at 0.26], hence 0.38.
pub const FACE_FROM_TOP: f32 = 0.38;
/// Seconds of the ease in [observed: fitted, rough; settled in about 0.6-1 s in the
/// original at samples every 4 frames].
pub const IN_SECONDS: f32 = 0.8;
/// Seconds of the ease out [observed: fitted, rough; about 0.3 s, 4-6 samples].
pub const OUT_SECONDS: f32 = 0.3;
/// The narrowest vertical field of view, radians, for a speaker far away (6°: a head at
/// the share above from about 490 units; the original's far spot was about 300) [design].
pub const MIN_VFOV: f32 = 6.0 * std::f32::consts::PI / 180.0;
/// How slowly the zoom follows the distance to the face (a speaker walking up), seconds;
/// a head's idle sway would otherwise breathe in the field of view [design].
pub const FOLLOW_SECONDS: f32 = 0.5;

/// Whether a conversation zooms (`nvforever --no-dialogue-zoom` switches it off).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoomSettings {
    pub enabled: bool,
}

impl Default for ZoomSettings {
    fn default() -> Self {
        ZoomSettings { enabled: true }
    }
}

/// The vertical field of view (radians) in which a head [`HEAD_HEIGHT`] tall at `distance`
/// fills [`HEAD_SHARE`] of the view's height, no narrower than [`MIN_VFOV`]. The view is
/// `2 d tan(v/2)` tall at distance `d`, so the aspect ratio does not enter.
pub fn framed_vfov(distance: f32) -> f32 {
    (2.0 * (HEAD_HEIGHT / (2.0 * HEAD_SHARE * distance.max(1.0))).atan()).max(MIN_VFOV)
}

/// How far under the face the view looks (radians) for the face to sit at
/// [`FACE_FROM_TOP`] in a view `vfov` tall.
pub fn face_drop(vfov: f32) -> f32 {
    (2.0 * (0.5 - FACE_FROM_TOP) * (vfov * 0.5).tan()).atan()
}

/// What [`DialogueZoom::apply`] asks of the camera this frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Applied {
    /// The field of view to write.
    pub write: Option<f32>,
    /// The ease out is over: the field of view underneath is back (or still) on the camera.
    pub back: Option<f32>,
}

/// The zoom of the conversation now, or easing out of the last one.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct DialogueZoom {
    /// 0 (the player's view) to 1 (framed), before [`smooth`].
    weight: f32,
    /// The field of view wanted this frame; none when the zoom is not wanted.
    want: Option<f32>,
    /// The field of view last wanted (held while easing out).
    target: Option<f32>,
    /// What is on the camera underneath the zoom.
    base: Option<f32>,
    /// What this module last wrote.
    applied: Option<f32>,
    /// The distance to the face, followed slowly ([`FOLLOW_SECONDS`]).
    distance: f32,
    /// The face, game space (held while easing out).
    face: Option<[f32; 3]>,
    /// The line for the end of the ease in has been said.
    told_in: bool,
    /// The conversation told it gets no zoom.
    told_none: Option<FormId>,
}

impl DialogueZoom {
    /// The field of view underneath `fov`, what is on the camera now: `fov` itself unless
    /// this module wrote it. Who keeps a base goes by this (the module notes).
    pub fn underlying(&self, fov: f32) -> f32 {
        match (self.applied, self.base) {
            (Some(a), Some(b)) if a == fov => b,
            _ => fov,
        }
    }

    /// How far the zoom has come, eased: 0 off, 1 framed.
    pub fn blend(&self) -> f32 {
        smooth(self.weight)
    }

    /// The speaker's face, game space, while there is a zoom.
    pub fn face(&self) -> Option<[f32; 3]> {
        self.face.filter(|_| self.weight > 0.0)
    }

    /// How far under the face the opening turn should aim (0 without a zoom).
    pub fn face_drop(&self) -> f32 {
        self.want.map_or(0.0, face_drop)
    }

    /// Whether a zoom is wanted this frame.
    pub fn wanted(&self) -> bool {
        self.want.is_some()
    }

    /// What looking around is multiplied by: the share of the unzoomed view's height the
    /// zoomed view has, 1 without a zoom [design: the sights' zoom has no such scale to
    /// follow].
    pub fn look_scale(&self) -> f32 {
        match (self.applied, self.base) {
            (Some(a), Some(b)) if b > 0.0 => ((a * 0.5).tan() / (b * 0.5).tan()).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    /// Another owner has the camera now (the Vigor Tester): off at once, and what this
    /// module last wrote, if still there, is taken back. Whatever else is there is left.
    fn let_go(&mut self, seen: f32) -> Applied {
        let write = self.base.filter(|b| self.applied == Some(seen) && *b != seen);
        (self.weight, self.want, self.applied, self.told_in) = (0.0, None, None, false);
        Applied { write, back: None }
    }

    /// Advance the ease by `dt` towards `want` (none: out). `instant` completes it at once.
    fn step(&mut self, want: Option<f32>, dt: f32, instant: bool) {
        self.want = want;
        if want.is_some() {
            self.target = want;
        }
        self.weight = match (want.is_some(), instant) {
            (true, true) => 1.0,
            (false, true) => 0.0,
            (true, false) => (self.weight + dt.max(0.0) / IN_SECONDS).min(1.0),
            (false, false) => (self.weight - dt.max(0.0) / OUT_SECONDS).max(0.0),
        };
    }

    /// What to do with the camera, whose field of view is `seen` now (the module notes).
    fn apply(&mut self, seen: f32) -> Applied {
        // Not what this module wrote: someone else's, the new base.
        if self.applied != Some(seen) {
            self.base = Some(seen);
        }
        let base = self.base.unwrap_or(seen);
        let s = self.blend();
        if s <= 0.0 {
            let was = self.applied.take().is_some();
            return Applied { write: (was && base != seen).then_some(base), back: was.then_some(base) };
        }
        // A very near speaker does not widen the view.
        let target = self.target.unwrap_or(base).min(base);
        let fov = base + (target - base) * s;
        self.applied = Some(fov);
        Applied { write: (fov != seen).then_some(fov), back: None }
    }
}

/// Where the speaker's face is, as `crate::dialogue_look::face_speaker` finds it.
fn speaker_face(actor: FormId, drawn: Option<(u32, [f32; 3])>, phys: &PhysState, sim: &SimState) -> Option<([f32; 3], FaceFrom)> {
    let volume = phys.world.actor_volume(actor.0).map(|v| (v.feet, v.height));
    let reference = sim.world.peek(actor).map(|s| s.pos).or_else(|| sim.refs.get(actor).map(|p| p.pos));
    crate::dialogue_look::face_point(actor.0, drawn, volume, reference)
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}

/// What the zoom wants this frame, and the third-person camera's travel with it. Before
/// `crate::dialogue_look::face_speaker`, which aims under the face by the result.
#[allow(clippy::too_many_arguments)]
pub(crate) fn track(
    time: Res<Time>,
    settings: Res<ZoomSettings>,
    (mode, paused): (Res<MenuPause>, Res<Paused>),
    script: Option<Res<ScriptState>>,
    face: Res<SpeakerFace>,
    (phys, sim): (Res<PhysState>, Res<SimState>),
    free: Option<Res<FreeCam>>,
    player: Option<ResMut<Player>>,
    mut control: ResMut<PlayerControl>,
    cams: Query<&Projection, With<PlayerCamera>>,
    mut zoom: ResMut<DialogueZoom>,
) {
    // The pause menu holds everything; the settings slider still reaches [`apply`].
    if paused.0 {
        return;
    }
    let Some(mut player) = player else { return };
    let Ok(Projection::Perspective(p)) = cams.single() else { return };
    let base = zoom.underlying(p.fov);
    let game = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    let conv = game.and_then(|g| g.dialogue.as_ref());
    // The Vigor Tester has the field of view for itself (`crate::chargen::vigor`).
    let vigor = game.is_some_and(|g| g.menu == Some(menus::LOVE_TESTER));
    // Something else has the camera.
    let taken = vigor || free.is_some_and(|f| f.on) || control.portrait.is_some() || control.cinematic;
    let found = match conv.filter(|_| settings.enabled && !taken) {
        None => Err(None),
        Some(c) if c.through.is_some() => Err(Some("it goes through a talking activator")),
        Some(c) if is_dead(script.as_deref(), c.actor.0) => Err(Some("the speaker is dead")),
        Some(c) => match speaker_face(c.actor, face.0, &phys, &sim) {
            Some((_, FaceFrom::Reference)) | None => Err(Some("the speaker is not drawn")),
            Some(found) => Ok((c.actor, found)),
        },
    };
    if conv.is_none() {
        zoom.told_none = None;
    }
    if let (Err(Some(why)), Some(c), Some(g)) = (&found, conv, game) {
        if zoom.told_none != Some(c.actor) {
            zoom.told_none = Some(c.actor);
            info!("dialogue zoom: no zoom in the conversation with {}: {why}", g.data.name(c.actor));
        }
    }
    let instant = vigor || *mode == MenuPause::Original;
    let dt = time.delta_secs();
    let mut said = None;
    let want = found.ok().map(|(actor, (point, from))| {
        let d = distance(player.eye(), point);
        // From the start of a zoom the distance is taken as it is, then followed.
        zoom.distance = if zoom.want.is_none() || instant { d } else { zoom.distance + (d - zoom.distance) * (1.0 - (-dt.max(0.0) / FOLLOW_SECONDS).exp()) };
        zoom.face = Some(point);
        said = Some((actor, from));
        framed_vfov(zoom.distance).min(base)
    });
    // The detached camera looks off the player's heading: the close-up looks along it, as
    // `crate::control::detach_camera` does when it stops (it runs only while playing). Here,
    // before the opening turn, so the turn starts from the view the player sees.
    if want.is_some() && zoom.weight == 0.0 && control.orbit.free && (control.orbit.yaw != 0.0 || control.orbit.pitch != 0.0) {
        let o = &mut control.orbit;
        player.heading = (player.heading + o.yaw).rem_euclid(std::f32::consts::TAU);
        player.pitch = (player.pitch + o.pitch).clamp(-MAX_PITCH, MAX_PITCH);
        (o.yaw, o.pitch) = (0.0, 0.0);
    }
    zoom.step(want, dt, instant);
    if let (Some(fov), Some((actor, from)), Some(g), true, false) = (want, said, game, zoom.weight >= 1.0, zoom.told_in) {
        zoom.told_in = true;
        let source = match from {
            FaceFrom::Head => "its head as drawn",
            FaceFrom::Volume => "the top of its hit volume: it has no drawn head",
            FaceFrom::Reference | FaceFrom::Activator => "?",
        };
        let close = if control.own_third_person() { "; third person: the camera at the eye, no body" } else { "" };
        info!(
            "dialogue zoom: in on {} ({source}): {:.0} units, vertical {:.1}° to {:.1}°, over {IN_SECONDS:.2} s{close}",
            g.data.name(actor),
            zoom.distance,
            base.to_degrees(),
            fov.to_degrees()
        );
    }
    // The third-person camera travels with the zoom.
    let camera = if control.own_third_person() { zoom.blend() } else { 0.0 };
    if control.talk_camera != camera {
        control.talk_camera = camera;
    }
}

/// Put the zoom on the camera. After `crate::aim_zoom::zoom`, the other writer of a frame,
/// which goes by [`DialogueZoom::underlying`]; a write of its own here is read as the base.
pub(crate) fn apply(script: Option<Res<ScriptState>>, mut zoom: ResMut<DialogueZoom>, mut cams: Query<&mut Projection, With<PlayerCamera>>) {
    let Ok(mut proj) = cams.single_mut() else { return };
    // Read first: a write every frame would mark the projection changed.
    let Projection::Perspective(p) = &*proj else { return };
    // The Vigor Tester may have written its own this frame, before [`track`] saw it open.
    let vigor = script.as_ref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.menu == Some(menus::LOVE_TESTER));
    let done = if vigor { zoom.let_go(p.fov) } else { zoom.apply(p.fov) };
    if let Some(fov) = done.write {
        if let Projection::Perspective(p) = &mut *proj {
            p.fov = fov;
        }
    }
    if let Some(back) = done.back {
        zoom.told_in = false;
        info!("dialogue zoom: out from {:.0} units: the view is back at vertical {:.1}°, over {OUT_SECONDS:.2} s", zoom.distance, back.to_degrees());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The share of the view's height a head at `distance` fills in a view `vfov` tall.
    fn share(distance: f32, vfov: f32) -> f32 {
        HEAD_HEIGHT / (2.0 * distance * (vfov * 0.5).tan())
    }

    const BASE: f32 = 1.044; // About 59.8°: `vertical_fov(75)`.

    /// One frame of the camera as `apply` leaves it.
    fn frame(z: &mut DialogueZoom, fov: &mut f32) -> Applied {
        let a = z.apply(*fov);
        if let Some(w) = a.write {
            *fov = w;
        }
        a
    }

    #[test]
    fn the_head_comes_out_the_same_size_from_any_distance() {
        for d in [60.0, 90.0, 150.0, 300.0] {
            assert!((share(d, framed_vfov(d)) - HEAD_SHARE).abs() < 1e-4, "at {d}");
        }
    }

    #[test]
    fn a_far_speaker_does_not_zoom_absurdly() {
        // The original's far spot, about 300 units, is still framed; beyond the limit the
        // head stays small.
        assert!(framed_vfov(300.0) > MIN_VFOV);
        assert_eq!(framed_vfov(100_000.0), MIN_VFOV);
    }

    #[test]
    fn a_very_near_speaker_does_not_widen_the_view() {
        assert!(framed_vfov(20.0) > BASE, "the framing alone would widen it");
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(Some(framed_vfov(20.0).min(BASE)), 1.0, false);
        frame(&mut z, &mut fov);
        assert_eq!(fov, BASE);
        // And even if the target were wider, the base is the limit.
        z.target = Some(BASE * 1.5);
        frame(&mut z, &mut fov);
        assert_eq!(fov, BASE);
    }

    #[test]
    fn the_face_lands_at_its_place_in_the_view() {
        for v in [8f32, 30.0, 59.8] {
            let v = v.to_radians();
            // Seen `drop` above the view's axis, on a screen of half-height tan(v/2).
            let y = 0.5 - 0.5 * face_drop(v).tan() / (v * 0.5).tan();
            assert!((y - FACE_FROM_TOP).abs() < 1e-5, "{y} at {v}");
        }
        // No zoom, no drop: the opening turn aims at the face itself.
        assert_eq!(DialogueZoom::default().face_drop(), 0.0);
    }

    #[test]
    fn the_ease_takes_its_times_and_gives_the_view_back_exactly() {
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        // Nothing wanted: the camera is not touched.
        z.step(None, 0.1, false);
        assert_eq!(frame(&mut z, &mut fov), Applied::default());
        let target = 0.6;
        z.step(Some(target), IN_SECONDS * 0.5, false);
        frame(&mut z, &mut fov);
        assert!((fov - (BASE + target) / 2.0).abs() < 1e-5, "halfway at half the time: {fov}");
        z.step(Some(target), IN_SECONDS, false);
        frame(&mut z, &mut fov);
        assert!((fov - target).abs() < 1e-6);
        // Settled: no more writes.
        z.step(Some(target), 0.1, false);
        assert_eq!(frame(&mut z, &mut fov).write, None);
        z.step(None, OUT_SECONDS * 0.5, false);
        frame(&mut z, &mut fov);
        assert!(fov > target && fov < BASE);
        z.step(None, OUT_SECONDS, false);
        let a = frame(&mut z, &mut fov);
        assert_eq!((fov, a.back), (BASE, Some(BASE)));
        // Over: not said again, not written again.
        z.step(None, 0.1, false);
        assert_eq!(frame(&mut z, &mut fov), Applied::default());
    }

    #[test]
    fn a_frozen_picture_gets_the_end_of_the_ease_at_once() {
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(Some(0.5), 0.0, true);
        frame(&mut z, &mut fov);
        assert!((fov - 0.5).abs() < 1e-6);
        z.step(None, 0.0, true);
        let a = frame(&mut z, &mut fov);
        assert_eq!((fov, a.back), (BASE, Some(BASE)));
        // The Esc pause menu: no time, not instant: held where it is.
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(Some(0.5), IN_SECONDS / 2.0, false);
        frame(&mut z, &mut fov);
        let held = fov;
        z.step(Some(0.5), 0.0, false);
        frame(&mut z, &mut fov);
        assert_eq!(fov, held);
    }

    #[test]
    fn a_zoomed_value_is_never_a_base() {
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(Some(0.5), IN_SECONDS * 0.5, false);
        frame(&mut z, &mut fov);
        assert_ne!(fov, BASE);
        // The sights' zoom, the console and the Vigor Tester see what is underneath.
        assert_eq!(z.underlying(fov), BASE);
        // A value this module did not write is not a zoomed one.
        assert_eq!(z.underlying(0.77), 0.77);
        // With nothing applied everything is itself.
        assert_eq!(DialogueZoom::default().underlying(fov), fov);
    }

    #[test]
    fn someone_elses_value_is_the_new_base() {
        // The settings slider (or the console) during the zoom: the zoom keeps on from it
        // and gives it back.
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(Some(0.5), IN_SECONDS * 2.0, false);
        frame(&mut z, &mut fov);
        assert!((fov - 0.5).abs() < 1e-6);
        let slider = 1.2;
        fov = slider;
        frame(&mut z, &mut fov);
        assert!((fov - 0.5).abs() < 1e-6, "framed again from the new base");
        assert_eq!(z.underlying(fov), slider);
        z.step(None, OUT_SECONDS * 2.0, false);
        let a = frame(&mut z, &mut fov);
        assert_eq!((fov, a.back), (slider, Some(slider)));
    }

    #[test]
    fn the_sights_zoom_going_out_under_the_ease_ends_on_its_own_base() {
        // The sights' zoom writes its own fading value each frame, the zoom lays over it.
        let mut z = DialogueZoom::default();
        let mut fov = 0.5f32;
        z.step(Some(0.3), IN_SECONDS / 4.0, false);
        frame(&mut z, &mut fov);
        for sights in [0.7f32, 0.9, BASE] {
            z.step(Some(0.3), IN_SECONDS / 4.0, false);
            fov = sights;
            frame(&mut z, &mut fov);
            assert_eq!(z.underlying(fov), sights);
        }
        z.step(None, OUT_SECONDS * 2.0, false);
        frame(&mut z, &mut fov);
        assert_eq!(fov, BASE);
    }

    #[test]
    fn looking_around_is_scaled_with_the_zoom_and_only_then() {
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(None, 0.1, false);
        frame(&mut z, &mut fov);
        assert_eq!(z.look_scale(), 1.0);
        z.step(Some(0.15), IN_SECONDS * 2.0, false);
        frame(&mut z, &mut fov);
        let want = (0.15f32 * 0.5).tan() / (BASE * 0.5).tan();
        assert!((z.look_scale() - want).abs() < 1e-5, "{}", z.look_scale());
        z.step(None, OUT_SECONDS * 2.0, false);
        frame(&mut z, &mut fov);
        assert_eq!(z.look_scale(), 1.0, "restored with the field of view");
        // A speaker too near to narrow the view: no change.
        z.step(Some(BASE), IN_SECONDS * 2.0, false);
        frame(&mut z, &mut fov);
        assert_eq!(z.look_scale(), 1.0);
    }

    #[test]
    fn letting_go_for_the_vigor_tester() {
        // It wrote its own first: left alone, and not reported as the view coming back.
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(Some(0.5), IN_SECONDS / 2.0, false);
        frame(&mut z, &mut fov);
        assert_eq!(z.let_go(0.9), Applied::default());
        assert_eq!((z.blend(), z.underlying(0.9)), (0.0, 0.9));
        // It has not drawn yet: the zoomed value goes, so it finds what lay under it.
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(Some(0.5), IN_SECONDS / 2.0, false);
        frame(&mut z, &mut fov);
        assert_eq!(z.let_go(fov), Applied { write: Some(BASE), back: None });
    }

    #[test]
    fn another_owner_at_the_end_of_the_ease_keeps_the_camera() {
        // The Vigor Tester wrote its own value as the zoom lets go at once: left alone.
        let mut z = DialogueZoom::default();
        let mut fov = BASE;
        z.step(Some(0.5), IN_SECONDS * 2.0, false);
        frame(&mut z, &mut fov);
        let vigor = 0.9;
        fov = vigor;
        z.step(None, 0.0, true);
        let a = frame(&mut z, &mut fov);
        assert_eq!((fov, a.write), (vigor, None));
    }
}
