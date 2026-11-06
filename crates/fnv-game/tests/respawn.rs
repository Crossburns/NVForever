//! Cell resets (fnv_game::respawn, docs/spec-notes/dead-on-spawn.md §4): dead actors whose
//! base (or template, with "Base Data") has Respawn come back alive when their cell is
//! loaded again after iHoursToRespawnCell (72 h without a setting); on a synthetic plugin.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

/// Base Health 0 and Respawn: a corpse that comes back alive.
const RESPAWN_CORPSE: u32 = 0x830;
/// Base Health 0 without Respawn: stays a corpse.
const CORPSE: u32 = 0x831;
/// Alive, Respawn: killed, then back.
const RESPAWN_LIVING: u32 = 0x832;
/// Alive, no Respawn: killed, stays dead.
const LIVING: u32 = 0x833;
/// No Respawn of its own; its template (RESPAWN_LIVING) supplies the base data.
const FROM_TEMPLATE: u32 = 0x834;
/// Respawn of its own; its template (LIVING) supplies the base data.
const OWN_FLAG_IGNORED: u32 = 0x835;
const CELL_A: u32 = 0x200;
const CELL_B: u32 = 0x201;
const BASES: [u32; 6] = [RESPAWN_CORPSE, CORPSE, RESPAWN_LIVING, LIVING, FROM_TEMPLATE, OWN_FLAG_IGNORED];
const RESPAWN: u32 = 0x8;
const BASE_DATA: u16 = 0x80;

fn reference(base: u32) -> FormId {
    FormId(0x300 + (base - 0x800))
}

fn acbs(flags: u32, template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = flags.to_le_bytes().to_vec();
    d.extend([0u8; 16]);
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc(fid: u32, edid: &str, health: i32, flags: u32, template: Option<u32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), acbs(flags, if template.is_some() { BASE_DATA } else { 0 })];
    if let Some(t) = template {
        subs.push(sub(b"TPLT", t.to_le_bytes().to_vec()));
    }
    subs.push(sub(b"DATA", [health.to_le_bytes().to_vec(), vec![5; 7]].concat()));
    record(b"NPC_", 0, fid, &subs)
}

