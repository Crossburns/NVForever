//! VATS mode (docs/design/vats-mode.md): NVForever's own VATS, a Dead Eye-style slow-time
//! mark and fire, not the original's [design: combat is not a parity target].
//!
//! - **Enter** (§3): V (`Control::Vats`) with a gun in hand, not reloading or jammed, a
//!   round loaded and the AP for a shot. With the weapon away V draws it first and enters
//!   once it is out.
//! - **Marking** (§6, §7): the clock runs at [`MARK_SCALE`] (eased in over the swipe,
//!   [`time_scale_at`]); left click (Use) marks the actor under the crosshair, on the body
//!   part the crosshair's ray strikes; each mark reserves the shot's AP
//!   (`fnv_combat::vats::MarkQueue`: no more marks than the AP left and the rounds loaded
//!   allow). A tap of right click (Block, let go within [`UNMARK_TAP`] real seconds)
//!   takes the last mark back, or with none ends the mode; held, it aims down the sights
//!   (§11.5); V again ends it; E (Activate) fires.
//! - **Firing** (§9): the clock runs at [`FIRE_SCALE`] (half speed); every
//!   [`SHOT_INTERVAL`] of game time the next mark in
//!   order: the view snaps to it, the hit is rolled on the mark's chance (§17) and
//!   `crate::combat` fires one shot straight at it, or wide of it on a miss
//!   ([`VatsMode::take_shot`]: no spread, no recoil, no jam, whatever the weapon's
//!   cooldown), and its AP is spent. [`AFTER_LAST`] after the last shot the mode is off
//!   and the clock is back at 1. In first person the player's own aim stays (a scope
//!   stays up); in third person a gun with sights is raised for a mark farther than 20 m
//!   and fired from the hip nearer, as the NPCs do (`crate::combat::aims_down_sights`).
//! - **The look** (§5): the swipe, the amber grade and its fading out are four numbers
//!   ([`VatsMode::look`]) the renderer's image-space pass draws (nvforever
//!   `imagespace.rs`); the marks' "x"s are drawn here ([`draw_marks`]).
//! - **Hit chance** (§17): each mark's chance to hit (`fnv_combat::vats::raw_chance`,
//!   `final_chance`: the part's BPTD To Hit Chance, the weapon's skill, the spread at the
//!   mark's distance, the player's to-hit perks, Concentrated Fire, the 95 % cap) is worked
//!   out again every frame and drawn as "NN%" beside its "x" ([`draw_labels`]).
//!   Before marking, the actor under or nearest the crosshair shows every body part's
//!   chance on the part, the same work as a mark's ([`spot_on`]), the part under the
//!   crosshair bracketed and brighter ([`draw_previews`], §17.5).
//! - **Helpers and AP perks** (§18): with Mysterious Stranger or Miss Fortune, a mark's
//!   attack may bring the helper beside the player, who fires one shot of their own
//!   weapon at the mark (an `ActorFire`) and fades away; Friendly Help doubles the odds.
//!   Grim Reaper's Sprint gives AP back after a sequence that killed; Nerves of Steel
//!   speeds the AP's regeneration.
//! - **Sounds** (§10): the base game's `UIVATS*` SOUN records, found by Editor ID.
//! - **AP** (§7.2): reserved while marking (the HUD's bar shows what is left after the
//!   reservations, [`VatsMode::reserved`]), spent per shot; outside the mode spent AP comes
//!   back at `fActionPointsRestoreRate` × the maximum a second [hypothesis].
//! - **Scripts** (§12): `fnv_game::Game::set_vats` each frame (GetVATSMode, GetVATSValue).
//! - **Beyond single shots** (§16): an automatic weapon fires a burst a mark
//!   (`fnv_combat::vats::Volley::bursts`); a melee or unarmed weapon marks within 5 m and,
//!   firing, puts the player beside each mark and swings once (`fnv_combat::vats::MeleeRun`,
//!   [`stand_beside`]; the swing is combat's own, [`VatsMode::take_swing`], landing on the mark,
//!   [`VatsMode::melee_mark`]); a grenade or thrown weapon is thrown on the low arc that
//!   lands on each mark (`crate::combat::fire`, `fnv_combat::vats::lob_dir`).
//! - Any menu, a conversation (a forced greeting too), death, a script disabling the
//!   fighting controls or the weapon leaving the hand ends the mode (§9.3, §11.6), and so
//!   does any action but marking, aiming and accepting ([`LEAVING`]: E with nothing
//!   marked activates what is under the crosshair, Wait, Jump, Sneak, the hotkeys…); a
//!   weapon that does not come out within [`DRAW_LIMIT`] gives up entering (§11.8).
//!   Nothing of it is saved (§11.7).
//!
//! Log lines (smoke tests read them): `vats mode: on (…)`, `vats mode: refused: …`,
//! `vats mode: marked …`, `vats mode: mark refused: …`, `vats mode: unmarked …`,
//! `vats mode: firing …`, `vats mode: shot k of n …` (`burst`, `throw`, `swing` for the
//! other kinds, and `closing on …` before a swing), `vats mode: preview …`, `vats mode: off (…)`, and at exit
//! `vats mode summary: …`.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use fnv_combat::vats::{
    final_chance, miss_point, raw_chance, restored_damage, rolled_hit, throw_miss_point, AttackStyle, HitInputs, HitTuning, MarkQueue, MeleeAction, MeleeLimits, MeleeRun, Volley, VolleyEvent, HELD_WEAPON_TO_HIT,
    SIZE_HELD_WEAPON, SIZE_TORSO, WHOLE_TO_HIT,
};
use fnv_game::vats::{pick_helper, Helper, HitPerks};
use fnv_game::actor_values::ACTION_POINTS;
use fnv_phys::{layers, Filter, Owner};
use fnv_sim::FormId;
use std::collections::HashMap;

use crate::activate::HudMessage;
use crate::bindings::{Control, ControlInput};
use crate::combat::{ActorFire, Combat, WeaponAnim};
use crate::control::{ControlLocks, Crosshair, PlayerControl};
use crate::hitvol::{ActorParts, WEAPON_PART};
use crate::interface::{bc_supported, face as font_face, plain_font, set_line, Face};
use bevy::image::CompressedImageFormatSupport;
use nvf_assets::GameData;
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sfx::{LoopSound, PlaySound, Sfx};
use crate::sim::SimState;
use crate::{space, PlaySet, PlayState, PlayerCamera};

/// The clock's speed while marking [design: the owner's "very, very slow"].
pub const MARK_SCALE: f32 = 0.05;
/// The swipe across the screen on entering, and the easing of the clock with it, real
/// seconds [design].
pub const ENTER_SWIPE: f32 = 0.35;
/// The wash over the whole screen after the swipe fades over this long, real seconds
/// [design, after the references' flash].
pub const ENTER_WASH: f32 = 0.6;
/// The wash's strength when the swipe has crossed [design].
pub const WASH_START: f32 = 0.5;
/// The band riding the swipe's edge [design].
pub const BAND_STRENGTH: f32 = 0.85;
/// The grade fades out over this long on leaving, real seconds [design].
pub const LEAVE_FADE: f32 = 0.3;
/// The clock's speed while the marked shots are fired [design: owner 2026-10-18, "the
/// firing is too fast": half speed instead of normal].
pub const FIRE_SCALE: f32 = 0.5;
/// Between two marked shots, game seconds: 0.4 s of real time at [`FIRE_SCALE`]
/// [design: owner 2026-10-18; was 0.12 at full speed].
pub const SHOT_INTERVAL: f32 = 0.2;
/// From the last shot to the end of the mode, game seconds (0.6 s real at
/// [`FIRE_SCALE`]) [design].
pub const AFTER_LAST: f32 = 0.3;
/// A right click let go within this many real seconds of its press, while marking, is a
/// tap: it takes the last mark back; held longer it aims down the sights (§11.5)
/// [design: owner 2026-10-23, "let the player aim or not whilst vats slow time is
/// running"; the aim waits out the tap so a tap does not twitch the zoom].
pub const UNMARK_TAP: f32 = 0.25;
/// V with the weapon away waits this long, real seconds, for the weapon to come out; then
/// entering is given up (§11.8) [design: a draw is well under a second].
pub const DRAW_LIMIT: f32 = 3.0;
/// The controls whose press ends the mode (§11.8): every action but marking (Use),
/// aiming or unmarking (Block), V and accepting the marks (Activate while marking with a
/// mark made, [`leaving_press`]); moving, looking and the camera go on [design: owner
/// 2026-10-25, "any action besides aiming and marking or accepting marks" kicks the
/// player out, so they are never stuck in the mode].
pub const LEAVING: [Control; 17] = [
    Control::Activate,
    Control::ReadyItem,
    Control::CrouchSneak,
    Control::Jump,
    Control::MenuMode,
    Control::Rest,
    Control::Hotkey1,
    Control::AmmoSwap,
    Control::Hotkey3,
    Control::Hotkey4,
    Control::Hotkey5,
    Control::Hotkey6,
    Control::Hotkey7,
    Control::Hotkey8,
    Control::QuickSave,
    Control::QuickLoad,
    Control::Grab,
];

/// The control pressed this frame that ends the mode (§11.8), if any: one of [`LEAVING`],
/// except Activate while marking with `marks` made, which fires them.
pub fn leaving_press(phase: Phase, marks: usize, pressed: impl Fn(Control) -> bool) -> Option<Control> {
    if phase == Phase::Off {
        return None;
    }
    let accepts = phase == Phase::Marking && marks > 0;
    LEAVING.into_iter().find(|&c| pressed(c) && !(c == Control::Activate && accepts))
}

/// How far the crosshair's ray looks for a target when the projectile has no range of
/// its own, units [design].
pub const REACH: f32 = 8192.0;
/// How far from the eye a melee weapon's marks may be, units (§16.2) [design: owner
/// 2026-10-20, "within 5 meters or so"; 5 m at 70 units a metre].
pub const MELEE_MARK_REACH: f32 = 350.0;
/// A melee mark is close enough to swing at within the weapon's reach plus this across
/// the ground, units: the two bodies' radii (§16.2) [design].
pub const CLOSE_SLACK: f32 = 64.0;
/// The melee sequence's limits, game seconds (§16.2) [design]: the longest run at a mark,
/// the longest a swing is waited for.
pub const APPROACH_LIMIT: f32 = 2.5;
pub const SWING_LIMIT: f32 = 2.0;
/// Where a teleport looks for the ground: from this far above the spot to this far
/// below, units [design].
const TELEPORT_UP: f32 = 150.0;
const TELEPORT_DOWN: f32 = 600.0;

/// How far from a melee mark's point the player is put, across the ground: half the
/// weapon's reach plus half [`CLOSE_SLACK`] (§16.2) [design].
pub fn stand_off(reach: f32) -> f32 {
    reach * 0.5 + CLOSE_SLACK * 0.5
}

