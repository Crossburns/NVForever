//! The weapon-instance plugin's functions (ppNVSE; fnv_game::nvse_pp and the file
//! functions in fnv_game::mod_files) through compiled scripts, on a synthetic plugin
//! written for the test (no game data, made-up names): docs/spec-notes/ext-pp.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const COUNTER: u32 = 0x800;
const WEAP: u32 = 0x810;
const IMOD: u32 = 0x811;
const IMOD2: u32 = 0x812;
const AMMO_A: u32 = 0x820;
const AMMO_B: u32 = 0x821;
const AMMO_C: u32 = 0x822;
const AMMO_LIST: u32 = 0x823;
const COUNT: u32 = 0x901;
const STOP: u32 = 0x902;
const SKIP: u32 = 0x903;
const MARK: u32 = 0x904;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn catalogue() -> (FunctionTable, Json) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    (FunctionTable::load(&p).unwrap(), j)
}

/// Names the test's function scripts use.
struct Names;
impl fnv_script::compile::Env for Names {
    fn form(&self, e: &str) -> Option<fnv_script::compile::FormInfo> {
        let (form, sig) = match e.to_ascii_lowercase().as_str() {
            "counter" => (COUNTER, *b"GLOB"),
            _ => return None,
        };
        Some(fnv_script::compile::FormInfo { form, sig })
    }
    fn info(&self, _: u32) -> Option<fnv_script::compile::FormInfo> {
        None
    }
    fn editor_id(&self, _: u32) -> Option<String> {
        None
    }
    fn local_form(&self, _: u32) -> Option<u32> {
        None
    }
    fn script_vars(&self, _: u32) -> Option<Vec<fnv_script::VarDecl>> {
        None
    }
}

/// A function script compiled by NVForever's compiler, as a SCPT record.
fn script(table: &FunctionTable, fid: u32, edid: &str, src: &str) -> Vec<u8> {
    let c = fnv_script::compile::compile(src, table, &Names, None);
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    let vars: Vec<(u32, String, u8)> = c.variables.iter().map(|v| (v.index, v.name.clone(), u8::from(v.ty == fnv_script::VarType::Int))).collect();
    let vars: Vec<(u32, &str, u8)> = vars.iter().map(|(i, n, t)| (*i, n.as_str(), *t)).collect();
    let refs: Vec<u32> = c.refs.iter().filter_map(|r| if let fnv_script::unit::RefSlot::Form(f) = r { Some(*f) } else { None }).collect();
    let mut subs = unit(0, &c.bytes, &vars, &refs);
    subs.push(sub(b"SCTX", src.as_bytes().to_vec()));
    record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], subs].concat())
}

fn fixture() -> Fixture {
    let (table, j) = catalogue();
    let glob = record(b"GLOB", 0, COUNTER, &[sub(b"EDID", z("Counter")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let weap = record(
        b"WEAP",
        0,
        WEAP,
        &[sub(b"EDID", z("Gun")), sub(b"DATA", vec![0; 15]), sub(b"NAM0", u32s(&[AMMO_LIST])), sub(b"WMI1", u32s(&[IMOD]))],
    );
    let imods = [record(b"IMOD", 0, IMOD, &[sub(b"EDID", z("Scope"))]), record(b"IMOD", 0, IMOD2, &[sub(b"EDID", z("Silencer"))])].concat();
    let ammo = |id: u32, edid: &str| record(b"AMMO", 0, id, &[sub(b"EDID", z(edid))]);
    let ammos = [ammo(AMMO_A, "RoundA"), ammo(AMMO_B, "RoundB"), ammo(AMMO_C, "RoundC")].concat();
    let flst = record(b"FLST", 0, AMMO_LIST, &[sub(b"EDID", z("GunAmmo")), sub(b"LNAM", u32s(&[AMMO_A])), sub(b"LNAM", u32s(&[AMMO_B])), sub(b"LNAM", u32s(&[AMMO_C]))]);
    let scripts = [
        script(&table, COUNT, "PPCount", "scn PPCount\nBegin Function { }\n  let Counter := Counter + 1\nEnd\n"),
        script(&table, STOP, "PPStop", "scn PPStop\nBegin Function { }\n  let Counter := Counter + 10\n  SetFunctionValue -1\nEnd\n"),
        script(&table, SKIP, "PPSkip", "scn PPSkip\nref r\nBegin Function {r}\n  let Counter := Counter + 100\n  SetFunctionValue 1\nEnd\n"),
        script(&table, MARK, "PPMark", "scn PPMark\nref m\nref o\nBegin Function {m, o}\n  let Counter := Counter + 1000\nEnd\n"),
    ]
    .concat();
    let plugin = [
        tes4(),
        top(b"GLOB", &glob),
        top(b"IMOD", &imods),
        top(b"AMMO", &ammos),
        top(b"FLST", &flst),
        top(b"WEAP", &weap),
        top(b"SCPT", &scripts),
        top(b"NPC_", &record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))])),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&plugin, "PPTest.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig::default())
}

