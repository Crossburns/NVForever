//! The barter view behind `[UI] TileMenus=BarterMenu` [design: barter-menu.md section 9].
//! Commands, sounds and events remain `crate::menu`'s. Shared list state lives in
//! `super::item_menu`; its measured container numbers are hypotheses for barter until
//! compared with a barter capture. Off, this adapter does no work.

use super::item_menu::{self, on, set, MenuTile, MenuTiles, Paths, SIDES};
use super::menus::Tiled;
use crate::menu::{barter, model::{Category, MenuModel, Mode, Side}};
use crate::pipboy::items::Info;
use bevy::prelude::*;
use fnv_ui::{Ui, Value};

/// [documented: menus\barter_menu.xml] The menu and the paths written by this adapter.
pub const BARTER_MENU: &str = "BarterMenu";
const BACKGROUND: &str = "BarterMenu/NOGLOW_BRANCH/BM_Background";
const ICON: &str = "BarterMenu/NOGLOW_BRANCH/BM_ItemIcon";
const CARD: &str = "BarterMenu/NOGLOW_BRANCH/BM_ItemData";
const FLOW: &str = "BarterMenu/NOGLOW_BRANCH/BM_CapsFlow";
const FLOW_ICON: &str = "BarterMenu/NOGLOW_BRANCH/BM_CapsFlow/BM_CapsFlowIcon";
const AMOUNT: &str = "BarterMenu/NOGLOW_BRANCH/BM_CapsFlow/BM_TransactionAmount";
const CAPS: &str = "BarterMenu/NOGLOW_BRANCH/BM_CapsFlow/BM_CapsLabel";
const ACCEPT: &str = "BarterMenu/NOGLOW_BRANCH/BM_ButtonRect/BM_ButtonX";
const EXIT: &str = "BarterMenu/NOGLOW_BRANCH/BM_ButtonRect/BM_ButtonB";
const PURSES: [&str; 2] = ["BarterMenu/NOGLOW_BRANCH/BM_ContainerRect/BM_Container_CapsLabel", "BarterMenu/NOGLOW_BRANCH/BM_ItemsRect/BM_Items_CapsLabel"];
const PRICE: &str = "BM_list_template_ItemValue";
const MARK: &str = "BM_list_template_ItemMarker";

/// [documented: barter_menu.xml] Names only; the file supplies places and expressions.
const PATHS: Paths = Paths {
    menu: BARTER_MENU,
    lists: ["BarterMenu/NOGLOW_BRANCH/BM_ContainerRect/BM_Container_InventoryList", "BarterMenu/NOGLOW_BRANCH/BM_ItemsRect/BM_Items_InventoryList"],
    titles: ["BarterMenu/NOGLOW_BRANCH/BM_ContainerRect/BM_ContainerTitle", "BarterMenu/NOGLOW_BRANCH/BM_ItemsRect/BM_ItemsTitle"],
    template: "BM_list_template", row: "BM_list_template_container", row_text: "ListItemText", row_mark: MARK,
    targets: &[
        ("BM_ButtonX", MenuTile::Accept), ("BM_ButtonB", MenuTile::Exit),
        ("BM_Items_LeftFilterArrow", MenuTile::Arrow(Side::Player, -1)),
        ("BM_Items_RightFilterArrow", MenuTile::Arrow(Side::Player, 1)),
        ("BM_Container_LeftFilterArrow", MenuTile::Arrow(Side::Container, -1)),
        ("BM_Container_RightFilterArrow", MenuTile::Arrow(Side::Container, 1)),
        ("BM_ItemsTitle", MenuTile::Title(Side::Player)),
        ("BM_ContainerTitle", MenuTile::Title(Side::Container)),
    ],
};

/// [design: menu::menu_keys, pad::menu_buttons] Physical keys already handled by the model.
/// Script MenuTapKey still follows the file's `_PCButton_A` and `_PCButton_X`.
pub const HAND_BUILT_KEYS: [&str; 16] = ["A", "X", "W", "S", "xup", "xdown", "xleft", "xright", "xbuttona", "xbuttonb", "xbuttonx", "xbuttonlb", "xbuttonrb", "xbuttonlt", "xbuttonrt", "xbuttonstart"];

