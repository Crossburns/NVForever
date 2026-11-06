//! The window's mode: an ordinary window (`--window windowed`, the default) or borderless
//! full screen on the display chosen in the Display group's "Monitor" row (the first
//! display until one is chosen; `--window borderless`). The settings page's Display group
//! switches it while the game runs (`settings.rs`), kept in `NVForever.ini` like every
//! switch. Each mode the primary window takes is logged.
//!
//! **The resolution** is the Display group's first row, a dropdown of the sizes the
//! window's monitor can show (its video modes, smallest first) [owner 2026-10-18: a
//! resolution dropdown at the top of Display, a 20 s countdown back to the one before].
//! In a window it is the window's size (`--size`); in full screen the monitor's own size
//! is borderless full screen as before, and a smaller one switches the monitor to it
//! (exclusive full screen at that video mode, its highest refresh rate) [design]. The two
//! are kept apart, so leaving full screen gives back the window's size: kept in the INI
//! as `resolution=<window>;<full screen>` (`desktop`: the monitor's own), e.g.
//! `resolution=1600x900;desktop`. A resolution chosen on the page goes back to the one
//! before unless it is kept within 20 seconds (`nvf_play::settings::PendingKeep`).
//!
//! [`apply_display`] makes the window what `Args` and [`FullScreenSize`] ask whenever they
//! change, so the settings only write those (a window resized by its frame is left as it
//! is until they change).

use crate::Args;
use bevy::prelude::*;
use bevy::window::{Monitor, MonitorSelection, OnMonitor, PrimaryMonitor, PrimaryWindow, VideoMode, VideoModeSelection, WindowMode};

/// `--window`: how the game's window is shown.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowChoice {
    /// A window of --size with a frame.
    #[default]
    Windowed,
    /// Borderless full screen on the display chosen in the Display group's "Monitor" row
    /// (its own resolution; --size is not used then, screenshots too are the display's size).
    Borderless,
}

impl WindowChoice {
    pub fn mode(self) -> WindowMode {
        match self {
            WindowChoice::Windowed => WindowMode::Windowed,
            // `Current` cannot be resolved while the window is being created (it has no
            // monitor yet) [documented: bevy_window MonitorSelection]; on Wayland there is no
            // "primary" monitor either [documented: winit Wayland backend], so the compositor
            // places the window [observed: #1641]. This is only the window's initial mode:
            // `apply_display` moves it to the chosen display (the first one until a choice is
            // kept) once the monitors are known.
            WindowChoice::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
        }
    }
}

/// Full screen's resolution: `None` the monitor's own (borderless), else that size
/// (exclusive full screen).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FullScreenSize(pub Option<UVec2>);

/// What the window's monitor can show: its own size (as the desktop has it, noted while
/// not in exclusive full screen) and its video modes.
#[derive(Resource, Clone, Debug, Default)]
pub struct MonitorModes {
    pub desktop: Option<UVec2>,
    pub modes: Vec<VideoMode>,
    /// Every monitor, for the Display "Monitor" list and for selecting one (its entity)
    /// once it is known.
    pub monitors: Vec<MonitorInfo>,
    /// The monitor the window is on now (its entity), for comparing against a chosen one.
    pub current: Option<Entity>,
}

/// A monitor: its connector name (stable — kept in the INI), its entity, its native size
/// (for the list's label), and its position in the desktop (for a stable left-to-right,
/// top-to-bottom order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorInfo {
    pub name: String,
    pub entity: Entity,
    pub size: UVec2,
    pub position: IVec2,
}

impl MonitorModes {
    /// The sizes of the video modes, smallest first, none under 640x480.
    pub fn sizes(&self) -> Vec<UVec2> {
        let mut s: Vec<UVec2> = self.modes.iter().map(|m| m.physical_size).filter(|s| s.x >= 640 && s.y >= 480).collect();
        s.extend(self.desktop);
        s.sort_by_key(|s| (u64::from(s.x) * u64::from(s.y), s.x));
        s.dedup();
        s
    }

    /// The video mode of size `size` with the highest refresh rate.
    fn mode_of(&self, size: UVec2) -> Option<VideoMode> {
        self.modes.iter().filter(|m| m.physical_size == size).max_by_key(|m| (m.refresh_rate_millihertz, m.bit_depth)).copied()
    }
}

