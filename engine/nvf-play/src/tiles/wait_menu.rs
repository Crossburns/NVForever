//! T15b: the Wait / Sleep menu drawn from `menus\sleep_wait_menu.xml`, behind
//! `[UI] TileMenus=SleepWaitMenu` [design: ui-tile-tree.md §6.2 step 3; plan:
//! docs/handoff/ui-tile-tree-tasks.md, T15b].
//!
//! - **The switch** (`super::vanilla::TileMenus::sleep_wait`): on, with the game's menu
//!   files read and `SleepWaitMenu` built into the tree while the menu is open (menu 1012,
//!   T3b), the hand-built drawing (`crate::wait`) is not spawned and `super::draw` draws the
//!   menu's whole tree at the hand-built menu's stacking level. Off (the default), none of
//!   this runs.
//! - **What the model writes** ([`write`], only when what the menu shows changed): the
//!   three texts and the Wait button's word the file leaves "set in code", the slider's
//!   value, and the background's opacity. The menu's rules (the hours, the keys, the hours
//!   passing) stay the hand-built menu's (`crate::wait::model::Model`) [design].
//! - **The slider**: the file's scrollbar has 24 items, one visible (values 0 to 23)
//!   [documented: sleep_wait_menu.xml]; hour `h` is value `h - 1`. The model writes
//!   `_current_value` and pokes `_SetInCode`, as the prefab asks of code that sets the
//!   value [documented: scrollbar_horiz.xml's comment on `_SetInCode`]. The prefab's own
//!   `_current_value` sums the arrows' and page areas' `clicked` counts from 0 in this
//!   tree (an expression starts from 0, `fnv_ui`), so it cannot follow a value code set;
//!   the clicks on the slider's pieces are therefore the hand-built menu's commands
//!   ([`slider_command`]). A value written from outside lasts only until a source changes
//!   again (a click's one-frame pulse, its end), so the model's hours are written back on
//!   every frame the menu is tiled, after the input's step (`super::input::feed`) [design].
//! - None of `crate::wait`'s layout numbers is read here: the places come from the file.

use super::menus::Tiled;
use crate::wait::model::{Command, View, MAX_HOURS};
use fnv_ui::{TileId, Ui, Value};

/// The menu's name [documented: sleep_wait_menu.xml].
pub const SLEEP_WAIT_MENU: &str = "SleepWaitMenu";

/// The tiles the model writes [documented: sleep_wait_menu.xml].
const MAIN: &str = "SleepWaitMenu/NOGLOW_BRANCH/SWM_MainRect";
const BACKGROUND: &str = "SleepWaitMenu/NOGLOW_BRANCH/SWM_Background";
const TITLE: &str = "SleepWaitMenu/NOGLOW_BRANCH/SWM_MainRect/SWM_HowManyText";
const HOURS: &str = "SleepWaitMenu/NOGLOW_BRANCH/SWM_MainRect/SWM_HoursChosen";
const TIME: &str = "SleepWaitMenu/NOGLOW_BRANCH/SWM_MainRect/SWM_CurrentTime";
const WAIT_BUTTON: &str = "SleepWaitMenu/NOGLOW_BRANCH/SWM_MainRect/SWM_WaitButton";
const SLIDER: &str = "SleepWaitMenu/NOGLOW_BRANCH/SWM_MainRect/SWM_Scrollbar";

/// [design] Every fixed path the model writes or input reads is part of a successful build;
/// a file a mod restructured keeps the hand-built menu.
pub fn built(ui: &Ui) -> Tiled {
    for path in [SLEEP_WAIT_MENU, MAIN, BACKGROUND, TITLE, HOURS, TIME, WAIT_BUTTON, SLIDER] {
        if ui.find_tile(path).is_none() {
            return Tiled::No(format!("{path} missing"));
        }
    }
    Tiled::Yes
}

/// Whether the open Wait / Sleep menu is drawn from its tiles: the switch on, the menu
/// files read, the menu open (`Game::menu` 1012, `crate::script::MENU_SLEEP_WAIT`) and its
/// file built with every model path. Every drawing and input reader uses this decision
/// [design].
pub fn tiled(on: bool, game: &fnv_game::Game<'_>) -> bool {
    on && game.ui_files.is_some() && game.menu == Some(crate::script::MENU_SLEEP_WAIT) && built(&game.ui_tree).yes()
}

