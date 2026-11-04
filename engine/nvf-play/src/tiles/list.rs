//! T15c: the parts of a `list_box.xml` list a menu's model has to do itself: rows made from
//! a template and kept by what they stand for, their measured places, the scroll counts and
//! the viewport, the highlight, the scrollbar's pieces and the marker's drag. The menu's
//! rules stay outside it: a menu gives its rows' keys and strings, and is told what was
//! clicked (`super::container_menu`).
//!
//! What the prefab leaves to code [documented: `list_box.xml`, "Set in code"; observed: the
//! container capture, barter-menu.md §8]:
//!
//! - **The rows**: a row's `_y` is the sum of the heights above it; its `height` is its
//!   text's height and the template's `_VerticalSpacing`, unless the template gave one. The
//!   file itself moves a row up by `_current_value` × `_scroll_delta` (`list_box_template.xml`),
//!   so nothing here or in the layout subtracts the scroll again.
//! - **The counts** ([`counts`]): the scrollbar has `rows - (rows fitting whole from the
//!   top) + 1` items and 1 visible, and `_scroll_delta` is what is left over by the positions
//!   after the first; a list that fits has 1 item and keeps the file's `_scroll_delta`.
//! - **The highlight**: `_highlight_y` the selected row's place after the scroll,
//!   `_selected_height` its height; -1 and 0 with none, which the file reads as hidden.
//! - **The scroll**: a menu's own state ([`write_value`]). The prefab's `_current_value`
//!   sums the arrows' and page areas' `clicked` from 0 in this tree (an expression starts
//!   from 0, `fnv_ui`), so the value is written again on every frame after the clicks, and
//!   `_SetInCode` poked [documented: scrollbar_vert.xml], as the wait menu's slider.

use fnv_ui::{TileId, Ui, Value};

/// The scrollbar's and the row's names [documented: list_box.xml, scrollbar_vert.xml].
pub const SCROLLBAR: &str = "lb_scrollbar";
pub const HIGHLIGHT: &str = "lb_highlight_box";
pub const MARKER: &str = "scrollbar_vert_marker";
pub const UP: &str = "scrollbar_vert_up";
pub const DOWN: &str = "scrollbar_vert_down";
pub const PAGE_UP: &str = "scrollbar_vert_page_up";
pub const PAGE_DOWN: &str = "scrollbar_vert_page_down";

pub(super) fn child(ui: &Ui, tile: TileId, name: &str) -> Option<TileId> {
    ui.tile(tile).children.iter().copied().find(|&c| ui.is_live(c) && ui.tile(c).name.eq_ignore_ascii_case(name))
}

fn num(ui: &Ui, tile: TileId, name: &str) -> f64 {
    ui.tile(tile).get(name).map_or(0.0, Value::num)
}

/// A row the code made: what it stands for, and a height its template gave.
#[derive(Clone, Debug, PartialEq)]
pub struct RowTile<K> {
    pub tile: TileId,
    pub key: K,
    /// The template's own `height`, when not 0 [documented: list_box_template.xml,
    /// `_VerticalSpacing` "only used if `<height>` == 0"].
    pub fixed: Option<f64>,
}

/// What [`reconcile`] needs of a row: what it stands for and its tile's name.
pub struct Wanted<'a, K> {
    pub key: &'a K,
    /// [observed: a row's tile is named after its item, `GetActiveUIComponentName`
    /// reads it in the capture].
    pub name: &'a str,
}

