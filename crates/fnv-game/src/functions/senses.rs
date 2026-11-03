//! What the engine sees of actors and tells the scripts: movement, sneaking, line of
//! sight, the camera, and who always shows subtitles (docs/design/script-stubs.md §3).
//!
//! The engine measures; this module only keeps the latest word ([`Senses`]) and answers
//! the script functions from it. An actor the engine has not told about (not drawn, too
//! far from the player, a headless run) reads as standing still, not sneaking and not
//! seen [engineering choice: the same "not measured is no" rule as GetDetected].

use super::{none, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// One actor's movement as the engine last measured it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Motion {
    /// It moves (above the engine's walking threshold).
    pub moving: bool,
    /// It moves at its run speed (the player: not holding Walk).
    pub running: bool,
    /// It is sneaking (crouched).
    pub sneaking: bool,
    /// It is turning: 0 not, 1 left, 2 right (IsTurning's codes).
    pub turning: u8,
    /// Its speed mode is run, whether or not it moves (the player: Walk not toggled).
    pub run_mode: bool,
    /// Its character swims (the physics' state: `fnv_phys` character `swimming`).
    pub swimming: bool,
}

/// What the engine told the scripts, and the switches scripts set for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Senses {
    pub motion: BTreeMap<FormId, Motion>,
    /// The camera shows the player from the eyes (first person). The game starts so
    /// [observed: the PC version opens in first person; `nvf_play` `Pov::First`].
    pub first_person: bool,
    /// Actors whose lines always show their subtitle, heard or not
    /// (AlwaysShowActorSubtitles 1).
    pub always_subtitles: BTreeSet<FormId>,
    /// Look: the reference each actor's head is held on, until StopLook. Not part of the
    /// saved state yet.
    pub looks: BTreeMap<FormId, Look>,
    /// When ([`Game::time`]) the animating side last told whether each actor has its
    /// weapon out (`Game::set_weapon_out`): it tells every frame while it draws the actor.
    pub weapon_told: BTreeMap<FormId, f64>,
    /// The open state of every object the engine animates open and closed (doors,
    /// animated containers), as GetOpenState numbers it: 1 open, 2 opening, 3 closed,
    /// 4 closing. Told by the engine's door animation (`nvf-play` doors.rs).
    pub open_states: BTreeMap<FormId, u8>,
    /// What each drawn actor's body does (GetAnimAction, IsMoving's way, its movement
    /// group), told every frame (`Game::set_anim_state`, `crate::anim_state`).
    pub anim: BTreeMap<FormId, crate::anim_state::AnimState>,
    /// The crosshair's ray as the engine last cast it (PlaceAtReticle); `None` before the
    /// engine told it (a headless run).
    pub reticle: Option<Reticle>,
    /// The reference the player would activate (GetCrosshairRef) and the first one the
    /// crosshair's ray meets at any range (GetCrosshairRefEx), as the engine tells them.
    pub crosshair_ref: Option<FormId>,
    pub crosshair_far: Option<FormId>,
    /// The reference the player holds with the grab (`nvf-play` grab.rs), told by the
    /// engine ([`Game::set_player_grab`]); `None` while the player holds nothing. Not
    /// part of the saved state: a grab ends with the frame the game is saved in.
    pub grabbed: Option<FormId>,
    /// The player's exposure to radiation, measured each frame (`crate::radiation`); the
    /// water the player is in is told by the engine.
    pub radiation: crate::radiation::Exposure,
    /// The radiation zones explosions left (`crate::radiation`; not saved [design]).
    pub rad_zones: Vec<crate::radiation::RadZone>,
    pub rad_serial: u32,
    /// What each actor last activated and when ([`Game::time`]), for IsWaterObject.
    pub activating: BTreeMap<FormId, (FormId, f64)>,
}

/// The ray under the crosshair, game space: where it starts (about the player's eye,
/// `nvf_play::control::Crosshair::ray`), its unit direction and the distance to the first
/// thing it hits within [`RETICLE_REACH`] (`None`: nothing).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reticle {
    pub start: [f32; 3],
    pub dir: [f32; 3],
    pub hit: Option<f32>,
}