/// The feet `off` units from `mark` across the ground, on the side of `feet` (on the
/// player's line to it), at the mark's height (the ground is looked for from there).
pub fn stand_beside(feet: [f32; 3], mark: [f32; 3], off: f32) -> [f32; 3] {
    let (dx, dy) = (feet[0] - mark[0], feet[1] - mark[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > 1e-3 { (dx / len, dy / len) } else { (1.0, 0.0) };
    [mark[0] + ux * off, mark[1] + uy * off, mark[2]]
}

/// Between two throws, game seconds (§16.3) [design].
pub const THROW_INTERVAL: f32 = 0.35;

/// A helper fires this long (game seconds) after appearing, and leaves this long after
/// appearing (§18.2) [design].
pub const HELPER_DRAW: f32 = 0.3;
pub const HELPER_STAY: f32 = 1.2;
/// A helper's eye above its feet, units: where its shot leaves from [design: a person's].
pub const HELPER_EYE: f32 = 110.0;
/// A mark's "x": its size as a share of the screen width, and its colour: the interface's
/// amber (`crate::interface::AMBER`) lifted [`MARK_LIFT`] of the way to white, so it
/// stands out on the amber grade [design: the owner's wishes, 2026-10-24, to match the HUD
/// and then to be brighter; it was a dark red before; owner 2026-10-25: smaller and thinner
/// than 0.019 with 22 % bars].
pub const MARK_SIZE: f32 = 0.013;
/// Each bar's thickness, a share of the "x"'s size [design].
const MARK_BAR: f32 = 10.0;
/// How far the marks' amber is lifted towards white (0 = the HUD's amber) [design].
const MARK_LIFT: f32 = 0.25;
const MARK_COLOUR: Color = Color::srgb(lift(crate::interface::AMBER[0]), lift(crate::interface::AMBER[1]), lift(crate::interface::AMBER[2]));

/// One channel of the interface's amber lifted [`MARK_LIFT`] of the way to white.
const fn lift(c: f32) -> f32 {
    c + (1.0 - c) * MARK_LIFT
}
/// The most marks drawn at once (more are kept but not drawn) [engineering choice].
const MAX_DRAWN: usize = 32;

/// VATS mode's state (§2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Off,
    /// V was pressed with the weapon away: it is being drawn, and the mode starts once it
    /// is out.
    Drawing,
    Marking,
    Firing,
}

/// What was pressed this frame, for the mode.
#[derive(Debug, Clone, Copy, Default)]
struct Keys {
    toggle: bool,
    mark: bool,
    unmark: bool,
    accept: bool,
}

/// Where a mark is and what it is on.
#[derive(Debug, Clone, Default)]
struct Spot {
    /// Its point now (game space): the part's capsule centre, followed while the actor
    /// moves; the last one seen once it has no volume.
    point: [f32; 3],
    name: String,
    part: String,
    /// The part's condition actor value (GetVATSValue Targeted Part).
    condition: Option<u16>,
    /// The part's base chance to hit, percent (§17.2).
    base: f32,
    /// The part's size against a torso's, for the range term (§17.2).
    size: f32,
    /// The player's to-hit perks for this mark, read when it was made (§17.3).
    perks: HitPerks,
    /// Marks made before it on the same actor and part (Concentrated Fire).
    earlier: u32,
    /// Its chance to hit now, percent (§17.2): drawn beside its "x", rolled on firing.
    chance: f32,
    /// Its last roll while firing (GetVATSValue Is Success).
    landed: Option<bool>,
}

/// The weapon VATS mode was entered with.
#[derive(Debug, Clone)]
struct InHand {
    id: FormId,
    reach: f32,
    style: AttackStyle,
    /// A melee weapon's own reach, units.
    melee_reach: f32,
    /// Its WEAP Sight FOV (0: no sights).
    sight_fov: f32,
    /// The player's skill with it, and the VATS settings of the hit chance (§17).
    skill: f32,
    hit: HitTuning,
}

/// A helper in the world (§18.2).
#[derive(Debug, Clone)]
struct ActiveHelper {
    who: Helper,
    reference: FormId,
    weapon: FormId,
    /// Where they stand (feet), and the mark they came for.
    feet: [f32; 3],
    target: FormId,
    part: Option<u8>,
    point: [f32; 3],
    /// Game seconds since they appeared, and whether they have fired.
    age: f32,
    fired: bool,
}

/// How the marks are being spent (§9, §16).
#[derive(Debug, Clone)]
enum Run {
    /// Shots, bursts or throws at fixed intervals.
    Volley(Volley),
    /// Close in and swing at each mark.
    Melee(MeleeRun),
}

impl Run {
    fn all_fired(&self) -> bool {
        match self {
            Run::Volley(v) => v.all_fired(),
            Run::Melee(m) => m.all_fired(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct VatsStats {
    pub entered: u32,
    pub marks: u32,
    pub shots: u32,
    pub ap_spent: f32,
    pub refused: u32,
    /// Rolls that hit and missed (§17.4).
    pub hits: u32,
    pub misses: u32,
    /// Helpers that came, and the AP Grim Reaper's Sprint gave back (§18).
    pub helpers: u32,
    pub ap_rewarded: f32,
}

#[derive(Resource, Default)]
pub struct VatsMode {
    pub phase: Phase,
    queue: MarkQueue,
    spots: Vec<Spot>,
    run: Option<Run>,
    weapon: Option<InHand>,
    /// Marks whose attack has started (their AP spent).
    started: usize,
    /// A melee sequence: the swing due this frame (`crate::combat::melee` taps Use), the
    /// mark being swung at, and the mark last teleported to.
    swing_now: bool,
    striking: Option<usize>,
    closing: Option<usize>,
    /// Real seconds (`Time<Real>`) when marking began, and when the look began to go.
    entered_at: Option<f64>,
    left_at: Option<f64>,
    now: f64,
    keys: Keys,
    /// The point the shot `crate::combat` fires this frame aims at.
    shot: Option<[f32; 3]>,
    /// The mark shot last, for the scripts (GetVATSValue).
    shooting: Option<usize>,
    /// This stay's shots and AP spent.
    fired: u32,
    spent: f32,
    /// Real seconds when Block went down while marking (a tap unmarks, §11.5).
    block_at: Option<f64>,
    /// Real seconds when the weapon began to be drawn for the mode ([`DRAW_LIMIT`]).
    drawing_at: Option<f64>,
    /// The mark the third-person sights rule looks at while firing (the one being fired
    /// or the next), and whether that rule has the sights up (`None`: not decided yet).
    aim_mark: usize,
    sights_up: Option<bool>,
    /// The hit rolls' generator (seeded, so a run repeats) and this stay's hits and misses.
    rng: Option<SplitMix>,
    hits: u32,
    misses: u32,
    /// The roll of the melee swing being made (§17.4).
    blow_lands: bool,
    /// The helper in the world, if one came (§18.2).
    helper: Option<ActiveHelper>,
    /// The player's kills when the marks began to be fired, and the same once the mode
    /// ended after firing, for Grim Reaper's Sprint (§18.3).
    kills_at_fire: Option<f64>,
    reward_due: Option<f64>,
    /// The AP regeneration factor last applied (Nerves of Steel, §18.4), for the log.
    regen_mult: Option<f64>,
    /// SOUN FormIDs by Editor ID, found on first use.
    sounds: Option<HashMap<String, FormId>>,
    /// Every body part's chance on the actor under or nearest the crosshair, before it is
    /// marked (§17.5), and the parts list it is built from (kept for its room).
    preview: Preview,
    preview_list: Vec<Option<u8>>,
    pub stats: VatsStats,
}

impl VatsMode {
    /// Marking or firing.
    pub fn active(&self) -> bool {
        matches!(self.phase, Phase::Marking | Phase::Firing)
    }

    /// The mode has Use, Block and Activate for itself (§11.5).
    pub fn holds_input(&self) -> bool {
        self.phase != Phase::Off
    }

    /// The actors marked now (references).
    pub fn marked(&self) -> Vec<u32> {
        self.queue.marks().iter().map(|m| m.target.0).collect()
    }

    /// The shot `crate::combat` is to fire now: the point it goes straight at.
    pub fn take_shot(&mut self) -> Option<[f32; 3]> {
        self.shot.take()
    }

    /// A melee mark's swing is due now: `crate::combat::melee` taps Use once (§16.2).
    pub fn take_swing(&mut self) -> bool {
        std::mem::take(&mut self.swing_now)
    }

    /// The melee mark being swung at: its actor, its point now, how close across the
    /// ground it must be for the blow to land on it (the weapon's reach plus
    /// [`CLOSE_SLACK`]) (§16.2), and whether the hit roll lets it land (§17.4).
    pub fn melee_mark(&self) -> Option<(u32, [f32; 3], f32, bool)> {
        if self.phase != Phase::Firing {
            return None;
        }
        let w = self.weapon.as_ref().filter(|w| w.style == AttackStyle::Melee)?;
        let i = self.striking?;
        Some((self.queue.marks().get(i)?.target.0, self.spots.get(i)?.point, w.melee_reach + CLOSE_SLACK, self.blow_lands))
    }

    /// Where the view's depth of field is focused (image-modifiers.md §8.4): the mark
    /// being shot while firing, the last marked while marking; none without a mark.
    pub fn focus(&self) -> Option<[f32; 3]> {
        match self.phase {
            Phase::Firing => self.shooting.and_then(|i| self.spots.get(i)).map(|s| s.point),
            Phase::Marking => self.spots.last().map(|s| s.point),
            _ => None,
        }
    }

    /// The AP the marks not yet started hold.
    pub fn reserved(&self) -> f32 {
        if !self.active() {
            return 0.0;
        }
        self.queue.cost * self.queue.len().saturating_sub(self.started) as f32
    }

    /// The clock's speed the mode asks for (`script::time_multiplier` multiplies it in).
    pub fn time_scale(&self) -> f32 {
        match (self.phase, self.entered_at) {
            (Phase::Marking, Some(t)) => time_scale_at((self.now - t) as f32),
            (Phase::Firing, _) => FIRE_SCALE,
            _ => 1.0,
        }
    }

    /// The look for the image-space pass (§5): the swipe's edge (a share of the screen
    /// width; graded to its left), the band on the edge, the wash over the screen, the
    /// grade's strength.
    pub fn look(&self) -> [f32; 4] {
        let since = |t: Option<f64>| t.map(|t| (self.now - t) as f32);
        look_at(self.active(), since(self.entered_at), since(self.left_at))
    }

    /// The marks to draw (§8, §17.5): one "x" and one chance for each actor and part with
    /// marks not yet fired, in the order first marked. Marks on the same part share a point,
    /// so drawing each would stack them; the chance shown is the newest mark's, the one
    /// Concentrated Fire has raised the most.
    fn drawn(&self) -> Vec<([f32; 3], f32)> {
        let skip = if self.phase == Phase::Firing { self.started } else { 0 };
        let mut seen: Vec<(u32, Option<u8>)> = Vec::new();
        let mut out: Vec<([f32; 3], f32)> = Vec::new();
        for (m, s) in self.queue.marks().iter().zip(&self.spots).skip(skip).take(if self.active() { usize::MAX } else { 0 }) {
            let key = (m.target.0, m.part);
            match seen.iter().position(|k| *k == key) {
                Some(i) => out[i].1 = s.chance,
                None => {
                    seen.push(key);
                    out.push((s.point, s.chance));
                }
            }
        }
        out
    }

    /// Roll a hit at `chance` percent (§17.4); the test aid `NVF_PLAY_TEST_VATS_ROLL`
    /// (`hit` or `miss`) decides instead. The roll, 0–100, and whether it hit.
    fn roll(&mut self, chance: f32) -> (f32, bool) {
        let u = self.rng.get_or_insert(SplitMix(0x7A75_4D0D)).next_f32();
        let hit = forced_roll().unwrap_or_else(|| rolled_hit(chance, u));
        if hit {
            self.hits += 1;
            self.stats.hits += 1;
        } else {
            self.misses += 1;
            self.stats.misses += 1;
        }
        (u * 100.0, hit)
    }

    /// Two numbers in [0, 1) for where a miss goes.
    fn two(&mut self) -> (f32, f32) {
        let r = self.rng.get_or_insert(SplitMix(0x7A75_4D0D));
        (r.next_f32(), r.next_f32())
    }

    fn sound(&mut self, sfx: Option<&Sfx>, edid: &str) -> Option<FormId> {
        let sfx = sfx?;
        let by = self.sounds.get_or_insert_with(|| sfx.forms.sounds.iter().filter_map(|(&id, s)| s.edid.clone().map(|e| (e, id))).collect());
        let found = by.get(edid).copied();
        if found.is_none() {
            debug!("vats mode: no sound {edid}");
        }
        found
    }
}

/// The test aid `NVF_PLAY_TEST_VATS_ROLL`: `hit` or `miss` makes every roll come out so
/// (vats-mode.md §13), for checks about something other than the chance.
fn forced_roll() -> Option<bool> {
    static FORCED: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| match std::env::var("NVF_PLAY_TEST_VATS_ROLL").ok()?.trim() {
        "hit" => Some(true),
        "miss" => Some(false),
        other => {
            warn!("vats mode: NVF_PLAY_TEST_VATS_ROLL={other}: not hit or miss, rolls are rolled");
            None
        }
    })
}

/// A mark's chance to hit now, percent (§17.2): `cone` is the gun's cone in VATS
/// (`Combat::vats_cone`); a melee weapon has none.
fn chance_of(spot: &Spot, w: &InHand, eye: [f32; 3], cone: Option<f32>) -> f32 {
    let distance = crate::combat::distance(eye, spot.point);
    let cone = if w.style == AttackStyle::Melee { None } else { Some(cone.unwrap_or(0.0)) };
    let raw = raw_chance(&HitInputs { base: spot.base, size: spot.size, skill: w.skill, distance, cone_deg: cone }, &w.hit);
    final_chance(raw, |c| spot.perks.apply(c as f64) as f32, spot.perks.concentrated_fire.then_some(spot.earlier), &w.hit)
}

/// Every mark's chance again (the marks and the player move).
fn rechance(v: &mut VatsMode, combat: &Combat, eye: [f32; 3]) {
    let Some(w) = v.weapon.as_ref() else { return };
    let cone = combat.vats_cone;
    for s in &mut v.spots {
        s.chance = chance_of(s, w, eye, cone);
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The clock's speed `since` real seconds after entering: from 1 down to [`MARK_SCALE`]
/// over the swipe.
pub fn time_scale_at(since: f32) -> f32 {
    1.0 - (1.0 - MARK_SCALE) * smoothstep(since / ENTER_SWIPE)
}

/// The look ([`VatsMode::look`]) while the mode is on (`on`), `entered` real seconds after
/// entering, or `left` seconds after it went off.
pub fn look_at(on: bool, entered: Option<f32>, left: Option<f32>) -> [f32; 4] {
    match (on, entered, left) {
        (true, Some(t), _) if t < ENTER_SWIPE => {
            // The edge crosses from the left edge until the band behind it has left too.
            let edge = t / ENTER_SWIPE * (1.0 + BAND_WIDTH);
            [edge, BAND_STRENGTH, 0.0, 1.0]
        }
        (true, Some(t), _) => [2.0, 0.0, WASH_START * (1.0 - (t - ENTER_SWIPE) / ENTER_WASH).max(0.0), 1.0],
        (true, None, _) => [2.0, 0.0, 0.0, 1.0],
        (false, _, Some(t)) if t < LEAVE_FADE => [2.0, 0.0, 0.0, 1.0 - t / LEAVE_FADE],
        _ => [0.0; 4],
    }
}

/// The band's width behind the edge, a share of the screen width (the shader's
/// `VATS_BAND`) [design].
pub const BAND_WIDTH: f32 = 0.22;

/// The player's Action Points: (current, maximum, damage); `None` without the script
/// runtime. The maximum counts the temporary modifiers, as the HUD's bar does.
fn player_ap(script: Option<&mut ScriptState>, sim: &mut SimState) -> Option<(f32, f32, f32)> {
    let rt = script.and_then(|s| s.rt.as_mut())?;
    with_world(rt, sim, |rt| {
        let [base, modifier, damage] = fnv_game::functions::actor_value_parts(&mut rt.game, FormId::PLAYER_REF, ACTION_POINTS).ok()?;
        let max = base + modifier + fnv_game::functions::actor_value_temporary(&rt.game, FormId::PLAYER_REF, ACTION_POINTS);
        Some(((max - damage) as f32, max as f32, damage as f32))
    })
}

/// The sound side of [`update`].
#[derive(SystemParam)]
struct Sounds<'w> {
    sfx: Option<Res<'w, Sfx>>,
    play: MessageWriter<'w, PlaySound>,
    loops: MessageWriter<'w, LoopSound>,
    /// A helper's shot (§18.2).
    fire: MessageWriter<'w, ActorFire>,
}

impl Sounds<'_> {
    fn play(&mut self, vats: &mut VatsMode, edid: &str) {
        if let Some(sound) = vats.sound(self.sfx.as_deref(), edid) {
            self.play.write(PlaySound { sound, at: None });
        }
    }

    fn looping(&mut self, vats: &mut VatsMode, on: bool) {
        if let Some(sound) = vats.sound(self.sfx.as_deref(), "UIVATSLoop") {
            self.loops.write(LoopSound { sound, on });
        }
    }
}

/// The keys the mode takes (after the frame's input, test aids and script locks), and
/// what it holds back from the rest of the game (§11.5): the trigger, reloading and Ammo
/// Swap while it is on. Aiming down the sights is the player's while marking (a tap of
/// Block unmarks instead) and, in first person, while firing; in third person the
/// marked shots raise a gun's sights by the mark's distance. V with the weapon away
/// draws it.
#[allow(clippy::too_many_arguments)]
fn read_keys(
    input: ControlInput,
    real: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    locks: Res<ControlLocks>,
    mut vats: ResMut<VatsMode>,
    mut control: ResMut<PlayerControl>,
    player: Res<Player>,
    mut sounds: Sounds,
) {
    let playing = *state.get() == PlayState::Playing && !input.suppressed();
    let pressed = |c: Control| playing && input.just_pressed(c);
    let now = real.elapsed_secs_f64();
    vats.keys = Keys { toggle: pressed(Control::Vats), mark: pressed(Control::Use), unmark: false, accept: pressed(Control::Activate) };
    // Any other action ends the mode (§11.8), here before activation, so an E with
    // nothing marked reaches what is under the crosshair.
    if let Some(c) = leaving_press(vats.phase, vats.queue.len(), pressed) {
        vats.now = now;
        vats.keys = Keys::default();
        end(&mut vats, &format!("{} pressed", crate::bindings::label(c)), &mut sounds);
    }
    // Block while marking: a tap unmarks, a hold aims (the aim waits out the tap).
    if vats.phase == Phase::Marking {
        if pressed(Control::Block) {
            vats.block_at = Some(now);
        }
        let held_for = vats.block_at.map(|t| (now - t) as f32);
        let tap = held_for.is_some_and(|h| h < UNMARK_TAP);
        if tap && playing && input.just_released(Control::Block) {
            vats.keys.unmark = true;
        }
        if !(playing && input.pressed(Control::Block)) {
            vats.block_at = None;
        }
        if tap {
            control.aiming = false;
        }
    } else {
        vats.block_at = None;
    }
    if vats.phase == Phase::Off && vats.keys.toggle && !control.weapon_out && !locks.fighting {
        control.weapon_out = true;
        vats.phase = Phase::Drawing;
        vats.drawing_at = Some(now);
        vats.keys.toggle = false;
        info!("vats mode: drawing the weapon first");
    }
    if vats.holds_input() {
        control.trigger = false;
        control.reload = false;
        control.ammo_swap = false;
    }
    match vats.phase {
        Phase::Drawing => control.aiming = false,
        Phase::Firing => {
            let gun = vats.weapon.as_ref().filter(|w| matches!(w.style, AttackStyle::Single | AttackStyle::Burst { .. }));
            match gun.map(|w| w.sight_fov) {
                // The body's clips show it: a gun's sights up for a far mark, from the hip
                // for a near one, as the NPCs fire (`crate::combat::aims_down_sights`).
                Some(fov) if control.third_person_view() => {
                    let away = vats.spots.get(vats.aim_mark).map(|s| crate::combat::distance(player.eye(), s.point));
                    let up = away.is_some_and(|d| crate::combat::aims_down_sights(vats.sights_up == Some(true), d, fov));
                    if vats.sights_up != Some(up) {
                        vats.sights_up = Some(up);
                        let how = if up { "aims down the sights (beyond 20 m)" } else { "fires from the hip (within 20 m)" };
                        info!("vats mode: third person, mark {} {:.0} units away: {how}", vats.aim_mark + 1, away.unwrap_or(0.0));
                    }
                    control.aiming = up;
                }
                // First person: the player's own aim (a scope stays usable).
                Some(_) => {}
                None => control.aiming = false,
            }
        }
        _ => {}
    }
}

/// The mode's work for the frame; before `crate::combat::fire`, which fires the shot asked
/// for ([`VatsMode::take_shot`]).
#[allow(clippy::too_many_arguments)]
fn update(
    (real, time): (Res<Time<Real>>, Res<Time>),
    state: Res<State<PlayState>>,
    mut vats: ResMut<VatsMode>,
    (combat, anim, parts): (Option<Res<Combat>>, Res<WeaponAnim>, Res<ActorParts>),
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
    control: Res<PlayerControl>,
    locks: Res<ControlLocks>,
    mut hud: ResMut<HudMessage>,
    mut sounds: Sounds,
) {
    let v = &mut *vats;
    v.now = real.elapsed_secs_f64();
    v.shot = None;
    let keys = std::mem::take(&mut v.keys);
    let playing = *state.get() == PlayState::Playing;
    let dead = script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.is_dead(FormId::PLAYER_REF));
    // A conversation (a forced greeting too) or a script's menu, caught before the state
    // follows it (§11.8).
    let held = script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.in_menu());
    let Some(combat) = combat else { return };
    // What ends the mode whatever it is doing (§9.3, §11.6, §11.8).
    let stop = if !playing {
        Some("a menu opened")
    } else if dead {
        Some("the player died")
    } else if held {
        Some("a conversation or a script's menu began")
    } else if v.phase == Phase::Drawing && !control.weapon_out {
        Some("the weapon was put away")
    } else if v.phase == Phase::Drawing && v.drawing_at.is_some_and(|t| v.now - t > f64::from(DRAW_LIMIT)) {
        Some("the weapon did not come out")
    } else if locks.fighting {
        Some("a script disabled fighting")
    } else if v.active()
        && !v.run.as_ref().is_some_and(Run::all_fired)
        && v.weapon.as_ref().map(|w| w.id) != combat.player_weapon().filter(|_| control.weapon_out)
    {
        // (The last throw spends the last one carried: then the weapon goes, §16.3.)
        Some("the weapon left the hand")
    } else {
        None
    };
    match (v.phase, stop) {
        (Phase::Off, _) => {
            if let Some(m) = regenerate(script.as_deref_mut(), &mut sim, combat.rules.ap_restore_rate, time.delta_secs()) {
                if v.regen_mult != Some(m) && (v.regen_mult.is_some() || m != 1.0) {
                    info!("vats mode: AP regenerates at {m:.2} x (the Action Point Regen perks)");
                }
                v.regen_mult = Some(m);
            }
            if keys.toggle {
                try_enter(v, &combat, script.as_deref_mut(), &mut sim, phys.world.params.gravity, playing && !dead && !held && !locks.fighting, &mut hud, &mut sounds);
            }
        }
        (_, Some(why)) => end(v, why, &mut sounds),
        (Phase::Drawing, None) => {
            if keys.toggle {
                v.phase = Phase::Off;
                info!("vats mode: entering cancelled");
            } else if anim.ready && control.weapon_out {
                v.phase = Phase::Off;
                try_enter(v, &combat, script.as_deref_mut(), &mut sim, phys.world.params.gravity, true, &mut hud, &mut sounds);
            }
        }
        (Phase::Marking, None) => {
            follow(v, &phys);
            rechance(v, &combat, player.eye());
            preview(v, &combat, &parts, script.as_deref_mut(), &mut sim, &mut phys, &player, &control);
            if keys.toggle {
                end(v, "V pressed again", &mut sounds);
            } else if keys.unmark {
                match v.queue.pop() {
                    Some(m) => {
                        let s = v.spots.pop();
                        info!("vats mode: unmarked {} ({}), {} marks left", m.target, s.map_or_else(String::new, |s| s.part), v.queue.len());
                        sounds.play(v, "UIVATSMove");
                    }
                    None => end(v, "right click with no marks", &mut sounds),
                }
            } else if keys.accept {
                // (With nothing marked, E has already ended the mode in `read_keys`.)
                if !v.queue.is_empty() {
                    v.phase = Phase::Firing;
                    (v.aim_mark, v.sights_up) = (0, None);
                    let n = v.queue.len();
                    let style = v.weapon.as_ref().map_or(AttackStyle::Single, |w| w.style);
                    let (run, what) = match style {
                        AttackStyle::Single => (Run::Volley(Volley::new(n, SHOT_INTERVAL, AFTER_LAST)), "shots"),
                        AttackStyle::Burst { shots, interval } => (Run::Volley(Volley::bursts(n, shots, SHOT_INTERVAL, interval, AFTER_LAST)), "bursts"),
                        AttackStyle::Thrown => (Run::Volley(Volley::new(n, THROW_INTERVAL, AFTER_LAST)), "throws"),
                        AttackStyle::Melee => (Run::Melee(MeleeRun::new(n, MeleeLimits { approach: APPROACH_LIMIT, swing: SWING_LIMIT, after: AFTER_LAST })), "swings"),
                    };
                    v.run = Some(run);
                    v.kills_at_fire = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| rt.game.player_kill_count());
                    sounds.looping(v, false);
                    sounds.play(v, "UIVATSReady");
                    info!("vats mode: firing {n} marked {what}");
                }
            } else if keys.mark {
                mark(v, &combat, &parts, script.as_deref_mut(), &mut sim, &mut phys, &player, &control, &mut hud, &mut sounds);
            }
        }
        (Phase::Firing, None) => {
            follow(v, &phys);
            rechance(v, &combat, player.eye());
            let dt = time.delta_secs();
            match v.run.as_mut() {
                Some(Run::Volley(vo)) => match vo.step(dt) {
                    Some(VolleyEvent::Shot { mark, round }) => shoot(v, mark, round, script.as_deref_mut(), &mut sim, &mut phys, &mut player, &control, &mut sounds),
                    Some(VolleyEvent::Done) => end(v, "all shots fired", &mut sounds),
                    None => {}
                },
                Some(Run::Melee(_)) => melee_step(v, dt, &combat, script.as_deref_mut(), &mut sim, &mut phys, &mut player, &control, &mut sounds),
                None => end(v, "nothing to fire", &mut sounds),
            }
        }
    }
    tick_helper(v, time.delta_secs(), script.as_deref_mut(), &mut sim, &phys, &mut sounds);
    if let Some(before) = v.reward_due.take() {
        kill_reward(v, before, script.as_deref_mut(), &mut sim);
    }
    tell_scripts(v, script.as_deref_mut(), &player);
}

/// Spent AP comes back outside the mode (§7.2), faster with the Action Point Regen perks
/// (Nerves of Steel, §18.4). The factor used, when any AP was spent.
fn regenerate(script: Option<&mut ScriptState>, sim: &mut SimState, rate: f32, dt: f32) -> Option<f64> {
    let rt = script.and_then(|s| s.rt.as_mut())?;
    if dt <= 0.0 {
        return None;
    }
    with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        let [base, modifier, damage] = fnv_game::functions::actor_value_parts(g, FormId::PLAYER_REF, ACTION_POINTS).ok()?;
        if damage <= 0.0 {
            return None;
        }
        let mult = g.ap_regen_mult();
        let max = base + modifier + fnv_game::functions::actor_value_temporary(g, FormId::PLAYER_REF, ACTION_POINTS);
        let left = restored_damage(damage as f32, max as f32, rate * mult as f32, dt) as f64;
        fnv_game::functions::restore_actor_value_damage(g, FormId::PLAYER_REF, ACTION_POINTS, damage - left);
        Some(mult)
    })
}

