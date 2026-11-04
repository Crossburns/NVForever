//! The dialogue menu drawn from `menus\dialog\dialog_menu.xml`, behind
//! `[UI] TileMenus=DialogMenu` [design: ui-tile-tree.md §6.2 step 3; the contract:
//! docs/spec-notes/dialogue-menu.md §6].
//!
//! - **The switch** (`super::vanilla::TileMenus::dialog`): on, with the game's menu files
//!   read, `DialogMenu` is in the tree while a conversation runs (`Game::dialogue`; a
//!   conversation never sets `Game::menu`) and `super::draw` draws its whole tree at the
//!   hand-built menu's stacking level, the hand-built menu (`crate::dialogue_menu`) not
//!   spawned. Off (the default), none of this runs.
//! - **The rules stay in `crate::dialogue`**: it says what the menu shows (`dialogue::view`)
//!   and which choice is selected. This module only writes that into the tree ([`write`]).
//! - **When**: after the frame's scripts, where the hand-built menu reads the conversation
//!   too, so a line that starts in the frame a choice is made is drawn in that frame; the tree
//!   is then worked out once (`super::update`) and drawn. The tile input of the next frame
//!   reads the tree as it was drawn: a click is on what the player saw [design]. A conversation
//!   that starts this frame builds the menu here, so its first frame is the tiles' too.
//! - **The window**: choices scroll whole (`crate::dialogue_menu::window`, shared with the
//!   hand-built menu). Rows outside it take `listindex` -1, which hides them by the file's own
//!   rule, and the scroll is the height of the rows above the first shown, counted in the
//!   file's `_scroll_delta`. The prefab's own value counts clicks from 0 in this tree, as the
//!   wait and tutorial menus' do, so it is written on every frame.
//! - **Readiness** ([`DialogTiles`], `dialogue-menu.md` §6.5): one decision for the tile
//!   drawing, the hand-built menu and the tile input. A file missing, cut down or restructured,
//!   a template that makes no row with its text, or a text that does not measure gives the
//!   whole hand-built menu: the rows made are unloaded, the reason warned once, and the
//!   preparation tried again next frame, as a font or a file may come.
//! - **The input** ([`follow`]): the tile input feeds the menu while it is drawn, nothing over
//!   it and no pause menu ([`DialogTiles::taking`]), and a pointer's or a script's click comes
//!   back as `crate::dialogue::ViewInput`: a row picks its choice, the pointer coming onto a
//!   row selects it, the click area and the line skip, the scroll bar's arrows and page areas
//!   step the selection. The keys and the wheel stay `crate::dialogue`'s. Each is held to
//!   the list it was read on, and a real press while the actor speaks skips once whichever way
//!   it arrives.
//! - **NVForever's own tiles**: `NVF_GoodbyeHint` and `NVF_SelectionFill`, from templates
//!   written here (`dialogue-menu.md` §6.4). The hint reads its wording and whether it shows
//!   from the same model as the hand-built one.

use super::menus::{self, SizesWritten, Tiled};
use crate::dialogue::{DialogueScreen, ListId, ViewInput};
use crate::dialogue_menu::{crop_x, window, DialogueMenu, MenuView, CROP_Y, GOODBYE_HINT, SHADE};
use bevy::prelude::*;
use fnv_ui::input::{By, Report};
use fnv_ui::{MemSource, Origin, TileId, Ui, Value};

/// The menu's name [observed: the vanilla files, by fnvtool ui tree].
pub const DIALOG_MENU: &str = "DialogMenu";

// The tiles the model writes [observed: the vanilla files, by fnvtool ui tree].
const ROOT: &str = "DialogMenu/NOGLOW_BRANCH";
const BAND: &str = "DialogMenu/NOGLOW_BRANCH/DM_TextBackground";
const CLICK_AREA: &str = "DialogMenu/NOGLOW_BRANCH/DM_ClickRect";
const NAME: &str = "DialogMenu/NOGLOW_BRANCH/DM_SpeakerNameLabel";
const LINE: &str = "DialogMenu/NOGLOW_BRANCH/DM_SpeakerText";
const LIST: &str = "DialogMenu/NOGLOW_BRANCH/DM_TopicList";
const HIGHLIGHT: &str = "DialogMenu/NOGLOW_BRANCH/DM_TopicList/lb_highlight_box";
const BAR: &str = "DialogMenu/NOGLOW_BRANCH/DM_TopicList/lb_scrollbar";
const TOP_BRACKET: &str = "DialogMenu/NOGLOW_BRANCH/DM_TopBracket";
const BOTTOM_BRACKET: &str = "DialogMenu/NOGLOW_BRANCH/DM_BottomBracket";
const FRAME_LINES: [&str; 4] = ["top", "left", "bottom", "right"];
const ROW_TEMPLATE: &str = "DM_TopicTemplate";
const ROW: &str = "DM_Topic";
const ROW_TEXT: &str = "ListItemText";

// NVForever's own tiles, not the file's (`dialogue-menu.md` §6.4).
const FILL_TEMPLATE: &str = "NVF_SelectionFillTemplate";
const HINT_TEMPLATE: &str = "NVF_GoodbyeHintTemplate";
const FILL: &str = "DialogMenu/NOGLOW_BRANCH/DM_TopicList/lb_highlight_box/NVF_SelectionFill";
const HINT: &str = "DialogMenu/NOGLOW_BRANCH/NVF_GoodbyeHint";
const HINT_BOX: &str = "DialogMenu/NOGLOW_BRANCH/NVF_GoodbyeHint/NVF_GoodbyeHintBox";
const HINT_TEXT: &str = "DialogMenu/NOGLOW_BRANCH/NVF_GoodbyeHint/NVF_GoodbyeHintText";
const OWN_TEMPLATES: &str = r#"<template name="NVF_SelectionFillTemplate"><rect name="NVF_SelectionFill">
  <width><copy src="parent()" trait="width"/></width><height><copy src="parent()" trait="height"/></height>
  <red>0</red><green>0</green><blue>0</blue></rect></template>
<template name="NVF_GoodbyeHintTemplate"><rect name="NVF_GoodbyeHint"><depth>3</depth><visible>0</visible>
  <image name="NVF_GoodbyeHintBox"><filename>Interface\Shared\Background\solid_black.dds</filename><zoom>-1</zoom></image>
  <text name="NVF_GoodbyeHintText"><depth>1</depth><font>6</font></text></rect></template>"#;

/// The hint's right end stands this far left of the band and its dark box reaches this far
/// round the text [design: owner list 5 item 62, as the hand-built hint].
const HINT_GAP: f64 = 20.0;
const HINT_PAD: f64 = 10.0;

/// A said-before choice's text: as bright as 128 of 255 [observed: dialogue-menu.md §3].
const DIM_ALPHA: f64 = 128.0;
/// The frame's look, §3's [observed: dialogue-menu.md §3]: on a short check its line reads
/// (100, 18, 16) after the file's line brightness of 200 of 255; its fill.
const SHORT_LINE: [f64; 3] = [127.5, 22.95, 20.4];
const SHORT_FILL: [f64; 3] = [62.0, 0.0, 0.0];
const SHORT_FILL_ALPHA: f64 = 82.0;
const FILL_BRIGHTNESS: f64 = 128.0;
const FILL_ALPHA: f64 = 25.5;

fn set(ui: &mut Ui, tile: TileId, name: &str, v: Value) {
    ui.set_trait(tile, name, v);
}

fn set_at(ui: &mut Ui, path: &str, name: &str, v: Value) {
    if let Some(t) = ui.find_tile(path) {
        ui.set_trait(t, name, v);
    }
}

fn num(ui: &Ui, tile: TileId, name: &str) -> f64 {
    ui.tile(tile).get(name).map_or(0.0, Value::num)
}

fn child(ui: &Ui, tile: TileId, name: &str) -> Option<TileId> {
    ui.tile(tile).children.iter().copied().find(|&c| ui.is_live(c) && ui.tile(c).name.eq_ignore_ascii_case(name))
}

