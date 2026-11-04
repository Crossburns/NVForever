//! The Pip-Boy's S.P.E.C.I.A.L. tab (STATS, tab 1) drawn from `menus\main\stats_menu.xml`,
//! behind `[UI] TileMenus=StatsMenu` [design: ui-tile-tree.md §6.2 step 3; the contract:
//! docs/spec-notes/pipboy.md §5.1.1; issue #1513].
//!
//! - **The one decision** ([`StatsTiles::ready`]): the switch on, STATS' tab 1 showing, the
//!   menu files read, the file's tiles there ([`paths`]), the rows made and a usable measured
//!   layout. Painting (`super::draw`), hand-built suppression (`crate::pipboy::frame::draw`) and
//!   input read it and nothing else, and [`settle`] publishes it each frame before any of them.
//!   Eligible to prepare is not ready to display: a pass that fails leaves the whole hand-built
//!   page, says why once, and is tried again the next frame.
//! - **What the model writes** ([`page`]): values from `Snapshot` and `StatsView`; this module
//!   works no figure out again. Places and sizes are the file's, worked out at run time, from
//!   the text sizes and row heights written here.
//! - **Page content only**: the page paints into the Pip-Boy's image, whose background and
//!   scanlines are the shell's, so the file's `background_layout` stays hidden in both frames.
//! - **The other tabs** stay fully hand-built. The menu and its rows persist across tabs and
//!   reopenings ([`Written`]); a new menu tile (a rebuilt tree) starts over.
//! - **The order in a frame**: [`write`] after the tile input, before the scripts, so they read
//!   this frame's model and the scroll outlasts a click's pulse; then `crate::pipboy::stats::prepare`
//!   (keys, test aid, what is shown) and [`settle`], which writes what they changed, works the
//!   tree out and decides, before `frame::draw` and `super::draw::draw`. So a tab change or a key
//!   is drawn in the frame it happens, from one state.
//! - **Input** ([`feed`], before the scripts): the pointer is the Pip-Boy's own, in canvas units
//!   (`crate::pipboy::PipBoyScreen::pointer`: the device's ray or the flat image's crop), acting
//!   on the page painted last frame, what the player saw, with the pointer found this frame
//!   (`crate::pipboy::PointerSet`). A row under the pointer selects and boxes as the hand-built
//!   list does, not one drawn under a pointer that has not moved; that is the tile under the
//!   pointer itself, not the highlight, which a file's `xup` or `xdown` can move. Such a move
//!   selects only for a key the hand-built keys did not also act on ([`keyed_row`]). A
//!   click on a row selects it; a tab sends `PipBoyInput::Tab`; the description's arrows, page
//!   areas and marker become `StatsView` commands. The value is the model's: written back after
//!   input, and the wheel stays `crate::pipboy::stats::prepare`'s, so nothing acts twice. The
//!   keys stay the hand-built ones ([`HAND_BUILT_KEYS`]): a file's binding for one does not act
//!   for a physical press, a script's click does. The screen menu's header stays hand-built, and
//!   no tile lies under it to take its click; nor do the file's hover sounds play here, so a
//!   tab's is `TabHovered`, once.
//! - **A menu over the page** takes the frame's input ([`modal_over`]), for all of the frame it
//!   closes in as well: its key closed it in the scripts, before the page's keys run.

use super::draw::FontKit;
use super::input::{frame_keys, log_report, Devices};
use super::layout;
use super::menus::{self, SizesWritten};
use super::vanilla::TileMenus;
use crate::pipboy::stats::model::{Row, Snapshot};
use crate::pipboy::stats::{self, Prepared, StatsView};
use crate::pipboy::sounds::TabHovered;
use crate::pipboy::{Frame, Page, PipBoy, PipBoyAid, PipBoyInput, PipBoyScreen, CANVAS};
use crate::script::ScriptState;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use fnv_ui::{TileId, Ui, Value};
use std::collections::{HashMap, HashSet};

/// The menu's name [documented: stats_menu.xml].
pub const STATS_MENU: &str = "StatsMenu";
/// The tab drawn from tiles.
pub const PAGE: usize = 1;
/// Test aid: a tile or template of the file to take as missing, so a smoke scenario sees the
/// fallback of a cut-down file without a changed game file: `<name>`, or `<name>@<seconds>` to
/// stop at that time, for the page's recovery.
pub const CUT_AID: &str = "NVF_PLAY_TEST_STATS_TILE_CUT";

/// The tiles the model writes and the input reads, by name [observed: the vanilla files,
/// docs/spec-notes/pipboy.md §5.1.1]. They are looked for by name under the menu, so a file
/// that nests them otherwise still serves.
const LIST: &str = "stats_special_container";
const ROW_TEMPLATE: &str = "stats_list_template";
const ROW_TEXT: &str = "ListItemText";
const DISPLAY: &str = "stats_display_rect";
const ICON: &str = "stats_icon";
const DESC_RECT: &str = "stats_description_rect";
const DESC: &str = "stats_description";
const BAR: &str = "stats_description_scrollbar";
const TITLE: &str = "stats_title";
const LINE: &str = "stats_headline_h2";
const BACKGROUND: &str = "background_layout";
/// The tabs, by the numbers `crate::pipboy::PipBoyInput::Tab` uses [observed: their ids 0 to 4].
const TABS: [&str; 5] = ["stats_tailline_status", "stats_tailline_SPECIAL", "stats_tailline_skills", "stats_tailline_perks", "stats_tailline_general"];
/// The rect inside a tab that holds its words and box [observed: by fnvtool ui tree].
const TAB_INNER: &str = "stb_text_box";
/// A row's spacing where the row's `_VerticalSpacing` reads 0 [hypothesis: pipboy.md §5.1.1].
const ROW_SPACING: f64 = 20.0;

fn num(ui: &Ui, t: TileId, name: &str) -> f64 {
    ui.tile(t).get(name).map_or(0.0, Value::num)
}

fn flag(on: bool) -> Value {
    Value::Num(f64::from(u8::from(on)))
}

/// The tiles found, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paths {
    pub menu: TileId,
    pub list: TileId,
    pub icon: TileId,
    pub desc_rect: TileId,
    pub desc: TileId,
    pub bar: TileId,
    pub title: TileId,
    pub line: TileId,
    /// The file's page background, hidden when the file has one ([`write_model`]).
    pub background: Option<TileId>,
    pub tabs: [TileId; 5],
    /// Each tab's inner rect, in a file that has one ([`write_tabs`]).
    pub tab_inners: [Option<TileId>; 5],
}

impl Paths {
    fn live(&self, ui: &Ui) -> bool {
        [self.menu, self.list, self.icon, self.desc_rect, self.desc, self.bar, self.title, self.line].into_iter().chain(self.tabs).chain(self.background).chain(self.tab_inners.into_iter().flatten()).all(|t| ui.is_live(t))
    }
}

/// The first tile named `name` under `root` (or `root`), in file order, case not mattering.
fn find_named(ui: &Ui, root: TileId, name: &str) -> Option<TileId> {
    let mut stack = vec![root];
    while let Some(t) = stack.pop() {
        let tile = ui.tile(t);
        if !tile.live {
            continue;
        }
        if tile.name.eq_ignore_ascii_case(name) {
            return Some(t);
        }
        stack.extend(tile.children.iter().rev().copied());
    }
    None
}

/// Whether the row's template can be found from `list` as the tree looks for it: on that tile
/// or on one above it (`fnv_ui::Ui::add_code_tile_from_template`).
fn has_template(ui: &Ui, list: TileId) -> bool {
    let mut at = Some(list);
    while let Some(t) = at {
        if ui.tile(t).templates.iter().any(|tp| tp.name.eq_ignore_ascii_case(ROW_TEMPLATE)) {
            return true;
        }
        at = ui.tile(t).parent;
    }
    false
}

/// Every tile the model writes or the input reads, and the row's template; the reason when
/// one is missing (a missing or cut-down file keeps the hand-built page). `cut`: [`CUT_AID`].
pub fn paths(ui: &Ui, cut: Option<&str>) -> Result<Paths, String> {
    let menu = ui.menu(STATS_MENU).ok_or_else(|| format!("{STATS_MENU} is not in the tree"))?;
    let mut found: HashMap<String, TileId> = HashMap::new();
    let mut stack = vec![menu];
    while let Some(t) = stack.pop() {
        let tile = ui.tile(t);
        if !tile.live {
            continue;
        }
        found.entry(tile.name.to_ascii_lowercase()).or_insert(t);
        stack.extend(tile.children.iter().rev().copied());
    }
    let cut = cut.map(str::to_ascii_lowercase);
    if let Some(c) = &cut {
        found.remove(c);
    }
    let get = |name: &str| found.get(&name.to_ascii_lowercase()).copied().ok_or_else(|| format!("{name} missing"));
    let list = get(LIST)?;
    get(DISPLAY)?;
    let mut tabs = [menu; 5];
    for (t, name) in tabs.iter_mut().zip(TABS) {
        *t = get(name)?;
    }
    if cut.as_deref() == Some(&ROW_TEMPLATE.to_ascii_lowercase()) || !has_template(ui, list) {
        return Err(format!("{ROW_TEMPLATE} missing"));
    }
    Ok(Paths {
        menu,
        list,
        icon: get(ICON)?,
        desc_rect: get(DESC_RECT)?,
        desc: get(DESC)?,
        bar: get(BAR)?,
        title: get(TITLE)?,
        line: get(LINE)?,
        background: get(BACKGROUND).ok(),
        tabs,
        tab_inners: tabs.map(|t| find_named(ui, t, TAB_INNER)),
    })
}

/// A row made from the template, known by its actor value: it keeps that place in the list
/// whatever else is taken away or made again. Its tile has the name the original's has, the
/// value's AVIF name [observed: private job pipboy_special_a, `GetActiveUIComponentName` under
/// the pointer]; scripts read that name, this code never looks a row up by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MadeRow {
    pub av: u16,
    pub tile: TileId,
    text: TileId,
    /// The template gives its height (a number or an expression): it is kept.
    explicit: bool,
}

