//! Editor mode (NVForever's own; docs/design/editor.md §6–§7): `editor` typed in the
//! console switches it on and off. It has no key of its own (owner 2026-11-01).
//!
//! - **Locked by the INI**: only with `editor=1` under `[Settings]` in the home's
//!   `NVForever.ini` (owner 2026-11-01; read when `editor` is typed, not a row of the
//!   settings page); otherwise the console says how to turn it on.
//! - **On**: the console closes, the world stands still (the free camera's mode 1,
//!   `crate::free_cam`: no game logic, no game clock), the HUD hides and the mouse
//!   pointer stays free. The camera flies with W/A/S/D, Q down, E up, Shift faster, the
//!   wheel for its speed; it looks round only while the right mouse button is held, so
//!   the left button is free for picking.
//! - **Selecting**: a left click picks the reference under the pointer the way the
//!   console's click does (the collision ray, else the render bounds;
//!   `crate::console::pick_under`); clicking it again unselects it. A box is drawn
//!   around it and its details are shown in the panel.
//! - **Dragging** (§7.2): a left-button press on a reference selects it, and moving the
//!   mouse while the button is held (4 pixels or more) drags it: it slides on the
//!   horizontal plane at its own height, keeping the point that was grabbed under the
//!   pointer; with Shift held at the press it moves up and down instead (on the upright
//!   plane through it that faces the camera); with Ctrl held its new place snaps to the
//!   16-unit grid. A drag is one undo step. A press and release without moving on the
//!   selection unselects it, as a click did before.
//! - **Setting down** (§7.2, snap to surface): End or `editor drop` lowers or raises the
//!   selection so the bottom of its drawn bounds rests on the first collision straight
//!   below them (its own collision left out). One undo step.
//! - **Moving**: the arrow keys move the selection along the world's X and Y axes, the
//!   one nearest to the camera's view (up: away from the camera); Page Up and Page Down
//!   move it up and down; Ctrl with Left and Right turns it about Z; 16 units or 15° a
//!   step, Alt for 1 unit or 1° [design]. A move goes through the script functions
//!   SetPos and SetAngle (`fnv_game::functions::refs`) as a console line would, so the
//!   world draws it at once. The references moved this session are counted in the panel.
//! - **The record picker** (§7.3, console form): `editor find <words>` lists the
//!   placeable base records whose name or Editor ID holds every word (the console's
//!   record index, `help`'s), named ones first, numbered; `editor place #<n>` places the
//!   n-th, `editor place <Editor ID>` one by its exact Editor ID. Only the types in
//!   [`PLACEABLE`]: actor bases (NPC_, CREA, LVLN, LVLC) are refused, as is any other
//!   record, because a new reference is written as a REFR.
//! - **Placing**: `editor place <base FormID>` makes a new reference of that base in the
//!   run where the screen's centre points (the first collision along the camera's ray,
//!   else 512 units ahead), in the player's cell (outdoors: the exterior cell under it),
//!   and selects it; P then places another of the same base under the mouse pointer. Saved,
//!   it is a new reference of the plugin (`fnv_esm::edit::NewRef`).
//! - **Deleting**: Delete disables the selection in the run; saved, it is written
//!   Initially Disabled and moved far below the ground, the community's way of removing
//!   another plugin's reference without the Deleted flag [hypothesis: xEdit's "Undelete
//!   and Disable References" practice; the design notes §4.5 say why]. A reference this
//!   session placed is simply not saved. A reference that follows an enable parent stays
//!   as its parent has it in the run (Disable does nothing to it), which the console says;
//!   it is still written Initially Disabled [design].
//! - **Undo**: Ctrl+Z puts the last move, turn, delete or placing back, one step at a time.
//! - **From the console** (and test aids): `editor drag <x0> <y0> <x1> <y1>` drags the
//!   selection as the mouse would between those points of the screen (fractions of its
//!   width and height), `editor drop` sets it down; `editor select <FormID>`, `editor move <dx>
//!   <dy> <dz>` (world axes, game units), `editor turn <degrees>` (about Z), `editor
//!   place <base>`, `editor delete` and `editor undo` do what a click and the keys do.
//! - **The target plugin** (§6.1, through the console until editor mode has a UI):
//!   `editor open` lists the plugins it could open (the loaded `.esp` files and those in
//!   NVForever's mods folder, with what each holds); `editor open <Name.esp>` makes that
//!   plugin the target, refusing what a save would refuse (a file of that name not loaded,
//!   a plugin holding records other than placed references). A name no file has yet is a
//!   new plugin. The target is remembered as `Target=` under `[Editor]` in the home's
//!   `NVForever.ini` and opened again on the first `editor` of the next run; the panel
//!   shows it. `editor save` without a name writes into it; with a name, that plugin
//!   becomes the target.
//! - **Saving**: `editor save <Name.esp>` in the console writes the moved references, at
//!   their places in the run, into that plugin in NVForever's mods folder
//!   (`<home>\mods\`, read before Data: `fnv_install::plugins`) with `fnv_esm::edit`,
//!   and turns it on in the home's `plugins.txt`, so the next start loads it. A plugin of
//!   that name that is loaded now keeps its references (those moved again take their new
//!   places); one that exists but is not loaded is not overwritten. The file it replaces
//!   is kept as `<name>.esp.bak`.
//! - **Saving under Mod Organizer 2** (owner 2026-11-01): when MO2 started the game, the
//!   save goes into the file of the enabled mod that gives `Data` the plugin (the
//!   overwrite folder first, then the mods from the highest priority down), else into
//!   MO2's overwrite folder; it is turned on in the MO2 profile's `plugins.txt` and
//!   `loadorder.txt`, not NVForever's list; the replaced file is kept in
//!   `<home>\editor-backups\` (`fnv_install::mo2::Layout`).
//! - **Off**: `editor` again: the camera returns to the player, the HUD comes back as it
//!   was, the world goes on.
//!
//! Test aid `NVF_PLAY_TEST_EDITOR=1`: editor mode is allowed without the INI line (a
//! smoke home has no INI). `NVF_PLAY_TEST_EDITOR_REPLACE=1`: `editor save` replaces a plugin of that name
//! in the mods folder that is not loaded (a smoke home kept from an earlier run), keeping
//! the `.bak` as always. `NVF_PLAY_TEST_EDITOR_MO2=<MO2 instance folder>`: save as if that
//! MO2 instance had started the game (sessions cannot start MO2); a folder without a
//! `ModOrganizer.ini` gets one naming profile `Smoke`.
//!
//! Log: `editor: on`, `editor: off`, `editor: selected <ref>`, `editor: moved <ref> …`.

use crate::console::{Console, ConsoleCall};
use crate::phys::PhysState;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::ui_pointer::PointerState;
use crate::{space, PlaySet, PlayWorld, PlayerCamera, RefVisual};
use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_sim::FormId;
use std::collections::BTreeSet;

/// How far a click picks, game units (the console's reach).
const REACH: f32 = 20_000.0;
/// A move step and a fine one, game units [design: a power of two common in modular kits].
const STEP: f32 = 16.0;
const FINE_STEP: f32 = 1.0;
/// How far ahead a new reference goes when the camera's ray hits nothing, game units [design].
const PLACE_AHEAD: f32 = 512.0;
/// Where a deleted reference is written, game units below the origin [hypothesis: the
/// height the community's tools sink disabled references to].
const SUNK_Z: f32 = -30_000.0;
/// A turn step and a fine one, degrees [design].
const TURN: f32 = 15.0;
const FINE_TURN: f32 = 1.0;

/// Editor mode's state.
#[derive(Resource, Default)]
pub struct Editor {
    pub on: bool,
    /// The console asked for a switch (`editor`).
    pub asked: bool,
    pub selected: Option<u32>,
    /// The references moved or turned this session.
    pub moved: BTreeSet<u32>,
    /// The references deleted this session (disabled in the run).
    pub deleted: BTreeSet<u32>,
    /// The references placed this session (made in the run, new in the plugin).
    pub created: BTreeSet<u32>,
    /// The base the last placing used (P places another).
    last_base: Option<u32>,
    /// Placings asked for: the base, and whether at the mouse pointer (else the screen's centre).
    to_place: Vec<(u32, bool)>,
    /// What Ctrl+Z puts back, newest last.
    history: Vec<Step>,
    /// `editor save <name>` asked for (empty: into the target plugin).
    pub save_asked: Option<String>,
    /// `editor open <name>` asked for (empty: list the plugins it could open).
    pub open_asked: Option<String>,
    /// The plugin saves go into (`editor open`, `editor save <name>`, else the last one
    /// used, from `[Editor] Target` in the home's INI).
    pub target: Option<String>,
    /// Whether this run has looked for the last target yet.
    target_looked: bool,
    /// `editor select`, `editor move`, `editor turn` asked for, in order.
    pub ops: Vec<Op>,
    /// `editor find <words>` asked for (answered when the record index is ready).
    pub find_asked: Option<String>,
    /// `editor place <#n | Editor ID>` asked for.
    pub place_named: Option<String>,
    /// The bases the last `editor find` listed, in its order (`#1` first).
    found: Vec<u32>,
    /// A left-button press on a reference, and the drag it may become.
    drag: Option<Drag>,
    /// Whether the HUD was hidden (`tm`) before editor mode hid it.
    menus_were_hidden: bool,
    text: String,
}

