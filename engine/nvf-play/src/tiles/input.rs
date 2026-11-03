//! T8: the pointer, the mouse buttons and the keys fed to the menus drawn from tiles
//! (`fnv_game::ui_input`, `fnv_ui::input`), each frame before scripts [design:
//! ui-tile-tree.md §4.1 step 1].
//!
//! - **Which menus take input**: a menu drawn from its file that the player works: since
//!   T15a the message box and the "How many?" prompt while drawn from tiles
//!   (`[UI] TileMenus=MessageMenu,QuantityMenu`, `super::menus`), the prompt under the
//!   box; since T11 the start menu while the pause menu's Mods page is up
//!   (`[UI] TileMenus=StartMenu`, `super::mods`). The HUD (`[UI] TileMenus=HUDMainMenu`) is drawn from tiles but has no pointer in
//!   play [hypothesis: the original's HUD takes no clicks]. With none, nothing is fed and
//!   the frame does no tree work [design: the maintainer's review of #902], unless the test
//!   aid below makes the HUD take input; but a script's ClickMenuButton or ClickMenuTile
//!   still ends (`clicked` back to 0) and fires its event the next frame
//!   (`fnv_game::ui_input::idle`) [design: the maintainer's review of #957].
//! - **What the clicks do** (T15a, [`MenuInputs`]): a pointer's or a script's click on a
//!   message box row answers the box with that button ([`MessageClick`], read by
//!   `crate::script_hud::menu_keys`), and the pointer over a row frames it (the hand-built
//!   box's rule, `crate::message_menu`); on the prompt OK, Cancel and the two arrows send
//!   their commands (`crate::menu::MenuInput`) and a press on the meter, held as it drags,
//!   sets the amount under the pointer (`crate::menu::model::amount_at`). The hand-built
//!   keys (`crate::menu`'s prompt keys, `crate::script_hud::menu_keys`) stay the menus'
//!   keys: a click a key made through the files' `_PCButton_*` or `x…` refs acts unless
//!   that key came from the keyboard (or a controller standing in for it) and the
//!   hand-built handler took it this frame (`super::menus::key_click_acts`), so a press acts
//!   once while a script's MenuTapKey and a file's own bindings still act; a highlight a
//!   ref moved for a hand-built Up or Down is not taken as the box's choice
//!   (`super::menus::chosen_row`) [design: the T15a review].
//! - **The Wait / Sleep menu** (T15b, `super::wait_menu`): its Wait and Cancel buttons and
//!   the slider's arrows and page areas send the hand-built menu's commands
//!   (`crate::wait::WaitInput`), a press on the marker, held as it drags, sets the hours
//!   under the pointer; the slider's value is written again after them.
//! - **Units**: the cursor's logical pixels to interface units, 960 to the view's height,
//!   as the tile renderer draws (`super::draw`, `crate::interface::u`) [design].
//! - **Keys**: A to Z as `_PCButton_<letter>` [documented: the vanilla files' letters;
//!   LStewieAl's Tweaks adds digits, F-keys, Caps, Tab and Alt, so the unmodded game has
//!   none of those]; the arrow keys as `xup` … `xright` [documented: Input]; the
//!   controller's buttons as their `x…` traits (A `xbuttona`, the shoulders `xbuttonlb`
//!   `xbuttonrb`, the triggers `xbuttonlt` `xbuttonrt`, the sticks' buttons, Start, the
//!   D-pad) [design: the Xbox names the files use]; MenuTapKey's keys (`Game::ui_taps`)
//!   as one frame's presses [documented: MenuTapKey, "fakes a key press for one frame in
//!   menu mode"]. A press is a key going down [hypothesis].
//! - The text tiles without a size are not hit: the fonts' measure lives in the renderer
//!   (`super::draw`), and the HUD's measured texts have their width written into the tree
//!   (`FontKit::write_widths`) [design].
//!
//! Test aid `NVF_PLAY_TEST_POINTER=<seconds>:<x>:<y>[:click][,…]`: from that time the HUD
//! (drawn from tiles) takes input as an open menu would, with the pointer at (x, y) in
//! interface units, and `click` a left-button press on that frame; `<seconds>:off` lets
//! it go. The mouse, keys and controller then reach it too (`NVF_PLAY_TEST_KEYS`).

use crate::script::ScriptState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_ui::input::{no_measure, Input};