/// The rows `wanted`, by actor value and name, in that order; the ones whole kept. A row
/// unwanted, renamed or missing its text is unloaded whole, so no root is left without its
/// parts. A failure makes none and leaves none of its own: a retry cannot double them.
/// Returns whether anything was unloaded or made: what was written of the rows is then out of
/// date, and with it what was measured and checked.
fn ensure_rows(ui: &mut Ui, p: &Paths, rows: &mut Vec<MadeRow>, wanted: &[(u16, &str)]) -> Result<bool, String> {
    if wanted.is_empty() {
        return Err("no rows to show".into());
    }
    let mut changed = false;
    rows.retain(|r| {
        let whole = ui.is_live(r.tile) && ui.is_live(r.text);
        let keep = whole && wanted.iter().any(|&(av, name)| av == r.av && ui.tile(r.tile).name == name);
        if !keep {
            if ui.is_live(r.tile) {
                ui.unload_tile(r.tile);
            }
            changed = true;
        }
        keep
    });
    let missing: Vec<(u16, &str)> = wanted.iter().copied().filter(|&(av, _)| !rows.iter().any(|r| r.av == av)).collect();
    if missing.is_empty() {
        return Ok(changed);
    }
    let at = ui.path_of(p.list);
    let mut made: Vec<(u16, TileId)> = Vec::with_capacity(missing.len());
    let undo = |ui: &mut Ui, made: &[(u16, TileId)]| {
        for &(_, t) in made {
            ui.unload_tile(t);
        }
        ui.update();
    };
    for &(av, name) in &missing {
        // The template's includes were spliced in when the file was read (menu-xml.md §2.2),
        // so no files are needed to make a row.
        match ui.add_code_tile_from_template(&fnv_ui::MemSource::default(), &at, ROW_TEMPLATE, Some(name)) {
            Some(t) => made.push((av, t)),
            None => {
                undo(ui, &made);
                return Err(format!("{ROW_TEMPLATE} made no row"));
            }
        }
    }
    ui.update();
    let mut new = Vec::with_capacity(made.len());
    for &(av, tile) in &made {
        let Some(text) = find_named(ui, tile, ROW_TEXT) else {
            undo(ui, &made);
            return Err(format!("{ROW_TEMPLATE} has no {ROW_TEXT}"));
        };
        let row = ui.tile(tile);
        let computed = row.trait_index("height").is_some_and(|k| row.traits[k].expr.is_some());
        new.push(MadeRow { av, tile, text, explicit: computed || num(ui, tile, "height") > 0.0 });
    }
    rows.extend(new);
    rows.sort_by_key(|r| r.av);
    Ok(true)
}

fn level_text(snap: &Snapshot) -> String {
    snap.level.map_or("--".into(), |l| l.to_string())
}

/// What the model shows, as written last.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub rows: Vec<Row>,
    pub level: String,
    pub hp: String,
    pub ap: String,
    pub xp: String,
    pub selected: usize,
    pub device: bool,
}

impl Shown {
    pub fn of(snap: &Snapshot, selected: usize, device: bool) -> Shown {
        Shown {
            rows: snap.special.clone(),
            level: level_text(snap),
            hp: stats::figures(snap.hp),
            ap: stats::figures(snap.ap),
            xp: stats::figures(snap.xp),
            selected,
            device,
        }
    }

    /// Whether this is what `snap` shows, without copying its rows.
    fn same(&self, snap: &Snapshot, selected: usize, device: bool) -> bool {
        self.selected == selected
            && self.device == device
            && self.rows == snap.special
            && self.level == level_text(snap)
            && self.hp == stats::figures(snap.hp)
            && self.ap == stats::figures(snap.ap)
            && self.xp == stats::figures(snap.xp)
    }
}

/// Write what the page shows ([`Shown`]), each value only when it differs from what this code
/// wrote last, so what a script set meanwhile stands until the model changes (`Ui::write_changed`).
pub fn write_model(ui: &mut Ui, p: &Paths, rows: &[MadeRow], s: &Shown) {
    ui.write_changed(p.menu, "user0", Value::Num(PAGE as f64));
    if let Some(background) = p.background {
        ui.write_changed(background, "visible", flag(false));
    }
    for (name, v) in [("user8", &s.level), ("user5", &s.hp), ("user6", &s.ap), ("user7", &s.xp)] {
        ui.write_changed(p.menu, name, Value::Str(v.clone()));
    }
    ui.write_changed(p.list, "user0", flag(s.rows.iter().any(|r| r.mark.is_some())));
    for (i, (made, row)) in rows.iter().zip(&s.rows).enumerate() {
        ui.write_changed(made.tile, "listindex", Value::Num(i as f64));
        // [observed: private job pipboy_special_a]
        ui.write_changed(made.tile, "id", Value::Num(-1.0));
        ui.write_changed(made.tile, "string", Value::Str(row.name.clone()));
        // A text value keeps the "--" of a game without scripts (`Value::text`); -1 hides the
        // number [observed: the template's own value].
        ui.write_changed(made.tile, "user1", row.value.clone().map_or(Value::Num(-1.0), Value::Str));
        ui.write_changed(made.tile, "user0", flag(row.mark.is_some()));
        ui.write_changed(made.tile, "user2", Value::Str(row.mark.unwrap_or_default().to_string()));
    }
    // A row without a picture keeps `visible` with an empty `filename`: the line and the text
    // follow it in the file, and the hand-built page draws them without a picture.
    let selected = s.rows.get(s.selected);
    ui.write_changed(p.icon, "filename", Value::Str(selected.and_then(|r| r.icon.clone()).unwrap_or_default()));
    ui.write_changed(p.icon, "visible", flag(selected.is_some()));
    ui.write_changed(p.desc, "string", Value::Str(selected.map(|r| r.description.clone()).unwrap_or_default()));
    // Only a name the reader left unresolved: a replacement's or a script's words stand.
    if ui.tile(p.title).get("string").is_some_and(|v| v.text().trim_start().starts_with('&')) {
        ui.set_trait(p.title, "string", Value::Str(Page::Stats.title().to_string()));
    }
}

/// What the screen menu changed of the file, to give back on the device.
#[derive(Clone, Debug, PartialEq)]
struct FlatChanges {
    title: Option<Value>,
    line: Option<(f64, f64)>,
}

/// The screen menu's layout, the maintainer's [design: the owner, 2026-10-31, `frame::heading_title`,
/// docs/design/screen-menu.md §2]: no title, the heading's line running on from the lead line.
/// On the device what it changed goes back. Called with the texts measured, as the
/// file may place the line by the title's width: what the file works out again for the line is
/// put right here, on every pass.
fn write_frame(ui: &mut Ui, p: &Paths, device: bool, flat: &mut Option<FlatChanges>) {
    if device {
        if let Some(was) = flat.take() {
            ui.set_trait(p.title, "visible", was.title.unwrap_or(flag(true)));
            if let Some((x, len)) = was.line {
                ui.set_trait(p.line, "x", Value::Num(x));
                ui.set_trait(p.line, "_length", Value::Num(len));
            }
        }
        return;
    }
    if flat.is_none() {
        let title = ui.tile(p.title).get("visible").cloned();
        let line = ui.tile(p.line).get("_length").map(Value::num).map(|len| (num(ui, p.line, "x"), len));
        ui.set_trait(p.title, "visible", flag(false));
        *flat = Some(FlatChanges { title, line });
    }
    if let Some((x, len)) = flat.as_ref().and_then(|f| f.line) {
        let from = f64::from(crate::pipboy::frame::lead_line_end());
        if num(ui, p.line, "x") != from {
            ui.set_trait(p.line, "x", Value::Num(from));
            ui.set_trait(p.line, "_length", Value::Num(x + len - from));
        }
    }
}

/// What this slice's layout needs of a tab, as the tree reads it at any size [observed: by
/// fnvtool ui tree]: the tab's inner rect repeats the tab's own place, and has `alpha` 0 over
/// shown words and lines. The original stands the tab at its one place [observed: private job
/// pipboy_special_a], so it does not add the two [hypothesis: the tab has no `locus`, the
/// inner rect has], and shows the words [hypothesis: as a hotrect's `alpha`,
/// `super::layout::place`]. `super::layout` reads no `locus` and multiplies a rect's `alpha`
/// for every menu, so here alone the inner rect's place is put back on every pass, as the file
/// works it out again when the words change, and what is under it keeps its own `alpha`
/// ([`super::layout::OWN_ALPHA`]), marked once [design].
fn write_tabs(ui: &mut Ui, p: &Paths, marked: &mut bool) {
    for inner in p.tab_inners.into_iter().flatten() {
        for name in ["x", "y"] {
            if num(ui, inner, name) != 0.0 {
                ui.set_trait(inner, name, Value::Num(0.0));
            }
        }
        if !*marked {
            let mut stack = ui.tile(inner).children.clone();
            while let Some(t) = stack.pop() {
                stack.extend(ui.tile(t).children.iter().copied());
                ui.set_trait(t, layout::OWN_ALPHA, Value::Num(1.0));
            }
        }
    }
    *marked = true;
}

/// The places code gives the file to work from: each row's height and `_y`, the list's height,
/// the box's place and the description's scroll steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Geometry {
    pub rows: Vec<(f64, f64)>,
    pub list: f64,
    pub highlight: (f64, f64),
    pub desc_bound: usize,
}

/// [`Geometry`] from the sizes measured; the reason when no usable layout comes of them. A list
/// that does not hold its rows has no scrolling here: the hand-built page takes over.
pub fn geometry(ui: &Ui, p: &Paths, rows: &[MadeRow], selected: usize, highlight: bool) -> Result<Geometry, String> {
    let mut places = Vec::with_capacity(rows.len());
    let mut y = 0.0;
    let mut tallest: f64 = 0.0;
    for (i, row) in rows.iter().enumerate() {
        let h = if row.explicit {
            num(ui, row.tile, "height")
        } else {
            let spacing = num(ui, row.tile, "_VerticalSpacing");
            num(ui, row.text, "height") + if spacing > 0.0 { spacing } else { ROW_SPACING }
        };
        if h <= 0.0 {
            return Err(format!("row {i} has no height"));
        }
        places.push((h, y));
        y += h;
        tallest = tallest.max(h);
    }
    let file = num(ui, p.list, "_number_of_visible_items");
    let visible = if file > 0.0 { file } else { stats::VISIBLE as f64 };
    if (rows.len() as f64) > visible {
        return Err(format!("{} rows and the list shows {visible}", rows.len()));
    }
    let window = num(ui, p.desc_rect, "height");
    if window <= 0.0 {
        return Err(format!("{DESC_RECT} has no height"));
    }
    let desc_bound = ((num(ui, p.desc, "height") - window) / f64::from(stats::DESC_STEP)).ceil().max(0.0) as usize;
    let box_at = places.get(selected).filter(|_| highlight).map_or((-1.0, 0.0), |&(h, y)| (y, h));
    Ok(Geometry { rows: places, list: visible * tallest, highlight: box_at, desc_bound })
}