fn query(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), Default::default()).expect("the query runs")
}

fn val(g: &mut Game, expr: &str) -> XVal {
    query(g, &format!("SetFunctionValue ({expr})"))
}

fn x(v: f64) -> XVal {
    XVal::Num(v)
}

fn s(t: &str) -> XVal {
    XVal::Str(t.to_string())
}

fn counter(g: &mut Game) -> f64 {
    match val(g, "Counter") {
        XVal::Num(n) => n,
        v => panic!("{v:?}"),
    }
}

#[test]
fn ax_variables_hold_values_by_key_index_and_form() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: AX Set's example, three types at 0, 1, 2 on one form].
    query(&mut g, "AX_Set 777 \"MyAuxList\" 0 Gun\nAX_Set \"Hello\" \"MyAuxList\" 1 Gun\narray_var a\nlet a := ar_List 4 5 6\nAX_Set a \"MyAuxList\" 2 Gun");
    assert_eq!(val(&mut g, "AX_Get \"MyAuxList\" 0 Gun"), x(777.0));
    assert_eq!(query(&mut g, "string_var t\nlet t := AX_Get \"MyAuxList\" 1 Gun\nSetFunctionValue t"), s("Hello"));
    // Keys are matched with case [observed: pp probe ax.exists].
    assert_eq!(val(&mut g, "AX_GetType \"myauxlist\" 1 Gun"), x(-2.0));
    // The array outlives the script that made it.
    assert_eq!(query(&mut g, "array_var b\nlet b := AX_Get \"MyAuxList\" 2 Gun\nSetFunctionValue b[1]"), x(5.0));
    assert_eq!(val(&mut g, "AX_GetType \"MyAuxList\" 0 Gun"), x(0.0));
    assert_eq!(val(&mut g, "AX_GetType \"MyAuxList\" 1 Gun"), x(2.0));
    assert_eq!(val(&mut g, "AX_GetType \"MyAuxList\" 2 Gun"), x(3.0));
    assert_eq!(val(&mut g, "AX_GetType \"MyAuxList\" 7 Gun"), x(-2.0));
    assert_eq!(val(&mut g, "AX_GetType \"Nothing\" 0 Gun"), x(-2.0));
    assert_eq!(val(&mut g, "AX_Size \"MyAuxList\" Gun"), x(3.0));
    // A global key is another key.
    assert_eq!(val(&mut g, "AX_Exists \"MyAuxList\""), x(0.0));
    assert_eq!(val(&mut g, "AX_Exists \"MyAuxList\" Gun"), x(1.0));
    query(&mut g, "AX_Set 256 \"MCM_UserControl_Activate\"");
    assert_eq!(val(&mut g, "AX_Get \"MCM_UserControl_Activate\" 0 0"), x(256.0));
    // Find: an entry, inside an array with a deep search.
    assert_eq!(val(&mut g, "AX_Find \"Hello\" \"MyAuxList\" 0 Gun"), x(1.0));
    // With case, and not inside arrays even with the deep flag [observed: pp probe ax.find].
    assert_eq!(val(&mut g, "AX_Find \"hello\" \"MyAuxList\" 0 Gun"), x(-1.0));
    assert_eq!(val(&mut g, "AX_Find 5 \"MyAuxList\" 0 Gun"), x(-1.0));
    assert_eq!(val(&mut g, "AX_Find 5 \"MyAuxList\" 1 Gun"), x(-1.0));
    // Null keeps the places, erase closes the gap [documented]; both give 0 [observed:
    // pp probes ax.null, ax.erase, ax.drop].
    assert_eq!(val(&mut g, "AX_Null \"MyAuxList\" 0 Gun"), x(0.0));
    assert_eq!(val(&mut g, "AX_GetType \"MyAuxList\" 0 Gun"), x(-1.0));
    assert_eq!(val(&mut g, "AX_Size \"MyAuxList\" Gun"), x(3.0));
    assert_eq!(val(&mut g, "AX_Erase \"MyAuxList\" 0 Gun"), x(0.0));
    assert_eq!(val(&mut g, "AX_Size \"MyAuxList\" Gun"), x(2.0));
    assert_eq!(val(&mut g, "AX_GetType \"MyAuxList\" 0 Gun"), x(2.0));
    let before = g.console_out.len();
    assert_eq!(val(&mut g, "AX_Dump \"MyAuxList\" Gun"), x(1.0));
    assert_eq!(g.console_out.len() - before, 3);
    // −1 deletes the key.
    assert_eq!(val(&mut g, "AX_Erase \"MyAuxList\" -1 Gun"), x(0.0));
    assert_eq!(val(&mut g, "AX_Exists \"MyAuxList\" Gun"), x(0.0));
}