impl Editor {
    /// References moved (not deleted, not placed) and placed (not deleted) this session.
    fn counts(&self) -> (usize, usize) {
        let moved = self.moved.iter().filter(|id| !self.deleted.contains(id) && !self.created.contains(id)).count();
        (moved, self.created.difference(&self.deleted).count())
    }
}

/// One step Ctrl+Z can put back.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    /// A reference moved or turned from here (game units, radians).
    Placed { id: u32, pos: [f32; 3], rot: [f32; 3] },
    Deleted { id: u32 },
    Created { id: u32 },
}

/// What the console asked of editor mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Select(u32),
    /// Game units along the world's axes.
    Move([f32; 3]),
    /// Degrees about Z.
    Turn(f32),
    /// Delete the selection.
    Delete,
    /// Put the last step back.
    Undo,
    /// A new reference of this base.
    Place(u32),
    /// Drag the selection as the mouse would from one point of the screen to another
    /// (fractions of its width and height): the test form of a drag.
    Drag([f32; 4]),
    /// Set the selection down on what is below it (End).
    Drop,
}

impl Op {
    /// `select <FormID>`, `move <dx> <dy> <dz>`, `turn <degrees>` (the words after `editor`).
    pub fn parse(words: &str) -> Option<Result<Op, String>> {
        let mut w = words.split_whitespace();
        let verb = w.next()?.to_ascii_lowercase();
        let nums: Vec<&str> = w.collect();
        let f = |s: &str| s.parse::<f32>().map_err(|_| format!("{s:?} is not a number"));
        Some(match (verb.as_str(), &nums[..]) {
            ("select", [id]) => crate::console::parse_form_id(id).map(|f| Op::Select(f.0)).ok_or_else(|| format!("{id:?} is not a FormID")),
            ("select", _) => Err("editor select <FormID>".into()),
            ("move", [x, y, z]) => (|| Ok(Op::Move([f(x)?, f(y)?, f(z)?])))(),
            ("move", _) => Err("editor move <dx> <dy> <dz>: game units along the world's axes".into()),
            ("turn", [d]) => f(d).map(Op::Turn),
            ("turn", _) => Err("editor turn <degrees>: about the vertical axis".into()),
            ("place", [id]) if !id.starts_with('#') && crate::console::parse_form_id(id).is_some() => Ok(Op::Place(crate::console::parse_form_id(id)?.0)),
            ("place", []) => Err("editor place <#n from editor find | Editor ID | base FormID>".into()),
            ("place", _) => return None,
            ("delete", []) => Ok(Op::Delete),
            ("drop", []) => Ok(Op::Drop),
            ("drag", [a, b, c, d]) => (|| Ok(Op::Drag([f(a)?, f(b)?, f(c)?, f(d)?])))(),
            ("drag", _) => Err("editor drag <x0> <y0> <x1> <y1>: fractions of the screen's width and height".into()),
            ("undo", []) => Ok(Op::Undo),
            _ => return None,
        })
    }
}

#[derive(Component)]
struct EditorRoot;
#[derive(Component)]
struct EditorText;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Editor>().add_systems(Update, (switch, open, save, pick, set_down, nudge, find, place_new, describe, draw, outline).chain().in_set(PlaySet::Ui).after(crate::console::ConsoleSet));
    }
}

/// Whether the INI (or the test aid) allows editor mode.
fn allowed(file: Option<&crate::settings::SettingsFile>) -> bool {
    if std::env::var("NVF_PLAY_TEST_EDITOR").is_ok_and(|v| v == "1") {
        return true;
    }
    let text = file.and_then(|f| f.0.as_ref()).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    ini_allows(&text)
}

/// `editor=1` (or true, on, yes) under `[Settings]`.
fn ini_allows(text: &str) -> bool {
    fnv_install::ini_get(text, crate::settings::SECTION, "editor").is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes"))
}

/// `editor` in the console: on or off.
#[allow(clippy::too_many_arguments)]
fn switch(
    mut ed: ResMut<Editor>,
    script: Option<ResMut<ScriptState>>,
    mut free: ResMut<crate::free_cam::FreeCam>,
    mut commands: Commands,
    roots: Query<Entity, With<EditorRoot>>,
    mut console: ResMut<Console>,
    file: Option<Res<crate::settings::SettingsFile>>,
) {
    if !std::mem::take(&mut ed.asked) {
        return;
    }
    if !ed.on && !allowed(file.as_deref()) {
        let line = "Editor mode is locked: add editor=1 under [Settings] in NVForever.ini (in NVForever's home) and type editor again";
        info!("editor: {line}");
        console.say(line);
        return;
    }
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else {
        info!("editor: scripts are not running yet; editor mode waits for them");
        return;
    };
    let cam = &mut rt.game.ui.camera;
    if !ed.on {
        ed.on = true;
        ed.menus_were_hidden = cam.menus_hidden;
        cam.menus_hidden = true;
        // The free camera in mode 1 (the world stands still), taken by `free_cam` next frame.
        if !free.on {
            cam.fly_toggles.push(1);
        }
        free.look_on_button = true;
        spawn(&mut commands);
        info!("editor: on");
        if !std::mem::replace(&mut ed.target_looked, true) && ed.target.is_none() {
            ed.open_asked = remembered(file.as_deref()).map(|n| format!("{LAST}{n}"));
        }
    } else {
        ed.on = false;
        cam.menus_hidden = ed.menus_were_hidden;
        if free.on {
            cam.fly_toggles.push(1);
        }
        free.look_on_button = false;
        ed.selected = None;
        for e in &roots {
            commands.entity(e).despawn();
        }
        let (moved, placed) = ed.counts();
        info!("editor: off; {moved} reference(s) moved, {} deleted, {placed} placed this session", ed.deleted.len());
    }
}

/// Whether `rest` (the words after `editor`) starts with the word `verb`.
pub fn verb_is(rest: &str, verb: &str) -> bool {
    rest.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case(verb))
}

/// `editor why [<topic>] [all]`: why the selected actor says what it says to the player on a
/// topic (GREETING when none is named), every line in the order the game tries them, with
/// its conditions' live values (docs/design/editor.md §14, `fnv_game::debugger`).
pub fn why(game: &mut fnv_game::game::Game, speaker: Option<u32>, words: &str) -> Result<Vec<String>, String> {
    let speaker = FormId(speaker.ok_or("editor why: select an actor first (click it, or editor select <FormID>)")?);
    let mut w: Vec<&str> = words.split_whitespace().collect();
    let every = w.last().is_some_and(|l| l.eq_ignore_ascii_case("all"));
    if every {
        w.pop();
    }
    let topic = why_topic(game.data, &w.join(" "))?;
    let lines = game.explain_topic_text(speaker, Some(FormId::PLAYER_REF), topic, every);
    info!("editor: why {topic} for {speaker}: {} console line(s); {}", lines.len(), lines.last().map(String::as_str).unwrap_or_default());
    Ok(lines)
}

/// The topic `editor why` names: an Editor ID or a FormID, GREETING when empty.
fn why_topic(data: &fnv_game::GameData, words: &str) -> Result<FormId, String> {
    let w = words.trim();
    let topic = if w.is_empty() {
        data.greeting.ok_or("editor why: the load order has no GREETING topic")?
    } else {
        crate::console::parse_form_id(w).or_else(|| data.by_edid(w)).ok_or_else(|| format!("editor why: no topic {w:?}"))?
    };
    if !data.topic_defs.contains_key(&topic) && !data.topics.contains_key(&topic) {
        return Err(format!("editor why: {w} is not a topic (DIAL)"));
    }
    Ok(topic)
}

/// Marks a target looked up from the INI (so `open` says it is the last one used).
const LAST: &str = "\u{1}";

/// The section and key of the home's INI that remember the last target [design].
const INI_SECTION: &str = "Editor";
const INI_TARGET: &str = "Target";

