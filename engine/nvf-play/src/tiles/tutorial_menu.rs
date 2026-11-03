//! T15b part 2: the tutorial menu drawn from `menus\tutorial_menu.xml`, behind
//! `[UI] TileMenus=TutorialMenu` [design: ui-tile-tree.md §6.2 step 3; plan:
//! docs/handoff/ui-tile-tree-tasks.md, T15b; notes: tutorial-menu.md §5].
//!
//! - **The switch** (`super::vanilla::TileMenus::tutorial`): on, with the game's menu files
//!   read and `TutorialMenu` built into the tree while the menu is open (menu 1059, T3b),
//!   the hand-built drawing (`crate::tutorial_menu`) gives way to a sheet taking the pointer
//!   and `super::draw` draws the menu's whole tree at the same stacking level. Off (the
//!   default), none of this runs and `clipwindow` stays unread.
//! - **What the model writes** ([`write`], when the page shown changes): the strings the
//!   file leaves "replaced in code" (`TM_title`, `TM_page`), the body (`TM_text`), the page
//!   count's and the buttons' state, the background's opacity, and the two marks that make
//!   this file come out as captured: `clipwindow` turned on
//!   (`fnv_ui::input::mark_clip_windows`) and the background's alpha kept from its children
//!   ([`super::layout::OWN_ALPHA`]). The file has no `io()` traits [documented]; the menu's
//!   code addresses its tiles by `<id>` and by these traits [hypothesis].
//! - **What the model does not work out**: the sizes. `TM_text`'s measured height goes to
//!   `_text_height`, that plus a step to the scrollbar's `_number_of_items`, the column's
//!   height to `_number_of_visible_items`, and the scroll range is what those leave
//!   ([`scroll_bound`]) [documented: the file; observed: 625 + 30 items at 440 visible].
//! - **The scroll**: `crate::tutorial_menu::TutorialView` owns it. [`write_scroll`] writes
//!   `_current_value` and pokes `_SetInCode` on every frame the menu is tiled, as the wait
//!   menu's slider: the prefab's own value counts clicks from 0 in this tree. The file moves
//!   the text by that value (`_text_y`); the layout moves nothing (menu-xml.md §3.1.1).
//! - **The order in a frame** (`super::write_tutorial`, after `crate::tutorial_menu::keys`,
//!   before the scripts): the frame's commands are applied first, then the page they lead
//!   to is written, measured and its scroll written, so no frame draws new text with an old
//!   scroll or the other way round.

use super::layout::Rect;
use super::menus::Tiled;
use crate::tutorial_menu::{Command, Page};
use fnv_ui::{TileId, Ui, Value};

/// The menu's name [documented: tutorial_menu.xml].
pub const TUTORIAL_MENU: &str = "TutorialMenu";

/// The tiles the model writes or the input reads [documented: tutorial_menu.xml,
/// scrollbar_vert.xml].
const BACKGROUND: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background";
const DEPTH: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect";
const TITLE: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_title";
const COUNT: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_page";
const COLUMN: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_text_container";
const TEXT: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_text_container/TM_text";
const BAR: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_scrollbar";
const MARKER: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_scrollbar/scrollbar_vert_marker";
const NEXT: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_next_button";
const PREVIOUS: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_prev_button";
const CLOSE: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_close_button";
const SYMBOL: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_VDSG_symbol";

/// [design] Every fixed path the model writes or input reads is part of a successful build;
/// a file a mod restructured keeps the hand-built menu.
pub fn built(ui: &Ui) -> Tiled {
    for path in [TUTORIAL_MENU, BACKGROUND, DEPTH, TITLE, COUNT, COLUMN, TEXT, BAR, MARKER, NEXT, PREVIOUS, CLOSE, SYMBOL] {
        if ui.find_tile(path).is_none() {
            return Tiled::No(format!("{path} missing"));
        }
    }
    Tiled::Yes
}