fn write_geometry(ui: &mut Ui, p: &Paths, rows: &[MadeRow], g: &Geometry) {
    for (row, &(h, y)) in rows.iter().zip(&g.rows) {
        if !row.explicit {
            ui.write_changed(row.tile, "height", Value::Num(h));
        }
        ui.write_changed(row.tile, "_y", Value::Num(y));
    }
    ui.write_changed(p.list, "height", Value::Num(g.list));
    ui.write_changed(p.list, "_highlight_y", Value::Num(g.highlight.0));
    ui.write_changed(p.list, "_selected_height", Value::Num(g.highlight.1));
    // Hidden at 1 [observed: pipboy.md §5.1.1]; the +1 is our adaptation [design].
    ui.write_changed(p.bar, "_number_of_items", Value::Num(g.desc_bound as f64 + 1.0));
}

/// The model's scroll over the file's own count of the clicks, as the prefab asks of code that
/// sets the value [observed: pipboy.md §5.1.1; design: as `super::tutorial_menu::write_scroll`].
pub fn write_scroll(ui: &mut Ui, bar: TileId, steps: usize) {
    ui.set_trait(bar, "_current_value", Value::Num(steps as f64));
    ui.set_trait(bar, "_SetInCode", Value::Num(1.0));
}

/// What the targets were last checked against: each region's tile, rectangle and clip, and
/// whether it is shown and a target. The pointer's check is run again when this changes.
type Regions = Vec<(TileId, [f64; 4], Option<[f64; 4]>, bool)>;

/// A rectangle's left, top, width and height.
fn corners(r: &layout::Rect) -> [f64; 4] {
    [r.left, r.top, r.width, r.height]
}

/// Whether a drawn item shows on the canvas: it has a size, lies on the canvas, and the middle
/// of it is inside what clips it. Its middle, when it does.
fn on_canvas(item: &layout::DrawItem) -> Option<(f64, f64)> {
    let r = item.rect;
    let (right, bottom) = (r.left + r.width, r.top + r.height);
    let inside = r.width > 0.0 && r.height > 0.0 && r.left >= 0.0 && r.top >= 0.0 && right <= f64::from(CANVAS.x) && bottom <= f64::from(CANVAS.y);
    let middle = (r.left + r.width / 2.0, r.top + r.height / 2.0);
    let in_clip = item.clip.is_none_or(|c| c.width > 0.0 && c.height > 0.0 && middle.0 >= c.left && middle.0 <= c.left + c.width && middle.1 >= c.top && middle.1 <= c.top + c.height);
    (inside && in_clip).then_some(middle)
}

/// Whether what the file worked out can be drawn and used, on every final pass, so what a
/// script moved or hid since is seen: every row's text and every tab shown, not transparent,
/// on the canvas and inside its clip, and the description shown. When any of that changed, the
/// rows and the tabs but the one showing are checked to take the pointer at their middles
/// (`regions`: [`Regions`]); the tab showing need not, as a click on it does nothing. The
/// description's area, for the wheel, or the reason for the hand-built page.
pub fn usable(ui: &Ui, p: &Paths, rows: &[MadeRow], regions: &mut Option<Regions>) -> Result<Rect, String> {
    let items = layout::layout(ui, p.menu, &|_, _, _| (0.0, 0.0), true);
    let drawn = |i: &&layout::DrawItem| i.visible && i.alpha > 0.0;
    let targets = |t: TileId| fnv_ui::input::shown(ui, t) && fnv_ui::input::targets(ui, t);
    let mut now: Regions = Vec::with_capacity(rows.len() + p.tabs.len() + 1);
    let mut wanted: Vec<(StatsTile, (f64, f64))> = Vec::with_capacity(rows.len() + p.tabs.len());
    for (i, row) in rows.iter().enumerate() {
        let item = items.iter().find(|x| x.tile == row.text).filter(drawn).ok_or_else(|| format!("row {i} is not shown"))?;
        let middle = on_canvas(item).ok_or_else(|| format!("row {i} does not fit"))?;
        // Its foot too: the rows are not scrolled here, so none may be cut off.
        let foot = item.rect.top + item.rect.height;
        if item.clip.is_some_and(|c| foot > c.top + c.height + 1.0) {
            return Err(format!("row {i} does not fit"));
        }
        now.push((row.tile, corners(&item.rect), item.clip.as_ref().map(corners), targets(row.tile)));
        wanted.push((StatsTile::Row(i), middle));
    }
    for (i, &tab) in p.tabs.iter().enumerate() {
        // A tab is a hotrect, which draws nothing of its own: the largest thing drawn in it.
        let item = items.iter().filter(drawn).filter(|x| x.rect.width > 0.0 && x.rect.height > 0.0 && inside(ui, x.tile, tab)).max_by(|a, b| (a.rect.width * a.rect.height).total_cmp(&(b.rect.width * b.rect.height)));
        let item = item.ok_or_else(|| format!("tab {i} is not shown"))?;
        let middle = on_canvas(item).ok_or_else(|| format!("tab {i} does not fit"))?;
        now.push((tab, corners(&item.rect), item.clip.as_ref().map(corners), targets(tab)));
        if i != PAGE {
            wanted.push((StatsTile::Tab(i), middle));
        }
    }
    let desc = items.iter().find(|x| x.tile == p.desc).filter(drawn).ok_or("the description is not shown")?;
    let area = desc.clip.unwrap_or(desc.rect);
    // An empty description measures nothing; the scroll moves the text, so only its width is
    // held to the canvas, and its height only where a window gives it.
    let says = ui.tile(p.desc).get("string").is_some_and(|v| !v.text().is_empty());
    if says && (area.width <= 0.0 || area.left < 0.0 || area.left + area.width > f64::from(CANVAS.x) || desc.clip.is_some_and(|c| c.height <= 0.0)) {
        return Err("the description does not fit".into());
    }
    now.push((p.desc, corners(&area), None, true));
    if regions.as_ref() != Some(&now) {
        for (what, at) in wanted {
            let hit = fnv_ui::input::hit(ui, &[p.menu], at, &fnv_ui::input::no_measure);
            if hit.and_then(|h| stats_tile(ui, p, rows, h.tile)) != Some(what) {
                return Err(format!("{what:?} does not take the pointer"));
            }
        }
        *regions = Some(now);
    }
    Ok(Rect::new(area.left as f32, area.top as f32, (area.left + area.width) as f32, (area.top + area.height) as f32))
}

/// Whether `t` is `of` or under it.
fn inside(ui: &Ui, t: TileId, of: TileId) -> bool {
    let mut at = Some(t);
    while let Some(x) = at {
        if x == of {
            return true;
        }
        at = ui.tile(x).parent;
    }
    false
}

/// What [`page`] keeps: the rows, what was last written, and the reasons warned of.
#[derive(Default)]
pub(crate) struct Written {
    menu: Option<TileId>,
    paths: Option<Paths>,
    rows: Vec<MadeRow>,
    shown: Option<Shown>,
    highlight: bool,
    geometry: Option<Geometry>,
    sizes: SizesWritten,
    flat: Option<FlatChanges>,
    /// What is under the tabs' inner rects is marked ([`write_tabs`]).
    tabs_marked: bool,
    /// What the targets were last checked against ([`usable`]).
    regions: Option<Regions>,
    /// What is shown changed since the log last said so.
    tell: bool,
    /// The description's scroll, the text's place and the bound last logged.
    drawn: Option<(f64, f64, usize)>,
    fallbacks: HashSet<String>,
}

/// The decision every reader shares, and what the page keeps.
#[derive(Resource, Default)]
pub struct StatsTiles {
    /// The menu, while the page is ready to display this frame ([`settle`]).
    pub ready: Option<TileId>,
    /// The menu painted by the last drawing (`super::draw`): what the player sees.
    pub shown: Option<TileId>,
    /// The description's area on the canvas, while ready.
    pub desc_area: Option<Rect>,
    /// The tab and frame that area was found for.
    pub desc_for: Option<(usize, Frame)>,
    /// [`CUT_AID`].
    pub cut: Option<String>,
    /// [`CUT_AID`]'s time.
    pub cut_until: Option<f32>,
    /// A menu over the page when this frame began ([`modal_over`]): set before the scripts,
    /// so it holds through the frame that menu closes in.
    pub modal: bool,
    /// Another menu took this frame's tile input (`super::input::feed`).
    pub others: bool,
    written: Written,
}

impl StatsTiles {
    /// The description's area, only when it belongs to the tab and frame shown now: the tab can
    /// change after the last decision and before the next.
    pub fn desc_area_for(&self, tab: usize, frame: Frame) -> Option<Rect> {
        self.desc_area.filter(|_| self.ready.is_some() && self.desc_for == Some((tab, frame)))
    }
}