/// What the menu showed when last written: the menu's tile and the view.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub menu: TileId,
    pub view: View,
}

/// The slider's value for hour `hours` (1 to 24): 0 to 23.
pub fn slider_value(hours: u32) -> f64 {
    f64::from(hours.clamp(1, MAX_HOURS) - 1)
}

/// Write the slider's value for `hours` and poke `_SetInCode`, so the file places its
/// marker by the value and not by a drag [documented: scrollbar_horiz.xml].
pub fn write_slider(ui: &mut Ui, hours: u32) {
    let Some(bar) = ui.find_tile(SLIDER) else { return };
    ui.set_trait(bar, "_current_value", Value::Num(slider_value(hours)));
    ui.set_trait(bar, "_SetInCode", Value::Num(1.0));
}

/// Write what `SleepWaitMenu` shows of `view` [documented: the file's "set in code"
/// comments]: `SWM_HowManyText` the title ("How long would you like to wait?"),
/// `SWM_HoursChosen` the hours and the time they end at, `SWM_CurrentTime` the date and
/// time now, `SWM_WaitButton` the bare verb (the file shows the key hint beside it from
/// `_PCButtonText`), the slider ([`write_slider`]) and the background's `alpha`
/// (`super::menus::background_alpha`, as the other T15 menus). The Cancel button's word is
/// the file's own (`&-sCancel;`).
pub fn write(ui: &mut Ui, view: &View) {
    if !built(ui).yes() {
        return;
    }
    for (path, s) in [(TITLE, &view.title), (HOURS, &view.hours), (TIME, &view.date), (WAIT_BUTTON, &view.action)] {
        if let Some(t) = ui.find_tile(path) {
            ui.set_trait(t, "string", Value::Str(s.clone()));
        }
    }
    write_slider(ui, view.marker);
    if let Some(bg) = ui.find_tile(BACKGROUND) {
        ui.set_trait(bg, "alpha", Value::Num(super::menus::background_alpha()));
    }
}

/// The keys the hand-built menu acts on itself (`crate::wait`'s `keys`), by the tile
/// input's names: E (accept), X (cancel), A and D and the arrows (a step), and the
/// controller buttons standing in for Enter, Esc, Page Up and Page Down
/// (`crate::pad::menu_key`). The file binds W to its Wait button and E to its Cancel button
/// [documented: `_PCButton_W`, `_PCButton_E`]: W, which the hand-built menu has no use for,
/// acts through the file; E stays the hand-built accept [design: NVForever's keys, wait-menu.md §6].
pub const HAND_BUILT_KEYS: [&str; 10] = ["E", "X", "A", "D", "xleft", "xright", "xbuttona", "xbuttonb", "xbuttonlb", "xbuttonrb"];

/// What the menu's tiles stand for, by name [documented: sleep_wait_menu.xml,
/// scrollbar_horiz.xml].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitTile {
    Wait,
    Cancel,
    StepLeft,
    StepRight,
    PageLeft,
    PageRight,
    /// The marker, or the slider where no piece is.
    Slider,
}

/// The menu's tile `tile` stands for, by its name or a parent's.
pub fn wait_tile(ui: &Ui, tile: TileId) -> Option<WaitTile> {
    let mut at = Some(tile);
    while let Some(t) = at {
        let name = ui.tile(t).name.to_ascii_lowercase();
        match name.as_str() {
            "swm_waitbutton" => return Some(WaitTile::Wait),
            "swm_cancelbutton" => return Some(WaitTile::Cancel),
            "scrollbar_horiz_left" => return Some(WaitTile::StepLeft),
            "scrollbar_horiz_right" => return Some(WaitTile::StepRight),
            "scrollbar_horiz_page_left" => return Some(WaitTile::PageLeft),
            "scrollbar_horiz_page_right" => return Some(WaitTile::PageRight),
            "scrollbar_horiz_marker" | "swm_scrollbar" => return Some(WaitTile::Slider),
            _ => {}
        }
        at = ui.tile(t).parent;
    }
    None
}