/// V: start marking if the weapon and the AP allow (§3).
#[allow(clippy::too_many_arguments)]
fn try_enter(v: &mut VatsMode, combat: &Combat, mut script: Option<&mut ScriptState>, sim: &mut SimState, gravity: f32, allowed: bool, hud: &mut HudMessage, sounds: &mut Sounds) {
    let ap = player_ap(script.as_deref_mut(), sim);
    let refused = |v: &mut VatsMode, why: &str, hud: &mut HudMessage, sounds: &mut Sounds| {
        info!("vats mode: refused: {why}");
        hud.show(why.to_string());
        v.stats.refused += 1;
        sounds.play(v, "UIVATSEnterFail");
    };
    if !allowed {
        return refused(v, "Fighting is not possible now.", hud, sounds);
    }
    let mut w = match combat.vats_weapon(sim, gravity) {
        Ok(w) => w,
        Err(why) => return refused(v, why, hud, sounds),
    };
    // The Action Point Cost perks (docs/spec-notes/perk-entry-points.md §2).
    let (cost, by) = crate::combat::player_weapon_perk(script.as_deref_mut(), sim, fnv_game::perks::ACTION_POINT_COST, w.id, w.cost);
    if !by.is_empty() {
        info!("vats mode: perks change the AP cost {:.1} -> {:.1} ({})", w.cost, cost.max(0.0), by.join(", "));
        w.cost = cost.max(0.0);
    }
    let ap_now = ap.map_or(f32::INFINITY, |a| a.0);
    v.queue = MarkQueue::new(w.cost, w.rounds_per_shot, w.rounds);
    // Not even one mark: no AP for it, or (a burst) not a whole burst loaded.
    if let Err(r) = v.queue.check(ap_now) {
        return refused(v, r.message(), hud, sounds);
    }
    v.phase = Phase::Marking;
    v.spots.clear();
    v.preview = Preview::default();
    v.run = None;
    v.fired = 0;
    v.started = 0;
    v.spent = 0.0;
    v.entered_at = Some(v.now);
    v.left_at = None;
    v.stats.entered += 1;
    let ap_text = ap.map_or_else(|| "unlimited".to_string(), |a| format!("{:.0}", a.0));
    let most = v.queue.most(ap_now);
    // A single shot's line is as it was; the other kinds name themselves (§16).
    match (w.style, w.style.label()) {
        (_, None) => info!("vats mode: on ({}, {:.0} AP a shot, {} rounds loaded, {ap_text} AP; at most {most} marks)", w.edid, w.cost, w.rounds),
        (AttackStyle::Melee, Some(kind)) => info!("vats mode: on ({}, {kind}, {:.0} AP a mark, reach {:.0}, {ap_text} AP; at most {most} marks)", w.edid, w.cost, w.melee_reach),
        (AttackStyle::Thrown, Some(kind)) => info!("vats mode: on ({}, {kind}, {:.0} AP a mark, {} carried, {ap_text} AP; at most {most} marks)", w.edid, w.cost, w.rounds),
        (_, Some(kind)) => info!("vats mode: on ({}, {kind}, {:.0} AP a mark, {} rounds loaded, {ap_text} AP; at most {most} marks)", w.edid, w.cost, w.rounds),
    }
    let sight_fov = combat.sight_fov().filter(|&(id, _)| id == w.id).map_or(0.0, |(_, f)| f);
    let skill = combat.player_weapon_skill(script, sim);
    v.weapon = Some(InHand { id: w.id, reach: w.reach, style: w.style, melee_reach: w.melee_reach, sight_fov, skill, hit: combat.rules.vats_hit });
    (v.hits, v.misses) = (0, 0);
    sounds.play(v, "UIVATSEnter");
    sounds.looping(v, true);
}

