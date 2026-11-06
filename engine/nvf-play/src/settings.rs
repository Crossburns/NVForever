//! The switches of the pause menu's settings page (`crate::pause` draws it): the on/off
//! choices of the command line that can change while the game runs, e.g. whether a
//! conversation freezes the picture (`--menu-pause`). Not the original's settings menu:
//! neither its entries nor its layout are copied.
//!
//! A [`Setting`] reads and writes the state the game itself uses (a resource of the
//! module that owns the switch), so the page keeps no copy of its own: a switch starts
//! as the command line left it and a flip acts at once. The module that owns a switch
//! registers it ([`add_setting`]): nvf-play its own in its plugins, nvforever the
//! renderer's after adding [`crate::PlayPlugin`].
//!
//! Every switch is kept in `[Settings]` of the home's `NVForever.ini`
//! (`fnv_install::EngineHome::config_file`, beside the game folder's `[Game]`), one
//! `key=1` or `key=0` line each: a flip writes all of them, and the next start (also the
//! restart that loads a save) sets each listed switch to its stored value once every
//! switch is registered ([`apply_stored`], `PostStartup`), so the INI wins over the
//! command line. A key not in the file keeps the command line's state; a value other
//! than 1/0 (true/false, on/off, yes/no) is skipped with a warning. Unattended runs (test
//! aids) leave the INI of the player's home alone, neither read nor written; with
//! `NVFOREVER_HOME` set they use that home's INI, so a test can check the round trip.
//!
//! A row can be a [`Slider`] instead ([`Setting::slider`]): a part of a full amount, up
//! to 1, set in steps by Left/Right or its meter (e.g. how far objects are drawn, as a
//! part of how far they are drawn at most). It is kept as `key=0.50`; a value that is not
//! a number is skipped with a warning, one out of range is brought into it. A slider can
//! show its value as words instead of a percentage ([`Slider::shown`]): a choice among a
//! few (the anti-aliasing, [`Slider::choice`]) or a number of its own (the field of
//! view's degrees), still kept as the part.
//!
//! A row can be a list of choices instead, shown as a dropdown ([`ChoiceList`],
//! [`add_choice_list`]): the choices are named (e.g. the resolutions of the window's
//! monitor) and may differ from run to run, so it is kept by its own text (e.g.
//! `resolution=1600x900;desktop`), not as a number. A list that asks to be kept (the
//! resolution) takes a choice for a while only ([`KEEP_SECS`]): the page asks whether to
//! keep it, and unless it is kept by then it goes back to the one before
//! ([`PendingKeep`]); until it is kept, the INI keeps the one before.
//!
//! What is not here: choices fixed when the game starts (which NPCs live by their
//! packages, FaceGen, the load order), most numbers (the volumes, the field of view and the
//! draw distances are sliders) and the test aids.

use bevy::prelude::*;
use fnv_install::{guard_write, ini_get, ini_set, EngineHome, HOME_ENV};
use std::path::{Path, PathBuf};

/// The INI section the switches are kept in.
pub const SECTION: &str = "Settings";

/// The INI the switches are read from and written to; `None`: neither (module notes).
#[derive(Resource, Clone, Debug, Default)]
pub struct SettingsFile(pub Option<PathBuf>);

impl SettingsFile {
    /// The home's INI, unless this run is unattended in the player's own home.
    pub fn for_run(unattended: bool) -> SettingsFile {
        let own_home = std::env::var_os(HOME_ENV).is_some_and(|h| !h.is_empty());
        if unattended && !own_home {
            return SettingsFile(None);
        }
        SettingsFile(EngineHome::default_root().map(|r| EngineHome::new(r).config_file()))
    }
}

/// One on/off switch of the settings page.
#[derive(Clone, Copy)]
pub struct Setting {
    /// Short name for the log and the test aid (`voice`, `menu-pause`).
    pub key: &'static str,
    /// The heading it is listed under; a new group goes after the ones before it.
    pub group: &'static str,
    pub label: &'static str,
    /// What it does, in a line, and the command-line switch it starts from.
    pub note: &'static str,
    pub get: fn(&World) -> bool,
    pub set: fn(&mut World, bool),
    /// A slider instead of a switch (`get` and `set` then do nothing).
    pub slider: Option<Slider>,
}