/// [design] A short or restructured file keeps the entire hand-built view.
pub fn built(ui: &Ui) -> Tiled {
    let mut paths: Vec<String> = [BARTER_MENU, BACKGROUND, ICON, CARD, FLOW, FLOW_ICON, AMOUNT, CAPS, ACCEPT, EXIT].into_iter().chain(PURSES).chain(PATHS.titles).map(str::to_owned).collect();
    for button in [ACCEPT, EXIT] {
        paths.extend(["button_text", "PCShortcutLabel"].map(|t| format!("{button}/{t}")));
    }
    for side in ["BM_ContainerRect/BM_Container", "BM_ItemsRect/BM_Items"] {
        for arrow in ["LeftFilterArrow", "RightFilterArrow"] {
            paths.push(format!("{BARTER_MENU}/NOGLOW_BRANCH/{side}_{arrow}"));
        }
    }
    for path in PATHS.lists {
        paths.push(path.into());
        paths.push(format!("{path}/{}", super::list::HIGHLIGHT));
        for piece in [super::list::MARKER, super::list::UP, super::list::DOWN, super::list::PAGE_UP, super::list::PAGE_DOWN] {
            paths.push(format!("{path}/{}/{piece}", super::list::SCROLLBAR));
        }
    }
    paths.extend(super::item_card::CARDS.iter().chain(&[super::item_card::EFFECTS, super::item_card::CONDITION_ARROWS, super::item_card::CONDITION_METER, super::item_card::CONDITION_METER_IMAGE]).map(|c| format!("{CARD}/{c}")));
    for card in super::item_card::CARDS.iter().chain(&[super::item_card::EFFECTS]) {
        paths.extend(super::item_card::CARD_TEXTS.map(|t| format!("{CARD}/{card}/{t}")));
    }
    item_menu::built(ui, &PATHS, paths, &[("text", PATHS.row_text), ("text", PRICE), ("image", MARK)])
}

pub fn is_barter(m: Option<&MenuModel>) -> bool {
    m.is_some_and(|m| matches!(m.mode, Mode::Barter(_)))
}

/// [design] This decision is shared by the writer, drawing and prompt writer.
pub fn tiled(on: bool, game: &fnv_game::Game<'_>, open: bool) -> bool {
    on && open && game.ui_files.is_some() && game.menu == Some(crate::script::MENU_BARTER) && built(&game.ui_tree).yes()
}

/// [design: LEGAL.md] Setting for all items; our own functional category words elsewhere.
fn title(category: Category, items: &str) -> String {
    match category {
        Category::All => items,
        Category::Weapons => "Weapons",
        Category::Apparel => "Clothing",
        Category::Aid => "Consumables",
        Category::Misc => "Other items",
        Category::Ammo => "Ammunition",
    }.into()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    common: item_menu::Snapshot,
    rows: [Vec<(String, bool)>; 2],
    purses: [String; 2],
    caps: String,
    hints: [String; 2],
    balance: i64,
    chosen: bool,
    affordable: bool,
}

impl Snapshot {
    /// [design] Read model figures, never `look::half_figure`'s transcribed caps heading.
    pub fn of(m: &MenuModel, info: Option<&Info>, items: String, caps: String, hints: [String; 2]) -> Self {
        let mut common = item_menu::Snapshot::of(m, info, None, String::new());
        common.titles = SIDES.map(|side| {
            if side == Side::Container && m.category[side as usize] == Category::All {
                m.title.clone()
            } else {
                title(m.category[side as usize], &items)
            }
        });
        let trade = m.trade.as_ref();
        Self {
            common,
            rows: SIDES.map(|side| m.list(side).rows.iter().map(|r| (r.price.map_or_else(|| "--".into(), barter::price_text), r.bartered)).collect()),
            purses: SIDES.map(|side| format!("{caps}  {}", trade.map_or(0, |t| t.purse[side as usize]))),
            caps, hints,
            balance: trade.map_or(0, |t| t.balance),
            chosen: trade.is_some_and(|t| !t.is_empty()),
            affordable: trade.is_none_or(|t| t.affordable()),
        }
    }
}

/// [design] A separate resource, with the same list implementation as the container.
#[derive(Resource)]
pub struct BarterTiles {
    pub(super) view: MenuTiles,
    shown: Option<Snapshot>,
}

impl Default for BarterTiles {
    fn default() -> Self { Self { view: MenuTiles::new(&PATHS), shown: None } }
}