/// The rows the code made under the list, in order.
pub fn rows(ui: &Ui) -> Vec<TileId> {
    let Some(list) = ui.find_tile(LIST) else { return Vec::new() };
    ui.tile(list).children.iter().copied().filter(|&c| ui.is_live(c) && ui.tile(c).origin == Origin::Code && ui.tile(c).name.eq_ignore_ascii_case(ROW)).collect()
}

/// [design] Every fixed path the model writes or the input reads is part of a successful
/// build; a file a mod restructured keeps the hand-built menu.
pub fn built(ui: &Ui) -> Tiled {
    for path in [DIALOG_MENU, ROOT, BAND, CLICK_AREA, NAME, LINE, LIST, HIGHLIGHT, BAR, TOP_BRACKET, BOTTOM_BRACKET] {
        if ui.find_tile(path).is_none() {
            return Tiled::No(format!("{path} missing"));
        }
    }
    Tiled::Yes
}

/// [design] What the file's template makes: a row with its text, under the list. Made once
/// and unloaded again, when the tile list has none yet, as a speaking phase has no rows and
/// the first choices should find no surprise.
fn template_makes_a_row(ui: &mut Ui) -> Tiled {
    if !rows(ui).is_empty() {
        return Tiled::Yes;
    }
    let Some(row) = ui.add_code_tile_from_template(&MemSource::default(), LIST, ROW_TEMPLATE, None) else {
        return Tiled::No(format!("no row from {ROW_TEMPLATE}"));
    };
    let has_text = child(ui, row, ROW_TEXT).is_some();
    let name_fits = ui.tile(row).name.eq_ignore_ascii_case(ROW);
    ui.unload_tile(row);
    if !name_fits {
        Tiled::No(format!("{ROW_TEMPLATE} makes no {ROW}"))
    } else if !has_text {
        Tiled::No(format!("{ROW_TEMPLATE} makes no {ROW_TEXT}"))
    } else {
        Tiled::Yes
    }
}

/// Unload the rows made, after a preparation that could not finish.
fn clean(ui: &mut Ui) {
    for row in rows(ui) {
        ui.unload_tile(row);
    }
}

/// [design] A preparation is finished only with every text it has to place measured.
fn check_measured(ui: &Ui, view: &MenuView) -> Result<(), String> {
    let line = view.line.as_deref().map_or("", str::trim);
    if view.choices.is_empty() && !line.is_empty() && ui.find_tile(LINE).is_none_or(|t| num(ui, t, "height") <= 0.0) {
        return Err(format!("{LINE} has no measure"));
    }
    let made = rows(ui);
    if made.len() != view.choices.len() {
        return Err(format!("{} rows for {} choices", made.len(), view.choices.len()));
    }
    for (&row, choice) in made.iter().zip(&view.choices) {
        let text = child(ui, row, ROW_TEXT).ok_or_else(|| format!("a {ROW} has no {ROW_TEXT}"))?;
        if !choice.label.trim().is_empty() && num(ui, text, "height") <= 0.0 {
            return Err(format!("{ROW_TEXT} has no measure"));
        }
    }
    Ok(())
}

/// Make NVForever's own tiles once the menu is built: their templates, then one of each.
fn own_tiles(ui: &mut Ui) -> bool {
    let src = MemSource::default();
    let Some(menu) = ui.menu(DIALOG_MENU) else { return false };
    if !ui.tile(menu).templates.iter().any(|t| t.name.eq_ignore_ascii_case(HINT_TEMPLATE)) && !ui.inject_text(&src, DIALOG_MENU, OWN_TEMPLATES) {
        return false;
    }
    (ui.find_tile(FILL).is_some() || ui.add_code_tile_from_template(&src, HIGHLIGHT, FILL_TEMPLATE, None).is_some())
        && (ui.find_tile(HINT).is_some() || ui.add_code_tile_from_template(&src, ROOT, HINT_TEMPLATE, None).is_some())
}

/// What the menu is to show this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// The menu's tile: a new one when it is built again.
    pub menu: TileId,
    pub list: ListId,
    pub view: MenuView,
    pub selected: usize,
    /// The model's first choice shown (`DialogueMenu::first`), after [`write`] the window's.
    pub first: usize,
    /// `screen()`'s width and height.
    pub screen: (f64, f64),
}

/// What [`write`] keeps between frames.
#[derive(Default)]
pub struct Written {
    shown: Option<Shown>,
    sizes: SizesWritten,
    /// The scroll, in the file's steps, written again every frame.
    scroll: f64,
    /// The menu whose row template was tried.
    probed: Option<TileId>,
    /// The reasons warned already: once each, as long as the run lasts.
    warned: std::collections::HashSet<String>,
    /// This conversation asked for the menu at once; the frames' own lifetime pass goes on
    /// asking (`ui_loading::sync`, which remembers a file it could not read).
    build_asked: bool,
}

impl Written {
    /// Warn of a reason the menu stays hand-built, once in the run ([`Written::warned`]).
    fn warn(&mut self, status: &Tiled) {
        if let Tiled::No(reason) = status {
            if self.warned.insert(reason.clone()) {
                warn!("{DIALOG_MENU} drawn by hand: {reason}");
            }
        }
    }

    fn forget(&mut self) {
        self.shown = None;
        self.sizes.clear();
        self.scroll = 0.0;
        self.build_asked = false;
    }
}

/// Write the speaker, the line, one row per choice, the phase and NVForever's own tiles'
/// words: what the file leaves to code (`dialogue-menu.md` §6.2). `fresh`: another list than
/// the last written: its rows are made again, so a click still queued on the last list's rows
/// (a script's ClickMenuTile, reported at the next input step) finds its tile gone.
fn write_view(ui: &mut Ui, view: &MenuView, fresh: bool) -> Result<(), String> {
    let menu = ui.menu(DIALOG_MENU).ok_or_else(|| format!("{DIALOG_MENU} missing"))?;
    let line = view.line.as_deref().map_or("", str::trim);
    let speaking = view.choices.is_empty();
    let flag = |on: bool| Value::Num(f64::from(u8::from(on)));
    set(ui, menu, "_DialogVisible", flag(true));
    set(ui, menu, "_ShowingText", flag(speaking));
    // No subtitles setting yet: on, but for a line with no text, which leaves the name alone.
    set(ui, menu, "_ShowSubtitles", flag(!speaking || !line.is_empty()));
    set_at(ui, NAME, "string", Value::Str(view.speaker.trim().to_string()));
    set_at(ui, LINE, "string", Value::Str(line.to_string()));
    set_at(ui, BAND, "alpha", Value::Num(f64::from(SHADE) * 255.0));
    set_at(ui, HINT_TEXT, "string", Value::Str(GOODBYE_HINT.to_string()));
    let screen_width = num(ui, ui.screen(), "width");
    let screen = ui.screen();
    set(ui, screen, "cropx", Value::Num(f64::from(crop_x(screen_width as f32))));
    set(ui, screen, "cropy", Value::Num(f64::from(CROP_Y)));
    let mut made = rows(ui);
    if fresh {
        made.drain(..).for_each(|t| ui.unload_tile(t));
    }
    while made.len() > view.choices.len() {
        if let Some(t) = made.pop() {
            ui.unload_tile(t);
        }
    }
    while made.len() < view.choices.len() {
        // The template's includes were spliced in when the file was read (menu-xml.md §2.2).
        match ui.add_code_tile_from_template(&MemSource::default(), LIST, ROW_TEMPLATE, None) {
            Some(t) => made.push(t),
            None => return Err(format!("no row from {ROW_TEMPLATE}")),
        }
    }
    for (i, (&row, choice)) in made.iter().zip(&view.choices).enumerate() {
        set(ui, row, "string", Value::Str(choice.label.trim().to_string()));
        set(ui, row, "listindex", Value::Num(i as f64));
        set(ui, row, "_line_alpha", Value::Num(if choice.darkened { DIM_ALPHA } else { 255.0 }));
    }
    Ok(())
}