pub const AID: &str = "NVF_PLAY_TEST_POINTER";

/// One item of the test aid: a place and whether it clicks, or off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AidPointer {
    At { x: f64, y: f64, click: bool },
    Off,
}

/// `<x>:<y>[:click]` or `off`.
pub fn aid_pointer(rest: &str) -> Result<AidPointer, String> {
    if rest.trim().eq_ignore_ascii_case("off") {
        return Ok(AidPointer::Off);
    }
    let mut parts = rest.split(':').map(str::trim);
    let num = |s: Option<&str>| s.and_then(|s| s.parse::<f64>().ok()).ok_or_else(|| format!("{rest:?}: expected <x>:<y>[:click] or off"));
    let x = num(parts.next())?;
    let y = num(parts.next())?;
    let click = match parts.next() {
        None => false,
        Some(c) if c.eq_ignore_ascii_case("click") => true,
        Some(c) => return Err(format!("{rest:?}: unknown {c:?}")),
    };
    Ok(AidPointer::At { x, y, click })
}

/// The test aid's state: what is still to come, and the pointer it holds now.
#[derive(Resource, Default)]
pub(crate) struct PointerAid {
    due: Vec<(f32, AidPointer)>,
    now: Option<(f64, f64)>,
}

/// The letters' keys, A to Z.
const LETTERS: [(KeyCode, &str); 26] = [
    (KeyCode::KeyA, "A"), (KeyCode::KeyB, "B"), (KeyCode::KeyC, "C"), (KeyCode::KeyD, "D"), (KeyCode::KeyE, "E"), (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"), (KeyCode::KeyH, "H"), (KeyCode::KeyI, "I"), (KeyCode::KeyJ, "J"), (KeyCode::KeyK, "K"), (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"), (KeyCode::KeyN, "N"), (KeyCode::KeyO, "O"), (KeyCode::KeyP, "P"), (KeyCode::KeyQ, "Q"), (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"), (KeyCode::KeyT, "T"), (KeyCode::KeyU, "U"), (KeyCode::KeyV, "V"), (KeyCode::KeyW, "W"), (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"), (KeyCode::KeyZ, "Z"),
];

const ARROWS: [(KeyCode, &str); 4] = [(KeyCode::ArrowUp, "xup"), (KeyCode::ArrowDown, "xdown"), (KeyCode::ArrowLeft, "xleft"), (KeyCode::ArrowRight, "xright")];

const PAD: [(GamepadButton, &str); 15] = [
    (GamepadButton::South, "xbuttona"),
    (GamepadButton::East, "xbuttonb"),
    (GamepadButton::West, "xbuttonx"),
    (GamepadButton::North, "xbuttony"),
    (GamepadButton::LeftTrigger2, "xbuttonlt"),
    (GamepadButton::RightTrigger2, "xbuttonrt"),
    (GamepadButton::LeftTrigger, "xbuttonlb"),
    (GamepadButton::RightTrigger, "xbuttonrb"),
    (GamepadButton::LeftThumb, "xbuttonls"),
    (GamepadButton::RightThumb, "xbuttonrs"),
    (GamepadButton::Start, "xbuttonstart"),
    (GamepadButton::DPadUp, "xup"),
    (GamepadButton::DPadDown, "xdown"),
    (GamepadButton::DPadLeft, "xleft"),
    (GamepadButton::DPadRight, "xright"),
];

/// A key's name for the tiles: a letter for `_PCButton_*`, or an arrow's trait.
pub fn key_name(k: KeyCode) -> Option<(bool, &'static str)> {
    LETTERS.iter().find(|(c, _)| *c == k).map(|(_, n)| (true, *n)).or_else(|| ARROWS.iter().find(|(c, _)| *c == k).map(|(_, n)| (false, *n)))
}

/// Adds a key once: a letter to the keys, an arrow or a button to the pads (the D-pad and
/// the arrow keys it stands for in menus, `crate::pad::menu_keys`, press `xup` once).
fn push(input: &mut Input, (letter, name): (bool, &'static str)) {
    let list = if letter { &mut input.keys } else { &mut input.pads };
    if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
    }
}