impl BarterTiles {
    pub fn menu(&self) -> Option<fnv_ui::TileId> { self.view.menu }

    fn close(&mut self) {
        self.view.close();
        self.shown = None;
    }
}

/// [design] File places and textures stay in the tree. Numeric writes borrowed from the
/// container are hypotheses for barter (section 9); selection and affordability are the model's.
fn write(ui: &mut Ui, s: &Snapshot, st: &mut BarterTiles) {
    item_menu::write_lists(ui, &s.common, &mut st.view);
    for side in SIDES {
        let i = side as usize;
        set(ui, PURSES[i], "string", Value::Str(s.purses[i].clone()));
        for ((row, (price, bartered)), shown) in st.view.rows[i].iter().zip(&s.rows[i]).zip(&s.common.lists[i].rows) {
            if let Some(t) = super::list::child(ui, row.tile, PRICE) {
                ui.set_trait(t, "string", Value::Str(price.clone()));
            }
            if let Some(t) = super::list::child(ui, row.tile, MARK) {
                ui.set_trait(t, "visible", on(*bartered || shown.equipped));
                // [documented: barter-menu.md sections 1 and 3] Open for an offered item.
                ui.set_trait(t, "filename", Value::Str(if *bartered { "interface\\shared\\marker\\square.dds" } else { "interface\\shared\\marker\\square_filled.dds" }.into()));
            }
        }
    }
    set(ui, BARTER_MENU, "_IsContainerListSelected", on(s.common.focus == Side::Container));
    // [hypothesis] Direction 0 buys and 1 sells; named textures in section 1 make the
    // arrow explicit while the file uses the same direction for its placement.
    set(ui, BARTER_MENU, "_ArrowDirection", on(s.balance > 0));
    set(ui, FLOW, "visible", on(s.balance != 0));
    set(ui, FLOW, "_Value", Value::Num(s.balance.unsigned_abs() as f64));
    set(ui, AMOUNT, "string", Value::Str(s.balance.unsigned_abs().to_string()));
    set(ui, CAPS, "string", Value::Str(s.caps.clone()));
    set(ui, FLOW_ICON, "filename", Value::Str(if s.balance > 0 { "interface\\barter\\barter_arrow_sell.dds" } else { "interface\\barter\\barter_arrow_buy.dds" }.into()));
    // [design: owner's request, barter-menu.md section 5] Insufficient funds dim the flow.
    for path in [FLOW_ICON, AMOUNT, CAPS] {
        set(ui, path, "brightness", Value::Num(if s.affordable { 255.0 } else { 128.0 }));
    }
    set(ui, ACCEPT, "target", on(s.chosen));
    for (button, hint) in [ACCEPT, EXIT].into_iter().zip(&s.hints) {
        set(ui, button, "_PCButtonText", Value::Str(hint.clone()));
    }
    // [hypothesis] As with the container, the text inherits no parent alpha/brightness.
    for path in [ACCEPT.to_string(), format!("{ACCEPT}/button_text"), format!("{ACCEPT}/PCShortcutLabel")] {
        set(ui, &path, "brightness", Value::Num(if s.chosen { 255.0 } else { 128.0 }));
    }
    set(ui, ICON, "visible", on(s.common.card.is_some() && s.common.icon.is_some()));
    set(ui, ICON, "filename", Value::Str(s.common.icon.clone().unwrap_or_default()));
    super::item_card::write(ui, CARD, s.common.card.as_ref());
    // [hypothesis] Container backing measurement reused until the barter capture arrives.
    set(ui, BACKGROUND, "alpha", Value::Num(super::container_menu::BACKGROUND_ALPHA));
}