/// The target remembered in the home's INI.
fn remembered(file: Option<&crate::settings::SettingsFile>) -> Option<String> {
    let text = std::fs::read_to_string(file?.0.as_ref()?).ok()?;
    fnv_install::ini_get(&text, INI_SECTION, INI_TARGET).filter(|v| !v.trim().is_empty())
}

/// Remember `name` as the target in the home's INI (other lines kept); nothing without one.
fn remember(file: Option<&crate::settings::SettingsFile>, name: &str) {
    let Some(path) = file.and_then(|f| f.0.as_ref()) else { return };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    if fnv_install::ini_get(&text, INI_SECTION, INI_TARGET).is_some_and(|v| v == name) {
        return;
    }
    if fnv_install::guard_write(path, None).is_ok() {
        if let Err(e) = std::fs::write(path, fnv_install::ini_set(&text, INI_SECTION, INI_TARGET, name)) {
            warn!("editor: could not remember the target in {}: {e}", path.display());
        }
    }
}

/// What a target plugin would be, or why it cannot be one: a loaded plugin that holds
/// only placed references (those editor mode writes so far), or a name no file has yet.
fn check_target(name: &str, d: &crate::PlayData, home: &fnv_install::EngineHome) -> Result<(String, String), String> {
    let name = plugin_name(name)?;
    let target = Target::choose(home, &name);
    let own = d.lo.plugins.iter().position(|p| p.name.eq_ignore_ascii_case(&name));
    let replace = std::env::var("NVF_PLAY_TEST_EDITOR_REPLACE").is_ok_and(|v| v == "1");
    let Some(pi) = own else {
        if target.exists && !replace {
            return Err(format!("{} exists but is not loaded: turn it on in {} and start again, or choose another name", target.path.display(), target.list_name(home)));
        }
        return Ok((name, format!("new; editor save writes it to {}", target.path.display())));
    };
    let (refs, other) = plugin_contents(d, pi);
    if let Some(sig) = other {
        return Err(format!("{name} holds {sig} records; editor mode writes only placed references so far"));
    }
    Ok((name, format!("loaded, {refs} reference(s) of its own; saves go to {}", target.path.display())))
}

/// A loaded plugin's placed references, and the first other kind of record it holds
/// (cells and worldspaces aside: they carry the references).
fn plugin_contents(d: &crate::PlayData, pi: usize) -> (usize, Option<String>) {
    let mut refs = 0;
    for r in &d.lo.records[pi] {
        match &r.header.sig {
            b"REFR" | b"ACHR" | b"ACRE" | b"PGRE" | b"PMIS" => refs += 1,
            b"CELL" | b"WRLD" => {}
            other => return (refs, Some(String::from_utf8_lossy(other).into_owned())),
        }
    }
    (refs, None)
}

/// The plugins `editor open` offers: loaded `.esp` files (and any loaded from NVForever's
/// mods folder), then the mods folder's plugins not loaded, each with what it holds.
fn open_list(d: &crate::PlayData, home: &fnv_install::EngineHome, mo2: bool) -> Vec<String> {
    let mods = home.mods();
    let mut lines = vec!["Plugins to open (editor open <Name.esp>; a new name makes a new plugin):".to_string()];
    for (pi, p) in d.lo.plugins.iter().enumerate() {
        let in_mods = !mo2 && mods.join(&p.name).is_file();
        if !p.name.to_ascii_lowercase().ends_with(".esp") && !in_mods {
            continue;
        }
        let what = match plugin_contents(d, pi) {
            (_, Some(sig)) => format!("holds {sig} records: not yet"),
            (refs, None) => format!("{refs} reference(s)"),
        };
        lines.push(format!("  {} (loaded, {what}){}", p.name, if in_mods { ", in the mods folder" } else { "" }));
    }
    if !mo2 {
        let mut idle: Vec<String> = std::fs::read_dir(&mods)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                (l.ends_with(".esp") || l.ends_with(".esm")) && !d.lo.plugins.iter().any(|p| p.name.eq_ignore_ascii_case(n))
            })
            .collect();
        idle.sort();
        for n in idle {
            lines.push(format!("  {n} (in the mods folder, not loaded: turn it on and start again)"));
        }
    }
    if lines.len() == 1 {
        lines.push("  none yet".into());
    }
    lines
}

/// `editor open [<name>]`: list the plugins, or make `name` the target.
fn open(mut ed: ResMut<Editor>, mut console: ResMut<Console>, play: Res<PlayWorld>, file: Option<Res<crate::settings::SettingsFile>>) {
    let Some(asked) = ed.open_asked.take() else { return };
    let Some(home) = fnv_install::EngineHome::user() else {
        console.say("No home folder for NVForever");
        return;
    };
    let (last, name) = match asked.strip_prefix(LAST) {
        Some(n) => (true, n.to_string()),
        None => (false, asked.trim().to_string()),
    };
    if name.is_empty() {
        let mo2 = Target::choose(&home, "-.esp").mo2.is_some();
        for l in open_list(&play.0, &home, mo2) {
            console.say(l);
        }
        return;
    }
    match check_target(&name, &play.0, &home) {
        Ok((name, what)) => {
            let note = if last { "; the last one used, editor open <Name.esp> to change it" } else { "" };
            info!("editor: target {name} ({what}){note}");
            console.say(format!("Target plugin: {name} ({what}){note}"));
            remember(file.as_deref(), &name);
            ed.target = Some(name);
        }
        Err(e) if last => {
            info!("editor: the last target {name} cannot be opened: {e}");
            console.say(format!("The last target plugin, {name}, cannot be opened ({e}): editor open <Name.esp> picks another"));
        }
        Err(e) => {
            info!("editor: not opened: {e}");
            console.say(format!("Not opened: {e}"));
        }
    }
}

/// `editor save <name>`: write the moved references into the plugin `name`.
fn save(mut ed: ResMut<Editor>, mut console: ResMut<Console>, play: Res<PlayWorld>, sim: Res<SimState>) {
    let Some(name) = ed.save_asked.take() else { return };
    let name = match (name.trim(), &ed.target) {
        ("", Some(t)) => t.clone(),
        ("", None) => {
            console.say("No target plugin: editor open <Name.esp> picks one (editor open lists them), or editor save <Name.esp>");
            return;
        }
        (n, _) => n.to_string(),
    };
    let place = |id: u32| live_place(&sim, id);
    let new: Vec<(u32, u32, Option<u32>)> = ed
        .created
        .difference(&ed.deleted)
        .filter_map(|&id| sim.world.peek(FormId(id)).map(|s| (id, s.base.0, s.cell.map(|c| c.0))))
        .collect();
    match save_plugin(&name, &ed.moved, &ed.deleted, &ed.created, &new, &play.0, place) {
        Ok(lines) => {
            for l in lines {
                console.say(l);
            }
            if let Ok(n) = plugin_name(&name) {
                ed.target = Some(n);
            }
        }
        Err(e) => console.say(format!("Not saved: {e}")),
    }
}

/// Where reference `id` is in the run: its state, else where the plugins place it. The
/// run's world is `SimState::world` outside the script frame, which swaps it into the
/// game while scripts run (`crate::script`).
fn live_place(sim: &SimState, id: u32) -> Option<([f32; 3], [f32; 3])> {
    match sim.world.peek(FormId(id)) {
        Some(s) => Some((s.pos, s.rot)),
        None => sim.refs.get(FormId(id)).map(|p| (p.pos, p.rot)),
    }
}

/// A plugin name for the mods folder: a file name ending in `.esp` or `.esm` (`.esp`
/// added when it has neither).
fn plugin_name(name: &str) -> Result<String, String> {
    let n = name.trim().trim_matches('"').trim();
    if n.is_empty() || n.contains(['/', '\\', ':', '*', '?', '<', '>', '|']) || n.starts_with('.') {
        return Err(format!("{n:?} is not a file name: editor save <Name.esp>"));
    }
    let l = n.to_ascii_lowercase();
    Ok(if l.ends_with(".esp") || l.ends_with(".esm") { n.to_string() } else { format!("{n}.esp") })
}

/// Where a save of a plugin goes: NVForever's mods folder, or under Mod Organizer 2 the
/// file of the mod that gives `Data` the plugin, else MO2's overwrite folder (owner
/// 2026-11-01; `fnv_install::mo2::Layout`).
struct Target {
    path: std::path::PathBuf,
    exists: bool,
    mo2: Option<fnv_install::mo2::Layout>,
    mod_name: Option<String>,
}