/// Leave the mode (§2): marks dropped, reservations freed, the look fading out.
fn end(v: &mut VatsMode, why: &str, sounds: &mut Sounds) {
    let was = v.phase;
    v.phase = Phase::Off;
    if was == Phase::Firing {
        v.reward_due = v.kills_at_fire.take();
    }
    v.run = None;
    v.shot = None;
    v.shooting = None;
    v.swing_now = false;
    v.striking = None;
    v.closing = None;
    if matches!(was, Phase::Marking | Phase::Firing) {
        if was == Phase::Marking {
            sounds.looping(v, false);
        }
        sounds.play(v, "UIVATSExit");
        v.left_at = Some(v.now);
        info!("vats mode: off ({} shots fired, {:.0} AP spent; {why})", v.fired, v.spent);
        if v.hits + v.misses > 0 {
            info!("vats mode: rolls: {} hit, {} missed", v.hits, v.misses);
        }
    } else if was == Phase::Drawing {
        info!("vats mode: entering cancelled ({why})");
    }
    v.entered_at = None;
    v.queue = MarkQueue::default();
    v.spots.clear();
    v.preview = Preview::default();
    v.blow_lands = false;
}

/// The point to aim at on an actor: its part's capsule centre, the whole capsule's aim
/// point without parts.
fn point_of(phys: &PhysState, actor: u32, part: Option<u8>) -> Option<[f32; 3]> {
    match part {
        Some(p) => phys.world.actor_part_center(actor, p).or_else(|| phys.world.actor_aim_point(actor)),
        None => phys.world.actor_aim_point(actor),
    }
}

/// The marks follow their actors.
fn follow(v: &mut VatsMode, phys: &PhysState) {
    let marks: Vec<_> = v.queue.marks().to_vec();
    for (m, s) in marks.iter().zip(v.spots.iter_mut()) {
        if let Some(p) = point_of(phys, m.target.0, m.part) {
            s.point = p;
        }
    }
}

/// Why an actor cannot be marked (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bar {
    Dead,
    /// Cloaked, and the player has no Enhanced Sensors.
    Cloaked,
    /// No VATS Melee, with a melee weapon in hand.
    NoVatsMelee,
}

/// Whether `actor` may be marked (§6): `Ok(true)` when it is cloaked and Enhanced Sensors
/// lets it be marked all the same.
fn barred(v: &VatsMode, mut script: Option<&mut ScriptState>, sim: &mut SimState, actor: u32) -> Result<bool, Bar> {
    if crate::combat::is_dead(script.as_deref(), actor) {
        return Err(Bar::Dead);
    }
    // A cloaked actor (Invisibility or Chameleon at least 1: a Stealth Boy, a Nightkin's
    // cloak) is not marked unless the player has Enhanced Sensors, whose entry point 69 Has
    // Improved Detection gives "VATS targeting of cloaked enemies" [documented: GECK wiki
    // Perk entry points; hypothesis: the actor values are what "cloaked" reads]
    // (perk-entry-points.md §13).
    let mut cloaked = false;
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        let (c, sensors) = crate::script::with_world(rt, sim, |rt| {
            let g = &mut rt.game;
            let av = |g: &mut fnv_game::Game, v: u16| fnv_game::functions::actor_value_capped(g, FormId(actor), v);
            let cloaked = av(g, fnv_game::detection::INVISIBILITY) >= 1.0 || av(g, fnv_game::detection::CHAMELEON) >= 1.0;
            (cloaked, cloaked && g.player_has_entry(fnv_game::perks::HAS_IMPROVED_DETECTION))
        });
        if c && !sensors {
            return Err(Bar::Cloaked);
        }
        cloaked = c;
    }
    // "The player cannot go into VATS with this NPC if he has a melee weapon equipped"
    // [documented: GECK wiki Category:NPC, No VATS Melee] (actor-flags.md §1).
    let melee = v.weapon.as_ref().is_some_and(|w| w.style == AttackStyle::Melee);
    // The base from the simulation: the game's world is not attached outside its turn.
    let flagged = melee
        && sim.base_of(actor).is_some_and(|b| script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| { let (seed, at) = sim.pick(&rt.game, FormId(actor)); rt.game.data.no_vats_melee(b, seed, at) }));
    if flagged {
        return Err(Bar::NoVatsMelee);
    }
    Ok(cloaked)
}

/// What a mark on `actor`'s `part` at `point` is (§17.2, §17.3): the names, the part's
/// base chance and size, the to-hit perks seeing it as the shot, and its chance now; with
/// the perks' names for the log. A mark and the preview of a part (§17.5) both come from
/// here, so the preview's figure is the one the mark then shows.
#[allow(clippy::too_many_arguments)]
fn spot_on(v: &VatsMode, combat: &Combat, parts: &ActorParts, script: Option<&mut ScriptState>, sim: &mut SimState, player: &Player, actor: u32, part: Option<u8>, point: [f32; 3], earlier: u32) -> (Spot, Vec<String>) {
    let bp = combat.vats_part(parts, actor, part);
    let part_name = match (part, bp.as_ref().and_then(|b| b.name.clone())) {
        (Some(crate::hitvol::WEAPON_PART), _) => "its weapon".to_string(),
        (_, Some(n)) => n,
        (Some(p), None) => format!("part {p}"),
        (None, None) => "whole".to_string(),
    };
    let condition = bp.as_ref().and_then(|b| b.actor_value.as_deref()).and_then(fnv_game::combat::limb_condition);
    let name = sim.base_of(actor).and_then(|b| sim.name(b).map(str::to_string)).unwrap_or_else(|| "?".into());
    // The part's base chance (§17.2): its BPTD To Hit Chance; an actor without
    // parts, or the weapon it holds, our own.
    let (base, size) = match (part, bp.as_ref()) {
        (Some(WEAPON_PART), _) => (HELD_WEAPON_TO_HIT, SIZE_HELD_WEAPON),
        (_, Some(b)) => (b.to_hit_chance as f32, fnv_combat::vats::part_size(b.actor_value.as_deref())),
        (_, None) => (WHOLE_TO_HIT, SIZE_TORSO),
    };
    // The to-hit perks see this mark as the shot (§17.3).
    let shot = v.weapon.as_ref().map(|w| fnv_game::vats::VatsShot {
        weapon: w.id,
        target: FormId(actor),
        distance: crate::combat::distance(player.eye(), point) as f64,
        condition,
        success: false,
        height: (point[2] - player.feet[2]) as f64,
    });
    let (perks, names) = match (shot, script.and_then(|s| s.rt.as_mut())) {
        (Some(shot), Some(rt)) => with_world(rt, sim, |rt| {
            let perks = rt.game.vats_hit_perks(shot);
            let names: Vec<String> = perks.ops.iter().map(|(p, _)| rt.game.data.name(*p)).collect();
            (perks, names)
        }),
        _ => (HitPerks::default(), Vec::new()),
    };
    let mut spot = Spot { point, name, part: part_name, condition, base, size, perks, earlier, chance: 0.0, landed: None };
    if let Some(w) = v.weapon.as_ref() {
        spot.chance = chance_of(&spot, w, player.eye(), combat.vats_cone);
    }
    (spot, names)
}

/// One body part's chance before it is marked (§17.5).
#[derive(Debug, Clone, Default)]
struct PreviewPart {
    /// The part (`None`: the whole actor, one capsule).
    part: Option<u8>,
    /// What a mark on it would be: its point now and its chance now.
    spot: Spot,
    /// Drawn this frame: in sight and not marked already.
    shown: bool,
    /// The part the crosshair is on: a click marks it.
    under: bool,
}

/// The actor whose parts' chances are shown before marking (§17.5).
#[derive(Debug, Clone, Default)]
struct Preview {
    actor: Option<u32>,
    parts: Vec<PreviewPart>,
    /// Real seconds when the parts (and their perks) were last worked out.
    built_at: f64,
}

/// The preview's actor, when the crosshair is not on one, is the nearest valid actor
/// within this angle of the crosshair's line, degrees [design].
pub const PREVIEW_CONE: f32 = 10.0;
/// The preview's parts, their names and their perks are worked out again this often,
/// real seconds (their points and chances every frame) [design: the perks' conditions
/// may read the distance; four times a second keeps them as fresh as the eye can tell].
const PREVIEW_REBUILD: f64 = 0.25;
/// The most preview labels drawn at once [engineering choice: a skeleton's parts and its
/// weapon].
const MAX_PREVIEW: usize = 24;

/// The shots' filter: through what the character passes and the actors' walking
/// capsules (their hit volumes are their bodies).
fn shot_filter(phys: &PhysState) -> Filter {
    let mut filter = Filter { exclude_layers: layers::shot_ignores(), ..Default::default() };
    filter.exclude.extend(phys.npcs.values().map(|b| b.ch.key()));
    filter
}