/// Make `rows` the rows `wanted`, in order: a tile already standing for a key stays that
/// key's, so what a click queued earlier meant stays true; a key gone unloads its tile
/// (the click on it is then on a dead tile), a new key gets a tile from `template` under
/// `list`. `listindex` is written [documented: the template's `visible`]. Returns false when
/// a tile could not be made.
pub fn reconcile<K: PartialEq + Clone>(ui: &mut Ui, list: &str, template: &str, rows: &mut Vec<RowTile<K>>, wanted: &[Wanted<'_, K>]) -> bool {
    let mut had = std::mem::take(rows);
    had.retain(|r| ui.is_live(r.tile));
    let mut ok = true;
    for (i, w) in wanted.iter().enumerate() {
        let row = match had.iter().position(|r| r.key == *w.key) {
            Some(at) => had.remove(at),
            None => {
                // The template's includes were spliced in when the file was read, so no
                // files are needed to make a row (`super::menus::write_message`).
                let Some(tile) = ui.add_code_tile_from_template(&fnv_ui::MemSource::default(), list, template, Some(w.name)) else {
                    ok = false;
                    continue;
                };
                let fixed = Some(num(ui, tile, "height")).filter(|h| *h > 0.0);
                RowTile { tile, key: w.key.clone(), fixed }
            }
        };
        ui.set_trait(row.tile, "listindex", Value::Num(i as f64));
        rows.push(row);
    }
    for r in had {
        ui.unload_tile(r.tile);
    }
    ok
}

/// The rows' heights from their text tiles `text` ([`write_text_sizes`] wrote each size, at
/// its final wrap width): the template's height, else the text's and `_VerticalSpacing`. Each
/// row's `height` and `_y` are written. Returns the heights.
///
/// [`write_text_sizes`]: super::menus::write_text_sizes
pub fn measure_rows<K>(ui: &mut Ui, rows: &[RowTile<K>], text: &str) -> Vec<f64> {
    let mut y = 0.0;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let measured = child(ui, r.tile, text).map_or(0.0, |t| num(ui, t, "height"));
        let height = r.fixed.unwrap_or(measured + num(ui, r.tile, "_VerticalSpacing"));
        ui.set_trait(r.tile, "height", Value::Num(height));
        ui.set_trait(r.tile, "_y", Value::Num(y));
        out.push(height);
        y += height;
    }
    out
}

/// The scrollbar's counts and the scroll step of a list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Counts {
    pub items: f64,
    pub visible: f64,
    /// Units a value of the scrollbar moves the rows; `None`: the file's own.
    pub delta: Option<f64>,
}

impl Counts {
    /// The highest value of the scrollbar [documented: scrollbar_vert.xml's `_current_value`].
    pub fn most(&self) -> f64 {
        (self.items - self.visible).max(0.0)
    }
}

/// The counts of rows `heights` in a list `page` high [observed: three lists of the container
/// capture, 15 rows 885 high: 10 items, delta 57.78; 16 rows 937 high: 11, 57.20, twice;
/// lists that fit: 1 item and the file's 26; design: a value's rows are the ones whole from
/// the top, as those numbers read].
pub fn counts(heights: &[f64], page: f64) -> Counts {
    let total: f64 = heights.iter().sum();
    if total <= page {
        return Counts { items: 1.0, visible: 1.0, delta: None };
    }
    let mut fit = 0;
    let mut y = 0.0;
    for h in heights {
        y += h;
        if y > page {
            break;
        }
        fit += 1;
    }
    // A first row taller than the list still scrolls.
    let positions = (heights.len() - fit).max(1);
    Counts { items: positions as f64 + 1.0, visible: 1.0, delta: Some((total - page) / positions as f64) }
}

/// The scroll to show a row whose top is `top` and height `height`, moved as little as keeps
/// it whole: `value` when it already shows whole. A row taller than the list shows its top.
pub fn keep_row_shown(value: f64, top: f64, height: f64, page: f64, delta: f64, most: f64) -> f64 {
    if delta <= 0.0 {
        return 0.0;
    }
    let mut v = value;
    if (top + height) - v * delta > page {
        v = ((top + height - page) / delta - 1e-9).ceil();
    }
    if top - v * delta < 0.0 {
        v = (top / delta + 1e-9).floor();
    }
    v.clamp(0.0, most.max(0.0))
}

/// Write the counts [`counts`] and the file's `_scroll_delta`, which a list that fits gets
/// back from `file_delta` (what the tile had when no code had written it).
pub fn write_counts(ui: &mut Ui, list: TileId, c: &Counts, file_delta: f64) {
    ui.set_trait(list, "_scroll_delta", Value::Num(c.delta.unwrap_or(file_delta)));
    if let Some(bar) = child(ui, list, SCROLLBAR) {
        ui.set_trait(bar, "_number_of_items", Value::Num(c.items));
        ui.set_trait(bar, "_number_of_visible_items", Value::Num(c.visible));
    }
}