/// The frame's colours: red on a check the player falls short of, the file's colour otherwise.
fn write_frame_colour(ui: &mut Ui, short: bool) {
    for line in FRAME_LINES {
        let Some(t) = ui.find_tile(&format!("{HIGHLIGHT}/{line}")) else { continue };
        set(ui, t, "systemcolor", Value::Num(if short { 0.0 } else { 1.0 }));
        if short {
            for (name, v) in ["red", "green", "blue"].into_iter().zip(SHORT_LINE) {
                set(ui, t, name, Value::Num(v));
            }
        }
    }
    let Some(fill) = ui.find_tile(FILL) else { return };
    set(ui, fill, "systemcolor", Value::Num(if short { 0.0 } else { 1.0 }));
    for (name, v) in ["red", "green", "blue"].into_iter().zip(SHORT_FILL) {
        set(ui, fill, name, Value::Num(v));
    }
    set(ui, fill, "brightness", Value::Num(if short { 255.0 } else { FILL_BRIGHTNESS }));
    set(ui, fill, "alpha", Value::Num(if short { SHORT_FILL_ALPHA } else { FILL_ALPHA }));
}

/// Set the rows out, one under the other from their measured texts, show the whole choices
/// [`window`] fits, the frame on the selected one and the list's height. Returns the first
/// choice shown and the scroll in the file's steps.
fn place(ui: &mut Ui, view: &MenuView, selected: usize, first: usize) -> (usize, f64) {
    let (Some(menu), Some(list), Some(bar)) = (ui.menu(DIALOG_MENU), ui.find_tile(LIST), ui.find_tile(BAR)) else { return (0, 0.0) };
    let made = rows(ui);
    if made.is_empty() {
        set(ui, list, "_highlight_y", Value::Num(-1.0));
        set(ui, list, "_selected_height", Value::Num(0.0));
        set(ui, bar, "_number_of_items", Value::Num(1.0));
        set(ui, bar, "_number_of_visible_items", Value::Num(1.0));
        return (0, 0.0);
    }
    let delta = num(ui, list, "_scroll_delta").max(1.0);
    let heights: Vec<f64> = made.iter().map(|&r| child(ui, r, ROW_TEXT).map_or(0.0, |t| num(ui, t, "height")) + num(ui, r, "_VerticalSpacing")).collect();
    let sizes: Vec<f32> = heights.iter().map(|&h| h as f32).collect();
    let (first, end) = window(first, selected, &sizes);
    let selected = selected.min(made.len() - 1);
    let mut y = 0.0;
    let mut tops = Vec::with_capacity(made.len());
    for (i, (&row, &height)) in made.iter().zip(&heights).enumerate() {
        tops.push(y);
        set(ui, row, "height", Value::Num(height));
        set(ui, row, "_y", Value::Num(y));
        set(ui, row, "listindex", Value::Num(if (first..end).contains(&i) { i as f64 } else { -1.0 }));
        y += height;
    }
    let total = y;
    let offset = tops[first];
    let shown: f64 = heights[first..end].iter().sum();
    let height = shown.max(num(ui, menu, "_MinListHeight"));
    set(ui, list, "height", Value::Num(height));
    set(ui, list, "_highlight_y", Value::Num(tops[selected] - offset));
    set(ui, list, "_selected_height", Value::Num(heights[selected]));
    set(ui, bar, "_number_of_items", Value::Num(total / delta));
    set(ui, bar, "_number_of_visible_items", Value::Num(height / delta));
    write_frame_colour(ui, view.choices.get(selected).is_some_and(|c| c.short));
    (first, offset / delta)
}

/// Place the Tab hint from the band and the bottom bracket as the file worked them out, left
/// of the band where the screen has room, else at the safe zone's edge. Shown while a Goodbye
/// choice is offered [design: dialogue-menu.md §6.4].
fn place_hint(ui: &mut Ui, view: &MenuView) {
    let Some(hint) = ui.find_tile(HINT) else { return };
    let show = view.goodbye && !view.choices.is_empty();
    set(ui, hint, "visible", Value::Num(f64::from(u8::from(show))));
    let (Some(text), Some(pad), Some(band), Some(bottom), true) = (ui.find_tile(HINT_TEXT), ui.find_tile(HINT_BOX), ui.get_float(&format!("{BAND}/x")), ui.get_float(&format!("{BOTTOM_BRACKET}/y")), show) else { return };
    let (width, height) = (num(ui, text, "width"), num(ui, text, "height"));
    let screen = num(ui, ui.screen(), "width");
    set(ui, hint, "x", Value::Num((band - HINT_GAP - width).max(f64::from(crop_x(screen as f32)) + HINT_PAD)));
    set(ui, hint, "y", Value::Num(bottom - height / 2.0));
    set(ui, pad, "x", Value::Num(-HINT_PAD));
    set(ui, pad, "y", Value::Num(-HINT_PAD / 2.0));
    set(ui, pad, "width", Value::Num(width + 2.0 * HINT_PAD));
    set(ui, pad, "height", Value::Num(height + HINT_PAD));
    set(ui, pad, "alpha", Value::Num(f64::from(SHADE) * 255.0));
}

/// The scroll, on every frame the menu is tiled: the prefab counts clicks from 0 in this
/// tree, so its value and the rows it moves would drift from the model's otherwise, and pokes
/// `_SetInCode`, as the prefab asks of code that sets the value [observed: the vanilla files,
/// by fnvtool: its comment].
fn write_scroll(ui: &mut Ui, scroll: f64) {
    let Some(bar) = ui.find_tile(BAR) else { return };
    set(ui, bar, "_current_value", Value::Num(scroll));
    set(ui, bar, "_SetInCode", Value::Num(1.0));
}

/// One frame of the menu shown, `round` being one round of measuring: the fonts as the tree
/// stands then, the sizes of the texts it is given written, how many were. The view when it
/// changed, the texts' sizes every frame, and the places when either moved. Returns the first
/// choice shown, for the model.
pub fn write(ui: &mut Ui, now: &Shown, written: &mut Written, round: &mut dyn FnMut(&mut Ui, &[TileId], &mut SizesWritten) -> usize) -> Result<usize, String> {
    let changed = written.shown.as_ref() != Some(now);
    if changed {
        if written.probed != Some(now.menu) {
            if let Tiled::No(reason) = template_makes_a_row(ui) {
                return Err(reason);
            }
            written.probed = Some(now.menu);
        }
        let same_list = written.shown.as_ref().is_some_and(|s| s.menu == now.menu && s.list == now.list);
        write_view(ui, &now.view, !same_list)?;
        ui.update();
    }
    let texts = menus::texts(ui, now.menu);
    let measured = round(ui, &texts, &mut written.sizes);
    let mut first = now.first;
    if changed || measured > 0 {
        ui.update();
        check_measured(ui, &now.view)?;
        (first, written.scroll) = place(ui, &now.view, now.selected, now.first);
        ui.update();
        place_hint(ui, &now.view);
    }
    write_scroll(ui, written.scroll);
    fnv_ui::input::mark_clip_windows(ui, now.menu);
    written.shown = Some(Shown { first, ..now.clone() });
    Ok(first)
}

/// A line for the log when what the menu shows changes (smoke reads these): the view as the
/// tree worked it out. Call after [`write`].
pub fn describe(ui: &Ui, now: &Shown, first: usize) -> String {
    let n = |x: f64| Value::Num(x).text();
    let f = |path: &str, name: &str| ui.get_float(&format!("{path}/{name}")).unwrap_or(-1.0);
    if now.view.choices.is_empty() {
        let line = now.view.line.as_deref().unwrap_or_default();
        return format!("tiles: {DIALOG_MENU} drawn from tiles: speaker {:?}, line {line:?}, band {}", now.view.speaker, f(BAND, "visible") != 0.0);
    }
    let made = rows(ui);
    let heights: Vec<String> = made.iter().map(|&r| n(num(ui, r, "height"))).collect();
    let shown = made.iter().filter(|&&r| num(ui, r, "listindex") >= 0.0).count();
    let dim = made.iter().filter(|&&r| num(ui, r, "_line_alpha") == DIM_ALPHA).count();
    let short = now.view.choices.get(now.selected).is_some_and(|c| c.short);
    format!(
        "tiles: {DIALOG_MENU} drawn from tiles: speaker {:?}, {} choices, showing {shown} from {first}, selected {}, row heights [{}], list y {} height {}, scroll {} of {}, bar {}, dim {dim}, frame {}, hint {}",
        now.view.speaker,
        made.len(),
        now.selected,
        heights.join(", "),
        n(f(LIST, "y")),
        n(f(LIST, "height")),
        n(f(BAR, "_current_value")),
        n(f(BAR, "_number_of_items")),
        f(BAR, "visible") != 0.0,
        if short { "red" } else { "plain" },
        f(HINT, "visible") != 0.0,
    )
}