/// How far the engine casts the crosshair's ray for PlaceAtReticle: the largest
/// maxDistance the function takes [documented: GECK wiki PlaceAtReticle, "1-50000"].
pub const RETICLE_REACH: f32 = 50000.0;

impl Reticle {
    /// The point the crosshair hits, when it hits within `max` units.
    pub fn point(&self, max: f32) -> Option<[f32; 3]> {
        let d = self.hit.filter(|&d| d <= max)?;
        Some([self.start[0] + self.dir[0] * d, self.start[1] + self.dir[1] * d, self.start[2] + self.dir[2] * d])
    }
}

impl Default for Senses {
    fn default() -> Senses {
        Senses { motion: BTreeMap::new(), first_person: true, always_subtitles: BTreeSet::new(), looks: BTreeMap::new(), weapon_told: BTreeMap::new(), open_states: BTreeMap::new(), anim: BTreeMap::new(), reticle: None, crosshair_ref: None, crosshair_far: None, grabbed: None, radiation: Default::default(), rad_zones: Vec::new(), rad_serial: 0, activating: BTreeMap::new() }
    }
}

/// A script's Look on one actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub target: FormId,
    /// The whole body turns to the target, not only the head (Look's second parameter).
    pub rotate_body: bool,
}

impl Game<'_> {
    /// The engine tells how an actor moves (the player included).
    pub fn set_motion(&mut self, actor: FormId, m: Motion) {
        if self.senses.motion.get(&actor) != Some(&m) {
            self.senses.motion.insert(actor, m);
        }
    }

    /// The actor left the engine's view (unloaded, disabled, dead): forget its motion.
    pub fn forget_motion(&mut self, actor: FormId) {
        self.senses.motion.remove(&actor);
    }

    /// The engine tells what the player holds with the grab (docs/spec-notes/grab.md §4).
    /// A change runs the OnRelease blocks of the reference let go, then the OnGrab
    /// blocks of the one taken [documented: GECK wiki block-type list (OnGrab,
    /// OnRelease) and the extender's OnPlayerGrab / OnPlayerRelease pages ("when the
    /// player picks up (Control Code 27) / releases (drops the grabbed ref)"); observed:
    /// codes 0x21 and 0x22, VCG01BrahminSkullSCRIPT sets CGTutorial stages 22 and 24 from
    /// them, script-functions.md §3.3]. The order of a swap within one frame is
    /// [hypothesis]; one grab key never swaps.
    /// Returns which ran: (OnRelease, OnGrab).
    pub fn set_player_grab(&mut self, now: Option<FormId>) -> (bool, bool) {
        let before = self.senses.grabbed;
        if before == now {
            return (false, false);
        }
        self.senses.grabbed = now;
        let player = Some(FormId::PLAYER_REF);
        let released = before.is_some_and(|r| self.ref_event(r, crate::game::event::ON_RELEASE, None, player));
        let grabbed = now.is_some_and(|r| self.ref_event(r, crate::game::event::ON_GRAB, None, player));
        (released, grabbed)
    }

    /// The engine tells whether the camera is in first person.
    pub fn set_first_person(&mut self, on: bool) {
        self.senses.first_person = on;
    }

    pub fn motion(&self, actor: FormId) -> Motion {
        self.senses.motion.get(&actor).copied().unwrap_or_default()
    }

    /// Whether the actor's lines show their subtitle out of hearing too.
    pub fn always_subtitles(&self, actor: FormId) -> bool {
        self.senses.always_subtitles.contains(&actor)
    }

    /// The reference a script holds the actor's head on (Look), if any.
    pub fn script_look(&self, actor: FormId) -> Option<Look> {
        self.senses.looks.get(&actor).copied()
    }

    /// The actors a script's Look turns whole (`rotateBody` 1), each with its target, in
    /// reference order: the body faces the target until StopLook (`fnv_ai` turns those
    /// it runs, the engine's `script_look.rs` the others).
    pub fn body_looks(&self) -> Vec<(FormId, FormId)> {
        self.senses.looks.iter().filter(|(_, l)| l.rotate_body).map(|(a, l)| (*a, l.target)).collect()
    }
}