/// Write the scrollbar's value and poke `_SetInCode` [documented: scrollbar_vert.xml].
pub fn write_value(ui: &mut Ui, list: TileId, value: f64) {
    if let Some(bar) = child(ui, list, SCROLLBAR) {
        ui.set_trait(bar, "_current_value", Value::Num(value));
        ui.set_trait(bar, "_SetInCode", Value::Num(1.0));
    }
}

/// Write the highlight: `selected` the row's top and height before the scroll, none hiding it
/// (`_highlight_y` -1, `_selected_height` 0) [observed: the original's list without a
/// selection and the one with, and -23.11 for a row at 208 with value 4 of delta 57.78].
pub fn write_highlight(ui: &mut Ui, list: TileId, selected: Option<(f64, f64)>, value: f64) {
    let delta = num(ui, list, "_scroll_delta");
    let (y, h) = selected.map_or((-1.0, 0.0), |(top, height)| (top - value * delta, height));
    ui.set_trait(list, "_highlight_y", Value::Num(y));
    ui.set_trait(list, "_selected_height", Value::Num(h));
}

/// The scrollbar's value in the tree, once worked out: what is drawn.
pub fn value_shown(ui: &Ui, list: TileId) -> Option<f64> {
    child(ui, list, SCROLLBAR).and_then(|b| ui.tile(b).get("_current_value")).map(Value::num)
}

/// What a click on a scrollbar's piece does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarPiece {
    Up,
    Down,
    PageUp,
    PageDown,
    Marker,
}

/// The scrollbar piece `tile` is, by its name.
pub fn bar_piece(ui: &Ui, tile: TileId) -> Option<BarPiece> {
    let name = ui.tile(tile).name.to_ascii_lowercase();
    Some(match name.as_str() {
        UP => BarPiece::Up,
        DOWN => BarPiece::Down,
        PAGE_UP => BarPiece::PageUp,
        PAGE_DOWN => BarPiece::PageDown,
        MARKER => BarPiece::Marker,
        _ => return None,
    })
}

/// The value a click on a piece leads to from `value`: the file's `_step_size` for the
/// arrows, its `_jump_size` for the page areas [documented: scrollbar_vert.xml, list_box.xml;
/// observed: a page click from 4 to the end at 9, then an arrow to 8, a page to 2]. Whole
/// values, held in the bar's range. The marker drags instead ([`marker_grab`]).
pub fn after_piece(ui: &Ui, list: TileId, piece: BarPiece, value: f64) -> Option<f64> {
    let bar = child(ui, list, SCROLLBAR)?;
    let (step, jump) = (num(ui, bar, "_step_size"), num(ui, bar, "_jump_size"));
    let by = match piece {
        BarPiece::Up => -step,
        BarPiece::Down => step,
        BarPiece::PageUp => -jump,
        BarPiece::PageDown => jump,
        BarPiece::Marker => return None,
    };
    let most = (num(ui, bar, "_number_of_items") - num(ui, bar, "_number_of_visible_items")).max(0.0);
    Some((value + by).clamp(0.0, most))
}

/// What a press on a marker holds while it drags: the menu and list it was made in, the
/// bar's top and how far under the marker's top the pointer took it (interface units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub menu: TileId,
    pub list: TileId,
    pub top: f64,
    pub grab: f64,
}

/// Whether a drag goes on: the press held and that very menu still drawn from its tiles and
/// taking input (`now`); a menu closed or built again ends it, on any frame.
pub fn drag_holds(drag: &Drag, now: Option<TileId>, held: bool) -> bool {
    held && now == Some(drag.menu)
}

/// The [`Drag`] of a press on the marker of `list`, in `menu`.
pub fn marker_grab(ui: &Ui, menu: TileId, list: TileId, pointer_y: f64) -> Option<Drag> {
    let bar = child(ui, list, SCROLLBAR)?;
    let marker = child(ui, bar, MARKER)?;
    let placed = super::layout::layout(ui, marker, &|_, _, _| (0.0, 0.0), true).into_iter().find(|i| i.tile == marker)?.rect;
    Some(Drag { menu, list, top: placed.top - num(ui, marker, "y"), grab: pointer_y - placed.top })
}