/// The display borderless full screen opens on: the Display "Monitor" choice (kept in the
/// INI by connector name), or the first display until one is chosen (or if the chosen one
/// is not attached now).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct MonitorChoice(pub Option<String>);

/// The "Monitor" list's label for a display: "Display N (WxH)".
fn monitor_label(i: usize, m: &MonitorInfo) -> String {
    format!("Display {} ({}x{})", i + 1, m.size.x, m.size.y)
}

/// A stable display order, independent of the discovery order: left-to-right, then
/// top-to-bottom (the discovery order can put the window's current display first).
fn ordered(mut monitors: Vec<MonitorInfo>) -> Vec<MonitorInfo> {
    monitors.sort_by_key(|m| (m.position.x, m.position.y));
    monitors
}

/// The monitor selection for [`MonitorChoice`]: the chosen monitor by its name, else the
/// first monitor (the default); no monitor known yet → `Primary`.
fn borderless_selection(modes: &MonitorModes, monitor: &MonitorChoice) -> MonitorSelection {
    let chosen = match &monitor.0 {
        Some(name) => modes.monitors.iter().find(|m| &m.name == name),
        None => None,
    };
    chosen.or_else(|| modes.monitors.first()).map_or(MonitorSelection::Primary, |m| MonitorSelection::Entity(m.entity))
}

pub struct WindowModePlugin;

/// Apply the chosen window settings before telemetry snapshots their physical size.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApplyWindowMode;

impl Plugin for WindowModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FullScreenSize>().init_resource::<MonitorModes>().init_resource::<MonitorChoice>().add_systems(Update, (watch_monitor, apply_display, log_mode).chain().in_set(ApplyWindowMode));
    }
}

/// The settings page's switch: full screen on or off (`apply_display` does it).
pub fn set_borderless(world: &mut World, on: bool) {
    world.resource_mut::<Args>().window = if on { WindowChoice::Borderless } else { WindowChoice::Windowed };
}

/// "WxH".
pub fn size_text(s: UVec2) -> String {
    format!("{}x{}", s.x, s.y)
}

/// A size from "WxH" (at least 320x240, at most 16384 a side).
pub fn parse_size(t: &str) -> Option<UVec2> {
    let (w, h) = t.trim().split_once(['x', 'X'])?;
    let s = UVec2::new(w.trim().parse().ok()?, h.trim().parse().ok()?);
    (s.x >= 320 && s.y >= 240 && s.x <= 16384 && s.y <= 16384).then_some(s)
}

/// The resolution row's choices: the monitor's sizes (in a window none larger than the
/// monitor), the chosen one among them.
pub fn resolution_names(world: &World) -> Vec<String> {
    let m = world.resource::<MonitorModes>();
    let windowed = world.resource::<Args>().window == WindowChoice::Windowed;
    let mut sizes: Vec<UVec2> = m.sizes().into_iter().filter(|s| !windowed || m.desktop.is_none_or(|d| s.x <= d.x && s.y <= d.y)).collect();
    if let Some(c) = parse_size(&resolution_chosen(world)) {
        if !sizes.contains(&c) {
            sizes.push(c);
            sizes.sort_by_key(|s| (u64::from(s.x) * u64::from(s.y), s.x));
        }
    }
    sizes.into_iter().map(size_text).collect()
}

/// The resolution in use: the window's, or full screen's (the monitor's own when it is
/// borderless).
pub fn resolution_chosen(world: &World) -> String {
    let args = world.resource::<Args>();
    match (args.window, world.resource::<FullScreenSize>().0, world.resource::<MonitorModes>().desktop) {
        (WindowChoice::Windowed, ..) => parse_size(&args.size).map_or(args.size.clone(), size_text),
        (WindowChoice::Borderless, Some(s), _) | (WindowChoice::Borderless, None, Some(s)) => size_text(s),
        (WindowChoice::Borderless, None, None) => "desktop".to_string(),
    }
}

/// Take a resolution from the row: the window's size, or full screen's.
pub fn resolution_pick(world: &mut World, name: &str) -> bool {
    let Some(s) = parse_size(name) else { return false };
    if world.resource::<Args>().window == WindowChoice::Windowed {
        world.resource_mut::<Args>().size = size_text(s);
    } else {
        let desktop = world.resource::<MonitorModes>().desktop;
        world.resource_mut::<FullScreenSize>().0 = (desktop != Some(s)).then_some(s);
    }
    true
}

