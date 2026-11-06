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
//!   count's and the buttons' state, the background's opacity, and the mark that keeps the
//!   background's alpha from its children ([`super::layout::OWN_ALPHA`]). The file has no
//!   `io()` traits [documented]; the menu's code addresses its tiles by `<id>` and by these
//!   traits [hypothesis]. `clipwindow` is turned on every frame after the scripts
//!   ([`mark_windows`]), so a window injected or enabled later is covered too.
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
//!   to is written, measured and its scroll written ([`settle`]), so a key or a click never
//!   draws new text with an old scroll or the other way round. A page a script adds in the
//!   frame's own `run_scripts` reaches the tree the next frame (tutorial-menu.md §5).

use super::layout::Rect;
use super::menus::{SizesWritten, Tiled};
use crate::tutorial_menu::{Command, Page, TutorialView};
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
const UP: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_scrollbar/scrollbar_vert_up";
const DOWN: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_scrollbar/scrollbar_vert_down";
const PAGE_UP: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_scrollbar/scrollbar_vert_page_up";
const PAGE_DOWN: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_scrollbar/scrollbar_vert_page_down";
const NEXT: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_next_button";
const PREVIOUS: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_prev_button";
const CLOSE: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_close_button";
const NEXT_TEXT: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_next_button/button_text";
const PREVIOUS_TEXT: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_prev_button/button_text";
const CLOSE_TEXT: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_close_button/button_text";
const SYMBOL: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/TM_VDSG_symbol";

/// The background's `alpha` (0 to 255): what the original's `_background_fill_alpha` reads
/// while the file's `alpha` copies it, although globals.xml writes 32 [observed: b2_tutorial,
/// menus-findings; 255 x 0.76]. Not the hand-built menu's `crate::interface::MENU_BACKING`.
pub const BACKING_ALPHA: f64 = 193.8;

/// [design] Every fixed path the model writes or input reads is part of a successful build,
/// and the buttons' words, which give them and the column their sizes; a file a mod
/// restructured keeps the hand-built menu.
pub fn built(ui: &Ui) -> Tiled {
    for path in [TUTORIAL_MENU, BACKGROUND, DEPTH, TITLE, COUNT, COLUMN, TEXT, BAR, MARKER, UP, DOWN, PAGE_UP, PAGE_DOWN, NEXT, PREVIOUS, CLOSE, NEXT_TEXT, PREVIOUS_TEXT, CLOSE_TEXT, SYMBOL] {
        if ui.find_tile(path).is_none() {
            return Tiled::No(format!("{path} missing"));
        }
    }
    Tiled::Yes
}

/// Whether the open tutorial menu is drawn from its tiles: the switch on, the menu files
/// read, the menu open and its file built with every model path. Every drawing and input
/// reader uses this decision; the tile input also waits for the hand-built buttons of the
/// frame before to go (`super::input`) [design].
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