impl Target {
    fn choose(home: &fnv_install::EngineHome, name: &str) -> Target {
        let test = std::env::var_os("NVF_PLAY_TEST_EDITOR_MO2").filter(|v| !v.is_empty());
        let mo2 = match test {
            Some(dir) => {
                // A smoke run's instance: made with profile `Smoke` when it has no INI.
                let dir = std::path::Path::new(&dir);
                let ini = dir.join("ModOrganizer.ini");
                if !ini.exists() && std::fs::create_dir_all(dir).is_ok() {
                    let _ = std::fs::write(&ini, "[General]\r\nselected_profile=@ByteArray(Smoke)\r\n");
                }
                fnv_install::mo2::Layout::of_instance(dir)
            }
            None => fnv_install::mo2::Layout::current(),
        };
        match mo2 {
            Some(l) => {
                let t = l.save_target(name);
                Target { path: t.path, exists: t.exists, mod_name: t.mod_name, mo2: Some(l) }
            }
            None => {
                let path = home.mods().join(name);
                Target { exists: path.exists(), path, mo2: None, mod_name: None }
            }
        }
    }

    /// The copy kept of the file a save replaces: beside it in NVForever's mods folder,
    /// in `<home>\editor-backups\` under MO2 (so no mod folder gains a stray file).
    fn backup(&self, home: &fnv_install::EngineHome, name: &str) -> std::path::PathBuf {
        match &self.mo2 {
            Some(_) => home.root.join("editor-backups").join(format!("{name}.bak")),
            None => home.mods().join(format!("{name}.bak")),
        }
    }

    /// The plugin list the plugin is turned on in.
    fn list_name(&self, home: &fnv_install::EngineHome) -> String {
        match &self.mo2 {
            Some(l) => format!("MO2's profile {}", l.profile.display()),
            None => home.plugins_file().display().to_string(),
        }
    }
}

/// Write `moved` (at the places `place` gives) into plugin `name` in the mods folder and
/// turn it on; the lines say what was done.
/// `created` are the references placed this session (left out of the overrides), `new`
/// those of them to write: (reference, base, cell).
#[allow(clippy::too_many_arguments)]
fn save_plugin(
    name: &str,
    moved: &BTreeSet<u32>,
    deleted: &BTreeSet<u32>,
    created: &BTreeSet<u32>,
    new: &[(u32, u32, Option<u32>)],
    d: &crate::PlayData,
    place: impl Fn(u32) -> Option<([f32; 3], [f32; 3])>,
) -> Result<Vec<String>, String> {
    use fnv_esm::edit::{write_plugin, NewPlugin, NewRef, Placement};
    let name = plugin_name(name)?;
    let home = fnv_install::EngineHome::user().ok_or("no home folder for NVForever")?;
    let target = Target::choose(&home, &name);
    let path = target.path.clone();
    let own = d.lo.plugins.iter().position(|p| p.name.eq_ignore_ascii_case(&name));
    let replace = std::env::var("NVF_PLAY_TEST_EDITOR_REPLACE").is_ok_and(|v| v == "1");
    if target.exists && own.is_none() && !replace {
        return Err(format!("{} exists but is not loaded: turn it on in {} and start again, or choose another name", path.display(), target.list_name(&home)));
    }
    let mut edits: Vec<Placement> = Vec::new();
    // A loaded plugin of that name keeps its own references (where the run has them).
    if let Some(pi) = own {
        for r in &d.lo.records[pi] {
            let global = d.lo.to_global(pi, r.header.form_id).unwrap_or(r.header.form_id);
            match &r.header.sig {
                b"REFR" | b"ACHR" | b"ACRE" | b"PGRE" | b"PMIS" => {
                    if !moved.contains(&global) && !deleted.contains(&global) {
                        let (pos, rot) = place(global).ok_or_else(|| format!("{} of {name} has no place", FormId(global)))?;
                        edits.push(Placement { form_id: global, pos, rot, disabled: false });
                    }
                }
                b"CELL" | b"WRLD" => {}
                other => return Err(format!("{name} holds a {} record; editor mode writes only placed references so far", String::from_utf8_lossy(other))),
            }
        }
    }
    for &id in moved.iter().filter(|id| !deleted.contains(id) && !created.contains(id)) {
        let (pos, rot) = place(id).ok_or_else(|| format!("{} has no place", FormId(id)))?;
        edits.push(Placement { form_id: id, pos, rot, disabled: false });
    }
    for &id in deleted.difference(created) {
        let (pos, rot) = place(id).ok_or_else(|| format!("{} has no place", FormId(id)))?;
        edits.push(Placement { form_id: id, pos: [pos[0], pos[1], SUNK_Z], rot, disabled: true });
    }
    let mut news = Vec::with_capacity(new.len());
    for &(id, base, cell) in new {
        let (pos, rot) = place(id).ok_or_else(|| format!("{} has no place", FormId(id)))?;
        let cell = cell.ok_or_else(|| format!("{} placed in no cell", FormId(id)))?;
        news.push(NewRef { base, cell, pos, rot });
    }
    if edits.is_empty() && news.is_empty() {
        return Err("nothing to save: move, delete or place something first".into());
    }
    let w = write_plugin(d.lo, d.schema, &NewPlugin { name: name.clone(), author: None, description: Some("Made in NVForever's editor mode".into()) }, &edits, &news)?;
    fnv_install::guard_write(&path, None).map_err(|e| e.to_string())?;
    let dir = path.parent().ok_or("no folder for the plugin")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if path.exists() {
        let bak = target.backup(&home, &name);
        if let Some(b) = bak.parent() {
            std::fs::create_dir_all(b).map_err(|e| format!("{}: {e}", b.display()))?;
        }
        std::fs::copy(&path, &bak).map_err(|e| format!("{}: {e}", bak.display()))?;
    }
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, &w.bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    // Read back: say so when the file does not parse.
    if let Err(e) = fnv_esm::Plugin::open(&path) {
        return Err(format!("{} was written but does not read back: {e}", path.display()));
    }
    let ids: Vec<String> = w.new_load_ids.iter().map(|&f| FormId(f).to_string()).collect();
    info!("editor: saved {} references and {} new ({}) into {} ({} bytes, {} masters)", w.references, w.new_ids.len(), ids.join(" "), path.display(), w.bytes.len(), w.masters.len());
    let mut lines = vec![format!("Saved {} changed and {} new reference(s) into {} ({} bytes)", w.references, w.new_ids.len(), path.display(), w.bytes.len())];
    for warning in w.warnings.iter().take(10) {
        lines.push(format!("warning: {warning}"));
    }
    if let Some(m) = &target.mod_name {
        lines.push(format!("(MO2: into mod \"{m}\", the one that gives Data this plugin)"));
    } else if target.mo2.is_some() {
        lines.push("(MO2: into the overwrite folder)".into());
    }
    let on = match &target.mo2 {
        Some(l) => l.turn_on(&name).map_err(|e| e.to_string()),
        None => fnv_install::plugins::turn_on(&home, &name).map_err(|e| e.to_string()),
    };
    let list = target.list_name(&home);
    match on {
        Ok(true) => lines.push(format!("{name} is on in {list}: it loads from the next start")),
        Ok(false) => lines.push(format!("{name} was already on in {list}")),
        Err(e) => lines.push(format!("{name} could not be turned on in the plugin list: {e}")),
    }
    Ok(lines)
}

/// How far the pointer moves with the button held before a press becomes a drag, pixels [design].
const DRAG_START: f32 = 4.0;

/// A press on a reference and the drag it may become.
#[derive(Debug, Clone, Copy)]
struct Drag {
    id: u32,
    /// Where the pointer was pressed, window pixels.
    press: Vec2,
    /// The reference's place when pressed (game units, radians).
    pos: [f32; 3],
    rot: [f32; 3],
    /// The grabbed point on the drag's plane, game units.
    grab: [f32; 3],
    /// Up and down (Shift at the press) rather than across.
    vertical: bool,
    /// The camera's horizontal forward at the press (the upright plane's normal).
    facing: [f32; 2],
    /// Whether the selection was already this reference (a click without a drag then unselects it).
    was_selected: bool,
    dragging: bool,
}

/// Where the ray `eye` + t·`dir` (t > 0) meets the horizontal plane at height `z`, within reach.
fn ray_at_height(eye: [f32; 3], dir: [f32; 3], z: f32) -> Option<[f32; 3]> {
    if dir[2].abs() < 1e-6 {
        return None;
    }
    let t = (z - eye[2]) / dir[2];
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    (t > 0.0 && t * len <= REACH).then(|| [eye[0] + dir[0] * t, eye[1] + dir[1] * t, z])
}

/// Where the ray meets the upright plane through `through` whose normal is the horizontal
/// direction `facing`, within reach.
fn ray_at_upright(eye: [f32; 3], dir: [f32; 3], through: [f32; 3], facing: [f32; 2]) -> Option<[f32; 3]> {
    let denom = dir[0] * facing[0] + dir[1] * facing[1];
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = ((through[0] - eye[0]) * facing[0] + (through[1] - eye[1]) * facing[1]) / denom;
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    (t > 0.0 && t * len <= REACH).then(|| [eye[0] + dir[0] * t, eye[1] + dir[1] * t, eye[2] + dir[2] * t])
}

