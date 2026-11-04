//! The trait menu (1084) drawn from `menus\trait_menu.xml`, behind `[UI] TileMenus=TraitMenu`
//! [design: ui-tile-tree.md §6.2 step 3; the file's contract: chargen.md §4.1]. Off (the
//! default), none of this runs and the hand-built menu (`crate::chargen::lists`) draws.
//!
//! - **The rules stay the hand-built menu's**: `crate::chargen`'s `ListMenu` holds the
//!   `TraitSheet`, and every change goes through its `apply`, with its sounds and logs. This
//!   module never makes a sheet (the constructors use up what scripts asked for): it reads the
//!   open menu ([`Snapshot`]), writes the file's "set in code" traits, and says what a click
//!   stands for ([`TileAct`], queued for `crate::chargen`'s `input` later in the frame).
//! - **Readiness** ([`prepare`], every frame, whatever was written last): the file built with
//!   every path, every row and its children live, and sizes that can be used. Otherwise the rows
//!   made so far go and the hand-built menu stands, its sheet as it was: nothing reopens, and the
//!   next frame tries again, with new rows. What is drawn is one decision ([`drawn`]) for the
//!   hand-built menu, the tiles and the blur: ready, for this very opening, in this very tree.
//! - **Three lifetimes**: an opening (`crate::chargen`'s generation) holds the scrolls and the
//!   highlight, and a fallback does not end it ([`TraitTiles::begin`]); a tree holds the counts
//!   the file gave before code wrote them ([`FileCounts`]), which a fallback must not take for
//!   the engine's; a readiness holds the rows.
//! - **A frame** ([`prepare`], before `super::input::feed`): the tree worked out, what changed
//!   written, the texts measured, the rows placed, the counts, scrolls and highlight written,
//!   the tree worked out again, as `super::container_menu`. The scrolls are written again after
//!   the input's step ([`TraitTiles::rewrite`]): the file's values count clicks from 0.
//! - **The hand-over**: `crate::chargen`'s `draw_menus` takes its menu away on the frame this
//!   is first ready, and the tile input starts on the next, when what stood on screen has gone
//!   ([`super::input`]), so a press acts once, on what the player saw.
//! - **Scrolls**: the list's and the description's are this module's own, no change of the
//!   sheet. The keys' choice brings its row into view (`navigated`), the pointer's does not,
//!   and a bar or the wheel leaves the choice alone. Another row starts its text at the top.
//! - **The chosen row and the highlight are apart**: the menu opens with row 0's picture and text
//!   and no highlight, and the pointer leaving the rows takes the highlight away and leaves the
//!   text [observed: the original's job `trait`, 1280 x 720, chargen.md §4.1]. The sheet's choice
//!   stands all the while, as the hand-built menu's.

use super::layout::{HOTRECT_FILL, OWN_ALPHA};
use super::list::{self, BarPiece, Drag, RowTile, Wanted};
use super::menus::{SizesWritten, Tiled};
use crate::chargen::lists::ListView;
use crate::chargen::TileAct;
use bevy::prelude::{info, warn, Res, ResMut, Resource};
use fnv_game::chargen::TraitSheet;
use fnv_game::game::menus as codes;
use fnv_sim::FormId;
use fnv_ui::{TileId, Ui, Value};
use std::collections::HashSet;

/// The menu's name [observed: the vanilla file, chargen.md §4.1].
pub const TRAIT_MENU: &str = "TraitMenu";

const MAIN: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect";
const BACKGROUND: &str = "TraitMenu/NOGLOW_BRANCH/LUM_Background";
const TITLE: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_HeadlineRect/LUM_Headline_Title";
const LIST: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_PerkList";
const ICON: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionIcon";
const COUNTER: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect/LUM_PointCounter";
const TEXT_BOX: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect/TM_DescriptionBox";
const TEXT_BAR: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect/TM_DescriptionBox/TM_DescriptionScrollbar";
const TEXT: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect/TM_DescriptionBox/TM_DescriptionText";
const RESET: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_ButtonRect/LUM_ResetButton";
const DONE: &str = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_ButtonRect/LUM_ContinueButton";
const TEMPLATE: &str = "LUM_PerkTemplate";
const ROW: &str = "LUM_Perk";
const ROW_TEXT: &str = "ListItemText";
const ROW_MARK: &str = "LUM_Template_ItemMarker";

/// The dark picture's opacity [observed: `LUM_Background` `alpha` 193.80, the original's job
/// `trait`, 1280 x 720; the file's `alpha` copies the shared fill, as the container's].
pub const BACKGROUND_ALPHA: f64 = 193.8;
/// A row's `id` [observed: the active component reads 10 on each row, the original's job `trait`].
const ROW_ID: f64 = 10.0;
/// A trait that cannot be taken: half strength, as the hand-built row [hypothesis: chargen.md §4;
/// the row's `_TextAlpha` allows it, no screenshot shows one].
const FAINT_ALPHA: f64 = 128.0;

fn num(ui: &Ui, path: &str, name: &str) -> f64 {
    ui.find_tile(path).and_then(|t| ui.tile(t).get(name)).map_or(0.0, Value::num)
}

fn set(ui: &mut Ui, path: &str, name: &str, v: Value) {
    if let Some(t) = ui.find_tile(path) {
        ui.set_trait(t, name, v);
    }
}

fn flag(b: bool) -> Value {
    Value::Num(f64::from(u8::from(b)))
}

/// [design] Every path the model writes or the input reads is part of a successful build, and
/// the rows' template with the children written: a file a mod cut down keeps the hand-built
/// menu whole.
pub fn built(ui: &Ui) -> Tiled {
    let mut paths: Vec<String> = [TRAIT_MENU, BACKGROUND, MAIN, TITLE, LIST, ICON, COUNTER, TEXT_BOX, TEXT_BAR, TEXT, RESET, DONE].iter().map(|s| (*s).to_string()).collect();
    paths.push(format!("{LIST}/{}", list::HIGHLIGHT));
    for piece in [list::MARKER, list::UP, list::DOWN, list::PAGE_UP, list::PAGE_DOWN] {
        paths.push(format!("{LIST}/{}/{piece}", list::SCROLLBAR));
        paths.push(format!("{TEXT_BAR}/{piece}"));
    }
    for path in &paths {
        if ui.find_tile(path).is_none() {
            return Tiled::No(format!("{path} missing"));
        }
    }
    let Some(menu) = ui.find_tile(TRAIT_MENU) else { return Tiled::No(format!("{TRAIT_MENU} missing")) };
    let Some(template) = ui.tile(menu).templates.iter().find(|t| t.name.eq_ignore_ascii_case(TEMPLATE)) else {
        return Tiled::No(format!("{TEMPLATE} missing"));
    };
    let row = template.body.iter().find_map(|n| match n {
        fnv_ui::xml::Node::Element(e) if e.attr("name").is_some_and(|n| n.eq_ignore_ascii_case(ROW)) => Some(e),
        _ => None,
    });
    let Some(row) = row else { return Tiled::No(format!("{TEMPLATE}/{ROW} missing")) };
    for (kind, name) in [("text", ROW_TEXT), ("image", ROW_MARK)] {
        if !row.elements().any(|e| e.is(kind) && e.attr("name").is_some_and(|n| n.eq_ignore_ascii_case(name))) {
            return Tiled::No(format!("{TEMPLATE}/{ROW}/{name} missing"));
        }
    }
    Tiled::Yes
}

/// A row as shown.
#[derive(Clone, Debug, PartialEq)]
pub struct RowShown {
    pub key: FormId,
    pub name: String,
    /// The file's `_selected`: the trait is taken. Not the highlight.
    pub taken: bool,
    /// The player does not meet its requirements.
    pub faint: bool,
}

/// One whole state of the menu, as it shows it: a plain snapshot, so tests need no world.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// Which opening of a menu this is (`crate::chargen::ChargenMenus`).
    pub generation: u64,
    pub title: String,
    pub rows: Vec<RowShown>,
    /// The row with the highlight.
    pub selected: Option<usize>,
    pub icon: Option<String>,
    pub info: String,
    pub counter: String,
    pub done: bool,
    pub taken: usize,
    pub most: usize,
    /// `crate::chargen::TraitsOpen::navigated`.
    pub navigated: u32,
}

impl Snapshot {
    /// What the open menu shows: `view` is `ListMenu::view`, `sheet` its sheet.
    pub fn of(view: &ListView, sheet: &TraitSheet, generation: u64, navigated: u32) -> Snapshot {
        let rows: Vec<RowShown> = sheet.traits.iter().zip(&view.rows).map(|(&key, r)| RowShown { key, name: r.text.clone(), taken: r.marked, faint: r.dim }).collect();
        Snapshot {
            generation,
            title: view.title.clone(),
            selected: Some(view.selected).filter(|&s| s < rows.len()),
            rows,
            icon: view.icon.clone(),
            info: view.info.clone(),
            counter: view.counter.clone(),
            done: view.done,
            taken: sheet.chosen.len(),
            most: sheet.max,
            navigated,
        }
    }
}

/// Which scroll a bar moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    List,
    Text,
}