fn glob(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

fn placed(base: u32, x: f32) -> Vec<u8> {
    let data = [x, 0.0, 0.0, 0.0, 0.0, 1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    record(b"ACHR", 0, reference(base).0, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn cell(fid: u32, edid: &str, refs: &[u8]) -> Vec<u8> {
    let c = record(b"CELL", 0, fid, &[sub(b"EDID", z(edid))]);
    [c, group(fid.to_le_bytes(), 6, &group(fid.to_le_bytes(), 9, refs))].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [
        npc(RESPAWN_CORPSE, "TestFiendDEAD", 0, RESPAWN, None),
        npc(CORPSE, "TestGhoulDEAD", 0, 0, None),
        npc(RESPAWN_LIVING, "TestFiend", 30, RESPAWN, None),
        npc(LIVING, "TestSettler", 30, 0, None),
        npc(FROM_TEMPLATE, "TestFiendCopy", 30, 0, Some(RESPAWN_LIVING)),
        npc(OWN_FLAG_IGNORED, "TestSettlerCopy", 30, RESPAWN, Some(LIVING)),
    ]
    .concat();
    let globs = [
        glob(0x900, "GameYear", 2281.0),
        glob(0x901, "GameMonth", 9.0),
        glob(0x902, "GameDay", 19.0),
        glob(0x903, "GameHour", 9.0),
        glob(0x904, "GameDaysPassed", 1.0),
    ]
    .concat();
    let refs: Vec<u8> = BASES.iter().enumerate().flat_map(|(i, &b)| placed(b, 100.0 * i as f32)).collect();
    let cells = [cell(CELL_A, "TestCellA", &refs), cell(CELL_B, "TestCellB", &[])].concat();
    [tes4(), top(b"GLOB", &globs), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells)))].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Respawn.esm".into()).unwrap()];
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
    Game::new(&f.data, world, GameConfig::default())
}

fn dead(g: &Game, bases: &[u32]) -> Vec<u32> {
    bases.iter().copied().filter(|&b| g.is_dead(reference(b))).collect()
}

#[test]
fn the_respawn_flag_follows_the_base_data_template_flag() {
    let f = fixture();
    let g = game(&f);
    let respawns: Vec<u32> = BASES.iter().copied().filter(|&b| g.respawns(reference(b))).collect();
    assert_eq!(respawns, vec![RESPAWN_CORPSE, RESPAWN_LIVING, FROM_TEMPLATE]);
}

#[test]
fn respawning_actors_come_back_when_their_cell_is_entered_after_three_days() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.player_entered(Some(FormId(CELL_A)));
    assert_eq!(dead(&g, &BASES), vec![RESPAWN_CORPSE, CORPSE]);
    for b in [RESPAWN_LIVING, LIVING, FROM_TEMPLATE, OWN_FLAG_IGNORED] {
        assert!(g.kill(reference(b), Some(FormId::PLAYER_REF)));
    }
    // The killed Fiend lies elsewhere.
    g.world.get(reference(RESPAWN_LIVING)).unwrap().pos = [900.0, 900.0, 0.0];
    let everyone_dead = BASES.to_vec();
    assert_eq!(dead(&g, &BASES), everyone_dead);

    // Back after 71 hours: too early (and the clock starts again when it is left).
    g.player_entered(Some(FormId(CELL_B)));
    g.world.advance_hours(71.0).unwrap();
    g.player_entered(Some(FormId(CELL_A)));
    assert_eq!(dead(&g, &BASES), everyone_dead, "71 h is not enough");
    assert!(g.take_cell_resets().is_empty());

    g.player_entered(Some(FormId(CELL_B)));
    g.world.advance_hours(72.0).unwrap();
    g.player_entered(Some(FormId(CELL_A)));
    assert_eq!(dead(&g, &BASES), vec![CORPSE, LIVING, OWN_FLAG_IGNORED], "the ones flagged Respawn came back");
    let resets = g.take_cell_resets();
    assert_eq!(resets.len(), 1);
    assert_eq!(resets[0].cell, FormId(CELL_A));
    assert!((resets[0].hours_away - 72.0).abs() < 0.01, "{}", resets[0].hours_away);
    assert_eq!(resets[0].respawned, [RESPAWN_CORPSE, RESPAWN_LIVING, FROM_TEMPLATE].map(reference).to_vec());
    // At its authored place, with no killer, and moved for the AI to pick it up there.
    let s = g.world.peek(reference(RESPAWN_LIVING)).unwrap();
    assert_eq!(s.pos, [200.0, 0.0, 0.0]);
    assert!(g.extra.get(&reference(RESPAWN_LIVING)).is_none_or(|e| e.killer.is_none()));
    assert!(g.take_teleported().contains(&reference(RESPAWN_LIVING)));
    // Staying in the cell resets nothing more.
    g.world.advance_hours(100.0).unwrap();
    g.player_entered(Some(FormId(CELL_A)));
    assert!(g.take_cell_resets().is_empty());
}

/// Fallout 3's ResetInterior: the cell resets at its next load whenever it was left, and
/// no dead actor comes back (respawn.rs, module notes).
#[test]
fn reset_interior_resets_at_the_next_load_without_bringing_actors_back() {
    let f = fixture();
    let mut g = game(&f);
    g.new_game();
    g.player_entered(Some(FormId(CELL_A)));
    assert!(g.kill(reference(RESPAWN_LIVING), Some(FormId::PLAYER_REF)));
    let ctx = fnv_game::functions::Ctx { unit: "test".into(), opcode: 0x1127, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[fnv_script::vm::ArgValue::Value(fnv_script::vm::Value::Ref(CELL_A))]);
    assert!(g.forms.expired_cells.contains(&FormId(CELL_A)));
    // Staying does not reset it: it is reset when loaded again, an hour later.
    g.player_entered(Some(FormId(CELL_A)));
    assert!(g.take_cell_resets().is_empty());
    g.player_entered(Some(FormId(CELL_B)));
    g.world.advance_hours(1.0).unwrap();
    g.player_entered(Some(FormId(CELL_A)));
    let resets = g.take_cell_resets();
    assert_eq!(resets.len(), 1);
    assert!(resets[0].respawned.is_empty(), "a scripted reset brings nobody back");
    assert!(g.is_dead(reference(RESPAWN_LIVING)));
    assert!(g.forms.expired_cells.is_empty());
    // Once only.
    g.player_entered(Some(FormId(CELL_B)));
    g.player_entered(Some(FormId(CELL_A)));
    assert!(g.take_cell_resets().is_empty());
}
