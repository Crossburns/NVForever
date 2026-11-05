//! Startup on files written for these tests, without Bevy or game data [design].

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    ui_loading::{self, UiFiles, ALWAYS_LOADED},
    Game, GameConfig, GameData,
};
use fnv_script::FunctionTable;
use fnv_sim::{settings::Setting, FormStore, RefTable, SimConfig, World};
use fnv_ui::{MemSource, Source, Value};
use fnv_vfs::Vfs;
use std::{path::Path, sync::Arc};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let gmst = record(b"GMST", 0, 0x800, &[sub(b"EDID", z("sProbe")), sub(b"DATA", z("plugin text"))]);
    let plugins = vec![Plugin::parse(&[tes4(), top(b"GMST", &gmst)].concat(), "Empty.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn files() -> MemSource {
    let mut src = MemSource::default()
        .with("menus/globals.xml", r#"<rect name="Strings"><_Probe>17</_Probe></rect>"#)
        .with("menus/prefabs/probe.xml", r#"<text name="Label"><string>&-sProbe;</string></text></wrong>"#);
    src.strings.insert("sProbe".into(), "fixture text".into());
    for (i, name) in ALWAYS_LOADED.iter().enumerate() {
        let include = if i % 2 == 0 { "probe.xml" } else { "PROBE.XML" };
        src = src.with(
            &format!("menus/added/{i}.xml"),
            &format!(
                r#"<menu name="{name}">
            <class>&{name};</class><depth><copy src="globals()" trait="_Probe"/></depth><include src="{include}"/>
            </menu>"#
            ),
        );
    }
    src
}

// These menus and their trait values are written for the tests [design]. Class codes
// come from fnv_ui::value::MENU_CLASSES [documented: Hardcoded UI Shortcuts, revision 39903].
fn lifetime_files() -> MemSource {
    files()
        .with(
            "menus/added/trade.xml",
            r#"<menu name="Trade"><class>&BarterMenu;</class><depth>42</depth><rect name="Row"><depth><copy src="parent()" trait="depth"/></depth></rect></menu>"#,
        )
        .with("menus/added/box.xml", r#"<menu name="MessageMenu"><class>&MessageMenu;</class><depth>7</depth></menu>"#)
        .with("menus/added/start.xml", r#"<menu name="StartMenu"><class>&StartMenu;</class><depth>8</depth></menu>"#)
        .with("menus/added/dialog.xml", r#"<menu name="DialogMenu"><class>&DialogMenu;</class></menu>"#)
}

fn load_lifetime(g: &mut Game<'_>) {
    let src = lifetime_files();
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(g, Box::new(src), paths);
}

#[test]
fn uio_uses_active_plugins_and_survives_menu_lifetime() {
    // [design] Synthetic lists exercise the real load/sync route, without installed mods.
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let src = lifetime_files()
        .with(
            "uio/public/fixture.txt",
            "added.xml::HUDMainMenu\n?EMPTY.ESM && #added.xml\nadded.xml::StartMenu\n?Empty.esm\nadded.xml::InventoryMenu\n?Disabled.esp",
        )
        .with_loose("menus/prefabs/added.xml", "<rect name=\"Added\"/>");
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    let report = ui_loading::load(&mut g, Box::new(src), paths);
    let hud = g.ui_tree.find_tile("HUDMainMenu/Added").unwrap();
    assert!(g.ui_tree.mod_tile_drawn(hud, None));
    assert!(g.ui_tree.find_tile("InventoryMenu/Added").is_none());
    assert!(report.warnings.values().flatten().any(|w| w.contains("condition false")));
    let report = ui_loading::sync(&mut g, true, false);
    assert!(g.ui_tree.find_tile("StartMenu/Added").is_some());
    assert!(report.changes.iter().any(|w| w.contains("UIO") && w.contains("applied")));
    ui_loading::sync(&mut g, false, false);
    assert!(g.ui_tree.find_tile("StartMenu/Added").is_none());
    ui_loading::sync(&mut g, true, false);
    assert!(g.ui_tree.find_tile("StartMenu/Added").is_some());
    assert_eq!(g.ui_tree.uio.decisions[1].applied, 2);
}

#[test]
fn open_close_and_inject_again() {
    use fnv_game::game::menus;
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    load_lifetime(&mut g);
    assert!(g.ui_tree.menu("Trade").is_none());
    g.menu = Some(menus::BARTER);
    let report = ui_loading::sync(&mut g, false, false);
    assert_eq!(report.changes, ["built BarterMenu (menus\\added\\trade.xml, 2 tiles)"]);
    assert!(report.warnings.is_empty());
    let root = g.ui_tree.menu("Trade").unwrap();
    assert_eq!(g.ui_tree.tile(root).get("depth"), Some(&Value::Num(42.0)));
    let row = g.ui_tree.find_tile("Trade/Row").unwrap();
    assert_eq!(g.ui_tree.tile(row).get("depth"), Some(&Value::Num(42.0)));
    assert!(ui_loading::sync(&mut g, false, false).changes.is_empty());
    g.ui_tree.inject_text(g.ui_files.as_deref().unwrap(), "Trade", r#"<rect name="Injected"><depth>9</depth></rect>"#);
    let injected = g.ui_tree.find_tile("Trade/Injected").unwrap();
    g.ui_tree.set_trait(root, "depth", Value::Num(99.0));
    g.menu = None;
    assert_eq!(ui_loading::sync(&mut g, false, false).changes, ["dropped BarterMenu"]);
    assert!(!g.ui_tree.tile(root).live);
    assert!(!g.ui_tree.tile(injected).live);
    assert!(g.ui_tree.menu("Trade").is_none());
    for name in ALWAYS_LOADED {
        assert!(g.ui_tree.menu(name).is_some());
    }
    g.menu = Some(menus::BARTER);
    assert_eq!(ui_loading::sync(&mut g, false, false).changes.len(), 1);
    let rebuilt = g.ui_tree.menu("Trade").unwrap();
    assert_ne!(rebuilt, root);
    assert_eq!(g.ui_tree.tile(rebuilt).get("depth"), Some(&Value::Num(42.0)));
    assert!(g.ui_tree.find_tile("Trade/Injected").is_none());
    // Persistent menus keep their injected content on close [design: §3.3].
    let hud = g.ui_tree.menu("HUDMainMenu").unwrap();
    g.ui_tree.inject_text(g.ui_files.as_deref().unwrap(), "InventoryMenu", r#"<rect name="Kept"/>"#);
    let kept = g.ui_tree.find_tile("InventoryMenu/Kept").unwrap();
    g.menu = Some(menus::INVENTORY);
    ui_loading::sync(&mut g, false, false);
    g.menu = None;
    ui_loading::sync(&mut g, false, false);
    assert_eq!(g.ui_tree.menu("HUDMainMenu"), Some(hud));
    assert!(g.ui_tree.menu("InventoryMenu").is_some());
    assert_eq!(g.ui_tree.find_tile("InventoryMenu/Kept"), Some(kept));
}

#[test]
fn start_menu_with_pause_or_main_menu() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    load_lifetime(&mut g);
    assert!(ui_loading::sync(&mut g, false, false).changes.is_empty());
    assert_eq!(ui_loading::sync(&mut g, true, false).changes, ["built StartMenu (menus\\added\\start.xml, 1 tiles)"]);
    let root = g.ui_tree.menu("StartMenu").unwrap();
    assert!(ui_loading::sync(&mut g, true, true).changes.is_empty());
    assert!(ui_loading::sync(&mut g, false, true).changes.is_empty());
    assert_eq!(g.ui_tree.menu("StartMenu"), Some(root));
    assert_eq!(ui_loading::sync(&mut g, false, false).changes, ["dropped StartMenu"]);
    assert_eq!(ui_loading::sync(&mut g, false, true).changes, ["built StartMenu (menus\\added\\start.xml, 1 tiles)"]);
    assert_ne!(g.ui_tree.menu("StartMenu"), Some(root));
    assert_eq!(ui_loading::sync(&mut g, false, false).changes, ["dropped StartMenu"]);
}

#[test]
fn underlying_menu_and_dialogue_stay_open() {
    use fnv_game::{dialogue::Conversation, game::menus};
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    load_lifetime(&mut g);
    g.menu = Some(menus::MESSAGE);
    g.menu_under = Some(menus::BARTER);
    g.dialogue = Some(Conversation { actor: fnv_sim::FormId::PLAYER_REF, through: None, line: None, choices: Vec::new(), topic_list: false, lines: 0, then: None, ending: false });
    assert_eq!(ui_loading::sync(&mut g, false, false).changes.len(), 3);
    g.menu = None;
    assert_eq!(ui_loading::sync(&mut g, false, false).changes, ["dropped MessageMenu"]);
    assert!(g.ui_tree.menu("Trade").is_some());
    assert!(g.ui_tree.menu("DialogMenu").is_some());
    g.menu_under = None;
    g.dialogue = None;
    assert_eq!(ui_loading::sync(&mut g, false, false).changes.len(), 2);
}

#[test]
fn the_engines_prompt_builds_over_the_open_menu_and_drops_with_it() {
    use fnv_game::game::menus;
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let src = lifetime_files().with("menus/added/quantity.xml", r#"<menu name="QuantityMenu"><class>&QuantityMenu;</class></menu>"#);
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(&mut g, Box::new(src), paths);
    // T15a: the prompt is no `Game::menu`, yet its file builds while `ui_prompt` holds it.
    g.menu = Some(menus::BARTER);
    g.ui_prompt = Some(menus::QUANTITY);
    let changes = ui_loading::sync(&mut g, false, false).changes;
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert!(g.ui_tree.menu("QuantityMenu").is_some() && g.ui_tree.menu("Trade").is_some());
    assert!(!g.menu_open(menus::QUANTITY), "MenuMode does not report the prompt");
    g.ui_prompt = None;
    assert_eq!(ui_loading::sync(&mut g, false, false).changes, ["dropped QuantityMenu"]);
    assert!(g.ui_tree.menu("Trade").is_some());
}

#[test]
fn missing_class_warns_once_and_side_codes_are_ignored() {
    use fnv_game::game::menus;
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    // A matching filename and menu name alone do not match a class [design].
    let src = lifetime_files().with("menus/quantity_menu.xml", r#"<menu name="QuantityMenu"/>"#);
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(&mut g, Box::new(src), paths);
    let quantity = fnv_ui::value::menu_class_number("QuantityMenu").unwrap();
    g.menu = Some(quantity);
    assert_eq!(ui_loading::sync(&mut g, false, false).warnings, ["QuantityMenu: no file with matching menu class"]);
    assert!(ui_loading::sync(&mut g, false, false).warnings.is_empty());
    g.menu = None;
    ui_loading::sync(&mut g, false, false);
    g.menu = Some(quantity);
    assert!(ui_loading::sync(&mut g, false, false).warnings.is_empty());
    assert!(g.ui_tree.menu("QuantityMenu").is_none());
    g.menu = Some(menus::LOCKPICK);
    assert_eq!(ui_loading::sync(&mut g, false, false).warnings.len(), 1);
    assert!(ui_loading::sync(&mut g, false, false).warnings.is_empty());
    for code in [menus::PIPBOY, menus::LOADING, menus::VATS] {
        g.menu = Some(code);
        let report = ui_loading::sync(&mut g, false, false);
        assert!(report.changes.is_empty());
        assert!(report.warnings.is_empty());
    }
    g.menu = None;
    g.loading = true;
    assert!(ui_loading::sync(&mut g, false, false).changes.is_empty());
    load_lifetime(&mut g);
    g.menu = Some(quantity);
    assert_eq!(ui_loading::sync(&mut g, false, false).warnings.len(), 1);
}

#[test]
fn reopened_menu_does_not_repeat_mends() {
    use fnv_game::game::menus;
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let src = lifetime_files().with(
        "menus/added/trade.xml",
        r#"<menu name="Trade"><class>&BarterMenu;</class><include src="PROBE.XML"/><depth><copy>1</copy><bogus/></depth></menu>"#,
    );
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(&mut g, Box::new(src), paths);
    g.menu = Some(menus::BARTER);
    let first = ui_loading::sync(&mut g, false, false);
    assert_eq!(first.warnings.len(), 1);
    assert!(first.warnings[0].contains("<bogus> is no operator"));
    g.menu = None;
    ui_loading::sync(&mut g, false, false);
    g.menu = Some(menus::BARTER);
    let second = ui_loading::sync(&mut g, false, false);
    assert_eq!(second.changes.len(), 1);
    assert!(second.warnings.is_empty());
}

#[test]
fn startup_and_rebuild() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let src = files();
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    let report = ui_loading::load(&mut g, Box::new(src), paths.clone());
    assert_eq!((report.menus, report.tiles), (6, 12));
    assert_eq!((report.game_strings, report.game_strings_unresolved), (1, 0));
    assert_eq!(report.warning_count(), 1);
    assert_eq!(report.warnings["menus\\prefabs\\probe.xml"].len(), 1);
    assert_eq!(g.ui_menus.path("hudmainmenu"), Some("menus\\added\\0.xml"));
    assert_eq!(g.ui_tree.tile(g.ui_tree.globals()).get("_Probe"), Some(&Value::Num(17.0)));
    let hud = g.ui_tree.menu("HUDMainMenu").unwrap();
    assert_eq!(g.ui_tree.tile(hud).get("depth"), Some(&Value::Num(17.0)));
    let label = g.ui_tree.find_tile("HUDMainMenu/Label").unwrap();
    assert_eq!(g.ui_tree.tile(label).get("string"), Some(&Value::Str("fixture text".into())));
    g.ui_tree.inject_text(g.ui_files.as_deref().unwrap(), "HUDMainMenu", r#"<rect name="Injected"/>"#);
    g.ui_taps.push(28);
    ui_loading::load(&mut g, Box::new(files()), paths);
    assert!(g.ui_tree.find_tile("HUDMainMenu/Injected").is_none());
    assert_eq!(g.ui_taps, [28]);
}

#[test]
fn engine_menus_read_game_strings_through_the_tree_source() {
    // T10 (menu-xml.md §2.4): the engine's own menus ask `ui_loading::game_string`: the
    // tree's source once loaded (the load order over NVForever's labels), the labels
    // alone before. Synthetic text for the source.
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    assert_eq!(ui_loading::game_string(&g, "sCloseButton"), fnv_ui::strings::built_in("sCloseButton").unwrap());
    assert_eq!(ui_loading::game_string(&g, "sNvfNowhere"), "");
    let mut src = files();
    src.strings.insert("sCloseButton".into(), "fixture close".into());
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(&mut g, Box::new(src), paths);
    assert_eq!(ui_loading::game_string(&g, "SCLOSEBUTTON"), "fixture close");
    assert_eq!(ui_loading::game_string(&g, "sProbe"), "fixture text");
    // A name the source lacks still reads the shipped label.
    assert_eq!(ui_loading::game_string(&g, "sNext"), fnv_ui::strings::built_in("sNext").unwrap());
    assert_eq!(ui_loading::game_string(&g, "sNvfNowhere"), "");
}

#[test]
fn missing_menu_and_globals_warn() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    let src = MemSource::default().with("menus/renamed.xml", r#"<menu name="HUDMainMenu"><depth>4</depth></menu>"#);
    let report = ui_loading::load(&mut g, Box::new(src), ["menus/renamed.xml".into()]);
    assert_eq!(report.menus, 1);
    assert_eq!(report.warning_count(), 6);
    assert_eq!((report.game_strings, report.game_strings_unresolved), (0, 0));
    assert_eq!(report.warnings["InventoryMenu"], ["menu not found"]);
    assert_eq!(report.warnings["menus\\globals.xml"], ["not found"]);
    assert!(g.ui_tree.menu("HUDMainMenu").is_some());
    assert!(g.ui_files.is_some());
}

#[test]
fn vfs_source_owns_strings_and_reads_files() {
    fn is_static_source<T: Source + Send + Sync + 'static>(_: &T) {}
    let f = fixture();
    let mut world = World::new(&f.forms, &f.refs, SimConfig::default());
    // Only these synthetic files under the repository's target directory [design].
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/ui-loading-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Menus")).unwrap();
    let xml = r#"<menu name="HUDMainMenu"><text name="Label"><string>&-sProbe;</string></text><text name="Gone"><string>&-sAbsent;</string></text></menu>"#;
    std::fs::write(dir.join("Menus/renamed.xml"), xml).unwrap();
    let vfs = Arc::new(Vfs::open(&dir, &[]).unwrap());
    let src = UiFiles::new(vfs.clone(), &world);
    is_static_source(&src);
    assert_eq!(src.menu_paths(), ["menus\\renamed.xml"]);
    assert_eq!(src.list("MENUS"), ["menus\\renamed.xml"]);
    assert_eq!(src.read("MENUS/RENAMED.XML").unwrap(), xml.as_bytes());
    assert!(src.is_loose("MENUS/RENAMED.XML"));
    assert!(!src.is_loose("menus/missing.xml"));
    assert_eq!(src.game_string("SPROBE").as_deref(), Some("plugin text"));
    world.change_setting("sProbe", Setting::Str("changed".into())).unwrap();
    assert_eq!(src.game_string("sProbe").as_deref(), Some("plugin text"));
    let next = UiFiles::new(vfs, &world);
    drop(world);
    assert_eq!(next.game_string("sProbe").as_deref(), Some("changed"));
    assert_eq!(src.game_string("sAbsent"), None);
    let mut ui = fnv_ui::Ui::new();
    let root = ui.load_menu(&src, "menus/renamed.xml").unwrap();
    assert_eq!(ui.mod_added_tiles(root), [root]);
    let label = ui.find_tile("HUDMainMenu/Label").unwrap();
    assert_eq!(ui.tile(label).get("string"), Some(&Value::Str("plugin text".into())));
    // T10: the load order's record answers; a name with no record and no built-in label
    // reads empty (menu-xml.md §2.4).
    assert_eq!(src.game_strings().lookup("sprobe"), Some(("plugin text", fnv_ui::strings::Origin::LoadOrder)));
    assert_eq!(ui.get_string("HUDMainMenu/Gone/string").as_deref(), Some(""));
    assert_eq!(ui.game_strings_read.get("sabsent"), Some(&false));
    assert_eq!(ui.game_strings_read.get("sprobe"), Some(&true));
    std::fs::remove_file(dir.join("Menus/renamed.xml")).unwrap();
    std::fs::remove_dir(dir.join("Menus")).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