/// Both resolutions as the INI keeps them: `<window>;<full screen>`.
pub fn resolution_save(world: &World) -> String {
    let full = world.resource::<FullScreenSize>().0.map_or("desktop".to_string(), size_text);
    format!("{};{full}", world.resource::<Args>().size)
}

/// Both resolutions from the INI's text (a window's size alone also does).
pub fn resolution_load(world: &mut World, text: &str) -> bool {
    let (window, full) = text.split_once(';').unwrap_or((text, ""));
    let Some(window) = parse_size(window) else { return false };
    let full = match full.trim() {
        "" => world.resource::<FullScreenSize>().0,
        f if f.eq_ignore_ascii_case("desktop") => None,
        f => match parse_size(f) {
            Some(s) => Some(s),
            None => return false,
        },
    };
    world.resource_mut::<Args>().size = size_text(window);
    world.resource_mut::<FullScreenSize>().0 = full;
    true
}

/// The Display "Monitor" list's choices: each display as "Display N (WxH)".
pub fn monitor_names(world: &World) -> Vec<String> {
    world.resource::<MonitorModes>().monitors.iter().enumerate().map(|(i, m)| monitor_label(i, m)).collect()
}

/// The chosen display: the saved choice while it is still attached, else the first display.
pub fn monitor_chosen(world: &World) -> String {
    let m = world.resource::<MonitorModes>();
    let c = world.resource::<MonitorChoice>();
    let idx = match &c.0 {
        Some(name) => m.monitors.iter().position(|m| &m.name == name).unwrap_or(0),
        None => 0,
    };
    m.monitors.get(idx).map(|m| monitor_label(idx, m)).unwrap_or_default()
}

/// Choose a display by its label; false when it is not attached now.
pub fn monitor_pick(world: &mut World, label: &str) -> bool {
    let found = world.resource::<MonitorModes>().monitors.iter().enumerate().find(|(i, m)| monitor_label(*i, m) == label).map(|(_, m)| m.name.clone());
    let Some(name) = found else { return false };
    world.resource_mut::<MonitorChoice>().0 = Some(name);
    true
}

/// The monitor choice as the INI keeps it: its connector name, or "" for none (the first
/// display).
pub fn monitor_save(world: &World) -> String {
    world.resource::<MonitorChoice>().0.clone().unwrap_or_default()
}

/// The monitor choice from its INI text; "" is none (the first display).
pub fn monitor_load(world: &mut World, text: &str) -> bool {
    let name = text.trim();
    world.resource_mut::<MonitorChoice>().0 = (!name.is_empty()).then(|| name.to_string());
    true
}

/// Note the window's monitor for the resolution list (its video modes and, while not in
/// exclusive full screen, its size), and the window's actual monitor (from `OnMonitor`, for
/// comparing against a chosen display).
fn watch_monitor(windows: Query<(&Window, Option<&OnMonitor>), With<PrimaryWindow>>, monitors: Query<(Entity, &Monitor, Has<PrimaryMonitor>)>, mut modes: ResMut<MonitorModes>) {
    let Ok((w, on)) = windows.single() else { return };
    let monitor = on.and_then(|o| monitors.get(o.0).ok()).or_else(|| monitors.iter().find(|m| m.2)).or_else(|| monitors.iter().next());
    let Some((_, m, _)) = monitor else { return };
    let desktop = if matches!(w.mode, WindowMode::Fullscreen(..)) { modes.desktop } else { Some(m.physical_size()) };
    let all = ordered(monitors.iter().map(|(e, m, _)| MonitorInfo { name: m.name.clone().unwrap_or_default(), entity: e, size: m.physical_size(), position: m.physical_position }).collect());
    let current = on.map(|o| o.0);
    if modes.desktop != desktop || modes.modes != m.video_modes || modes.monitors != all || modes.current != current {
        info!("window: monitor {} {}, {} video modes", m.name.as_deref().unwrap_or("?"), desktop.map_or("?".to_string(), size_text), m.video_modes.len());
        modes.desktop = desktop;
        modes.modes = m.video_modes.clone();
        modes.monitors = all;
        modes.current = current;
    }
}

/// What [`apply_display`] last made the window: the mode, the window size, the full-screen
/// size and the chosen monitor.
type DisplayWant = (WindowChoice, String, Option<UVec2>, Option<String>);

/// [`apply_display`]'s frame-to-frame state.
#[derive(Default)]
struct DisplayState {
    /// The frame after a monitor move bounced through windowed: the target mode to enter.
    bounce: Option<WindowMode>,
}

