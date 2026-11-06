//! The interface's two colours as settings: the one the HUD and the menus are drawn in,
//! and the one of the Pip-Boy's screens (`crate::interface::hud_rgb`, `pipboy_rgb`). Each
//! is a row of named colours on the settings page (Amber, the default, and a few others;
//! "Custom" once the sliders leave them) over a slider for each of red, green and blue
//! (0 to 255), kept in `[Settings]` of `NVForever.ini` like every row (`crate::settings`):
//! `hud-colour=Amber`, `hud-red=1.00`, … The page's Defaults puts both back to amber.
//!
//! Not the original's settings: its own HUD and Pip-Boy colour choices are not copied
//! [design: owner 2026-10-26, "rgb colour pickers" for the HUD and the Pip-Boy, amber
//! the default]. The colours of the presets other than amber are NVForever's [design].
//!
//! What is drawn already is recoloured when a colour changes ([`recolour`]): every UI
//! colour that is the old colour at some brightness becomes the new one at that
//! brightness, the Pip-Boy's screens (under `crate::pipboy::PipBoyRoot`) by its own
//! colour, everything else by the HUD's. What is drawn later takes the new colour from
//! `amber` and `pipboy_amber`. The mouse pointer keeps the amber it was made in
//! (`crate::cursor`). The Pip-Boy device's background follows its colour by its own owner,
//! whose nodes are left alone here (`crate::pipboy::device_screen`).

use crate::interface::{hud_rgb, pipboy_rgb, set_hud_rgb, set_pipboy_rgb, AMBER_RGB};
use crate::settings::{add_choice_list, add_setting, ChoiceList, Setting, Slider};
use bevy::prelude::*;

/// The settings page's heading for these rows.
pub const GROUP: &str = "Interface";

/// The named colours, the default first [design, module notes].
pub const PRESETS: [(&str, [u8; 3]); 4] = [("Amber", AMBER_RGB), ("Green", [26, 255, 128]), ("Blue", [46, 207, 255]), ("White", [230, 240, 255])];

/// What the list shows for a colour that is none of [`PRESETS`].
pub const CUSTOM: &str = "Custom";

/// Which of the two colours: 0 the HUD's, 1 the Pip-Boy's.
fn rgb<const W: usize>() -> [u8; 3] {
    if W == 0 {
        hud_rgb()
    } else {
        pipboy_rgb()
    }
}

fn set_rgb<const W: usize>(rgb: [u8; 3]) {
    if W == 0 {
        set_hud_rgb(rgb)
    } else {
        set_pipboy_rgb(rgb)
    }
}

/// The preset's name for `rgb`, else [`CUSTOM`].
pub fn preset_name(rgb: [u8; 3]) -> &'static str {
    PRESETS.iter().find(|(_, c)| *c == rgb).map_or(CUSTOM, |(n, _)| n)
}

fn names<const W: usize>(_: &World) -> Vec<String> {
    let mut out: Vec<String> = PRESETS.iter().map(|(n, _)| n.to_string()).collect();
    if preset_name(rgb::<W>()) == CUSTOM {
        out.push(CUSTOM.to_string());
    }
    out
}

fn chosen<const W: usize>(_: &World) -> String {
    preset_name(rgb::<W>()).to_string()
}

/// Take a preset by name; [`CUSTOM`] keeps the colour as it is (the sliders say which).
fn pick<const W: usize>(_: &mut World, name: &str) -> bool {
    if name == CUSTOM {
        return true;
    }
    match PRESETS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        Some((_, c)) => {
            set_rgb::<W>(*c);
            true
        }
        None => false,
    }
}

fn save<const W: usize>(w: &World) -> String {
    chosen::<W>(w)
}

/// Channel `C` (0 red, 1 green, 2 blue) as a slider's part.
fn channel<const W: usize, const C: usize>(_: &World) -> f32 {
    // Rounded as `Slider::fit` keeps a part, so a stored value compares equal.
    (f32::from(rgb::<W>()[C]) / 255.0 * 1000.0).round() / 1000.0
}