/// A slider of the settings page: a part of a full amount, from `min` to 1 in `step`s.
#[derive(Clone, Copy)]
pub struct Slider {
    pub get: fn(&World) -> f32,
    pub set: fn(&mut World, f32),
    pub min: f32,
    pub step: f32,
    /// How the page and the log show a value (`None`: as a percentage).
    pub shown: Option<fn(f32) -> String>,
}

impl Slider {
    /// A choice among `n` (at least 2) as a slider: choice `i` is the part `i / (n - 1)`
    /// ([`choice_part`], [`choice_index`]), `shown` names it.
    pub fn choice(n: usize, get: fn(&World) -> f32, set: fn(&mut World, f32), shown: fn(f32) -> String) -> Slider {
        Slider { get, set, min: 0.0, step: 1.0 / (n.max(2) - 1) as f32, shown: Some(shown) }
    }

    /// The value as the page and the log show it.
    pub fn text(&self, v: f32) -> String {
        match self.shown {
            Some(f) => f(v),
            None => format!("{:.0}%", v * 100.0),
        }
    }

    /// `v` brought into `min..=1` and onto a step (so a stored value is one the keys
    /// reach), rounded to a thousandth so 0.55 stays 0.55.
    pub fn fit(&self, v: f32) -> f32 {
        let step = self.step.max(1e-3);
        let v = (v.clamp(self.min, 1.0) / step).round() * step;
        ((v * 1000.0).round() / 1000.0).clamp(self.min, 1.0)
    }
}

/// A row of named choices, shown as a dropdown ([`add_choice_list`]); its [`Setting`]'s
/// `get`, `set` and `slider` do nothing.
#[derive(Clone, Copy)]
pub struct ChoiceList {
    /// The choices as the page lists them now, the chosen one among them.
    pub names: fn(&World) -> Vec<String>,
    /// The chosen one's name.
    pub chosen: fn(&World) -> String,
    /// Take the choice of this name; false: not one of the list's.
    pub pick: fn(&mut World, &str) -> bool,
    /// The state as kept in the INI (it may hold more than the chosen name).
    pub save: fn(&World) -> String,
    /// The state from its INI text; false: not one.
    pub load: fn(&mut World, &str) -> bool,
    /// A choice from the page is taken for [`KEEP_SECS`] only unless kept (module notes).
    pub ask_keep: bool,
}

/// How long a choice that asks to be kept waits for it [design; owner 2026-10-18: 20
/// seconds].
pub const KEEP_SECS: f64 = 20.0;

/// A choice taken from the page that waits to be kept: its row, the state before (as the
/// INI keeps it) and when it was taken (real seconds).
#[derive(Resource, Clone, Debug)]
pub struct PendingKeep {
    pub row: usize,
    pub before: String,
    pub since: f64,
}

impl PendingKeep {
    /// Whole seconds left (at least 1 while it waits).
    pub fn left(&self, now: f64) -> u32 {
        (KEEP_SECS - (now - self.since)).ceil().max(1.0) as u32
    }
}

/// Choice `i` of `n` as a slider's part, as [`Slider::fit`] keeps it.
pub fn choice_part(i: usize, n: usize) -> f32 {
    let n = n.max(2) - 1;
    ((i.min(n) as f32 / n as f32) * 1000.0).round() / 1000.0
}

/// The choice of `n` a slider's part `v` stands for.
pub fn choice_index(v: f32, n: usize) -> usize {
    let n = n.max(2) - 1;
    ((v.clamp(0.0, 1.0) * n as f32).round() as usize).min(n)
}

impl Setting {
    /// A slider row.
    pub fn slider(key: &'static str, group: &'static str, label: &'static str, note: &'static str, slider: Slider) -> Setting {
        Setting { key, group, label, note, get: |_| false, set: |_, _| {}, slider: Some(slider) }
    }

    /// The row's state as a number: a slider's value, a switch's 1 or 0.
    pub fn value(&self, world: &World) -> f32 {
        match &self.slider {
            Some(s) => (s.get)(world),
            None => f32::from(u8::from((self.get)(world))),
        }
    }

    /// Set the row's state from a number (a switch: on above a half).
    pub fn put(&self, world: &mut World, v: f32) {
        match &self.slider {
            Some(s) => (s.set)(world, s.fit(v)),
            None => (self.set)(world, v > 0.5),
        }
    }