/// The actor to preview (§17.5) and, when the crosshair is on it, the part it is on: the
/// one under the crosshair, else the nearest living actor with a hit volume within
/// [`PREVIEW_CONE`] of the crosshair's line, in reach and in sight.
fn preview_target(phys: &mut PhysState, script: Option<&ScriptState>, from: [f32; 3], dir: [f32; 3], reach: f32, filter: &Filter) -> Option<(u32, Option<Option<u8>>)> {
    if let Some(h) = phys.ray(from, dir, reach, filter.clone()) {
        if let Owner::Actor { form_id, part, .. } = h.owner {
            if !crate::combat::is_dead(script, form_id) {
                return Some((form_id, Some((part != fnv_phys::actors::WHOLE).then_some(part))));
            }
        }
    }
    let cos_cone = PREVIEW_CONE.to_radians().cos();
    let mut best: Option<(f32, u32, [f32; 3])> = None;
    for actor in phys.world.actor_volumes() {
        if actor == FormId::PLAYER_REF.0 {
            continue;
        }
        let Some(p) = phys.world.actor_aim_point(actor) else { continue };
        let d = [p[0] - from[0], p[1] - from[1], p[2] - from[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if !(1.0..=reach).contains(&len) {
            continue;
        }
        let cos = (d[0] * dir[0] + d[1] * dir[1] + d[2] * dir[2]) / len;
        if cos < cos_cone || best.is_some_and(|b| cos <= b.0) || crate::combat::is_dead(script, actor) {
            continue;
        }
        best = Some((cos, actor, p));
    }
    let (_, actor, p) = best?;
    in_sight(phys, from, p, filter).then_some((actor, None))
}

/// Whether the line from `from` to `to` meets no world before it: an actor's volume in
/// the way does not hide it [design].
fn in_sight(phys: &mut PhysState, from: [f32; 3], to: [f32; 3], filter: &Filter) -> bool {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1.0 {
        return true;
    }
    let dir = [d[0] / len, d[1] / len, d[2] / len];
    match phys.ray(from, dir, len, filter.clone()) {
        None => true,
        Some(h) => matches!(h.owner, Owner::Actor { .. }) || h.distance >= len - 2.0,
    }
}

/// The parts an actor's preview shows: each body part its hit volume has a capsule for
/// (the weapon it holds too), in the body part data's order; the whole actor with one
/// capsule.
fn parts_of(phys: &PhysState, parts: &ActorParts, actor: u32, out: &mut Vec<Option<u8>>) {
    out.clear();
    if phys.world.actor_part_count(actor) > 0 {
        if let Some(posed) = parts.by_ref.get(&actor) {
            for s in &posed.segments {
                if !out.contains(&Some(s.part)) && phys.world.actor_part_center(actor, s.part).is_some() {
                    out.push(Some(s.part));
                }
            }
        }
    }
    if out.is_empty() {
        out.push(None);
    }
}

/// While marking, every body part's chance on the previewed actor (§17.5): the actor and
/// its parts' spots are worked out again when the actor changes and every
/// [`PREVIEW_REBUILD`]; their points, chances and sight every frame.
#[allow(clippy::too_many_arguments)]
fn preview(v: &mut VatsMode, combat: &Combat, parts: &ActorParts, mut script: Option<&mut ScriptState>, sim: &mut SimState, phys: &mut PhysState, player: &Player, control: &PlayerControl) {
    let (from, dir) = Crosshair::ray(control.crosshair.as_ref(), player.eye(), player.look_dir());
    let reach = v.weapon.as_ref().map_or(REACH, |w| w.reach);
    let filter = shot_filter(phys);
    let target = preview_target(phys, script.as_deref(), from, dir, reach, &filter);
    let actor = target.map(|t| t.0);
    let under = target.and_then(|t| t.1);
    let mut pv = std::mem::take(&mut v.preview);
    if actor != pv.actor || v.now - pv.built_at >= PREVIEW_REBUILD {
        let changed = actor != pv.actor;
        pv.actor = actor;
        pv.built_at = v.now;
        pv.parts.clear();
        if let Some(actor) = actor.filter(|&a| barred(v, script.as_deref_mut(), sim, a).is_ok()) {
            let mut list = std::mem::take(&mut v.preview_list);
            parts_of(phys, parts, actor, &mut list);
            for &part in list.iter().take(MAX_PREVIEW) {
                let Some(point) = point_of(phys, actor, part) else { continue };
                let (spot, _) = spot_on(v, combat, parts, script.as_deref_mut(), sim, player, actor, part, point, 0);
                pv.parts.push(PreviewPart { part, spot, shown: false, under: false });
            }
            v.preview_list = list;
            if changed {
                let name = pv.parts.first().map_or("?", |p| p.spot.name.as_str());
                let figures: Vec<String> = pv.parts.iter().map(|p| format!("{} {}", p.spot.part, chance_text(p.spot.chance))).collect();
                info!("vats mode: preview {name} {actor:08X}: {}", figures.join(", "));
            }
        }
    }
    if let (Some(actor), Some(w)) = (pv.actor, v.weapon.as_ref()) {
        let eye = player.eye();
        let one = pv.parts.len() == 1;
        for p in &mut pv.parts {
            if let Some(at) = point_of(phys, actor, p.part) {
                p.spot.point = at;
            }
            p.spot.chance = chance_of(&p.spot, w, eye, combat.vats_cone);
            // With one capsule the whole actor is under the crosshair whenever it is.
            p.under = under.is_some_and(|u| one || u == p.part);
            // A part marked already shows its mark's label only.
            let marked = v.queue.marks().iter().any(|m| m.target.0 == actor && m.part == p.part);
            p.shown = !marked && in_sight(phys, from, p.spot.point, &filter);
        }
    }
    v.preview = pv;
}

/// Left click: mark the actor (and body part) under the crosshair (§6, §7.2).
#[allow(clippy::too_many_arguments)]
fn mark(
    v: &mut VatsMode,
    combat: &Combat,
    parts: &ActorParts,
    mut script: Option<&mut ScriptState>,
    sim: &mut SimState,
    phys: &mut PhysState,
    player: &Player,
    control: &PlayerControl,
    hud: &mut HudMessage,
    sounds: &mut Sounds,
) {
    let (from, dir) = Crosshair::ray(control.crosshair.as_ref(), player.eye(), player.look_dir());
    let reach = v.weapon.as_ref().map_or(REACH, |w| w.reach);
    // Through what the character passes and the actors' walking capsules (their hit
    // volumes are their bodies), as the shots go.
    let mut filter = Filter { exclude_layers: layers::shot_ignores(), ..Default::default() };
    filter.exclude.extend(phys.npcs.values().map(|b| b.ch.key()));
    let hit = phys.ray(from, dir, reach, filter);
    // A placed object VATS may target (§20): a destructible flagged VATS Targetable, or
    // one a script's SetVatsTarget turned on.
    if let Some((object, point)) = hit.as_ref().and_then(|h| match h.owner {
        Owner::Ref { form_id, .. } => Some((form_id, h.point)),
        _ => None,
    }) {
        let targetable = script.as_deref_mut().and_then(|s| s.rt.as_mut()).is_some_and(|rt| with_world(rt, sim, |rt| rt.game.vats_targetable(FormId(object))));
        if targetable {
            mark_object(v, object, point, script, sim, player, hud, sounds);
        } else {
            info!("vats mode: nothing to mark under the crosshair ({object:08X} is not a VATS target)");
        }
        return;
    }
    let Some((actor, part)) = hit.as_ref().and_then(|h| match h.owner {
        Owner::Actor { form_id, part, .. } => Some((form_id, (part != fnv_phys::actors::WHOLE).then_some(part))),
        _ => None,
    }) else {
        info!("vats mode: nothing to mark under the crosshair");
        return;
    };
    match barred(v, script.as_deref_mut(), sim, actor) {
        Err(Bar::Dead) => {
            info!("vats mode: {actor:08X} is dead: not marked");
            return;
        }
        Err(Bar::Cloaked) => {
            v.stats.refused += 1;
            info!("vats mode: {actor:08X} is cloaked: not marked (Enhanced Sensors would let the player mark it)");
            return;
        }
        Err(Bar::NoVatsMelee) => {
            v.stats.refused += 1;
            info!("vats mode: {actor:08X} is flagged No VATS Melee: not marked with a melee weapon");
            return;
        }
        Ok(true) => info!("vats mode: {actor:08X} is cloaked: marked through Enhanced Sensors"),
        Ok(false) => {}
    }
    let ap_now = player_ap(script.as_deref_mut(), sim).map_or(f32::INFINITY, |a| a.0);
    let earlier = v.queue.marks().iter().filter(|m| m.target.0 == actor && m.part == part).count() as u32;
    match v.queue.push(FormId(actor), part, ap_now) {
        Ok(k) => {
            let point = point_of(phys, actor, part).or(hit.map(|h| h.point)).unwrap_or(from);
            v.stats.marks += 1;
            let (spot, names) = spot_on(v, combat, parts, script, sim, player, actor, part, point, earlier);
            let perk_text = if names.is_empty() { String::new() } else { format!(", perks {}", names.join(", ")) };
            let cf = if spot.perks.concentrated_fire && earlier > 0 { format!(", concentrated fire +{earlier}") } else { String::new() };
            // What the preview showed for this part (§17.5): the same figure.
            let previewed = match v.preview.parts.iter().find(|p| earlier == 0 && v.preview.actor == Some(actor) && p.part == part) {
                Some(p) => format!("; previewed {}", chance_text(p.spot.chance)),
                None => String::new(),
            };
            info!(
                "vats mode: marked {} {actor:08X} ({}), mark {k}; {:.0} AP reserved, {} AP left; {:.0}% to hit (base {:.0}{perk_text}{cf}); {:.0} units, skill {:.0}, cone {}{previewed}",
                spot.name,
                spot.part,
                v.queue.reserved(),
                if ap_now.is_finite() { format!("{:.0}", ap_now - v.queue.reserved()) } else { "unlimited".into() },
                spot.chance,
                spot.base,
                crate::combat::distance(player.eye(), spot.point),
                v.weapon.as_ref().map_or(0.0, |w| w.skill),
                combat.vats_cone.map_or("none".to_string(), |c| format!("{c:.2} deg")),
            );
            v.spots.push(spot);
            sounds.play(v, if part.is_some() { "UIVATSSelectTargetPart" } else { "UIVATSSelectTarget" });
        }
        Err(r) => {
            v.stats.refused += 1;
            info!("vats mode: mark refused: {} ({} marks, {:.0} AP reserved)", r.message(), v.queue.len(), v.queue.reserved());
            hud.show(r.message().to_string());
            sounds.play(v, "UIVATSInsufficientAP");
        }
    }
}

/// Mark the placed object `object` at `point` (§20): the whole object, at
/// [`WHOLE_TO_HIT`]'s base chance; its point stays where it was marked.
#[allow(clippy::too_many_arguments)]
fn mark_object(v: &mut VatsMode, object: u32, point: [f32; 3], mut script: Option<&mut ScriptState>, sim: &mut SimState, player: &Player, hud: &mut HudMessage, sounds: &mut Sounds) {
    let ap_now = player_ap(script.as_deref_mut(), sim).map_or(f32::INFINITY, |a| a.0);
    let earlier = v.queue.marks().iter().filter(|m| m.target.0 == object).count() as u32;
    match v.queue.push(FormId(object), None, ap_now) {
        Ok(k) => {
            let name = sim.base_of(object).and_then(|b| sim.name(b).map(str::to_string)).unwrap_or_else(|| "?".into());
            let shot = v.weapon.as_ref().map(|w| fnv_game::vats::VatsShot {
                weapon: w.id,
                target: FormId(object),
                distance: crate::combat::distance(player.eye(), point) as f64,
                condition: None,
                success: false,
                height: (point[2] - player.feet[2]) as f64,
            });
            let perks = match (shot, script.and_then(|s| s.rt.as_mut())) {
                (Some(shot), Some(rt)) => with_world(rt, sim, |rt| rt.game.vats_hit_perks(shot)),
                _ => HitPerks::default(),
            };
            let mut spot = Spot { point, name, part: "object".into(), condition: None, base: WHOLE_TO_HIT, size: SIZE_TORSO, perks, earlier, chance: 0.0, landed: None };
            if let Some(w) = v.weapon.as_ref() {
                spot.chance = chance_of(&spot, w, player.eye(), None);
            }
            v.stats.marks += 1;
            info!("vats mode: marked object {} {object:08X}, mark {k}; {:.0} AP reserved; {:.0}% to hit; {:.0} units", spot.name, v.queue.reserved(), spot.chance, crate::combat::distance(player.eye(), point));
            v.spots.push(spot);
            sounds.play(v, "UIVATSSelectTarget");
        }
        Err(r) => {
            v.stats.refused += 1;
            info!("vats mode: mark refused: {} ({} marks, {:.0} AP reserved)", r.message(), v.queue.len(), v.queue.reserved());
            hud.show(r.message().to_string());
            sounds.play(v, "UIVATSInsufficientAP");
        }
    }
}

/// Fire round `round` of mark `i` (§9.1, §16.1, §16.3): the view snaps to it,
/// `crate::combat` fires at it this frame (a throw on its arc); the mark's first round
/// spends its AP.
#[allow(clippy::too_many_arguments)]
fn shoot(v: &mut VatsMode, i: usize, round: u32, mut script: Option<&mut ScriptState>, sim: &mut SimState, phys: &mut PhysState, player: &mut Player, control: &PlayerControl, sounds: &mut Sounds) {
    let Some((point, chance)) = v.spots.get(i).map(|s| (s.point, s.chance)) else { return };
    face(player, control, point);
    // Every round is rolled on the mark's chance now (§17.4); a miss goes wide of it.
    let (rolled, hit) = v.roll(chance);
    let thrown = v.weapon.as_ref().is_some_and(|w| w.style == AttackStyle::Thrown);
    let aim = if hit {
        point
    } else {
        let (u1, u2) = v.two();
        if thrown {
            throw_miss_point(point, u1, u2)
        } else {
            miss_point(player.eye(), point, u1, u2)
        }
    };
    if let Some(s) = v.spots.get_mut(i) {
        s.landed = Some(hit);
    }
    let outcome = format!("{chance:.0}% to hit: {} (rolled {rolled:.0})", if hit { "hit" } else { "missed" });
    v.shot = Some(aim);
    v.shooting = Some(i);
    v.fired += 1;
    v.stats.shots += 1;
    // The sights rule looks at this mark until its burst is out, then at the next.
    let rounds = match v.weapon.as_ref().map(|w| w.style) {
        Some(AttackStyle::Burst { shots, .. }) => shots,
        _ => 1,
    };
    v.aim_mark = if round + 1 < rounds { i } else { (i + 1).min(v.spots.len().saturating_sub(1)) };
    if round > 0 {
        debug!("vats mode: round {} of mark {}: {outcome}", round + 1, i + 1);
        return;
    }
    let cost = start_mark(v, i, script.as_deref_mut(), sim);
    call_helper(v, i, script, sim, phys, player, sounds);
    let (Some(m), Some(s)) = (v.queue.marks().get(i), v.spots.get(i)) else { return };
    let n = v.queue.len();
    match v.weapon.as_ref().map(|w| w.style) {
        Some(AttackStyle::Burst { shots, .. }) => info!("vats mode: burst {} of {n} at {} {} ({}), {shots} shots, {cost:.0} AP spent; round 1 {outcome}", i + 1, s.name, m.target, s.part),
        Some(AttackStyle::Thrown) => info!("vats mode: throw {} of {n} at {} {} ({}), {cost:.0} AP spent; {outcome}", i + 1, s.name, m.target, s.part),
        _ => info!("vats mode: shot {} of {n} at {} {} ({}), {cost:.0} AP spent; {outcome}", i + 1, s.name, m.target, s.part),
    }
}

/// The view snaps to `point`.
fn face(player: &mut Player, control: &PlayerControl, point: [f32; 3]) {
    let eye = player.eye();
    (player.heading, player.pitch) = Crosshair::aim_at(control.crosshair.as_ref(), eye, point);
}

/// Mark `i`'s attack starts: its AP is spent (§7.2). The AP spent.
fn start_mark(v: &mut VatsMode, i: usize, script: Option<&mut ScriptState>, sim: &mut SimState) -> f32 {
    let Some(m) = v.queue.marks().get(i).copied() else { return 0.0 };
    v.started = v.started.max(i + 1);
    let Some(rt) = script.and_then(|sc| sc.rt.as_mut()) else { return 0.0 };
    with_world(rt, sim, |rt| fnv_game::functions::add_actor_value_damage(&mut rt.game, FormId::PLAYER_REF, ACTION_POINTS, m.cost as f64));
    v.spent += m.cost;
    v.stats.ap_spent += m.cost;
    m.cost
}

/// A melee sequence's frame (§16.2): the player is put beside the mark, swings at it, and
/// the swing is waited for.
#[allow(clippy::too_many_arguments)]
fn melee_step(v: &mut VatsMode, dt: f32, combat: &Combat, mut script: Option<&mut ScriptState>, sim: &mut SimState, phys: &mut PhysState, player: &mut Player, control: &PlayerControl, sounds: &mut Sounds) {
    let Some(Run::Melee(run)) = v.run.as_mut() else { return };
    let i = run.current.min(run.marks.saturating_sub(1));
    let eye = player.eye();
    let across = v.spots.get(i).map_or(f32::INFINITY, |s| ((s.point[0] - eye[0]).powi(2) + (s.point[1] - eye[1]).powi(2)).sqrt());
    let close = across <= v.weapon.as_ref().map_or(0.0, |w| w.melee_reach) + CLOSE_SLACK;
    let action = run.step(dt, close, combat.player_swinging());
    match action {
        Some(MeleeAction::Approach(i)) => {
            let Some(s) = v.spots.get(i) else { return };
            let p = s.point;
            // Once a mark: the player is put on the ground between where they stand and
            // the mark, `stand_off` from it across the ground (owner 2026-10-20: "a
            // teleport to target and attack").
            if v.closing != Some(i) {
                v.closing = Some(i);
                let reach = v.weapon.as_ref().map_or(0.0, |w| w.melee_reach);
                let feet = stand_beside(player.feet, p, stand_off(reach));
                phys.place_player(feet, TELEPORT_UP, TELEPORT_DOWN);
                player.feet = phys.prev_feet;
                info!("vats mode: teleported to {} {} ({}), {across:.0} units away, now {:.0}", s.name, v.queue.marks()[i].target, s.part, stand_off(reach));
            }
            face(player, control, p);
        }
        Some(MeleeAction::Swing(i)) => {
            let Some((p, chance)) = v.spots.get(i).map(|s| (s.point, s.chance)) else { return };
            face(player, control, p);
            let (rolled, hit) = v.roll(chance);
            v.blow_lands = hit;
            if let Some(s) = v.spots.get_mut(i) {
                s.landed = Some(hit);
            }
            v.swing_now = true;
            v.striking = Some(i);
            v.shooting = Some(i);
            v.fired += 1;
            v.stats.shots += 1;
            let cost = start_mark(v, i, script.as_deref_mut(), sim);
            call_helper(v, i, script, sim, phys, player, sounds);
            let (m, s) = (v.queue.marks()[i], &v.spots[i]);
            info!(
                "vats mode: swing {} of {} at {} {} ({}), {across:.0} units across, {cost:.0} AP spent; {chance:.0}% to hit: {} (rolled {rolled:.0})",
                i + 1,
                v.queue.len(),
                s.name,
                m.target,
                s.part,
                if hit { "hit" } else { "missed" }
            );
        }
        // The view stays on the mark being swung at.
        Some(MeleeAction::Wait(i)) => {
            if let Some(p) = v.spots.get(i).map(|s| s.point).filter(|_| v.striking == Some(i)) {
                face(player, control, p);
            }
        }
        Some(MeleeAction::Done) => end(v, "all shots fired", sounds),
        None => {}
    }
}

impl VatsMode {
    /// The last mark's shot, once every mark's attack has started (a kill camera watches
    /// its actor, `crate::kill_cam`, §19): as the scripts see a shot, with the weapon VATS
    /// mode was entered with, and its point now.
    pub fn final_shot(&self, player: &Player) -> Option<(fnv_game::vats::VatsShot, [f32; 3])> {
        let eye = player.eye();
        let n = self.queue.len();
        if self.phase != Phase::Firing || n == 0 || self.started < n {
            return None;
        }
        let (m, s) = (self.queue.marks().get(n - 1)?, self.spots.get(n - 1)?);
        let d = ((s.point[0] - eye[0]).powi(2) + (s.point[1] - eye[1]).powi(2) + (s.point[2] - eye[2]).powi(2)).sqrt();
        let shot = fnv_game::vats::VatsShot { weapon: self.weapon.as_ref()?.id, target: m.target, distance: d as f64, condition: s.condition, success: s.landed.unwrap_or(false), height: (s.point[2] - player.feet[2]) as f64 };
        Some((shot, s.point))
    }
}

/// The test aid `NVF_PLAY_TEST_VATS_HELPER`: `stranger` or `fortune` brings that helper
/// for every mark whose odds for them are above 0 (the player has the perk), `none` none
/// (vats-mode.md §13).
fn forced_helper() -> Option<Option<Helper>> {
    static FORCED: std::sync::OnceLock<Option<Option<Helper>>> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| match std::env::var("NVF_PLAY_TEST_VATS_HELPER").ok()?.trim() {
        "stranger" => Some(Some(Helper::Stranger)),
        "fortune" => Some(Some(Helper::Fortune)),
        "none" => Some(None),
        other => {
            warn!("vats mode: NVF_PLAY_TEST_VATS_HELPER={other}: not stranger, fortune or none; helpers are rolled");
            None
        }
    })
}

/// Where a helper stands for a mark at `point` (§18.2): `away` from the player's feet across
/// the line to the mark, on its right, else its left, else behind the player; the first
/// spot from whose eye nothing but the mark's actor stands in the way. `None` when all are
/// blocked.
fn helper_spot(phys: &mut PhysState, feet: [f32; 3], point: [f32; 3], target: u32, away: f32) -> Option<[f32; 3]> {
    let (dx, dy) = (point[0] - feet[0], point[1] - feet[1]);
    let flat = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (fx, fy) = (dx / flat, dy / flat);
    let spots = [[feet[0] + fy * away, feet[1] - fx * away, feet[2]], [feet[0] - fy * away, feet[1] + fx * away, feet[2]], [feet[0] - fx * away, feet[1] - fy * away, feet[2]]];
    spots.into_iter().find(|&at| {
        let eye = [at[0], at[1], at[2] + HELPER_EYE];
        let to = [point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]];
        let len = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
        if len < 1.0 {
            return false;
        }
        let dir = [to[0] / len, to[1] / len, to[2] / len];
        let mut filter = Filter { exclude_layers: layers::shot_ignores(), ..Default::default() };
        filter.exclude.extend(phys.npcs.values().map(|b| b.ch.key()));
        match phys.ray(eye, dir, len + 30.0, filter) {
            None => true,
            Some(h) => matches!(h.owner, Owner::Actor { form_id, .. } if form_id == target) || h.distance >= len - 30.0,
        }
    })
}