/// [design] The writer runs before input; only the enabled adapter reads item figures.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_system(
    mut script: ResMut<crate::script::ScriptState>, switch: Res<super::vanilla::TileMenus>,
    active: Res<crate::menu::ActiveMenu>, mut sim: ResMut<crate::sim::SimState>,
    (combat, play): (Option<Res<crate::combat::Combat>>, Option<Res<crate::PlayWorld>>),
    mut forms: Local<Option<crate::pipboy::stats::read::Forms>>, mut fonts: super::draw::FontKit,
    mut st: ResMut<BarterTiles>,
) {
    if !switch.barter { return; }
    let model = active.0.as_ref().filter(|_| is_barter(active.0.as_ref()));
    let Some(rt) = script.rt.as_ref() else { return };
    let game = &rt.game;
    if model.is_some() && game.ui_files.is_some() && game.menu == Some(crate::script::MENU_BARTER) && !st.view.warned {
        if let Tiled::No(reason) = built(&game.ui_tree) {
            warn!("{BARTER_MENU} drawn by hand: {reason}");
            st.view.warned = true;
        }
    }
    let Some(m) = model else { st.close(); return };
    st.view.menu = if tiled(true, game, true) { game.ui_tree.menu(BARTER_MENU) } else { None };
    let Some(menu) = st.menu() else { return };
    let due = active.is_changed() || st.view.needs_snapshot(menu);
    let words = due.then(|| {
        let word = |key| fnv_game::ui_loading::game_string(game, key);
        let pad = crate::pad::at_menu(crate::script::MENU_BARTER);
        (word("sInventoryItems"), word("sInventoryCaps"), [
            crate::pad::game_hint(&word("sPCMenuHintA"), &[KeyCode::KeyA], pad),
            crate::pad::game_hint(&word("sPCMenuHintX"), &[KeyCode::KeyX], pad),
        ])
    });
    let new = words.map(|(items, caps, hints)| {
        if forms.is_none() { *forms = play.as_ref().map(|p| crate::pipboy::stats::read::Forms::new(p.lo, p.schema)); }
        let info = m.selected_row(m.focus).map(|r| crate::pipboy::items::info_of(r, combat.as_deref(), sim.bypass_change_detection(), forms.as_mut(), Some(script.bypass_change_detection())));
        Snapshot::of(m, info.as_ref(), items, caps, hints)
    });
    let Some(rt) = script.rt.as_mut() else { return };
    let ui = &mut rt.game.ui_tree;
    let mut told = false;
    if let Some(s) = new {
        let changed_menu = st.view.needs_snapshot(menu);
        st.view.is_new(menu, &s.common);
        if changed_menu || st.shown.as_ref() != Some(&s) {
            write(ui, &s, &mut st);
            told = true;
            st.shown = Some(s);
        }
    }
    let texts = super::menus::texts(ui, menu);
    st.view.settle_shown(ui, &mut |ui, sizes| {
        let size = fonts.menu_measure(&super::menus::fonts_of(ui, &texts));
        super::menus::write_text_sizes_of(ui, &texts, sizes, &size)
    });
    mark_tiles(ui);
    if let (true, Some(s)) = (told, st.shown.as_ref()) {
        let picked = s.common.lists[s.common.focus as usize].selected;
        let selected = picked.and_then(|i| s.common.lists[s.common.focus as usize].rows.get(i));
        let price = picked.and_then(|i| s.rows[s.common.focus as usize].get(i)).map_or("--", |r| r.0.as_str());
        info!("tiles: {BARTER_MENU} drawn from tiles: player {} rows, merchant {} rows; balance {}; offered {}; affordable {}; headings {:?}; purses {:?}; selected {:?} price {:?}; figures: {}",
            s.common.lists[1].rows.len(), s.common.lists[0].rows.len(), s.balance, s.chosen, s.affordable,
            s.common.titles, s.purses, selected.map_or("none", |r| r.text.as_str()), price, super::item_card::describe(ui, CARD));
    }
}