/// The hand-built menu's command for a click on a slider piece or a button: a step of one
/// for the arrows (`_step_size` 1), six for the page areas (`_jump_size` 6) [documented:
/// sleep_wait_menu.xml], Accept and Cancel for the buttons; the marker and the slider take
/// the hours under the pointer ([`hours_under`]).
pub fn slider_command(what: WaitTile, under: Option<u32>) -> Option<Command> {
    use crate::wait::model::JUMP;
    Some(match what {
        WaitTile::Wait => Command::Accept,
        WaitTile::Cancel => Command::Cancel,
        WaitTile::StepLeft => Command::Step(-1),
        WaitTile::StepRight => Command::Step(1),
        WaitTile::PageLeft => Command::Step(-JUMP),
        WaitTile::PageRight => Command::Step(JUMP),
        WaitTile::Slider => Command::Hours(under?),
    })
}

/// The slider's left edge (interface units): the marker's place less its own `x` in the
/// slider. The layout lists the drawn tiles (the marker, not the slider's hotrect), so this
/// lays out the whole menu: worked out once a press, and kept while it drags [design].
pub fn slider_left(ui: &Ui) -> Option<f64> {
    let marker = marker(ui)?;
    let menu = ui.menu(SLEEP_WAIT_MENU)?;
    let placed = super::layout::layout(ui, menu, &|_, _, _| (0.0, 0.0), true).into_iter().find(|i| i.tile == marker)?.rect;
    Some(placed.left - ui.tile(marker).get("x").map_or(0.0, Value::num))
}

fn marker(ui: &Ui) -> Option<TileId> {
    let bar = ui.find_tile(SLIDER)?;
    ui.tile(bar).children.iter().copied().find(|&c| ui.is_live(c) && ui.tile(c).name.eq_ignore_ascii_case("scrollbar_horiz_marker"))
}

/// The hours under the pointer `x` (interface units) on the slider whose left edge is
/// `left` ([`slider_left`]): the value whose marker, `_step_width` apart and `width` wide,
/// is centred nearest the pointer [documented: the prefab places the marker at value ×
/// `_step_width`; design: the nearest centre].
pub fn hours_at(ui: &Ui, left: f64, x: f64) -> Option<u32> {
    let marker = marker(ui)?;
    let num = |n: &str| ui.tile(marker).get(n).map_or(0.0, Value::num);
    let (step, width) = (num("_step_width"), num("width"));
    if step <= 0.0 {
        return None;
    }
    let value = ((x - left - width / 2.0) / step).round().clamp(0.0, f64::from(MAX_HOURS - 1));
    Some(value as u32 + 1)
}

/// The hours under the pointer `x`: [`hours_at`] the slider's [`slider_left`].
pub fn hours_under(ui: &Ui, x: f64) -> Option<u32> {
    hours_at(ui, slider_left(ui)?, x)
}

/// The slider's value in the tree: after the tree is worked out, the value drawn.
pub fn slider_shown(ui: &Ui) -> Option<f64> {
    ui.get_float(&format!("{SLIDER}/_current_value"))
}