/// The point the drag's plane holds under a ray.
fn drag_point(d: &Drag, eye: [f32; 3], dir: [f32; 3]) -> Option<[f32; 3]> {
    if d.vertical {
        ray_at_upright(eye, dir, d.pos, d.facing)
    } else {
        ray_at_height(eye, dir, d.pos[2])
    }
}

/// The dragged reference's new place for a pointer ray: moved by as much as the grabbed
/// point, across or up and down only, on the grid when `snap`.
fn drag_place(d: &Drag, eye: [f32; 3], dir: [f32; 3], snap: bool) -> Option<[f32; 3]> {
    let at = drag_point(d, eye, dir)?;
    let mut p = d.pos;
    let axes: &[usize] = if d.vertical { &[2] } else { &[0, 1] };
    for &i in axes {
        p[i] = d.pos[i] + at[i] - d.grab[i];
        if snap {
            p[i] = (p[i] / STEP).round() * STEP;
        }
    }
    Some(p)
}

/// A pointer ray in game space, from window pixels.
fn pointer_ray(cam: &Camera, cam_t: &GlobalTransform, at: Vec2) -> Option<([f32; 3], [f32; 3])> {
    let ray = cam.viewport_to_world(cam_t, at).ok()?;
    let eye = space::to_game(ray.origin);
    let p = space::to_game(ray.origin + *ray.direction);
    Some((eye, [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]]))
}

/// Start a press on reference `id` at window pixels `at`.
fn press_on(id: u32, at: Vec2, ray: ([f32; 3], [f32; 3]), place: ([f32; 3], [f32; 3]), vertical: bool, cam_fwd: [f32; 3], was_selected: bool) -> Option<Drag> {
    let h = (cam_fwd[0] * cam_fwd[0] + cam_fwd[1] * cam_fwd[1]).sqrt().max(1e-6);
    let mut d = Drag { id, press: at, pos: place.0, rot: place.1, grab: [0.0; 3], vertical, facing: [cam_fwd[0] / h, cam_fwd[1] / h], was_selected, dragging: false };
    d.grab = drag_point(&d, ray.0, ray.1)?;
    Some(d)
}

/// Send the reference to `to` (only the axes that changed) through SetPos.
fn set_pos(st: &mut ScriptState, id: u32, from: [f32; 3], to: [f32; 3]) {
    for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
        if (to[i] - from[i]).abs() > 1e-3 {
            st.console.push(ConsoleCall { line: format!("SetPos {axis} {}", to[i]), selected: Some(id), active_quest: None });
        }
    }
}

/// The end of a drag: one undo step, counted as moved.
fn end_drag(ed: &mut Editor, d: &Drag, to: [f32; 3]) {
    ed.history.push(Step::Placed { id: d.id, pos: d.pos, rot: d.rot });
    ed.moved.insert(d.id);
    info!("editor: dragged {} by ({:.0}, {:.0}, {:.0})", FormId(d.id), to[0] - d.pos[0], to[1] - d.pos[1], to[2] - d.pos[2]);
}

/// A left press selects the reference under the pointer and may become a drag (module notes).
#[allow(clippy::too_many_arguments)]
fn pick(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ed: ResMut<Editor>,
    console: Res<Console>,
    free: Res<crate::free_cam::FreeCam>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    phys: Option<ResMut<PhysState>>,
    visuals: Query<(&RefVisual, &Aabb, &GlobalTransform, &InheritedVisibility)>,
    panel: Query<&PointerState, With<EditorRoot>>,
    sim: Res<SimState>,
    script: Option<ResMut<ScriptState>>,
) {
    let (Ok(window), Ok((cam, cam_t))) = (windows.single(), camera.single()) else { return };
    let mut st = script.filter(|s| s.rt.is_some());
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let fwd = {
        let (o, f) = (space::to_game(cam_t.translation()), space::to_game(cam_t.translation() + cam_t.forward().as_vec3()));
        [f[0] - o[0], f[1] - o[1], f[2] - o[2]]
    };
    // The test form: `editor drag x0 y0 x1 y1` drags the selection in one go.
    let (drags, rest): (Vec<Op>, Vec<Op>) = std::mem::take(&mut ed.ops).into_iter().partition(|o| matches!(o, Op::Drag(_)));
    ed.ops = rest;
    for op in drags {
        let Op::Drag([x0, y0, x1, y1]) = op else { continue };
        let px = |x: f32, y: f32| Vec2::new(window.width() * x, window.height() * y);
        let (Some(id), Some(st)) = (ed.selected, st.as_deref_mut()) else {
            info!("editor: nothing selected to drag");
            continue;
        };
        let Some(place) = live_place(&sim, id) else { continue };
        let (Some(r0), Some(r1)) = (pointer_ray(cam, cam_t, px(x0, y0)), pointer_ray(cam, cam_t, px(x1, y1))) else { continue };
        let Some(d) = press_on(id, px(x0, y0), r0, place, shift, fwd, true) else {
            info!("editor: the drag's start is off the plane");
            continue;
        };
        match drag_place(&d, r1.0, r1.1, ctrl) {
            Some(to) => {
                set_pos(st, id, d.pos, to);
                end_drag(&mut ed, &d, to);
            }
            None => info!("editor: the drag's end is off the plane"),
        }
    }
    if !ed.on {
        ed.drag = None;
        return;
    }
    let at = window.cursor_position();
    // Holding: a drag past the threshold follows the pointer.
    if let (Some(mut d), true, Some(at)) = (ed.drag, mouse.pressed(MouseButton::Left), at) {
        if !d.dragging && at.distance(d.press) >= DRAG_START {
            d.dragging = true;
        }
        if d.dragging {
            if let (Some(r), Some(st)) = (pointer_ray(cam, cam_t, at), st.as_deref_mut()) {
                if let (Some(to), Some(now)) = (drag_place(&d, r.0, r.1, ctrl), live_place(&sim, d.id)) {
                    set_pos(st, d.id, now.0, to);
                }
            }
        }
        ed.drag = Some(d);
    }
    if mouse.just_released(MouseButton::Left) {
        if let Some(d) = ed.drag.take() {
            if d.dragging {
                let to = at.and_then(|a| pointer_ray(cam, cam_t, a)).and_then(|r| drag_place(&d, r.0, r.1, ctrl)).or_else(|| live_place(&sim, d.id).map(|p| p.0)).unwrap_or(d.pos);
                if let Some(st) = st.as_deref_mut() {
                    if let Some(now) = live_place(&sim, d.id) {
                        set_pos(st, d.id, now.0, to);
                    }
                }
                end_drag(&mut ed, &d, to);
            } else if d.was_selected {
                ed.selected = None;
                info!("editor: unselected {}", FormId(d.id));
            }
        }
    }
    if console.open || !free.active || !mouse.just_pressed(MouseButton::Left) || panel.iter().any(|p| *p != PointerState::None) {
        return;
    }
    let Some(at) = at else { return };
    let Ok(ray) = cam.viewport_to_world(cam_t, at) else { return };
    let picked = crate::console::pick_under(ray.origin, *ray.direction, REACH, phys.map(|p| p.into_inner()), &visuals).picked;
    let Some(id) = picked else {
        info!("editor: nothing there to select");
        return;
    };
    let was_selected = ed.selected == Some(id);
    if !was_selected {
        ed.selected = Some(id);
        info!("editor: selected {}", FormId(id));
    }
    ed.drag = match (live_place(&sim, id), pointer_ray(cam, cam_t, at)) {
        (Some(place), Some(r)) => press_on(id, at, r, place, shift, fwd, was_selected),
        _ => None,
    };
}