/// Make the window what `Args` (`window`, `size`) and [`FullScreenSize`] ask, when they
/// change (and again once the monitor's modes are known, for exclusive full screen).
fn apply_display(args: Res<Args>, full: Res<FullScreenSize>, modes: Res<MonitorModes>, monitor: Res<MonitorChoice>, mut windows: Query<&mut Window, With<PrimaryWindow>>, mut last: Local<Option<DisplayWant>>, mut state: Local<DisplayState>) {
    // The frame after a monitor move bounced through windowed: enter the target mode.
    if let Some(mode) = state.bounce {
        if let Ok(mut w) = windows.single_mut() {
            if w.mode != mode {
                w.mode = mode;
            }
            state.bounce = None;
        }
        return;
    }
    let want = (args.window, args.size.clone(), full.0, monitor.0.clone());
    // Re-applied when the choice or the monitors change: a chosen monitor's entity is only
    // known after the monitors are discovered, a frame or so after the start.
    if last.as_ref() == Some(&want) && !modes.is_changed() {
        return;
    }
    *last = Some(want);
    let Ok(mut w) = windows.single_mut() else { return };
    let selection = borderless_selection(&modes, &monitor);
    let mode = match (args.window, full.0) {
        (WindowChoice::Windowed, _) => WindowMode::Windowed,
        (WindowChoice::Borderless, None) => WindowMode::BorderlessFullscreen(selection),
        (WindowChoice::Borderless, Some(s)) => match modes.mode_of(s) {
            Some(m) => WindowMode::Fullscreen(selection, VideoModeSelection::Specific(m)),
            None => {
                // Not known yet (or not the monitor's): borderless until it is.
                if !modes.modes.is_empty() {
                    warn!("window: the monitor has no video mode {}; borderless full screen instead", size_text(s));
                }
                WindowMode::BorderlessFullscreen(selection)
            }
        },
    };
    if w.mode != mode {
        // Moving full screen to a different display: the compositor ignores a full-screen
        // request to another output while already full screen [observed: #1641], so bounce
        // through windowed (one frame) and re-enter on the next. Only when the chosen
        // display is a specific monitor other than the one the window is already on.
        let target = match selection {
            MonitorSelection::Entity(e) => Some(e),
            _ => None,
        };
        let moving = target.is_some() && modes.current != target
            && matches!(w.mode, WindowMode::BorderlessFullscreen(_) | WindowMode::Fullscreen(..))
            && matches!(mode, WindowMode::BorderlessFullscreen(_) | WindowMode::Fullscreen(..));
        if moving {
            w.mode = WindowMode::Windowed;
            state.bounce = Some(mode);
        } else {
            w.mode = mode;
        }
    }
    // A window of --size (leaving full screen keeps no size of its own [observed:
    // 800x600]). main checked --size at the start.
    if let (WindowMode::Windowed, Some(s)) = (mode, parse_size(&args.size)) {
        if w.resolution.physical_size() != s {
            w.resolution.set_physical_resolution(s.x, s.y);
        }
    }
}