/// A line for the log when what the menu shows changes (smoke reads these).
pub fn describe(ui: &Ui) -> String {
    let s = |p: &str| ui.get_string(&format!("{p}/string")).unwrap_or_default();
    let value = slider_shown(ui).unwrap_or(-1.0);
    format!("tiles: {SLEEP_WAIT_MENU} drawn from tiles: \"{}\", \"{}\", \"{}\", button \"{}\", slider {}", s(TITLE), s(HOURS), s(TIME), s(WAIT_BUTTON), Value::Num(value).text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    /// [design] A menu in the vanilla file's shape (its names, the slider's pieces and the
    /// prefab's marker place and step width), cut down to what these tests read.
    fn menu() -> Ui {
        menu_changed(str::to_string)
    }

    fn menu_changed(change: impl FnOnce(&str) -> String) -> Ui {
        let src = MemSource::default().with(
            "swm.xml",
            &change(
                r#"<menu name="SleepWaitMenu"><rect name="NOGLOW_BRANCH">
  <image name="SWM_Background"><alpha>32</alpha></image>
  <hotrect name="SWM_MainRect"><x>50</x><y>100</y><width>860</width><height>400</height>
    <text name="SWM_HowManyText"><string></string></text>
    <text name="SWM_HoursChosen"><string></string></text>
    <text name="SWM_CurrentTime"><string></string></text>
    <hotrect name="SWM_Scrollbar"><x>112.5</x><y>145</y><width>640</width><height>64</height>
      <_number_of_items>24</_number_of_items><_number_of_visible_items>1</_number_of_visible_items>
      <_current_value><copy>0</copy><add src="child(scrollbar_horiz_right)" trait="clicked"/></_current_value>
      <image name="scrollbar_horiz_page_left"/><image name="scrollbar_horiz_page_right"/>
      <image name="scrollbar_horiz_marker"><width>48</width>
        <x><copy src="parent()" trait="_current_value"/><mul src="me()" trait="_step_width"/></x>
        <_step_width><copy src="parent()" trait="width"/><sub src="me()" trait="width"/><div>23</div></_step_width></image>
      <image name="scrollbar_horiz_left"/><image name="scrollbar_horiz_right"/>
    </hotrect>
    <hotrect name="SWM_WaitButton"><string> </string><text name="button_text"/></hotrect>
    <hotrect name="SWM_CancelButton"><string>Cancel</string><text name="button_text"/></hotrect>
  </hotrect></rect></menu>"#,
            ),
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "swm.xml").unwrap();
        ui.update();
        ui
    }

    fn view(hours: u32) -> View {
        let words = crate::wait::model::Words::default();
        let cal = fnv_sim::clock::Calendar { hour: 10.5, day: 19.0, month: 9.0, year: 2281.0, days_passed: 0.0 };
        crate::wait::model::Model { hours, ..crate::wait::model::Model::open(false) }.view(&words, &cal)
    }

    #[test]
    fn the_menu_writes_its_texts_the_button_and_the_slider() {
        let mut ui = menu();
        write(&mut ui, &view(5));
        ui.update();
        let s = |p: &str| ui.get_string(&format!("{MAIN}/{p}/string")).unwrap();
        assert_eq!(s("SWM_HowManyText"), "How long would you like to wait?");
        assert_eq!(s("SWM_HoursChosen"), "5 hours (15:30)");
        assert!(s("SWM_CurrentTime").ends_with("10:30"), "{}", s("SWM_CurrentTime"));
        // The bare verb: the file shows the key hint itself.
        assert_eq!(s("SWM_WaitButton"), "Wait");
        assert_eq!(s("SWM_CancelButton"), "Cancel", "the file's own word");
        // Hour 5 is value 4; the marker stands 4 steps of (640 - 48) / 23 in.
        assert_eq!(ui.get_float(&format!("{SLIDER}/_current_value")), Some(4.0));
        assert_eq!(ui.get_float(&format!("{SLIDER}/_SetInCode")), Some(1.0));
        let x = ui.get_float(&format!("{SLIDER}/scrollbar_horiz_marker/x")).unwrap();
        assert!((x - 4.0 * 592.0 / 23.0).abs() < 1e-9);
        assert_eq!(ui.get_float(&format!("{BACKGROUND}/alpha")), Some(super::super::menus::background_alpha()));
        assert!(describe(&ui).contains("slider 4"));
    }

    #[test]
    fn the_slider_takes_the_hours_under_the_pointer_and_its_pieces_step() {
        let mut ui = menu();
        write(&mut ui, &view(1));
        ui.update();
        // The slider's left edge: the main rect's 50 and the bar's 112.5; the marker 48 wide,
        // centred on value v at 162.5 + 24 + v × 592/23.
        let at = |v: f64| 162.5 + 24.0 + v * 592.0 / 23.0;
        assert_eq!(hours_under(&ui, at(0.0)), Some(1));
        assert_eq!(hours_under(&ui, at(11.0) + 3.0), Some(12));
        assert_eq!(hours_under(&ui, at(23.0)), Some(24));
        assert_eq!(hours_under(&ui, 0.0), Some(1), "left of the slider: its first hour");
        assert_eq!(hours_under(&ui, 5000.0), Some(24));
        // The pieces by name.
        let tile = |p: &str| ui.find_tile(&format!("{SLIDER}/{p}")).unwrap();
        assert_eq!(wait_tile(&ui, tile("scrollbar_horiz_page_right")), Some(WaitTile::PageRight));
        assert_eq!(wait_tile(&ui, tile("scrollbar_horiz_marker")), Some(WaitTile::Slider));
        assert_eq!(wait_tile(&ui, ui.find_tile(&format!("{MAIN}/SWM_CancelButton/button_text")).unwrap()), Some(WaitTile::Cancel));
        assert_eq!(wait_tile(&ui, ui.find_tile(TITLE).unwrap()), None);
        assert_eq!(slider_command(WaitTile::PageLeft, None), Some(Command::Step(-6)));
        assert_eq!(slider_command(WaitTile::StepRight, None), Some(Command::Step(1)));
        assert_eq!(slider_command(WaitTile::Wait, None), Some(Command::Accept));
        assert_eq!(slider_command(WaitTile::Slider, Some(7)), Some(Command::Hours(7)));
        assert_eq!(slider_command(WaitTile::Slider, None), None);
    }

    /// A frame of the menu with a click on the right arrow (a one-frame pulse, `clicked` 1)
    /// or none (`clicked` 0): the file's count is worked out, then the model's `hours`
    /// written back as `super::input::feed` does on every tiled frame, then the tree worked
    /// out again (after scripts). The slider's value then.
    fn frame(ui: &mut Ui, click: bool, hours: u32) -> f64 {
        let right = ui.find_tile(&format!("{SLIDER}/scrollbar_horiz_right")).unwrap();
        ui.set_trait(right, "clicked", Value::Num(f64::from(u8::from(click))));
        write_slider(ui, hours);
        ui.update();
        ui.get_float(&format!("{SLIDER}/_current_value")).unwrap()
    }

    #[test]
    fn a_click_counted_by_the_file_is_put_back_to_the_models_value() {
        // [design] The prefab's value sums clicks from 0 here; the value written after the
        // command is the model's.
        let mut ui = menu();
        write(&mut ui, &view(10));
        ui.update();
        let right = ui.find_tile(&format!("{SLIDER}/scrollbar_horiz_right")).unwrap();
        ui.set_trait(right, "clicked", Value::Num(1.0));
        ui.update();
        assert_eq!(ui.get_float(&format!("{SLIDER}/_current_value")), Some(1.0), "the file's own count");
        write_slider(&mut ui, 11);
        ui.update();
        assert_eq!(ui.get_float(&format!("{SLIDER}/_current_value")), Some(10.0));
        // The pulse ends next frame: the file works its count out again (0) unless the
        // model's value is written back on that frame too (#1307 review).
        ui.set_trait(right, "clicked", Value::Num(0.0));
        ui.update();
        assert_eq!(ui.get_float(&format!("{SLIDER}/_current_value")), Some(0.0), "the file's count once the pulse ends");
        assert_eq!(frame(&mut ui, false, 11), 10.0, "written back on the pulse's end");
        assert_eq!(frame(&mut ui, false, 11), 10.0, "and on the frames after");
    }

    #[test]
    fn a_click_at_24_hours_keeps_the_marker_at_24() {
        // At 24 the model clamps a step right: its hours do not change, and the slider
        // stays at value 23 on the click's frame, the pulse's end and after (#1307 review).
        let mut ui = menu();
        write(&mut ui, &view(24));
        ui.update();
        let mut model = crate::wait::model::Model { hours: 24, ..crate::wait::model::Model::open(false) };
        model.apply(slider_command(WaitTile::StepRight, None).unwrap());
        assert_eq!(model.marker(), 24, "clamped");
        assert_eq!(frame(&mut ui, true, model.marker()), 23.0, "the click's frame");
        assert_eq!(frame(&mut ui, false, model.marker()), 23.0, "the pulse's end");
        assert_eq!(frame(&mut ui, false, model.marker()), 23.0, "a frame later");
        assert!(describe(&ui).contains("slider 23"));
    }

    #[test]
    fn cut_down_files_keep_the_hand_built_menu() {
        for (name, missing) in [("SWM_MainRect", MAIN), ("SWM_HoursChosen", HOURS), ("SWM_Scrollbar", SLIDER), ("SWM_Background", BACKGROUND), ("SWM_WaitButton", WAIT_BUTTON)] {
            let mut ui = menu_changed(|xml| xml.replace(&format!("name=\"{name}\""), "name=\"Modded\""));
            assert_eq!(built(&ui), Tiled::No(format!("{missing} missing")));
            write(&mut ui, &view(3));
            // `_SetInCode` is only ever the code's (the file's slider has its own value).
            assert_eq!(ui.get_float(&format!("{SLIDER}/_SetInCode")), None, "nothing written");
        }
        assert_eq!(built(&menu()), Tiled::Yes);
    }

    #[test]
    fn values_clamp_to_the_sliders_items() {
        assert_eq!((slider_value(1), slider_value(24), slider_value(0), slider_value(99)), (0.0, 23.0, 0.0, 23.0));
    }
}