/// Whether the open tutorial menu is drawn from its tiles: the switch on, the menu files
/// read, the menu open and its file built with every model path. Every drawing and input
/// reader uses this decision [design].
pub fn tiled(on: bool, game: &fnv_game::Game<'_>) -> bool {
    on && game.ui_files.is_some() && game.menu == Some(fnv_game::game::menus::TUTORIAL) && built(&game.ui_tree).yes()
}

/// What the menu showed when last written: the menu's tile (a new one when it is built
/// again), the page and where it is in the pages.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub menu: TileId,
    pub page: Page,
    pub at: usize,
    pub of: usize,
}

fn set(ui: &mut Ui, path: &str, name: &str, v: Value) {
    if let Some(t) = ui.find_tile(path) {
        ui.set_trait(t, name, v);
    }
}

/// Write what `TutorialMenu` shows of `shown`, with the background at `alpha` (0 to 255):
/// see the module notes.
///
/// - The title and the body: the message's name and text. A message with neither shows
///   empty strings (`crate::tutorial_menu::pages`).
/// - The count `n/N` only with more than one page, `TM_fill_line` taking its place by the
///   file's own rule [documented: its `visible`; observed: a one-page help leaves `TM_page`
///   not shown; design: `n/N`, as the hand-built menu, multi-page is not captured].
/// - Next and Previous hidden with one page [observed: the one-page help's shot, 1280 x 720,
///   shows neither], and with more a button with nowhere to go not a target, which the
///   file's `_line_alpha` dims and takes the highlight from [documented: text_box.xml;
///   design: the hand-built menu's rule].
pub fn write(ui: &mut Ui, shown: &Shown, alpha: f64) {
    if !built(ui).yes() {
        return;
    }
    let Shown { menu, page, at, of } = shown;
    set(ui, TITLE, "string", Value::Str(page.title.clone()));
    set(ui, TEXT, "string", Value::Str(page.text.clone()));
    let many = *of > 1;
    set(ui, COUNT, "string", Value::Str(format!("{}/{of}", at + 1)));
    let on = |b: bool| Value::Num(f64::from(u8::from(b)));
    set(ui, COUNT, "visible", on(many));
    set(ui, NEXT, "visible", on(many));
    set(ui, PREVIOUS, "visible", on(many));
    set(ui, NEXT, "target", on(at + 1 < *of));
    set(ui, PREVIOUS, "target", on(*at > 0));
    set(ui, BACKGROUND, "alpha", Value::Num(alpha));
    set(ui, DEPTH, super::layout::OWN_ALPHA, Value::Num(1.0));
    fnv_ui::input::mark_clip_windows(ui, *menu);
}

/// The VDSG symbol's picture's width, which its `width` and the words after it are worked
/// out from: `filewidth` less the file's `_dead_space` [documented: tutorial_menu.xml;
/// documented: its header, 128 x 64, Fallout - Textures2.bsa]. `texels` is what the file
/// holds; without it, the hand-built menu's stand-in (`crate::tutorial_menu::SYMBOL_W` and
/// the dead space) [design].
pub fn write_symbol(ui: &mut Ui, texels: Option<f64>) {
    use crate::tutorial_menu::{SYMBOL_DEAD, SYMBOL_W};
    set(ui, SYMBOL, "filewidth", Value::Num(texels.unwrap_or(f64::from(SYMBOL_W + SYMBOL_DEAD))));
}

/// The name of the picture [`write_symbol`] measures, as the file gives it.
pub fn symbol_file(ui: &Ui) -> Option<String> {
    ui.get_string(&format!("{SYMBOL}/filename"))
}

/// How far the text scrolls: the file's upper bound of `_current_value`, the scrollbar's
/// items less its visible ones, at least 0 [documented: scrollbar_vert.xml]. Not the
/// hand-built menu's `max_scroll`, which counts the text's 21 down and rounds to steps.
pub fn scroll_bound(ui: &Ui) -> f64 {
    let n = |name: &str| ui.get_float(&format!("{BAR}/{name}")).unwrap_or(0.0);
    (n("_number_of_items") - n("_number_of_visible_items")).max(0.0)
}