/// End or `editor drop`: set the selection down on the first collision below its drawn
/// bounds (its own collision left out).
#[allow(clippy::too_many_arguments)]
fn set_down(
    keys: Res<ButtonInput<KeyCode>>,
    mut ed: ResMut<Editor>,
    console: Res<Console>,
    free: Res<crate::free_cam::FreeCam>,
    phys: Option<ResMut<PhysState>>,
    visuals: Query<(&RefVisual, &Aabb, &GlobalTransform)>,
    sim: Res<SimState>,
    script: Option<ResMut<ScriptState>>,
) {
    let mut asked = ed.ops.iter().filter(|o| matches!(o, Op::Drop)).count();
    ed.ops.retain(|o| !matches!(o, Op::Drop));
    if ed.on && !console.open && free.active && keys.just_pressed(KeyCode::End) {
        asked += 1;
    }
    if asked == 0 || !ed.on {
        return;
    }
    let Some(id) = ed.selected else {
        info!("editor: nothing selected to set down");
        return;
    };
    let (Some(mut st), Some(phys)) = (script.filter(|s| s.rt.is_some()), phys) else { return };
    let Some((pos, rot)) = live_place(&sim, id) else { return };
    // The bottom of the drawn bounds, game units (the lowest corner of every part's box).
    let mut bottom: Option<f32> = None;
    for (r, aabb, gt) in &visuals {
        if r.0 != id {
            continue;
        }
        let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        for k in 0..8 {
            let corner = c + Vec3::new(if k & 1 == 0 { -h.x } else { h.x }, if k & 2 == 0 { -h.y } else { h.y }, if k & 4 == 0 { -h.z } else { h.z });
            let z = space::to_game(gt.transform_point(corner))[2];
            bottom = Some(bottom.map_or(z, |b: f32| b.min(z)));
        }
    }
    let bottom = bottom.unwrap_or(pos[2]);
    let mut filter = fnv_phys::Filter::default();
    filter.exclude_layers.insert(fnv_phys::layers::TRIGGER);
    filter.exclude.push(fnv_phys::Key::Ref(id));
    let hits = phys.into_inner().ray_all([pos[0], pos[1], bottom + 1.0], [0.0, 0.0, -1.0], REACH, filter);
    let Some(h) = hits.first() else {
        info!("editor: nothing below {} to set it down on", FormId(id));
        return;
    };
    let to = [pos[0], pos[1], pos[2] + h.point[2] - bottom];
    let onto = match h.owner {
        fnv_phys::Owner::Terrain { .. } => "the ground".to_string(),
        fnv_phys::Owner::Ref { form_id, .. } => FormId(form_id).to_string(),
        _ => "something".to_string(),
    };
    set_pos(&mut st, id, pos, to);
    ed.history.push(Step::Placed { id, pos, rot });
    ed.moved.insert(id);
    info!("editor: set {} down by {:.0} onto {onto}", FormId(id), to[2] - pos[2]);
}

/// The world axis (game space, X or Y, with its sign) nearest to `dir`'s horizontal part.
fn nearest_axis(dir: [f32; 3]) -> [f32; 2] {
    if dir[0].abs() >= dir[1].abs() {
        [dir[0].signum(), 0.0]
    } else {
        [0.0, dir[1].signum()]
    }
}

/// The arrow keys, Page Up/Down and Ctrl+Left/Right move and turn the selection.
#[allow(clippy::too_many_arguments)]
fn nudge(
    keys: Res<ButtonInput<KeyCode>>,
    mut ed: ResMut<Editor>,
    mut console: ResMut<Console>,
    free: Res<crate::free_cam::FreeCam>,
    script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    let mut ops = std::mem::take(&mut ed.ops);
    let keys_on = ed.on && !console.open && free.active;
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if keys_on && keys.just_pressed(KeyCode::Delete) {
        ops.push(Op::Delete);
    }
    if keys_on && ctrl && keys.just_pressed(KeyCode::KeyZ) {
        ops.push(Op::Undo);
    }
    let Some(mut st) = script.filter(|s| s.rt.is_some()) else { return };
    let mut asked = ([0.0f32; 3], 0.0f32);
    let call = |st: &mut ScriptState, id: u32, line: String| st.console.push(ConsoleCall { line, selected: Some(id), active_quest: None });
    for op in ops {
        match op {
            Op::Select(id) => {
                ed.selected = Some(id);
                info!("editor: selected {}", FormId(id));
            }
            Op::Move(d) => (0..3).for_each(|i| asked.0[i] += d[i]),
            Op::Turn(t) => asked.1 += t,
            Op::Place(base) => ed.to_place.push((base, false)),
            Op::Drag(_) | Op::Drop => {}
            Op::Delete => {
                let Some(id) = ed.selected.take() else {
                    info!("editor: nothing selected to delete");
                    continue;
                };
                if let Some(parent) = sim.refs.get(FormId(id)).and_then(|p| p.enable_parent) {
                    let line = format!(
                        "{} follows the enable parent {}: in the run it stays as its parent has it; saved, it is written Initially Disabled",
                        FormId(id),
                        parent.reference
                    );
                    info!("editor: {line}");
                    console.say(line);
                }
                call(&mut st, id, "Disable".into());
                ed.deleted.insert(id);
                ed.history.push(Step::Deleted { id });
                info!("editor: deleted {}", FormId(id));
            }
            Op::Undo => match ed.history.pop() {
                Some(Step::Placed { id, pos, rot }) => {
                    for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                        call(&mut st, id, format!("SetPos {axis} {}", pos[i]));
                        call(&mut st, id, format!("SetAngle {axis} {}", rot[i].to_degrees()));
                    }
                    info!("editor: undid the move of {}", FormId(id));
                }
                Some(Step::Created { id }) => {
                    if sim.world.set_enabled(FormId(id), false).is_ok() {
                        if let Some(rt) = st.rt.as_mut() {
                            rt.game.emit(fnv_game::trace::EventKind::RefDisabled { reference: FormId(id), fade: false });
                        }
                    }
                    ed.created.remove(&id);
                    if ed.selected == Some(id) {
                        ed.selected = None;
                    }
                    info!("editor: undid the placing of {}", FormId(id));
                }
                Some(Step::Deleted { id }) => {
                    // Straight back on, not through Enable, which leaves a reference with
                    // an enable parent alone (`fnv_game::functions::refs::enable`); the
                    // engine draws it again on the event, as after Enable.
                    if sim.world.set_enabled(FormId(id), true).is_ok() {
                        if let Some(rt) = st.rt.as_mut() {
                            rt.game.emit(fnv_game::trace::EventKind::RefEnabled { reference: FormId(id), fade: false });
                        }
                    }
                    ed.deleted.remove(&id);
                    ed.selected = Some(id);
                    info!("editor: undid the delete of {}", FormId(id));
                }
                None => info!("editor: nothing to undo"),
            },
        }
    }
    let Some(id) = ed.selected.filter(|_| ed.on) else { return };
    let fine = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let (step, turn) = if fine { (FINE_STEP, FINE_TURN) } else { (STEP, TURN) };
    let Some((pos, rot)) = live_place(&sim, id) else { return };
    let Ok(cam) = camera.single() else { return };
    let fwd = {
        let (o, f) = (space::to_game(cam.translation()), space::to_game(cam.translation() + cam.forward().as_vec3()));
        nearest_axis([f[0] - o[0], f[1] - o[1], 0.0])
    };
    let right = [fwd[1], -fwd[0]];
    let (mut d, mut turned) = asked;
    for (k, dx, dy) in [(KeyCode::ArrowUp, fwd[0], fwd[1]), (KeyCode::ArrowDown, -fwd[0], -fwd[1]), (KeyCode::ArrowRight, right[0], right[1]), (KeyCode::ArrowLeft, -right[0], -right[1])] {
        if !keys_on || !keys.just_pressed(k) {
            continue;
        }
        match (ctrl, k) {
            (true, KeyCode::ArrowLeft) => turned -= turn,
            (true, KeyCode::ArrowRight) => turned += turn,
            (true, _) => {}
            _ => {
                d[0] += dx * step;
                d[1] += dy * step;
            }
        }
    }
    if keys_on && keys.just_pressed(KeyCode::PageUp) {
        d[2] += step;
    }
    if keys_on && keys.just_pressed(KeyCode::PageDown) {
        d[2] -= step;
    }
    if d == [0.0; 3] && turned == 0.0 {
        return;
    }
    let mut lines = Vec::new();
    for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
        if d[i] != 0.0 {
            lines.push(format!("SetPos {axis} {}", pos[i] + d[i]));
        }
    }
    if turned != 0.0 {
        lines.push(format!("SetAngle Z {}", (rot[2].to_degrees() + turned).rem_euclid(360.0)));
    }
    info!("editor: moved {} by ({:.0}, {:.0}, {:.0}), turned {turned:.0}°", FormId(id), d[0], d[1], d[2]);
    for line in lines {
        call(&mut st, id, line);
    }
    ed.history.push(Step::Placed { id, pos, rot });
    ed.moved.insert(id);
}

