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
//! - **NVForever's own tiles**: `NVF_GoodbyeHint` and `NVF_SelectionFill`, from templates
//!   written here (`dialogue-menu.md` §6.4). The hint reads its wording and whether it shows
//!   from the same model as the hand-built one.

use super::menus::{self, SizesWritten, Tiled};
use crate::dialogue::{DialogueScreen, ListId};
use crate::dialogue_menu::{crop_x, window, DialogueMenu, MenuView, CROP_Y, GOODBYE_HINT, SHADE};
use bevy::prelude::*;
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
        if let Tiled::No(reason) = template_makes_a_row(ui) {
            return Err(reason);
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
        "tiles: {DIALOG_MENU} drawn from tiles: speaker {:?}, {} choices, showing {shown} from {first}, selected {}, row heights [{}], list height {}, scroll {} of {}, bar {}, dim {dim}, frame {}, hint {}",
        now.view.speaker,
        made.len(),
        now.selected,
        heights.join(", "),
        n(f(LIST, "height")),
        n(f(BAR, "_current_value")),
        n(f(BAR, "_number_of_items")),
        f(BAR, "visible") != 0.0,
        if short { "red" } else { "plain" },
        f(HINT, "visible") != 0.0,
    )
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