/// Write the scroll's value and poke `_SetInCode`, so the file places its marker by the
/// value and not by a drag [documented: scrollbar_vert.xml's comment on `_SetInCode`].
pub fn write_scroll(ui: &mut Ui, value: f64) {
    set(ui, BAR, "_current_value", Value::Num(value));
    set(ui, BAR, "_SetInCode", Value::Num(1.0));
}

/// The scroll in the tree: after the tree is worked out, the value drawn.
pub fn scroll_shown(ui: &Ui) -> Option<f64> {
    ui.get_float(&format!("{BAR}/_current_value"))
}

/// The body's `y` in its column, as worked out from the scroll by the file (`_text_y`).
pub fn text_y(ui: &Ui) -> Option<f64> {
    ui.get_float(&format!("{TEXT}/y"))
}

/// What the menu's tiles stand for, by name [documented: tutorial_menu.xml,
/// scrollbar_vert.xml].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TutorialTile {
    Next,
    Previous,
    Close,
    StepUp,
    StepDown,
    PageUp,
    PageDown,
    Marker,
}

/// The menu's tile `tile` stands for, by its name or a parent's. A button's text and key
/// hint are its button.
pub fn tutorial_tile(ui: &Ui, tile: TileId) -> Option<TutorialTile> {
    let mut at = Some(tile);
    while let Some(t) = at {
        let name = ui.tile(t).name.to_ascii_lowercase();
        match name.as_str() {
            "tm_next_button" => return Some(TutorialTile::Next),
            "tm_prev_button" => return Some(TutorialTile::Previous),
            "tm_close_button" => return Some(TutorialTile::Close),
            "scrollbar_vert_up" => return Some(TutorialTile::StepUp),
            "scrollbar_vert_down" => return Some(TutorialTile::StepDown),
            "scrollbar_vert_page_up" => return Some(TutorialTile::PageUp),
            "scrollbar_vert_page_down" => return Some(TutorialTile::PageDown),
            "scrollbar_vert_marker" => return Some(TutorialTile::Marker),
            _ => {}
        }
        at = ui.tile(t).parent;
    }
    None
}

/// The hand-built menu's command for a click on a tile: the file's `_step_size` for the
/// arrows, its `_jump_size` for the page areas [documented: tutorial_menu.xml, 30 and the
/// column's height less 30]. The marker drags instead ([`marker_grab`]).
pub fn tile_command(ui: &Ui, what: TutorialTile) -> Option<Command> {
    let n = |name: &str| ui.get_float(&format!("{BAR}/{name}")).unwrap_or(0.0) as f32;
    Some(match what {
        TutorialTile::Next => Command::Next,
        TutorialTile::Previous => Command::Previous,
        TutorialTile::Close => Command::Close,
        TutorialTile::StepUp => Command::Scroll(-n("_step_size")),
        TutorialTile::StepDown => Command::Scroll(n("_step_size")),
        TutorialTile::PageUp => Command::Scroll(-n("_jump_size")),
        TutorialTile::PageDown => Command::Scroll(n("_jump_size")),
        TutorialTile::Marker => return None,
    })
}

/// What a press on the marker holds while it drags: the scrollbar's top and how far under
/// the marker's top the pointer took it (interface units). The layout lists the drawn
/// tiles (the marker, not the scrollbar's hotrect), so this lays out the whole menu: worked
/// out once a press, as the wait menu's slider [design].
pub fn marker_grab(ui: &Ui, pointer_y: f64) -> Option<(f64, f64)> {
    let marker = ui.find_tile(MARKER)?;
    let menu = ui.menu(TUTORIAL_MENU)?;
    let placed: Rect = super::layout::layout(ui, menu, &|_, _, _| (0.0, 0.0), true).into_iter().find(|i| i.tile == marker)?.rect;
    Some((placed.top - ui.tile(marker).get("y").map_or(0.0, Value::num), pointer_y - placed.top))
}