/// One pass over the page; the menu when all of it went through. `late`: the pass before the
/// drawing; the other, before the scripts, does the sizes only for a model that changed.
/// `sizes` is one round of measuring the given text tiles, with the tree worked out.
pub(crate) fn page(ui: &mut Ui, snap: &Snapshot, view: &mut StatsView, device: bool, late: bool, tiles: &mut StatsTiles, sizes: &mut dyn FnMut(&mut Ui, &[TileId], &mut SizesWritten) -> usize) -> Result<TileId, String> {
    let w = &mut tiles.written;
    let p = match w.paths.filter(|c| c.live(ui) && ui.menu(STATS_MENU) == Some(c.menu)) {
        Some(p) => p,
        None => paths(ui, tiles.cut.as_deref())?,
    };
    // A new menu tile (the tree built again): its rows, sizes and writes are all to do again.
    if w.menu != Some(p.menu) {
        let fallbacks = std::mem::take(&mut w.fallbacks);
        *w = Written { menu: Some(p.menu), fallbacks, ..Default::default() };
    }
    w.paths = Some(p);
    let wanted: Vec<(u16, &str)> = fnv_game::actor_values::SPECIALS.zip(snap.special.iter().map(|r| r.name.as_str())).collect();
    if ensure_rows(ui, &p, &mut w.rows, &wanted)? {
        // Rows made or gone: nothing written, measured or checked for the last set holds.
        w.shown = None;
        w.geometry = None;
        w.regions = None;
    }
    let selected = view.selected[PAGE];
    let changed = !w.shown.as_ref().is_some_and(|s| s.same(snap, selected, device)) || w.highlight != view.highlight;
    if changed {
        let shown = Shown::of(snap, selected, device);
        write_model(ui, &p, &w.rows, &shown);
        w.shown = Some(shown);
        w.highlight = view.highlight;
        w.tell = true;
    }
    if changed || late {
        ui.update();
        let texts = menus::texts(ui, p.menu);
        // Three rounds: a size leads to a place and a place to a wrap width [design: as
        // `super::tutorial_menu::settle`].
        for _ in 0..3 {
            if sizes(ui, &texts, &mut w.sizes) == 0 {
                break;
            }
            ui.update();
        }
        write_tabs(ui, &p, &mut w.tabs_marked);
        write_frame(ui, &p, device, &mut w.flat);
        ui.update();
        let g = geometry(ui, &p, &w.rows, selected, view.highlight)?;
        if view.desc_bound() != g.desc_bound {
            view.set_desc_bound(g.desc_bound);
        }
        if w.geometry.as_ref() != Some(&g) {
            write_geometry(ui, &p, &w.rows, &g);
            ui.update();
            w.geometry = Some(g);
            w.tell = true;
        }
        // On every drawn frame, so a window injected or enabled later crops too.
        fnv_ui::input::mark_clip_windows(ui, p.menu);
    }
    write_scroll(ui, p.bar, view.desc);
    if late {
        ui.update();
        // Every final pass, so readiness is about the tree that is about to be painted.
        let area = usable(ui, &p, &w.rows, &mut w.regions)?;
        tiles.desc_area = Some(area);
        let w = &mut tiles.written;
        if std::mem::take(&mut w.tell) {
            info!("{}", describe(ui, &p, w));
        }
        let now = (num(ui, p.bar, "_current_value"), num(ui, p.desc, "y"), view.desc_bound());
        if w.drawn != Some(now) {
            let (scroll, text_y) = (Value::Num(now.0).text(), Value::Num(now.1).text());
            info!("tiles: {STATS_MENU} description scroll drawn at {scroll} of {}, text y {text_y}", view.desc_bound());
            w.drawn = Some(now);
        }
    }
    Ok(p.menu)
}

/// A line for the log when what the page shows changes (smoke reads these).
fn describe(ui: &Ui, p: &Paths, w: &Written) -> String {
    let n = |x: f64| Value::Num(x).text();
    let (Some(s), Some(g)) = (w.shown.as_ref(), w.geometry.as_ref()) else { return format!("tiles: {STATS_MENU} page written") };
    let row = s.rows.get(s.selected);
    format!(
        "tiles: {STATS_MENU} page from tiles: {} rows, \"{}\" selected, value {}, boxed at {}, list height {}, row height {}, description {} high, scrolls {}, {}",
        s.rows.len(),
        row.map_or("", |r| r.name.as_str()),
        row.and_then(|r| r.value.as_deref()).unwrap_or("none"),
        n(g.highlight.0),
        n(g.list),
        n(g.rows.first().map_or(0.0, |r| r.0)),
        n(num(ui, p.desc, "height")),
        g.desc_bound,
        if s.device { "device" } else { "screen menu" },
    )
}

/// What a pass over the page reads.
#[derive(SystemParam)]
pub(crate) struct StatsPage<'w> {
    script: Option<ResMut<'w, ScriptState>>,
    switch: Option<Res<'w, TileMenus>>,
    pip: Option<Res<'w, PipBoy>>,
    prepared: Option<Res<'w, Prepared>>,
    view: Option<ResMut<'w, StatsView>>,
    tiles: ResMut<'w, StatsTiles>,
    fonts: FontKit<'w>,
    time: crate::pause::AidTime<'w>,
}

/// Whether the page is eligible to prepare: the switch on and the tab showing.
pub fn eligible(switch: TileMenus, pip: &PipBoy) -> bool {
    switch.stats && pip.shown() && pip.page == Page::Stats && pip.tab() == PAGE
}

/// A pass; `None` when the page is not eligible (nothing is touched then).
fn pass(p: &mut StatsPage<'_>, late: bool) -> Option<Result<TileId, String>> {
    let pip = p.pip.as_deref()?;
    if !eligible(p.switch.as_deref().copied().unwrap_or_default(), pip) {
        return None;
    }
    let device = pip.frame == Frame::Device;
    let game = &mut p.script.as_mut()?.rt.as_mut()?.game;
    game.ui_files.as_ref()?;
    let Some(snap) = p.prepared.as_deref().and_then(|x| x.snap.as_ref()) else {
        return late.then(|| Err("nothing to show yet".to_string()));
    };
    // What the page does to the view (the bound) is no new look for the hand-built page.
    let view = p.view.as_mut()?.bypass_change_detection();
    let fonts = &mut p.fonts;
    let mut sizes = |ui: &mut Ui, texts: &[TileId], written: &mut SizesWritten| {
        let size = fonts.menu_measure(&menus::fonts_of(ui, texts));
        menus::write_text_sizes_of(ui, texts, written, &size)
    };
    Some(page(&mut game.ui_tree, snap, view, device, late, &mut p.tiles, &mut sizes))
}

/// Before the scripts, after the tile input: this frame's model in the tree, and the scroll.
pub(crate) fn write(mut p: StatsPage) {
    // A failure here is told by `settle`, which decides.
    let _ = pass(&mut p, false);
}

/// Before the drawing, after the model's keys: what they changed, the tree worked out, and the
/// one decision every reader of this frame shares.
pub(crate) fn settle(mut p: StatsPage) {
    if p.tiles.cut_until.is_some_and(|until| p.time.elapsed_secs() >= until) {
        info!("test aid: {CUT_AID} over");
        p.tiles.cut = None;
        p.tiles.cut_until = None;
    }
    let ready = match pass(&mut p, true) {
        Some(Ok(menu)) => Some(menu),
        Some(Err(reason)) => {
            let line = format!("{STATS_MENU} drawn by hand: {reason}");
            if p.tiles.written.fallbacks.insert(line.clone()) {
                warn!("{line}");
            }
            None
        }
        None => None,
    };
    if ready.is_none() {
        // The next time it is written whole; the rows stay.
        let w = &mut p.tiles.written;
        w.shown = None;
        w.geometry = None;
        w.regions = None;
        w.drawn = None;
        p.tiles.desc_area = None;
    }
    if p.tiles.ready != ready {
        p.tiles.ready = ready;
    }
    p.tiles.desc_for = ready.and(p.pip.as_deref()).map(|pip| (PAGE, pip.frame));
}

/// The keys the hand-built page acts on itself (`crate::pipboy::stats::keys`,
/// `crate::pipboy::keys`), by the tile input's names, and the controller's triggers standing in
/// for its pages' keys (`crate::pad::menu_buttons`).
pub const HAND_BUILT_KEYS: [&str; 10] = ["W", "S", "A", "D", "xup", "xdown", "xleft", "xright", "xbuttonlt", "xbuttonrt"];

/// Whether the open menu is one over the Pip-Boy's pages and not one of them.
pub fn modal_over(open: Option<u16>) -> bool {
    use fnv_game::game::menus;
    open.is_some_and(|code| !menus::answers(menus::PIPBOY, code))
}

/// The menu, when the page takes this frame's tile input: painted last frame, and the page up,
/// with nothing over it. Every reader of the frame's input decides by this.
pub fn takes(tiles: &StatsTiles, switch: TileMenus, pip: &PipBoy, game: &fnv_game::Game<'_>, paused: bool) -> Option<TileId> {
    if !pip.open() || !eligible(switch, pip) || paused || modal_over(game.menu) {
        return None;
    }
    tiles.shown.filter(|&m| game.ui_files.is_some() && game.ui_tree.is_live(m))
}

/// What a tile of the page stands for, by the tile or a parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsTile {
    Row(usize),
    Tab(usize),
    StepUp,
    StepDown,
    PageUp,
    PageDown,
    Marker,
}

/// What `tile` stands for: a row the page made, a tab, or a piece of the description's scroll
/// bar [documented: scrollbar_vert.xml's names].
pub fn stats_tile(ui: &Ui, p: &Paths, rows: &[MadeRow], tile: TileId) -> Option<StatsTile> {
    let mut at = Some(tile);
    let mut in_bar = false;
    while let Some(t) = at {
        in_bar |= t == p.bar;
        at = ui.tile(t).parent;
    }
    let mut at = Some(tile);
    while let Some(t) = at {
        if let Some(i) = rows.iter().position(|r| r.tile == t) {
            return Some(StatsTile::Row(i));
        }
        if let Some(i) = p.tabs.iter().position(|&x| x == t) {
            return Some(StatsTile::Tab(i));
        }
        if in_bar {
            match ui.tile(t).name.to_ascii_lowercase().as_str() {
                "scrollbar_vert_up" => return Some(StatsTile::StepUp),
                "scrollbar_vert_down" => return Some(StatsTile::StepDown),
                "scrollbar_vert_page_up" => return Some(StatsTile::PageUp),
                "scrollbar_vert_page_down" => return Some(StatsTile::PageDown),
                "scrollbar_vert_marker" => return Some(StatsTile::Marker),
                _ => {}
            }
        }
        at = ui.tile(t).parent;
    }
    None
}

/// The scroll steps of a page area's click: the window's less a step, at least one [design].
pub fn page_steps(ui: &Ui, p: &Paths) -> i32 {
    ((num(ui, p.desc_rect, "height") / f64::from(stats::DESC_STEP)).floor() as i32 - 1).max(1)
}

/// A press on the description's marker, held: the bar's top and how far under the marker's top
/// the pointer took it (canvas units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerDrag {
    marker: TileId,
    top: f64,
    grab: f64,
}

/// The [`MarkerDrag`] of a press on the marker; worked out once a press, as the tutorial's.
pub fn marker_grab(ui: &Ui, p: &Paths, pointer_y: f64) -> Option<MarkerDrag> {
    let marker = find_named(ui, p.bar, "scrollbar_vert_marker")?;
    let placed = layout::layout(ui, p.menu, &|_, _, _| (0.0, 0.0), true).into_iter().find(|i| i.tile == marker)?.rect;
    Some(MarkerDrag { marker, top: placed.top - num(ui, marker, "y"), grab: pointer_y - placed.top })
}

/// The scroll steps with the marker held at `pointer_y`, by the file's `_step_height`
/// [documented: scrollbar_vert.xml], within `most`.
pub fn steps_under(ui: &Ui, drag: MarkerDrag, pointer_y: f64, most: usize) -> Option<usize> {
    let step = num(ui, drag.marker, "_step_height");
    (step > 0.0).then(|| ((pointer_y - drag.grab - drag.top) / step + 1e-9).floor().clamp(0.0, most as f64) as usize)
}

