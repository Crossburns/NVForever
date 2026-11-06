//! The combat begin blocks (OnStartCombat, OnCombatEnd), OnReset on a cell reset, and the
//! script functions over the combat AI (docs/design/script-stubs.md §3.6); on a synthetic
//! plugin.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::{CombatRequest, HitInfo};
use fnv_game::functions::Ctx;
use fnv_game::awareness::State;
use fnv_game::game::{Game, GameConfig, GameSnapshot, InstKey};
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const SCRIPT: u32 = 0x920;
const GUARD: u32 = 0x800;
const RAIDER: u32 = 0x801;
const GUARD_REF: FormId = FormId(0x300);
const RAIDER_REF: FormId = FormId(0x301);
const CELL_A: u32 = 0x200;
const CELL_B: u32 = 0x201;
const RESPAWN: u32 = 0x8;

const GET_GROUP_MEMBER_COUNT: u16 = 0x11A0;
const GET_GROUP_TARGET_COUNT: u16 = 0x11A1;
const IS_COMBAT_TARGET: u16 = 0x1203;
const GET_SHOULD_ATTACK: u16 = 0x1042;
const GET_KNOCKED_STATE: u16 = 0x106B;
const GET_CAUSE_OF_DEATH: u16 = 0x118D;
const GET_ARMOR_RATING: u16 = 0x1051;
const FIRE_WEAPON: u16 = 0x11E2;
const FORCE_FLEE: u16 = 0x10F5;
const SET_COMBAT_STYLE: u16 = 0x10B1;

fn r(i: u16) -> Vec<u8> {
    [vec![b'r'], le16(i)].concat()
}

fn acbs(flags: u32) -> fnv_esm::raw::Subrecord {
    let mut d = flags.to_le_bytes().to_vec();
    d.extend([0u8; 16]);
    d.extend(0i16.to_le_bytes());
    d.extend(0u16.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc(fid: u32, edid: &str, flags: u32, script: Option<u32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), acbs(flags)];
    if let Some(s) = script {
        subs.push(sub(b"SCRI", s.to_le_bytes().to_vec()));
    }
    subs.push(sub(b"DATA", [30i32.to_le_bytes().to_vec(), vec![5; 7]].concat()));
    record(b"NPC_", 0, fid, &subs)
}

