//! The window's mode: an ordinary window (`--window windowed`, the default) or borderless
//! full screen on the monitor the window is on (`--window borderless`). The settings
//! page's Display group switches it while the game runs (`settings.rs`), kept in
//! `NVForever.ini` like every switch. Each mode the primary window takes is logged.
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
    /// Borderless full screen on the current monitor (its own resolution; --size is not
    /// used then, screenshots too are the monitor's size).
    Borderless,
}

impl WindowChoice {
    pub fn mode(self) -> WindowMode {
        match self {
            WindowChoice::Windowed => WindowMode::Windowed,
            WindowChoice::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
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

pub struct WindowModePlugin;

/// Apply the chosen window settings before telemetry snapshots their physical size.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApplyWindowMode;

impl Plugin for WindowModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FullScreenSize>().init_resource::<MonitorModes>().add_systems(Update, (watch_monitor, apply_display, log_mode).chain().in_set(ApplyWindowMode));
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

/// Note the monitor the window is on (else the primary one): its video modes, and its
/// size while the window is not in exclusive full screen (then it is the desktop's).
fn watch_monitor(windows: Query<(&Window, Option<&OnMonitor>), With<PrimaryWindow>>, monitors: Query<(Entity, &Monitor, Has<PrimaryMonitor>)>, mut modes: ResMut<MonitorModes>) {
    let Ok((w, on)) = windows.single() else { return };
    let monitor = on.and_then(|o| monitors.get(o.0).ok()).or_else(|| monitors.iter().find(|m| m.2)).or_else(|| monitors.iter().next());
    let Some((_, m, _)) = monitor else { return };
    let desktop = if matches!(w.mode, WindowMode::Fullscreen(..)) { modes.desktop } else { Some(m.physical_size()) };
    if modes.desktop != desktop || modes.modes != m.video_modes {
        info!("window: monitor {} {}, {} video modes", m.name.as_deref().unwrap_or("?"), desktop.map_or("?".to_string(), size_text), m.video_modes.len());
        modes.desktop = desktop;
        modes.modes = m.video_modes.clone();
    }
}

/// Make the window what `Args` (`window`, `size`) and [`FullScreenSize`] ask, when they
/// change (and again once the monitor's modes are known, for exclusive full screen).
fn apply_display(args: Res<Args>, full: Res<FullScreenSize>, modes: Res<MonitorModes>, mut windows: Query<&mut Window, With<PrimaryWindow>>, mut last: Local<Option<(WindowChoice, String, Option<UVec2>)>>, mut waiting: Local<bool>) {
    let want = (args.window, args.size.clone(), full.0);
    if last.as_ref() == Some(&want) && !(*waiting && modes.is_changed()) {
        return;
    }
    *last = Some(want);
    *waiting = false;
    let Ok(mut w) = windows.single_mut() else { return };
    let mode = match (args.window, full.0) {
        (WindowChoice::Windowed, _) => WindowMode::Windowed,
        (WindowChoice::Borderless, None) => WindowChoice::Borderless.mode(),
        (WindowChoice::Borderless, Some(s)) => match modes.mode_of(s) {
            Some(m) => WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Specific(m)),
            None => {
                // Not known yet (or not the monitor's): borderless until it is.
                *waiting = modes.modes.is_empty();
                if !*waiting {
                    warn!("window: the monitor has no video mode {}; borderless full screen instead", size_text(s));
                }
                WindowChoice::Borderless.mode()
            }
        },
    };
    if w.mode != mode {
        w.mode = mode;
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
        WindowMode::BorderlessFullscreen(_) => info!("window: borderless full screen on the current monitor"),
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
        let m = MonitorModes { desktop: Some(UVec2::new(1920, 1080)), modes: vec![mode(1280, 720, 60000), mode(1920, 1080, 60000), mode(1920, 1080, 144000), mode(640, 400, 60000), mode(1600, 900, 60000)] };
        assert_eq!(m.sizes(), [UVec2::new(1280, 720), UVec2::new(1600, 900), UVec2::new(1920, 1080)]);
        assert_eq!(m.mode_of(UVec2::new(1920, 1080)).map(|m| m.refresh_rate_millihertz), Some(144000), "the highest refresh rate");
        assert_eq!(m.mode_of(UVec2::new(1024, 768)), None);
    }
}