    /// The state as kept in the INI: a slider's to the thousandth [`Slider::fit`] keeps
    /// (a step under a hundredth, a colour's 1 of 255, needs it), at least two places.
    fn stored_text(&self, v: f32) -> String {
        match self.slider {
            Some(_) => {
                let mut t = format!("{v:.3}");
                if t.ends_with('0') {
                    t.pop();
                }
                t
            }
            None => if v > 0.5 { "1" } else { "0" }.to_string(),
        }
    }

    /// A stored value read back; `None`: not one of this row's.
    fn parse(&self, v: &str) -> Option<f32> {
        match &self.slider {
            Some(s) => v.trim().parse::<f32>().ok().filter(|x| x.is_finite()).map(|x| s.fit(x)),
            None => parse_switch(v).map(|on| f32::from(u8::from(on))),
        }
    }
}

/// The switches in the order the page lists them: by group, each group in the order its
/// switches were added.
#[derive(Resource, Default)]
pub struct Settings(Vec<Setting>, Vec<(&'static str, ChoiceList)>);

impl Settings {
    /// Add a switch, after the last one of its group. A key added twice is kept once.
    pub fn add(&mut self, s: Setting) {
        if self.index(s.key).is_some() {
            warn!("settings: {} is already listed", s.key);
            return;
        }
        let at = self.0.iter().rposition(|o| o.group == s.group).map_or(self.0.len(), |i| i + 1);
        self.0.insert(at, s);
    }

    pub fn list(&self) -> &[Setting] {
        &self.0
    }

    pub fn index(&self, key: &str) -> Option<usize> {
        self.0.iter().position(|s| s.key == key)
    }

    /// Row `i`'s choices, when it is a list.
    pub fn choices(&self, i: usize) -> Option<ChoiceList> {
        let key = self.0.get(i)?.key;
        self.1.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)
    }

    /// Every row's state as a number: [`Setting::value`], a list's chosen index (-1: not
    /// listed).
    pub fn values(&self, world: &World) -> Vec<f32> {
        (0..self.0.len()).map(|i| self.value(world, i)).collect()
    }

    fn value(&self, world: &World, i: usize) -> f32 {
        match self.choices(i) {
            Some(c) => {
                let chosen = (c.chosen)(world);
                (c.names)(world).iter().position(|n| *n == chosen).map_or(-1.0, |p| p as f32)
            }
            None => self.0[i].value(world),
        }
    }
}

/// Register a switch of the settings page.
pub fn add_setting(app: &mut App, s: Setting) {
    app.init_resource::<Settings>();
    app.world_mut().resource_mut::<Settings>().add(s);
}

/// Register a row of named choices (a dropdown) with its [`Setting`] (`slider` `None`).
pub fn add_choice_list(app: &mut App, s: Setting, list: ChoiceList) {
    app.init_resource::<Settings>();
    let mut settings = app.world_mut().resource_mut::<Settings>();
    if settings.index(s.key).is_none() {
        settings.1.push((s.key, list));
    }
    settings.add(s);
}

/// Take choice `name` of list row `i` and keep every row in the INI; false: not taken
/// (not a list, not one of its choices, or chosen already). A list that asks to be kept
/// waits for it ([`keep`]).
pub fn pick(world: &mut World, i: usize, name: &str) -> bool {
    let Some(c) = world.get_resource::<Settings>().and_then(|s| s.choices(i)) else { return false };
    if (c.chosen)(world) == name {
        return false;
    }
    let pick = c.pick;
    take(world, i, |w| pick(w, name))
}

/// List row `i`'s state changed by `apply` (false: not taken), then kept in the INI; a
/// list that asks to be kept waits for it.
fn take(world: &mut World, i: usize, apply: impl FnOnce(&mut World) -> bool) -> bool {
    let Some(settings) = world.get_resource::<Settings>() else { return false };
    let (Some(s), Some(c)) = (settings.list().get(i).copied(), settings.choices(i)) else { return false };
    let before = (c.save)(world);
    if !apply(world) {
        warn!("settings: {} has no such choice", s.key);
        return false;
    }
    let name = (c.chosen)(world);
    if c.ask_keep {
        // Chosen again while waiting: the one before the first goes back.
        let before = world.get_resource::<PendingKeep>().filter(|p| p.row == i).map_or(before, |p| p.before.clone());
        let since = world.get_resource::<Time<Real>>().map_or(0.0, |t| t.elapsed_secs_f64());
        info!("settings: {} {name}, until kept (back in {KEEP_SECS:.0} s)", s.key);
        world.insert_resource(PendingKeep { row: i, before, since });
    } else {
        info!("settings: {} {name}", s.key);
    }
    store(world);
    true
}

/// Keep the choice that waits ([`PendingKeep`]).
pub fn keep(world: &mut World) {
    let Some(p) = world.remove_resource::<PendingKeep>() else { return };
    let key = world.resource::<Settings>().list().get(p.row).map_or("", |s| s.key);
    info!("settings: {key} kept");
    store(world);
}

/// Put the choice that waits back to the one before.
pub fn revert(world: &mut World) {
    let Some(p) = world.remove_resource::<PendingKeep>() else { return };
    let settings = world.resource::<Settings>();
    let (Some(s), Some(c)) = (settings.list().get(p.row).copied(), settings.choices(p.row)) else { return };
    (c.load)(world, &p.before);
    info!("settings: {} back to {} (not kept)", s.key, (c.chosen)(world));
    store(world);
}

/// A choice not kept in time goes back.
fn revert_unkept(world: &mut World) {
    let Some(p) = world.get_resource::<PendingKeep>() else { return };
    if world.resource::<Time<Real>>().elapsed_secs_f64() - p.since >= KEEP_SECS {
        revert(world);
    }
}

/// Flip switch `i`, as a click on its row, and keep every switch in the INI; the new
/// state. A slider's or a list's row is not flipped (`None`).
pub fn flip(world: &mut World, i: usize) -> Option<bool> {
    let settings = world.get_resource::<Settings>()?;
    let s = *settings.list().get(i)?;
    if s.slider.is_some() || settings.choices(i).is_some() {
        return None;
    }
    let on = !(s.get)(world);
    (s.set)(world, on);
    info!("settings: {} {}", s.key, if on { "on" } else { "off" });
    store(world);
    Some(on)
}

/// Move slider `i` by `steps` of its step and keep every row in the INI; the new value.
pub fn step_slider(world: &mut World, i: usize, steps: i32) -> Option<f32> {
    let s = *world.get_resource::<Settings>()?.list().get(i)?;
    let slider = s.slider?;
    set_slider(world, i, (slider.get)(world) + steps as f32 * slider.step)
}

/// Set slider `i` to `v` (fitted to its range and steps) and keep every row in the INI;
/// the new value. Nothing is written when the value stays.
pub fn set_slider(world: &mut World, i: usize, v: f32) -> Option<f32> {
    let s = *world.get_resource::<Settings>()?.list().get(i)?;
    let slider = s.slider?;
    let v = slider.fit(v);
    if (slider.get)(world) != v {
        (slider.set)(world, v);
        info!("settings: {} {}", s.key, slider.text(v));
        store(world);
    }
    Some(v)
}

/// Put row `i` back to `v` (the page's Defaults): flip a switch, set a slider, pick a
/// list's choice of that index.
pub fn restore(world: &mut World, i: usize, v: f32) {
    let Some(settings) = world.get_resource::<Settings>() else { return };
    let Some(s) = settings.list().get(i).copied() else { return };
    if settings.value(world, i) == v && settings.choices(i).is_none() {
        return;
    }
    if let Some(c) = settings.choices(i) {
        // A list goes back to its state as kept at the start: its choices may differ by
        // now (the monitor's sizes come after the start), so not by its index.
        match world.get_resource::<StartChoices>().and_then(|sc| sc.0.iter().find(|(r, _)| *r == i)).map(|(_, t)| t.clone()) {
            Some(t) if (c.save)(world) != t => {
                let load = c.load;
                take(world, i, |w| load(w, &t));
            }
            Some(_) => {}
            None => {
                if let Some(name) = (v >= 0.0).then(|| (c.names)(world).get(v as usize).cloned()).flatten() {
                    pick(world, i, &name);
                }
            }
        }
        return;
    }
    if s.slider.is_some() {
        set_slider(world, i, v);
    } else {
        flip(world, i);
    }
}

pub(crate) fn setup(app: &mut App) {
    app.init_resource::<Settings>();
    app.insert_resource(SettingsFile::for_run(crate::test_aid::Unattended::from_env().0));
    app.add_systems(PostStartup, (apply_stored, crate::bindings::apply_stored));
    app.add_systems(PostStartup, fix_ui_scale.after(apply_stored));
    app.add_systems(Update, revert_unkept);
}

/// The UI scale is fixed for the run once the INI is read (`crate::interface::fix_ui_scale`).
pub fn fix_ui_scale() {
    crate::interface::fix_ui_scale();
}

/// `1`/`0` (also true/false, on/off, yes/no, any case).
fn parse_switch(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

/// `text` (an INI, or empty) with every row's state, as kept there, in `[Settings]`.
fn with_switches(text: &str, states: &[(&str, String)]) -> String {
    states.iter().fold(text.to_string(), |t, (key, v)| ini_set(&t, SECTION, key, v))
}

/// The rows' states stored in `text`: (index in `list`, [`Setting::value`]); values that
/// are not of the row's kind go to `bad`. The list rows (`lists`, by key) are left out.
fn stored(text: &str, list: &[Setting], lists: &[&str], bad: &mut Vec<String>) -> Vec<(usize, f32)> {
    let mut out = Vec::new();
    for (i, s) in list.iter().enumerate() {
        if lists.contains(&s.key) {
            continue;
        }
        let Some(v) = ini_get(text, SECTION, s.key) else { continue };
        match s.parse(&v) {
            Some(x) => out.push((i, x)),
            None => bad.push(format!("{}={v}", s.key)),
        }
    }
    out
}

/// Every row's state ([`Setting::value`]) before the INI was read: the defaults and the
/// command line (what the page's Defaults goes back to).
#[derive(Resource, Clone, Debug, Default)]
pub struct StartStates(pub Vec<f32>);

/// The list rows' states before the INI was read, as the INI keeps them (what the page's
/// Defaults goes back to): (row, text).
#[derive(Resource, Clone, Debug, Default)]
pub struct StartChoices(pub Vec<(usize, String)>);

/// Set the rows the INI lists (module notes), after noting the states before.
pub fn apply_stored(world: &mut World) {
    let Some(settings) = world.get_resource::<Settings>() else { return };
    let start = settings.values(world);
    let choices = (0..settings.list().len()).filter_map(|i| settings.choices(i).map(|c| (i, (c.save)(world)))).collect();
    world.insert_resource(StartStates(start));
    world.insert_resource(StartChoices(choices));
    let Some(path) = world.get_resource::<SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            warn!("settings: {} not read: {e}", path.display());
            return;
        }
    };
    let list = world.resource::<Settings>().list().to_vec();
    let lists: Vec<(usize, ChoiceList)> = (0..list.len()).filter_map(|i| Some((i, world.resource::<Settings>().choices(i)?))).collect();
    let keys: Vec<&str> = lists.iter().map(|(i, _)| list[*i].key).collect();
    let mut bad = Vec::new();
    let found = stored(&text, &list, &keys, &mut bad);
    let mut changed = Vec::new();
    let mut listed = 0;
    for (i, c) in lists {
        let Some(v) = ini_get(&text, SECTION, list[i].key) else { continue };
        let v = v.trim();
        if (c.save)(world) == v {
            listed += 1;
        } else if (c.load)(world, v) {
            listed += 1;
            changed.push(format!("{} {}", list[i].key, (c.chosen)(world)));
        } else {
            bad.push(format!("{}={v}", list[i].key));
        }
    }
    for &(i, v) in &found {
        let s = list[i];
        if s.value(world) != v {
            s.put(world, v);
            changed.push(match s.slider {
                Some(sl) => format!("{} {}", s.key, sl.text(v)),
                None => format!("{} {}", s.key, if v > 0.5 { "on" } else { "off" }),
            });
        }
    }
    for b in &bad {
        warn!("settings: [{SECTION}] {b} in {} is not 1 or 0 (a slider's: a number, a list's: one of its choices); left as it is", path.display());
    }
    let what = if changed.is_empty() { String::new() } else { format!(": {}", changed.join(", ")) };
    info!("settings: {} of {} switch(es) read from {}, {} changed{what}", found.len() + listed, list.len(), path.display(), changed.len());
}