/// `func r` with `r` the form whose editor ID is `edid` (an instance has no FormID a
/// script can name; EditorIDToFormID finds it by its editor ID).
fn on(g: &mut Game, func: &str, edid: &str) -> XVal {
    query(g, &format!("ref r\nlet r := EditorIDToFormID \"{edid}\"\nSetFunctionValue ({func} r)"))
}

#[test]
fn form_instances_stand_for_their_base_and_are_saved() {
    let f = fixture();
    let mut g = game(&f);
    let XVal::Ref(inst) = val(&mut g, "CreateFormInstance Gun \"MyInstances\"") else { panic!() };
    assert_ne!(inst, 0);
    // Its editor ID is "Inst" and the count, without the base's [observed: pp probe inst].
    assert_eq!(val(&mut g, "EditorIDToFormID \"Inst1\""), XVal::Ref(inst));
    assert_eq!(on(&mut g, "IsFormInstance", "Inst1"), x(1.0));
    assert_eq!(val(&mut g, "IsFormInstance Gun"), x(0.0));
    assert_eq!(on(&mut g, "GetStaticForm", "Inst1"), XVal::Ref(WEAP));
    assert_eq!(on(&mut g, "GetWeaponBase", "Inst1"), XVal::Ref(WEAP));
    // Not an instance of an instance [documented].
    assert_eq!(on(&mut g, "CreateFormInstance", "Inst1"), XVal::Ref(0));
    let XVal::Ref(second) = val(&mut g, "NewWeaponInstance Gun") else { panic!() };
    assert_eq!(val(&mut g, "EditorIDToFormID \"Inst2\""), XVal::Ref(second));
    // A clone reads its base's fields.
    assert_eq!(query(&mut g, "ref r\nlet r := EditorIDToFormID \"Inst1\"\nSetFunctionValue (GetWeaponItemMod 1 r)"), XVal::Ref(IMOD));
    // Saved with the game; a reconstruct handler that sets 1 drops one on the load: the
    // handler filters on "MyInstances", the first instance's key; the second has none.
    query(&mut g, "SetOnInstanceReconstruct 1 PPSkip \"MyInstances\"");
    let snap = g.snapshot();
    assert_eq!(snap.form_instances.len(), 2);
    let session = g.take_session();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut h = Game::restore(&f.data, world, GameConfig::default(), &snap).unwrap();
    h.put_session(session);
    assert!(!h.pp.instances.contains_key(&FormId(inst)));
    assert!(h.pp.instances.contains_key(&FormId(second)));
    assert_eq!(on(&mut h, "IsFormInstance", "Inst2"), x(1.0));
    assert_eq!(on(&mut h, "GetStaticForm", "Inst2"), XVal::Ref(WEAP));
    assert_eq!(counter(&mut h), 100.0);
    // New instances do not take the restored ones' ids.
    let XVal::Ref(third) = val(&mut h, "CreateFormInstance Gun") else { panic!() };
    assert!(third > second);
    // Its count follows the restored ones, not the live count, so "Inst2", still in use,
    // is not made again (PR #1291 review).
    assert_eq!(h.pp.instances[&FormId(third)].edid, "Inst3");
}