/// A window position (logical pixels from the top left) in interface units.
pub fn to_units(at: Vec2, window_height: f32) -> (f64, f64) {
    let per = f64::from(window_height.max(1.0)) / 960.0;
    (f64::from(at.x) / per, f64::from(at.y) / per)
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Devices<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    pads: Query<'w, 's, &'static Gamepad>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

/// T15a: the message box button the pointer or a script clicked on its tiles this frame,
/// for `crate::script_hud::menu_keys` to answer the box with (taken there).
#[derive(Resource, Default)]
pub struct MessageClick(pub Option<usize>);

/// T15a: what the clicks on the message box and the prompt drawn from tiles act on.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuInputs<'w, 's> {
    panel: Option<ResMut<'w, crate::message_menu::MessagePanel>>,
    click: ResMut<'w, MessageClick>,
    active: Option<Res<'w, crate::menu::ActiveMenu>>,
    commands: MessageWriter<'w, crate::menu::MenuInput>,
    /// A press on the prompt's meter, held: the amount follows the pointer.
    dragging: Local<'s, bool>,
    /// T15b: the Wait / Sleep menu, and its commands.
    wait: Option<Res<'w, crate::wait::WaitMenu>>,
    wait_commands: MessageWriter<'w, crate::wait::WaitInput>,
    /// A press on the Wait menu's slider, held: the hours follow the pointer.
    wait_dragging: Local<'s, bool>,
    /// The slider's left edge while it drags (`super::wait_menu::slider_left`), worked out
    /// at the press.
    wait_left: Local<'s, Option<f64>>,
    /// The hours the slider was written for this frame, for [`check_wait_drawn`].
    wait_check: ResMut<'w, WaitSliderCheck>,
}

impl MenuInputs<'_, '_> {
    fn quantity(&self) -> Option<crate::menu::model::Quantity> {
        self.active.as_ref().and_then(|a| a.0.as_ref()).and_then(|m| m.quantity)
    }

    /// The menus drawn from tiles that take input this frame, back to front: the prompt,
    /// then the message box over it.
    fn taking(&self, menus: super::vanilla::TileMenus, game: &fnv_game::Game<'_>) -> Vec<fnv_ui::TileId> {
        use super::menus::{message_tiled, quantity_tiled, MESSAGE_MENU, QUANTITY_MENU};
        let mut out = Vec::new();
        if quantity_tiled(menus.quantity, game, self.quantity().is_some()) {
            out.extend(game.ui_tree.menu(QUANTITY_MENU));
        }
        if message_tiled(menus.message, game) {
            out.extend(game.ui_tree.menu(MESSAGE_MENU));
        }
        if super::wait_menu::tiled(menus.sleep_wait, game) {
            out.extend(game.ui_tree.menu(super::wait_menu::SLEEP_WAIT_MENU));
        }
        out
    }

    /// T15b: act on a click on the Wait / Sleep menu's tiles: the hand-built menu's command
    /// for the piece (`super::wait_menu::slider_command`), unless a key made it that the
    /// hand-built menu took this frame. Returns the hours the slider then shows, for its
    /// value to be written again (`super::wait_menu::write_slider`).
    fn follow_wait(&mut self, ui: &fnv_ui::Ui, c: &fnv_ui::input::Click, at: Option<(f64, f64)>, held: bool, physical: &[&str]) -> Option<Option<u32>> {
        use super::wait_menu::{hours_at, slider_command, slider_left, wait_tile, WaitTile, SLEEP_WAIT_MENU};
        let model = self.wait.as_ref().and_then(|w| w.0.clone())?;
        let what = wait_tile(ui, c.tile)?;
        if let fnv_ui::input::By::Key(k) = &c.by {
            if !super::menus::key_click_acts(SLEEP_WAIT_MENU, k, physical) {
                return Some(None);
            }
        }
        let mut under = None;
        if what == WaitTile::Slider {
            *self.wait_dragging = held && matches!(c.by, fnv_ui::input::By::Pointer);
            *self.wait_left = slider_left(ui);
            under = at.zip(*self.wait_left).and_then(|((x, _), left)| hours_at(ui, left, x));
        }
        let Some(cmd) = slider_command(what, under) else { return Some(None) };
        info!("tiles: {SLEEP_WAIT_MENU} {cmd:?} on its tiles");
        self.wait_commands.write(crate::wait::WaitInput(cmd));
        let mut next = model.clone();
        next.apply(cmd);
        Some(Some(next.marker()))
    }