/// Make the references asked for (`editor place`, P) in the run and select the last.
#[allow(clippy::too_many_arguments)]
fn place_new(
    keys: Res<ButtonInput<KeyCode>>,
    mut ed: ResMut<Editor>,
    console: Res<Console>,
    free: Res<crate::free_cam::FreeCam>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    phys: Option<ResMut<PhysState>>,
    mut sim: ResMut<SimState>,
    script: Option<ResMut<ScriptState>>,
    play: Res<PlayWorld>,
) {
    if ed.on && !console.open && free.active && keys.just_pressed(KeyCode::KeyP) {
        match ed.last_base {
            Some(b) => ed.to_place.push((b, true)),
            None => info!("editor: P places another of the last base; place one first (editor place <base>)"),
        }
    }
    let asked = std::mem::take(&mut ed.to_place);
    if asked.is_empty() || !ed.on {
        return;
    }
    let (Ok(window), Ok((cam, cam_t))) = (windows.single(), camera.single()) else { return };
    let Some(mut st) = script.filter(|s| s.rt.is_some()) else { return };
    let mut phys = phys.map(|p| p.into_inner());
    for (base, at_pointer) in asked {
        if let Err(e) = play.0.lo.winner(base).ok_or_else(|| format!("{} is no record", FormId(base))).and_then(|(_, r)| placeable(&r.header.sig)) {
            info!("editor: {} not placed: {e}", FormId(base));
            continue;
        }
        let at = if at_pointer { window.cursor_position() } else { None }.unwrap_or(Vec2::new(window.width() / 2.0, window.height() / 2.0));
        let Ok(ray) = cam.viewport_to_world(cam_t, at) else { continue };
        let eye = space::to_game(ray.origin);
        let dir = {
            let p = space::to_game(ray.origin + *ray.direction);
            [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]]
        };
        let dist = phys
            .as_deref_mut()
            .and_then(|p| {
                let mut filter = fnv_phys::Filter::default();
                filter.exclude_layers.insert(fnv_phys::layers::TRIGGER);
                p.ray_all(eye, dir, REACH, filter).first().map(|h| h.distance)
            })
            .unwrap_or(PLACE_AHEAD);
        let pos = [eye[0] + dir[0] * dist, eye[1] + dir[1] * dist, eye[2] + dir[2] * dist];
        let cell = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.cell);
        let cell = cell.map(|c| sim.world.placed.exterior_cell_at(c, pos).unwrap_or(c));
        match sim.world.place(FormId(base), cell, pos, [0.0; 3], 1) {
            Ok(id) => {
                if let Some(rt) = st.rt.as_mut() {
                    rt.game.emit(fnv_game::trace::EventKind::Placed { reference: id, base: FormId(base), cell, pos });
                }
                ed.created.insert(id.0);
                ed.history.push(Step::Created { id: id.0 });
                ed.selected = Some(id.0);
                ed.last_base = Some(base);
                info!("editor: placed {} as {id} at ({:.0}, {:.0}, {:.0})", FormId(base), pos[0], pos[1], pos[2]);
            }
            Err(e) => info!("editor: {} cannot be placed: {e}", FormId(base)),
        }
    }
}

/// The base record types editor mode places as a new REFR (§7.3's list less the actors:
/// NPC_, CREA, LVLN and LVLC are placed as ACHR/ACRE, which the writer does not write
/// yet, and a REFR of an actor base would not load in the original game [design]).
pub const PLACEABLE: [&[u8; 4]; 22] = [
    b"STAT", b"MSTT", b"SCOL", b"FURN", b"DOOR", b"CONT", b"LIGH", b"ACTI", b"TACT", b"TERM", b"MISC", b"WEAP", b"ARMO", b"AMMO", b"ALCH", b"BOOK", b"NOTE", b"KEYM", b"IDLM", b"TREE", b"ADDN", b"SOUN",
];

/// How many matches `editor find` lists [design: a console screen].
const FIND_SHOWN: usize = 20;

/// Whether base record type `sig` can be placed, or why not.
fn placeable(sig: &[u8; 4]) -> Result<(), String> {
    if PLACEABLE.contains(&sig) {
        return Ok(());
    }
    let s = String::from_utf8_lossy(sig);
    Err(match sig {
        b"NPC_" | b"CREA" | b"LVLN" | b"LVLC" => format!("a {s} is placed as an actor (ACHR/ACRE), which editor mode does not write yet"),
        _ => format!("a {s} is not something editor mode places"),
    })
}

/// The placeable records of the index matching `terms`, things with a name first.
fn find_placeable<'a>(ix: &'a [crate::console::Found], terms: &[String]) -> Vec<&'a crate::console::Found> {
    let mut hits: Vec<&crate::console::Found> = ix.iter().filter(|f| PLACEABLE.contains(&&f.sig) && crate::console::matches(f, terms)).collect();
    hits.sort_by_key(|f| (f.name.is_none(), f.edid.as_ref().map(|e| e.len()).unwrap_or(usize::MAX)));
    hits
}

/// What `editor place <what>` names: `#n` from the last `editor find`, else an Editor ID
/// (exact, without case) of a placeable record.
fn resolve_place(what: &str, found: &[u32], ix: &[crate::console::Found]) -> Result<u32, String> {
    let w = what.trim();
    if let Some(n) = w.strip_prefix('#') {
        let n: usize = n.parse().map_err(|_| format!("{w:?}: #<number> from the last editor find"))?;
        return found.get(n.wrapping_sub(1)).copied().ok_or_else(|| format!("#{n}: the last editor find listed {} record(s)", found.len()));
    }
    let f = ix.iter().find(|f| f.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(w))).ok_or_else(|| format!("no record has the Editor ID {w:?}: editor find <words> searches by name"))?;
    placeable(&f.sig)?;
    Ok(f.id.0)
}

/// `editor find <words>` and `editor place <#n | Editor ID>`: answered once the console's
/// record index is ready (it is built on a thread when the console first opens).
fn find(mut ed: ResMut<Editor>, mut console: ResMut<Console>, index: Res<crate::console::SearchIndex>) {
    if ed.find_asked.is_none() && ed.place_named.is_none() {
        return;
    }
    let Some(ix) = index.ready.as_deref() else { return };
    if let Some(q) = ed.find_asked.take() {
        let terms = crate::console::query_terms(&q);
        let hits = find_placeable(ix, &terms);
        ed.found = hits.iter().take(FIND_SHOWN).map(|f| f.id.0).collect();
        info!("editor: find {q:?}: {} placeable record(s)", hits.len());
        if hits.is_empty() {
            console.say(format!("Nothing placeable is named like {q}"));
        } else {
            console.say(format!("{} placeable record(s) match {q} (editor place #<n> places one):", hits.len()));
            for (i, f) in hits.iter().take(FIND_SHOWN).enumerate() {
                let name = f.name.as_deref().map(|n| format!(" \"{n}\"")).unwrap_or_default();
                console.say(format!("  #{} {} {} {}{name}", i + 1, f.id, String::from_utf8_lossy(&f.sig), f.edid.as_deref().unwrap_or("-")));
            }
            if hits.len() > FIND_SHOWN {
                console.say(format!("  ... {} more: narrow the search", hits.len() - FIND_SHOWN));
            }
        }
    }
    if let Some(what) = ed.place_named.take() {
        match resolve_place(&what, &ed.found, ix) {
            Ok(base) => ed.to_place.push((base, false)),
            Err(e) => {
                info!("editor: not placed: {e}");
                console.say(format!("Not placed: {e}"));
            }
        }
    }
}

/// The selected reference's details for the panel.
fn describe(mut ed: ResMut<Editor>, play: Res<PlayWorld>, sim: Res<SimState>, mut script: Option<ResMut<ScriptState>>) {
    if !ed.on {
        return;
    }
    let mut text = String::from(
        "EDITOR MODE   (type `editor` in the console to leave)\n\
         Fly: W A S D, Q down, E up, Shift faster, wheel speed; hold the right mouse button to look\n\
         Click: select   Drag: move (Shift: up/down, Ctrl: 16-unit grid)   End: set down on what is below\n\
         Arrows: move   Page Up/Down: up/down   Ctrl+Left/Right: turn   Alt: fine steps   Delete: delete   Ctrl+Z: undo   P: place another\n\
         Console: editor find <words> lists things to place; editor place #<n> (or an Editor ID, or a base FormID) places one where the screen's centre points\n\
         Console: editor open <Name.esp> picks the plugin to save into (editor open lists them); editor save writes into it\n\
         Console: editor why [topic] [all] shows why the selected actor says what it says (GREETING when no topic is named)\n",
    );
    match &ed.target {
        Some(t) => text.push_str(&format!("Target plugin: {t}\n")),
        None => text.push_str("Target plugin: none yet (editor open <Name.esp>)\n"),
    }
    match ed.selected {
        Some(id) => {
            let lines = crate::console::describe(id, &play.0, &sim, script.as_deref_mut().and_then(|s| s.rt.as_mut()));
            text.push_str(&lines.join("\n"));
        }
        None => text.push_str("Nothing selected"),
    }
    let (moved, placed) = ed.counts();
    text.push_str(&format!("\n{moved} reference(s) moved, {} deleted, {placed} placed this session (editor save to keep them)", ed.deleted.len()));
    if ed.text != text {
        ed.text = text;
    }
}