/// What [`feed`] keeps between frames.
#[derive(Default)]
pub(crate) struct Feed {
    /// The page took input last frame, in this frame of the Pip-Boy.
    taking: Option<Frame>,
    /// The pointer when the page came up, until it moves: a row under it does not take the
    /// selection, as the hand-built list's rule.
    drawn_at: Option<Vec2>,
    /// The row the pointer last selected, while it stays on it.
    acted: Option<usize>,
    drag: Option<MarkerDrag>,
    /// Where the pointer was last frame, and the tile under it then.
    last_at: Option<(f64, f64)>,
    under: Option<TileId>,
}

/// What the row under the pointer does to the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverAct {
    Select(usize),
    BoxOff,
}

/// The row under the pointer, `under`, against the one it last selected ([`Feed::acted`]).
/// `settled`: the pointer has moved since the page came up.
fn follow_hover(acted: &mut Option<usize>, under: Option<usize>, settled: bool) -> Option<HoverAct> {
    match under {
        Some(i) if settled && *acted != Some(i) => {
            *acted = Some(i);
            Some(HoverAct::Select(i))
        }
        Some(_) => None,
        None => acted.take().map(|_| HoverAct::BoxOff),
    }
}

/// The row a file's own key binding moved the highlight onto in this step, wherever it went,
/// the row under the pointer too: the step says what moved it. Not for a press the hand-built
/// keys act on as well, or the page would step twice.
fn keyed_row(ui: &Ui, p: &Paths, rows: &[MadeRow], report: &fnv_ui::input::Report, physical: &[&str]) -> Option<usize> {
    if !report.over_by_key || physical.iter().any(|k| menus::hand_built_takes(STATS_MENU, k)) {
        return None;
    }
    let (_, to) = report.over?;
    match stats_tile(ui, p, rows, to?)? {
        StatsTile::Row(i) => Some(i),
        _ => None,
    }
}

/// What the page's tile input reads of the Pip-Boy.
#[derive(SystemParam)]
pub(crate) struct FeedPip<'w> {
    pip: Res<'w, PipBoy>,
    screen: Option<Res<'w, PipBoyScreen>>,
    aid: Res<'w, PipBoyAid>,
    paused: Option<Res<'w, crate::pause::Paused>>,
}

/// The page's tile input, on a frame it takes it ([`takes`]): see the module notes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn feed(
    mut script: ResMut<ScriptState>,
    switch: Res<TileMenus>,
    from: FeedPip,
    mut tiles: ResMut<StatsTiles>,
    devices: Devices,
    view: Option<ResMut<StatsView>>,
    (mut tabs, mut hovered): (MessageWriter<PipBoyInput>, MessageWriter<TabHovered>),
    mut state: Local<Feed>,
) {
    let FeedPip { pip, screen, aid, paused } = from;
    let Some(rt) = script.rt.as_mut() else { return };
    let game = &mut rt.game;
    let modal = eligible(*switch, &pip) && pip.open() && modal_over(game.menu);
    if tiles.modal != modal {
        tiles.modal = modal;
    }
    let is_paused = paused.as_ref().is_some_and(|p| p.0);
    let menu = takes(&tiles, *switch, &pip, game, is_paused).filter(|_| !tiles.others);
    let (Some(menu), Some(p), Some(mut view)) = (menu, tiles.written.paths.filter(|p| Some(p.menu) == tiles.shown), view) else {
        *state = Feed::default();
        return;
    };
    let pointer = screen.as_ref().and_then(|s| s.pointer);
    if state.taking != Some(pip.frame) {
        *state = Feed { taking: Some(pip.frame), drawn_at: pointer, ..Default::default() };
    }
    if pointer != state.drawn_at {
        state.drawn_at = None;
    }
    let settled = state.drawn_at.is_none() || pointer.is_none();
    // The test aid's click (`crate::pipboy::drive_pointer`): 1 is its press's frame.
    let aid_press = aid.click == 1 && !aid.right;
    let held = aid_press || devices.mouse.pressed(MouseButton::Left);
    let at = pointer.map(|p| (f64::from(p.x), f64::from(p.y)));
    let mut input = fnv_ui::input::Input {
        at,
        press: aid_press || devices.mouse.just_pressed(MouseButton::Left),
        shift: devices.keys.pressed(KeyCode::ShiftLeft) || devices.keys.pressed(KeyCode::ShiftRight),
        ..Default::default()
    };
    let pad_menu = crate::pad::MenuAt { code: Some(fnv_game::game::menus::STATS), tab: PAGE };
    let physical = frame_keys(&mut input, &devices, game, pad_menu);
    let report = fnv_game::ui_input::step(game, &[menu], &input, &fnv_ui::input::no_measure);
    let ui = &game.ui_tree;
    // What is under the pointer itself, whatever a key moved the highlight to; looked for again
    // only when the pointer moved or the step's own look changed something.
    if report.over.is_some() || at != state.last_at {
        state.under = at.and_then(|at| fnv_ui::input::hit(ui, &[menu], at, &fnv_ui::input::no_measure)).map(|h| h.tile);
        state.last_at = at;
    }
    log_report(ui, &report);
    let rows = &tiles.written.rows;
    let what = |t: TileId| stats_tile(ui, &p, rows, t);
    if let Some((_, Some(to))) = report.over {
        if let Some(StatsTile::Tab(i)) = what(to) {
            hovered.write(TabHovered(i));
        }
    }
    let under = state.under;
    let under_row = under.and_then(what).and_then(|w| if let StatsTile::Row(i) = w { Some(i) } else { None });
    // Only a real change touches the view, which the page's other readers watch.
    match follow_hover(&mut state.acted, under_row, settled) {
        Some(HoverAct::Select(i)) => {
            view.select(PAGE, i);
            view.highlight = true;
        }
        Some(HoverAct::BoxOff) => view.highlight = false,
        None => {}
    }
    if let Some(i) = keyed_row(ui, &p, rows, &report, &physical) {
        view.select(PAGE, i);
        view.highlight = true;
    }
    if !held {
        state.drag = None;
    }
    let mut pressed_marker = false;
    for c in report.clicks.iter().filter(|c| ui.is_live(c.tile)) {
        if let fnv_ui::input::By::Key(k) = &c.by {
            if !menus::key_click_acts(STATS_MENU, k, &physical) {
                continue;
            }
        }
        let Some(tile) = what(c.tile) else { continue };
        info!("tiles: {STATS_MENU} {tile:?} on its tiles");
        match tile {
            StatsTile::Row(i) => view.select(PAGE, i),
            StatsTile::Tab(i) => {
                tabs.write(PipBoyInput::Tab(i));
            }
            StatsTile::StepUp => view.scroll_desc(-1),
            StatsTile::StepDown => view.scroll_desc(1),
            StatsTile::PageUp => view.scroll_desc(-page_steps(ui, &p)),
            StatsTile::PageDown => view.scroll_desc(page_steps(ui, &p)),
            StatsTile::Marker => {
                pressed_marker = true;
                state.drag = at.filter(|_| held && matches!(c.by, fnv_ui::input::By::Pointer)).and_then(|(_, y)| marker_grab(ui, &p, y));
            }
        }
    }
    // A drag: the scroll under the pointer while the press holds.
    if let (false, Some(drag), Some((_, y))) = (pressed_marker, state.drag, at) {
        if let Some(steps) = steps_under(ui, drag, y, view.desc_bound()).filter(|&s| s != view.desc) {
            view.desc = steps;
        }
    }
}

/// The resource, with the test aid read.
pub(crate) fn setup(app: &mut App) {
    let (cut, cut_until) = std::env::var(CUT_AID).ok().map_or((None, None), |v| {
        let (name, until) = parse_cut(&v);
        (Some(name), until)
    });
    app.insert_resource(StatsTiles { cut, cut_until, ..Default::default() });
}