/// The scroll for the pointer at `pointer_y` with a marker held at `grab` ([`marker_grab`]):
/// the marker's place over the file's `_step_height`, whole values [documented:
/// scrollbar_vert.xml's `_drag_value`; design: the marker's own drag traits are not built,
/// as the wait menu's slider].
pub fn scroll_under(ui: &Ui, (top, grab): (f64, f64), pointer_y: f64) -> Option<f64> {
    let step = ui.get_float(&format!("{MARKER}/_step_height")).filter(|s| *s > 0.0)?;
    // A hair over, so a marker held where it stands gives its own value back.
    Some(((pointer_y - grab - top) / step + 1e-9).floor().clamp(0.0, scroll_bound(ui)))
}

/// A line for the log when what the menu shows changes (smoke reads these).
pub fn describe(ui: &Ui, shown: &Shown) -> String {
    let n = |p: &str| ui.get_float(p).unwrap_or(-1.0);
    let s = |p: &str| ui.get_string(&format!("{p}/string")).unwrap_or_default();
    let shows = |p: &str| ui.find_tile(p).is_some_and(|t| ui.tile(t).get("visible").is_none_or(Value::truthy));
    format!(
        "tiles: {TUTORIAL_MENU} drawn from tiles: page {}/{} \"{}\", count {}, text height {}, column {} high, scroll to {}",
        shown.at + 1,
        shown.of,
        s(TITLE),
        if shows(COUNT) { s(COUNT) } else { "hidden".into() },
        Value::Num(n(&format!("{COLUMN}/_text_height"))).text(),
        Value::Num(n(&format!("{COLUMN}/height"))).text(),
        Value::Num(scroll_bound(ui)).text(),
    )
}

