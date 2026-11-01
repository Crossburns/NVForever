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
//! - **Moving**: the arrow keys move the selection along the world's X and Y axes, the
//!   one nearest to the camera's view (up: away from the camera); Page Up and Page Down
//!   move it up and down; Ctrl with Left and Right turns it about Z; 16 units or 15° a
//!   step, Alt for 1 unit or 1° [design]. A move goes through the script functions
//!   SetPos and SetAngle (`fnv_game::functions::refs`) as a console line would, so the
//!   world draws it at once. The references moved this session are counted in the panel.
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
//! - **From the console** (and test aids): `editor select <FormID>`, `editor move <dx>
//!   <dy> <dz>` (world axes, game units), `editor turn <degrees>` (about Z), `editor
//!   place <base>`, `editor delete` and `editor undo` do what a click and the keys do.
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
    /// `editor save <name>` asked for.
    pub save_asked: Option<String>,
    /// `editor select`, `editor move`, `editor turn` asked for, in order.
    pub ops: Vec<Op>,
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
            ("place", [id]) => crate::console::parse_form_id(id).map(|f| Op::Place(f.0)).ok_or_else(|| format!("{id:?} is not a FormID")),
            ("place", _) => Err("editor place <base FormID>".into()),
            ("delete", []) => Ok(Op::Delete),
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
        app.init_resource::<Editor>().add_systems(Update, (switch, save, pick, nudge, place_new, describe, draw, outline).chain().in_set(PlaySet::Ui).after(crate::console::ConsoleSet));
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

/// `editor save <name>`: write the moved references into the plugin `name`.
fn save(mut ed: ResMut<Editor>, mut console: ResMut<Console>, play: Res<PlayWorld>, sim: Res<SimState>) {
    let Some(name) = ed.save_asked.take() else { return };
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
    let ids: Vec<String> = w.new_ids.iter().map(|&f| FormId(f).to_string()).collect();
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

/// A left click selects the reference under the pointer.
#[allow(clippy::too_many_arguments)]
fn pick(
    mouse: Res<ButtonInput<MouseButton>>,
    mut ed: ResMut<Editor>,
    console: Res<Console>,
    free: Res<crate::free_cam::FreeCam>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    phys: Option<ResMut<PhysState>>,
    visuals: Query<(&RefVisual, &Aabb, &GlobalTransform, &InheritedVisibility)>,
    panel: Query<&PointerState, With<EditorRoot>>,
) {
    if !ed.on || console.open || !free.active || !mouse.just_pressed(MouseButton::Left) || panel.iter().any(|p| *p != PointerState::None) {
        return;
    }
    let (Ok(window), Ok((cam, cam_t))) = (windows.single(), camera.single()) else { return };
    let Some(at) = window.cursor_position() else { return };
    let Ok(ray) = cam.viewport_to_world(cam_t, at) else { return };
    let picked = crate::console::pick_under(ray.origin, *ray.direction, REACH, phys.map(|p| p.into_inner()), &visuals).picked;
    match picked {
        Some(id) if ed.selected == Some(id) => {
            ed.selected = None;
            info!("editor: unselected {}", FormId(id));
        }
        Some(id) => {
            ed.selected = Some(id);
            info!("editor: selected {}", FormId(id));
        }
        None => info!("editor: nothing there to select"),
    }
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

/// The selected reference's details for the panel.
fn describe(mut ed: ResMut<Editor>, play: Res<PlayWorld>, sim: Res<SimState>, mut script: Option<ResMut<ScriptState>>) {
    if !ed.on {
        return;
    }
    let mut text = String::from(
        "EDITOR MODE   (type `editor` in the console to leave)\n\
         Fly: W A S D, Q down, E up, Shift faster, wheel speed; hold the right mouse button to look\n\
         Click: select   Arrows: move   Page Up/Down: up/down   Ctrl+Left/Right: turn   Alt: fine steps   Delete: delete   Ctrl+Z: undo   P: place another\n\
         Console: editor place <base FormID> places a new reference where the screen's centre points\n\
         Console: editor save <Name.esp> writes the moved references into the mods folder and turns the plugin on\n",
    );
    match ed.selected {
        Some(id) => {
            let lines = crate::console::describe(id, &play.0, &sim, script.as_deref_mut().and_then(|s| s.rt.as_mut()));
            text.push_str(&lines.join("\n"));
        }
        None => text.push_str("Nothing selected"),
    }
    let (moved, placed) = ed.counts();
    text.push_str(&format!("\n{moved} reference(s) moved, {} deleted, {placed} placed this session (editor save <Name.esp> to keep them)", ed.deleted.len()));
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

    #[test]
    fn arrows_follow_the_axis_nearest_the_view() {
        assert_eq!(nearest_axis([0.9, 0.2, -0.3]), [1.0, 0.0]);
        assert_eq!(nearest_axis([-0.1, -0.8, 0.0]), [0.0, -1.0]);
        assert_eq!(nearest_axis([-0.7, 0.7, 0.0]), [-1.0, 0.0]);
    }
}
