//! Console functions the catalogue lists without an opcode (docs/design/console.md):
//! the console types them by name, and the function table gives each a number of ours
//! (`fnv_script::table::CONSOLE_OPCODE_BASE`), never in compiled scripts. They are registered
//! here by catalogue name ([`super::console_registry`]).
//!
//! The world and render toggles: the game's own (actors' AI, detection of the player,
//! scripts) are switched here and read by the AI, detection and [`Game::tick`]; the
//! engine's (collision, god mode, the drawing of grass, sky, water, trees, distant land
//! and navmesh, head tracking, faces' emotions) are asked of the engine in [`Toggles::asked`], which switches them and
//! prints their new state in the console.

use super::{yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// A console toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    /// `tcl`: the player passes through everything and flies where they look.
    Collision,
    /// `tai`: every actor's AI ([`Toggles::actors_ai_off`]).
    ActorsAI,
    /// `tdetect`: nobody detects the player ([`Toggles::detection_off`]).
    Detection,
    /// `tgm`.
    GodMode,
    /// `tscr`: per-frame script blocks ([`Toggles::scripts_off`]).
    Scripts,
    Grass,
    Sky,
    Water,
    Trees,
    /// `tlv`: the SpeedTrees' leaves alone.
    Leaves,
    LodLand,
    Navmesh,
    /// xNVSE's Con_TFIK (ToggleFootIK): feet placed on the ground, as the settings page
    /// switches them.
    FootIk,
    /// `tlik`: actors turn their heads to look at others (head tracking, as the settings
    /// page switches it).
    LookIk,
    /// `temo`: faces show emotions (a line's emotion, the expressions and moods other
    /// systems and MFG ask for).
    Emotions,
}

/// The console toggles' state on the game's side, and those asked of the engine.
/// Not saved: a load starts with every toggle as it is at a game's start [design].
#[derive(Debug, Clone, Default)]
pub struct Toggles {
    /// ToggleActorsAI: no actor's AI runs, on top of each actor's SetActorsAI
    /// ([`Game::ai_off`]).
    pub actors_ai_off: bool,
    /// ToggleDetection: nobody perceives the player.
    pub detection_off: bool,
    /// ToggleScripts: no GameMode or MenuMode block runs; event blocks (OnActivate,
    /// OnLoad, …) still do [hypothesis: the catalogue says "all scripts"; the events a
    /// console tester causes are kept].
    pub scripts_off: bool,
    /// God mode, as `tgm` last left it (GetDebugModeState 0).
    pub god_mode: bool,
    /// Toggles asked since the engine last looked, in order, for it to switch and report.
    pub asked: Vec<Toggle>,
}

impl Game<'_> {
    /// Whether an actor's AI is off: by SetActorsAI 0, or for everyone by ToggleActorsAI.
    pub fn ai_off(&self, actor: FormId) -> bool {
        self.toggles.actors_ai_off || self.extra.get(&actor).is_some_and(|e| e.ai_off)
    }
}

fn ask(g: &mut Game, t: Toggle) -> Result<Value, String> {
    g.toggles.asked.push(t);
    Ok(Value::ZERO)
}

pub fn toggle_collision(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::Collision)
}

/// [hypothesis: every actor's AI, whichever reference is selected, as the console's
/// `tai` behaves when played; the catalogue's summary speaks of the calling actor.]
pub fn toggle_actors_ai(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    g.toggles.actors_ai_off = !g.toggles.actors_ai_off;
    ask(g, Toggle::ActorsAI)
}

/// 1 when the calling actor's AI is off (SetActorsAI 0 or ToggleActorsAI), else 0.
pub fn is_actors_ai_off(g: &mut Game, ctx: &Ctx, _: &Args) -> Result<Value, String> {
    let a = ctx.subject()?;
    yes(g.ai_off(a))
}

pub fn toggle_detection(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    g.toggles.detection_off = !g.toggles.detection_off;
    ask(g, Toggle::Detection)
}

pub fn toggle_god_mode(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    g.toggles.god_mode = !g.toggles.god_mode;
    ask(g, Toggle::GodMode)
}

pub fn toggle_scripts(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    g.toggles.scripts_off = !g.toggles.scripts_off;
    ask(g, Toggle::Scripts)
}

pub fn toggle_grass(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::Grass)
}

pub fn toggle_sky(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::Sky)
}

pub fn toggle_water_system(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::Water)
}

pub fn toggle_trees(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::Trees)
}

/// `ToggleLeaves` (`tlv`): "disables/enables all leaves of speedtrees" [documented: GECK
/// wiki ToggleLeaves]; the engine hides the leaf cards, trunks and branches stay.
pub fn toggle_leaves(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::Leaves)
}

pub fn toggle_lod_land(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::LodLand)
}

pub fn toggle_navmesh(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::Navmesh)
}

/// `tlik`: "Disables/enables actors moving their heads to face other actors" [documented:
/// GECK wiki ToggleLookIK]; the engine switches its head tracking.
pub fn toggle_look_ik(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::LookIk)
}

/// `temo`: "Disables/enables facial emotions of all actors" [documented: GECK wiki
/// ToggleEmotions]; the engine's faces leave out expressions and moods while off.
pub fn toggle_emotions(g: &mut Game, _: &Ctx, _: &Args) -> Result<Value, String> {
    ask(g, Toggle::Emotions)
}