/// One line each time the primary window's mode changes (the first: the start's), and
/// each time a window takes the size asked (not each size while its frame is dragged).
fn log_mode(args: Res<Args>, windows: Query<&Window, (With<PrimaryWindow>, Changed<Window>)>, mut last: Local<Option<(WindowMode, UVec2)>>) {
    let Ok(w) = windows.single() else { return };
    let size = w.resolution.physical_size();
    let now = (w.mode, size);
    if *last == Some(now) || (last.is_some_and(|l| l.0 == w.mode) && (w.mode != WindowMode::Windowed || parse_size(&args.size) != Some(size))) {
        return;
    }
    *last = Some(now);
    match w.mode {
        WindowMode::Windowed => info!("window: windowed, {}x{}", w.resolution.width(), w.resolution.height()),
        WindowMode::BorderlessFullscreen(_) => info!("window: borderless full screen"),
        WindowMode::Fullscreen(_, VideoModeSelection::Specific(m)) => info!("window: exclusive full screen, {} at {:.0} Hz", size_text(m.physical_size), m.refresh_rate_millihertz as f32 / 1000.0),
        WindowMode::Fullscreen(..) => info!("window: exclusive full screen"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_and_write_as_wxh() {
        assert_eq!(parse_size(" 1600x900 "), Some(UVec2::new(1600, 900)));
        assert_eq!(parse_size("1600X900"), Some(UVec2::new(1600, 900)));
        assert_eq!(parse_size("100x100"), None);
        assert_eq!(parse_size("wide"), None);
        assert_eq!(size_text(UVec2::new(1280, 800)), "1280x800");
    }

    #[test]
    fn the_monitor_sizes_are_listed_smallest_first_once_each() {
        let mode = |x, y, hz| VideoMode { physical_size: UVec2::new(x, y), bit_depth: 32, refresh_rate_millihertz: hz };
        let m = MonitorModes { desktop: Some(UVec2::new(1920, 1080)), modes: vec![mode(1280, 720, 60000), mode(1920, 1080, 60000), mode(1920, 1080, 144000), mode(640, 400, 60000), mode(1600, 900, 60000)], ..default() };
        assert_eq!(m.sizes(), [UVec2::new(1280, 720), UVec2::new(1600, 900), UVec2::new(1920, 1080)]);
        assert_eq!(m.mode_of(UVec2::new(1920, 1080)).map(|m| m.refresh_rate_millihertz), Some(144000), "the highest refresh rate");
        assert_eq!(m.mode_of(UVec2::new(1024, 768)), None);
    }

    #[test]
    fn borderless_opens_on_the_chosen_display_else_the_first() {
        let e1 = Entity::from_raw_u32(1).unwrap();
        let e2 = Entity::from_raw_u32(2).unwrap();
        let info = |name: &str, e| MonitorInfo { name: name.to_string(), entity: e, size: UVec2::new(1920, 1080), position: IVec2::ZERO };
        let modes = MonitorModes { monitors: vec![info("DP-1", e1), info("HDMI-A-1", e2)], ..default() };
        // No choice: the first display.
        assert_eq!(borderless_selection(&modes, &MonitorChoice(None)), MonitorSelection::Entity(e1));
        // A chosen display that is attached: its entity.
        assert_eq!(borderless_selection(&modes, &MonitorChoice(Some("HDMI-A-1".into()))), MonitorSelection::Entity(e2));
        // A chosen display that is not attached now (unplugged): the first display.
        assert_eq!(borderless_selection(&modes, &MonitorChoice(Some("DP-2".into()))), MonitorSelection::Entity(e1));
        // No monitors known yet: the initial (primary) selection.
        assert_eq!(borderless_selection(&MonitorModes::default(), &MonitorChoice(None)), MonitorSelection::Primary);
    }

    #[test]
    fn the_monitor_list_picks_a_display_by_label() {
        let mut world = World::new();
        world.init_resource::<MonitorModes>();
        world.init_resource::<MonitorChoice>();
        world.resource_mut::<MonitorModes>().monitors = vec![MonitorInfo { name: "HDMI-A-1".to_string(), entity: Entity::from_raw_u32(1).unwrap(), size: UVec2::new(1920, 1080), position: IVec2::ZERO }];
        // The list shows each display's label, and the default (nothing saved) is the first.
        assert_eq!(monitor_names(&world), vec!["Display 1 (1920x1080)".to_string()]);
        assert_eq!(monitor_chosen(&world), "Display 1 (1920x1080)");
        // Pick it by label; a label that is not attached is refused.
        assert!(monitor_pick(&mut world, "Display 1 (1920x1080)"));
        assert_eq!(monitor_chosen(&world), "Display 1 (1920x1080)");
        assert!(!monitor_pick(&mut world, "Display 2 (2560x1440)"));
        // A saved display that is then unplugged reads as the first (the effective selection).
        world.resource_mut::<MonitorModes>().monitors.clear();
        assert_eq!(monitor_chosen(&world), "");
    }

    #[test]
    fn displays_are_ordered_left_to_right_then_top_to_bottom() {
        let m = |x, y| MonitorInfo { name: format!("({x},{y})"), entity: Entity::PLACEHOLDER, size: UVec2::ZERO, position: IVec2::new(x, y) };
        let ordered = ordered(vec![m(0, 100), m(-200, 0), m(0, 0), m(200, 0)]);
        let pos: Vec<IVec2> = ordered.iter().map(|m| m.position).collect();
        assert_eq!(pos, vec![IVec2::new(-200, 0), IVec2::new(0, 0), IVec2::new(0, 100), IVec2::new(200, 0)]);
    }
}