/// The keys `crate::dialogue::input` acts on itself, by the tile input's names: W, S and the
/// arrows (a step), E and the controller's A (Activate). Any other key a file binds is the
/// file's (`super::menus::key_click_acts`).
pub const HAND_BUILT_KEYS: [&str; 6] = ["W", "S", "E", "xup", "xdown", "xbuttona"];

/// What the menu's tiles stand for, by name [observed: the vanilla files, by fnvtool ui tree].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogTile {
    /// A row shown, the choice's number.
    Choice(usize),
    /// The click area or the line.
    Skip,
    /// A scroll bar's arrow or page area: choices to move the selection by. A page is the
    /// choices shown, so a step always lands on a choice [design].
    Step(i32),
}

/// What `tile` stands for, by its name or a parent's. A row hidden (out of the window) is
/// nothing, and neither is the scroll bar's marker: dragging it is not built [design].
pub fn dialog_tile(ui: &Ui, tile: TileId) -> Option<DialogTile> {
    let page = || rows(ui).iter().filter(|&&r| num(ui, r, "listindex") >= 0.0).count().max(1) as i32;
    let mut at = Some(tile);
    while let Some(t) = at {
        let tile = ui.tile(t);
        match tile.name.to_ascii_lowercase().as_str() {
            "dm_topic" if tile.origin == Origin::Code => {
                let i = num(ui, t, "listindex");
                return (i >= 0.0).then_some(DialogTile::Choice(i as usize));
            }
            "dm_clickrect" | "dm_speakertext" => return Some(DialogTile::Skip),
            "scrollbar_vert_up" => return Some(DialogTile::Step(-1)),
            "scrollbar_vert_down" => return Some(DialogTile::Step(1)),
            "scrollbar_vert_page_up" => return Some(DialogTile::Step(-page())),
            "scrollbar_vert_page_down" => return Some(DialogTile::Step(page())),
            "scrollbar_vert_marker" => return None,
            _ => {}
        }
        at = tile.parent;
    }
    None
}

/// Turn a frame's report on the menu's tiles into what the view asks of the conversation:
/// see the module notes. `physical`: the keys from the keyboard (or a controller standing in
/// for them) this frame; `list`: what the rows were written for.
pub fn follow(ui: &Ui, report: &Report, physical: &[&str], menu: TileId, list: Option<&ListId>, asked: &mut ViewInput) {
    asked.list = list.cloned();
    let mine = |t: TileId| ui.is_live(t) && ui.root_of(t) == menu;
    // A highlight a file's ref moved for a hand-built Up or Down is not the pointer's: the
    // selection moves once, by the key.
    let by_key = physical.iter().any(|k| k.eq_ignore_ascii_case("xup") || k.eq_ignore_ascii_case("xdown"));
    if let (false, Some((_, Some(to)))) = (by_key, report.over) {
        if let Some(DialogTile::Choice(i)) = Some(to).filter(|&t| mine(t)).and_then(|t| dialog_tile(ui, t)) {
            asked.hover = Some(i);
        }
    }
    for c in report.clicks.iter().filter(|c| mine(c.tile)) {
        if let By::Key(k) = &c.by {
            if !menus::key_click_acts(DIALOG_MENU, k, physical) {
                continue;
            }
        }
        match dialog_tile(ui, c.tile) {
            // The first in a frame, as one act a frame is what `crate::dialogue` takes.
            Some(DialogTile::Choice(i)) if asked.pick.is_none() => {
                info!("tiles: {DIALOG_MENU} choice {i} picked on its tiles");
                asked.pick = Some(i);
            }
            Some(DialogTile::Skip) if !asked.skip => {
                info!("tiles: {DIALOG_MENU} line skipped on its tiles");
                asked.skip = true;
            }
            Some(DialogTile::Step(n)) => {
                info!("tiles: {DIALOG_MENU} selection stepped {n} on its tiles");
                asked.step += n;
            }
            _ => {}
        }
    }
}

/// What decides how the menu is drawn this frame, worked out once by [`write_system`] after
/// the scripts, for the tile drawing, the hand-built menu and the next frame's tile input
/// alike: they cannot disagree, and what takes a click is what was on screen.
#[derive(Resource, Debug, Default, PartialEq)]
pub struct DialogTiles {
    /// The menu, while it is drawn from its tiles.
    menu: Option<TileId>,
    /// Nothing over it: a menu over the conversation, or the character creator, hides it as
    /// they hide the hand-built one.
    shown: bool,
    /// The list its rows were written for.
    list: Option<ListId>,
}

impl DialogTiles {
    /// The hand-built menu gives way.
    pub fn by_tiles(&self) -> bool {
        self.menu.is_some()
    }

    /// The tile renderer draws the menu.
    pub fn drawn(&self) -> bool {
        self.menu.is_some() && self.shown
    }

    /// The menu, when it takes the tile input this frame: drawn last frame, the conversation
    /// still running with nothing over it, and the pause menu not up. A menu over the
    /// conversation keeps the input until it is gone, its closing frame included: `Game::menu`
    /// goes in that frame's tick, after this.
    pub fn taking(&self, game: &fnv_game::Game<'_>, paused: bool) -> Option<TileId> {
        self.menu.filter(|_| self.shown && !paused && game.dialogue.is_some() && !crate::dialogue::held(game))
    }

    /// The list the rows were written for.
    pub fn list(&self) -> Option<&ListId> {
        self.list.as_ref()
    }
}