fn spawn(commands: &mut Commands) {
    commands.spawn((
        EditorRoot,
        PointerState::None,
        Node { position_type: PositionType::Absolute, left: Val::Px(8.0), top: Val::Px(8.0), max_width: Val::Percent(60.0), padding: UiRect::all(Val::Px(8.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
        GlobalZIndex(110),
        children![(EditorText, Text::new(""), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(Color::srgb(0.85, 0.95, 1.0)))],
    ));
}

fn draw(ed: Res<Editor>, mut texts: Query<&mut Text, With<EditorText>>) {
    for mut t in &mut texts {
        if t.0 != ed.text {
            t.0 = ed.text.clone();
        }
    }
}

/// A box around the selected reference's drawn parts.
fn outline(ed: Res<Editor>, visuals: Query<(&RefVisual, &Aabb, &GlobalTransform, &InheritedVisibility)>, mut gizmos: Gizmos) {
    let (true, Some(id)) = (ed.on, ed.selected) else { return };
    for (r, aabb, gt, v) in &visuals {
        if r.0 != id || !v.get() {
            continue;
        }
        let local = Transform::from_translation(Vec3::from(aabb.center)).with_scale(Vec3::from(aabb.half_extents) * 2.0);
        gizmos.cube(gt.mul_transform(local), Color::srgb(0.3, 0.9, 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_words() {
        assert_eq!(Op::parse("select 0001A2B3"), Some(Ok(Op::Select(0x0001_A2B3))));
        assert_eq!(Op::parse("MOVE 16 0 -8"), Some(Ok(Op::Move([16.0, 0.0, -8.0]))));
        assert_eq!(Op::parse("turn 90"), Some(Ok(Op::Turn(90.0))));
        assert!(matches!(Op::parse("move 1 2"), Some(Err(_))));
        assert_eq!(Op::parse("delete"), Some(Ok(Op::Delete)));
        assert_eq!(Op::parse("place 0010A1F6"), Some(Ok(Op::Place(0x0010_A1F6))));
        assert!(matches!(Op::parse("place"), Some(Err(_))));
        assert_eq!(Op::parse("undo"), Some(Ok(Op::Undo)));
        assert_eq!(Op::parse("save x.esp"), None);
        assert_eq!(Op::parse(""), None);
    }

    #[test]
    fn the_ini_unlocks_it() {
        assert!(ini_allows("[Settings]
editor=1
"));
        assert!(ini_allows("[Game]
Path=x
[settings]
Editor = on
"));
        assert!(!ini_allows("[Settings]
editor=0
"));
        assert!(!ini_allows("[Game]
editor=1
"));
        assert!(!ini_allows(""));
    }

    #[test]
    fn plugin_names() {
        assert_eq!(plugin_name("Edits").unwrap(), "Edits.esp");
        assert_eq!(plugin_name(" My Edits.ESM ").unwrap(), "My Edits.ESM");
        assert!(plugin_name("..\\Data\\x.esp").is_err());
        assert!(plugin_name("").is_err());
    }

    fn drag(vertical: bool) -> Drag {
        // A reference at (100, 200, 50); the camera looks along +X from (0, 200, 150).
        let ray = ([0.0, 200.0, 150.0], [1.0, 0.0, -1.0]);
        press_on(7, Vec2::ZERO, ray, ([100.0, 200.0, 50.0], [0.0; 3]), vertical, [1.0, 0.0, -0.5], false).unwrap()
    }

    #[test]
    fn drag_across_keeps_the_grab_and_height() {
        let d = drag(false);
        // Grabbed where the ray meets z = 50: x 100.
        assert_eq!(d.grab, [100.0, 200.0, 50.0]);
        // The pointer now meets the plane at (130, 210): the reference moves by as much.
        let to = drag_place(&d, [0.0, 210.0, 150.0], [1.3, 0.0, -1.0], false).unwrap();
        assert!((to[0] - 130.0).abs() < 1e-3 && (to[1] - 210.0).abs() < 1e-3 && to[2] == 50.0, "{to:?}");
        // On the grid: 128, 208.
        assert_eq!(drag_place(&d, [0.0, 210.0, 150.0], [1.3, 0.0, -1.0], true).unwrap(), [128.0, 208.0, 50.0]);
        // A ray going up never meets the plane below the eye; nor one past reach.
        assert_eq!(drag_place(&d, [0.0, 200.0, 150.0], [1.0, 0.0, 1.0], false), None);
        assert_eq!(drag_place(&d, [0.0, 200.0, 150.0], [1.0, 0.0, -1e-5], false), None);
    }

    #[test]
    fn drag_up_and_down_on_the_upright_plane() {
        let d = drag(true);
        // The upright plane through x 100 facing the camera: grabbed at z 50.
        assert_eq!(d.facing, [1.0, 0.0]);
        assert!((d.grab[2] - 50.0).abs() < 1e-3);
        // A flatter ray meets it at z 100: the reference rises 50, x and y stay.
        let to = drag_place(&d, [0.0, 200.0, 150.0], [1.0, 0.0, -0.5], false).unwrap();
        assert!((to[2] - 100.0).abs() < 1e-3 && to[0] == 100.0 && to[1] == 200.0, "{to:?}");
    }

    fn rec(id: u32, sig: &[u8; 4], edid: &str, name: Option<&str>) -> crate::console::Found {
        crate::console::Found { id: FormId(id), sig: *sig, edid: Some(edid.into()), name: name.map(Into::into) }
    }

    #[test]
    fn picker_finds_placeable_records_only() {
        let ix = vec![
            rec(1, b"MISC", "NukaColaBottleEmpty", Some("Nuka-Cola bottle")),
            rec(2, b"STAT", "BottleRack01", None),
            rec(3, b"NPC_", "BottleNPC", Some("Bottle Man")),
            rec(4, b"QUST", "BottleQuest", Some("Bottle")),
        ];
        let terms = crate::console::query_terms("bottle");
        let hits: Vec<u32> = find_placeable(&ix, &terms).iter().map(|f| f.id.0).collect();
        // Named first; the actor and the quest left out.
        assert_eq!(hits, vec![1, 2]);
        // #n from the list, an Editor ID without case; an actor's Editor ID refused.
        assert_eq!(resolve_place("#2", &hits, &ix), Ok(2));
        assert!(resolve_place("#3", &hits, &ix).is_err() && resolve_place("#x", &hits, &ix).is_err());
        assert_eq!(resolve_place("nukacolabottleempty", &hits, &ix), Ok(1));
        assert!(resolve_place("BottleNPC", &hits, &ix).unwrap_err().contains("ACHR"));
        assert!(resolve_place("NoSuch", &hits, &ix).is_err());
        assert!(placeable(b"STAT").is_ok() && placeable(b"CREA").is_err() && placeable(b"GRAS").is_err());
    }

    #[test]
    fn place_words() {
        assert_eq!(Op::parse("place 0010A1F6"), Some(Ok(Op::Place(0x0010_A1F6))));
        // #n and Editor IDs are not Ops: the console sends them to the picker.
        assert_eq!(Op::parse("place #2"), None);
        assert_eq!(Op::parse("place NukaColaBottleEmpty"), None);
        assert!(matches!(Op::parse("place"), Some(Err(_))));
        assert!(verb_is("find nuka cola", "find"));
    }

    #[test]
    fn drag_and_drop_words() {
        assert_eq!(Op::parse("drop"), Some(Ok(Op::Drop)));
        assert_eq!(Op::parse("drag 0.5 0.5 0.6 0.5"), Some(Ok(Op::Drag([0.5, 0.5, 0.6, 0.5]))));
        assert!(matches!(Op::parse("drag 0.5"), Some(Err(_))));
    }

    #[test]
    fn open_and_save_words() {
        assert!(verb_is("open", "open") && verb_is("OPEN My.esp", "open") && verb_is("save", "save"));
        assert!(!verb_is("opener", "open") && !verb_is("", "save") && !verb_is("select 1", "save"));
        // Neither is one of the Ops (the console sends them to the target fields instead).
        assert_eq!(Op::parse("open x.esp"), None);
    }

    #[test]
    fn arrows_follow_the_axis_nearest_the_view() {
        assert_eq!(nearest_axis([0.9, 0.2, -0.3]), [1.0, 0.0]);
        assert_eq!(nearest_axis([-0.1, -0.8, 0.0]), [0.0, -1.0]);
        assert_eq!(nearest_axis([-0.7, 0.7, 0.0]), [-1.0, 0.0]);
    }
}