/// What a click on the menu's tiles stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraitTile {
    Row(FormId),
    Reset,
    Done,
    Bar(Scroll, BarPiece),
    /// Another tile of the menu: nothing to do.
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ListScroll {
    /// The scrollbar's value, whole.
    value: f64,
    most: f64,
    /// The highlighted row's top and height before the scroll.
    selected: Option<(f64, f64)>,
    /// What a value moves the rows now.
    scale: f64,
    /// The highlight shows: off when the menu opens, and off the rows.
    highlight: bool,
    /// [`Snapshot::navigated`] the view last followed.
    navigated: Option<u32>,
    /// The rows standing whole in the list from its top, for the keys' page.
    page_rows: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct TextScroll {
    value: f64,
    most: f64,
    /// The row whose text this is: another starts at the top.
    key: Option<FormId>,
}

/// What the file gave a tree before any code wrote it, kept as long as that tree stands.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FileCounts {
    menu: TileId,
    /// The list's `_scroll_delta`, for a list that fits.
    list_step: f64,
    /// The description bar's `_number_of_items`.
    text_items: f64,
}

impl FileCounts {
    fn read(ui: &Ui, menu: TileId) -> FileCounts {
        FileCounts { menu, list_step: num(ui, LIST, "_scroll_delta"), text_items: num(ui, TEXT_BAR, "_number_of_items") }
    }
}

/// The tile menu's state: what the writer, the input and the hand-built menu share.
#[derive(Resource, Default)]
pub struct TraitTiles {
    /// The menu while it is drawn from its tiles: the one decision ([`prepare`]).
    pub menu: Option<TileId>,
    /// Done was pressed, or the pause menu is over it: drawn, taking nothing.
    held: bool,
    /// The opening the scrolls and the highlight belong to ([`TraitTiles::begin`]).
    generation: Option<u64>,
    /// The counts of the tree last prepared.
    file: Option<FileCounts>,
    rows: Vec<RowTile<FormId>>,
    shown: Option<Snapshot>,
    written_in: Option<TileId>,
    sizes: SizesWritten,
    list: ListScroll,
    text: TextScroll,
    drag: Option<(Scroll, Drag)>,
    /// The pointer is over the description or its bar, for the wheel.
    over_text: bool,
    /// For `crate::chargen`'s `input`, with the opening they were made in.
    acts: Vec<(u64, TileAct)>,
    /// What the snapshot was last made from.
    seen: Option<(crate::chargen::lists::ListMenu, u64, u32, bool)>,
    /// What was last logged as drawn.
    drawn: Option<(f64, f64)>,
    told: bool,
    /// [design] Each fallback reason is warned once a session.
    warned: HashSet<String>,
}

/// Write what the menu shows of `s`: the rows, the strings, the picture and the backing. The
/// places are [`settle`]'s, once the texts are measured. False when a row could not be made.
pub fn write(ui: &mut Ui, s: &Snapshot, st: &mut TraitTiles) -> bool {
    let wanted: Vec<Wanted<'_, FormId>> = s.rows.iter().map(|r| Wanted { key: &r.key, name: ROW }).collect();
    if !list::reconcile(ui, LIST, TEMPLATE, &mut st.rows, &wanted) {
        return false;
    }
    for (row, shown) in st.rows.iter().zip(&s.rows) {
        if list::child(ui, row.tile, ROW_TEXT).is_none() || list::child(ui, row.tile, ROW_MARK).is_none() {
            return false;
        }
        ui.set_trait(row.tile, "id", Value::Num(ROW_ID));
        ui.set_trait(row.tile, "string", Value::Str(shown.name.clone()));
        ui.set_trait(row.tile, "_selected", flag(shown.taken));
        ui.set_trait(row.tile, "_TextAlpha", Value::Num(if shown.faint { FAINT_ALPHA } else { 255.0 }));
    }
    // The list's `_enabled` follows its `visible`, which the file leaves to code; without it
    // the rows take no pointer and the highlight stays hidden.
    set(ui, LIST, "visible", flag(true));
    set(ui, TITLE, "string", Value::Str(s.title.clone()));
    set(ui, TEXT, "string", Value::Str(s.info.clone()));
    set(ui, COUNTER, "string", Value::Str(s.counter.clone()));
    set(ui, ICON, "visible", flag(s.icon.is_some()));
    set(ui, ICON, "filename", Value::Str(s.icon.clone().unwrap_or_default()));
    set(ui, TRAIT_MENU, "_CurrPoints", Value::Num(s.taken as f64));
    set(ui, TRAIT_MENU, "_MaxPoints", Value::Num(s.most as f64));
    set(ui, DONE, "target", flag(s.done));
    set(ui, BACKGROUND, "alpha", Value::Num(BACKGROUND_ALPHA));
    true
}

/// How far a text whose top stands at `top` at value 0 scrolls, a value moving it `step`, in a
/// box `page` high: as far as its bar's own count goes (`file_most`), which does not follow the
/// text [observed: 9 items on every row read, the text 207 to 277 high, the original's job
/// `trait`], and further only for a text that count would leave cut off [design: nothing in
/// the original is known to end the scroll sooner or reach further, and a description must be
/// readable to its end].
pub fn text_most(top: f64, height: f64, page: f64, step: f64, file_most: f64) -> f64 {
    let over = top + height - page;
    if over <= 0.0 || step <= 0.0 {
        file_most
    } else {
        file_most.max((over / step - 1e-9).ceil())
    }
}

/// A tiled frame's sizes, places and scrolls (see the module notes). `write_sizes` is one
/// measuring round, on the tree just worked out, so it finds a font computed from what was
/// written.
pub fn settle(ui: &mut Ui, s: &Snapshot, st: &mut TraitTiles, write_sizes: &mut dyn FnMut(&mut Ui, &mut SizesWritten) -> usize) {
    ui.update();
    for _ in 0..3 {
        if write_sizes(ui, &mut st.sizes) == 0 {
            break;
        }
        ui.update();
    }
    if let Some(list_tile) = ui.find_tile(LIST) {
        let heights = list::measure_rows(ui, &st.rows, ROW_TEXT);
        let page = num(ui, LIST, "height");
        let step = st.file.map_or(0.0, |f| f.list_step);
        // The container capture's counts hold here too [observed: 14 rows of 52 in 676, the
        // original's job `trait`: 2 items, `_scroll_delta` 52, the marker 322].
        let counts = list::counts(&heights, page);
        list::write_counts(ui, list_tile, &counts, step);
        let scale = counts.delta.unwrap_or(step);
        st.list.scale = scale;
        st.list.most = counts.most();
        let picked = s.selected.filter(|&k| k < heights.len());
        st.list.selected = picked.map(|k| (heights[..k].iter().sum(), heights[k]));
        if st.list.navigated != Some(s.navigated) {
            // A first frame shows no highlight; the keys' choice does.
            st.list.highlight |= st.list.navigated.is_some();
            if let Some((top, height)) = st.list.selected {
                st.list.value = list::keep_row_shown(st.list.value, top, height, page, scale, counts.most());
            }
            st.list.navigated = Some(s.navigated);
        }
        st.list.value = st.list.value.clamp(0.0, counts.most());
        let mut y = 0.0;
        st.list.page_rows = heights
            .iter()
            .take_while(|h| {
                y += **h;
                y <= page
            })
            .count();
    }
    let key = s.selected.and_then(|k| s.rows.get(k)).map(|r| r.key);
    if st.text.key != key {
        st.text.key = key;
        st.text.value = 0.0;
    }
    if let Some(bar) = ui.find_tile(TEXT_BAR) {
        let step = num(ui, TEXT_BAR, "_ScrollDelta");
        let value = num(ui, TEXT_BAR, "_current_value");
        // The text's place with no scroll, whatever the file adds to it.
        let top = num(ui, TEXT, "y") + value * step;
        let visible = ui.tile(bar).get("_number_of_visible_items").map_or(1.0, Value::num);
        let file_items = st.file.map_or(visible, |f| f.text_items);
        let most = text_most(top, num(ui, TEXT, "height"), num(ui, TEXT_BOX, "height"), step, (file_items - visible).max(0.0));
        st.text.most = most;
        st.text.value = st.text.value.clamp(0.0, most);
        // Left at the file's count unless the text needs more: its marker and arrows stay as the
        // file has them on every row [observed].
        if most + visible != file_items {
            ui.set_trait(bar, "_number_of_items", Value::Num(most + visible));
        } else if ui.tile(bar).get("_number_of_items").is_none_or(|n| n.num() != file_items) {
            ui.set_trait(bar, "_number_of_items", Value::Num(file_items));
        }
    }
    st.rewrite(ui);
    ui.update();
}