/// Mark `i`'s attack started: with Mysterious Stranger or Miss Fortune, one of them may come
/// for it (§18.1, §18.2). One helper at a time.
#[allow(clippy::too_many_arguments)]
fn call_helper(v: &mut VatsMode, i: usize, script: Option<&mut ScriptState>, sim: &mut SimState, phys: &mut PhysState, player: &Player, sounds: &mut Sounds) {
    if v.helper.is_some() {
        return;
    }
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return };
    let (Some(m), Some(point)) = (v.queue.marks().get(i).copied(), v.spots.get(i).map(|s| s.point)) else { return };
    let odds = with_world(rt, sim, |rt| rt.game.vats_helper_odds(m.target));
    if odds.stranger + odds.fortune <= 0.0 {
        return;
    }
    let u = v.two().0 as f64;
    let who = match forced_helper() {
        Some(Some(Helper::Stranger)) => (odds.stranger > 0.0).then_some(Helper::Stranger),
        Some(Some(Helper::Fortune)) => (odds.fortune > 0.0).then_some(Helper::Fortune),
        Some(None) => None,
        None => pick_helper(odds, u),
    };
    let Some(who) = who else {
        debug!("vats mode: no helper for mark {} ({:.0}% the Stranger, {:.0}% Miss Fortune, rolled {:.0})", i + 1, odds.stranger * 100.0, odds.fortune * 100.0, u * 100.0);
        return;
    };
    let Some(feet) = helper_spot(phys, player.feet, point, m.target.0, odds.distance as f32) else {
        info!("vats mode: {} would come for mark {}, but nowhere beside the player sees it", who.label(), i + 1);
        return;
    };
    let heading = (point[0] - feet[0]).atan2(point[1] - feet[1]);
    let (reference, weapon) = with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        (g.summon_vats_helper(who, feet, heading), g.data.by_edid(who.weapon_edid()))
    });
    let (Some(reference), Some(weapon)) = (reference, weapon) else {
        info!("vats mode: {} cannot come: the load order has no {} or {}", who.label(), who.edid(), who.weapon_edid());
        return;
    };
    info!(
        "vats mode: {} comes for mark {} ({}), {:.0} units from the player ({:.0}% odds, rolled {:.0})",
        who.label(),
        i + 1,
        m.target,
        odds.distance,
        if who == Helper::Stranger { odds.stranger } else { odds.fortune } * 100.0,
        u * 100.0
    );
    v.stats.helpers += 1;
    // The Stranger's theme [observed: FalloutNV.esm SOUN MUSMysteriousStrangerA01, A02;
    // hypothesis: played as he comes, one of the two].
    if who == Helper::Stranger {
        sounds.play(v, if u < odds.stranger / 2.0 { "MUSMysteriousStrangerA01" } else { "MUSMysteriousStrangerA02" });
    }
    v.helper = Some(ActiveHelper { who, reference, weapon, feet, target: m.target, part: m.part, point, age: 0.0, fired: false });
}