    /// Act on a frame's report (see the module doc): `at` the pointer, `held` the left
    /// button down, `physical` the keys from the keyboard (or a controller standing in for
    /// them) this frame.
    fn follow(&mut self, ui: &fnv_ui::Ui, report: &fnv_ui::input::Report, at: Option<(f64, f64)>, held: bool, physical: &[&str]) -> Option<u32> {
        use super::menus::{chosen_row, key_click_acts, message_pick, nav_by_hand, quantity_tile, row_button, QuantityTile, QUANTITY_MENU};
        use crate::menu::{model::Command, MenuInput};
        use fnv_ui::input::By;
        if let (Some(i), Some(panel)) = (chosen_row(ui, report, nav_by_hand(physical)), self.panel.as_mut()) {
            if panel.framed() != Some(i) {
                panel.choose(i);
            }
        }
        let q = self.quantity();
        if q.is_none() || !held {
            *self.dragging = false;
        }
        let mut clicked = false;
        let mut slider = None;
        if self.wait.as_ref().is_none_or(|w| w.0.is_none()) || !held {
            *self.wait_dragging = false;
        }
        if !*self.wait_dragging {
            *self.wait_left = None;
        }
        let mut wait_clicked = false;
        for c in report.clicks.iter().filter(|c| ui.is_live(c.tile)) {
            if let Some(hours) = self.follow_wait(ui, c, at, held, physical) {
                wait_clicked = true;
                slider = hours.or(slider);
                continue;
            }
            if row_button(ui, c.tile).is_some() {
                if let Some(i) = message_pick(ui, c, physical) {
                    info!("tiles: {} button {i} picked on its tiles", super::menus::MESSAGE_MENU);
                    self.click.0 = Some(i);
                }
                continue;
            }
            if let By::Key(k) = &c.by {
                if !key_click_acts(QUANTITY_MENU, k, physical) {
                    continue;
                }
            }
            let Some(q) = q else { continue };
            let cmd = match quantity_tile(ui, c.tile) {
                Some(QuantityTile::Ok) => Command::AmountOk,
                Some(QuantityTile::Cancel) => Command::AmountCancel,
                Some(QuantityTile::Decrease) => Command::AmountStep(-1),
                Some(QuantityTile::Increase) => Command::AmountStep(1),
                Some(QuantityTile::Meter) => {
                    *self.dragging = held && matches!(c.by, By::Pointer);
                    match amount_under(ui, at, q.max) {
                        Some(n) => Command::Amount(n),
                        None => continue,
                    }
                }
                None => continue,
            };
            clicked = true;
            info!("tiles: {} {cmd:?} on its tiles", super::menus::QUANTITY_MENU);
            self.commands.write(MenuInput(cmd));
        }
        // A drag: the amount under the pointer while the press holds.
        if let (true, false, Some(q)) = (*self.dragging, clicked, q) {
            if let Some(n) = amount_under(ui, at, q.max).filter(|&n| n != q.amount) {
                self.commands.write(MenuInput(Command::Amount(n)));
            }
        }
        // T15b: a drag on the Wait menu's slider, the hours under the pointer.
        let hours = self.wait.as_ref().and_then(|w| w.0.as_ref()).map(|m| m.hours);
        if let (true, false, Some(now), Some((x, _)), Some(left)) = (*self.wait_dragging, wait_clicked, hours, at, *self.wait_left) {
            if let Some(h) = super::wait_menu::hours_at(ui, left, x).filter(|&h| h != now) {
                self.wait_commands.write(crate::wait::WaitInput(crate::wait::model::Command::Hours(h)));
                slider = Some(h);
            }
        }
        slider
    }
}

/// T15b: the hours the Wait menu's slider was written for this frame (`feed`), and the
/// value last seen drawn.
#[derive(Resource, Default)]
pub(crate) struct WaitSliderCheck {
    wrote: Option<u32>,
    drawn: Option<f64>,
}

/// T15b: the slider's value once the tree is worked out after scripts (what is drawn),
/// logged when it changes, and when it is not the value written this frame for the model's
/// hours (smoke reads both: the model's value must outlast a click's pulse, #1307 review).
pub(crate) fn check_wait_drawn(ui: &fnv_ui::Ui, check: &mut WaitSliderCheck) {
    use super::wait_menu::{slider_shown, slider_value, SLEEP_WAIT_MENU};
    let Some(h) = check.wrote.take() else { return };
    let Some(v) = slider_shown(ui) else { return };
    if check.drawn != Some(v) {
        info!("tiles: {SLEEP_WAIT_MENU} slider drawn at {}", fnv_ui::Value::Num(v).text());
        check.drawn = Some(v);
    }
    if slider_value(h) != v {
        info!("tiles: {SLEEP_WAIT_MENU} slider drawn at {}, not the {} written", fnv_ui::Value::Num(v).text(), slider_value(h));
    }
}