/// A length that can be used: a number, more than nothing.
fn room(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

/// [`Tiled`] for what [`settle`] left, read from the tree as it stands, every frame: each row and
/// its children live, room for the list and the text, and text measured [design: a menu with no
/// room or no measure cannot be read, so the hand-built one stays; a row's spacing alone is no
/// measure].
fn usable(ui: &Ui, s: &Snapshot, st: &TraitTiles) -> Tiled {
    if !room(num(ui, LIST, "height")) || !room(num(ui, LIST, "width")) || !room(st.list.scale) {
        return Tiled::No(format!("{LIST} has no room or scroll step"));
    }
    if !room(num(ui, TEXT_BOX, "height")) || !room(num(ui, TEXT_BOX, "width")) || !room(num(ui, TEXT_BAR, "_ScrollDelta")) {
        return Tiled::No(format!("{TEXT_BOX} has no room or scroll step"));
    }
    if st.rows.len() != s.rows.len() {
        return Tiled::No("a row could not be made".into());
    }
    for r in &st.rows {
        if !ui.is_live(r.tile) || list::child(ui, r.tile, ROW_MARK).is_none() {
            return Tiled::No(format!("a row lost its tile or {ROW_MARK}"));
        }
        let Some(text) = list::child(ui, r.tile, ROW_TEXT) else { return Tiled::No(format!("a row lost its {ROW_TEXT}")) };
        if !room(ui.tile(text).get("height").map_or(0.0, Value::num)) || !room(ui.tile(r.tile).get("height").map_or(0.0, Value::num)) {
            return Tiled::No("a row measured no height".into());
        }
    }
    let text = num(ui, TEXT, "height");
    if !text.is_finite() || (!s.info.is_empty() && text <= 0.0) {
        return Tiled::No(format!("{TEXT} measured no height"));
    }
    Tiled::Yes
}

/// [`drawn`]'s decision from what it reads.
fn drawn_by(st: &TraitTiles, opening: Option<u64>, game_menu: Option<u16>, live: Option<TileId>) -> Option<TileId> {
    let menu = st.menu?;
    (opening.is_some() && st.generation == opening && game_menu == Some(codes::TRAITS) && live == Some(menu)).then_some(menu)
}

/// The menu when it is drawn from its tiles now: ready ([`prepare`], this frame), for the opening
/// the hand-built side has (`open`, none when the scripts put another menu in its place), the
/// menu the game has open, in the tree that stands. The hand-built menu, the tiles and the blur
/// all read this, so a menu in its place never goes undrawn.
pub fn drawn(st: &TraitTiles, open: Option<crate::chargen::TraitsOpen<'_>>, game: Option<&fnv_game::Game<'_>>) -> Option<TileId> {
    let game = game.filter(|g| g.ui_files.is_some())?;
    drawn_by(st, open.map(|o| o.generation), game.menu, game.ui_tree.menu(TRAIT_MENU))
}

/// One frame's readiness (see the module notes). `new` is what the menu shows now, when it was
/// read this frame. Whatever is not ready removes what was made; the hand-built menu stands.
pub fn prepare(ui: &mut Ui, new: Option<Snapshot>, st: &mut TraitTiles, write_sizes: &mut dyn FnMut(&mut Ui, &mut SizesWritten) -> usize) -> Tiled {
    let status = built(ui);
    if !status.yes() {
        st.fail(ui);
        return status;
    }
    let Some(menu) = ui.menu(TRAIT_MENU) else {
        st.fail(ui);
        return Tiled::No(format!("{TRAIT_MENU} missing"));
    };
    if st.written_in != Some(menu) {
        // Built again: nothing of the last tree stands.
        st.fail(ui);
        st.written_in = Some(menu);
    }
    if st.file.is_none_or(|f| f.menu != menu) {
        // Before this frame's writes. A tree this drawing wrote to keeps what it read first.
        st.file = Some(FileCounts::read(ui, menu));
    }
    if let Some(s) = new {
        st.begin(s.generation);
        if st.shown.as_ref() != Some(&s) {
            if !write(ui, &s, st) {
                st.fail(ui);
                return Tiled::No("a row could not be made".into());
            }
            st.shown = Some(s);
            st.told = false;
        }
    }
    let Some(s) = st.shown.take() else {
        st.menu = None;
        return Tiled::No("nothing to show yet".into());
    };
    settle(ui, &s, st, write_sizes);
    let status = usable(ui, &s, st);
    st.shown = Some(s);
    if status.yes() {
        st.menu = Some(menu);
    } else {
        st.fail(ui);
    }
    status
}

impl TraitTiles {
    /// A new opening, whatever became of the last: nothing of how it was left shows in this one.
    /// The same opening goes on through a fallback and back, with its scrolls.
    pub fn begin(&mut self, generation: u64) {
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            self.list = ListScroll::default();
            self.text = TextScroll::default();
            self.drag = None;
            self.over_text = false;
            self.acts.clear();
            self.drawn = None;
            self.told = false;
        }
    }

    /// Whether this drawing is of the opening `generation`.
    pub fn is_for(&self, generation: u64) -> bool {
        self.generation == Some(generation)
    }

    /// Nothing of this drawing stays: the rows made go from the tree and the hand-built menu
    /// draws. A later frame builds them again. The opening and the tree go on.
    pub fn fail(&mut self, ui: &mut Ui) {
        for r in std::mem::take(&mut self.rows) {
            if ui.is_live(r.tile) {
                ui.unload_tile(r.tile);
            }
        }
        self.menu = None;
        self.shown = None;
        self.seen = None;
        self.written_in = None;
        self.drag = None;
        self.over_text = false;
        self.acts.clear();
        self.sizes.clear();
        self.drawn = None;
    }

    /// The menu closed: nothing of it stays for the next (the warnings do).
    pub fn close(&mut self) {
        *self = TraitTiles { warned: std::mem::take(&mut self.warned), ..Default::default() };
    }

    /// Whether there is anything to close.
    pub fn has_state(&self) -> bool {
        self.generation.is_some() || self.menu.is_some() || self.written_in.is_some() || self.shown.is_some() || self.file.is_some()
    }

    /// The menu while it takes input: drawn, not closing, not under the pause menu.
    pub fn takes(&self) -> Option<TileId> {
        self.menu.filter(|_| !self.held)
    }

    /// Whether a new snapshot is needed: what it was made from changed, none stands, or the
    /// menu was built again (a tree dropped takes what was written with it).
    fn needs_snapshot(&self, menu: Option<TileId>, from: &(crate::chargen::lists::ListMenu, u64, u32, bool)) -> bool {
        self.shown.is_none() || self.written_in != menu || self.seen.as_ref() != Some(from)
    }

    /// The scrolls and the highlight written from the view: after [`settle`]'s places, and
    /// after the input's step and its clicks ([`super::input::feed`]).
    pub fn rewrite(&self, ui: &mut Ui) {
        if let Some(l) = ui.find_tile(LIST) {
            list::write_value(ui, l, self.list.value);
            list::write_highlight(ui, l, self.list.selected.filter(|_| self.list.highlight), self.list.value);
        }
        if let Some(bar) = ui.find_tile(TEXT_BAR) {
            list::write_bar_value(ui, bar, self.text.value);
        }
    }

    /// Queue an act for `crate::chargen`'s `input`, as of the opening shown.
    fn act(&mut self, act: TileAct) {
        if let Some(g) = self.generation {
            self.acts.push((g, act));
        }
    }

    /// The acts queued for the opening `generation`; those of another are dropped.
    pub fn take_acts(&mut self, generation: u64) -> Vec<TileAct> {
        std::mem::take(&mut self.acts).into_iter().filter(|(g, _)| *g == generation).map(|(_, a)| a).collect()
    }

    /// Nothing queued acts: the menu is closing.
    pub fn drop_acts(&mut self) {
        self.acts.clear();
    }

    /// The whole rows the list shows at once, for the keys' page.
    pub fn page_rows(&self) -> i32 {
        self.list.page_rows.max(1) as i32
    }

    fn scroll_to(&mut self, which: Scroll, value: f64) {
        match which {
            Scroll::List => self.list.value = value.clamp(0.0, self.list.most),
            Scroll::Text => self.text.value = value.clamp(0.0, self.text.most),
        }
    }

    fn value(&self, which: Scroll) -> f64 {
        match which {
            Scroll::List => self.list.value,
            Scroll::Text => self.text.value,
        }
    }

    /// The wheel: the description under the pointer, else the list, by the file's step of one
    /// value a notch [documented: scrollbar_vert.xml's `_wheelmoved`]. Applied on the next
    /// frame's writes.
    pub fn wheel(&mut self, by: f64) {
        let which = if self.over_text { Scroll::Text } else { Scroll::List };
        self.scroll_to(which, self.value(which) + by);
    }

    /// The description's scroll by `by` values (the keys, `crate::chargen`).
    pub fn scroll_text(&mut self, by: f64) {
        self.scroll_to(Scroll::Text, self.text.value + by);
    }

    /// A drag ends with its press or when the menu stops taking input.
    pub fn hold_drag(&mut self, held: bool) {
        let now = self.takes();
        if self.drag.is_some_and(|(_, d)| !list::drag_holds(&d, now, held)) {
            self.drag = None;
        }
    }

    /// What a click on `tile` stands for, while the menu takes input. None otherwise: off, in
    /// the fallback or while it closes a script's queued click on the file's tile acts on
    /// nothing.
    pub fn clicked(&self, ui: &Ui, tile: TileId) -> Option<TraitTile> {
        let menu = self.takes()?;
        if !ui.is_live(tile) || ui.root_of(tile) != menu {
            return None;
        }
        let mut piece = None;
        let mut at = Some(tile);
        while let Some(t) = at {
            if let Some(r) = self.rows.iter().find(|r| r.tile == t) {
                return Some(TraitTile::Row(r.key));
            }
            if piece.is_none() {
                piece = list::bar_piece(ui, t);
            }
            match ui.tile(t).name.to_ascii_lowercase().as_str() {
                "lum_resetbutton" => return Some(TraitTile::Reset),
                "lum_continuebutton" => return Some(TraitTile::Done),
                "lum_perklist" => return Some(piece.map_or(TraitTile::Other, |p| TraitTile::Bar(Scroll::List, p))),
                "tm_descriptionscrollbar" => return Some(piece.map_or(TraitTile::Other, |p| TraitTile::Bar(Scroll::Text, p))),
                _ => {}
            }
            at = ui.tile(t).parent;
        }
        Some(TraitTile::Other)
    }

    /// Whether `key` is the row with the highlight.
    fn is_selected(&self, key: FormId) -> bool {
        self.shown.as_ref().is_some_and(|s| s.selected.and_then(|k| s.rows.get(k)).is_some_and(|r| r.key == key))
    }

    /// Logged when a scroll drawn changes, and when it is not the value written (smoke reads
    /// them): the view's values must outlast a click's pulse, as the container menu's.
    pub fn check_drawn(&mut self, ui: &Ui, log: &mut dyn FnMut(String)) {
        let (Some(l), Some(t)) = (ui.find_tile(LIST), ui.find_tile(TEXT_BAR)) else { return };
        let (Some(lv), Some(tv)) = (list::value_shown(ui, l), ui.tile(t).get("_current_value").map(Value::num)) else { return };
        let n = |x: f64| Value::Num(x).text();
        if self.drawn != Some((lv, tv)) {
            let end = num(ui, TEXT, "y") + num(ui, TEXT, "height");
            log(format!("tiles: {TRAIT_MENU} list scrolled to {} of {}, text scrolled to {} of {}, its end at {} in a box {} high", n(lv), n(self.list.most), n(tv), n(self.text.most), n(end), n(num(ui, TEXT_BOX, "height"))));
            self.drawn = Some((lv, tv));
        }
        if lv != self.list.value {
            log(format!("tiles: {TRAIT_MENU} list drawn at {}, not the {} written", n(lv), n(self.list.value)));
        }
        if tv != self.text.value {
            log(format!("tiles: {TRAIT_MENU} text drawn at {}, not the {} written", n(tv), n(self.text.value)));
        }
    }
}