/// The helper's frame (§18.2): their shot at the mark after [`HELPER_DRAW`], their leaving
/// after [`HELPER_STAY`]. Runs whether the mode is on or not.
fn tick_helper(v: &mut VatsMode, dt: f32, script: Option<&mut ScriptState>, sim: &mut SimState, phys: &PhysState, sounds: &mut Sounds) {
    let Some(h) = v.helper.as_mut() else { return };
    h.age += dt.max(0.0);
    if !h.fired && h.age >= HELPER_DRAW {
        h.fired = true;
        let point = point_of(phys, h.target.0, h.part).unwrap_or(h.point);
        let origin = [h.feet[0], h.feet[1], h.feet[2] + HELPER_EYE];
        let to = [point[0] - origin[0], point[1] - origin[1], point[2] - origin[2]];
        let len = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt().max(1e-3);
        let aim = [to[0] / len, to[1] / len, to[2] / len];
        sounds.fire.write(ActorFire { shooter: h.reference, weapon: h.weapon, origin, aim, cone: 0.0, muzzle: None, harmless: false });
        info!("vats mode: {} fires at {} from {len:.0} units", h.who.label(), h.target);
    }
    if h.age >= HELPER_STAY {
        let (r, who) = (h.reference, h.who);
        v.helper = None;
        if let Some(rt) = script.and_then(|s| s.rt.as_mut()) {
            with_world(rt, sim, |rt| rt.game.dismiss_vats_helper(r));
        }
        info!("vats mode: {} leaves", who.label());
    }
}

/// After a sequence that killed (the player's kills rose since it began to be fired), the
/// Player Kill AP Reward perks give AP back (Grim Reaper's Sprint, §18.3).
fn kill_reward(v: &mut VatsMode, before: f64, script: Option<&mut ScriptState>, sim: &mut SimState) {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return };
    let given = with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        let kills = g.player_kill_count() - before;
        if kills < 1.0 {
            return None;
        }
        let reward = g.vats_kill_ap_reward();
        if reward <= 0.0 {
            return None;
        }
        let [_, _, damage] = fnv_game::functions::actor_value_parts(g, FormId::PLAYER_REF, ACTION_POINTS).ok()?;
        let back = reward.min(damage.max(0.0));
        fnv_game::functions::restore_actor_value_damage(g, FormId::PLAYER_REF, ACTION_POINTS, back);
        Some((kills, reward, back))
    });
    if let Some((kills, reward, back)) = given {
        v.stats.ap_rewarded += back as f32;
        info!("vats mode: {kills:.0} killed: the Player Kill AP Reward perks give {reward:.0} AP back ({back:.0} restored)");
    }
}

/// GetVATSMode and GetVATSValue's view of the mode (§12).
fn tell_scripts(v: &VatsMode, script: Option<&mut ScriptState>, player: &Player) {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return };
    use fnv_game::vats::{VatsShot, MODE_OFF, MODE_PLAYBACK, MODE_TARGET_SELECT};
    let mode = match v.phase {
        Phase::Marking => MODE_TARGET_SELECT,
        Phase::Firing => MODE_PLAYBACK,
        _ => MODE_OFF,
    };
    let shot = v.shooting.filter(|_| mode == MODE_PLAYBACK).and_then(|i| {
        let (m, s) = (v.queue.marks().get(i)?, v.spots.get(i)?);
        let eye = player.eye();
        let d = ((s.point[0] - eye[0]).powi(2) + (s.point[1] - eye[1]).powi(2) + (s.point[2] - eye[2]).powi(2)).sqrt();
        Some(VatsShot { weapon: v.weapon.as_ref()?.id, target: m.target, distance: d as f64, condition: s.condition, success: s.landed.unwrap_or(false), height: (s.point[2] - player.feet[2]) as f64 })
    });
    if rt.game.vats.mode != mode || rt.game.vats.shot != shot {
        rt.game.set_vats(mode, shot);
    }
}

/// A mark's "x" on the screen: two crossed bars.
#[derive(Component)]
struct MarkCross(usize);

fn spawn_marks(mut commands: Commands) {
    for i in 0..MAX_PREVIEW {
        commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, GlobalZIndex(5), PreviewLabel { i, shown: String::new(), width: 0.0 }));
    }
    for i in 0..MAX_DRAWN {
        commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, GlobalZIndex(5), MarkLabel { i, shown: String::new() }));
    }
    for i in 0..MAX_DRAWN {
        commands
            .spawn((Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, GlobalZIndex(5), MarkCross(i)))
            .with_children(|p| {
                for deg in [45.0f32, -45.0] {
                    p.spawn((
                        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(MARK_BAR), top: Val::Percent((100.0 - MARK_BAR) / 2.0), ..default() },
                        BackgroundColor(MARK_COLOUR),
                        UiTransform::from_rotation(Rot2::degrees(deg)),
                    ));
                }
            });
    }
}

/// Put each mark's "x" where its point is on the screen (§8).
fn draw_marks(
    vats: Res<VatsMode>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut crosses: Query<(&MarkCross, &mut Node, &mut Visibility)>,
) {
    let width = windows.iter().next().map_or(1280.0, |w| w.width());
    let size = (MARK_SIZE * width).max(6.0);
    let camera = cameras.single().ok();
    let spots: Vec<[f32; 3]> = vats.drawn().into_iter().map(|(p, _)| p).collect();
    for (c, mut node, mut vis) in &mut crosses {
        let at = spots.get(c.0).zip(camera).and_then(|(p, (cam, t))| cam.world_to_viewport(t, space::to_bevy(*p)).ok());
        let want = if at.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let Some(at) = at {
            node.left = Val::Px(at.x - size / 2.0);
            node.top = Val::Px(at.y - size / 2.0);
            node.width = Val::Px(size);
            node.height = Val::Px(size);
        }
    }
}

/// A mark's chance to hit, "NN%", beside its "x" (§17.5); `shown` is the text its glyphs
/// now spell.
#[derive(Component)]
struct MarkLabel {
    i: usize,
    shown: String,
}

/// The labels' font: the HUD's (`crate::hud`), and where the line stands in it.
const LABEL_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
/// The label's baseline below its top, interface units (960 to the view's height).
const LABEL_BASE: f32 = 30.0;
/// Its height, interface units, for centring it on the "x".
const LABEL_HEIGHT: f32 = 36.0;
/// The marks' brightened amber [design: the owner's wishes, 2026-10-24; it was a creamy
/// white before].
const LABEL_COLOUR: Color = MARK_COLOUR;

/// The text of a chance: whole percent.
pub fn chance_text(chance: f32) -> String {
    format!("{:.0}%", chance.clamp(0.0, 100.0))
}

/// Put each mark's chance beside its "x" (§17.5): to its right, centred on it.
#[allow(clippy::too_many_arguments)]
fn draw_labels(
    mut commands: Commands,
    vats: Res<VatsMode>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut face_: Local<Option<Face>>,
    mut labels: Query<(Entity, &mut MarkLabel, &mut Node, &mut Visibility)>,
) {
    let (width, height) = windows.iter().next().map_or((1280.0, 720.0), |w| (w.width(), w.height()));
    let size = (MARK_SIZE * width).max(6.0);
    let tall = LABEL_HEIGHT / 960.0 * height;
    let camera = cameras.single().ok();
    let spots = vats.drawn();
    for (e, mut label, mut node, mut vis) in &mut labels {
        let spot = spots.get(label.i);
        let at = spot.zip(camera).and_then(|((p, _), (cam, t))| cam.world_to_viewport(t, space::to_bevy(*p)).ok());
        let want = if at.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let (Some(at), Some((_, chance))) = (at, spot) else { continue };
        node.left = Val::Px(at.x + size * 0.75);
        node.top = Val::Px(at.y - tall / 2.0);
        let text = chance_text(*chance);
        if label.shown != text {
            let f = face_.get_or_insert_with(|| font_face(LABEL_FONT, plain_font(36.0, 15.0), data.as_deref(), &mut images, bc_supported(formats.as_deref())));
            commands.entity(e).despawn_related::<Children>().with_children(|p| {
                if let Some(line) = f.font.lines(&text, None).into_iter().next() {
                    set_line(p, f, &line, 0.0, LABEL_BASE, LABEL_COLOUR);
                }
            });
            label.shown = text;
        }
    }
}

/// A previewed part's chance (§17.5): `shown` is the text its glyphs now spell, `width`
/// that text's width in interface units.
#[derive(Component)]
struct PreviewLabel {
    i: usize,
    shown: String,
    width: f32,
}

/// The part under the crosshair's preview: the amber lifted this far towards white,
/// brighter than the marks' [design].
const PREVIEW_UNDER_LIFT: f32 = 0.75;
const PREVIEW_UNDER_COLOUR: Color = Color::srgb(
    crate::interface::AMBER[0] + (1.0 - crate::interface::AMBER[0]) * PREVIEW_UNDER_LIFT,
    crate::interface::AMBER[1] + (1.0 - crate::interface::AMBER[1]) * PREVIEW_UNDER_LIFT,
    crate::interface::AMBER[2] + (1.0 - crate::interface::AMBER[2]) * PREVIEW_UNDER_LIFT,
);

impl VatsMode {
    /// The preview labels to draw (§17.5), while marking only: each shown part's point,
    /// its text and whether the crosshair is on it.
    fn previewed(&self) -> impl Iterator<Item = (&PreviewPart, [f32; 3])> {
        let on = self.phase == Phase::Marking;
        self.preview.parts.iter().filter(move |p| on && p.shown).map(|p| (p, p.spot.point))
    }
}

/// A previewed part's label (§17.5): its name in small letters and its chance,
/// "head 41%"; the whole actor's chance alone; the part under the crosshair in brackets,
/// "[head 41%]" [design: NVForever's own wording].
pub fn preview_text(out: &mut String, part: &str, chance: f32, under: bool) {
    use std::fmt::Write;
    out.clear();
    if under {
        out.push('[');
    }
    match part {
        "whole" => {}
        "its weapon" => out.push_str("weapon "),
        _ => {
            for c in part.chars() {
                out.extend(c.to_lowercase());
            }
            out.push(' ');
        }
    }
    let _ = write!(out, "{:.0}%", chance.clamp(0.0, 100.0));
    if under {
        out.push(']');
    }
}

/// Put each previewed part's chance on its point (§17.5): centred on it, a label that
/// would overlap one placed before it moved down a line, so a far actor's labels stay
/// apart.
#[allow(clippy::too_many_arguments)]
fn draw_previews(
    mut commands: Commands,
    vats: Res<VatsMode>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut face_: Local<Option<Face>>,
    mut text: Local<String>,
    mut placed: Local<Vec<[f32; 4]>>,
    mut labels: Query<(Entity, &mut PreviewLabel, &mut Node, &mut Visibility)>,
) {
    let height = windows.iter().next().map_or(720.0, |w| w.height());
    let px = height / 960.0;
    let tall = LABEL_HEIGHT * px;
    let camera = cameras.single().ok();
    placed.clear();
    let mut shown = vats.previewed();
    // The labels in their order (the query's is not theirs).
    let mut order: [Option<Entity>; MAX_PREVIEW] = [None; MAX_PREVIEW];
    for (e, label, _, _) in &labels {
        if let Some(slot) = order.get_mut(label.i) {
            *slot = Some(e);
        }
    }
    for e in order.into_iter().flatten() {
        let Ok((e, mut label, mut node, mut vis)) = labels.get_mut(e) else { continue };
        let next = shown.next();
        let at = next.zip(camera).and_then(|((_, p), (cam, t))| cam.world_to_viewport(t, space::to_bevy(p)).ok());
        let want = if at.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let (Some(at), Some((p, _))) = (at, next) else { continue };
        preview_text(&mut text, &p.spot.part, p.spot.chance, p.under);
        if label.shown != *text {
            let f = face_.get_or_insert_with(|| font_face(LABEL_FONT, plain_font(36.0, 15.0), data.as_deref(), &mut images, bc_supported(formats.as_deref())));
            let colour = if p.under { PREVIEW_UNDER_COLOUR } else { LABEL_COLOUR };
            let line = f.font.lines(&text, None).into_iter().next();
            label.width = line.as_ref().map_or(0.0, |l| l.width);
            commands.entity(e).despawn_related::<Children>().with_children(|c| {
                if let Some(line) = &line {
                    set_line(c, f, line, 0.0, LABEL_BASE, colour);
                }
            });
            label.shown.clone_from(&text);
        }
        let w = label.width * px;
        let (x, mut y) = (at.x - w / 2.0, at.y - tall / 2.0);
        while placed.iter().any(|r| x < r[0] + r[2] && r[0] < x + w && y < r[1] + r[3] && r[1] < y + tall) {
            y += tall;
        }
        placed.push([x, y, w, tall]);
        node.left = Val::Px(x);
        node.top = Val::Px(y);
    }
}

fn report(mut exit: MessageReader<AppExit>, vats: Res<VatsMode>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = vats.stats;
    if s.entered == 0 && s.refused == 0 {
        return;
    }
    info!(
        "vats mode summary: entered {} times, {} marks, {} shots, {:.0} AP spent, {} refused, {} hit, {} missed, {} helpers, {:.0} AP rewarded",
        s.entered, s.marks, s.shots, s.ap_spent, s.refused, s.hits, s.misses, s.helpers, s.ap_rewarded
    );
}

/// VATS mode's frame (`update`), for systems that read what it decided (`crate::kill_cam`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct VatsUpdate;

pub struct VatsModePlugin;