/// The body's text from the message's: every line break the message has stays, tags go
/// (`crate::tutorial_menu::marks_to_breaks`), each line is trimmed, and runs of empty lines
/// count as one, with none at either end [observed: the original shows the single breaks of
/// 0x0007235B's list on lines of their own, and the double ones as one empty line: 625 = 36 +
/// 19 x 31; hypothesis: `<p>` as an empty line, as the hand-built menu].
/// The hand-built [`crate::tutorial_menu::plain_text`] joins the single breaks instead.
pub fn body_text(message: &str) -> String {
    let marked = crate::tutorial_menu::marks_to_breaks(message);
    let mut lines: Vec<&str> = Vec::new();
    for line in marked.split('\n').map(str::trim) {
        if !line.is_empty() || lines.last().is_some_and(|l| !l.is_empty()) {
            lines.push(line);
        }
    }
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Write what `TutorialMenu` shows of `shown`: see the module notes.
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
pub fn write(ui: &mut Ui, shown: &Shown) {
    if !built(ui).yes() {
        return;
    }
    let Shown { page, at, of, .. } = shown;
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
    set(ui, BACKGROUND, "alpha", Value::Num(BACKING_ALPHA));
    set(ui, DEPTH, super::layout::OWN_ALPHA, Value::Num(1.0));
}

/// Turn the menu's `clipwindow`s on: on every tiled frame, once the tree is worked out after
/// the scripts and before it is drawn, so a window injected or enabled at any time crops from
/// that frame on. A walk of the menu's tiles, writing only for a window not marked yet.
pub fn mark_windows(ui: &mut Ui) {
    if let Some(menu) = ui.menu(TUTORIAL_MENU) {
        fnv_ui::input::mark_clip_windows(ui, menu);
    }
}

/// A tiled frame's sizes, range and scroll, after what it writes of the page: the tree worked
/// out first, as the screen may have changed with the page the same (a new window size leads
/// to the column's width and the body's wrap width); the sizes lead to the places, and the
/// column's width to the wrap width, so worked out again until the body is measured at its
/// final width [design: three rounds are the buttons, the column and the body]. Only then the
/// range is read and the scroll, held in it, written. Returns the scroll written.
///
/// `write_sizes` is one round of measuring, called with the tree worked out: it finds its
/// fonts then, so a font computed from what was just written has its face loaded.
pub fn settle(ui: &mut Ui, sizes: &mut SizesWritten, write_sizes: &mut dyn FnMut(&mut Ui, &mut SizesWritten) -> usize, view: &mut TutorialView) -> f64 {
    ui.update();
    for _ in 0..3 {
        if write_sizes(ui, sizes) == 0 {
            break;
        }
        ui.update();
    }
    view.set_scroll_bound(scroll_bound(ui) as f32);
    let scroll = f64::from(view.scroll);
    write_scroll(ui, scroll);
    scroll
}

/// The VDSG symbol's picture's width, which its `width` and the words after it are worked
/// out from: `filewidth` less the file's `_dead_space` [documented: tutorial_menu.xml;
/// observed: its header, 128 x 64, Fallout - Textures2.bsa]. `texels` is what the file
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

/// The marker's `y` in the scrollbar, as worked out from the scroll by the file.
pub fn marker_y(ui: &Ui) -> Option<f64> {
    ui.get_float(&format!("{MARKER}/y"))
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

/// What a click on `tile` stands for, while the tutorial menu is drawn from its tiles: `menu`
/// is that menu ([`tiled_menu`]), none otherwise. Off or in the fallback a script's queued
/// click on the file's tile acts on nothing: the hand-built menu takes only its own input.
pub fn clicked(ui: &Ui, tile: TileId, menu: Option<TileId>) -> Option<TutorialTile> {
    if menu? != ui.root_of(tile) {
        return None;
    }
    tutorial_tile(ui, tile)
}

/// The menu when it is drawn from its tiles ([`tiled`]).
pub fn tiled_menu(on: bool, game: &fnv_game::Game<'_>) -> Option<TileId> {
    if tiled(on, game) {
        game.ui_tree.menu(TUTORIAL_MENU)
    } else {
        None
    }
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

/// What a press on the marker holds while it drags: the menu it was made in, the scrollbar's
/// top and how far under the marker's top the pointer took it (interface units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub menu: TileId,
    pub top: f64,
    pub grab: f64,
}

/// Whether a drag goes on: the press held and that very menu still drawn from its tiles
/// (`now`, [`tiled_menu`]); a menu closed, or built again, ends it.
pub fn drag_holds(drag: &Drag, now: Option<TileId>, held: bool) -> bool {
    held && now == Some(drag.menu)
}

/// The [`Drag`] of a press on the marker. The layout lists the drawn tiles (the marker, not
/// the scrollbar's hotrect), so this lays out the whole menu: worked out once a press, as
/// the wait menu's slider [design].
pub fn marker_grab(ui: &Ui, pointer_y: f64) -> Option<Drag> {
    let marker = ui.find_tile(MARKER)?;
    let menu = ui.menu(TUTORIAL_MENU)?;
    let placed: Rect = super::layout::layout(ui, menu, &|_, _, _| (0.0, 0.0), true).into_iter().find(|i| i.tile == marker)?.rect;
    Some(Drag { menu, top: placed.top - ui.tile(marker).get("y").map_or(0.0, Value::num), grab: pointer_y - placed.top })
}

/// The scroll for the pointer at `pointer_y` with the marker held ([`marker_grab`]):
/// the marker's place over the file's `_step_height`, whole values [documented:
/// scrollbar_vert.xml's `_drag_value`; design: the marker's own drag traits are not built,
/// as the wait menu's slider].
pub fn scroll_under(ui: &Ui, Drag { top, grab, .. }: Drag, pointer_y: f64) -> Option<f64> {
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
pub(crate) mod tests {
    use super::super::menus::{fonts_of, write_text_sizes_of, Size};
    use super::*;
    use crate::tutorial_menu::{SYMBOL_DEAD, SYMBOL_W};
    use fnv_sim::FormId;
    use fnv_ui::MemSource;

    /// [design] A small layout written for these tests, with its own structure, numbers and
    /// expressions; it does not come from the game's files and says nothing about them. It keeps
    /// the tile names and paths the model and the input look up, which are the engine's interface
    /// ([`built`]), and only the dependencies the tests exercise. The real layout's numbers are
    /// checked by the smoke scenario `tiles-tutorial`.
    fn menu() -> Ui {
        menu_changed(str::to_string)
    }

    /// The test screen's width: the column is this less 300, 900 wide.
    const SCREEN_WIDTH: f64 = 1200.0;
    /// The body's top in its column before any scroll.
    const TEXT_TOP: f64 = 7.0;
    /// The items the scrollbar shows at once.
    const VISIBLE: f64 = 200.0;
    /// The scrollbar's top on the screen: the background's 40 and its own 60.
    const BAR_TOP: f64 = 100.0;

    pub(crate) fn menu_changed(change: impl FnOnce(&str) -> String) -> Ui {
        let src = MemSource::default().with(
            "tm.xml",
            &change(
                r#"<menu name="TutorialMenu"><rect name="NOGLOW_BRANCH">
  <image name="TM_Background"><x>50</x><y>40</y><width>1100</width><height>600</height><alpha>20</alpha>
   <rect name="TM_depth_rect">
    <text name="TM_title"><string>Heading</string></text>
    <text name="TM_page"><string>0/0</string></text>
    <image name="CountWatcher"><_count_on><copy src="sibling(TM_page)" trait="visible"/></_count_on></image>
    <hotrect name="TM_scrollbar"><x>10</x><y>60</y><width>20</width><height>300</height><alpha>0</alpha><target>&true;</target>
      <_step_size>40</_step_size>
      <_jump_size>150</_jump_size>
      <_number_of_visible_items>200</_number_of_visible_items>
      <_number_of_items><copy src="sibling(TM_text_container)" trait="_text_height"/></_number_of_items>
      <_current_value><copy src="child(scrollbar_vert_down)" trait="clicked"/></_current_value>
      <hotrect name="scrollbar_vert_page_up"><target>&true;</target><width>20</width><height>100</height></hotrect>
      <hotrect name="scrollbar_vert_page_down"><target>&true;</target><y>120</y><width>20</width><height>180</height></hotrect>
      <image name="scrollbar_vert_marker"><target>&true;</target><width>20</width><height>20</height>
        <_step_height>2.5</_step_height>
        <y><copy src="parent()" trait="_current_value"/><mul src="me()" trait="_step_height"/></y>
      </image>
      <image name="scrollbar_vert_up"/><image name="scrollbar_vert_down"/>
    </hotrect>
    <hotrect name="TM_text_container"><x>40</x><y>60</y><height>200</height><alpha>0</alpha><clipwindow>&true;</clipwindow>
      <width><copy src="screen()" trait="width"/><sub>300</sub></width>
      <_text_height><copy src="child(TM_text)" trait="height"/></_text_height>
      <_shift><copy src="sibling(TM_scrollbar)" trait="_current_value"/><mul>-1</mul></_shift>
      <text name="TM_text"><wrapwidth><copy src="parent()" trait="width"/></wrapwidth>
        <y><copy src="parent()" trait="_shift"/><add>7</add></y></text>
    </hotrect>
    <hotrect name="TM_next_button"><target>&true;</target><text name="button_text"/></hotrect>
    <hotrect name="TM_prev_button"><target>&true;</target><text name="button_text"/></hotrect>
    <hotrect name="TM_close_button"><target>&true;</target><text name="button_text"/></hotrect>
    <image name="TM_VDSG_symbol"><filename>test\emblem.dds</filename>
      <width><copy src="me()" trait="filewidth"/><add>3</add></width></image>
   </rect></image></rect></menu>"#,
            ),
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "tm.xml").unwrap();
        screen(&mut ui, SCREEN_WIDTH);
        ui.update();
        ui
    }

    /// A new window size, as `super::super::write_screen` writes it: no evaluation with it.
    fn screen(ui: &mut Ui, width: f64) {
        let id = ui.screen();
        ui.set_trait(id, "width", Value::Num(width));
    }

    pub(crate) fn shown(ui: &Ui, title: &str, text: &str, at: usize, of: usize) -> Shown {
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

    /// The text's height with words 100 wide each, so its lines follow the wrap width.
    fn wrapped(_: f64, text: &str, wrap: Option<f64>) -> (f64, f64) {
        let words = text.split_whitespace().count().max(1) as f64;
        let per_line = wrap.map_or(words, |w| (w / 100.0).floor().max(1.0));
        (wrap.unwrap_or(words * 100.0), lines((words / per_line).ceil() as usize))
    }

    /// A page's change as `super::super::write_tutorial` does it: `write`, the symbol, then
    /// [`settle`], and what `update` does after the scripts.
    fn frame(ui: &mut Ui, s: &Shown, view: &mut TutorialView) {
        write(ui, s);
        write_symbol(ui, Some(128.0));
        unchanged_frame(ui, &mut SizesWritten::default(), &size, view);
    }

    /// A frame with the page the same: [`settle`] with the sizes kept from the frames before.
    fn unchanged_frame(ui: &mut Ui, sizes: &mut SizesWritten, measure: &Size<'_>, view: &mut TutorialView) {
        let menu = ui.menu(TUTORIAL_MENU).unwrap();
        let texts = super::super::menus::texts(ui, menu);
        settled(ui, &texts, sizes, measure, view);
        ui.update();
        mark_windows(ui);
    }

    /// [`settle`] with a measure whose fonts are all there.
    fn settled(ui: &mut Ui, texts: &[TileId], sizes: &mut SizesWritten, measure: &Size<'_>, view: &mut TutorialView) {
        settle(ui, sizes, &mut |ui, sizes| write_text_sizes_of(ui, texts, sizes, measure), view);
    }

    /// What the fixture's watcher tile read of the count's `visible`.
    const COUNT_WATCHED: &str = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect/CountWatcher/_count_on";

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
        // A tile reading the count's `visible`, a trait the layout does not have until the model
        // writes it, follows the write.
        assert_eq!(ui.get_float(COUNT_WATCHED), Some(1.0));
        // The first page of two: Next is a target, Previous not.
        assert_eq!(ui.get_float(&format!("{NEXT}/target")), Some(1.0));
        assert_eq!(ui.get_float(&format!("{PREVIOUS}/target")), Some(0.0));
        assert_eq!(ui.get_float(&format!("{BACKGROUND}/alpha")), Some(193.8), "the original's value, not the hand-built 0.75");
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
        assert_eq!(ui.get_float(COUNT_WATCHED), Some(0.0));
        // Next and Previous are not shown: the original's one-page help has neither.
        assert_eq!(ui.get_float(&format!("{NEXT}/visible")), Some(0.0));
        assert_eq!(ui.get_float(&format!("{PREVIOUS}/visible")), Some(0.0));
        assert!(describe(&ui, &s).contains("count hidden"), "{}", describe(&ui, &s));
    }

    #[test]
    fn measured_text_sets_scroll_extent() {
        // A text of 20 lines, 625 high, carried on to the scrollbar's items, of which 200 show.
        // The body wraps at the column's width, which follows the screen.
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &words(20), 0, 1);
        frame(&mut ui, &s, &mut view);
        assert_eq!(ui.get_float(&format!("{TEXT}/height")), Some(625.0));
        assert_eq!(ui.get_float(&format!("{COLUMN}/_text_height")), Some(625.0));
        assert_eq!(ui.get_float(&format!("{BAR}/_number_of_items")), Some(625.0));
        assert_eq!(scroll_bound(&ui), 625.0 - VISIBLE);
        assert_eq!(ui.get_float(&format!("{TEXT}/wrapwidth")), Some(SCREEN_WIDTH - 300.0));
        // A short text does not scroll.
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
        assert_eq!(text_y(&ui), Some(TEXT_TOP - 120.0), "the layout moves the text by the value");
        assert_eq!(pulse(&mut ui, true, 150.0), 150.0, "the click's frame");
        assert_eq!(pulse(&mut ui, false, 150.0), 150.0, "the pulse's end");
        assert_eq!(pulse(&mut ui, false, 150.0), 150.0, "a frame later");
        assert_eq!(text_y(&ui), Some(TEXT_TOP - 150.0));
        // The expression competing for the value, left alone, would be nothing like it.
        let down = ui.find_tile(&format!("{BAR}/scrollbar_vert_down")).unwrap();
        ui.set_trait(down, "clicked", Value::Num(1.0));
        ui.update();
        assert_eq!(scroll_shown(&ui), Some(1.0), "the layout's own expression, no model write");
    }

    #[test]
    fn page_change_and_resize_clamp_scroll() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        // At the bottom of a long page.
        let long = shown(&ui, "Long", &words(20), 0, 2);
        frame(&mut ui, &long, &mut view);
        assert!(!view.apply(2, Command::Scroll(500.0)));
        assert_eq!(view.scroll, 425.0, "the layout's bound, not a whole number of steps");
        frame(&mut ui, &long, &mut view);
        assert_eq!(text_y(&ui), Some(TEXT_TOP - 425.0));
        // A turn starts the next page at its top, whatever its length.
        assert!(!view.apply(2, Command::Next));
        assert_eq!((view.page, view.scroll), (1, 0.0));
        let short = shown(&ui, "Short", &words(3), 1, 2);
        frame(&mut ui, &short, &mut view);
        assert_eq!(text_y(&ui), Some(TEXT_TOP));
    }

    #[test]
    fn a_resize_with_the_page_the_same_settles_in_its_own_frame() {
        // 180 words 100 wide. The column 900 wide: nine a line, 20 lines, 625 high.
        let mut ui = menu();
        let mut view = TutorialView::default();
        let mut sizes = SizesWritten::default();
        let s = shown(&ui, "Long", &words(180), 0, 1);
        write(&mut ui, &s);
        unchanged_frame(&mut ui, &mut sizes, &wrapped, &mut view);
        assert_eq!((ui.get_float(&format!("{TEXT}/height")), scroll_bound(&ui)), (Some(625.0), 425.0));
        assert!(!view.apply(1, Command::Scroll(500.0)));
        unchanged_frame(&mut ui, &mut sizes, &wrapped, &mut view);
        assert_eq!(text_y(&ui), Some(TEXT_TOP - 425.0));
        // Narrower: the column 700, seven a line, 26 lines, 811 high. One frame, the model not
        // written again: the body, the range and the scroll are all of the new size as the
        // frame's writes end, before the tree is worked out after the scripts.
        screen(&mut ui, 1000.0);
        let texts = super::super::menus::texts(&ui, ui.menu(TUTORIAL_MENU).unwrap());
        settled(&mut ui, &texts, &mut sizes, &wrapped, &mut view);
        assert_eq!(ui.get_float(&format!("{TEXT}/wrapwidth")), Some(700.0));
        assert_eq!(ui.get_float(&format!("{TEXT}/height")), Some(811.0));
        assert_eq!(scroll_bound(&ui), 611.0);
        assert_eq!(view.scroll, 425.0, "still in the wider range");
        ui.update();
        assert_eq!(ui.get_float(&format!("{TEXT}/height")), Some(811.0), "nothing moves after the scripts");
        assert_eq!(text_y(&ui), Some(TEXT_TOP - 425.0));
        // Wider: the column 1900, 19 a line, 10 lines, 315 high: less range, the scroll held to it.
        screen(&mut ui, 2200.0);
        settled(&mut ui, &texts, &mut sizes, &wrapped, &mut view);
        assert_eq!(ui.get_float(&format!("{TEXT}/height")), Some(315.0));
        assert_eq!((scroll_bound(&ui), view.scroll), (115.0, 115.0));
        ui.update();
        assert_eq!(text_y(&ui), Some(TEXT_TOP - 115.0));
        // Wider still: the column 5900, 59 a line, 4 lines, 129 high: no range left.
        screen(&mut ui, 6200.0);
        settled(&mut ui, &texts, &mut sizes, &wrapped, &mut view);
        assert_eq!(ui.get_float(&format!("{TEXT}/height")), Some(129.0));
        assert_eq!((scroll_bound(&ui), view.scroll), (0.0, 0.0));
        ui.update();
        assert_eq!(text_y(&ui), Some(TEXT_TOP));
    }

    #[test]
    fn a_window_injected_or_enabled_later_crops_on_its_next_frame() {
        let mut ui = menu_changed(|xml| xml.replace("<clipwindow>&true;</clipwindow>", "<clipwindow>&false;</clipwindow>"));
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &words(20), 0, 1);
        frame(&mut ui, &s, &mut view);
        let menu = ui.menu(TUTORIAL_MENU).unwrap();
        let text = ui.find_tile(TEXT).unwrap();
        let clip = |ui: &Ui, t| super::super::layout::layout(ui, menu, &size, true).into_iter().find(|i| i.tile == t).unwrap().clip;
        assert_eq!(clip(&ui, text), None, "false: nothing crops");
        // Enabled with the page the same.
        let column = ui.find_tile(COLUMN).unwrap();
        ui.set_trait(column, "clipwindow", Value::Num(1.0));
        unchanged_frame(&mut ui, &mut SizesWritten::default(), &size, &mut view);
        assert_eq!(clip(&ui, text).map(|c| c.height), Some(200.0));
        // Injected with the page the same, by a mod's script.
        let src = MemSource::default();
        let depth = "TutorialMenu/NOGLOW_BRANCH/TM_Background/TM_depth_rect";
        assert!(ui.inject_text(&src, depth, r#"<hotrect name="Late"><width>10</width><height>10</height><clipwindow>1</clipwindow><image name="Inner"><width>99</width><height>99</height></image></hotrect>"#));
        unchanged_frame(&mut ui, &mut SizesWritten::default(), &size, &mut view);
        let inner = ui.find_tile(&format!("{depth}/Late/Inner")).unwrap();
        assert_eq!(clip(&ui, inner).map(|c| c.width), Some(10.0));
    }

    #[test]
    fn a_click_counts_only_while_drawn_from_tiles() {
        let mut ui = menu();
        ui.load_menu(&MemSource::default().with("o.xml", r#"<menu name="Other"><image name="scrollbar_vert_up"/></menu>"#), "o.xml").unwrap();
        let tutorial = ui.menu(TUTORIAL_MENU);
        let next = ui.find_tile(NEXT).unwrap();
        let close = ui.find_tile(&format!("{CLOSE}/button_text")).unwrap();
        // Drawn from tiles: its tiles' commands.
        assert_eq!(clicked(&ui, next, tutorial), Some(TutorialTile::Next));
        assert_eq!(clicked(&ui, close, tutorial), Some(TutorialTile::Close));
        // Off or in the fallback a script's queued click on the same tiles does nothing.
        assert_eq!(clicked(&ui, next, None), None);
        assert_eq!(clicked(&ui, close, None), None);
        // A tile of that name in another menu is not this menu's.
        let other = ui.find_tile("Other/scrollbar_vert_up").unwrap();
        assert_eq!(clicked(&ui, other, tutorial), None);
    }

    #[test]
    fn a_drag_ends_with_its_press_and_its_menu() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &words(20), 0, 1);
        frame(&mut ui, &s, &mut view);
        let drag = marker_grab(&ui, BAR_TOP + 10.0).unwrap();
        let menu = ui.menu(TUTORIAL_MENU);
        assert!(drag_holds(&drag, menu, true));
        assert!(!drag_holds(&drag, menu, false), "the press let go");
        assert!(!drag_holds(&drag, None, true), "the menu closed, or no longer tiled");
        // Opened again with the mouse still down: a new menu tile, and no old grab.
        let again = ui.find_tile(BAR);
        assert!(!drag_holds(&drag, again, true), "another tile than the one it was made in");
        let src = MemSource::default().with("tm2.xml", r#"<menu name="TutorialMenu"/>"#);
        let built_again = ui.load_menu(&src, "tm2.xml").unwrap();
        assert_ne!(Some(built_again), menu);
        assert!(!drag_holds(&drag, Some(built_again), true));
    }

    #[test]
    fn the_symbols_width_comes_from_its_picture() {
        let mut ui = menu();
        let width = format!("{SYMBOL}/width");
        assert_eq!(ui.get_float(&width), Some(3.0), "the layout alone, before the code writes");
        assert_eq!(symbol_file(&ui).as_deref(), Some("test\\emblem.dds"));
        write_symbol(&mut ui, Some(128.0));
        ui.update();
        assert_eq!(ui.get_float(&width), Some(131.0));
        write_symbol(&mut ui, None);
        ui.update();
        assert_eq!(ui.get_float(&width), Some(f64::from(SYMBOL_W + SYMBOL_DEAD) + 3.0));
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
        // The column's box on the screen: the background's 50, 40 and its own 40, 60.
        let clip = of(text).clip.unwrap();
        assert_eq!((clip.left, clip.top, clip.height), (90.0, 100.0, 200.0));
        assert_eq!((of(text).rect.top, of(text).rect.height), (100.0 + TEXT_TOP, 625.0));
        assert_eq!(of(title).clip, None);
        // [observed] Under a background of alpha 193.8 the text and lines read full strength.
        assert_eq!((of(title).alpha, of(text).alpha), (255.0, 255.0));
        let bg = ui.find_tile(BACKGROUND).unwrap();
        assert!((of(bg).alpha - BACKING_ALPHA).abs() < 1e-9);
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
        assert_eq!(cmd("scrollbar_vert_down"), Some(Command::Scroll(40.0)));
        assert_eq!(cmd("scrollbar_vert_up"), Some(Command::Scroll(-40.0)));
        assert_eq!(cmd("scrollbar_vert_page_down"), Some(Command::Scroll(150.0)));
        assert_eq!(cmd("scrollbar_vert_page_up"), Some(Command::Scroll(-150.0)));
        assert_eq!(tile_command(&ui, TutorialTile::Marker), None);
        assert_eq!(tile_command(&ui, TutorialTile::Close), Some(Command::Close));
    }

    /// The fixture with the marker's step height `step`.
    fn with_step_height(step: &str) -> Ui {
        menu_changed(|xml| xml.replace("<_step_height>2.5</_step_height>", &format!("<_step_height>{step}</_step_height>")))
    }

    #[test]
    fn the_marker_drags_by_its_step_height() {
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &words(20), 0, 1);
        frame(&mut ui, &s, &mut view);
        // The marker taken 10 under its top at value 0.
        let grab = marker_grab(&ui, BAR_TOP + 10.0).unwrap();
        assert_eq!((grab.top, grab.grab), (BAR_TOP, 10.0));
        assert_eq!(scroll_under(&ui, grab, BAR_TOP + 10.0), Some(0.0));
        assert_eq!(scroll_under(&ui, grab, BAR_TOP + 10.0 + 100.5 * 2.5), Some(100.0), "whole values");
        assert_eq!(scroll_under(&ui, grab, 5000.0), Some(425.0), "held to the bound");
        assert_eq!(scroll_under(&ui, grab, 0.0), Some(0.0));
        // Another step height: it is read from the marker, not assumed.
        let mut other = with_step_height("4");
        let s = shown(&other, "Long", &words(20), 0, 1);
        frame(&mut other, &s, &mut TutorialView::default());
        let grab = marker_grab(&other, BAR_TOP + 10.0).unwrap();
        assert_eq!(scroll_under(&other, grab, BAR_TOP + 10.0 + 50.5 * 4.0), Some(50.0));
        // Taken lower down, the marker does not jump to the pointer and gives its own value
        // back, also at a step that has no exact binary value.
        let mut inexact = with_step_height("1.7");
        let s = shown(&inexact, "Long", &words(20), 0, 1);
        frame(&mut inexact, &s, &mut TutorialView::default());
        for value in [1.0, 30.0, 100.0, 215.0, 333.0] {
            write_scroll(&mut inexact, value);
            inexact.update();
            let at = BAR_TOP + marker_y(&inexact).unwrap() + 5.0;
            let grab = marker_grab(&inexact, at).unwrap();
            assert_eq!(scroll_under(&inexact, grab, at), Some(value));
        }
    }

    #[test]
    fn the_body_keeps_the_messages_line_breaks() {
        // The shape of 0x0007235B: paragraphs apart by an empty line, a list by single breaks.
        assert_eq!(body_text("a b\r\n\r\nc:\r\n\r\n* d\r\n* e\r\n\r\nf"), "a b\n\nc:\n\n* d\n* e\n\nf");
        // The shape of 0x00072358: `<div>` at the start, `<p>` on a line of its own between.
        assert_eq!(body_text("<div>\r\na\r\n<p>\r\nb\r\n<p>\r\nc"), "a\n\nb\n\nc");
        assert_eq!(body_text("a<br>b </div>"), "a\n\nb");
        assert_eq!(body_text(" a \n\n\n\n b \r\n\r\n"), "a\n\nb");
        assert_eq!(body_text("a < b"), "a < b");
        assert_eq!(body_text(""), "");
        // The hand-built drawing's text is left as it was.
        assert_eq!(crate::tutorial_menu::plain_text("* d\r\n* e\r\n\r\nf"), "* d * e\n\nf");
    }

    /// The text's height line by line: nine words to a line, an empty line one line.
    fn by_lines(_: f64, text: &str, wrap: Option<f64>) -> (f64, f64) {
        let n: usize = text.split('\n').map(|l| l.split_whitespace().count().div_ceil(9).max(1)).sum();
        (wrap.unwrap_or(900.0), lines(n))
    }

    #[test]
    fn single_breaks_make_the_observed_height() {
        // [observed] The original's 625 high, 20 lines: 5, an empty one, 1, an empty one, 5 of
        // the list, an empty one, 3, an empty one, 2. Joined, the list takes 2 and leaves 532.
        let message = format!("{}\r\n\r\nh\r\n\r\n* a\r\n* b\r\n* c\r\n* d\r\n* e\r\n\r\n{}\r\n\r\n{}", words(45), words(27), words(18));
        assert_eq!(by_lines(0.0, &crate::tutorial_menu::plain_text(&message), None).1, 532.0);
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &body_text(&message), 0, 1);
        write(&mut ui, &s);
        unchanged_frame(&mut ui, &mut SizesWritten::default(), &by_lines, &mut view);
        assert_eq!(ui.get_float(&format!("{TEXT}/height")), Some(625.0));
    }

    /// The menu with `lines_of_text` lines scrolled to `value`.
    fn scrolled(lines_of_text: usize, value: f64) -> Ui {
        let mut ui = menu();
        let mut view = TutorialView::default();
        let s = shown(&ui, "Long", &words(lines_of_text), 0, 1);
        frame(&mut ui, &s, &mut view);
        write_scroll(&mut ui, value);
        ui.update();
        ui
    }

    /// The model's write is the value read back and asks the layout to place the marker by the
    /// value, and a layout's marker follows it by the step height it holds. The real layout's
    /// marker numbers are the smoke scenario `tiles-tutorial`'s.
    #[test]
    fn the_models_scroll_is_read_back_and_a_marker_follows_it() {
        let ui = scrolled(20, 40.0);
        assert_eq!(scroll_shown(&ui), Some(40.0));
        assert_eq!(ui.get_float(&format!("{BAR}/_SetInCode")), Some(1.0));
        assert_eq!(marker_y(&ui), Some(100.0), "40 x 2.5");
        let mut other = with_step_height("4");
        write_scroll(&mut other, 40.0);
        other.update();
        assert_eq!(marker_y(&other), Some(160.0), "40 x 4");
    }

    /// A click either side of the marker reaches the page area there and its command, and one on
    /// the marker drags. The areas here stand still, and no test checks how the real layout's
    /// move with the marker: `tiles-tutorial` checks the marker's places and one page-down click.
    #[test]
    fn a_click_either_side_of_the_marker_is_that_sides_page_command() {
        // The scrollbar on the screen: x 60 to 80. Its areas: y 100 to 200 and y 220 to 400.
        let on = |ui: &Ui, y: f64| {
            let menu = ui.menu(TUTORIAL_MENU).unwrap();
            let hit = fnv_ui::input::hit(ui, &[menu], (70.0, y), &size)?;
            clicked(ui, hit.tile, Some(menu))
        };
        // The marker at 100 under the bar's top: y 200 to 220, between the areas.
        let ui = scrolled(20, 40.0);
        assert_eq!(on(&ui, 150.0), Some(TutorialTile::PageUp));
        assert_eq!(on(&ui, 210.0), Some(TutorialTile::Marker));
        assert_eq!(on(&ui, 300.0), Some(TutorialTile::PageDown));
        assert_eq!(tile_command(&ui, TutorialTile::PageUp), Some(Command::Scroll(-150.0)));
        assert_eq!(tile_command(&ui, TutorialTile::PageDown), Some(Command::Scroll(150.0)));
        assert_eq!(tile_command(&ui, TutorialTile::Marker), None);
        // The marker moved to y 300 to 320: drawn over the lower area, it takes the click there.
        let ui = scrolled(20, 80.0);
        assert_eq!(on(&ui, 310.0), Some(TutorialTile::Marker));
        assert_eq!(on(&ui, 330.0), Some(TutorialTile::PageDown));
    }

    /// A renderer's faces: a font not loaded measures nothing, as `FontKit::menu_measure`.
    #[derive(Default)]
    struct Faces(Vec<f64>);

    impl Faces {
        fn load(&mut self, fonts: &[f64]) {
            for &f in fonts {
                if !self.0.contains(&f) {
                    self.0.push(f);
                }
            }
        }

        fn measure(&self, font: f64, text: &str, wrap: Option<f64>) -> (f64, f64) {
            if self.0.contains(&font) {
                size(font, text, wrap)
            } else {
                (0.0, 0.0)
            }
        }
    }

    #[test]
    fn a_font_computed_from_what_was_written_is_measured_in_its_own_face() {
        // The limits of this test: `round` repeats the closure in `super::super::write_tutorial`
        // (which takes the engine's `FontKit`), so a change to that closure alone would not fail
        // it; and `size` ignores which face it is given, so it shows each round's fonts are
        // loaded before they are measured, not that a text gets its own face's metrics.
        // A mod's title: font 6, or 7 with the count shown, which `write` turns on.
        let mut ui = menu_changed(|xml| {
            xml.replace(r#"<text name="TM_title">"#, r#"<text name="TM_title"><font><copy>6</copy><add src="sibling(TM_page)" trait="visible"/></font>"#)
        });
        let mut view = TutorialView::default();
        let mut sizes = SizesWritten::default();
        let mut faces = Faces::default();
        let texts = super::super::menus::texts(&ui, ui.menu(TUTORIAL_MENU).unwrap());
        let (font, height) = (format!("{TITLE}/font"), format!("{TITLE}/height"));
        let one = shown(&ui, "Title", &words(3), 0, 1);
        write(&mut ui, &one);
        let mut round = |ui: &mut Ui, sizes: &mut SizesWritten| {
            faces.load(&fonts_of(ui, &texts));
            write_text_sizes_of(ui, &texts, sizes, &|f, t, w| faces.measure(f, t, w))
        };
        settle(&mut ui, &mut sizes, &mut round, &mut view);
        assert_eq!((ui.get_float(&font), ui.get_float(&height)), (Some(6.0), Some(lines(1))));
        let two = shown(&ui, "Title", &words(3), 0, 2);
        write(&mut ui, &two);
        assert_eq!(ui.get_float(&font), Some(6.0), "not worked out yet: looking now would miss font 7");
        settle(&mut ui, &mut sizes, &mut round, &mut view);
        assert_eq!((ui.get_float(&font), ui.get_float(&height)), (Some(7.0), Some(lines(1))));
    }

    #[test]
    fn a_zero_measure_is_kept_as_any_other() {
        let mut ui = menu();
        let texts = super::super::menus::texts(&ui, ui.menu(TUTORIAL_MENU).unwrap());
        let height = format!("{TITLE}/height");
        let mut sizes = SizesWritten::default();
        let asked = std::cell::Cell::new(0);
        let nothing = |_: f64, _: &str, _: Option<f64>| {
            asked.set(asked.get() + 1);
            (0.0, 0.0)
        };
        assert!(write_text_sizes_of(&mut ui, &texts, &mut sizes, &nothing) > 0);
        assert_eq!(ui.get_float(&height), Some(0.0));
        assert!(sizes.contains_key(&ui.find_tile(TITLE).unwrap()));
        let first = asked.get();
        assert!(first > 0);
        // The same string, font and wrap are not measured again.
        assert_eq!(write_text_sizes_of(&mut ui, &texts, &mut sizes, &nothing), 0);
        assert_eq!(asked.get(), first);
        // Another string is.
        ui.set_string(&format!("{TITLE}/string"), "Other");
        assert_eq!(write_text_sizes_of(&mut ui, &texts, &mut sizes, &size), 1);
        assert_eq!(ui.get_float(&height), Some(lines(1)));
    }

    #[test]
    fn missing_required_paths_fall_back() {
        for (name, missing) in [
            ("TM_title", TITLE),
            ("TM_page", COUNT),
            ("TM_text_container", COLUMN),
            ("TM_text", TEXT),
            ("TM_scrollbar", BAR),
            ("scrollbar_vert_marker", MARKER),
            ("scrollbar_vert_up", UP),
            ("scrollbar_vert_down", DOWN),
            ("scrollbar_vert_page_up", PAGE_UP),
            ("scrollbar_vert_page_down", PAGE_DOWN),
            ("TM_next_button", NEXT),
            ("TM_prev_button", PREVIOUS),
            ("TM_close_button", CLOSE),
            ("button_text", NEXT_TEXT),
            ("TM_VDSG_symbol", SYMBOL),
        ] {
            let mut ui = menu_changed(|xml| xml.replace(&format!("name=\"{name}\""), "name=\"Modded\""));
            assert_eq!(built(&ui), Tiled::No(format!("{missing} missing")));
            let s = shown(&ui, "T", "x", 0, 1);
            write(&mut ui, &s);
            assert_eq!(ui.get_float(&format!("{BACKGROUND}/alpha")), Some(20.0), "nothing written");
        }
        // Each button's words on their own.
        for (button, missing) in [("TM_prev_button", PREVIOUS_TEXT), ("TM_close_button", CLOSE_TEXT)] {
            let was = format!(r#"<hotrect name="{button}"><target>&true;</target><text name="button_text"/>"#);
            let ui = menu_changed(|xml| xml.replace(&was, &format!(r#"<hotrect name="{button}"><target>&true;</target><text name="Modded"/>"#)));
            assert_eq!(built(&ui), Tiled::No(format!("{missing} missing")));
        }
        assert_eq!(built(&menu()), Tiled::Yes);
    }
}