/// Act on a frame's click (see the module notes). `None` when the click is not for this menu,
/// so the other menus' rules see it. `physical` as `super::menus::key_click_acts`.
pub fn follow_click(st: &mut TraitTiles, ui: &Ui, c: &fnv_ui::input::Click, at: Option<(f64, f64)>, held: bool, physical: &[&str]) -> Option<()> {
    use fnv_ui::input::By;
    let what = st.clicked(ui, c.tile)?;
    if let By::Key(k) = &c.by {
        if !super::menus::key_click_acts(TRAIT_MENU, k, physical) {
            return Some(());
        }
    }
    match what {
        TraitTile::Row(key) => {
            info!("tiles: {TRAIT_MENU} row {key} clicked on its tiles");
            st.act(TileAct::Click(key));
        }
        TraitTile::Reset => {
            info!("tiles: {TRAIT_MENU} Reset on its tiles");
            st.act(TileAct::Reset);
        }
        TraitTile::Done => {
            info!("tiles: {TRAIT_MENU} Done on its tiles");
            st.act(TileAct::Done);
        }
        TraitTile::Bar(which, BarPiece::Marker) => {
            let (owner, bar) = bar_of(ui, which)?;
            let menu = st.takes()?;
            st.drag = at.filter(|_| held && matches!(c.by, By::Pointer)).and_then(|(_, y)| list::marker_grab_on(ui, menu, owner, bar, y)).map(|d| (which, d));
        }
        TraitTile::Bar(which, piece) => {
            let (_, bar) = bar_of(ui, which)?;
            let value = st.value(which);
            // The description's page areas do nothing: the file's `_jump_size` is 0 [observed].
            if let Some(v) = list::after_piece_on(ui, bar, piece, value) {
                info!("tiles: {TRAIT_MENU} {which:?} {piece:?} on its tiles");
                st.scroll_to(which, v);
            }
        }
        TraitTile::Other => {}
    }
    Some(())
}

/// A scroll's tile and its bar.
fn bar_of(ui: &Ui, which: Scroll) -> Option<(TileId, TileId)> {
    match which {
        Scroll::List => {
            let l = ui.find_tile(LIST)?;
            Some((l, list::child(ui, l, list::SCROLLBAR)?))
        }
        Scroll::Text => {
            let b = ui.find_tile(TEXT_BAR)?;
            Some((b, b))
        }
    }
}

/// Whether `at` is over the description's box or its bar beside it. The box takes no pointer, so
/// nothing under the pointer says so; its place is worked out as the layout's, from the tiles'
/// `x` and `y` up the tree.
fn over_text(ui: &Ui, at: (f64, f64)) -> bool {
    let Some(b) = ui.find_tile(TEXT_BOX) else { return false };
    let (mut left, mut top) = (0.0, 0.0);
    let mut up = Some(b);
    while let Some(t) = up {
        left += ui.tile(t).get("x").map_or(0.0, Value::num);
        top += ui.tile(t).get("y").map_or(0.0, Value::num);
        up = ui.tile(t).parent;
    }
    let width = num(ui, TEXT_BOX, "width") + num(ui, TEXT_BAR, "width");
    at.0 >= left && at.0 < left + width && at.1 >= top && at.1 < top + num(ui, TEXT_BOX, "height")
}

/// The pointer's part besides clicks: a drag on a marker, the row or button it came onto, and
/// whether it is over the description, for the wheel. `moved`: it moved this frame, so a list
/// scrolled under a resting pointer leaves the choice alone [as the container menu, owner
/// 2026-10-22].
pub fn follow_pointer(st: &mut TraitTiles, ui: &Ui, report: &fnv_ui::input::Report, at: Option<(f64, f64)>, moved: bool) {
    st.over_text = at.is_some_and(|p| over_text(ui, p));
    if let (Some((which, drag)), Some((_, y))) = (st.drag, at) {
        if let Some(v) = list::value_under(ui, drag, y) {
            st.scroll_to(which, v);
        }
    }
    let Some((_, to)) = report.over else { return };
    // Off the rows the highlight goes and the text stays [observed: `_highlight_y` -1 and 0
    // high, the original's job `trait`].
    let what = to.and_then(|to| st.clicked(ui, to));
    if !matches!(what, Some(TraitTile::Row(_))) {
        st.list.highlight = false;
    }
    match what {
        Some(TraitTile::Row(key)) if st.is_selected(key) => st.list.highlight = true,
        Some(TraitTile::Row(key)) if moved => {
            info!("tiles: {TRAIT_MENU} row {key} chosen by the pointer on its tiles");
            st.list.highlight = true;
            st.act(TileAct::Hover(key));
        }
        Some(TraitTile::Reset | TraitTile::Done) => st.act(TileAct::ButtonFocus),
        _ => {}
    }
}

/// The mark and clip writes that follow what is in the tree, after the scripts and before the
/// drawing: every tile takes only its own alpha, as the container menu's [hypothesis:
/// layout.rs's `OWN_ALPHA`, the same prefabs], the highlight draws its fill, the windows crop,
/// and the description's bar, beside its window, is not cropped by it.
pub fn mark_tiles(ui: &mut Ui) {
    let Some(menu) = ui.menu(TRAIT_MENU) else { return };
    fnv_ui::input::mark_clip_windows(ui, menu);
    let mut stack = vec![menu];
    while let Some(t) = stack.pop() {
        if !ui.is_live(t) {
            continue;
        }
        let tile = ui.tile(t);
        stack.extend(tile.children.iter().copied());
        if !tile.get(OWN_ALPHA).is_some_and(Value::truthy) {
            ui.set_trait(t, OWN_ALPHA, Value::Num(1.0));
        }
    }
    set(ui, &format!("{LIST}/{}", list::HIGHLIGHT), HOTRECT_FILL, Value::Num(1.0));
    set(ui, TEXT_BAR, fnv_ui::input::OUTSIDE_CLIP, Value::Num(1.0));
}

/// The keys the hand-built menu acts on itself (`crate::chargen`'s `input`), by the tile input's
/// names, with the controller buttons standing in for them there (`crate::pad::menu_buttons`,
/// and the shared ones). The file binds R and A and Y [documented]: all stay the hand-built
/// keys, a script's MenuTapKey acting through the file.
pub const HAND_BUILT_KEYS: [&str; 15] = ["A", "R", "W", "S", "xup", "xdown", "xleft", "xright", "xbuttona", "xbuttonx", "xbuttony", "xbuttonlb", "xbuttonrb", "xbuttonlt", "xbuttonrt"];