/// `actor.IsSneaking`: 1 while the actor is sneaking (crouched) [documented: GECK wiki
/// IsSneaking]. The engine tells it ([`Game::set_motion`]): the player's sneak control,
/// and for other actors [`Game::sneaks`] (SetForceSneak, a package's Always Sneak).
/// An actor the engine does not measure (not drawn, far) sneaks by the same rule
/// ([`Game::sneaks`]).
pub fn is_sneaking(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(g.is_sneaking(r))
}

/// `actor.IsMoving`: 0 not moving, 1 forward, 2 backward, 3 left, 4 right: the
/// direction the actor is "requesting to move" in [documented: GECK wiki IsMoving]. The
/// engine tells the way each frame (`crate::anim_state`, [`Game::anim_state`]): the
/// player's movement keys, an NPC's walk in its own axes. An actor with no fresh
/// snapshot that the engine still measures moving reads 1 [hypothesis, as before the
/// way was told]; 0 at rest [observed: probes].
pub fn is_moving(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    let code = match g.anim_state(r) {
        Some(s) => s.moving.code(),
        None => u8::from(g.motion(r).moving),
    };
    Ok(Value::Num(code as f64))
}

/// `actor.IsRunning`: 1 when the actor's speed mode is run rather than walk, "even if
/// they are standing still" [documented: GECK wiki IsRunning notes; observed: the player
/// standing still read 1 and Sunny Smiles standing still 0, capture `fa_batch1_queries`].
/// The player's mode is the Walk toggle (`Motion::run_mode`); another actor's is read
/// from the engine's measure, running while it moves at its run speed [hypothesis for
/// NPCs: their mode while standing is not told].
pub fn is_running(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    let m = g.motion(r);
    yes(m.running || m.run_mode)
}

/// How long after an activation the object counts as "being activated", game seconds
/// [design: about the length of an activation's idle].
pub const ACTIVATING_SECONDS: f64 = 2.0;

impl Game<'_> {
    /// `actor` activates `target` now (for IsWaterObject).
    pub fn note_activating(&mut self, actor: FormId, target: FormId) {
        let now = self.time;
        self.senses.activating.insert(actor, (target, now));
    }

    /// What `actor` is activating: the reference it activated within
    /// [`ACTIVATING_SECONDS`].
    pub fn activating(&self, actor: FormId) -> Option<FormId> {
        self.senses.activating.get(&actor).filter(|(_, at)| self.time - at <= ACTIVATING_SECONDS).map(|(t, _)| *t)
    }
}

/// `actor.IsWaterObject`: 1 if the object the actor is activating is placeable water
/// [documented: GECK wiki IsWaterObject, "Returns 1 if the object currently being
/// activated is Placeable Water. Used in vanilla game for drinking animation"]. Placeable
/// water is taken as an activator with a Water Type (ACTI WNAM: the sinks, toilets and
/// pumps the player drinks from, [`Game::drink_water`]) [hypothesis]; "being activated"
/// as activated within [`ACTIVATING_SECONDS`] [design]. Its one use is the condition of
/// the idle `ActWaterSource` [observed: FalloutNV.esm].
pub fn is_water_object(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::bool(g.activating(r).and_then(|t| g.drink_water(t)).is_some()))
}

/// `actor.IsSwimming`: 1 while the calling actor swims [documented: GECK wiki
/// IsSwimming]. The engine tells it with the rest of the movement ([`Game::set_motion`],
/// `nvforever` detection.rs): the swimming state of the actor's physics character, which
/// starts and stops as physics.md §6.6 describes. On the player it reads 0 whatever the
/// player does [documented: GECK wiki IsSwimming notes, "will always return 0 if called
/// on the player"; the extender's IsPlayerSwimming exists for that case]. An actor the
/// engine does not measure (not drawn, far) reads 0 [engineering choice: the same "not
/// measured is no" rule as IsMoving]. The shipped uses are idle conditions
/// (`IsSwimming 0` beside the flee idles, `crate::idles`) [observed: catalogue use
/// counts, 7 condition uses].
pub fn is_swimming(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(r != FormId::PLAYER_REF && g.motion(r).swimming)
}