pub fn mark_tiles(ui: &mut Ui) { item_menu::mark_tiles(ui, &PATHS); }

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::menu::model::RowKey;
    use fnv_sim::FormId;
    use fnv_ui::MemSource;

    /// [design] A fixture written for these tests, with its own dimensions and simple
    /// dependencies. No expressions or values were taken from the game menu file.
    pub(crate) fn fixture(change: impl FnOnce(String) -> String) -> Ui {
        let side = |prefix: &str, x: u32| format!(r#"
            <rect name="{prefix}Rect"><x>{x}</x>
              <text name="{prefix}Title"/><text name="{prefix}_CapsLabel"/>
              <image name="{prefix}_LeftFilterArrow"/><image name="{prefix}_RightFilterArrow"/>
              <hotrect name="{prefix}_InventoryList"><height>95</height><width>240</width>
                <clipwindow>1</clipwindow><_scroll_delta>13</_scroll_delta>
                <_offset><copy src="child(lb_scrollbar)" trait="_current_value"/><mul src="me()" trait="_scroll_delta"/></_offset>
                <hotrect name="lb_highlight_box"/>
                <image name="lb_scrollbar"><_current_value>0</_current_value>
                  <image name="scrollbar_vert_marker"/><image name="scrollbar_vert_up"/><image name="scrollbar_vert_down"/>
                  <hotrect name="scrollbar_vert_page_up"/><hotrect name="scrollbar_vert_page_down"/>
                </image>
              </hotrect>
            </rect>"#);
        let cards: String = super::super::item_card::CARDS.into_iter().chain([super::super::item_card::EFFECTS]).map(|card| {
            let extra = if card == "ConditionInfo" { r#"<rect name="ConditionMeter"><image name="MeterImage"/></rect><image name="CNDArrows"/>"# } else { "" };
            format!(r#"<rect name="{card}"><_Title>Own label</_Title><text name="CI_TitleText"/><text name="CI_ValueText"/>{extra}</rect>"#)
        }).collect();
        let xml = format!(r#"<menu name="BarterMenu"><rect name="NOGLOW_BRANCH">
          <image name="BM_Background"/>{}{}
          <rect name="BM_CapsFlow"><image name="BM_CapsFlowIcon"/><text name="BM_TransactionAmount"/><text name="BM_CapsLabel"/></rect>
          <rect name="BM_ButtonRect"><image name="BM_ButtonX"><text name="button_text"><string>Trade</string></text><text name="PCShortcutLabel"/></image>
          <image name="BM_ButtonB"><text name="button_text"><string>Leave</string></text><text name="PCShortcutLabel"/></image></rect>
          <image name="BM_ItemIcon"/><rect name="BM_ItemData"><user1>25</user1>{cards}</rect>
          </rect><template name="BM_list_template"><hotrect name="BM_list_template_container"><width>220</width><target>1</target>
          <_y>0</_y><_VerticalSpacing>8</_VerticalSpacing>
          <y><copy src="me()" trait="_y"/><sub src="parent()" trait="_offset"/></y>
          <text name="ListItemText"/><text name="BM_list_template_ItemValue"/><image name="BM_list_template_ItemMarker"/>
          </hotrect></template></menu>"#, side("BM_Items", 60), side("BM_Container", 460));
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("fixture.xml", &change(xml)), "fixture.xml").unwrap();
        ui.update();
        ui
    }

    pub(crate) fn snapshot() -> Snapshot {
        let row = |id, equipped| item_menu::RowShown {
            key: RowKey { units: (FormId(id), None, 0), reference: None, nth: 0 },
            name: format!("Item {id}"), text: format!("Item {id}"), equipped,
        };
        Snapshot {
            common: item_menu::Snapshot {
                lists: [item_menu::ListShown { rows: vec![row(1, false)], selected: None, ..Default::default() },
                    item_menu::ListShown { rows: vec![row(2, true), row(3, false), row(4, false), row(5, false)], selected: Some(0), ..Default::default() }],
                titles: ["Merchant".into(), "Personal items".into()], weight: String::new(), focus: Side::Player,
                pointer_pick: false, asking: false, navigated: 0,
                card: Some(Info { weight: Some("2.50".into()), value: Some("8".into()), ..Default::default() }),
                icon: None, take_all: false, take_all_hint: String::new(), note: None,
            },
            rows: [vec![("0.9".into(), true)], vec![("--".into(), false), ("18".into(), false), ("2".into(), false), ("3".into(), false)]],
            purses: ["Coins  200".into(), "Coins  40".into()], caps: "Coins".into(), hints: ["[A]".into(), "[X]".into()],
            balance: 18, chosen: true, affordable: true,
        }
    }

    pub(crate) fn frame(ui: &mut Ui, s: &Snapshot, st: &mut BarterTiles) {
        let menu = ui.menu(BARTER_MENU).unwrap();
        st.view.menu = Some(menu);
        st.view.is_new(menu, &s.common);
        write(ui, s, st);
        let texts = super::super::menus::texts(ui, menu);
        st.view.settle_shown(ui, &mut |ui, sizes| super::super::menus::write_text_sizes_of(ui, &texts, sizes, &super::super::list::lines_size));
        mark_tiles(ui);
    }

    pub(crate) fn of_model(m: &MenuModel) -> Ui {
        let mut ui = fixture(|s| s);
        let s = Snapshot::of(m, None, "Personal items".into(), "Coins".into(), ["[A]".into(), "[X]".into()]);
        frame(&mut ui, &s, &mut BarterTiles::default());
        ui
    }

    fn number(ui: &Ui, path: &str, name: &str) -> f64 { ui.get_float(&format!("{path}/{name}")).unwrap() }
    fn string(ui: &Ui, path: &str, name: &str) -> String { ui.get_string(&format!("{path}/{name}")).unwrap() }

    #[test]
    fn incomplete_file_or_price_template_falls_back_whole() {
        assert_eq!(built(&fixture(|s| s)), Tiled::Yes);
        for missing in ["BM_CapsFlowIcon", "BM_Container_CapsLabel", "BM_list_template_ItemValue", "CI_ValueText", "PCShortcutLabel", "scrollbar_vert_marker"] {
            let ui = fixture(|s| s.replace(missing, "Absent"));
            assert!(!built(&ui).yes(), "{missing}");
        }
        assert!(!built(&Ui::new()).yes());
    }

    #[test]
    fn prices_purses_marks_and_unit_card_are_written() {
        let (mut ui, mut st) = (fixture(|s| s), BarterTiles::default());
        frame(&mut ui, &snapshot(), &mut st);
        let offered = st.view.rows[0][0].tile;
        let equipped = st.view.rows[1][0].tile;
        let child = |tile, name| super::super::list::child(&ui, tile, name).unwrap();
        assert_eq!(ui.tile(child(offered, PRICE)).get("string").unwrap().text(), "0.9");
        assert_eq!(ui.tile(child(equipped, PRICE)).get("string").unwrap().text(), "--");
        assert_eq!(ui.tile(child(offered, MARK)).get("filename").unwrap().text(), "interface\\shared\\marker\\square.dds");
        assert_eq!(ui.tile(child(equipped, MARK)).get("filename").unwrap().text(), "interface\\shared\\marker\\square_filled.dds");
        assert!(ui.tile(child(offered, MARK)).get("visible").unwrap().truthy());
        assert_eq!(string(&ui, PURSES[1], "string"), "Coins  40");
        assert_eq!(string(&ui, CARD, "WeightInfo/_Value"), "2.50");
        assert_eq!(string(&ui, CARD, "ValueInfo/_Value"), "8");
        assert_eq!(string(&ui, CARD, "WeightInfo/_Title"), "Own label");
        assert_eq!(string(&ui, ACCEPT, "_PCButtonText"), "[A]");
        assert_eq!(string(&ui, EXIT, "_PCButtonText"), "[X]");
    }

    #[test]
    fn balance_direction_affordability_and_empty_accept_are_distinct() {
        let (mut ui, mut st, mut s) = (fixture(|s| s), BarterTiles::default(), snapshot());
        frame(&mut ui, &s, &mut st);
        assert_eq!(number(&ui, BARTER_MENU, "_ArrowDirection"), 1.0);
        assert!(string(&ui, FLOW_ICON, "filename").ends_with("barter_arrow_sell.dds"));
        s.balance = -37;
        s.affordable = false;
        s.common.focus = Side::Container;
        frame(&mut ui, &s, &mut st);
        assert_eq!(number(&ui, BARTER_MENU, "_ArrowDirection"), 0.0);
        assert_eq!(number(&ui, BARTER_MENU, "_IsContainerListSelected"), 1.0);
        assert_eq!(number(&ui, FLOW_ICON, "brightness"), 128.0);
        assert_eq!(string(&ui, AMOUNT, "string"), "37");
        assert_eq!(number(&ui, ACCEPT, "brightness"), 255.0, "a chosen trade stays bright even if unaffordable");
        s.balance = 0;
        frame(&mut ui, &s, &mut st);
        assert_eq!(number(&ui, FLOW, "visible"), 0.0);
        assert_eq!(number(&ui, ACCEPT, "target"), 1.0, "a zero-net trade can still be accepted");
        s.chosen = false;
        frame(&mut ui, &s, &mut st);
        assert_eq!(number(&ui, ACCEPT, "target"), 0.0);
        assert_eq!(number(&ui, ACCEPT, "button_text/brightness"), 128.0);
    }

    #[test]
    fn children_stand_for_actions_and_stale_rows_are_guarded() {
        let (mut ui, mut st) = (fixture(|s| s), BarterTiles::default());
        frame(&mut ui, &snapshot(), &mut st);
        for (path, what) in [(format!("{ACCEPT}/button_text"), MenuTile::Accept), (format!("{EXIT}/PCShortcutLabel"), MenuTile::Exit),
            (PATHS.titles[0].into(), MenuTile::Title(Side::Container)),
            ("BarterMenu/NOGLOW_BRANCH/BM_ItemsRect/BM_Items_RightFilterArrow".into(), MenuTile::Arrow(Side::Player, 1))] {
            assert_eq!(st.view.clicked(&ui, ui.find_tile(&path).unwrap()), Some(what));
        }
        let old = st.view.rows[1][0].tile;
        let mut s = snapshot();
        let mode = Mode::Barter(FormId(77));
        assert_eq!(st.view.guard(mode, Side::Player, 0, true, 1).unwrap().key, s.common.lists[1].rows[0].key);
        s.common.lists[1].rows.remove(0);
        s.rows[1].remove(0);
        assert_eq!(st.view.row_in(Side::Player, 0, &s.common.lists[1].rows), None);
        frame(&mut ui, &s, &mut st);
        assert!(!ui.is_live(old));
        assert_eq!(st.view.clicked(&ui, old), None);
        st.close();
        assert!(st.menu().is_none());
        assert_eq!(st.view.name(), BARTER_MENU, "close preserves the adapter's paths");
    }

    #[test]
    fn shared_scroll_survives_rewrite_and_pointer_picks_do_not_scroll() {
        let (mut ui, mut st, mut s) = (fixture(|s| s), BarterTiles::default(), snapshot());
        frame(&mut ui, &s, &mut st);
        assert!(st.view.scroll_to(Side::Player, 1.0));
        s.common.pointer_pick = true;
        s.common.lists[1].selected = Some(3);
        frame(&mut ui, &s, &mut st);
        assert_eq!(st.view.value(Side::Player), 1.0);
        let list = st.view.list_tile(&ui, Side::Player).unwrap();
        super::super::list::write_value(&mut ui, list, 0.0);
        st.view.rewrite(&mut ui);
        ui.update();
        assert_eq!(super::super::list::value_shown(&ui, list), Some(1.0));
        s.common.pointer_pick = false;
        s.common.navigated += 1;
        frame(&mut ui, &s, &mut st);
        assert!(st.view.value(Side::Player) > 1.0);
    }

    #[test]
    fn physical_buttons_act_once_and_scripts_keep_the_file_bindings() {
        for key in ["A", "X", "xbuttonx", "xbuttonb", "xbuttona", "xleft", "xbuttonrt"] {
            assert!(!super::super::menus::key_click_acts(BARTER_MENU, key, &[key]));
            assert!(super::super::menus::key_click_acts(BARTER_MENU, key, &[]));
        }
        assert_eq!(title(Category::All, "Personal items"), "Personal items");
        assert_eq!(title(Category::Apparel, "Personal items"), "Clothing");
    }

    #[test]
    fn prompt_follows_only_a_successful_barter_view_and_blocks_it() {
        let mut state = crate::script::ScriptState::over_no_plugins();
        let game = &mut state.rt.as_mut().unwrap().game;
        game.ui_files = Some(Box::new(MemSource::default()));
        game.menu = Some(crate::script::MENU_BARTER);
        game.ui_tree = fixture(|s| s);
        let root = game.ui_tree.menu(BARTER_MENU);
        assert!(tiled(true, game, true));
        assert!(!tiled(false, game, true));
        assert!(!tiled(true, game, false));
        assert_eq!(item_menu::takes_input(root, true), None);
        assert!(!super::super::menus::quantity_tiled(false, game, true, true), "a missing prompt stays hand-built");
        game.ui_tree.load_menu(&MemSource::default().with("prompt.xml", r#"<menu name="QuantityMenu"><rect name="NOGLOW_BRANCH"><image name="QM_Background"/><rect name="QM_MainRect"><rect name="QM_AmountMeter"/></rect></rect></menu>"#), "prompt.xml").unwrap();
        assert!(super::super::menus::quantity_tiled(false, game, true, tiled(true, game, true)));
        let caps = game.ui_tree.find_tile(CAPS).unwrap();
        game.ui_tree.unload_tile(caps);
        assert!(!tiled(true, game, true));
        assert!(!super::super::menus::quantity_tiled(false, game, true, tiled(true, game, true)));
    }
}