#[test]
fn item_mod_slots_vanilla_and_extended() {
    let f = fixture();
    let mut g = game(&f);
    // A vanilla slot on a base form turns nothing on [observed: pp probe attach.vanilla,
    // on an instance]; nor does a slot Devkit does not define [observed: pp probe attach].
    assert_eq!(val(&mut g, "AttachMod Gun \"1\""), x(0.0));
    assert_eq!(val(&mut g, "GetAllAttachedWeaponMods Gun \"1\""), XVal::Ref(0));
    assert_eq!(val(&mut g, "AttachMod Gun \"Barrel Device\" Silencer"), x(0.0));
    // An extended slot as Devkit would define it, with the attach and detach handlers
    // [documented: AttachMod; OnAttachItemMod].
    g.pp.slots.extend(["barrel device".to_string(), "grip".to_string()]);
    query(&mut g, "OnAttachItemMod 1 PPMark 1 \"1\" Silencer \"2\" Gun\nOnDetachItemMod 1 PPCount 1");
    assert_eq!(val(&mut g, "HasExtendedMods Gun"), x(0.0));
    // The flag counts the vanilla mods the weapon has (its WMI1) [observed: pp probes].
    assert_eq!(val(&mut g, "HasExtendedMods Gun 1"), x(1.0));
    assert_eq!(val(&mut g, "AttachMod Gun \"Barrel Device\" Silencer"), x(1.0));
    assert_eq!(counter(&mut g), 1000.0);
    assert_eq!(val(&mut g, "HasExtendedMods Gun"), x(1.0));
    assert_eq!(val(&mut g, "HasExtendedMods Gun 1"), x(1.0));
    assert_eq!(val(&mut g, "GetAllAttachedWeaponMods Gun \"barrel device\""), XVal::Ref(IMOD2));
    assert_eq!(query(&mut g, "array_var a\nlet a := GetAllAttachedMods Gun\nSetFunctionValue (ar_Size a)"), x(1.0));
    assert_eq!(query(&mut g, "array_var a\nlet a := GetAllModsInSlot Gun \"1\"\nSetFunctionValue a[\"1\"]"), XVal::Ref(IMOD));
    assert_eq!(val(&mut g, "AttachMod Gun \"Barrel Device\""), x(1.0));
    assert_eq!(counter(&mut g), 1001.0);
    assert_eq!(val(&mut g, "GetAllAttachedWeaponMods Gun \"Barrel Device\""), XVal::Ref(0));
    // Removed handler: no more calls.
    query(&mut g, "OnDetachItemMod PPCount 0");
    query(&mut g, "AttachMod Gun \"Grip\" Scope\nAttachMod Gun \"Grip\"");
    assert_eq!(counter(&mut g), 1001.0);
    // Akimbo sets and traits come from Devkit: none.
    assert_eq!(val(&mut g, "IsAkimboForm Gun"), x(0.0));
    assert_eq!(val(&mut g, "HasAkimboSet Gun Gun"), x(0.0));
    assert_eq!(query(&mut g, "ref l\nref r\nGetAkimboWeaponsAlt Gun l r\nSetFunctionValue l"), XVal::Ref(0));
    assert_eq!(val(&mut g, "GetFormTrait 0 Gun \"MoveNode\""), x(0.0));
}