/// The amount under the pointer on the prompt's meter: the part of the meter's length left
/// of it, of the stack (`crate::menu::model::amount_at`, as the hand-built meter) [design:
/// the file's own drag value reads a `dragx` whose units are not known].
fn amount_under(ui: &fnv_ui::Ui, at: Option<(f64, f64)>, most: i32) -> Option<i32> {
    let (x, _) = at?;
    let b = super::menus::meter_box(ui).filter(|b| b.width > 0.0)?;
    Some(crate::menu::model::amount_at(((x - b.left) / b.width) as f32, most))
}

pub(crate) fn feed(
    mut script: ResMut<ScriptState>,
    menus: Res<super::vanilla::TileMenus>,
    aid: Option<ResMut<PointerAid>>,
    time: crate::pause::AidTime,
    devices: Devices,
    mut inputs: MenuInputs,
    (paused, page): (Option<Res<crate::pause::Paused>>, Option<Res<crate::pause::Page>>),
) {
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.ui_files.is_none() {
        return;
    }
    // The test aid's pointer, moved on its clock.
    let mut aid_click = false;
    let mut aid_at = None;
    if let Some(mut aid) = aid {
        let now = time.elapsed_secs();
        let mut fired: Vec<(f32, AidPointer)> = aid.due.iter().copied().filter(|(t, _)| *t <= now).collect();
        fired.sort_by(|a, b| a.0.total_cmp(&b.0));
        aid.due.retain(|(t, _)| *t > now);
        for (_, item) in fired {
            match item {
                AidPointer::At { x, y, click } => {
                    info!("test aid: tile pointer at {x} {y}{}", if click { ", click" } else { "" });
                    aid.now = Some((x, y));
                    aid_click |= click;
                }
                AidPointer::Off => {
                    info!("test aid: tile pointer off");
                    aid.now = None;
                }
            }
        }
        aid_at = aid.now;
    }
    // The menus that take input, back to front (see the module doc): the HUD only under
    // the test aid.
    let mut taking = Vec::new();
    if aid_at.is_some() && menus.hud {
        taking.extend(rt.game.ui_tree.menu(super::draw::HUD_MENU));
    }
    taking.extend(inputs.taking(*menus, &rt.game));
    // T11: the pause menu's Mods page takes the pointer and keys for the start menu's
    // tiles (`super::mods`).
    if menus.start && paused.as_ref().zip(page.as_ref()).is_some_and(|(p, pg)| crate::pause::mods_open(p, pg)) {
        taking.extend(rt.game.ui_tree.menu(super::mods::START_MENU));
    }
    let wait_tiled = super::wait_menu::tiled(menus.sleep_wait, &rt.game);
    inputs.wait_check.wrote = None;
    if !wait_tiled {
        inputs.wait_check.drawn = None;
    }
    if taking.is_empty() {
        // A highlight left from the last menu that took input goes (mouseover back to 0),
        // and the scripts' clicks end and fire their events; otherwise nothing to do
        // [design: `fnv_game::ui_input::idle`].
        if fnv_game::ui_input::idle(&mut rt.game).is_some_and(|r| r.over.is_some()) {
            info!("tiles: pointer over nothing (no menu takes input)");
        }
        return;
    }
    // The aid's pointer, else the cursor in the window.
    let at = aid_at.or_else(|| devices.windows.single().ok().and_then(|w| Some(to_units(w.cursor_position()?, w.height()))));
    let mut input = Input {
        at,
        press: aid_click || devices.mouse.just_pressed(MouseButton::Left),
        shift: devices.keys.pressed(KeyCode::ShiftLeft) || devices.keys.pressed(KeyCode::ShiftRight),
        ..Input::default()
    };
    // The keys from the keyboard, and the controller buttons that stood in for a key this
    // frame (`crate::pad::menu_key`): what the hand-built handlers saw too.
    let mut physical: Vec<&'static str> = Vec::new();
    for k in devices.keys.get_just_pressed() {
        if let Some(n) = key_name(*k) {
            push(&mut input, n);
            physical.push(n.1);
        }
    }
    for code in std::mem::take(&mut rt.game.ui_taps) {
        if let Some(n) = u8::try_from(code).ok().and_then(crate::bindings::dik_key).and_then(key_name) {
            push(&mut input, n);
        }
    }
    for (button, name) in PAD {
        if devices.pads.iter().any(|g| g.just_pressed(button)) {
            push(&mut input, (false, name));
            if crate::pad::menu_key(button).is_some_and(|k| devices.keys.just_pressed(k)) {
                physical.push(name);
            }
        }
    }
    let report = fnv_game::ui_input::step(&mut rt.game, &taking, &input, &no_measure);
    let ui = &rt.game.ui_tree;
    let slider = inputs.follow(ui, &report, at, aid_click || devices.mouse.pressed(MouseButton::Left), &physical);
    if let Some((_, to)) = report.over {
        match to.filter(|&t| ui.is_live(t)) {
            Some(t) => {
                let id = ui.tile(t).get("id").map_or_else(|| "no id".to_string(), |v| format!("id {}", v.text()));
                let over = ui.tile(t).get(fnv_ui::input::MOUSEOVER).map_or(0.0, fnv_ui::Value::num);
                info!("tiles: pointer over {} ({id}), mouseover {over}", ui.path_of(t));
            }
            None => info!("tiles: pointer over nothing"),
        }
    }
    for c in &report.clicks {
        if ui.is_live(c.tile) {
            let clicked = ui.tile(c.tile).get(fnv_ui::input::CLICKED).map_or(0.0, fnv_ui::Value::num);
            info!("tiles: {}, clicked {clicked}", fnv_game::ui_input::click_line(ui, c));
        }
    }
    for line in &report.unfollowed {
        info!("tiles: key not followed: {line}");
    }
    // T15b: the slider's value is the model's again on every frame the menu is tiled, over
    // the file's own count of the clicks and their pulses' ends (`super::wait_menu`), after
    // this frame's step and before the tree is worked out after scripts: the hours a command
    // this frame leads to, else the model's.
    if wait_tiled {
        let model = inputs.wait.as_ref().and_then(|w| w.0.as_ref()).map(crate::wait::model::Model::marker);
        if let Some(h) = slider.or(model) {
            super::wait_menu::write_slider(&mut rt.game.ui_tree, h);
            inputs.wait_check.wrote = Some(h);
        }
    }
}