fn set_channel<const W: usize, const C: usize>(_: &mut World, v: f32) {
    let mut c = rgb::<W>();
    c[C] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    set_rgb::<W>(c);
}

/// A slider's part as the 0 to 255 the page shows.
fn shown(v: f32) -> String {
    format!("{:.0}", v * 255.0)
}

fn slider<const W: usize, const C: usize>() -> Slider {
    Slider { get: channel::<W, C>, set: set_channel::<W, C>, min: 0.0, step: 1.0 / 255.0, shown: Some(shown) }
}

fn rows<const W: usize>(app: &mut App, key: &'static str, label: &'static str, note: &'static str, channels: [(&'static str, &'static str); 3]) {
    add_choice_list(
        app,
        Setting { key, group: GROUP, label, note, get: |_| false, set: |_, _| {}, slider: None },
        ChoiceList { names: names::<W>, chosen: chosen::<W>, pick: pick::<W>, save: save::<W>, load: pick::<W>, ask_keep: false },
    );
    let sliders = [slider::<W, 0>(), slider::<W, 1>(), slider::<W, 2>()];
    for ((key, label), s) in channels.into_iter().zip(sliders) {
        add_setting(app, Setting::slider(key, GROUP, label, "One part of the colour above, 0 to 255; drag the meter or use Left and Right.", s));
    }
}

/// `c` is `old` at some brightness (not black): the same brightness of `new`, with `c`'s
/// alpha; else `None`.
pub fn retint(c: Color, old: [u8; 3], new: [u8; 3]) -> Option<Color> {
    let s = c.to_srgba();
    let ch = [s.red, s.green, s.blue];
    let o = old.map(|x| f32::from(x) / 255.0);
    let m = (0..3).max_by(|&a, &b| o[a].total_cmp(&o[b]))?;
    if o[m] <= 0.0 || ch[m] <= 0.0 {
        return None;
    }
    let b = ch[m] / o[m];
    if b > 1.0 + 2e-3 || (0..3).any(|i| (ch[i] - o[i] * b).abs() > 1.5 / 255.0) {
        return None;
    }
    let n = new.map(|x| f32::from(x) / 255.0 * b);
    Some(Color::srgba(n[0], n[1], n[2], s.alpha))
}

/// The colours the drawn interface has now (module notes).
#[derive(Resource, Clone, Copy)]
struct Drawn {
    hud: [u8; 3],
    pipboy: [u8; 3],
}

/// Recolour what is drawn when a colour changed (module notes).
#[allow(clippy::type_complexity)]
fn recolour(
    mut drawn: ResMut<Drawn>,
    roots: Query<Entity, With<crate::pipboy::PipBoyRoot>>,
    children: Query<&Children>,
    mut nodes: Query<(Entity, Option<&mut TextColor>, Option<&mut ImageNode>, Option<&mut BackgroundColor>, Option<&mut BorderColor>), (Or<(With<TextColor>, With<ImageNode>, With<BackgroundColor>, With<BorderColor>)>, Without<crate::pipboy::device_screen::ScreenFxNode>)>,
) {
    let now = Drawn { hud: hud_rgb(), pipboy: pipboy_rgb() };
    if now.hud == drawn.hud && now.pipboy == drawn.pipboy {
        return;
    }
    let pipboy: Vec<Entity> = roots.iter().flat_map(|r| std::iter::once(r).chain(children.iter_descendants(r))).collect();
    for (e, text, image, back, border) in &mut nodes {
        let (old, new) = if pipboy.contains(&e) { (drawn.pipboy, now.pipboy) } else { (drawn.hud, now.hud) };
        if old == new {
            continue;
        }
        let fix = |c: &mut Color| {
            if let Some(n) = retint(*c, old, new) {
                *c = n;
            }
        };
        if let Some(mut t) = text {
            fix(&mut t.0);
        }
        if let Some(mut i) = image {
            fix(&mut i.color);
        }
        if let Some(mut b) = back {
            fix(&mut b.0);
        }
        if let Some(mut b) = border {
            let b = &mut *b;
            for c in [&mut b.top, &mut b.right, &mut b.bottom, &mut b.left] {
                fix(c);
            }
        }
    }
    info!("interface colours: HUD {:?}, Pip-Boy {:?}", now.hud, now.pipboy);
    *drawn = now;
}