/// The keys the hand-built menu acts on itself (`crate::tutorial_menu::keys`), by the tile
/// input's names: A and S and E, the four arrows, and the controller buttons standing in
/// for them there: RT and LT for A and S (`crate::pad::menu_buttons`), A and B for Enter and
/// Esc. The file binds RB and LB to its page areas [documented: `xbuttonrb`, `xbuttonlb`],
/// which the hand-built menu has no use for, so they act through the file [design].
pub const HAND_BUILT_KEYS: [&str; 11] = ["A", "S", "E", "xleft", "xright", "xup", "xdown", "xbuttonrt", "xbuttonlt", "xbuttona", "xbuttonb"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tutorial_menu::{TutorialView, SYMBOL_DEAD, SYMBOL_W};
    use fnv_sim::FormId;
    use fnv_ui::MemSource;

    /// [design] A menu in the vanilla file's shape: its names and the expressions the model
    /// relies on (the sizes, the text's place, the scrollbar's range and marker), cut down
    /// to what these tests read, at the observed screen 1706.67 x 960.
    fn menu() -> Ui {
        menu_changed(str::to_string)
    }

    fn menu_changed(change: impl FnOnce(&str) -> String) -> Ui {
        let src = MemSource::default().with(
            "tm.xml",
            &change(
                r#"<menu name="TutorialMenu"><rect name="NOGLOW_BRANCH">
  <image name="TM_Background"><x>170</x><y>96</y><width>1365</width><height>768</height><alpha>32</alpha>
   <rect name="TM_depth_rect">
    <text name="TM_title"><string>Title Text</string></text>
    <text name="TM_page"><string>??/??</string></text>
    <image name="TM_fill_line"><visible><not src="sibling(TM_page)" trait="visible"/></visible></image>
    <hotrect name="TM_scrollbar"><y>172</y><width>32</width><height>424</height><alpha>0</alpha><target>&true;</target>
      <_step_size>30</_step_size>
      <_number_of_items><copy src="sibling(TM_text_container)" trait="_text_height"/><add src="me()" trait="_step_size"/></_number_of_items>
      <_number_of_visible_items><copy src="sibling(TM_text_container)" trait="height"/></_number_of_visible_items>
      <_jump_size><copy src="sibling(TM_text_container)" trait="height"/><sub src="me()" trait="_step_size"/></_jump_size>
      <_current_value>
        <add><copy src="child(scrollbar_vert_down)" trait="clicked"/><sub src="child(scrollbar_vert_up)" trait="clicked"/><mul src="me()" trait="_step_size"/></add>
        <min><copy src="me()" trait="_number_of_items"/><sub src="me()" trait="_number_of_visible_items"/></min><max>0</max>
      </_current_value>
      <hotrect name="scrollbar_vert_page_up"/><hotrect name="scrollbar_vert_page_down"/>
      <image name="scrollbar_vert_marker"><height>32</height>
        <y><copy src="parent()" trait="_current_value"/><mul src="me()" trait="_step_height"/></y>
        <_step_height><copy src="parent()" trait="height"/><sub src="me()" trait="height"/>
          <div><copy src="parent()" trait="_number_of_items"/><sub src="parent()" trait="_number_of_visible_items"/></div></_step_height>
      </image>
      <image name="scrollbar_vert_up"/><image name="scrollbar_vert_down"/>
    </hotrect>
    <hotrect name="TM_text_container"><alpha>0</alpha><clipwindow>&true;</clipwindow><x>108</x><y>172</y><height>440</height>
      <width><copy src="sibling(TM_prev_button)" trait="x"/><sub src="me()" trait="x"/><sub>64</sub></width>
      <_text_height><copy src="child(TM_text)" trait="height"/></_text_height>
      <_text_y><copy>0</copy><sub src="sibling(TM_scrollbar)" trait="_current_value"/><add>21</add></_text_y>
      <text name="TM_text"><wrapwidth><copy src="parent()" trait="width"/><sub>5</sub></wrapwidth><x>5</x>
        <y><copy src="parent()" trait="_text_y"/></y></text>
    </hotrect>
    <hotrect name="TM_next_button"><target>&true;</target><text name="button_text"/></hotrect>
    <hotrect name="TM_prev_button"><x>1157</x><target>&true;</target><text name="button_text"/></hotrect>
    <hotrect name="TM_close_button"><target>&true;</target><text name="button_text"/></hotrect>
    <image name="TM_VDSG_symbol"><filename>loading\symbol.dds</filename>
      <width><copy src="me()" trait="filewidth"/><sub src="me()" trait="_dead_space"/></width><_dead_space>36</_dead_space></image>
   </rect></image></rect></menu>"#,
            ),
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "tm.xml").unwrap();
        ui.update();
        ui
    }

    fn shown(ui: &Ui, title: &str, text: &str, at: usize, of: usize) -> Shown {
        Shown { menu: ui.menu(TUTORIAL_MENU).unwrap(), page: Page { message: FormId(0x72358), title: title.into(), text: text.into() }, at, of }
    }

    /// Font 1's height of `lines` lines [observed: 36 + (n - 1) x 31, menus-findings F].
    fn lines(n: usize) -> f64 {
        36.0 + (n - 1) as f64 * 31.0
    }

    /// The text's height by its string: as many lines as the string has words, so a test
    /// chooses its length; 500 wide unless it wraps.
    fn size(_: f64, text: &str, wrap: Option<f64>) -> (f64, f64) {
        (wrap.unwrap_or(500.0).min(500.0), lines(text.split_whitespace().count().max(1)))
    }

    /// `write`, the sizes and the scroll as `super::super::write_tutorial` does for a change.
    fn frame(ui: &mut Ui, s: &Shown, view: &mut TutorialView) {
        write(ui, s, 191.25);
        write_symbol(ui, Some(128.0));
        ui.update();
        settle(ui, view);
        ui.update();
    }

    fn settle(ui: &mut Ui, view: &mut TutorialView) {
        let menu = ui.menu(TUTORIAL_MENU).unwrap();
        let texts = super::super::menus::texts(ui, menu);
        let mut last = super::super::menus::SizesWritten::default();
        for _ in 0..3 {
            if super::super::menus::write_text_sizes_of(ui, &texts, &mut last, &size) == 0 {
                break;
            }
            ui.update();
        }
        view.set_scroll_bound(scroll_bound(ui) as f32);
        write_scroll(ui, f64::from(view.scroll));
    }

    fn words(n: usize) -> String {
        vec!["w"; n].join(" ")
    }

    #[test]
    fn writes_page_and_navigation_state() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "First", &words(3), 0, 2);
        frame(&mut ui, &s, &mut view);
        let string = |p: &str| ui.get_string(&format!("{p}/string")).unwrap();
        assert_eq!(string(TITLE), "First");
        assert_eq!(string(TEXT), "w w w");
        assert_eq!(string(COUNT), "1/2");
        assert_eq!(ui.get_float(&format!("{COUNT}/visible")), Some(1.0));
        // The file's line in the count's place gives way to it, read through a trait the
        // file does not have until the model writes it.
        let fill = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_fill_line/visible";
        assert_eq!(ui.get_float(fill), Some(0.0));
        // The first page of two: Next is a target, Previous not (the file dims it).
        assert_eq!(ui.get_float(&format!("{NEXT}/target")), Some(1.0));
        assert_eq!(ui.get_float(&format!("{PREVIOUS}/target")), Some(0.0));
        assert_eq!(ui.get_float(&format!("{BACKGROUND}/alpha")), Some(191.25));
        let s = shown(&ui, "Second", &words(3), 1, 2);
        frame(&mut ui, &s, &mut view);
        assert_eq!(ui.get_string(&format!("{COUNT}/string")).unwrap(), "2/2");
        assert_eq!(ui.get_float(&format!("{NEXT}/target")), Some(0.0));
        assert_eq!(ui.get_float(&format!("{PREVIOUS}/target")), Some(1.0));
        assert!(describe(&ui, &s).contains("page 2/2 \"Second\", count 2/2"), "{}", describe(&ui, &s));
    }

    #[test]
    fn one_page_hides_count() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Only", &words(3), 0, 1);
        frame(&mut ui, &s, &mut view);
        assert_eq!(ui.get_float(&format!("{COUNT}/visible")), Some(0.0));
        // The file's own line takes the count's place.
        let fill = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_fill_line/visible";
        assert_eq!(ui.get_float(fill), Some(1.0));
        // Next and Previous are not shown: the original's one-page help has neither.
        assert_eq!(ui.get_float(&format!("{NEXT}/visible")), Some(0.0));
        assert_eq!(ui.get_float(&format!("{PREVIOUS}/visible")), Some(0.0));
        assert!(describe(&ui, &s).contains("count hidden"), "{}", describe(&ui, &s));
    }

    #[test]
    fn measured_text_sets_scroll_extent() {
        // [observed] The capture's numbers: a text 625 high (20 lines of font 1) in a column
        // 440 high: 655 items, 215 to scroll. The column's width follows Previous's place.
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &words(20), 0, 1);
        frame(&mut ui, &s, &mut view);
        assert_eq!(ui.get_float(&format!("{TEXT}/height")), Some(625.0));
        assert_eq!(ui.get_float(&format!("{COLUMN}/_text_height")), Some(625.0));
        assert_eq!(ui.get_float(&format!("{BAR}/_number_of_items")), Some(655.0));
        assert_eq!(scroll_bound(&ui), 215.0);
        assert_eq!(ui.get_float(&format!("{TEXT}/wrapwidth")), Some(1157.0 - 108.0 - 64.0 - 5.0));
        // A short text does not scroll; the file's scrollbar has nothing to do.
        let s = shown(&ui, "Short", &words(3), 0, 1);
        frame(&mut ui, &s, &mut view);
        assert_eq!(scroll_bound(&ui), 0.0);
    }

    /// A frame of the menu with a click on the down arrow (a one-frame pulse) or none: the
    /// model's scroll written back as `write_tutorial` does on every tiled frame, then the
    /// tree worked out (after scripts). The value then.
    fn pulse(ui: &mut Ui, click: bool, scroll: f64) -> f64 {
        let down = ui.find_tile(&format!("{BAR}/scrollbar_vert_down")).unwrap();
        ui.set_trait(down, "clicked", Value::Num(f64::from(u8::from(click))));
        write_scroll(ui, scroll);
        ui.update();
        scroll_shown(ui).unwrap()
    }

    #[test]
    fn scroll_survives_click_pulse_end() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &words(20), 0, 1);
        frame(&mut ui, &s, &mut view);
        assert_eq!(pulse(&mut ui, false, 120.0), 120.0);
        assert_eq!(text_y(&ui), Some(21.0 - 120.0), "the file moves the text by the value");
        assert_eq!(pulse(&mut ui, true, 150.0), 150.0, "the click's frame");
        assert_eq!(pulse(&mut ui, false, 150.0), 150.0, "the pulse's end");
        assert_eq!(pulse(&mut ui, false, 150.0), 150.0, "a frame later");
        assert_eq!(text_y(&ui), Some(21.0 - 150.0));
        // The file's own count, left alone, would be nothing like it.
        let down = ui.find_tile(&format!("{BAR}/scrollbar_vert_down")).unwrap();
        ui.set_trait(down, "clicked", Value::Num(1.0));
        ui.update();
        assert_eq!(scroll_shown(&ui), Some(30.0), "the file's count from 0");
    }

    #[test]
    fn page_change_and_resize_clamp_scroll() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        // At the bottom of a long page.
        let long = shown(&ui, "Long", &words(20), 0, 2);
        frame(&mut ui, &long, &mut view);
        assert!(!view.apply(2, Command::Scroll(500.0)));
        assert_eq!(view.scroll, 215.0, "the file's bound, not a whole number of steps");
        frame(&mut ui, &long, &mut view);
        assert_eq!(text_y(&ui), Some(21.0 - 215.0));
        // A turn starts the next page at its top, whatever its length.
        assert!(!view.apply(2, Command::Next));
        assert_eq!((view.page, view.scroll), (1, 0.0));
        let short = shown(&ui, "Short", &words(3), 1, 2);
        frame(&mut ui, &short, &mut view);
        assert_eq!(text_y(&ui), Some(21.0));
        // Back on the long page, at its bottom: when its text comes out shorter (a wider
        // window wraps it less), a scroll past the new range is held in it.
        assert!(!view.apply(2, Command::Previous));
        frame(&mut ui, &long, &mut view);
        assert!(!view.apply(2, Command::Scroll(500.0)));
        let shorter = shown(&ui, "Long", &words(16), 0, 2);
        frame(&mut ui, &shorter, &mut view);
        assert_eq!(scroll_bound(&ui), lines(16) + 30.0 - 440.0);
        assert_eq!(f64::from(view.scroll), scroll_bound(&ui));
        assert_eq!(text_y(&ui), Some(21.0 - scroll_bound(&ui)));
    }

    #[test]
    fn the_symbols_width_comes_from_its_picture() {
        let mut ui = menu();
        let width = format!("{SYMBOL}/width");
        assert_eq!(ui.get_float(&width), Some(-36.0), "the file alone, before the code writes");
        assert_eq!(symbol_file(&ui).as_deref(), Some("loading\\symbol.dds"));
        write_symbol(&mut ui, Some(128.0));
        ui.update();
        assert_eq!(ui.get_float(&width), Some(92.0));
        write_symbol(&mut ui, None);
        ui.update();
        assert_eq!(ui.get_float(&width), Some(f64::from(SYMBOL_W + SYMBOL_DEAD) - 36.0));
    }

    #[test]
    fn the_text_is_cropped_to_its_column_and_the_content_keeps_its_strength() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        let menu = ui.menu(TUTORIAL_MENU).unwrap();
        let text = ui.find_tile(TEXT).unwrap();
        let title = ui.find_tile(TITLE).unwrap();
        let s = shown(&ui, "Long", &words(20), 0, 1);
        frame(&mut ui, &s, &mut view);
        let items = super::super::layout::layout(&ui, menu, &size, true);
        let of = |t| items.iter().find(|i| i.tile == t).unwrap();
        // The column's box on the screen: the background's 170, 96 and its own 108, 172.
        let clip = of(text).clip.unwrap();
        assert_eq!((clip.left, clip.top, clip.height), (278.0, 268.0, 440.0));
        assert_eq!((of(text).rect.top, of(text).rect.height), (268.0 + 21.0, 625.0));
        assert_eq!(of(title).clip, None);
        // [observed] Under a background of alpha 193.8 the text and lines read full strength.
        assert_eq!((of(title).alpha, of(text).alpha), (1.0, 1.0));
        let bg = ui.find_tile(BACKGROUND).unwrap();
        assert!((of(bg).alpha - 191.25 / 255.0).abs() < 1e-9);
    }

    #[test]
    fn the_tiles_stand_for_the_commands() {
        let ui = menu();
        let tile = |p: &str| ui.find_tile(p).unwrap();
        assert_eq!(tutorial_tile(&ui, tile(&format!("{NEXT}/button_text"))), Some(TutorialTile::Next));
        assert_eq!(tutorial_tile(&ui, tile(PREVIOUS)), Some(TutorialTile::Previous));
        assert_eq!(tutorial_tile(&ui, tile(CLOSE)), Some(TutorialTile::Close));
        assert_eq!(tutorial_tile(&ui, tile(MARKER)), Some(TutorialTile::Marker));
        assert_eq!(tutorial_tile(&ui, tile(TITLE)), None);
        let cmd = |p: &str| tile_command(&ui, tutorial_tile(&ui, tile(&format!("{BAR}/{p}"))).unwrap());
        assert_eq!(cmd("scrollbar_vert_down"), Some(Command::Scroll(30.0)));
        assert_eq!(cmd("scrollbar_vert_up"), Some(Command::Scroll(-30.0)));
        assert_eq!(cmd("scrollbar_vert_page_down"), Some(Command::Scroll(410.0)));
        assert_eq!(cmd("scrollbar_vert_page_up"), Some(Command::Scroll(-410.0)));
        assert_eq!(tile_command(&ui, TutorialTile::Marker), None);
        assert_eq!(tile_command(&ui, TutorialTile::Close), Some(Command::Close));
    }

    #[test]
    fn the_marker_drags_by_the_files_step_height() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &words(20), 0, 1);
        frame(&mut ui, &s, &mut view);
        // The bar's top on the screen: the background's 96 and its own 172; a step of
        // (424 - 32) / 215 units, the marker taken 10 under its top at value 0.
        let grab = marker_grab(&ui, 278.0).unwrap();
        assert_eq!(grab, (268.0, 10.0));
        let step = 392.0 / 215.0;
        assert_eq!(scroll_under(&ui, grab, 278.0), Some(0.0));
        assert_eq!(scroll_under(&ui, grab, 278.0 + 100.5 * step), Some(100.0));
        assert_eq!(scroll_under(&ui, grab, 5000.0), Some(215.0), "held to the bound");
        assert_eq!(scroll_under(&ui, grab, 0.0), Some(0.0));
        // Taken lower down, the marker does not jump to the pointer.
        write_scroll(&mut ui, 100.0);
        ui.update();
        let grab = marker_grab(&ui, 268.0 + 100.0 * step + 5.0).unwrap();
        assert_eq!(scroll_under(&ui, grab, 268.0 + 100.0 * step + 5.0), Some(100.0));
    }

    #[test]
    fn missing_required_paths_fall_back() {
        for (name, missing) in [("TM_title", TITLE), ("TM_page", COUNT), ("TM_text_container", COLUMN), ("TM_scrollbar", BAR), ("TM_next_button", NEXT), ("TM_close_button", CLOSE), ("TM_VDSG_symbol", SYMBOL)] {
            let mut ui = menu_changed(|xml| xml.replace(&format!("name=\"{name}\""), "name=\"Modded\""));
            assert_eq!(built(&ui), Tiled::No(format!("{missing} missing")));
            let s = shown(&ui, "T", "x", 0, 1);
            write(&mut ui, &s, 200.0);
            assert_eq!(ui.get_float(&format!("{BACKGROUND}/alpha")), Some(32.0), "nothing written");
        }
        assert_eq!(built(&menu()), Tiled::Yes);
    }
}