fn glob(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

fn placed(fid: FormId, base: u32, x: f32) -> Vec<u8> {
    let data = [x, 0.0, 0.0, 0.0, 0.0, 1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    record(b"ACHR", 0, fid.0, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn cell(fid: u32, edid: &str, refs: &[u8]) -> Vec<u8> {
    let c = record(b"CELL", 0, fid, &[sub(b"EDID", z(edid))]);
    [c, group(fid.to_le_bytes(), 6, &group(fid.to_le_bytes(), 9, refs))].concat()
}

fn plugin_bytes() -> Vec<u8> {
    // The guard's script: nStart (1), nEnd (2), nReset (3), nAny (4), all shorts.
    //   begin OnStartCombat player ; set nStart to nStart + 1 ; end
    //   begin OnStartCombat        ; set nAny to nAny + 1     ; end
    //   begin OnCombatEnd          ; set nEnd to nEnd + 1     ; end
    //   begin OnReset              ; set nReset to nReset + 1 ; end
    let inc = |i: u16| set(var(b's', i), ex(&[var(b's', i), a("1"), a("+")]));
    let body = script(&[(0x19, Some([le16(1), r(1)].concat()), inc(1)), (0x19, None, inc(4)), (0x0C, None, inc(2)), (0x1E, None, inc(3))]);
    let scpt = [vec![sub(b"EDID", z("GuardScript"))], unit(0, &body, &[(1, "nStart", 1), (2, "nEnd", 1), (3, "nReset", 1), (4, "nAny", 1)], &[0x14])].concat();
    let npcs = [npc(GUARD, "TestGuard", RESPAWN, Some(SCRIPT)), npc(RAIDER, "TestRaider", 0, None)].concat();
    let globs = [
        glob(0x900, "GameYear", 2281.0),
        glob(0x901, "GameMonth", 9.0),
        glob(0x902, "GameDay", 19.0),
        glob(0x903, "GameHour", 9.0),
        glob(0x904, "GameDaysPassed", 1.0),
    ]
    .concat();
    let refs = [placed(GUARD_REF, GUARD, 0.0), placed(RAIDER_REF, RAIDER, 300.0)].concat();
    let cells = [cell(CELL_A, "TestCellA", &refs), cell(CELL_B, "TestCellB", &[])].concat();
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"SCPT", &record(b"SCPT", 0, SCRIPT, &scpt)),
        top(b"NPC_", &npcs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Combat.esm".into()).unwrap()];
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
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.new_game();
    g.player_entered(Some(FormId(CELL_A)));
    g
}

fn local(g: &Game, i: u16) -> f64 {
    g.var(InstKey::Ref { reference: GUARD_REF }, i).num()
}

fn ask(g: &mut Game, on: FormId, opcode: u16, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, args).num()
}

fn form(f: FormId) -> ArgValue {
    ArgValue::Value(Value::Ref(f.0))
}

#[test]
fn on_start_combat_runs_once_a_fight_and_on_combat_end_when_it_is_over() {
    let f = fixture();
    let mut g = game(&f);
    g.tick();
    assert_eq!((local(&g, 1), local(&g, 4)), (0.0, 0.0));
    // A fight with the player: both OnStartCombat blocks (the one naming the player and
    // the one naming nobody), once.
    g.start_combat(GUARD_REF, FormId::PLAYER_REF);
    g.tick();
    g.tick();
    assert_eq!((local(&g, 1), local(&g, 4), local(&g, 2)), (1.0, 1.0, 0.0));
    g.stop_combat(GUARD_REF);
    g.tick();
    assert_eq!(local(&g, 2), 1.0, "OnCombatEnd once the fight is over");
    g.tick();
    assert_eq!(local(&g, 2), 1.0);
    // A fight with someone else: `OnStartCombat player` does not take it.
    g.start_combat(GUARD_REF, RAIDER_REF);
    g.tick();
    assert_eq!((local(&g, 1), local(&g, 4)), (1.0, 2.0));
    // Dying in the fight ends it without OnCombatEnd.
    g.kill(GUARD_REF, Some(RAIDER_REF));
    g.stop_combat(GUARD_REF);
    g.tick();
    assert_eq!(local(&g, 2), 1.0);
}

/// A save and load through the JSON a save holds.
fn reload<'a>(f: &'a Fixture, g: &Game) -> Game<'a> {
    from_json(f, g, serde_json::to_value(g.snapshot()).unwrap())
}

fn from_json<'a>(f: &'a Fixture, g: &Game, v: Json) -> Game<'a> {
    let s: GameSnapshot = serde_json::from_value(v).unwrap();
    let world = World::restore(&f.forms, &f.refs, &s.sim).unwrap();
    Game::restore(&f.data, world, g.cfg, &s).unwrap()
}

fn set_state(g: &mut Game, actor: FormId, state: State) {
    g.awareness.actors.get_mut(&actor).unwrap().state = state;
}

/// The runs so far of `begin OnStartCombat player`, `begin OnStartCombat`, `begin OnCombatEnd`.
fn counts(g: &Game) -> (f64, f64, f64) {
    (local(g, 1), local(g, 4), local(g, 2))
}

#[test]
fn a_load_in_a_fight_runs_no_second_start_and_its_end_runs_once() {
    let f = fixture();
    let mut g = game(&f);
    g.start_combat(GUARD_REF, FormId::PLAYER_REF);
    g.tick();
    assert_eq!(counts(&g), (1.0, 1.0, 0.0));
    let mut h = reload(&f, &g);
    h.tick();
    h.tick();
    assert_eq!(counts(&h), (1.0, 1.0, 0.0), "the fight the save holds does not start again");
    h.stop_combat(GUARD_REF);
    h.tick();
    h.tick();
    assert_eq!(counts(&h), (1.0, 1.0, 1.0), "it ends once");
}

#[test]
fn a_load_with_the_target_lost_is_the_same_fight_when_it_is_found_again() {
    let f = fixture();
    let mut g = game(&f);
    g.start_combat(GUARD_REF, FormId::PLAYER_REF);
    g.tick();
    set_state(&mut g, GUARD_REF, State::Lost);
    g.tick();
    let mut h = reload(&f, &g);
    h.tick();
    set_state(&mut h, GUARD_REF, State::Combat);
    h.tick();
    assert_eq!(counts(&h), (1.0, 1.0, 0.0), "Lost and back across a load is one fight");
}

