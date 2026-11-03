//! An MCM mod's menu script against the start menu's tile tree (task T7 of
//! docs/handoff/ui-tile-tree-tasks.md, issue #976), headless: the start menu is built
//! from files with MCM's include chain while the start menu is open, and a quest
//! script's `Begin MenuMode 1013` block, compiled from its source as an MCM mod's is,
//! reads and writes MCM's traits through GetMCMFloat, SetMCMFloat and SetMCMString.
//!
//! The menu files and the script are written for the test in MCM's shape (menu-xml.md
//! §10); no game or mod data is used.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{menus, Game, GameConfig, InstKey};
use fnv_game::{ui_loading, GameData};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
use serde_json::Value as Json;

/// The mod's quest and its script, in the second plugin of the load order (index 1).
const QUEST: u32 = 0x0100_0800;
const SCRIPT: u32 = 0x0100_0900;

/// The quest script, in the way MCM mods write theirs (FOV Slider's `MenuMode 1013`
/// block, menu-xml.md §10): do nothing unless MCM's `_ActiveMod` is this plugin; fill in
/// the first option; take a new value when MCM says one was entered.
const SOURCE: &str = r#"scn McmTestMenuScript
short iShown
float fValue

Begin MenuMode 1013
  if GetMCMFloat 0 0 "_ActiveMod" != GetSelfModIndex
    return
  endif
  SetMCMString 1 1 "_title" "Test option"
  SetMCMFloat 1 1 "_enable" 1
  set iShown to iShown + 1
  if GetMCMFloat 0 0 "_NewValue" == 1
    set fValue to GetMCMFloat 0 0 "_Value"
    SetMCMFloat 0 0 "_NewValue" 0
    SetMCMFloat 0 0 "_Reset" 1
  endif
End
"#;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    // The bytes call an opcode no function has, as bytes compiled with the script
    // extender loaded may: the unit runs from its source (fnv_game::source_compile).
    let bytes = script(&[(0, None, st(0x7FFF, &le16(0)))]);
    let mut subs = unit(1, &bytes, &[(1, "iShown", 1), (2, "fValue", 0)], &[]);
    subs.push(sub(b"SCTX", SOURCE.as_bytes().to_vec()));
    let scpt = record(b"SCPT", 0, SCRIPT & 0xFF_FFFF, &[vec![sub(b"EDID", z("McmTestMenuScript"))], subs].concat());
    // Start game enabled; a short quest delay, so the script runs every frame.
    let data = sub(b"DATA", [vec![1u8, 0, 0, 0], 0.01f32.to_le_bytes().to_vec()].concat());
    let qust = record(b"QUST", 0, QUEST & 0xFF_FFFF, &[sub(b"EDID", z("McmTestQuest")), sub(b"SCRI", (SCRIPT & 0xFF_FFFF).to_le_bytes().to_vec()), data]);
    let plugin = [tes4(), top(b"SCPT", &scpt), top(b"QUST", &qust)].concat();
    let plugins = vec![Plugin::parse(&tes4(), "Base.esm".into()).unwrap(), Plugin::parse(&plugin, "McmTest.esp".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

/// A start menu whose include list adds MCM's file, as MCM's replacement `start_menu.xml`
/// and `includes_StartMenu.xml` do, and an MCM rect with its options list.
fn files() -> MemSource {
    MemSource::default()
        .with("menus\\options\\start_menu.xml", r#"<menu name="StartMenu"> <class> &StartMenu; </class> <include src="includes_StartMenu.xml"/> </menu>"#)
        .with("menus\\prefabs\\includes_StartMenu.xml", r#"<include src="MCM\MCM.xml"/>"#)
        .with(
            "menus\\prefabs\\MCM\\MCM.xml",
            r#"<rect name="MCM"> <_ActiveMod> 0 </_ActiveMod> <_NewValue> 0 </_NewValue> <_Value> 0 </_Value> <_Reset> 0 </_Reset>
               <include src="MCM\Options.xml"/> <rect name="MCM_Back"/> </rect>"#,
        )
        .with(
            "menus\\prefabs\\MCM\\Options.xml",
            r#"<rect name="MCM_Options">
               <hotrect name="Option1"> <_optionID> 1 </_optionID> <_title> </_title> <_enable> 0 </_enable> </hotrect>
               <hotrect name="Option2"> <_optionID> 2 </_optionID> <_title> </_title> <_enable> 0 </_enable> </hotrect>
               </rect>"#,
        )
}

fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    g.new_game();
    let src = files();
    let paths = src.files.keys().cloned().collect::<Vec<_>>();
    ui_loading::load(&mut g, Box::new(src), paths);
    g
}

fn title(g: &Game) -> String {
    g.ui_tree.get_string("StartMenu/MCM/MCM_Options/Option1/_title").unwrap_or_default()
}

#[test]
fn an_mcm_menu_script_runs_against_the_start_menu() {
    let f = fixture();
    assert!(f.data.script(FormId(SCRIPT)).is_some(), "the script loads from its source");
    let mut g = game(&f);
    let quest = InstKey::Quest { quest: FormId(QUEST) };
    let shown = |g: &Game| g.var(quest, 1).num();

    // In play the block does not run, and there is no start menu.
    g.frames(5);
    assert_eq!(shown(&g), 0.0);
    assert!(g.ui_tree.menu("StartMenu").is_none());

    // The start menu opens: its file is built with MCM's include, and the block runs,
    // but MCM shows another mod (0).
    g.menu = Some(menus::START);
    let report = ui_loading::sync(&mut g, true, false);
    assert!(report.changes.iter().any(|c| c.starts_with("built StartMenu")), "{:?}", report.changes);
    assert!(g.ui_tree.find_tile("StartMenu/MCM/MCM_Options/Option2").is_some());
    g.frames(5);
    assert_eq!((shown(&g), title(&g)), (0.0, String::new()));

    // MCM picks this mod (its load-order index, 1): the script fills in its option.
    assert!(g.ui_tree.set_float("StartMenu/MCM/_ActiveMod", 1.0));
    g.frames(3);
    assert!(shown(&g) >= 1.0);
    assert_eq!(title(&g), "Test option");
    assert_eq!(g.ui_tree.get_float("StartMenu/MCM/MCM_Options/Option1/_enable"), Some(1.0));
    assert_eq!(g.ui_tree.get_float("StartMenu/MCM/MCM_Options/Option2/_enable"), Some(0.0));

    // A value entered in MCM's scale: the script takes it and answers.
    g.ui_tree.set_float("StartMenu/MCM/_Value", 42.0);
    g.ui_tree.set_float("StartMenu/MCM/_NewValue", 1.0);
    g.frames(2);
    assert_eq!(g.var(quest, 2).num(), 42.0);
    assert_eq!(g.ui_tree.get_float("StartMenu/MCM/_NewValue"), Some(0.0));
    assert_eq!(g.ui_tree.get_float("StartMenu/MCM/_Reset"), Some(1.0));

    // The menu closes: the block stops, and the start menu is dropped.
    g.menu = None;
    ui_loading::sync(&mut g, false, false);
    let before = shown(&g);
    g.frames(5);
    assert_eq!(shown(&g), before);
    assert!(g.ui_tree.menu("StartMenu").is_none());
}