/// `actor.GetLineOfSight target`: 1 when the calling actor can see the target: nothing
/// solid between them and within ±95° of where it looks [documented: GECK wiki
/// GetLineOfSight]. Read from the detection the engine last measured for the pair
/// (`crate::detection`, `Detection::seen`: line of sight and field of view): every two
/// actors of the loaded area within detection reach of each other (`crate::sight`). On
/// the player it is the camera that sees, from where the camera is and along its view
/// [documented: same page, "it returns true if the *camera* can see the object";
/// observed: player.GetLineOfSight Ringo 1, function probes fp1, 2026-10-17]. A pair not
/// measured reads 0 [engineering choice]. A target that is not an actor is not
/// measured: 0 [documented: on a non-player actor GetLOS returns false for anything but
/// an actor; on the player the original also tests other objects, which NVForever does
/// not yet].
pub fn get_line_of_sight(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let target = a.form(0)?;
    yes(g.detection.get(r, target).is_some_and(|d| d.seen))
}

/// `IsPlayerGrabbedRef ref`: 1 while the player holds that reference with the grab
/// [documented: GECK wiki IsPlayerGrabbedRef, "returns true if the player is currently
/// grabbing the selected reference"]; the engine tells what is held
/// ([`Game::set_player_grab`]). 0 at rest [observed: function probes, script-function-
/// coverage.md]. The shipped uses ask it of corpses carried into a trigger
/// (VMS18CarlyleStClairREF, ProperBurialCorpseref) [observed: their scripts].
pub fn is_player_grabbed_ref(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = a.form(0)?;
    yes(g.senses.grabbed == Some(r))
}

/// `IsPC1stPerson`: 1 while the camera is in first person [documented: GECK wiki
/// IsPC1stPerson]; the engine tells it ([`Game::set_first_person`]).
pub fn is_pc_1st_person(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.senses.first_person)
}

/// `actor.AlwaysShowActorSubtitles toggle`: 1 shows the actor's subtitles always, 0
/// back to normal [documented: catalogue summary]. "Always" is taken to mean also out
/// of hearing, where a line otherwise shows none (`nvf_play::voice::audible`)
/// [hypothesis: the original's "normal" also hides subtitles by distance].
pub fn always_show_actor_subtitles(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if a.num_or(0, 0.0) != 0.0 {
        g.senses.always_subtitles.insert(r);
    } else {
        g.senses.always_subtitles.remove(&r);
    }
    none()
}

/// `actor.IsTurning`: 0 not turning, 1 turning left, 2 turning right [documented: GECK
/// wiki IsTurning]. The engine tells it with the rest of the movement
/// ([`Game::set_motion`]; `nvforever` `detection.rs` measures the heading's change).
pub fn is_turning(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    Ok(Value::Num(g.motion(r).turning.min(2) as f64))
}

/// `IsPlayerMovingIntoNewSpace`: 1 while the player is moving into a new space, through a
/// load door or by fast travel [documented: GECK wiki IsPlayerMovingIntoNewSpace]: while
/// the engine holds the game under a loading screen ([`Game::loading`], set by
/// `nvf_play::script` from the travel's frozen frames). Scripts do not run then, so a
/// script reads 0 in its own blocks, as the page's note warns [documented: "may only
/// work within a MenuMode, OnLoad or similar block"]; package conditions evaluated during
/// the load see 1.
pub fn is_player_moving_into_new_space(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.loading)
}

/// `actor.SetForceSneak flag`: forced sneaking on (non-zero) or off [documented: GECK
/// wiki SetForceSneak]. The engine crouches the actor and walks it at the sneaking pace
/// while it holds ([`Game::sneaks`]).
pub fn set_force_sneak(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().force_sneak = a.num_or(0, 0.0) != 0.0;
    none()
}