#[test]
fn a_start_still_due_when_the_save_was_made_runs_once_after_the_load() {
    let f = fixture();
    let mut g = game(&f);
    g.tick();
    g.start_combat(GUARD_REF, FormId::PLAYER_REF);
    let mut h = reload(&f, &g);
    h.tick();
    h.tick();
    assert_eq!(counts(&h), (1.0, 1.0, 0.0));
}

#[test]
fn a_save_without_the_field_counts_combat_and_lost_as_started_and_not_alert() {
    let f = fixture();
    let mut g = game(&f);
    g.start_combat(GUARD_REF, FormId::PLAYER_REF);
    g.tick();
    for (state, started) in [(State::Combat, true), (State::Lost, true), (State::Alert, false)] {
        set_state(&mut g, GUARD_REF, state);
        let mut v = serde_json::to_value(g.snapshot()).unwrap();
        assert!(v.as_object_mut().unwrap().remove("fighting").is_some());
        let mut h = from_json(&f, &g, v);
        assert_eq!(h.fighting.contains(&GUARD_REF), started, "{state:?}");
        h.tick();
        set_state(&mut h, GUARD_REF, State::Combat);
        h.tick();
        assert_eq!(local(&h, 4), if started { 1.0 } else { 2.0 }, "{state:?}");
        h.stop_combat(GUARD_REF);
        h.tick();
        h.tick();
        assert_eq!(local(&h, 2), 1.0, "{state:?}: its end runs once");
    }
}

#[test]
fn fighters_follow_a_remapped_load_order_and_a_gone_plugin_takes_its_own() {
    let e = |n: &str| fnv_save::PluginEntry { name: n.into(), size: 0 };
    let f = fixture();
    let mut g = game(&f);
    g.start_combat(GUARD_REF, FormId::PLAYER_REF);
    g.tick();
    let mut v = serde_json::to_value(g.snapshot()).unwrap();
    // The save as made with Extra.esm before Combat.esm.
    fnv_save::remap_json(&mut v, &fnv_save::PluginRemap::new(&[e("Combat.esm"), e("Extra.esm")], &[e("Extra.esm"), e("Combat.esm")]));
    assert_eq!(v["fighting"], serde_json::json!(["0x01000300"]));
    // Extra.esm's own fighters: one placed, one created from one of its bases.
    let guard = v["awareness"]["0x01000300"].clone();
    let mut created = v["sim"]["refs"].as_object().unwrap().values().next().cloned().expect("a changed reference");
    created["base"] = serde_json::json!("0x00000ABC");
    v["sim"]["refs"]["0xFF000099"] = created;
    for id in ["0x00000123", "0xFF000099"] {
        v["awareness"][id] = guard.clone();
    }
    v["fighting"] = serde_json::json!(["0x00000123", "0x01000300", "0xFF000099"]);
    // Loaded without Extra.esm.
    fnv_save::remap_json(&mut v, &fnv_save::PluginRemap::new(&[e("Extra.esm"), e("Combat.esm")], &[e("Combat.esm")]));
    assert_eq!(v["fighting"], serde_json::json!(["0x00000300", "0xFF000099"]));
    let mut s: GameSnapshot = serde_json::from_value(v).unwrap();
    assert_eq!(s.drop_gone(|id| fnv_save::is_gone(id.0)), 1, "the created reference");
    assert_eq!(s.fighting, Some([GUARD_REF].into_iter().collect()));
    assert_eq!(s.awareness.keys().copied().collect::<Vec<_>>(), [GUARD_REF]);
    let world = World::restore(&f.forms, &f.refs, &s.sim).unwrap();
    let mut h = Game::restore(&f.data, world, g.cfg, &s).unwrap();
    h.tick();
    assert_eq!(counts(&h), (1.0, 1.0, 0.0));
}

#[test]
fn a_cell_reset_runs_on_reset_and_a_plain_reload_does_not() {
    let f = fixture();
    let mut g = game(&f);
    g.player_entered(Some(FormId(CELL_B)));
    g.world.advance_hours(10.0).unwrap();
    g.player_entered(Some(FormId(CELL_A)));
    assert_eq!(local(&g, 3), 0.0, "no reset after 10 hours");
    g.player_entered(Some(FormId(CELL_B)));
    g.world.advance_hours(72.0).unwrap();
    g.player_entered(Some(FormId(CELL_A)));
    assert_eq!(g.take_cell_resets().len(), 1);
    assert_eq!(local(&g, 3), 1.0, "OnReset after the reset");
}

