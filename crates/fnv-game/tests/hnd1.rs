//! Batch hnd1 (fnv_game::nvse_hnd1; docs/spec-notes/ext-hnd1.md): the answers the original
//! gave in probe sets quests2 and hnd2, run as scripts.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let npcs = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let bytes = [tes4(), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)))].concat();
    let plugins = vec![Plugin::parse(&bytes, "Hnd1.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("array_var aA\nstring_var s\nBegin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

#[test]
fn aux_string_maps_answer_as_the_original_did() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(q(&mut g, "AuxStringMapArraySetFloat \"nvfProbe\" \"k1\" 2.5\nSetFunctionValue (AuxStringMapArrayGetFloat \"nvfProbe\" \"k1\")"), XVal::Num(2.5));
    assert_eq!(q(&mut g, "AuxStringMapArraySetString \"nvfProbe\" \"k2\" \"hello\"\nSetFunctionValue (AuxStringMapArrayGetString \"nvfProbe\" \"k2\")"), XVal::Str("hello".into()));
    assert_eq!(q(&mut g, "SetFunctionValue (AuxStringMapArrayGetFloat \"nvfProbe\" \"nokey\")"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (AuxStringMapArrayGetFloat \"nvfProbe\" \"k2\")"), XVal::Num(0.0), "a string read as a number");
    assert_eq!(q(&mut g, "SetFunctionValue (AuxStringMapArrayGetFloat \"NVFPROBE\" \"K1\")"), XVal::Num(2.5), "names and keys ignore case");
    assert_eq!(q(&mut g, "SetFunctionValue (AuxStringMapArraySetFloat \"nvfProbe\" \"k3\" 7)"), XVal::Num(0.0));
    assert_eq!(
        q(&mut g, "let aA := ar_Map \"a\"::1 \"b\"::\"x\"\nAuxStringMapArraySetFromArray \"nvfProbe2\" aA 0\nSetFunctionValue $(AuxStringMapArrayGetFloat \"nvfProbe2\" \"a\") + \" \" + (AuxStringMapArrayGetString \"nvfProbe2\" \"b\")"),
        XVal::Str("1 x".into())
    );
    assert_eq!(q(&mut g, "SetFunctionValue (AuxStringMapArrayDestroy \"nvfProbe\")"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (AuxStringMapArrayGetFloat \"nvfProbe\" \"k1\")"), XVal::Num(0.0));
}

#[test]
fn quest_lists_are_no_array_when_empty() {
    let f = fixture();
    let mut g = game(&f);
    // No quest has an objective shown or is completed here: JIP's lists give 0.
    assert_eq!(q(&mut g, "SetFunctionValue (GetQuests 0)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetQuests 1)"), XVal::Num(0.0));
    assert_eq!(q(&mut g, "SetFunctionValue (PlayerRef.GetSequenceAnimGroup 3)"), XVal::Num(-1.0));
}

#[test]
fn hnd4_reads_answer_as_the_original_did() {
    let f = fixture();
    let mut g = game(&f);
    // [observed: hnd4]
    let n = |g: &mut Game, s: &str| match q(g, &format!("SetFunctionValue ({s})")) {
        XVal::Num(x) => x,
        v => panic!("{s}: {v:?}"),
    };
    assert_eq!(n(&mut g, "GetSystemColor 1"), f64::from(0xFFB642));
    assert_eq!(n(&mut g, "GetSystemColor 4"), f64::from(0x1AFF80));
    assert_eq!(n(&mut g, "GetSystemColor 6"), 0.0);
    assert_eq!(n(&mut g, "GetJohnnyPatch 1"), 1.0);
    assert_eq!(n(&mut g, "GetJohnnyPatch 2"), 0.0);
    assert_eq!(n(&mut g, "GetJohnnyPatch 8"), 1.0);
    assert_eq!(n(&mut g, "GetJohnnyPatch 9"), 0.0);
    assert_eq!(n(&mut g, "player.GetRadiationLevelAlt"), 0.0);
    q(&mut g, "SetSystemColor 1 1 2 3");
    assert_eq!(n(&mut g, "GetSystemColor 1"), f64::from(0x010203), "SetSystemColor's colour");
}

#[test]
fn set_objective_text_changes_get_objective_text_only_for_objectives_that_exist() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(q(&mut g, "SetObjectiveText Player 10 \"x %g\" 5\nSetFunctionValue (GetObjectiveText Player 10)"), XVal::Str(String::new()));
}