/// A line for the log when what the menu shows changes (smoke reads these).
pub fn describe(ui: &Ui, s: &Snapshot, st: &TraitTiles) -> String {
    let n = |x: f64| Value::Num(x).text();
    let selected = s.selected.and_then(|k| s.rows.get(k)).map_or("none", |r| r.name.as_str());
    format!(
        "tiles: {TRAIT_MENU} drawn from tiles: \"{}\" {} rows, {} of {} taken, selected \"{selected}\", counter \"{}\", text height {}, box {} high, text scrolls to {}, list scrolls to {} by {}",
        s.title,
        s.rows.len(),
        s.taken,
        s.most,
        s.counter,
        n(num(ui, TEXT, "height")),
        n(num(ui, TEXT_BOX, "height")),
        n(st.text.most),
        n(st.list.most),
        n(st.list.scale),
    )
}

/// The test aid `NVF_PLAY_TEST_TRAIT_TILES=<seconds>:<step>[,…]`, on the world aids' clock
/// (`crate::pause::AidTime`): `missing` (the file reads as not there, from then on), `cut:<tile
/// name>` (the first tile of that name goes from the tree when the menu is built, a file cut
/// down, or one a mod changed while it shows) and `long` (the description reads twice over,
/// past the bar's own count on most rows, so its scroll has an end to reach). Nothing is written to the game's files.
#[derive(Resource, Default)]
pub struct TraitTileAid {
    due: Vec<(f32, AidStep)>,
    missing: bool,
    long: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AidStep {
    Missing,
    Cut(String),
    Long,
}

pub const AID: &str = "NVF_PLAY_TEST_TRAIT_TILES";

/// Read [`TraitTileAid`]'s list.
pub fn parse_aid(s: &str) -> Result<Vec<(f32, AidStep)>, String> {
    crate::test_aid::timed(s)?
        .into_iter()
        .map(|(t, step)| match step.trim().split_once(':') {
            Some(("cut", name)) => Ok((t, AidStep::Cut(name.trim().to_string()))),
            None if step.trim() == "missing" => Ok((t, AidStep::Missing)),
            None if step.trim() == "long" => Ok((t, AidStep::Long)),
            _ => Err(format!("{step:?}: unknown step")),
        })
        .collect()
}

/// The first live tile named `name` under `root`.
fn tile_named(ui: &Ui, root: TileId, name: &str) -> Option<TileId> {
    let mut stack = vec![root];
    while let Some(t) = stack.pop() {
        if ui.is_live(t) {
            if ui.tile(t).name.eq_ignore_ascii_case(name) {
                return Some(t);
            }
            stack.extend(ui.tile(t).children.iter().rev().copied());
        }
    }
    None
}

impl TraitTileAid {
    fn step(&mut self, ui: &mut Ui, now: Option<f32>) {
        let Some(now) = now else { return };
        let menu = ui.menu(TRAIT_MENU);
        self.due.retain(|(t, step)| {
            if *t > now {
                return true;
            }
            match step {
                AidStep::Missing => {
                    info!("test aid: {TRAIT_MENU} file taken as missing");
                    self.missing = true;
                    false
                }
                AidStep::Long => {
                    info!("test aid: {TRAIT_MENU} descriptions long");
                    self.long = true;
                    false
                }
                AidStep::Cut(name) => {
                    // Waits for the menu's tree.
                    let Some(menu) = menu else { return true };
                    if let Some(tile) = tile_named(ui, menu, name) {
                        info!("test aid: {TRAIT_MENU} {name} cut from its tree");
                        ui.unload_tile(tile);
                    }
                    false
                }
            }
        });
    }
}

/// Read the test aid at start.
pub(super) fn aid_from_env() -> TraitTileAid {
    let due = match std::env::var(AID) {
        Ok(v) => parse_aid(&v).unwrap_or_else(|e| {
            warn!("{AID}: {e}");
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    TraitTileAid { due, ..Default::default() }
}

/// Write the menu's traits before the input's step (`super::input::feed`): what changed in the
/// open menu, then its sizes, places and scrolls, every frame it is drawn ([`prepare`]). Off,
/// nothing.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_system(
    mut script: ResMut<crate::script::ScriptState>,
    switch: Res<super::vanilla::TileMenus>,
    chargen: Option<Res<crate::chargen::ChargenMenus>>,
    sim: Option<ResMut<crate::sim::SimState>>,
    paused: Option<Res<crate::pause::Paused>>,
    time: crate::pause::AidTime,
    mut fonts: super::draw::FontKit,
    (mut st, mut aid): (ResMut<TraitTiles>, ResMut<TraitTileAid>),
) {
    if !switch.traits {
        return;
    }
    let open = chargen.as_deref().and_then(crate::chargen::ChargenMenus::traits_open);
    let game_open = script.rt.as_ref().is_some_and(|rt| rt.game.menu == Some(codes::TRAITS) && rt.game.ui_files.is_some());
    let (Some(open), true, Some(mut sim)) = (open, game_open, sim) else {
        if st.has_state() {
            st.close();
        }
        return;
    };
    st.begin(open.generation);
    let Some(rt) = script.rt.as_mut() else { return };
    aid.step(&mut rt.game.ui_tree, time.world_secs());
    let status = if aid.missing { Tiled::No("menus\\trait_menu.xml missing".into()) } else { built(&rt.game.ui_tree) };
    if let Tiled::No(reason) = status {
        if st.warned.insert(reason.clone()) {
            warn!("{TRAIT_MENU} drawn by hand: {reason}");
        }
        st.fail(&mut rt.game.ui_tree);
        return;
    }
    st.held = open.answered || paused.is_some_and(|p| p.0);
    // What the menu shows is read when it changed.
    let from = (open.menu.clone(), open.generation, open.navigated, aid.long);
    let new = if st.needs_snapshot(rt.game.ui_tree.menu(TRAIT_MENU), &from) {
        let view = crate::chargen::with_game(&mut script, &mut sim, |g| open.menu.view(g));
        let sheet = open.menu.trait_sheet();
        let made = view.as_ref().zip(sheet).map(|(v, sheet)| {
            let mut s = Snapshot::of(v, sheet, open.generation, open.navigated);
            if aid.long {
                s.info = [s.info.as_str(); 2].join("\n\n");
            }
            s
        });
        st.seen = Some(from);
        made
    } else {
        None
    };
    let Some(rt) = script.rt.as_mut() else { return };
    let ui = &mut rt.game.ui_tree;
    let Some(menu) = ui.menu(TRAIT_MENU) else { return };
    // Each round's texts and fonts as the tree stands then, with the rows a first write adds.
    let mut write_sizes = |ui: &mut Ui, sizes: &mut SizesWritten| {
        let texts = super::menus::texts(ui, menu);
        let size = fonts.menu_measure(&super::menus::fonts_of(ui, &texts));
        super::menus::write_text_sizes_of(ui, &texts, sizes, &size)
    };
    let status = prepare(ui, new, &mut st, &mut write_sizes);
    if let Tiled::No(reason) = status {
        if st.warned.insert(reason.clone()) {
            warn!("{TRAIT_MENU} drawn by hand: {reason}");
        }
        return;
    }
    mark_tiles(ui);
    if !st.told {
        st.told = true;
        if let Some(s) = st.shown.as_ref() {
            info!("{}", describe(ui, s, &st));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::menus::write_text_sizes_of;
    use super::*;
    use fnv_ui::input::{By, Click};
    use fnv_ui::MemSource;

    /// [design] A small layout written for these tests, with its own structure, numbers and
    /// expressions; it does not come from the game's files and says nothing about them. It keeps
    /// only the tile and trait names the model and input look up, which are the engine's interface
    /// ([`built`]), and plain literals wherever the arithmetic is not what a test is about. Its one
    /// dependency: the text stands 3 less its bar's `_moved`, the bar's step times its value. A
    /// bar's `_own_count` stands for a file's own count of clicks, for the model's writes to beat.
    /// So these tests say how the model drives the interface and give up the real file's geometry,
    /// fonts, wrapped rows, rows moving with the scroll, the highlight's and marks' showing, and
    /// its scroll counts, which the smoke scenarios `tiles-trait-*` read. With [`size`] every row
    /// is 40 high (one line of 30 and 10 of spacing), and the description 30 a line.
    fn menu_changed(change: impl FnOnce(&str) -> String) -> Ui {
        let bar = |name: &str, extra: &str| {
            format!(
                r#"<hotrect name="{name}"><width>10</width><target>&true;</target>{extra}
                  <_number_of_visible_items>1</_number_of_visible_items><_jump_size>0</_jump_size><_step_size>1</_step_size><_number_of_items>3</_number_of_items>
                  <_own_count>0</_own_count>
                  <_current_value><copy src="me()" trait="_own_count"/></_current_value>
                  <image name="scrollbar_vert_up"/><image name="scrollbar_vert_down"/>
                  <image name="scrollbar_vert_marker"><height>8</height><width>4</width><_step_height>5</_step_height></image>
                  <hotrect name="scrollbar_vert_page_up"/><hotrect name="scrollbar_vert_page_down"/></hotrect>"#
            )
        };
        let list_bar = bar("lb_scrollbar", "");
        let text_bar = bar("TM_DescriptionScrollbar", r#"<x>200</x><_ScrollDelta>25</_ScrollDelta><_moved><copy src="me()" trait="_ScrollDelta"/><mul src="me()" trait="_current_value"/></_moved>"#);
        let xml = format!(
            r#"<menu name="TraitMenu">
            <rect name="NOGLOW_BRANCH"><image name="LUM_Background"><alpha>7</alpha></image>
            <rect name="LUM_MainRect"><x>50</x><y>30</y>
              <rect name="LUM_HeadlineRect"><text name="LUM_Headline_Title"/></rect>
              <hotrect name="LUM_PerkList"><y>20</y><height>200</height><width>300</width><_scroll_delta>15</_scroll_delta>
                <hotrect name="lb_highlight_box"/>
                {list_bar}</hotrect>
              <image name="LUM_SelectionIcon"/>
              <rect name="LUM_SelectionInfoRect"><x>400</x><y>100</y>
                <hotrect name="TM_DescriptionBox"><y>10</y><height>100</height><width>200</width><clipwindow>&true;</clipwindow>
                  <_moved><add src="child(TM_DescriptionScrollbar)" trait="_moved"/></_moved>
                  {text_bar}
                  <text name="TM_DescriptionText"><wrapwidth>180</wrapwidth><y><copy>3</copy><sub src="parent()" trait="_moved"/></y></text>
                </hotrect>
                <text name="LUM_PointCounter"/></rect>
              <rect name="LUM_ButtonRect"><y>0</y><x>700</x>
                <hotrect name="LUM_ResetButton"><height>20</height><width>50</width><target>&true;</target></hotrect>
                <hotrect name="LUM_ContinueButton"><height>20</height><width>50</width><target>&true;</target><y>30</y></hotrect>
              </rect></rect></rect>
            <template name="LUM_PerkTemplate"><hotrect name="LUM_Perk"><width>280</width><x>10</x><target>&true;</target><_VerticalSpacing>10</_VerticalSpacing>
              <text name="ListItemText"><string>a row</string></text>
              <image name="LUM_Template_ItemMarker"/>
            </hotrect></template>
            <template name="SpareCounter"><text name="LUM_PointCounter"/></template></menu>"#
        );
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("tm.xml", &change(&xml)), "tm.xml").unwrap();
        ui.update();
        ui
    }

    fn menu() -> Ui {
        menu_changed(str::to_string)
    }

    /// Text heights by the strings: as many lines as `|` splits.
    fn size(font: f64, text: &str, wrap: Option<f64>) -> (f64, f64) {
        list::lines_size(font, text, wrap)
    }

    fn snapshot(rows: usize, selected: usize, info: &str) -> Snapshot {
        Snapshot {
            generation: 1,
            title: "PICK".into(),
            rows: (0..rows as u32).map(|i| RowShown { key: FormId(100 + i), name: format!("Trait {i}"), taken: false, faint: false }).collect(),
            selected: Some(selected),
            icon: Some("x.dds".into()),
            info: info.into(),
            counter: "2 LEFT".into(),
            done: true,
            taken: 0,
            most: 2,
            navigated: 0,
        }
    }

    /// A frame as the writer does it.
    fn frame(ui: &mut Ui, s: &Snapshot, st: &mut TraitTiles) -> Tiled {
        let menu = ui.menu(TRAIT_MENU);
        let mut write_sizes = |ui: &mut Ui, sizes: &mut SizesWritten| match menu {
            Some(m) if ui.is_live(m) => {
                let texts = super::super::menus::texts(ui, m);
                write_text_sizes_of(ui, &texts, sizes, &size)
            }
            _ => 0,
        };
        let status = prepare(ui, Some(s.clone()), st, &mut write_sizes);
        if status.yes() {
            mark_tiles(ui);
        }
        status
    }

    /// `n` lines of description.
    fn lines(n: usize) -> String {
        vec!["line"; n].join("|")
    }

    fn click(tile: TileId, by: By) -> Click {
        Click { tile, id: None, by }
    }

    fn at(ui: &Ui, path: &str) -> f64 {
        ui.get_float(path).unwrap_or(-999.0)
    }

    #[test]
    fn readiness_needs_the_whole_file() {
        let ui = menu();
        assert_eq!(built(&ui), Tiled::Yes);
        assert!(!built(&Ui::new()).yes(), "the file missing");
        for gone in ["TM_DescriptionText", "LUM_PointCounter", "LUM_ResetButton", "scrollbar_vert_marker", "lb_highlight_box", "ListItemText", "LUM_Template_ItemMarker", "LUM_PerkTemplate"] {
            let cut = menu_changed(|xml| xml.replace(&format!(r#"name="{gone}""#), r#"name="Modded""#));
            assert!(!built(&cut).yes(), "{gone} cut");
        }
    }

    #[test]
    fn a_row_not_made_leaves_nothing() {
        // A template gone once the file passed: nothing stays made, and no menu is drawn.
        let mut ui = menu_changed(|xml| xml.replace("LUM_PerkTemplate", "Other"));
        let mut st = TraitTiles::default();
        assert!(!write(&mut ui, &snapshot(2, 0, "x"), &mut st));
        st.fail(&mut ui);
        assert!(st.menu.is_none() && st.rows.is_empty());
        assert!(!frame(&mut ui, &snapshot(2, 0, "x"), &mut st).yes());
        assert!(st.menu.is_none(), "the hand-built menu stays");
    }

    /// The counter cut, as a mod might, and made again from the layout's spare.
    fn cut_counter(ui: &mut Ui) {
        let counter = ui.find_tile(COUNTER).unwrap();
        ui.unload_tile(counter);
    }

    fn restore_counter(ui: &mut Ui) {
        let info = "TraitMenu/NOGLOW_BRANCH/LUM_MainRect/LUM_SelectionInfoRect";
        assert!(ui.add_code_tile_from_template(&MemSource::default(), info, "SpareCounter", Some("LUM_PointCounter")).is_some());
    }

    #[test]
    fn recovery_comes_back_with_the_files_counts_and_the_openings_scrolls() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        // A long text raises the description's count past the layout's 3.
        assert!(frame(&mut ui, &snapshot(10, 0, &lines(10)), &mut st).yes());
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 10.0);
        st.scroll_to(Scroll::List, 2.0);
        // The file loses a tile: the rows go, and the hand-built menu draws.
        cut_counter(&mut ui);
        assert!(!frame(&mut ui, &snapshot(10, 0, "short"), &mut st).yes());
        assert!(st.menu.is_none() && st.rows.is_empty());
        // It comes back: tiles again, the same opening's scroll, and a short text back at the
        // file's count, not the 10 the engine wrote.
        restore_counter(&mut ui);
        assert_eq!(frame(&mut ui, &snapshot(10, 0, "short"), &mut st), Tiled::Yes);
        assert!(st.menu.is_some() && st.rows.len() == 10);
        assert_eq!(st.list.value, 2.0);
        assert_eq!((st.text.most, at(&ui, &format!("{TEXT_BAR}/_number_of_items"))), (2.0, 3.0));
    }

    #[test]
    fn a_new_opening_after_a_fallback_starts_over() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let mut s = snapshot(10, 0, &lines(10));
        assert!(frame(&mut ui, &s, &mut st).yes());
        s.navigated = 1;
        s.selected = Some(8);
        assert!(frame(&mut ui, &s, &mut st).yes());
        st.scroll_text(4.0);
        assert!(st.list.highlight && st.list.value > 0.0);
        cut_counter(&mut ui);
        assert!(!frame(&mut ui, &s, &mut st).yes());
        assert!(st.has_state(), "the fallback ended no opening");
        // The menu opens again, the tree whole.
        restore_counter(&mut ui);
        let next = Snapshot { generation: 2, navigated: 1, selected: Some(0), ..snapshot(10, 0, &lines(10)) };
        assert!(frame(&mut ui, &next, &mut st).yes());
        assert_eq!((st.list.value, st.text.value, st.list.highlight), (0.0, 0.0, false));
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), -1.0, "no highlight as it opens");
        // Closed with the drawing fallen back: closing still clears it.
        cut_counter(&mut ui);
        assert!(!frame(&mut ui, &next, &mut st).yes());
        st.close();
        assert!(!st.has_state());
    }

    #[test]
    fn a_rows_child_going_is_caught_with_the_sheet_unchanged_and_made_again() {
        for child in [ROW_TEXT, ROW_MARK] {
            let mut ui = menu();
            let mut st = TraitTiles::default();
            let s = snapshot(3, 0, "x");
            assert!(frame(&mut ui, &s, &mut st).yes());
            let gone = list::child(&ui, st.rows[1].tile, child).unwrap();
            ui.unload_tile(gone);
            // The same snapshot, so nothing is written: readiness reads the tree.
            assert!(!frame(&mut ui, &s, &mut st).yes(), "{child}");
            assert!(st.menu.is_none());
            assert!(frame(&mut ui, &s, &mut st).yes(), "{child}: the next frame makes the rows again");
            assert_eq!(st.rows.len(), 3);
        }
        // A row gone whole.
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let s = snapshot(3, 0, "x");
        assert!(frame(&mut ui, &s, &mut st).yes());
        ui.unload_tile(st.rows[0].tile);
        assert!(!frame(&mut ui, &s, &mut st).yes());
    }

    #[test]
    fn no_room_means_the_hand_built_menu() {
        for (from, to) in [
            ("<height>200</height><width>300</width>", "<height>0</height><width>300</width>"),
            ("<height>200</height><width>300</width>", "<height>200</height><width>0</width>"),
            ("<y>10</y><height>100</height><width>200</width>", "<y>10</y><height>0</height><width>200</width>"),
            ("<y>10</y><height>100</height><width>200</width>", "<y>10</y><height>100</height><width>0</width>"),
            ("<_ScrollDelta>25</_ScrollDelta>", "<_ScrollDelta>0</_ScrollDelta>"),
        ] {
            let mut ui = menu_changed(|xml| {
                assert!(xml.contains(from), "{from}");
                xml.replace(from, to)
            });
            let mut st = TraitTiles::default();
            assert!(!frame(&mut ui, &snapshot(3, 0, "x"), &mut st).yes(), "{to}");
            assert!(st.menu.is_none() && st.rows.is_empty(), "{to}");
        }
        // Nothing measured: a row's spacing alone is no measure, an empty text may be.
        assert!(!room(f64::NAN) && !room(f64::INFINITY) && !room(0.0) && room(1.0));
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let none = |_: f64, _: &str, _: Option<f64>| (0.0, 0.0);
        let menu_tile = ui.menu(TRAIT_MENU).unwrap();
        let mut write_sizes = |ui: &mut Ui, sizes: &mut SizesWritten| {
            let texts = super::super::menus::texts(ui, menu_tile);
            write_text_sizes_of(ui, &texts, sizes, &none)
        };
        assert!(!prepare(&mut ui, Some(snapshot(3, 0, "x")), &mut st, &mut write_sizes).yes());
    }

    #[test]
    fn the_one_decision_follows_the_menu_open_now() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TRAITS), None), None, "nothing ready");
        st.begin(1);
        assert!(frame(&mut ui, &snapshot(3, 0, "x"), &mut st).yes());
        let m = ui.menu(TRAIT_MENU);
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TRAITS), m), m);
        // Scripts put the tag-skill menu in its place: st.menu still holds this frame's readiness.
        assert_eq!(drawn_by(&st, None, Some(codes::TAG_SKILLS), m), None, "the hand-built side has moved on");
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TAG_SKILLS), m), None, "the game has, before that side saw it");
        assert_eq!(drawn_by(&st, None, Some(codes::TRAITS), m), None, "the sheet is gone");
        // Another opening of the same menu, or a tree built again, wait for their preparation.
        assert_eq!(drawn_by(&st, Some(2), Some(codes::TRAITS), m), None);
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TRAITS), None), None);
        // Readiness gone.
        cut_counter(&mut ui);
        assert!(!frame(&mut ui, &snapshot(3, 0, "x"), &mut st).yes());
        assert_eq!(drawn_by(&st, Some(1), Some(codes::TRAITS), ui.menu(TRAIT_MENU)), None);
    }

    #[test]
    fn a_cut_file_gives_the_menu_back_and_comes_again_when_whole() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let s = snapshot(3, 0, "x");
        assert_eq!(frame(&mut ui, &s, &mut st), Tiled::Yes);
        assert_eq!(st.takes(), ui.menu(TRAIT_MENU));
        let rows: Vec<TileId> = st.rows.iter().map(|r| r.tile).collect();
        st.act(TileAct::Reset);
        let counter = ui.find_tile(COUNTER).unwrap();
        ui.unload_tile(counter);
        assert!(!frame(&mut ui, &s, &mut st).yes());
        assert!(st.menu.is_none() && st.takes().is_none());
        assert!(rows.iter().all(|&r| !ui.is_live(r)), "the rows made go");
        assert!(st.take_acts(1).is_empty(), "nothing queued acts");
        assert!(st.clicked(&ui, ui.menu(TRAIT_MENU).unwrap()).is_none());
        restore_counter(&mut ui);
        assert_eq!(frame(&mut ui, &s, &mut st), Tiled::Yes);
        assert_eq!(st.takes(), ui.menu(TRAIT_MENU), "whole again, it takes input");
        assert!(rows.iter().all(|r| !st.rows.iter().any(|now| now.tile == *r)), "with new rows");
    }

    #[test]
    fn rows_write_taken_and_faint_apart_from_the_highlight() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let mut s = snapshot(3, 1, "x");
        s.rows[0].taken = true;
        s.rows[2].faint = true;
        assert!(frame(&mut ui, &s, &mut st).yes());
        let row = |ui: &Ui, i: usize, name: &str| ui.tile(st.rows[i].tile).get(name).map_or(-1.0, Value::num);
        assert_eq!((row(&ui, 0, "_selected"), row(&ui, 1, "_selected")), (1.0, 0.0), "taken, not chosen");
        assert_eq!((row(&ui, 2, "_TextAlpha"), row(&ui, 0, "_TextAlpha")), (FAINT_ALPHA, 255.0));
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), -1.0, "no highlight as the menu opens");
        assert_eq!(ui.get_string(&format!("{TEXT}/string")).as_deref(), Some("x"), "though the text shows");
        assert_eq!(ui.tile(st.rows[0].tile).name, ROW);
        assert_eq!(ui.tile(st.rows[0].tile).get("id").map(Value::num), Some(ROW_ID));
        st.list.highlight = true;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), 40.0, "the chosen row, the second");
        assert_eq!(at(&ui, &format!("{LIST}/_selected_height")), 40.0);
        assert_eq!(at(&ui, &format!("{TRAIT_MENU}/_MaxPoints")), 2.0);
        assert_eq!(ui.get_string(&format!("{TEXT}/string")).as_deref(), Some("x"));
        // A row keeps its tile through a later change, so a click queued means what it did.
        let tile = st.rows[1].tile;
        s.rows[1].taken = true;
        s.taken = 2;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.rows[1].tile, tile);
    }

    #[test]
    fn the_keys_reveal_their_row_and_the_bar_is_left_where_it_was_put() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        // Ten rows of 40 in a list of 200: five whole, five positions after the first, 40 each.
        let mut s = snapshot(10, 0, "x");
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.list.most, 5.0);
        assert_eq!(st.list.scale, 40.0);
        assert_eq!(st.page_rows(), 5);
        // The pointer chooses a row out of view: nothing moves.
        s.selected = Some(8);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.list.value, 0.0);
        // The keys choose: it comes into view, whole.
        s.navigated = 1;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.list.value, 4.0, "(320 + 40 - 200) / 40, up");
        let (y, h) = (at(&ui, &format!("{LIST}/_highlight_y")), at(&ui, &format!("{LIST}/_selected_height")));
        assert!(y >= 0.0 && y + h <= 200.0, "{y} {h}");
        // The bar moves the list and the next frame leaves it there.
        st.scroll_to(Scroll::List, 3.0);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.list.value, 3.0);
        assert_eq!(at(&ui, &format!("{LIST}/lb_scrollbar/_current_value")), 3.0);
    }

    #[test]
    fn a_long_description_reaches_its_end_and_another_starts_at_the_top() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        // Ten lines of 30 from 3 down in a box of 100: 203 over, 9 values of 25.
        let mut s = snapshot(3, 0, &lines(10));
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.text.most, 9.0);
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 10.0);
        st.scroll_text(100.0);
        st.rewrite(&mut ui);
        ui.update();
        assert_eq!(st.text.value, 9.0, "held at the end");
        let end = at(&ui, &format!("{TEXT}/y")) + at(&ui, &format!("{TEXT}/height"));
        assert!(end <= 100.0, "the last line in the box, at {end}");
        // The same row with a shorter text holds the value in the new range.
        s.info = lines(5);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!((st.text.most, st.text.value), (3.0, 3.0));
        // Another row: from the top.
        s.selected = Some(1);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(st.text.value, 0.0);
        assert_eq!(at(&ui, &format!("{TEXT}/y")), 3.0);
    }

    #[test]
    fn a_short_description_keeps_the_files_count_and_a_longer_one_goes_past_it() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(3, 0, &lines(2)), &mut st).yes());
        assert_eq!(st.text.most, 2.0, "the layout's 3 items, whatever the text");
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 3.0);
        assert!(frame(&mut ui, &snapshot(3, 0, &lines(10)), &mut st).yes());
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 10.0);
        assert!(frame(&mut ui, &snapshot(3, 0, &lines(2)), &mut st).yes());
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_number_of_items")), 3.0, "and back");
    }

    #[test]
    fn off_the_rows_the_highlight_goes_and_the_text_stays() {
        use fnv_ui::input::Report;
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let mut s = snapshot(3, 0, "x");
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert!(!st.list.highlight, "none as the menu opens");
        let (first, second, reset) = (st.rows[0].tile, st.rows[1].tile, ui.find_tile(RESET).unwrap());
        let onto = |to| Report { over: Some((None, to)), ..Report::default() };
        // Onto the chosen row: no act, the highlight shows.
        follow_pointer(&mut st, &ui, &onto(Some(first)), None, true);
        assert!(st.list.highlight && st.take_acts(1).is_empty());
        // Onto another: chosen, once.
        follow_pointer(&mut st, &ui, &onto(Some(second)), None, true);
        assert_eq!(st.take_acts(1), [TileAct::Hover(st.rows[1].key)]);
        // A row moved under a resting pointer chooses nothing.
        follow_pointer(&mut st, &ui, &onto(Some(second)), None, false);
        assert!(st.take_acts(1).is_empty());
        // Onto a button, and off everything: the highlight goes.
        follow_pointer(&mut st, &ui, &onto(Some(reset)), None, true);
        assert!(!st.list.highlight);
        assert_eq!(st.take_acts(1), [TileAct::ButtonFocus]);
        st.list.highlight = true;
        follow_pointer(&mut st, &ui, &onto(None), None, true);
        assert!(!st.list.highlight);
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), -1.0);
        assert_eq!(at(&ui, &format!("{LIST}/_selected_height")), 0.0);
        assert_eq!(ui.get_string(&format!("{TEXT}/string")).as_deref(), Some("x"));
        // The keys bring it back.
        s.navigated = 1;
        assert!(frame(&mut ui, &s, &mut st).yes());
        assert!(st.list.highlight);
        assert_eq!(at(&ui, &format!("{LIST}/_highlight_y")), 0.0);
    }

    #[test]
    fn the_bar_beside_its_window_is_not_cropped() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(1, 0, &lines(10)), &mut st).yes());
        let menu = ui.menu(TRAIT_MENU).unwrap();
        let down = ui.find_tile(&format!("{TEXT_BAR}/scrollbar_vert_down")).unwrap();
        let items = super::super::layout::layout(&ui, menu, &|_, _, _| (0.0, 0.0), true);
        assert_eq!(items.iter().find(|i| i.tile == down).unwrap().clip, None, "the bar stands clear of its window");
        let text = ui.find_tile(TEXT).unwrap();
        assert!(items.iter().find(|i| i.tile == text).unwrap().clip.is_some(), "the text is cropped");
    }

    #[test]
    fn a_click_acts_once_and_only_as_its_input_may() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(3, 0, "x"), &mut st).yes());
        let (reset, done, row) = (ui.find_tile(RESET).unwrap(), ui.find_tile(DONE).unwrap(), st.rows[2].tile);
        let key = st.rows[2].key;
        // The pointer and a script: the same act.
        assert!(follow_click(&mut st, &ui, &click(row, By::Pointer), None, false, &[]).is_some());
        assert!(follow_click(&mut st, &ui, &click(done, By::Script), None, false, &[]).is_some());
        assert_eq!(st.take_acts(1), [TileAct::Click(key), TileAct::Done]);
        // A key the hand-built menu took: the file's click acts on nothing.
        assert!(follow_click(&mut st, &ui, &click(reset, By::Key("R".into())), None, false, &["R"]).is_some());
        assert!(st.take_acts(1).is_empty());
        // The controller standing in for R, and one that is not.
        assert!(follow_click(&mut st, &ui, &click(reset, By::Key("xbuttony".into())), None, false, &["R", "xbuttony"]).is_some());
        assert!(st.take_acts(1).is_empty());
        assert!(follow_click(&mut st, &ui, &click(reset, By::Key("xbuttony".into())), None, false, &[]).is_some());
        // A script's MenuTapKey.
        assert!(follow_click(&mut st, &ui, &click(reset, By::Key("R".into())), None, false, &[]).is_some());
        assert_eq!(st.take_acts(1), [TileAct::Reset, TileAct::Reset]);
        // Acts of another opening are dropped.
        st.act(TileAct::Done);
        assert!(st.take_acts(2).is_empty());
    }

    #[test]
    fn a_closing_menu_takes_nothing_and_a_gone_row_acts_on_nothing() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(2, 0, "x"), &mut st).yes());
        let gone = st.rows[1].tile;
        assert!(frame(&mut ui, &snapshot(1, 0, "x"), &mut st).yes());
        assert!(!ui.is_live(gone));
        assert!(follow_click(&mut st, &ui, &click(gone, By::Pointer), None, false, &[]).is_none(), "a tile of a row gone");
        st.held = true;
        let row = st.rows[0].tile;
        assert_eq!(st.takes(), None);
        assert!(st.menu.is_some(), "still drawn");
        assert!(follow_click(&mut st, &ui, &click(row, By::Pointer), None, false, &[]).is_none());
    }

    #[test]
    fn the_bars_pieces_move_their_own_scroll() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        assert!(frame(&mut ui, &snapshot(10, 0, &lines(10)), &mut st).yes());
        let piece = |ui: &Ui, bar: &str, p: &str| ui.find_tile(&format!("{bar}/{p}")).unwrap();
        let down = piece(&ui, &format!("{LIST}/{}", list::SCROLLBAR), list::DOWN);
        assert!(follow_click(&mut st, &ui, &click(down, By::Pointer), None, false, &[]).is_some());
        assert_eq!((st.list.value, st.text.value), (1.0, 0.0));
        let down = piece(&ui, TEXT_BAR, list::DOWN);
        assert!(follow_click(&mut st, &ui, &click(down, By::Pointer), None, false, &[]).is_some());
        assert_eq!((st.list.value, st.text.value), (1.0, 1.0));
        // The layout's page step is nothing: its page area moves nothing.
        let page = piece(&ui, TEXT_BAR, list::PAGE_DOWN);
        assert!(follow_click(&mut st, &ui, &click(page, By::Pointer), None, false, &[]).is_some());
        assert_eq!(st.text.value, 1.0);
        st.scroll_text(3.0);
        assert_eq!(st.text.value, 4.0);
        // A value written after the click's pulse stays the view's.
        st.rewrite(&mut ui);
        ui.update();
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_current_value")), 4.0);
        let bar = ui.find_tile(TEXT_BAR).unwrap();
        ui.set_trait(bar, "_own_count", Value::Num(1.0));
        st.rewrite(&mut ui);
        ui.update();
        ui.set_trait(bar, "_own_count", Value::Num(0.0));
        st.rewrite(&mut ui);
        ui.update();
        assert_eq!(at(&ui, &format!("{TEXT_BAR}/_current_value")), 4.0);
        // The wheel goes by what is under the pointer.
        st.over_text = true;
        st.wheel(1.0);
        assert_eq!((st.list.value, st.text.value), (1.0, 5.0));
        st.over_text = false;
        st.wheel(1.0);
        assert_eq!((st.list.value, st.text.value), (2.0, 5.0));
        st.wheel(-100.0);
        assert_eq!(st.list.value, 0.0, "held at the top");
    }

    #[test]
    fn a_new_opening_starts_over_and_the_warnings_stay() {
        let mut ui = menu();
        let mut st = TraitTiles::default();
        let s = snapshot(10, 0, &lines(10));
        assert!(frame(&mut ui, &s, &mut st).yes());
        st.scroll_to(Scroll::List, 5.0);
        st.scroll_text(4.0);
        let mut next = s.clone();
        next.generation = 2;
        assert!(frame(&mut ui, &next, &mut st).yes());
        assert_eq!((st.list.value, st.text.value), (0.0, 0.0));
        st.warned.insert("x".into());
        st.close();
        assert!(!st.has_state() && st.warned.contains("x"));
    }

    #[test]
    fn the_pointer_over_the_description_is_worked_out_from_the_tree() {
        let ui = menu();
        // The main rect at 50, 30, the info rect at 400, 100, the box 10 down, 200 x 100, its bar
        // 10 wide beside it.
        assert!(over_text(&ui, (451.0, 141.0)));
        assert!(over_text(&ui, (655.0, 239.0)), "the bar's width counts");
        assert!(!over_text(&ui, (449.0, 141.0)));
        assert!(!over_text(&ui, (451.0, 241.0)));
    }

    #[test]
    fn text_most_reaches_the_end() {
        assert_eq!(text_most(3.0, 300.0, 100.0, 25.0, 0.0), 9.0);
        assert_eq!(text_most(0.0, 100.0, 100.0, 25.0, 0.0), 0.0);
        assert_eq!(text_most(0.0, 101.0, 100.0, 25.0, 0.0), 1.0);
        assert_eq!(text_most(0.0, 900.0, 100.0, 0.0, 2.0), 2.0, "no step: the bar's own");
        assert_eq!(text_most(0.0, 90.0, 100.0, 25.0, 4.0), 4.0, "a text that fits: the bar's own");
        assert_eq!(text_most(0.0, 150.0, 100.0, 25.0, 4.0), 4.0, "its own reaches the end");
        assert_eq!(text_most(0.0, 300.0, 100.0, 25.0, 4.0), 8.0, "past its own only to the end");
    }

    #[test]
    fn the_aid_reads() {
        assert_eq!(parse_aid("1:missing,2:long,3:cut:LUM_PointCounter").unwrap(), vec![(1.0, AidStep::Missing), (2.0, AidStep::Long), (3.0, AidStep::Cut("LUM_PointCounter".into()))]);
        assert!(parse_aid("1:broken").is_err());
    }
}