/// Read the test aid at start.
pub(crate) fn setup(app: &mut App) {
    app.init_resource::<MessageClick>().init_resource::<WaitSliderCheck>();
    let Ok(v) = std::env::var(AID) else { return };
    match crate::test_aid::timed(&v).and_then(|items| items.into_iter().map(|(s, r)| aid_pointer(r).map(|p| (s, p))).collect::<Result<Vec<_>, _>>()) {
        Ok(due) => {
            app.insert_resource(PointerAid { due, now: None });
        }
        Err(e) => warn!("{AID}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aid_items() {
        assert_eq!(aid_pointer("120:340"), Ok(AidPointer::At { x: 120.0, y: 340.0, click: false }));
        assert_eq!(aid_pointer(" 1.5 : 2 : click "), Ok(AidPointer::At { x: 1.5, y: 2.0, click: true }));
        assert_eq!(aid_pointer("OFF"), Ok(AidPointer::Off));
        assert!(aid_pointer("12").is_err());
        assert!(aid_pointer("1:2:press").is_err());
        let items = crate::test_aid::timed("3:10:20,4:10:20:click,9:off").unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(aid_pointer(items[1].1), Ok(AidPointer::At { x: 10.0, y: 20.0, click: true }));
    }

    #[test]
    fn keys_and_units() {
        assert_eq!(key_name(KeyCode::KeyE), Some((true, "E")));
        assert_eq!(key_name(KeyCode::ArrowDown), Some((false, "xdown")));
        assert_eq!(key_name(KeyCode::Digit1), None, "no digits in the unmodded game");
        // 960 units to the height: at 1280 x 720, a pixel is 4/3 of a unit.
        assert_eq!(to_units(Vec2::new(640.0, 360.0), 720.0), (853.3333333333334, 480.0));
        assert_eq!(to_units(Vec2::new(0.0, 960.0), 960.0), (0.0, 960.0));
    }
}