#[test]
fn combat_groups_targets_and_the_wish_to_attack() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(ask(&mut g, GUARD_REF, GET_GROUP_MEMBER_COUNT, &[]), 0.0, "not in combat");
    assert_eq!(ask(&mut g, GUARD_REF, GET_GROUP_TARGET_COUNT, &[]), 0.0);
    assert_eq!(ask(&mut g, GUARD_REF, GET_SHOULD_ATTACK, &[form(FormId::PLAYER_REF)]), 0.0);
    g.start_combat(GUARD_REF, FormId::PLAYER_REF);
    assert_eq!(ask(&mut g, GUARD_REF, GET_GROUP_MEMBER_COUNT, &[]), 1.0, "alone");
    assert_eq!(ask(&mut g, GUARD_REF, GET_GROUP_TARGET_COUNT, &[]), 1.0);
    assert_eq!(ask(&mut g, GUARD_REF, IS_COMBAT_TARGET, &[form(FormId::PLAYER_REF)]), 1.0);
    assert_eq!(ask(&mut g, GUARD_REF, IS_COMBAT_TARGET, &[form(RAIDER_REF)]), 0.0);
    // StartCombat makes it want to attack.
    assert_eq!(ask(&mut g, GUARD_REF, GET_SHOULD_ATTACK, &[form(FormId::PLAYER_REF)]), 1.0);
    // ForceFlee: from its combat target.
    ask(&mut g, GUARD_REF, FORCE_FLEE, &[]);
    assert_eq!(g.awareness.tactics.flights.get(&GUARD_REF).map(|f| f.from), Some(FormId::PLAYER_REF));
    // No threat, no flight.
    ask(&mut g, RAIDER_REF, FORCE_FLEE, &[]);
    assert!(!g.awareness.tactics.flights.contains_key(&RAIDER_REF));
}

#[test]
fn knockouts_causes_of_death_armour_shots_and_styles() {
    use fnv_game::functions::cause_code;
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(ask(&mut g, GUARD_REF, GET_KNOCKED_STATE, &[]), 0.0);
    g.extra.entry(GUARD_REF).or_default().unconscious = true;
    assert_eq!(ask(&mut g, GUARD_REF, GET_KNOCKED_STATE, &[]), 1.0);
    // Alive: no cause; killed by a weaponless hit: hand to hand; KillActor alone: none.
    assert_eq!(ask(&mut g, RAIDER_REF, GET_CAUSE_OF_DEATH, &[]), -1.0);
    g.hit(RAIDER_REF, 500.0, HitInfo { attacker: Some(GUARD_REF), weapon: None, location: 0, critical: false, limb: None });
    assert!(g.is_dead(RAIDER_REF));
    assert_eq!(ask(&mut g, RAIDER_REF, GET_CAUSE_OF_DEATH, &[]), 4.0);
    g.kill(GUARD_REF, None);
    assert_eq!(ask(&mut g, GUARD_REF, GET_CAUSE_OF_DEATH, &[]), -1.0);
    // The weapon types: hand to hand, melee, guns, explosives, thrown.
    let codes: Vec<i32> = (0..=13).map(|t| cause_code(Some(t))).collect();
    assert_eq!(codes, [4, 3, 3, 2, 2, 2, 2, 2, 2, 0, 0, 0, 0, 5]);
    // Nothing worn: 0.
    assert_eq!(ask(&mut g, FormId::PLAYER_REF, GET_ARMOR_RATING, &[]), 0.0);
    // FireWeapon asks the engine for a shot.
    ask(&mut g, GUARD_REF, FIRE_WEAPON, &[form(FormId(0x123))]);
    assert_eq!(g.combat_requests, vec![CombatRequest::Fire { from: GUARD_REF, weapon: FormId(0x123) }]);
    // SetCombatStyle refuses a form that is not a combat style.
    ask(&mut g, RAIDER_REF, SET_COMBAT_STYLE, &[form(FormId(SCRIPT))]);
    assert!(g.extra.get(&RAIDER_REF).is_none_or(|e| e.combat_style.is_none()));
}