/// [`CUT_AID`]'s value: the name and the time it stops at.
fn parse_cut(v: &str) -> (String, Option<f32>) {
    match v.split_once('@') {
        Some((name, until)) => (name.trim().to_string(), until.trim().parse().ok()),
        None => (v.trim().to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::input::{no_measure, By, Input, Pointer};
    use fnv_ui::MemSource;

    /// [design] A small layout written for these tests, with its own structure and numbers and
    /// plain literals; it does not come from the game's files and says nothing about them. It
    /// keeps the tile and template names the model and the input look up, which are the engine's
    /// interface ([`paths`]). Its only expressions are a row's place from what the model wrote
    /// and a stand-in for a file's own count of clicks, read from a test tile. So these tests do
    /// not cover the file's own places, how its expressions read what is written (a row's text
    /// from its `string`, the text's `y` and the marker's from the value), or whether a name is where the file has it:
    /// the smoke scenarios `tiles-stats-special*` check those on the real file.
    const FIXTURE: &str = r#"<menu name="StatsMenu"><rect name="Branch">
      <template name="stats_list_template"><hotrect name="TestRow"><target>&true;</target><x>8</x><width>90</width>
        <_VerticalSpacing>6</_VerticalSpacing>
        <y><add src="me()" trait="_y"/></y>
        <text name="ListItemText"><string>Row</string></text>
      </hotrect></template>
      <image name="background_layout"/>
      <text name="stats_title"><string>Words</string></text>
      <rect name="stats_headline_h2"><x>300</x><_length>40</_length></rect>
      <hotrect name="stats_special_container"><x>12</x><y>30</y><width>200</width><clipwindow>&true;</clipwindow><_number_of_visible_items>8</_number_of_visible_items></hotrect>
      <rect name="stats_display_rect"><x>400</x><y>34</y>
        <image name="stats_icon"/>
        <hotrect name="stats_description_rect"><y>100</y><width>200</width><height>60</height>
          <text name="stats_description"/>
          <rect name="Pulse"><user3>0</user3></rect>
          <hotrect name="stats_description_scrollbar"><x>210</x>
            <_current_value><copy src="sibling(Pulse)" trait="user3"/></_current_value>
            <hotrect name="scrollbar_vert_up"><target>&true;</target><width>12</width><height>12</height></hotrect>
            <hotrect name="scrollbar_vert_page_up"><target>&true;</target><y>12</y><width>12</width><height>8</height></hotrect>
            <hotrect name="scrollbar_vert_page_down"><target>&true;</target><y>20</y><width>12</width><height>8</height></hotrect>
            <hotrect name="scrollbar_vert_down"><target>&true;</target><y>40</y><width>12</width><height>12</height></hotrect>
            <image name="scrollbar_vert_marker"><target>&true;</target><x>20</x><width>12</width><height>6</height><_step_height>3</_step_height></image>
          </hotrect>
        </hotrect>
      </rect>
      <hotrect name="stats_tailline_status"><target>&true;</target><x>10</x><y>500</y><width>50</width><height>20</height><text name="TabWords"><string>Tab</string></text></hotrect>
      <hotrect name="stats_tailline_SPECIAL"><target>&true;</target><x>70</x><y>500</y><width>50</width><height>20</height><text name="TabWords"><string>Tab</string></text></hotrect>
      <hotrect name="stats_tailline_skills"><target>&true;</target><x>130</x><y>500</y><width>50</width><height>20</height><text name="TabWords"><string>Tab</string></text></hotrect>
      <hotrect name="stats_tailline_perks"><target>&true;</target><x>190</x><y>500</y><width>50</width><height>20</height><text name="TabWords"><string>Tab</string></text></hotrect>
      <hotrect name="stats_tailline_general"><target>&true;</target><x>250</x><y>500</y><width>50</width><height>20</height><text name="TabWords"><string>Tab</string></text></hotrect>
    </rect></menu>"#;

    /// Each row's height in the fixture: its text's 30 and its own spacing of 6.
    const ROW: f64 = 36.0;

    fn tree_changed(change: impl FnOnce(&str) -> String) -> Ui {
        let src = MemSource::default().with("s.xml", &change(FIXTURE));
        let mut ui = Ui::new();
        ui.load_menu(&src, "s.xml").unwrap();
        ui.update();
        ui
    }

    fn tree() -> Ui {
        tree_changed(str::to_string)
    }

    fn rows() -> Vec<Row> {
        (0..7).map(|i| Row { name: format!("Stat {i}"), value: Some((i + 1).to_string()), mark: None, description: "one|two".into(), icon: Some(format!("pics\\{i}.dds")) }).collect()
    }

    fn snap() -> Snapshot {
        Snapshot { level: Some(3), hp: Some((20.0, 40.0)), ap: Some((5.0, 6.0)), xp: Some((7.0, 8.0)), special: rows(), ..Default::default() }
    }

    fn view() -> StatsView {
        StatsView::with_lens([0, 7, 0, 0, 0])
    }

    /// A pass, with a measure of 10 a character and 30 a line, lines parted by `|`.
    fn run(ui: &mut Ui, snap: &Snapshot, view: &mut StatsView, device: bool, tiles: &mut StatsTiles) -> Result<TileId, String> {
        let mut sizes = |ui: &mut Ui, texts: &[TileId], w: &mut SizesWritten| {
            let size = |_: f64, text: &str, _: Option<f64>| (10.0 * text.chars().count() as f64, 30.0 * text.split('|').count() as f64);
            menus::write_text_sizes_of(ui, texts, w, &size)
        };
        page(ui, snap, view, device, true, tiles, &mut sizes)
    }

    fn text(ui: &Ui, t: TileId, name: &str) -> String {
        ui.tile(t).get(name).map(|v| v.text().to_string()).unwrap_or_default()
    }

    fn live_children(ui: &Ui, t: TileId) -> usize {
        ui.tile(t).children.iter().filter(|&&c| ui.is_live(c)).count()
    }

    #[test]
    fn the_model_writes_the_figures_and_seven_rows() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        let menu = run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        assert_eq!(num(&ui, menu, "user0"), 1.0, "the page");
        assert_eq!(text(&ui, menu, "user8"), "3", "the level");
        assert_eq!(text(&ui, menu, "user5"), "20/40");
        assert_eq!(text(&ui, menu, "user6"), "5/6");
        assert_eq!(text(&ui, menu, "user7"), "7/8");
        let p = paths(&ui, None).unwrap();
        let made = t.written.rows.clone();
        assert_eq!((made.len(), live_children(&ui, p.list)), (7, 7));
        assert_eq!(text(&ui, made[2].tile, "string"), "Stat 2");
        assert_eq!(ui.tile(made[2].tile).name, "Stat 2", "a row's tile has its name");
        assert_eq!(num(&ui, made[2].tile, "id"), -1.0);
        assert_eq!(text(&ui, made[2].tile, "user1"), "3");
        assert_eq!(num(&ui, made[2].tile, "listindex"), 2.0);
        assert_eq!(num(&ui, made[2].tile, "height"), ROW);
        assert_eq!(num(&ui, made[2].tile, "_y"), 2.0 * ROW);
        assert_eq!(num(&ui, p.list, "height"), 8.0 * ROW, "the file's visible rows, not a hand-built height");
        assert_eq!((num(&ui, p.list, "_highlight_y"), num(&ui, p.list, "_selected_height")), (-1.0, 0.0), "no box until a key or the pointer");
        assert_eq!(num(&ui, p.list, "user0"), 0.0);
        assert_eq!(text(&ui, p.icon, "filename"), "pics\\0.dds");
        assert_eq!(text(&ui, p.desc, "string"), "one|two");
    }

    #[test]
    fn a_placeholder_stays_text_and_no_value_hides_the_number() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        let mut s = snap();
        s.special[0].value = Some("--".into());
        s.special[1].value = None;
        s.special[2].mark = Some("(+)");
        run(&mut ui, &s, &mut v, true, &mut t).unwrap();
        let made = t.written.rows.clone();
        assert_eq!(text(&ui, made[0].tile, "user1"), "--", "a numeric write would lose it");
        assert_eq!(num(&ui, made[1].tile, "user1"), -1.0);
        assert_eq!((num(&ui, made[2].tile, "user0"), text(&ui, made[2].tile, "user2")), (1.0, "(+)".to_string()));
        assert_eq!(num(&ui, made[0].tile, "user0"), 0.0);
        let p = paths(&ui, None).unwrap();
        assert_eq!(num(&ui, p.list, "user0"), 1.0, "the list knows a row is marked");
    }

    #[test]
    fn the_box_follows_the_selection_only_while_highlighted() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        v.select(PAGE, 3);
        v.highlight = true;
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        assert_eq!((num(&ui, p.list, "_highlight_y"), num(&ui, p.list, "_selected_height")), (3.0 * ROW, ROW));
        assert_eq!(text(&ui, p.icon, "filename"), "pics\\3.dds");
        v.highlight = false;
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        assert_eq!(num(&ui, p.list, "_highlight_y"), -1.0);
        assert_eq!(text(&ui, p.icon, "filename"), "pics\\3.dds", "the display stays filled without the box");
    }

    #[test]
    fn a_row_without_a_picture_keeps_the_display() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        let mut s = snap();
        s.special[0].icon = None;
        run(&mut ui, &s, &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        assert_eq!(text(&ui, p.icon, "filename"), "");
        assert_eq!(num(&ui, p.icon, "visible"), 1.0, "what follows it in the file stays shown");
        assert_eq!(text(&ui, p.desc, "string"), "one|two");
    }

    #[test]
    fn the_description_scrolls_by_the_tiles_own_measure_and_the_value_is_the_models() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        let mut s = snap();
        // 150 high in a window of 60, 20 a step: 5 steps.
        s.special[0].description = "a|b|c|d|e".into();
        run(&mut ui, &s, &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        assert_eq!(v.desc_bound(), 5);
        assert_eq!(num(&ui, p.bar, "_number_of_items"), 6.0);
        v.scroll_desc(2);
        run(&mut ui, &s, &mut v, true, &mut t).unwrap();
        assert_eq!(num(&ui, p.bar, "_current_value"), 2.0);
        // A pulse in the value's own source: the model's value outlasts it and its end.
        let pulse = find_named(&ui, p.desc_rect, "Pulse").unwrap();
        for count in [1.0, 0.0] {
            ui.set_trait(pulse, "user3", Value::Num(count));
            run(&mut ui, &s, &mut v, true, &mut t).unwrap();
            assert_eq!(num(&ui, p.bar, "_current_value"), 2.0);
        }
        // A short text: nothing to scroll, the scroll pulled back, the bar's count at 1.
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        assert_eq!((v.desc_bound(), v.desc), (0, 0));
        assert_eq!(num(&ui, p.bar, "_number_of_items"), 1.0);
    }

    #[test]
    fn passes_and_reopenings_keep_the_rows_and_a_new_tree_makes_its_own() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let made = t.written.rows.clone();
        for _ in 0..3 {
            run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        }
        assert_eq!(t.written.rows, made);
        let p = paths(&ui, None).unwrap();
        assert_eq!(live_children(&ui, p.list), 7);
        // The tree built again.
        let mut again = tree();
        run(&mut again, &snap(), &mut v, true, &mut t).unwrap();
        let p = paths(&again, None).unwrap();
        assert_eq!(live_children(&again, p.list), 7);
        assert!(t.written.rows.iter().all(|r| again.is_live(r.tile)));
    }

    /// A middle row taken away: the others keep their tiles and places, and the new one has
    /// that place and everything written to it.
    #[test]
    fn a_row_taken_away_is_made_again_in_its_own_place() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let before = t.written.rows.clone();
        ui.unload_tile(before[3].tile);
        ui.update();
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let after = t.written.rows.clone();
        assert_eq!(after.len(), 7);
        for i in [0, 1, 2, 4, 5, 6] {
            assert_eq!(after[i], before[i], "row {i} stays itself");
        }
        assert_ne!(after[3].tile, before[3].tile);
        assert_eq!(after[3].av, before[3].av);
        assert_eq!(text(&ui, after[3].tile, "string"), "Stat 3");
        assert_eq!(text(&ui, after[3].tile, "user1"), "4");
        assert_eq!(num(&ui, after[3].tile, "listindex"), 3.0);
        assert_eq!(num(&ui, after[3].tile, "height"), ROW);
        assert_eq!(num(&ui, after[3].tile, "_y"), 3.0 * ROW);
    }

    /// A row that lost its text is unloaded whole, leaving no root without its parts.
    #[test]
    fn a_row_without_its_text_is_made_again_whole() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let before = t.written.rows.clone();
        ui.unload_tile(before[2].text);
        ui.update();
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let list = paths(&ui, None).unwrap().list;
        assert_eq!(live_children(&ui, list), 7);
        assert!(!ui.is_live(before[2].tile));
        assert!(ui.is_live(t.written.rows[2].text));
    }

    /// Whatever a pass finds wrong, the reason, no row left and none doubled on the retry.
    fn falls_back(change: impl FnOnce(&str) -> String, reason: &str) {
        let (mut ui, mut v, mut t) = (tree_changed(change), view(), StatsTiles::default());
        for _ in 0..2 {
            let got = run(&mut ui, &snap(), &mut v, true, &mut t).unwrap_err();
            assert!(got.contains(reason), "{got}");
            let list = ui.menu(STATS_MENU).and_then(|m| find_named(&ui, m, LIST));
            assert!(list.is_none_or(|l| live_children(&ui, l) == 0 || reason.contains("rows")), "{reason}: rows left");
        }
    }

    #[test]
    fn a_cut_down_file_is_refused_with_its_reason() {
        falls_back(|x| x.replace(r#"template name="stats_list_template""#, r#"template name="other""#), "stats_list_template missing");
        falls_back(|x| x.replace("ListItemText", "Other"), "has no ListItemText");
        falls_back(|x| x.replace("stats_display_rect", "other"), "stats_display_rect missing");
        falls_back(|x| x.replace("stats_tailline_perks", "other"), "stats_tailline_perks missing");
        falls_back(|x| x.replace("stats_special_container", "other"), "stats_special_container missing");
        falls_back(|x| x.replace(r#"menu name="StatsMenu""#, r#"menu name="Other""#), "StatsMenu is not in the tree");
        // A list that does not hold its rows: no scrolling here.
        falls_back(|x| x.replace("<_number_of_visible_items>8<", "<_number_of_visible_items>5<"), "7 rows and the list shows 5");
    }

    #[test]
    fn the_test_aid_cuts_a_name_and_the_page_can_come_back() {
        let (mut ui, mut v) = (tree(), view());
        let mut t = StatsTiles { cut: Some(ROW_TEMPLATE.into()), ..Default::default() };
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).is_err());
        t.cut = Some(ICON.to_ascii_uppercase());
        assert_eq!(run(&mut ui, &snap(), &mut v, true, &mut t).unwrap_err(), format!("{ICON} missing"));
        t.cut = None;
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).is_ok(), "a failed pass is tried again");
    }

    #[test]
    fn the_cut_aid_reads_a_name_and_a_time() {
        assert_eq!(parse_cut("stats_icon"), ("stats_icon".to_string(), None));
        assert_eq!(parse_cut(" stats_icon @ 33.5 "), ("stats_icon".to_string(), Some(33.5)));
    }

    /// [design] A tab as `write_tabs` meets it, in numbers of this test's own: an inner rect
    /// repeating the tab's place, transparent, over words and a line that are not.
    #[test]
    fn a_tabs_inner_rect_neither_moves_it_twice_nor_hides_it() {
        let tab = r#"<hotrect name="stats_tailline_skills"><target>&true;</target><x>130</x><y>500</y><width>50</width><height>20</height><text name="TabWords"><string>Tab</string></text></hotrect>"#;
        let nested = r#"<hotrect name="stats_tailline_skills"><target>&true;</target><x>130</x><y>500</y><width>50</width><height>20</height>
          <rect name="stb_text_box"><target>&true;</target><x>130</x><y>500</y><width>50</width><height>20</height><alpha>0</alpha>
            <text name="InnerWords"><alpha>200</alpha><string>Tab</string></text>
            <rect name="InnerBox"><alpha>0</alpha><image name="InnerLine"><filename>a.dds</filename><width>50</width><height>2</height></image></rect>
          </rect></hotrect>"#;
        let (mut ui, mut v, mut t) = (tree_changed(|x| x.replace(tab, nested)), view(), StatsTiles::default());
        let menu = run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        let items = layout::layout(&ui, menu, &|_, _, _| (0.0, 0.0), true);
        let item = |name: &str| items.iter().find(|i| ui.tile(i.tile).name == name).unwrap();
        let words = item("InnerWords");
        assert_eq!((words.rect.left, words.rect.top), (130.0, 500.0), "at the tab's one place");
        assert_eq!(words.alpha, 200.0, "its own alpha, through the transparent rect");
        assert_eq!(item("InnerLine").alpha, 255.0);
        // It takes the pointer there, and not where the two places added would be.
        let at_tab = fnv_ui::input::hit(&ui, &[menu], (140.0, 505.0), &no_measure).unwrap();
        assert_eq!(stats_tile(&ui, &p, &t.written.rows, at_tab.tile), Some(StatsTile::Tab(2)));
        assert!(fnv_ui::input::hit(&ui, &[menu], (270.0, 1005.0), &no_measure).is_none());
        // The file works the place out again: put back on the next pass.
        let inner = p.tab_inners[2].unwrap();
        ui.set_trait(inner, "x", Value::Num(130.0));
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        assert_eq!(num(&ui, inner, "x"), 0.0);
    }

    #[test]
    fn a_row_past_the_canvas_is_no_usable_layout() {
        let (mut ui, mut v, mut t) = (tree_changed(|x| x.replace("<x>12</x><y>30</y>", "<x>12</x><y>750</y>")), view(), StatsTiles::default());
        let got = run(&mut ui, &snap(), &mut v, true, &mut t).unwrap_err();
        assert!(got.contains("does not fit"), "{got}");
    }

    #[test]
    fn a_page_off_the_canvas_or_unseen_or_untargeted_is_no_usable_layout() {
        for (change, reason) in [
            (("<x>12</x><y>30</y>", "<x>1020</x><y>30</y>"), "row 0 does not fit"),
            (("<x>70</x><y>500</y>", "<x>1020</x><y>500</y>"), "tab 1 does not fit"),
            ((r#"<rect name="Branch">"#, r#"<rect name="Branch"><alpha>0</alpha>"#), "row 0 is not shown"),
            ((r#"<hotrect name="TestRow"><target>&true;</target>"#, r#"<hotrect name="TestRow"><target>0</target>"#), "Row(0) does not take the pointer"),
            ((r#"<hotrect name="stats_tailline_skills"><target>&true;</target>"#, r#"<hotrect name="stats_tailline_skills"><target>0</target>"#), "Tab(2) does not take the pointer"),
        ] {
            let (mut ui, mut v, mut t) = (tree_changed(|x| x.replace(change.0, change.1)), view(), StatsTiles::default());
            let got = run(&mut ui, &snap(), &mut v, true, &mut t).unwrap_err();
            assert!(got.contains(reason), "{reason}: {got}");
        }
        // The tab showing need not take the pointer: a click on it does nothing.
        let (mut ui, mut v, mut t) = (tree_changed(|x| x.replace(r#"<hotrect name="stats_tailline_SPECIAL"><target>&true;</target>"#, r#"<hotrect name="stats_tailline_SPECIAL"><target>0</target>"#)), view(), StatsTiles::default());
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).is_ok());
    }

    /// With nothing in the model changed, what a script does to the tree is seen on the next
    /// final pass, and the page comes back when it is put right.
    #[test]
    fn what_a_script_moves_or_hides_is_seen_with_the_model_unchanged() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        let area = t.desc_area;
        ui.set_trait(p.list, "x", Value::Num(1020.0));
        ui.update();
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).unwrap_err().contains("does not fit"));
        ui.set_trait(p.list, "x", Value::Num(12.0));
        ui.update();
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).is_ok());
        // The row's target off: the same places, and the pointer's check runs again.
        let row = t.written.rows[1].tile;
        ui.set_trait(row, "target", Value::Num(0.0));
        ui.update();
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).unwrap_err().contains("Row(1)"));
        ui.set_trait(row, "target", Value::Num(1.0));
        ui.update();
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).is_ok());
        // The description hidden; then moved, and the wheel's area goes with it.
        ui.set_trait(p.desc, "visible", Value::Num(0.0));
        ui.update();
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).unwrap_err().contains("description"));
        ui.set_trait(p.desc, "visible", Value::Num(1.0));
        ui.set_trait(p.desc, "x", Value::Num(40.0));
        ui.update();
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).is_ok());
        assert_ne!(t.desc_area, area);
    }

    #[test]
    fn the_screen_menu_has_no_title_or_background_and_gives_them_back() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        run(&mut ui, &snap(), &mut v, false, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        let from = f64::from(crate::pipboy::frame::lead_line_end());
        assert_eq!(num(&ui, p.title, "visible"), 0.0);
        assert_eq!((num(&ui, p.line, "x"), num(&ui, p.line, "_length")), (from, 340.0 - from), "the line runs on from the lead line to where it ended");
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        assert_eq!(num(&ui, p.title, "visible"), 1.0);
        assert_eq!((num(&ui, p.line, "x"), num(&ui, p.line, "_length")), (300.0, 40.0));
    }

    /// Only page content goes into the image: its background is the shell's, in either frame,
    /// and a file without one serves as well.
    #[test]
    fn the_page_paints_no_background_of_its_own() {
        for device in [true, false] {
            let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
            run(&mut ui, &snap(), &mut v, device, &mut t).unwrap();
            let p = paths(&ui, None).unwrap();
            assert_eq!(num(&ui, p.background.unwrap(), "visible"), 0.0, "device {device}");
        }
        let (mut ui, mut v, mut t) = (tree_changed(|x| x.replace("background_layout", "other")), view(), StatsTiles::default());
        assert!(run(&mut ui, &snap(), &mut v, true, &mut t).is_ok());
    }

    #[test]
    fn only_an_unresolved_title_is_replaced() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        assert_eq!(text(&ui, p.title, "string"), "Words", "a replacement's words stand");
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        let title = paths(&ui, None).unwrap().title;
        ui.set_trait(title, "string", Value::Str("&sSomeName".into()));
        run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        assert_eq!(text(&ui, title, "string"), Page::Stats.title());
    }

    /// One step of the pointer on the canvas.
    fn at(ui: &mut Ui, pointer: &mut Pointer, menu: TileId, x: f64, y: f64, press: bool) -> fnv_ui::input::Report {
        pointer.step(ui, &[menu], &Input { at: Some((x, y)), press, ..Default::default() }, &no_measure)
    }

    #[test]
    fn canvas_points_hit_rows_tabs_and_the_description_controls() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        let menu = run(&mut ui, &snap(), &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        let rows = t.written.rows.clone();
        let mut pointer = Pointer::default();
        // Row 2: the list's 12 + 8 across, its 30 + two rows down.
        let r = at(&mut ui, &mut pointer, menu, 50.0, 30.0 + 2.5 * ROW, true);
        assert_eq!(stats_tile(&ui, &p, &rows, pointer.over.unwrap()), Some(StatsTile::Row(2)));
        assert_eq!(r.clicks.len(), 1);
        assert!(matches!(r.clicks[0].by, By::Pointer));
        assert_eq!(stats_tile(&ui, &p, &rows, r.clicks[0].tile), Some(StatsTile::Row(2)));
        // The tabs by their own numbers.
        for (i, x) in [20.0, 80.0, 140.0, 200.0, 260.0].into_iter().enumerate() {
            let r = at(&mut ui, &mut pointer, menu, x, 505.0, true);
            assert_eq!(r.clicks.iter().filter_map(|c| stats_tile(&ui, &p, &rows, c.tile)).next_back(), Some(StatsTile::Tab(i)));
        }
        // The bar's pieces, under the display's 400, 34, the window's 100 and the bar's 210.
        let (bx, by) = (400.0 + 210.0 + 5.0, 34.0 + 100.0);
        for (y, piece) in [(5.0, StatsTile::StepUp), (15.0, StatsTile::PageUp), (24.0, StatsTile::PageDown), (45.0, StatsTile::StepDown)] {
            let r = at(&mut ui, &mut pointer, menu, bx, by + y, true);
            assert_eq!(r.clicks.iter().filter_map(|c| stats_tile(&ui, &p, &rows, c.tile)).next_back(), Some(piece));
        }
        // Nothing there.
        at(&mut ui, &mut pointer, menu, 900.0, 700.0, false);
        assert_eq!(pointer.over, None);
    }

    #[test]
    fn the_marker_drags_within_the_bound() {
        let (mut ui, mut v, mut t) = (tree(), view(), StatsTiles::default());
        let mut s = snap();
        s.special[0].description = "a|b|c|d|e".into();
        run(&mut ui, &s, &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        // At value 0 the marker's top is the bar's, 134 down: pressed 2 under it.
        let drag = marker_grab(&ui, &p, 136.0).unwrap();
        assert_eq!(steps_under(&ui, drag, 136.0, 5), Some(0), "held where it stands");
        assert_eq!(steps_under(&ui, drag, 142.0, 5), Some(2), "6 lower at 3 a step");
        assert_eq!(steps_under(&ui, drag, 900.0, 5), Some(5));
        assert_eq!(steps_under(&ui, drag, 0.0, 5), Some(0));
    }

    #[test]
    fn the_pointer_selects_as_the_hand_built_list_does() {
        let mut acted = None;
        // A row under a pointer that has not moved keeps the selection.
        assert_eq!(follow_hover(&mut acted, Some(4), false), None);
        assert_eq!(acted, None);
        // Moved onto it: selected and boxed, once.
        assert_eq!(follow_hover(&mut acted, Some(4), true), Some(HoverAct::Select(4)));
        assert_eq!(follow_hover(&mut acted, Some(4), true), None, "a key's step while it stays there is not undone");
        // Off the rows: no box, once, so a key's box after that holds.
        assert_eq!(follow_hover(&mut acted, None, true), Some(HoverAct::BoxOff));
        assert_eq!(follow_hover(&mut acted, None, true), None);
    }

    /// A menu whose own `xdown` moves the highlight to a row and `xup` back to another. A
    /// physical press is the hand-built key's, which steps the list: the moved highlight must
    /// not select as well, on that frame or while it is held there. A script's tap has no
    /// hand-built step, so it selects, the row under a stationary pointer too.
    #[test]
    fn a_files_navigation_acts_once() {
        let change = |x: &str| x.replace(r#"<menu name="StatsMenu">"#, r#"<menu name="StatsMenu"><xdown><ref src="Row3" trait="mouseover"/></xdown><xup><ref src="Row1" trait="mouseover"/></xup>"#);
        let mut s = snap();
        s.special[1].name = "Row1".into();
        s.special[3].name = "Row3".into();
        let spot = (50.0, 30.0 + 1.5 * ROW);
        let (mut ui, mut v, mut t) = (tree_changed(change), view(), StatsTiles::default());
        let menu = run(&mut ui, &s, &mut v, true, &mut t).unwrap();
        let p = paths(&ui, None).unwrap();
        let rows = t.written.rows.clone();
        let mut pointer = Pointer::default();
        let mut step = |ui: &mut Ui, pads: &[&str]| pointer.step(ui, &[menu], &Input { at: Some(spot), pads: pads.iter().map(|k| k.to_string()).collect(), ..Default::default() }, &no_measure);
        // The pointer alone moves onto its own row: no key's doing.
        let r = step(&mut ui, &[]);
        assert_eq!(r.over.and_then(|o| o.1), Some(rows[1].tile));
        assert_eq!(keyed_row(&ui, &p, &rows, &r, &[]), None);
        // A physical Down: the highlight moves to the file's row, and nothing selects it.
        let r = step(&mut ui, &["xdown"]);
        assert_eq!(r.over.and_then(|o| o.1), Some(rows[3].tile), "the fixture's reference moved it");
        assert_eq!(keyed_row(&ui, &p, &rows, &r, &["xdown"]), None);
        // Held there, nothing further; the hover reads the row under the pointer.
        let r = step(&mut ui, &[]);
        assert_eq!(r.over, None);
        let mut acted = Some(1);
        assert_eq!(follow_hover(&mut acted, Some(1), true), None);
        // A script's tap back onto the row under the stationary pointer selects it, though
        // the hover has nothing new to say.
        let r = step(&mut ui, &["xup"]);
        assert_eq!(r.over.and_then(|o| o.1), Some(rows[1].tile));
        assert_eq!(keyed_row(&ui, &p, &rows, &r, &[]), Some(1));
        // And a tap away selects where it went.
        let r = step(&mut ui, &["xdown"]);
        assert_eq!(keyed_row(&ui, &p, &rows, &r, &[]), Some(3));
    }

    #[test]
    fn the_description_area_belongs_to_the_tab_and_frame_it_was_found_for() {
        let ui = tree();
        let area = Rect::new(1.0, 2.0, 3.0, 4.0);
        let t = StatsTiles { ready: ui.menu(STATS_MENU), desc_area: Some(area), desc_for: Some((PAGE, Frame::Device)), ..Default::default() };
        assert_eq!(t.desc_area_for(PAGE, Frame::Device), Some(area));
        assert_eq!(t.desc_area_for(2, Frame::Device), None, "the tab changed this frame");
        assert_eq!(t.desc_area_for(PAGE, Frame::Screen), None, "the frame changed this frame");
        let not_ready = StatsTiles { ready: None, ..t };
        assert_eq!(not_ready.desc_area_for(PAGE, Frame::Device), None);
    }

    #[test]
    fn a_physical_press_acts_once_and_a_script_or_a_file_only_binding_acts() {
        for key in ["S", "W", "A", "D", "xup", "xdown", "xleft", "xright"] {
            assert!(!menus::key_click_acts(STATS_MENU, key, &[key]), "{key} by hand");
            assert!(menus::key_click_acts(STATS_MENU, key, &[]), "{key} tapped by a script");
        }
        assert!(menus::key_click_acts(STATS_MENU, "E", &["E"]), "no hand-built use for it on this tab");
    }

    fn stats_page_up(tab: usize) -> PipBoy {
        let mut pip = PipBoy::default();
        pip.raise();
        pip.apply(PipBoyInput::Page(Page::Stats));
        pip.apply(PipBoyInput::Tab(tab));
        pip
    }

    /// Input acts on the page painted, the tab up, with nothing over it; only tab 1 is tiled,
    /// the switch off nothing takes it; a menu over the page has the frame.
    #[test]
    fn the_page_takes_input_only_while_painted_up_and_uncovered() {
        use fnv_game::game::menus as codes;
        let mut state = ScriptState::over_no_plugins();
        let game = &mut state.rt.as_mut().unwrap().game;
        game.ui_files = Some(Box::new(MemSource::default()));
        game.ui_tree = tree();
        game.menu = Some(codes::STATS);
        let menu = game.ui_tree.menu(STATS_MENU).unwrap();
        let on = TileMenus { stats: true, ..Default::default() };
        let pip = stats_page_up(PAGE);
        let mut t = StatsTiles::default();
        assert_eq!(takes(&t, on, &pip, game, false), None, "not painted yet");
        t.shown = Some(menu);
        assert_eq!(takes(&t, on, &pip, game, false), Some(menu));
        assert_eq!(takes(&t, TileMenus::default(), &pip, game, false), None, "the switch off");
        assert_eq!(takes(&t, on, &stats_page_up(2), game, false), None, "the other tabs stay hand-built");
        assert_eq!(takes(&t, on, &pip, game, true), None, "paused");
        assert_eq!(takes(&t, on, &PipBoy::default(), game, false), None, "down");
        game.menu = Some(codes::TUTORIAL);
        assert_eq!(takes(&t, on, &pip, game, false), None, "a menu over it");
        game.menu = None;
        game.ui_files = None;
        assert_eq!(takes(&t, on, &pip, game, false), None, "no menu files");
    }

    #[test]
    fn a_menu_over_the_page_is_told_from_the_pages_own_codes() {
        use fnv_game::game::menus as codes;
        assert!(modal_over(Some(codes::TUTORIAL)));
        assert!(modal_over(Some(codes::MESSAGE)));
        for own in [codes::STATS, codes::INVENTORY, codes::MAP] {
            assert!(!modal_over(Some(own)));
        }
        assert!(!modal_over(None));
    }

    #[test]
    fn only_tab_one_of_stats_is_eligible() {
        let on = TileMenus { stats: true, ..Default::default() };
        assert!(eligible(on, &stats_page_up(PAGE)));
        assert!(!eligible(TileMenus::default(), &stats_page_up(PAGE)));
        for tab in [0, 2, 3, 4] {
            assert!(!eligible(on, &stats_page_up(tab)));
        }
        let mut items = stats_page_up(PAGE);
        items.apply(PipBoyInput::Page(Page::Items));
        assert!(!eligible(on, &items));
        assert!(!eligible(on, &PipBoy::default()), "down");
    }

    #[test]
    fn what_is_shown_is_compared_without_a_copy_of_the_rows() {
        let s = snap();
        let shown = Shown::of(&s, 1, true);
        assert!(shown.same(&s, 1, true));
        assert!(!shown.same(&s, 2, true));
        assert!(!shown.same(&s, 1, false));
        let mut hurt = s.clone();
        hurt.hp = Some((10.0, 40.0));
        assert!(!shown.same(&hurt, 1, true));
        let mut other = s.clone();
        other.special[6].name = "Other".into();
        assert!(!shown.same(&other, 1, true));
    }
}