impl Game<'_> {
    /// Whether `actor`'s current package (as last chosen) has the PKDT general flag `flag`
    /// (`crate::package::flags`).
    pub fn package_has(&self, actor: FormId, flag: u32) -> bool {
        self.extra.get(&actor).and_then(|e| e.current_package).and_then(|p| self.package_def(&p)).is_some_and(|d| d.has(flag))
    }

    /// Whether `actor`'s current package allows a Fallout behaviour (a
    /// `crate::package::fallout` bit): the bits count only on a package with "Skip
    /// Fallout Behavior" set (docs/spec-notes/packages.md §3); with no package, yes.
    pub fn package_allows(&self, actor: FormId, behaviour: u16) -> bool {
        let def = self.extra.get(&actor).and_then(|e| e.current_package).and_then(|p| self.package_def(&p));
        def.is_none_or(|d| !d.has(crate::package::flags::SKIP_FALLOUT_BEHAVIOR) || d.fallout_flags & behaviour != 0)
    }

    /// Whether the actor sneaks now, as IsSneaking reads it: what the engine measured this
    /// frame ([`Game::set_motion`]), else the rule of [`Game::sneaks`].
    pub fn is_sneaking(&self, actor: FormId) -> bool {
        self.senses.motion.get(&actor).map_or_else(|| self.sneaks(actor), |m| m.sneaking)
    }

    /// Whether the HUD's compass gives the actor a tick (docs/spec-notes/hud.md §11): alive,
    /// present and not sneaking [observed: the owner, 2026-10-23, "npc's sneaking should
    /// not show up on the compass"]. Range and bearing are the engine's to check.
    pub fn compass_shows(&self, actor: FormId) -> bool {
        !self.is_dead(actor) && self.present(actor) && !self.is_sneaking(actor)
    }

    /// Whether an actor other than the player sneaks: SetForceSneak holds, or its current
    /// package has "Always Sneak" ("Actor always sneaks during this package"
    /// [documented: GECK wiki Packages, Flags tab]). In a combat state an actor stands up
    /// to fight unless its package goes on during combat ("Continue During Combat")
    /// [hypothesis]. A teammate following the player sneaks while the player does
    /// [documented: GECK wiki SetPlayerTeammate].
    pub fn sneaks(&self, actor: FormId) -> bool {
        if actor == FormId::PLAYER_REF || self.extra.get(&actor).is_some_and(|e| e.dead) {
            return false;
        }
        // ITR's ForceCrouch and DisableCrouching over every other rule [documented:
        // ForceCrouch; DisableCrouching, "regardless of what the combat AI, follower
        // behaviour, or movement packages request"] (`crate::nvse_itr1`).
        if let Some(c) = self.itr_crouch(actor) {
            return c;
        }
        if self.extra.get(&actor).is_some_and(|e| e.force_sneak) {
            return true;
        }
        // Crouched to reload in a weapon use (`crate::use_weapon`).
        if self.weapon_uses.get(&actor).is_some_and(|w| w.crouched) {
            return true;
        }
        use crate::package::flags;
        let fighting = self.awareness.state(actor) == crate::awareness::State::Combat;
        // Crouched to fire by the combat AI (docs/design/combat-ai.md §14.2).
        if fighting && self.awareness.crouching.contains(&actor) {
            return true;
        }
        // A person hiding at its flee spot, in any combat state (§12.2.1).
        if self.at_flee_spot(actor) && self.world.placed.get(actor).is_some_and(|p| &p.sig == b"ACHR") {
            return true;
        }
        // A teammate following the player imitates the player's sneak state
        // (`crate::companion_wheel`); in a fight it stands up to fight [hypothesis, as for
        // the packages' Always Sneak].
        if !fighting && self.sneaks_with_player(actor) {
            return true;
        }
        self.package_has(actor, flags::ALWAYS_SNEAK) && (!fighting || self.package_has(actor, flags::CONTINUE_DURING_COMBAT))
    }
}

/// IsInDangerousWater: 1 when the player is in a water with "Causes Damage"
/// (`crate::radiation`); 0 on anyone else [documented: GECK wiki IsInDangerousWater;
/// observed: 0 in the Mojave's water, whose types lack the flag (physics.md); hypothesis:
/// only the player's water is known].
pub fn is_in_dangerous_water(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject_or_player();
    yes(r == FormId::PLAYER_REF && g.in_dangerous_water())
}

/// GetRadiationLevel: the radiation level of the player's surroundings, rads a second
/// after Rad Resistance (`crate::radiation`, `Exposure::resisted`); 0 on any other actor
/// [documented: GECK wiki GetRadiationLevel and GetRadiationLevelAlt; observed: the
/// original's 9.013784 in the test site's shack is the marker's level there times
/// (1 − 8 ÷ 100) for Rad Resistance 8, issue #894].
pub fn get_radiation_level(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(if r == FormId::PLAYER_REF { g.radiation().resisted } else { 0.0 }))
}