impl Plugin for VatsModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VatsMode>()
            .add_systems(Startup, spawn_marks)
            .add_systems(Update, read_keys.after(PlaySet::Input).after(crate::control::enforce_locks).after(crate::player::encumbered_walks).before(PlaySet::Physics))
            .add_systems(Update, update.in_set(PlaySet::Activation).in_set(VatsUpdate).before(crate::combat::fire))
            .add_systems(Update, (draw_marks, draw_labels, draw_previews).after(PlaySet::Activation))
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_action_but_marking_aiming_and_accepting_leaves() {
        let only = |k: Control| move |c: Control| c == k;
        // Activate accepts with a mark made, and leaves (to activate) with none or while firing.
        assert_eq!(leaving_press(Phase::Marking, 2, only(Control::Activate)), None);
        assert_eq!(leaving_press(Phase::Marking, 0, only(Control::Activate)), Some(Control::Activate));
        assert_eq!(leaving_press(Phase::Firing, 2, only(Control::Activate)), Some(Control::Activate));
        assert_eq!(leaving_press(Phase::Drawing, 0, only(Control::Activate)), Some(Control::Activate));
        for c in [Control::Rest, Control::Jump, Control::CrouchSneak, Control::Hotkey3, Control::QuickLoad, Control::Grab] {
            assert_eq!(leaving_press(Phase::Marking, 1, only(c)), Some(c), "{c:?}");
        }
        // The mode's own controls, moving and the camera stay.
        for c in [Control::Use, Control::Block, Control::Vats, Control::Forward, Control::Run, Control::TogglePov] {
            assert_eq!(leaving_press(Phase::Marking, 1, only(c)), None, "{c:?}");
        }
        assert_eq!(leaving_press(Phase::Off, 0, only(Control::Rest)), None);
    }

    #[test]
    fn a_chance_reads_as_whole_percent() {
        assert_eq!(chance_text(62.4), "62%");
        assert_eq!(chance_text(94.6), "95%");
        assert_eq!(chance_text(0.0), "0%");
        assert_eq!(chance_text(-3.0), "0%");
    }

    #[test]
    fn a_marks_chance_follows_range_and_perks() {
        let w = |style, skill| InHand { id: FormId(1), reach: REACH, style, melee_reach: 75.0, sight_fov: 0.0, skill, hit: HitTuning { spread_mult: 1.1, ..HitTuning::default() } };
        let torso = Spot { point: [500.0, 0.0, 0.0], base: 60.0, size: SIZE_TORSO, ..Default::default() };
        let gun = w(AttackStyle::Single, 50.0);
        let near = chance_of(&torso, &gun, [0.0; 3], Some(2.0));
        let far = chance_of(&Spot { point: [3000.0, 0.0, 0.0], ..torso.clone() }, &gun, [0.0; 3], Some(2.0));
        assert!(near > far && far > 0.0, "{near} {far}");
        // Melee ignores the range: the player is put beside the mark.
        let blade = w(AttackStyle::Melee, 50.0);
        assert_eq!(chance_of(&torso, &blade, [0.0; 3], None), chance_of(&Spot { point: [300.0, 0.0, 0.0], ..torso.clone() }, &blade, [0.0; 3], None));
        // A melee mark follows the skill and is not pinned at the cap.
        let head = Spot { base: 30.0, ..torso.clone() };
        let (weak, strong) = (chance_of(&head, &w(AttackStyle::Melee, 0.0), [0.0; 3], None), chance_of(&head, &blade, [0.0; 3], None));
        assert!(weak < strong && strong < 95.0, "{weak} {strong}");
        // Close by: capped at fVATSMaxChance.
        assert_eq!(chance_of(&torso, &gun, [490.0, 0.0, 0.0], Some(2.0)), 95.0);
        // Concentrated Fire's bonus counts only with the perk.
        let again = Spot { earlier: 2, ..torso.clone() };
        assert_eq!(chance_of(&again, &gun, [0.0; 3], Some(2.0)), near);
        let cf = Spot { perks: HitPerks { ops: Vec::new(), concentrated_fire: true }, ..again };
        assert!((chance_of(&cf, &gun, [0.0; 3], Some(2.0)) - (near + 10.0)).abs() < 1e-3);
    }

    #[test]
    fn a_preview_figure_is_the_marks_chance() {
        let gun = InHand { id: FormId(1), reach: REACH, style: AttackStyle::Single, melee_reach: 0.0, sight_fov: 0.0, skill: 45.0, hit: HitTuning::default() };
        let head = Spot { point: [900.0, 0.0, 60.0], part: "Head".into(), base: 30.0, size: 0.4, ..Default::default() };
        let mut v = VatsMode { phase: Phase::Marking, queue: MarkQueue::new(20.0, 1, 100), weapon: Some(gun.clone()), ..Default::default() };
        let previewed = chance_of(&head, &gun, [0.0; 3], Some(1.5));
        v.preview = Preview { actor: Some(7), parts: vec![PreviewPart { part: Some(1), spot: Spot { chance: previewed, ..head.clone() }, shown: true, under: true }], built_at: 0.0 };
        // Marked, the part's chance is worked out again the marks' way: the same figure.
        v.queue.push(FormId(7), Some(1), f32::INFINITY).unwrap();
        v.spots.push(head);
        let combat_cone = Some(1.5);
        for s in &mut v.spots {
            s.chance = chance_of(s, v.weapon.as_ref().unwrap(), [0.0; 3], combat_cone);
        }
        assert_eq!(chance_text(v.spots[0].chance), chance_text(previewed));
        assert_eq!(v.previewed().count(), 1);
        v.phase = Phase::Firing;
        assert_eq!(v.previewed().count(), 0, "nothing is previewed while firing");
        v.phase = Phase::Marking;
        v.preview.parts[0].shown = false;
        assert_eq!(v.previewed().count(), 0, "a hidden or marked part is not drawn");
    }

    #[test]
    fn a_preview_label_names_the_part() {
        let mut t = String::new();
        preview_text(&mut t, "Head", 41.4, false);
        assert_eq!(t, "head 41%");
        preview_text(&mut t, "Left Arm", 7.6, true);
        assert_eq!(t, "[left arm 8%]");
        preview_text(&mut t, "whole", 62.0, false);
        assert_eq!(t, "62%");
        preview_text(&mut t, "its weapon", 120.0, false);
        assert_eq!(t, "weapon 100%");
    }

    #[test]
    fn the_clock_eases_down_over_the_swipe() {
        assert_eq!(time_scale_at(0.0), 1.0);
        let mid = time_scale_at(ENTER_SWIPE / 2.0);
        assert!(mid < 1.0 && mid > MARK_SCALE, "{mid}");
        assert!((time_scale_at(ENTER_SWIPE) - MARK_SCALE).abs() < 1e-6);
        assert!((time_scale_at(10.0) - MARK_SCALE).abs() < 1e-6);
    }

    #[test]
    fn the_swipe_crosses_then_the_wash_fades_and_the_grade_stays() {
        let start = look_at(true, Some(0.0), None);
        assert_eq!(start, [0.0, BAND_STRENGTH, 0.0, 1.0]);
        let half = look_at(true, Some(ENTER_SWIPE / 2.0), None);
        assert!(half[0] > 0.5 && half[0] < 0.7, "{half:?}");
        let after = look_at(true, Some(ENTER_SWIPE), None);
        assert_eq!(after, [2.0, 0.0, WASH_START, 1.0]);
        assert_eq!(look_at(true, Some(ENTER_SWIPE + ENTER_WASH + 1.0), None), [2.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn leaving_fades_the_grade_out() {
        let l = look_at(false, None, Some(LEAVE_FADE / 2.0));
        assert!((l[3] - 0.5).abs() < 1e-6, "{l:?}");
        assert_eq!(look_at(false, None, Some(LEAVE_FADE)), [0.0; 4]);
        assert_eq!(look_at(false, None, None), [0.0; 4]);
    }

    #[test]
    fn marking_nearly_stops_the_clock_and_firing_halves_it() {
        let mut v = VatsMode { phase: Phase::Marking, entered_at: Some(0.0), now: 5.0, ..Default::default() };
        assert!((v.time_scale() - MARK_SCALE).abs() < 1e-6);
        v.phase = Phase::Firing;
        assert_eq!(v.time_scale(), FIRE_SCALE);
        assert_eq!(FIRE_SCALE, 0.5, "owner 2026-10-18: half speed");
        // The shots come SHOT_INTERVAL / FIRE_SCALE apart in real time.
        assert!((SHOT_INTERVAL / FIRE_SCALE - 0.4).abs() < 1e-6);
        v.phase = Phase::Drawing;
        assert_eq!(v.time_scale(), 1.0);
        v.phase = Phase::Off;
        assert_eq!(v.time_scale(), 1.0);
    }

    #[test]
    fn the_reservation_shrinks_as_shots_are_fired() {
        let mut v = VatsMode { phase: Phase::Marking, queue: MarkQueue::new(26.0, 1, 6), ..Default::default() };
        v.queue.push(FormId(1), None, 92.0).unwrap();
        v.queue.push(FormId(2), Some(1), 92.0).unwrap();
        assert_eq!(v.reserved(), 52.0);
        v.phase = Phase::Firing;
        v.started = 1;
        assert_eq!(v.reserved(), 26.0);
        v.phase = Phase::Off;
        assert_eq!(v.reserved(), 0.0);
    }

    #[test]
    fn a_melee_teleport_lands_beside_the_mark_on_the_players_side() {
        // A machete: reach 150 × 0.5 = 75; put 69.5 units from the mark, within reach + slack.
        let off = stand_off(75.0);
        assert!((off - 69.5).abs() < 1e-4);
        assert!(off <= 75.0 + CLOSE_SLACK);
        let feet = stand_beside([300.0, 0.0, 10.0], [0.0, 0.0, 40.0], off);
        assert_eq!(feet, [69.5, 0.0, 40.0]);
        // Standing on the mark: put to one side all the same.
        let feet = stand_beside([5.0, 5.0, 0.0], [5.0, 5.0, 0.0], 50.0);
        assert!((((feet[0] - 5.0).powi(2) + (feet[1] - 5.0).powi(2)).sqrt() - 50.0).abs() < 1e-4);
        // The owner's 5 m at 70 units a metre.
        assert_eq!(MELEE_MARK_REACH, 350.0);
    }

    #[test]
    fn third_person_shots_use_the_sights_beyond_20_m_only() {
        use crate::combat::aims_down_sights;
        // 20 m = 1,400 units; once up, the sights stay up to 1,350.
        assert!(!aims_down_sights(false, 1400.0, 55.0) && aims_down_sights(false, 1401.0, 55.0));
        assert!(aims_down_sights(true, 1360.0, 55.0) && !aims_down_sights(true, 1350.0, 55.0));
        // A gun without sights (Sight FOV 0) never raises them.
        assert!(!aims_down_sights(false, 5000.0, 0.0));
        assert_eq!(crate::combat::distance([0.0, 0.0, 0.0], [3.0, 4.0, 12.0]), 13.0);
    }

    #[test]
    fn marks_on_one_part_draw_once_with_the_newest_chance() {
        let mut v = VatsMode { phase: Phase::Marking, queue: MarkQueue::new(20.0, 1, 100), ..Default::default() };
        for (target, part, chance) in [(7, Some(1), 63.0), (7, Some(1), 68.0), (9, None, 40.0), (7, Some(1), 73.0), (7, Some(2), 30.0)] {
            v.queue.push(FormId(target), part, f32::INFINITY).unwrap();
            v.spots.push(Spot { point: [target as f32, part.unwrap_or(0) as f32, 0.0], chance, ..Default::default() });
        }
        assert_eq!(v.drawn(), vec![([7.0, 1.0, 0.0], 73.0), ([9.0, 0.0, 0.0], 40.0), ([7.0, 2.0, 0.0], 30.0)]);
        v.phase = Phase::Firing;
        v.started = 3;
        assert_eq!(v.drawn(), vec![([7.0, 1.0, 0.0], 73.0), ([7.0, 2.0, 0.0], 30.0)], "the fired marks go");
    }

    #[test]
    fn the_melee_mark_is_the_one_being_swung_at_while_firing() {
        let mut v = VatsMode { phase: Phase::Firing, queue: MarkQueue::new(20.0, 1, 100), ..Default::default() };
        v.queue.push(FormId(7), None, 92.0).unwrap();
        v.spots.push(Spot { point: [1.0, 2.0, 3.0], name: "dog".into(), part: "whole".into(), ..Default::default() });
        v.weapon = Some(InHand { id: FormId(1), reach: MELEE_MARK_REACH, style: AttackStyle::Melee, melee_reach: 75.0, sight_fov: 0.0, skill: 50.0, hit: HitTuning::default() });
        assert_eq!(v.melee_mark(), None, "no swing yet");
        v.striking = Some(0);
        assert_eq!(v.melee_mark(), Some((7, [1.0, 2.0, 3.0], 75.0 + CLOSE_SLACK, false)), "the roll has not let it land");
        v.blow_lands = true;
        assert_eq!(v.melee_mark(), Some((7, [1.0, 2.0, 3.0], 75.0 + CLOSE_SLACK, true)));
        v.swing_now = true;
        assert!(v.take_swing() && !v.take_swing(), "one tap");
        v.weapon.as_mut().unwrap().style = AttackStyle::Thrown;
        assert_eq!(v.melee_mark(), None, "a throw is no blow");
    }
}