#[test]
fn ammo_cycles_through_the_weapons_list() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(val(&mut g, "PlayerRef.FindWeaponAmmo Gun"), XVal::Ref(0));
    let inv = g.world.inventory_mut(FormId::PLAYER_REF).unwrap();
    inv.add(FormId(AMMO_A), 5);
    inv.add(FormId(AMMO_B), 5);
    assert_eq!(val(&mut g, "PlayerRef.FindWeaponAmmo Gun"), XVal::Ref(AMMO_A));
    // After B: C (not carried), round to A [documented: the example's order].
    assert_eq!(val(&mut g, "PlayerRef.FindWeaponAmmo Gun RoundB"), XVal::Ref(AMMO_A));
    assert_eq!(val(&mut g, "PlayerRef.FindWeaponAmmo Gun RoundA"), XVal::Ref(AMMO_B));
    // ReplaceItemInInventory on an inventory reference.
    assert_eq!(query(&mut g, "ref r\nforeach r <- PlayerRef\n  if r.GetBaseObject == RoundA\n    r.ReplaceItemInInventory RoundC\n  endif\nloop"), x(0.0));
    let inv = g.world.inventory_mut(FormId::PLAYER_REF).unwrap();
    assert_eq!((inv.count(FormId(AMMO_A)), inv.count(FormId(AMMO_C))), (0, 5));
}

#[test]
fn call_loops_run_until_they_stop() {
    let f = fixture();
    let mut g = game(&f);
    query(&mut g, "CallLoop 0 PPCount\nCallLoop 0 PPStop");
    g.tick();
    assert_eq!(counter(&mut g), 11.0);
    g.tick();
    assert_eq!(counter(&mut g), 12.0);
    // −1 removes it.
    query(&mut g, "CallLoop -1 PPCount");
    g.tick();
    assert_eq!(counter(&mut g), 12.0);
}

#[test]
fn the_player_the_sights_and_the_rest() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(val(&mut g, "IsThirdPerson"), x(0.0));
    g.set_first_person(false);
    assert_eq!(val(&mut g, "IsThirdPerson"), x(1.0));
    g.world.get(FormId::PLAYER_REF).unwrap().rot = [10f32.to_radians(), 0.0, 90f32.to_radians()];
    let v = query(&mut g, "float a\nfloat b\nfloat c\nGetPlayerCameraRotation a b c\nSetFunctionValue a * 1000 + c");
    let XVal::Num(n) = v else { panic!("{v:?}") };
    assert!((n - 10090.0).abs() < 1e-3, "{n}");
    assert_eq!(query(&mut g, "string_var n\nlet n := GetCurrentSightingNode\nSetFunctionValue n"), s("##SightingNode"));
    query(&mut g, "SetSightingNode \"MySights\"");
    assert_eq!(query(&mut g, "string_var n\nlet n := GetCurrentSightingNode\nSetFunctionValue n"), s("MySights"));
    // The degradation multiplier: fDamageToWeaponValue until set.
    let XVal::Num(d) = val(&mut g, "SetWeaponDegradationMult Gun") else { panic!() };
    assert!((d - 0.2).abs() < 1e-6, "{d}");
    query(&mut g, "SetWeaponDegradationMult Gun 0.5");
    assert_eq!(val(&mut g, "SetWeaponDegradationMult Gun"), x(0.5));
    // EvaluateString [documented: its example].
    assert_eq!(val(&mut g, "EvaluateString \"IsFormValid Gun\""), x(1.0));
    assert_eq!(val(&mut g, "EvaluateString \"Counter > 3\""), x(0.0));
    assert_eq!(val(&mut g, "EvaluateString \"Counter < 3\""), x(1.0));
    assert_eq!(query(&mut g, "string_var p\nlet p := GetTopicPrompt Gun\nSetFunctionValue p"), s(""));
}

