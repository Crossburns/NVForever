//! The result-script sweep (`fnv_game::sweep`) on a plugin built in memory (no game
//! data): scripts that never stop calling themselves end at a guard and are listed as
//! causes, and the sweep goes on to the next script instead of overflowing its stack.
//! - Stage 10 of RecQ (Allow Repeated Stages) sets stage 10 again (`SetStage RecQ 10`): the nesting guard
//!   (`fnv_game::game::MAX_DEPTH`) stops it, a `script_error` cause.
//! - Stage 20 makes ARef choose its package (`EvaluatePackage`); the first package of its
//!   list has the condition GetIsCurrentPackage, which asked for the current package of
//!   an actor that had none and started the choice again, without end (the TTW load
//!   order overflowed the stack this way, FalloutNV.esm INFO 13F680). Now it is a
//!   `skipped` cause and the actor gets the next valid package.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::compile::{Env, FormInfo};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x900u32.to_le_bytes()].concat();
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

const QUEST: u32 = 0x810;
const P_SELF: u32 = 0x820;
const P_DEFAULT: u32 = 0x821;
const NPC_A: u32 = 0x840;
const CELL: u32 = 0x860;
const REF_A: u32 = 0x871;
const GET_IS_CURRENT_PACKAGE: u16 = 0xA1;
const SANDBOX: u8 = 12;

/// The editor IDs the stage scripts name.
struct Names;

impl Env for Names {
    fn form(&self, e: &str) -> Option<FormInfo> {
        let (form, sig) = match e.to_ascii_lowercase().as_str() {
            "recq" => (QUEST, *b"QUST"),
            "aref" => (REF_A, *b"ACHR"),
            _ => return None,
        };
        Some(FormInfo { form, sig })
    }
    fn info(&self, _: u32) -> Option<FormInfo> {
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

/// A stage result script compiled by NVForever's compiler: SCHR, SCDA, SCRO list, SCTX.
fn stage_script(table: &FunctionTable, src: &str) -> Vec<Subrecord> {
    let c = fnv_script::compile::compile(src, table, &Names, None);
    assert!(c.errors().next().is_none(), "{src}: {:?}", c.diags);
    let refs: Vec<u32> = c.refs.iter().filter_map(|r| if let fnv_script::unit::RefSlot::Form(f) = r { Some(*f) } else { None }).collect();
    let mut schr = vec![0u8; 4];
    schr.extend((refs.len() as u32).to_le_bytes());
    schr.extend((c.bytes.len() as u32).to_le_bytes());
    schr.extend(0u32.to_le_bytes());
    schr.extend(0u16.to_le_bytes());
    schr.extend(1u16.to_le_bytes());
    let mut out = vec![sub(b"SCHR", schr), sub(b"SCDA", c.bytes.clone())];
    out.extend(refs.iter().map(|r| sub(b"SCRO", r.to_le_bytes().to_vec())));
    out.push(sub(b"SCTX", src.as_bytes().to_vec()));
    out
}

fn pack(id: u32, edid: &str, condition: Option<(u16, u32)>) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"PKDT", [&0u32.to_le_bytes()[..], &[SANDBOX, 0], &[0; 6]].concat()),
        sub(b"PLDT", [3i32.to_le_bytes(), 0u32.to_le_bytes(), 0i32.to_le_bytes()].concat()),
        sub(b"PSDT", [&[0xFF, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
    ];
    if let Some((function, p1)) = condition {
        let mut d = vec![0u8, 0, 0, 0];
        d.extend(1.0f32.to_le_bytes());
        d.extend(function.to_le_bytes());
        d.extend([0, 0]);
        d.extend(p1.to_le_bytes());
        d.extend([0u8; 12]);
        subs.push(sub(b"CTDA", d));
    }
    record(b"PACK", id, &subs)
}

fn plugin_bytes(table: &FunctionTable) -> Vec<u8> {
    // Allow Repeated Stages, so that stage 10 runs again each time it is set.
    let mut q = vec![sub(b"EDID", z("RecQ")), sub(b"DATA", [vec![fnv_game::quest::ALLOW_REPEATED_STAGES, 0, 0, 0], 0f32.to_le_bytes().to_vec()].concat())];
    q.push(sub(b"INDX", 10i16.to_le_bytes().to_vec()));
    q.push(sub(b"QSDT", vec![0]));
    q.extend(stage_script(table, "SetStage RecQ 10\n"));
    q.push(sub(b"INDX", 20i16.to_le_bytes().to_vec()));
    q.push(sub(b"QSDT", vec![0]));
    q.extend(stage_script(table, "ARef.EvaluatePackage\n"));
    let quest = record(b"QUST", QUEST, &q);
    let packs = [pack(P_SELF, "WhileCurrent", Some((GET_IS_CURRENT_PACKAGE, P_SELF))), pack(P_DEFAULT, "Default", None)].concat();
    let npcs = [
        record(b"NPC_", 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", NPC_A, &[sub(b"EDID", z("A")), sub(b"PKID", P_SELF.to_le_bytes().to_vec()), sub(b"PKID", P_DEFAULT.to_le_bytes().to_vec())]),
    ]
    .concat();
    let aref = record(b"ACHR", REF_A, &[sub(b"EDID", z("ARef")), sub(b"NAME", NPC_A.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell = [record(b"CELL", CELL, &[sub(b"EDID", z("TestCell"))]), group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &aref))].concat();
    [
        tes4(),
        top(b"NPC_", &npcs),
        top(b"PACK", &packs),
        top(b"QUST", &quest),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&p).unwrap();
    let plugins = vec![Plugin::parse(&plugin_bytes(&table), "Recursion.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

#[test]
fn a_package_condition_asking_for_the_current_package_does_not_choose_again() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig::default());
    // No current package yet: GetIsCurrentPackage WhileCurrent is false, Default runs.
    assert_eq!(g.evaluate_package(FormId(REF_A)), Some(FormId(P_DEFAULT)));
    assert!(g.events.iter().any(|e| matches!(&e.kind, EventKind::Skipped { what, .. } if what.contains("asked by a package condition"))));
}

#[test]
fn self_recursive_scripts_end_at_a_guard_and_are_listed_as_causes() {
    let f = fixture();
    let (r, _) = fnv_game::sweep::sweep(&f.data, &f.forms, &f.refs, 5);
    assert_eq!(r["summary"]["scripts"]["stage"]["run"], 2, "{r:#}");
    assert_eq!(r["summary"]["scripts"]["stage"]["with_problems"], 2, "{r:#}");
    let causes = |kind: &str| -> Vec<String> {
        r["causes"][kind].as_array().into_iter().flatten().map(|c| format!("{} {}", c["cause"].as_str().unwrap_or(""), c["e.g."])).collect()
    };
    let errors = causes("script_error");
    assert!(errors.iter().any(|c| c.starts_with("nesting deeper than #") && c.contains("stage 10")), "{r:#}");
    let skipped = causes("skipped");
    assert!(skipped.iter().any(|c| c.contains("asked by a package condition") && c.contains("stage 20")), "{r:#}");
}