pub struct UiColourPlugin;

impl Plugin for UiColourPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Drawn { hud: hud_rgb(), pipboy: pipboy_rgb() }).add_systems(Update, recolour.after(crate::PlaySet::Ui).before(crate::PlaySet::Sync));
        rows::<0>(
            app,
            "hud-colour",
            "HUD colour",
            "The colour of the HUD and the menus: Amber (the default) or another; the sliders below set it to any colour.",
            [("hud-red", "HUD red"), ("hud-green", "HUD green"), ("hud-blue", "HUD blue")],
        );
        rows::<1>(
            app,
            "pipboy-colour",
            "Pip-Boy colour",
            "The colour of the Pip-Boy's screens (items, stats, data): Amber (the default) or another; the sliders below set it to any colour.",
            [("pipboy-red", "Pip-Boy red"), ("pipboy-green", "Pip-Boy green"), ("pipboy-blue", "Pip-Boy blue")],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_is_named_by_its_preset_else_custom() {
        assert_eq!(preset_name(AMBER_RGB), "Amber");
        assert_eq!(preset_name([26, 255, 128]), "Green");
        assert_eq!(preset_name([1, 2, 3]), CUSTOM);
        assert_eq!(PRESETS[0], ("Amber", [255, 182, 66]), "amber is the default");
    }

    #[test]
    fn a_slider_part_is_a_channel_and_survives_its_steps() {
        let s = slider::<0, 0>();
        for x in [0u8, 1, 66, 128, 182, 254, 255] {
            let v = s.fit(f32::from(x) / 255.0);
            assert_eq!((v * 255.0).round() as u8, x, "{x} on a step");
            assert_eq!(shown(v), x.to_string());
        }
    }

    #[test]
    fn the_device_screens_nodes_are_left_to_their_owner() {
        use crate::pipboy::device_screen::ScreenFxNode;
        // A colour no other test sets, so a change is seen whatever the global colours are.
        let old = [10, 20, 30];
        let full = crate::interface::tint(old, 255.0);
        let mut app = App::new();
        app.insert_resource(Drawn { hud: old, pipboy: old }).add_systems(Update, recolour);
        let hud = app.world_mut().spawn(BackgroundColor(full)).id();
        let fx = app.world_mut().spawn((ScreenFxNode, BackgroundColor(full), ImageNode { color: full, ..default() })).id();
        app.update();
        assert_ne!(app.world().get::<BackgroundColor>(hud).unwrap().0, full, "the HUD's takes the new colour");
        assert_eq!(app.world().get::<BackgroundColor>(fx).unwrap().0, full);
        assert_eq!(app.world().get::<ImageNode>(fx).unwrap().color, full);
    }

    #[test]
    fn only_the_old_colour_at_a_brightness_is_recoloured() {
        let green = [26, 255, 128];
        let half = crate::interface::tint(AMBER_RGB, 128.0).with_alpha(0.5);
        let got = retint(half, AMBER_RGB, green).unwrap().to_srgba();
        let want = crate::interface::tint(green, 128.0).to_srgba();
        assert!((got.red - want.red).abs() < 1e-4 && (got.green - want.green).abs() < 1e-4 && (got.blue - want.blue).abs() < 1e-4, "{got:?} {want:?}");
        assert_eq!(got.alpha, 0.5, "alpha kept");
        assert!(retint(Color::BLACK, AMBER_RGB, green).is_none(), "black stays");
        assert!(retint(Color::srgb(0.8, 0.1, 0.1), AMBER_RGB, green).is_none(), "another colour stays");
        assert!(retint(Color::WHITE, AMBER_RGB, green).is_none(), "brighter than the colour: not it");
    }
}