/// Write every switch's state to the INI, keeping its other lines.
fn store(world: &World) {
    let Some(path) = world.get_resource::<SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let Some(settings) = world.get_resource::<Settings>() else { return };
    // A choice waiting to be kept is kept as the one before.
    let pending = world.get_resource::<PendingKeep>();
    let states: Vec<(&str, String)> = settings
        .list()
        .iter()
        .enumerate()
        .map(|(i, s)| match (settings.choices(i), pending) {
            (Some(_), Some(p)) if p.row == i => (s.key, p.before.clone()),
            (Some(c), _) => (s.key, (c.save)(world)),
            (None, _) => (s.key, s.stored_text(s.value(world))),
        })
        .collect();
    match write(&path, &states) {
        Ok(()) => debug!("settings: {} switch(es) written to {}", states.len(), path.display()),
        Err(e) => warn!("settings: not saved: {e}"),
    }
}

fn write(path: &Path, states: &[(&str, String)]) -> Result<(), String> {
    guard_write(path, None).map_err(|e| e.to_string())?;
    let old = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, with_switches(&old, states)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Lamp(bool);

    fn setting(key: &'static str, group: &'static str) -> Setting {
        Setting { key, group, label: key, note: "", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on, slider: None }
    }

    #[derive(Resource)]
    struct Reach(f32);

    fn slider(key: &'static str) -> Setting {
        Setting::slider(key, "Test", key, "", Slider { get: |w| w.resource::<Reach>().0, set: |w, v| w.resource_mut::<Reach>().0 = v, min: 0.1, step: 0.05, shown: None })
    }

    #[test]
    fn switches_are_listed_by_group_in_the_order_added() {
        let mut s = Settings::default();
        for (key, group) in [("a", "Dialogue"), ("b", "Picture"), ("c", "Dialogue"), ("d", "Developer"), ("e", "Picture"), ("a", "Picture")] {
            s.add(setting(key, group));
        }
        assert_eq!(s.list().iter().map(|s| s.key).collect::<Vec<_>>(), ["a", "c", "b", "e", "d"]);
        assert_eq!(s.index("e"), Some(3));
        assert_eq!(s.index("f"), None);
    }

    #[test]
    fn a_flip_writes_the_state_the_game_reads() {
        let mut app = App::new();
        app.init_resource::<Lamp>();
        add_setting(&mut app, setting("lamp", "Test"));
        let world = app.world_mut();
        assert_eq!(flip(world, 0), Some(true));
        assert!(world.resource::<Lamp>().0);
        assert_eq!(flip(world, 0), Some(false));
        assert!(!world.resource::<Lamp>().0);
        assert_eq!(flip(world, 1), None);
    }

    #[test]
    fn switches_are_kept_beside_the_game_folder_and_read_back() {
        let ini = "; NVForever settings.\n[Game]\nPath=F:\\FNV\n";
        let st = |a: &[(&'static str, &str)]| a.iter().map(|(k, v)| (*k, v.to_string())).collect::<Vec<_>>();
        let text = with_switches(ini, &st(&[("grass", "0"), ("god-mode", "1")]));
        assert_eq!(text, "; NVForever settings.\n[Game]\nPath=F:\\FNV\n\n[Settings]\ngrass=0\ngod-mode=1\n");
        let text = with_switches(&text, &st(&[("grass", "1"), ("god-mode", "1")]));
        assert_eq!(ini_get(&text, SECTION, "grass").as_deref(), Some("1"));
        assert_eq!(ini_get(&text, "Game", "Path").as_deref(), Some("F:\\FNV"));
        let list = [setting("lamp", "Test"), setting("grass", "Test"), setting("god-mode", "Test")];
        let mut bad = Vec::new();
        let text = format!("{text}lamp=maybe\n");
        assert_eq!(stored(&text, &list, &[], &mut bad), [(1, 1.0), (2, 1.0)]);
        assert_eq!(bad, ["lamp=maybe"]);
        assert_eq!(parse_switch(" Off "), Some(false));
    }

    #[test]
    fn a_flip_is_saved_and_the_next_start_reads_it() {
        let dir = std::env::temp_dir().join(format!("nvf-settings-{}", std::process::id()));
        let path = dir.join("NVForever.ini");
        let _ = std::fs::remove_dir_all(&dir);
        let start = |path: &Path| {
            let mut app = App::new();
            app.init_resource::<Lamp>();
            app.insert_resource(SettingsFile(Some(path.to_path_buf())));
            add_setting(&mut app, setting("lamp", "Test"));
            app.add_systems(PostStartup, apply_stored);
            app.update();
            app
        };
        let mut app = start(&path);
        assert!(!app.world().resource::<Lamp>().0, "no INI yet: the default");
        flip(app.world_mut(), 0);
        assert!(std::fs::read_to_string(&path).unwrap().contains("[Settings]\nlamp=1"));
        let app = start(&path);
        assert!(app.world().resource::<Lamp>().0, "the stored state wins");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #1631: the UI scale row is kept in the INI as its part and read back at the next
    /// start; a value out of range is brought to the nearest end, one that is not a
    /// number is skipped. (The only test that sets the UI scale setting.)
    #[test]
    fn the_ui_scale_survives_a_restart_and_is_clamped() {
        let dir = std::env::temp_dir().join(format!("nvf-settings-ui-scale-{}", std::process::id()));
        let path = dir.join("NVForever.ini");
        let _ = std::fs::remove_dir_all(&dir);
        let start = |path: &Path| {
            crate::interface::set_ui_scale_setting(1.0);
            let mut app = App::new();
            app.insert_resource(SettingsFile(Some(path.to_path_buf())));
            add_setting(&mut app, crate::interface::ui_scale_row("Display"));
            app.add_systems(PostStartup, apply_stored);
            app.update();
            app
        };
        let mut app = start(&path);
        assert_eq!(crate::interface::ui_scale_setting(), 1.0, "no INI yet: 100%");
        set_slider(app.world_mut(), 0, 0.25);
        assert_eq!(crate::interface::ui_scale_setting(), 0.75);
        assert!(std::fs::read_to_string(&path).unwrap().contains("[Settings]
ui-scale=0.25
"));
        let _app = start(&path);
        assert_eq!(crate::interface::ui_scale_setting(), 0.75, "the stored scale wins at the next start");
        for (stored, scale) in [("7", 1.5), ("-2", 0.5), ("0.26", 0.75), ("big", 1.0)] {
            std::fs::write(&path, format!("[Settings]
ui-scale={stored}
")).unwrap();
            let _app = start(&path);
            assert_eq!(crate::interface::ui_scale_setting(), scale, "ui-scale={stored}");
        }
        crate::interface::set_ui_scale_setting(1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_slider_steps_within_its_range_and_is_kept_as_a_number() {
        let dir = std::env::temp_dir().join(format!("nvf-settings-slider-{}", std::process::id()));
        let path = dir.join("NVForever.ini");
        let _ = std::fs::remove_dir_all(&dir);
        let start = |path: &Path| {
            let mut app = App::new();
            app.init_resource::<Lamp>().insert_resource(Reach(1.0));
            app.insert_resource(SettingsFile(Some(path.to_path_buf())));
            add_setting(&mut app, setting("lamp", "Test"));
            add_setting(&mut app, slider("reach"));
            app.add_systems(PostStartup, apply_stored);
            app.update();
            app
        };
        let mut app = start(&path);
        let w = app.world_mut();
        assert_eq!(flip(w, 1), None, "a slider is not flipped");
        assert_eq!(step_slider(w, 1, 1), Some(1.0), "not past the full amount");
        assert_eq!(step_slider(w, 1, -10), Some(0.5));
        assert_eq!(set_slider(w, 1, 0.02), Some(0.1), "not under its least");
        assert_eq!(set_slider(w, 1, 0.537), Some(0.55), "onto a step");
        assert_eq!(step_slider(w, 0, 1), None, "a switch has no steps");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("lamp=0\nreach=0.55"), "{text}");
        let app = start(&path);
        assert_eq!(app.world().resource::<Reach>().0, 0.55, "the stored value wins");
        assert_eq!(app.world().resource::<StartStates>().0, [0.0, 1.0], "Defaults goes back to the start");
        let mut bad = Vec::new();
        assert!(stored("[Settings]\nreach=far\n", &[slider("reach")], &[], &mut bad).is_empty());
        assert_eq!(bad, ["reach=far"]);
        assert_eq!(stored("[Settings]\nreach=3\n", &[slider("reach")], &[], &mut bad), [(0, 1.0)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[derive(Resource)]
    struct Size(String);

    fn sizes() -> ChoiceList {
        ChoiceList {
            names: |_| ["800x600", "1280x800", "1920x1080"].map(String::from).to_vec(),
            chosen: |w| w.resource::<Size>().0.clone(),
            pick: |w, n| {
                let ok = ["800x600", "1280x800", "1920x1080"].contains(&n);
                if ok {
                    w.resource_mut::<Size>().0 = n.to_string();
                }
                ok
            },
            save: |w| format!("{};kept", w.resource::<Size>().0),
            load: |w, t| match t.strip_suffix(";kept") {
                Some(n) => {
                    w.resource_mut::<Size>().0 = n.to_string();
                    true
                }
                None => false,
            },
            ask_keep: true,
        }
    }

    #[test]
    fn a_list_choice_goes_back_unless_kept_and_the_ini_keeps_only_a_kept_one() {
        let dir = std::env::temp_dir().join(format!("nvf-settings-list-{}", std::process::id()));
        let path = dir.join("NVForever.ini");
        let _ = std::fs::remove_dir_all(&dir);
        let start = |path: &Path| {
            let mut app = App::new();
            app.init_resource::<Lamp>().insert_resource(Size("1280x800".into())).init_resource::<Time<Real>>();
            app.insert_resource(SettingsFile(Some(path.to_path_buf())));
            add_setting(&mut app, setting("lamp", "Test"));
            add_choice_list(&mut app, Setting { slider: None, ..setting("size", "Test") }, sizes());
            app.add_systems(PostStartup, apply_stored);
            app.update();
            app
        };
        let mut app = start(&path);
        let w = app.world_mut();
        assert_eq!(w.resource::<Settings>().values(w), [0.0, 1.0], "a list's state is its chosen index");
        assert_eq!(flip(w, 1), None, "a list is not flipped");
        assert!(!pick(w, 1, "640x480"), "not one of its choices");
        assert!(!pick(w, 1, "1280x800"), "chosen already");
        assert!(pick(w, 1, "1920x1080"));
        assert_eq!(w.resource::<Size>().0, "1920x1080", "taken at once");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("size=1280x800;kept"), "the INI keeps the one before until kept: {text}");
        assert!(pick(w, 1, "800x600"));
        assert_eq!(w.resource::<PendingKeep>().before, "1280x800;kept", "chosen again: the first one before goes back");
        // Not kept in time: back to the one before.
        w.resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs_f64(KEEP_SECS + 0.1));
        revert_unkept(w);
        assert_eq!(w.resource::<Size>().0, "1280x800");
        assert!(w.get_resource::<PendingKeep>().is_none());
        // Kept: the INI has it and the next start takes it.
        assert!(pick(w, 1, "800x600"));
        keep(w);
        revert_unkept(w);
        assert_eq!(w.resource::<Size>().0, "800x600");
        let app = start(&path);
        assert_eq!(app.world().resource::<Size>().0, "800x600", "the kept choice is read back");
        // Defaults picks the start's index again, and asks to be kept.
        let mut app = app;
        let w = app.world_mut();
        restore(w, 1, 1.0);
        assert_eq!((w.resource::<Size>().0.as_str(), w.contains_resource::<PendingKeep>()), ("1280x800", true));
        revert(w);
        assert_eq!(w.resource::<Size>().0, "800x600");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_choice_is_a_slider_whose_steps_are_the_choices_and_survives_the_ini() {
        let shown: fn(f32) -> String = |v| ["off", "low", "high"][choice_index(v, 3)].to_string();
        let c = Slider::choice(3, |_| 0.0, |_, _| {}, shown);
        assert_eq!((c.text(0.5), c.text(1.0)), ("low".to_string(), "high".to_string()));
        for n in [2, 3, 4, 5, 7, 13] {
            let c = Slider::choice(n, |_| 0.0, |_, _| {}, shown);
            let row = Setting::slider("c", "Test", "c", "", c);
            for i in 0..n {
                let v = choice_part(i, n);
                assert_eq!(c.fit(v), v, "{i} of {n} is on a step");
                let back = row.parse(&row.stored_text(v)).expect("a number");
                assert_eq!((back, choice_index(back, n)), (v, i), "{i} of {n} read back");
            }
        }
    }
}