/// Worked out after the frame's scripts: the switch, a conversation, its menu built, and what
/// the model shows written into it ([`write`]); [`DialogTiles`] says the result.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_system(
    mut script: ResMut<crate::script::ScriptState>,
    switch: Res<super::vanilla::TileMenus>,
    files: super::MenuFiles,
    sim: Res<crate::sim::SimState>,
    mut fonts: super::draw::FontKit,
    (screen, mut model): (Option<Res<DialogueScreen>>, Option<ResMut<DialogueMenu>>),
    creator: Option<Res<crate::creator::CreatorMenu>>,
    mut state: ResMut<DialogTiles>,
    mut written: Local<Written>,
) {
    if !switch.dialog {
        return;
    }
    let game = script.rt.as_mut().map(|rt| &mut rt.game).filter(|g| g.dialogue.is_some() && g.ui_files.is_some());
    let Some(game) = game else {
        written.forget();
        state.set_if_neq(DialogTiles::default());
        return;
    };
    // A conversation that started this frame: its menu now, so no frame is drawn by hand.
    if game.ui_tree.menu(DIALOG_MENU).is_none() && !written.build_asked {
        written.build_asked = true;
        files.write(game);
        game.ui_tree.update();
    }
    // One reason for the whole menu to stay hand-built, or none.
    let status = match built(&game.ui_tree) {
        Tiled::Yes if !own_tiles(&mut game.ui_tree) => Tiled::No("NVForever's own tiles were not made".into()),
        status => status,
    };
    let menu = game.ui_tree.menu(DIALOG_MENU);
    let (Some(menu), Tiled::Yes) = (menu, status.clone()) else {
        written.warn(&status);
        clean(&mut game.ui_tree);
        written.shown = None;
        state.set_if_neq(DialogTiles::default());
        return;
    };
    let creating = creator.is_some_and(|c| c.is_open());
    let (view, list) = (crate::dialogue::view(game, &sim).filter(|_| !creating), crate::dialogue::list_id(game));
    let (Some(view), Some(list)) = (view, list) else {
        set_at(&mut game.ui_tree, DIALOG_MENU, "_DialogVisible", Value::Num(0.0));
        written.shown = None;
        state.set_if_neq(DialogTiles { menu: Some(menu), shown: false, list: None });
        return;
    };
    let ui = &mut game.ui_tree;
    let now = Shown {
        menu,
        list: list.clone(),
        view,
        selected: screen.as_ref().map_or(0, |s| s.selected()),
        first: model.as_ref().map_or(0, |m| m.first),
        screen: (num(ui, ui.screen(), "width"), num(ui, ui.screen(), "height")),
    };
    let changed = written.shown.as_ref() != Some(&now);
    let mut round = |ui: &mut Ui, texts: &[TileId], sizes: &mut SizesWritten| {
        let size = fonts.menu_measure(&menus::fonts_of(ui, texts));
        menus::write_text_sizes_of(ui, texts, sizes, &size)
    };
    match write(ui, &now, &mut written, &mut round) {
        Ok(first) => {
            if let Some(m) = model.as_mut().filter(|m| m.first != first) {
                m.first = first;
            }
            if changed {
                info!("{}", describe(ui, &now, first));
            }
            state.set_if_neq(DialogTiles { menu: Some(menu), shown: true, list: Some(list) });
        }
        Err(reason) => {
            written.warn(&Tiled::No(reason));
            written.shown = None;
            clean(ui);
            state.set_if_neq(DialogTiles::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialogue_menu::ChoiceRow;
    use fnv_ui::input::{no_measure, Input, Pointer};

    /// [design] A fixture written for these tests: only the tile and trait names the code looks
    /// up are the file's; the structure, numbers and expressions are this test's own (no menu
    /// file's). So nothing here proves the real file's expressions work with the model: the
    /// smoke scenarios `tiles-dialog-*` do, on the player's archives.
    fn fixture() -> &'static str {
        r#"<menu name="DialogMenu"><_MinListHeight>70</_MinListHeight>
<rect name="NOGLOW_BRANCH"><systemcolor>1</systemcolor>
  <image name="DM_TextBackground"><filename>p.dds</filename><x>40</x><y>400</y><width>800</width><height>100</height></image>
  <hotrect name="DM_ClickRect"><target><copy>0</copy><or src="io()" trait="_ShowingText"/></target><width>900</width><height>900</height></hotrect>
  <text name="DM_SpeakerNameLabel"><font>7</font><x>500</x><y>10</y></text>
  <text name="DM_SpeakerText"><font>6</font><wrapwidth>400</wrapwidth><x>60</x><y>410</y></text>
  <hotrect name="DM_TopicList"><x>100</x><y>300</y><width>700</width><height>50</height><alpha>0</alpha><clipwindow>1</clipwindow><_scroll_delta>4</_scroll_delta>
    <hotrect name="lb_highlight_box"><width>600</width><x>30</x>
      <y><copy src="parent()" trait="_highlight_y"/></y><height><copy src="parent()" trait="_selected_height"/></height>
      <image name="top"><filename>l.dds</filename><width>600</width><height>2</height></image>
      <image name="left"><filename>l.dds</filename><width>2</width><height>9</height></image>
      <image name="bottom"><filename>l.dds</filename><width>600</width><height>2</height></image>
      <image name="right"><filename>l.dds</filename><width>2</width><height>9</height></image>
    </hotrect>
    <image name="lb_scrollbar"><width>30</width><height>40</height>
      <visible><copy src="me()" trait="_number_of_visible_items"/><lt src="me()" trait="_number_of_items"/></visible>
      <_current_value><copy>0</copy><add src="child(scrollbar_vert_down)" trait="clicked"/></_current_value>
      <hotrect name="scrollbar_vert_page_up"><target>1</target><width>30</width><height>5</height></hotrect>
      <hotrect name="scrollbar_vert_page_down"><target>1</target><y>5</y><width>30</width><height>5</height></hotrect>
      <image name="scrollbar_vert_marker"><filename>m.dds</filename><target>1</target><y>10</y><width>30</width><height>5</height></image>
      <image name="scrollbar_vert_up"><filename>u.dds</filename><target>1</target><y>15</y><width>30</width><height>5</height></image>
      <image name="scrollbar_vert_down"><filename>d.dds</filename><target>1</target><y>20</y><width>30</width><height>5</height></image>
    </image>
  </hotrect>
  <rect name="DM_TopBracket"><y>280</y></rect>
  <rect name="DM_BottomBracket"><y><copy src="sibling(DM_TopicList)" trait="height"/><add>310</add></y></rect>
</rect>
<template name="DM_TopicTemplate"><hotrect name="DM_Topic"><x>30</x><width>600</width><target>1</target><clips>1</clips><_VerticalSpacing>12</_VerticalSpacing>
  <_words><copy src="me()" trait="string"/></_words>
  <y><copy src="me()" trait="_y"/><sub><copy src="sibling(lb_scrollbar)" trait="_current_value"/><mul src="parent()" trait="_scroll_delta"/></sub></y>
  <visible><copy src="me()" trait="listindex"/><add>1</add><gt>0</gt></visible>
  <text name="ListItemText"><font>6</font><wrapwidth>200</wrapwidth><string><copy src="parent()" trait="_words"/></string>
    <alpha><copy src="parent()" trait="_line_alpha"/><mul>1</mul></alpha></text>
</hotrect></template></menu>"#
    }

    fn menu_changed(change: impl FnOnce(&str) -> String) -> Ui {
        let src = MemSource::default().with("d.xml", &change(fixture()));
        let mut ui = Ui::new();
        ui.load_menu(&src, "d.xml").unwrap();
        ui.update();
        super::super::write_screen(&mut ui, 1280.0, 720.0, true);
        ui.update();
        ui
    }

    /// Prepared as `write_system` prepares it.
    fn menu() -> Ui {
        let mut ui = menu_changed(str::to_string);
        assert_eq!(built(&ui), Tiled::Yes);
        assert!(own_tiles(&mut ui));
        ui
    }

    /// Every character 10 wide, a line 40 high, wrapped at the wrap width.
    fn size(_font: f64, text: &str, wrap: Option<f64>) -> (f64, f64) {
        let width = text.chars().count() as f64 * 10.0;
        match wrap {
            Some(w) if width > w => (w, (width / w).ceil() * 40.0),
            _ => (width, 40.0),
        }
    }

    fn round(ui: &mut Ui, texts: &[TileId], sizes: &mut SizesWritten) -> usize {
        menus::write_text_sizes_of(ui, texts, sizes, &size)
    }

    fn row(label: &str) -> ChoiceRow {
        ChoiceRow { label: label.to_string(), ..Default::default() }
    }

    fn choosing(rows: Vec<ChoiceRow>) -> MenuView {
        MenuView { speaker: "Nora".into(), line: None, choices: rows, goodbye: false }
    }

    fn shown(ui: &Ui, view: MenuView, lines: u32, selected: usize, first: usize) -> Shown {
        let topics: Vec<u32> = (0..view.choices.len() as u32).map(|i| 0x100 + i).collect();
        Shown { menu: ui.menu(DIALOG_MENU).unwrap(), list: ListId::for_test(lines, &topics), view, selected, first, screen: (1706.0, 960.0) }
    }

    /// One frame: written, the tree worked out as `super::super::update` does.
    fn frame(ui: &mut Ui, written: &mut Written, now: &Shown) -> usize {
        let first = write(ui, now, written, &mut round).unwrap();
        ui.update();
        first
    }

    fn frame_on(ui: &mut Ui, written: &mut Written, view: MenuView, lines: u32, selected: usize, first: usize) -> usize {
        let now = shown(ui, view, lines, selected, first);
        frame(ui, written, &now)
    }

    fn f(ui: &Ui, path: &str, name: &str) -> f64 {
        ui.get_float(&format!("{path}/{name}")).unwrap()
    }

    /// The tile renderer's own selection of what it paints.
    fn pieces(ui: &Ui) -> Vec<super::super::draw::Piece> {
        let menu = ui.menu(DIALOG_MENU).unwrap();
        let measure = |font: f64, text: &str, wrap: Option<f64>| size(font, text, wrap);
        super::super::draw::pieces(ui, menu, &measure, super::super::draw::Select::Whole { kept: &[], strip: None }, true).0
    }

    /// The text drawn: its string and what shows of it.
    fn texts_drawn(ui: &Ui) -> Vec<(String, f32)> {
        pieces(ui)
            .into_iter()
            .filter_map(|p| match p.look {
                super::super::draw::Look::Text { string, .. } => Some((string, p.colour[3])),
                _ => None,
            })
            .collect()
    }

    fn pictures_drawn(ui: &Ui, name: &str) -> usize {
        pieces(ui).iter().filter(|p| matches!(&p.look, super::super::draw::Look::Picture { name: n, .. } if n == name)).count()
    }

    fn step(ui: &mut Ui, pointer: &mut Pointer, at: (f64, f64), press: bool) -> Report {
        let menu = ui.menu(DIALOG_MENU).unwrap();
        pointer.step(ui, &[menu], &Input { at: Some(at), press, ..Input::default() }, &no_measure)
    }

    fn asked(ui: &Ui, report: &Report, physical: &[&str], lines: u32) -> ViewInput {
        let mut asked = ViewInput::default();
        follow(ui, report, physical, ui.menu(DIALOG_MENU).unwrap(), Some(&ListId::for_test(lines, &[])), &mut asked);
        asked
    }

    #[test]
    fn the_line_is_written_with_its_speaker_and_no_list() {
        let mut ui = menu();
        let mut written = Written::default();
        let view = MenuView { speaker: "Nora".into(), line: Some("Stay a while.".into()), choices: Vec::new(), goodbye: false };
        frame_on(&mut ui, &mut written, view, 1, 0, 0);
        assert_eq!(f(&ui, DIALOG_MENU, "_ShowingText"), 1.0);
        assert_eq!(f(&ui, DIALOG_MENU, "_ShowSubtitles"), 1.0);
        assert_eq!(f(&ui, DIALOG_MENU, "_DialogVisible"), 1.0);
        let drawn = texts_drawn(&ui);
        assert!(drawn.iter().any(|(s, _)| s == "Nora"), "{drawn:?}");
        assert!(drawn.iter().any(|(s, _)| s == "Stay a while."), "{drawn:?}");
        assert!(rows(&ui).is_empty());
        assert_eq!(f(&ui, HINT, "visible"), 0.0);
        // The safe zone the file reads.
        assert_eq!(num(&ui, ui.screen(), "cropy"), 15.0);
        assert!((num(&ui, ui.screen(), "cropx") - 11.25).abs() < 1e-3);
        // A response with no text: the name alone, no band behind nothing.
        let view = MenuView { speaker: "Nora".into(), line: None, choices: Vec::new(), goodbye: false };
        frame_on(&mut ui, &mut written, view, 2, 0, 0);
        assert_eq!(f(&ui, DIALOG_MENU, "_ShowSubtitles"), 0.0);
        assert_eq!(texts_drawn(&ui).iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["Nora"]);
    }

    #[test]
    fn choices_stand_in_rows_of_their_wrapped_height() {
        let mut ui = menu();
        let mut written = Written::default();
        // Wrap at 200: 25 characters take two lines of 40.
        let view = choosing(vec![row("Short one."), row(&"w".repeat(25)), row("Third.")]);
        frame_on(&mut ui, &mut written, view, 1, 1, 0);
        let made = rows(&ui);
        assert_eq!(made.len(), 3);
        let g = |t: TileId, n: &str| ui.tile(t).get(n).map_or(-1.0, Value::num);
        assert_eq!((g(made[0], "height"), g(made[1], "height"), g(made[2], "height")), (52.0, 92.0, 52.0));
        assert_eq!((g(made[0], "_y"), g(made[1], "_y"), g(made[2], "_y")), (0.0, 52.0, 144.0));
        // They fit: the list holds them, the bar stays away, the frame on the selected row.
        assert_eq!(f(&ui, LIST, "height"), 196.0);
        assert_eq!(f(&ui, BAR, "visible"), 0.0);
        assert_eq!((f(&ui, LIST, "_highlight_y"), f(&ui, LIST, "_selected_height")), (52.0, 92.0));
        assert_eq!(f(&ui, HIGHLIGHT, "y"), 52.0);
        // A list that wraps otherwise after a script's change: the row is measured and moved.
        ui.set_trait(made[0], "string", Value::Str("x".repeat(25)));
        ui.update();
        frame_on(&mut ui, &mut written, choosing(vec![row("Short one."), row(&"w".repeat(25)), row("Third.")]), 1, 1, 0);
        assert_eq!(ui.tile(made[0]).get("height").map(Value::num), Some(92.0));
        assert_eq!(ui.tile(made[1]).get("_y").map(Value::num), Some(92.0));
    }

    #[test]
    fn a_short_list_keeps_the_files_least_height() {
        let mut ui = menu();
        frame_on(&mut ui, &mut Written::default(), choosing(vec![row("One.")]), 1, 0, 0);
        // 52 of row, the file's least 70.
        assert_eq!(f(&ui, LIST, "height"), 70.0);
        assert_eq!(f(&ui, DIALOG_MENU, "_MinListHeight"), 70.0);
    }

    #[test]
    fn an_overflowing_list_scrolls_by_whole_choices() {
        let mut ui = menu();
        let mut written = Written::default();
        // Rows of 52: five fit the most the list grows to (290).
        let labels: Vec<ChoiceRow> = (0..8).map(|i| row(&format!("Choice {i}."))).collect();
        let first = frame_on(&mut ui, &mut written, choosing(labels.clone()), 1, 0, 0);
        assert_eq!(first, 0);
        let index = |ui: &Ui| rows(ui).iter().map(|&r| num(ui, r, "listindex") as i32).collect::<Vec<_>>();
        assert_eq!(index(&ui), [0, 1, 2, 3, 4, -1, -1, -1], "hidden whole by the file's rule");
        assert_eq!(f(&ui, LIST, "height"), 260.0);
        assert_eq!(f(&ui, BAR, "visible"), 1.0);
        assert_eq!(f(&ui, BAR, "_current_value"), 0.0);
        // Counted in the file's own steps of 4.
        assert_eq!((f(&ui, BAR, "_number_of_items"), f(&ui, BAR, "_number_of_visible_items")), (104.0, 65.0));
        let drawn = texts_drawn(&ui);
        assert!(drawn.iter().any(|(s, _)| s == "Choice 4.") && !drawn.iter().any(|(s, _)| s == "Choice 5."), "{drawn:?}");
        // Down past the last shown: the window moves one choice, a whole row's height.
        let first = frame_on(&mut ui, &mut written, choosing(labels.clone()), 1, 5, first);
        assert_eq!(first, 1);
        assert_eq!(index(&ui), [-1, 1, 2, 3, 4, 5, -1, -1]);
        assert_eq!(f(&ui, BAR, "_current_value"), 13.0);
        assert_eq!(ui.tile(rows(&ui)[1]).get("y").map(Value::num), Some(0.0), "the first shown stands at the list's top");
        assert_eq!(f(&ui, LIST, "_highlight_y"), 208.0);
        // The end, and back up.
        let first = frame_on(&mut ui, &mut written, choosing(labels.clone()), 1, 7, first);
        assert_eq!(first, 3);
        assert_eq!(f(&ui, BAR, "_current_value") + f(&ui, BAR, "_number_of_visible_items"), f(&ui, BAR, "_number_of_items"), "the bar at its end");
        let first = frame_on(&mut ui, &mut written, choosing(labels), 1, 2, first);
        assert_eq!(first, 2);
        assert_eq!(f(&ui, LIST, "_highlight_y"), 0.0);
    }

    #[test]
    fn one_oversized_choice_stands_alone_and_an_empty_list_has_no_rows() {
        let mut ui = menu();
        let mut written = Written::default();
        // Wrap at 200: 150 characters are eight lines, 332 with the spacing, past the most the
        // list grows to.
        let view = choosing(vec![row(&"o".repeat(150)), row("After.")]);
        let first = frame_on(&mut ui, &mut written, view, 1, 0, 0);
        assert_eq!(first, 0);
        let index = rows(&ui).iter().map(|&r| num(&ui, r, "listindex") as i32).collect::<Vec<_>>();
        assert_eq!(index, [0, -1], "no part of the next");
        assert_eq!(f(&ui, LIST, "height"), 332.0, "the window is what it holds, past the most");
        // The list gone: its rows too.
        let view = MenuView { speaker: "Nora".into(), line: Some("Well.".into()), choices: Vec::new(), goodbye: false };
        frame_on(&mut ui, &mut written, view, 2, 0, 0);
        assert!(rows(&ui).is_empty());
        assert_eq!(f(&ui, LIST, "_selected_height"), 0.0);
    }

    #[test]
    fn the_scroll_outlasts_a_click_counted_by_the_file() {
        // [design] The fixture's value counts clicks from 0; the model's is written every frame.
        let mut ui = menu();
        let mut written = Written::default();
        let labels: Vec<ChoiceRow> = (0..8).map(|i| row(&format!("Choice {i}."))).collect();
        let now = shown(&ui, choosing(labels), 1, 5, 0);
        let first = frame(&mut ui, &mut written, &now);
        let now = Shown { first, ..now };
        assert_eq!(f(&ui, BAR, "_current_value"), 13.0);
        let down = ui.find_tile(&format!("{BAR}/scrollbar_vert_down")).unwrap();
        ui.set_trait(down, "clicked", Value::Num(1.0));
        frame(&mut ui, &mut written, &now);
        assert_eq!(f(&ui, BAR, "_current_value"), 13.0, "the click's frame");
        ui.set_trait(down, "clicked", Value::Num(0.0));
        frame(&mut ui, &mut written, &now);
        assert_eq!(f(&ui, BAR, "_current_value"), 13.0, "the pulse's end");
    }

    #[test]
    fn a_dim_row_and_a_short_checks_frame_are_painted_so() {
        let mut ui = menu();
        let mut written = Written::default();
        let short = ChoiceRow { label: "[Sneak 17/25]  I could.".into(), short: true, ..Default::default() };
        let dim = ChoiceRow { label: "Said before.".into(), darkened: true, ..Default::default() };
        let view = choosing(vec![short, row("Plain."), dim]);
        frame_on(&mut ui, &mut written, view.clone(), 1, 0, 0);
        let alpha = |ui: &Ui, s: &str| texts_drawn(ui).iter().find(|(t, _)| t == s).unwrap().1;
        assert!((alpha(&ui, "Said before.") - 128.0 / 255.0).abs() < 1e-4);
        assert_eq!(alpha(&ui, "Plain."), 1.0);
        // The frame's four lines red, its fill dark red: the short check is selected.
        let lines = |ui: &Ui| pieces(ui).into_iter().filter(|p| matches!(&p.look, super::super::draw::Look::Picture { name, .. } if name == "l.dds")).collect::<Vec<_>>();
        assert_eq!(lines(&ui).len(), 4);
        let [r, g, b, _] = lines(&ui)[0].colour;
        let bright = 255.0;
        assert!((r * bright - 127.5).abs() < 0.5 && (g * bright - 22.95).abs() < 0.5 && (b * bright - 20.4).abs() < 0.5, "{r} {g} {b}");
        let fill = pieces(&ui).into_iter().find(|p| matches!(p.look, super::super::draw::Look::Box)).expect("the fill");
        assert!((fill.colour[0] * 255.0 - 62.0).abs() < 0.5 && (fill.colour[3] * 255.0 - 82.0).abs() < 0.5, "{:?}", fill.colour);
        // Off the check the frame goes back to the interface's colour.
        frame_on(&mut ui, &mut written, view, 1, 1, 0);
        let [r, g, b, _] = lines(&ui)[0].colour;
        assert!(g > 0.2 && (r, g, b) != (0.5, 0.09, 0.08), "the HUD's colour: {r} {g} {b}");
        let fill = pieces(&ui).into_iter().find(|p| matches!(p.look, super::super::draw::Look::Box)).unwrap();
        assert!((fill.colour[3] * 255.0 - 25.5).abs() < 0.5);
    }

    #[test]
    fn the_hint_shows_while_a_goodbye_is_offered_anchored_to_the_files_geometry() {
        let mut ui = menu();
        let mut written = Written::default();
        let mut view = choosing(vec![row("Bye."), row("Stay.")]);
        frame_on(&mut ui, &mut written, view.clone(), 1, 0, 0);
        assert!(!texts_drawn(&ui).iter().any(|(s, _)| s == GOODBYE_HINT));
        assert_eq!(pictures_drawn(&ui, "Interface\\Shared\\Background\\solid_black.dds"), 0);
        view.goodbye = true;
        frame_on(&mut ui, &mut written, view.clone(), 1, 0, 0);
        assert!(texts_drawn(&ui).iter().any(|(s, _)| s == GOODBYE_HINT));
        assert_eq!(pictures_drawn(&ui, "Interface\\Shared\\Background\\solid_black.dds"), 1);
        // Left of the band where it has room? This band is at 40, so the safe zone's edge; and
        // level with the bottom bracket: the list's 104 and the fixture's 310, less half a line.
        let screen = num(&ui, ui.screen(), "width");
        assert!((f(&ui, HINT, "x") - (f64::from(crop_x(screen as f32)) + HINT_PAD)).abs() < 1e-3);
        assert_eq!(f(&ui, HINT, "y"), f(&ui, BOTTOM_BRACKET, "y") - 20.0);
        // A wider gap: right end of the text 20 left of the band.
        ui.set_trait(ui.find_tile(BAND).unwrap(), "x", Value::Num(400.0));
        let moved = MenuView { speaker: "Nora again".into(), ..view };
        frame_on(&mut ui, &mut written, moved, 1, 0, 0);
        assert_eq!(f(&ui, HINT, "x"), 400.0 - HINT_GAP - 120.0);
        // Code's own, and not taken for a mod's tile.
        let menu = ui.menu(DIALOG_MENU).unwrap();
        assert!(ui.mod_added_tiles(menu).is_empty(), "code tiles are not a mod's");
        assert_eq!(ui.tile(ui.find_tile(HINT).unwrap()).origin, Origin::Code);
        // During a line: no hint, whatever the model last said of it.
        let line = MenuView { speaker: "Nora".into(), line: Some("Fine.".into()), choices: Vec::new(), goodbye: false };
        frame_on(&mut ui, &mut written, line, 2, 0, 0);
        assert!(!texts_drawn(&ui).iter().any(|(s, _)| s == GOODBYE_HINT));
    }

    #[test]
    fn the_pointer_selects_and_picks_on_the_rows_as_they_are_drawn() {
        let mut ui = menu();
        let labels: Vec<ChoiceRow> = (0..8).map(|i| row(&format!("Choice {i}."))).collect();
        frame_on(&mut ui, &mut Written::default(), choosing(labels), 1, 5, 0);
        let mut p = Pointer::default();
        // The list at 300, the window from choice 1: rows of 52, so y 330 is choice 1, y 400
        // choice 2.
        let r = step(&mut ui, &mut p, (200.0, 330.0), false);
        assert_eq!(asked(&ui, &r, &[], 1).hover, Some(1));
        let r = step(&mut ui, &mut p, (200.0, 400.0), true);
        let a = asked(&ui, &r, &[], 1);
        assert_eq!((a.hover, a.pick), (Some(2), Some(2)));
        // Under the list's clip: no row (a hidden one takes nothing either).
        let r = step(&mut ui, &mut p, (200.0, 1000.0), true);
        let a = asked(&ui, &r, &[], 1);
        assert_eq!(a.pick, None, "{a:?}");
    }

    #[test]
    fn a_key_the_hand_built_input_took_does_not_act_again() {
        let mut ui = menu();
        frame_on(&mut ui, &mut Written::default(), choosing(vec![row("A."), row("B.")]), 1, 0, 0);
        let bound = rows(&ui)[1];
        for key in ["xbuttona", "E", "S", "xdown"] {
            let click = fnv_ui::input::Click { tile: bound, by: By::Key(key.into()), id: None };
            let r = Report { clicks: vec![click.clone()], ..Report::default() };
            assert_eq!(asked(&ui, &r, &[key], 1).pick, None, "{key} by hand");
            // A script's MenuTapKey is not physical: the file's own binding.
            assert_eq!(asked(&ui, &r, &[], 1).pick, Some(1), "{key} tapped by a script");
        }
        // A highlight moved for a hand-built Down is not the pointer's.
        let r = Report { over: Some((None, Some(bound))), ..Report::default() };
        assert_eq!(asked(&ui, &r, &["xdown"], 1).hover, None);
        assert_eq!(asked(&ui, &r, &[], 1).hover, Some(1));
        // A press on a row and a queued click on another: the first in the frame.
        let first = rows(&ui)[0];
        let click = |tile| fnv_ui::input::Click { tile, by: By::Pointer, id: None };
        let r = Report { clicks: vec![click(first), click(bound)], ..Report::default() };
        assert_eq!(asked(&ui, &r, &[], 1).pick, Some(0));
    }

    #[test]
    fn a_script_click_is_followed_and_a_stale_one_finds_its_row_gone() {
        let mut ui = menu();
        let mut written = Written::default();
        let mut p = Pointer::default();
        frame_on(&mut ui, &mut written, choosing(vec![row("A."), row("B.")]), 1, 0, 0);
        let on_b = rows(&ui)[1];
        p.script_click(&mut ui, on_b);
        let r = step(&mut ui, &mut p, (0.0, 0.0), false);
        assert_eq!(asked(&ui, &r, &[], 1).pick, Some(1), "a script's click, reported at the next step");
        // Queued, then the conversation went on: the rows of the new list are other tiles.
        let stale = rows(&ui)[1];
        p.script_click(&mut ui, stale);
        frame_on(&mut ui, &mut written, choosing(vec![row("C."), row("D.")]), 2, 0, 0);
        assert!(!ui.is_live(stale), "the old list's row went with it");
        let r = step(&mut ui, &mut p, (0.0, 0.0), false);
        assert_eq!(asked(&ui, &r, &[], 2).pick, None);
    }

    #[test]
    fn the_click_area_skips_and_the_bar_steps_by_whole_choices() {
        let mut ui = menu();
        let mut written = Written::default();
        let line = MenuView { speaker: "Nora".into(), line: Some("Well.".into()), choices: Vec::new(), goodbye: false };
        frame_on(&mut ui, &mut written, line, 1, 0, 0);
        let area = ui.find_tile(CLICK_AREA).unwrap();
        assert_eq!(dialog_tile(&ui, area), Some(DialogTile::Skip));
        let click = fnv_ui::input::Click { tile: area, by: By::Pointer, id: None };
        let r = Report { clicks: vec![click.clone(), click], ..Report::default() };
        assert!(asked(&ui, &r, &[], 1).skip, "twice in a frame is one skip");
        // An overflowing list: a page is the choices shown.
        let labels: Vec<ChoiceRow> = (0..8).map(|i| row(&format!("Choice {i}."))).collect();
        frame_on(&mut ui, &mut written, choosing(labels), 2, 0, 0);
        let piece = |ui: &Ui, n: &str| dialog_tile(ui, ui.find_tile(&format!("{BAR}/{n}")).unwrap());
        assert_eq!(piece(&ui, "scrollbar_vert_up"), Some(DialogTile::Step(-1)));
        assert_eq!(piece(&ui, "scrollbar_vert_down"), Some(DialogTile::Step(1)));
        assert_eq!(piece(&ui, "scrollbar_vert_page_up"), Some(DialogTile::Step(-5)));
        assert_eq!(piece(&ui, "scrollbar_vert_page_down"), Some(DialogTile::Step(5)));
        assert_eq!(piece(&ui, "scrollbar_vert_marker"), None);
        // Hidden rows take nothing, even from a script.
        assert_eq!(dialog_tile(&ui, rows(&ui)[7]), None);
        assert_eq!(dialog_tile(&ui, rows(&ui)[1]), Some(DialogTile::Choice(1)));
    }

    #[test]
    fn cut_down_files_keep_the_whole_hand_built_menu() {
        for (name, missing) in [("DM_TextBackground", BAND), ("DM_SpeakerText", LINE), ("DM_TopicList", LIST), ("lb_highlight_box", HIGHLIGHT), ("lb_scrollbar", BAR), ("DM_BottomBracket", BOTTOM_BRACKET), ("DM_ClickRect", CLICK_AREA)] {
            let ui = menu_changed(|xml| xml.replace(&format!(r#"name="{name}""#), r#"name="Modded""#));
            assert_eq!(built(&ui), Tiled::No(format!("{missing} missing")), "{name}");
        }
        // No menu at all.
        assert_eq!(built(&Ui::new()), Tiled::No(format!("{DIALOG_MENU} missing")));
        // A template gone, or making a row with no text: nothing written, nothing left.
        for (change, reason) in [
            (r#"name="DM_TopicTemplate""#, format!("no row from {ROW_TEMPLATE}")),
            (r#"name="ListItemText""#, format!("{ROW_TEMPLATE} makes no {ROW_TEXT}")),
            (r#"name="DM_Topic""#, format!("{ROW_TEMPLATE} makes no {ROW}")),
        ] {
            let mut ui = menu_changed(|xml| xml.replace(change, r#"name="Other""#));
            assert!(own_tiles(&mut ui));
            let now = shown(&ui, choosing(vec![row("A."), row("B.")]), 1, 0, 0);
            assert_eq!(write(&mut ui, &now, &mut Written::default(), &mut round), Err(reason), "{change}");
            assert!(rows(&ui).is_empty(), "{change}");
        }
    }

    #[test]
    fn text_that_does_not_measure_is_not_ready_and_is_tried_again() {
        let mut ui = menu();
        let mut written = Written::default();
        let now = shown(&ui, choosing(vec![row("A."), row("B.")]), 1, 0, 0);
        let mut nothing = |ui: &mut Ui, texts: &[TileId], sizes: &mut SizesWritten| menus::write_text_sizes_of(ui, texts, sizes, &|_: f64, _: &str, _: Option<f64>| (0.0, 0.0));
        assert_eq!(write(&mut ui, &now, &mut written, &mut nothing), Err(format!("{ROW_TEXT} has no measure")));
        // As `write_system` cleans up.
        clean(&mut ui);
        written.shown = None;
        written.sizes.clear();
        assert!(rows(&ui).is_empty());
        // The font came: next frame is whole.
        assert_eq!(write(&mut ui, &now, &mut written, &mut round), Ok(0));
        assert_eq!(rows(&ui).len(), 2);
    }

    #[test]
    fn a_warning_is_given_once_for_each_reason() {
        let mut written = Written::default();
        for _ in 0..3 {
            written.warn(&Tiled::No("a".into()));
            written.warn(&Tiled::Yes);
        }
        assert_eq!(written.warned.len(), 1);
        written.warn(&Tiled::No("b".into()));
        assert_eq!(written.warned.len(), 2);
    }

    #[test]
    fn the_decision_is_one() {
        let none = DialogTiles::default();
        assert!(!none.by_tiles() && !none.drawn());
        let mut ui = menu();
        let menu = ui.menu(DIALOG_MENU);
        let over = DialogTiles { menu, shown: false, list: None };
        assert!(over.by_tiles() && !over.drawn(), "a menu over it: neither view draws");
        let up = DialogTiles { menu, shown: true, list: None };
        assert!(up.by_tiles() && up.drawn());
        // The picture it stands for, as `super::draw` is given it.
        frame_on(&mut ui, &mut Written::default(), choosing(vec![row("A.")]), 1, 0, 0);
        assert_eq!(pictures_drawn(&ui, "m.dds"), 0, "no bar for a list that fits");
        assert!(!pieces(&ui).is_empty());
        assert_eq!(describe(&ui, &shown(&ui, choosing(vec![row("A.")]), 1, 0, 0), 0), "tiles: DialogMenu drawn from tiles: speaker \"Nora\", 1 choices, showing 1 from 0, selected 0, row heights [52], list y 300 height 70, scroll 0 of 13, bar false, dim 0, frame plain, hint false");
    }
}