#[test]
fn files_are_read_by_line_and_written_beside_the_game() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(query(&mut g, "array_var a\nlet a := ar_List \"one\" \"two\" \"three\"\nSetFunctionValue (pWriteFile \"Data\\NVFTest\\lines.txt\" a)"), x(1.0));
    assert_eq!(query(&mut g, "array_var a\nlet a := pReadFile \"Data\\NVFTest\\lines.txt\"\nSetFunctionValue (ar_Size a)"), x(3.0));
    assert_eq!(query(&mut g, "array_var a\nlet a := pReadFile \"Data\\NVFTest\\lines.txt\" 1 3\nSetFunctionValue a[0]"), s("two"));
    assert_eq!(query(&mut g, "array_var a\nlet a := ar_List \"four\"\nSetFunctionValue (pWriteFile \"Data\\NVFTest\\lines.txt\" a 1)"), x(1.0));
    assert_eq!(query(&mut g, "array_var a\nlet a := pReadFile \"Data\\NVFTest\\lines.txt\"\nSetFunctionValue a[3]"), s("four"));
    assert_eq!(query(&mut g, "array_var a\nlet a := pReadFile \"Data\\NVFTest\\missing.txt\"\nSetFunctionValue (ar_Size a)"), x(0.0));
    assert_eq!(query(&mut g, "array_var a\nlet a := ar_List \"x\"\nSetFunctionValue (pWriteFileAlt \"Config\\Test.ini\" a)"), x(1.0));
}

/// Paths that would leave NVForever's layer (a `..` part, a drive, a root) are refused by
/// the readers and the writers: nothing written, 0, and an empty array (PR #1291 review).
#[test]
fn file_paths_cannot_leave_the_layer() {
    let f = fixture();
    let mut g = game(&f);
    for bad in ["..\\..\\x.txt", "C:\\x.txt", "\\x.txt", "Data\\..\\..\\..\\x.txt"] {
        for func in ["pWriteFile", "pWriteFileAlt"] {
            let q = format!("array_var a\nlet a := ar_List \"x\"\nSetFunctionValue ({func} \"{bad}\" a)");
            assert_eq!(query(&mut g, &q), x(0.0), "{func} {bad}");
        }
        let q = format!("array_var a\nlet a := pReadFile \"{bad}\"\nSetFunctionValue (ar_Size a)");
        assert_eq!(query(&mut g, &q), x(0.0), "pReadFile {bad}");
    }
    // The INI writers go through the same check.
    assert_eq!(query(&mut g, "SetINIFloat \"General:fX\" 2 \"..\\..\\Fallout\"\nSetFunctionValue (GetINIFloat \"General:fX\" \"..\\..\\Fallout\")"), x(0.0));
}

/// AX_Set refuses an index past the cap or below 0: 0, nothing set.
#[test]
fn ax_set_is_bounded() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(val(&mut g, "AX_Set 1 \"Big\" 1000000000 Gun"), x(0.0));
    assert_eq!(val(&mut g, "AX_Exists \"Big\" Gun"), x(0.0));
    assert_eq!(val(&mut g, "AX_Set 1 \"Big\" -5 Gun"), x(0.0));
    assert_eq!(val(&mut g, "AX_Set 1 \"Big\" 65535 Gun"), x(1.0));
    assert_eq!(val(&mut g, "AX_Size \"Big\" Gun"), x(65536.0));
}

/// EvaluateString compiles each text once, a failing one too, and at most
/// EVALUATE_MAX texts in a game.
#[test]
fn evaluate_string_is_bounded() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(val(&mut g, "EvaluateString \"Counter < 3\""), x(1.0));
    assert_eq!(val(&mut g, "EvaluateString \"NoSuchThing ((\""), x(0.0));
    assert_eq!(val(&mut g, "EvaluateString \"NoSuchThing ((\""), x(0.0));
    assert_eq!(g.pp.evaluated.len(), 2);
    for n in 0..fnv_game::nvse_pp::EVALUATE_MAX {
        let _ = val(&mut g, &format!("EvaluateString \"Counter < {n}\""));
    }
    assert_eq!(g.pp.evaluated.len(), fnv_game::nvse_pp::EVALUATE_MAX);
    // Texts already compiled still work; a new one past the cap gives 0.
    assert_eq!(val(&mut g, "EvaluateString \"Counter < 3\""), x(1.0));
    assert_eq!(val(&mut g, "EvaluateString \"Counter < 100000\""), x(0.0));
}