/// The value for the pointer at `pointer_y` with the marker held: the marker's place over the
/// file's `_step_height`, whole values [documented: scrollbar_vert.xml's `_drag_value`; design:
/// the marker's own drag traits are not built, as the wait menu's slider].
pub fn value_under(ui: &Ui, drag: Drag, pointer_y: f64) -> Option<f64> {
    let bar = child(ui, drag.list, SCROLLBAR)?;
    let marker = child(ui, bar, MARKER)?;
    let step = num(ui, marker, "_step_height");
    if step <= 0.0 {
        return None;
    }
    let most = (num(ui, bar, "_number_of_items") - num(ui, bar, "_number_of_visible_items")).max(0.0);
    // A hair over, so a marker held where it stands gives its own value back.
    Some(((pointer_y - drag.grab - drag.top) / step + 1e-9).floor().clamp(0.0, most))
}

/// Text heights by a list's rows' strings, for a test: as many lines as `|` splits.
#[cfg(test)]
pub(super) fn lines_size(_: f64, text: &str, wrap: Option<f64>) -> (f64, f64) {
    let n = text.split('|').count();
    (wrap.unwrap_or(100.0).min(100.0), 32.0 + 35.0 * (n as f64 - 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    const LIST: &str = "M/R/L";
    const XML: &str = r#"<menu name="M"><rect name="R"><x>100</x><y>100</y><hotrect name="L"><width>462</width><height>365</height>
      <_scroll_delta>26</_scroll_delta><_highlight_y>-1</_highlight_y><_selected_height>0</_selected_height><clipwindow>&true;</clipwindow>
      <hotrect name="lb_highlight_box"><y><copy src="parent()" trait="_highlight_y"/></y></hotrect>
      <image name="lb_scrollbar"><_height><copy src="parent()" trait="height"/></_height>
        <_step_size>1</_step_size><_SetInCode></_SetInCode>
        <_number_of_items>1</_number_of_items><_number_of_visible_items>1</_number_of_visible_items>
        <_jump_size><copy src="parent()" trait="height"/><div src="parent()" trait="_scroll_delta"/><floor>0</floor></_jump_size>
        <_current_value>
          <add><copy src="child(scrollbar_vert_down)" trait="clicked"/><sub src="child(scrollbar_vert_up)" trait="clicked"/></add>
          <min><copy src="me()" trait="_number_of_items"/><sub src="me()" trait="_number_of_visible_items"/></min><max>0</max>
        </_current_value>
        <y>16</y><height>333</height>
        <image name="scrollbar_vert_marker"><height>32</height>
          <_step_height><copy src="parent()" trait="height"/><sub src="me()" trait="height"/><div>10</div></_step_height>
          <y><copy src="parent()" trait="_current_value"/><mul src="me()" trait="_step_height"/></y></image>
        <image name="scrollbar_vert_up"/><image name="scrollbar_vert_down"/>
      </image></hotrect></rect>
      <template name="T"><hotrect name="Row"><height>0</height><_y></_y><_VerticalSpacing>20</_VerticalSpacing>
        <y><copy src="me()" trait="_y"/><sub><copy src="sibling(lb_scrollbar)" trait="_current_value"/><mul src="parent()" trait="_scroll_delta"/></sub></y>
        <text name="ListItemText"><string></string></text></hotrect></template>
    </menu>"#;

    fn menu() -> (Ui, TileId) {
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("m.xml", XML), "m.xml").unwrap();
        ui.update();
        let list = ui.find_tile(LIST).unwrap();
        (ui, list)
    }

    fn wanted(keys: &[u32]) -> Vec<(u32, String)> {
        keys.iter().map(|k| (*k, format!("Item {k}"))).collect()
    }

    fn sync(ui: &mut Ui, rows: &mut Vec<RowTile<u32>>, keys: &[u32]) {
        let names = wanted(keys);
        let w: Vec<Wanted<'_, u32>> = names.iter().map(|(key, name)| Wanted { key, name }).collect();
        assert!(reconcile(ui, LIST, "T", rows, &w));
    }

    #[test]
    fn rows_keep_identity_across_refresh() {
        let (mut ui, list) = menu();
        let mut rows = Vec::new();
        sync(&mut ui, &mut rows, &[1, 2, 3]);
        let (a, b, c) = (rows[0].tile, rows[1].tile, rows[2].tile);
        assert_eq!(ui.tile(a).name, "Item 1", "named after its item");
        // Row 1 goes, 4 comes in front: 2 and 3 keep their tiles, at new places.
        sync(&mut ui, &mut rows, &[4, 2, 3]);
        assert_eq!((rows[1].tile, rows[2].tile), (b, c));
        assert_eq!(ui.tile(b).get("listindex").map(Value::num), Some(1.0));
        assert!(!ui.is_live(a), "the tile of an item gone is unloaded");
        assert_ne!(rows[0].tile, a);
        ui.update();
        assert_eq!(ui.tile(list).children.iter().filter(|&&c| ui.tile(c).get("listindex").is_some()).count(), 3);
    }

    #[test]
    fn removed_row_click_is_ignored() {
        // A queued click holds a row's tile: when its item is gone the tile is not live, even
        // when a later row takes its slot.
        let (mut ui, _) = menu();
        let mut rows = Vec::new();
        sync(&mut ui, &mut rows, &[1, 2]);
        let clicked = rows[1].tile;
        sync(&mut ui, &mut rows, &[1]);
        sync(&mut ui, &mut rows, &[1, 9]);
        assert!(!ui.is_live(clicked));
        assert!(rows.iter().all(|r| r.tile != clicked));
    }

    #[test]
    fn wrapped_rows_define_scroll_extent() {
        // The original's left list: 15 rows, three of two lines... 885 high, value 4 at -231.11.
        let h: Vec<f64> = [52, 52, 52, 52, 87, 52, 52, 52, 52, 52, 52, 87, 87, 52, 52].iter().map(|&x| f64::from(x)).collect();
        let c = counts(&h, 365.0);
        assert_eq!((c.items, c.visible), (10.0, 1.0));
        assert!((c.delta.unwrap() - 57.777).abs() < 0.01);
        let r: Vec<f64> = [52, 52, 52, 87, 52, 52, 52, 52, 52, 52, 52, 52, 52, 52, 52, 122].iter().map(|&x| f64::from(x)).collect();
        let c = counts(&r, 365.0);
        assert_eq!(c.items, 11.0);
        assert!((c.delta.unwrap() - 57.2).abs() < 1e-9);
        // The rows' places follow the measure.
        let (mut ui, list) = menu();
        let mut rows = Vec::new();
        sync(&mut ui, &mut rows, &[1, 2, 3]);
        for (r, s) in rows.iter().zip(["one", "two|lines", "three"]) {
            let t = child(&ui, r.tile, "ListItemText").unwrap();
            ui.set_trait(t, "string", Value::Str(s.into()));
            ui.set_trait(t, "height", Value::Num(lines_size(2.0, s, None).1));
        }
        let heights = measure_rows(&mut ui, &rows, "ListItemText");
        assert_eq!(heights, [52.0, 87.0, 52.0]);
        ui.update();
        assert_eq!(ui.tile(rows[2].tile).get("_y").map(Value::num), Some(139.0));
        let c = counts(&heights, 100.0);
        write_counts(&mut ui, list, &c, 26.0);
        write_value(&mut ui, list, 1.0);
        ui.update();
        // The file moves the row: 139 less one delta of (191 - 100) / 2.
        assert_eq!(ui.tile(rows[2].tile).get("y").map(Value::num), Some(139.0 - 45.5));
    }

    #[test]
    fn a_template_height_is_kept() {
        let xml = XML.replace("<height>0</height><_y>", "<height>40</height><_y>");
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("m.xml", &xml), "m.xml").unwrap();
        ui.update();
        let mut rows = Vec::new();
        sync(&mut ui, &mut rows, &[1]);
        assert_eq!(rows[0].fixed, Some(40.0));
        assert_eq!(measure_rows(&mut ui, &rows, "ListItemText"), [40.0]);
    }

    #[test]
    fn hover_does_not_scroll() {
        // Selected by the pointer, a partial bottom row stays where it is: the view moves only
        // through this call, which the model's pointer pick does not make; called for a key's
        // pick it shows the row whole.
        let v = keep_row_shown(0.0, 347.0, 52.0, 365.0, 57.78, 9.0);
        assert_eq!(v, 1.0, "a key's pick: moved as little as shows it whole");
        assert_eq!(keep_row_shown(4.0, 0.0, 52.0, 365.0, 57.78, 9.0), 0.0, "up to a row above");
        assert_eq!(keep_row_shown(2.0, 208.0, 87.0, 365.0, 57.78, 9.0), 2.0, "already whole: no move");
        assert_eq!(keep_row_shown(0.0, 10.0, 900.0, 365.0, 10.0, 90.0), 1.0, "too tall: its top");
    }

    #[test]
    fn scroll_survives_pulse_release() {
        // A value written after the click's pulse and on its end stays the model's, though the
        // file's own sum counts from 0.
        let (mut ui, list) = menu();
        write_counts(&mut ui, list, &Counts { items: 10.0, visible: 1.0, delta: Some(57.0) }, 26.0);
        write_value(&mut ui, list, 5.0);
        ui.update();
        let down = ui.find_tile(&format!("{LIST}/lb_scrollbar/scrollbar_vert_down")).unwrap();
        ui.set_trait(down, "clicked", Value::Num(1.0));
        write_value(&mut ui, list, 6.0);
        ui.update();
        assert_eq!(value_shown(&ui, list), Some(6.0), "the click's frame");
        ui.set_trait(down, "clicked", Value::Num(0.0));
        write_value(&mut ui, list, 6.0);
        ui.update();
        assert_eq!(value_shown(&ui, list), Some(6.0), "the pulse's end");
        ui.update();
        assert_eq!(value_shown(&ui, list), Some(6.0), "a frame later");
    }

    #[test]
    fn drag_clamps_at_both_ends() {
        let (mut ui, list) = menu();
        write_counts(&mut ui, list, &Counts { items: 11.0, visible: 1.0, delta: Some(57.0) }, 26.0);
        write_value(&mut ui, list, 0.0);
        ui.update();
        let menu = ui.menu("M").unwrap();
        // The bar's top at 100 + 16, the marker's at its top; a press 5 under it.
        let drag = marker_grab(&ui, menu, list, 121.0).unwrap();
        assert_eq!((drag.top, drag.grab), (116.0, 5.0));
        assert_eq!(value_under(&ui, drag, 121.0), Some(0.0), "where it stands");
        assert_eq!(value_under(&ui, drag, 121.0 + 3.0 * 30.1), Some(3.0));
        assert_eq!(value_under(&ui, drag, -500.0), Some(0.0));
        assert_eq!(value_under(&ui, drag, 5000.0), Some(10.0));
        // A menu closed or built again ends it.
        assert!(drag_holds(&drag, Some(menu), true));
        assert!(!drag_holds(&drag, None, true));
        assert!(!drag_holds(&drag, Some(menu), false));
    }

    #[test]
    fn empty_and_filtered_lists_clamp() {
        let c = counts(&[], 365.0);
        assert_eq!((c.items, c.most(), c.delta), (1.0, 0.0, None));
        let (mut ui, list) = menu();
        write_counts(&mut ui, list, &c, 26.0);
        assert_eq!(ui.tile(list).get("_scroll_delta").map(Value::num), Some(26.0), "the file's own again");
        assert_eq!(keep_row_shown(9.0, 0.0, 52.0, 365.0, 26.0, c.most()), 0.0);
        write_highlight(&mut ui, list, None, 0.0);
        ui.update();
        assert_eq!(ui.tile(list).get("_highlight_y").map(Value::num), Some(-1.0));
        assert_eq!(ui.tile(list).get("_selected_height").map(Value::num), Some(0.0));
        // Pieces hold the value in the range.
        write_counts(&mut ui, list, &Counts { items: 10.0, visible: 1.0, delta: Some(58.0) }, 26.0);
        ui.update();
        assert_eq!(after_piece(&ui, list, BarPiece::PageDown, 4.0), Some(9.0), "6 a page, held at the end");
        assert_eq!(after_piece(&ui, list, BarPiece::Up, 9.0), Some(8.0));
        assert_eq!(after_piece(&ui, list, BarPiece::PageUp, 8.0), Some(2.0));
        assert_eq!(after_piece(&ui, list, BarPiece::Marker, 8.0), None);
        // The highlight follows the scroll.
        write_highlight(&mut ui, list, Some((208.0, 87.0)), 4.0);
        assert_eq!(ui.tile(list).get("_highlight_y").map(Value::num), Some(208.0 - 232.0));
    }
}
