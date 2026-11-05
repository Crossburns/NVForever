//! Unconscious actors (docs/spec-notes/unconscious.md): essential actors at zero health
//! and actors out of fatigue are knocked out, answer the script functions as the wiki
//! says, are left alone, and get up again; on a synthetic plugin.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::HitInfo;
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::unconscious::{Cause, ESSENTIAL_DOWN, ESSENTIAL_UP_HEALTH, FATIGUE_DOWN, GETUP_MAX};
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const HERO: u32 = 0x800;
const BOXER: u32 = 0x801;
const HERO_REF: FormId = FormId(0x300);
const BOXER_REF: FormId = FormId(0x301);
const CELL: u32 = 0x200;
/// ACBS flags [documented: xEdit]: Essential.
const ESSENTIAL: u32 = 0x2;

const GET_ACTOR_VALUE: u16 = 0x100E;
const DAMAGE_ACTOR_VALUE: u16 = 0x1181;
const GET_DEAD: u16 = 0x102E;
const GET_HIT_LOCATION: u16 = 0x1187;
const GET_CAUSE_OF_DEATH: u16 = 0x118D;
const GET_KILLING_BLOW_LIMB: u16 = 0x11F0;
const KILL_ACTOR: u16 = 0x108B;
const RESURRECT_ACTOR: u16 = 0x108C;
const GET_UNCONSCIOUS: u16 = 0x10F2;
const IS_FACING_UP: u16 = 0x106A;
const GET_KNOCKED_STATE: u16 = 0x106B;
const HEALTH: i32 = 16;
const FATIGUE: i32 = 22;

fn acbs(flags: u32, fatigue: u16) -> fnv_esm::raw::Subrecord {
    let mut d = flags.to_le_bytes().to_vec();
    d.extend(fatigue.to_le_bytes());
    d.extend([0u8; 14]);
    d.extend(0i16.to_le_bytes());
    d.extend(0u16.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc(fid: u32, edid: &str, flags: u32) -> Vec<u8> {
    let subs = vec![sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), acbs(flags, 50), sub(b"DATA", [30i32.to_le_bytes().to_vec(), vec![5; 7]].concat())];
    record(b"NPC_", 0, fid, &subs)
}

fn placed(fid: FormId, base: u32, x: f32) -> Vec<u8> {
    let data = [x, 0.0, 0.0, 0.0, 0.0, 1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    record(b"ACHR", 0, fid.0, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    let npcs = [npc(HERO, "TestHero", ESSENTIAL), npc(BOXER, "TestBoxer", 0)].concat();
    let refs = [placed(HERO_REF, HERO, 0.0), placed(BOXER_REF, BOXER, 300.0)].concat();
    let c = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let cells = [c, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs))].concat();
    [tes4(), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells)))].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Knockout.esm".into()).unwrap()];
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
    g.player_entered(Some(FormId(CELL)));
    g
}

fn ask(g: &mut Game, on: FormId, opcode: u16, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, args).num()
}

fn av(code: i32) -> ArgValue {
    ArgValue::Enum(code as u16)
}

fn num(x: f64) -> ArgValue {
    ArgValue::Value(Value::Num(x))
}

/// Run game frames for `seconds` of game time.
fn wait(g: &mut Game, seconds: f64) {
    let n = (seconds / g.cfg.frame_seconds).ceil() as usize;
    for _ in 0..n {
        g.tick();
    }
}

fn events(g: &Game, pick: impl Fn(&EventKind) -> bool) -> usize {
    g.events.iter().filter(|e| pick(&e.kind)).count()
}

/// Getting up: still knocked out (it does not move) until its clip is over.
fn rising(g: &Game, r: FormId) -> bool {
    g.knockout(r).is_some_and(|k| !k.lying()) && g.is_out_cold(r)
}

fn by_player() -> HitInfo {
    HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 0, critical: false, limb: None }
}

#[test]
fn an_essential_actor_beaten_to_zero_is_knocked_out_not_killed_and_gets_up() {
    let f = fixture();
    let mut g = game(&f);
    let max = ask(&mut g, HERO_REF, GET_ACTOR_VALUE, &[av(HEALTH)]);
    assert!(max > 1.0);
    let r = g.hit(HERO_REF, max * 10.0, by_player());
    assert!(r.knocked_out && !r.died && r.xp == 0.0, "{r:?}");
    assert_eq!(r.health, 1.0);
    assert!(!g.is_dead(HERO_REF));
    assert_eq!(g.knockout(HERO_REF).map(|k| k.cause), Some(Cause::Essential));
    assert_eq!(events(&g, |k| matches!(k, EventKind::KnockedOut { actor, cause: "essential" } if *actor == HERO_REF)), 1);
    // The wiki's readings: knocked out (1), not dead, not SetUnconscious's.
    assert_eq!(ask(&mut g, HERO_REF, GET_KNOCKED_STATE, &[]), 1.0);
    assert_eq!(ask(&mut g, HERO_REF, GET_DEAD, &[]), 0.0);
    assert_eq!(ask(&mut g, HERO_REF, GET_UNCONSCIOUS, &[]), 0.0);
    // Hit again while down: nothing new, still at 1.
    let again = g.hit(HERO_REF, 50.0, by_player());
    assert!(!again.knocked_out && !again.died && again.health == 1.0, "{again:?}");
    assert_eq!(events(&g, |k| matches!(k, EventKind::KnockedOut { .. })), 1);
    // Nobody fights it or talks to it while it is down.
    assert!(!g.would_attack(BOXER_REF, HERO_REF));
    assert!(g.start_dialogue(HERO_REF, None).is_err());
    assert_eq!(events(&g, |k| matches!(k, EventKind::TalkedToUnconscious { actor } if *actor == HERO_REF)), 1);
    // The engine saw it come to rest face down: IsFacingUp reads it.
    g.set_knocked_side(HERO_REF, false);
    assert_eq!(ask(&mut g, HERO_REF, IS_FACING_UP, &[]), 0.0);
    // Up after its time, with half its health.
    wait(&mut g, ESSENTIAL_DOWN - 1.0);
    assert!(g.knockout(HERO_REF).is_some());
    wait(&mut g, 1.5);
    assert!(rising(&g, HERO_REF), "getting up: still held");
    assert!(!g.would_attack(BOXER_REF, HERO_REF));
    assert_eq!(events(&g, |k| matches!(k, EventKind::GotUp { actor, .. } if *actor == HERO_REF)), 1);
    // The engine's get-up clip is over: it may move again.
    g.finish_getting_up(HERO_REF);
    assert!(g.knockout(HERO_REF).is_none() && !g.is_out_cold(HERO_REF));
    let health = ask(&mut g, HERO_REF, GET_ACTOR_VALUE, &[av(HEALTH)]);
    assert!((health - max * ESSENTIAL_UP_HEALTH).abs() < 1e-6, "{health} of {max}");
    assert_eq!(ask(&mut g, HERO_REF, GET_KNOCKED_STATE, &[]), 0.0);
    assert_eq!(ask(&mut g, HERO_REF, IS_FACING_UP, &[]), 1.0);
}

#[test]
fn kill_actor_knocks_an_essential_actor_out() {
    let f = fixture();
    let mut g = game(&f);
    ask(&mut g, HERO_REF, KILL_ACTOR, &[]);
    assert!(!g.is_dead(HERO_REF));
    assert_eq!(g.knockout(HERO_REF).map(|k| k.cause), Some(Cause::Essential));
    // ResurrectActor ends it at once.
    ask(&mut g, HERO_REF, RESURRECT_ACTOR, &[]);
    assert!(g.knockout(HERO_REF).is_none());
    // The others still die.
    ask(&mut g, BOXER_REF, KILL_ACTOR, &[]);
    assert!(g.is_dead(BOXER_REF));
}

/// "Essential flagged teammates are not essential in Hardcore (New Vegas)" [documented:
/// GECK wiki SetPlayerTeammate notes].
#[test]
fn an_essential_teammate_dies_in_hardcore_and_stays_teammate_free() {
    const SET_PLAYER_TEAMMATE: u16 = 0x11C5;
    let f = fixture();
    // Outside Hardcore the essential teammate is knocked out.
    let mut g = game(&f);
    ask(&mut g, HERO_REF, SET_PLAYER_TEAMMATE, &[num(1.0)]);
    assert!(g.is_teammate(HERO_REF));
    ask(&mut g, HERO_REF, KILL_ACTOR, &[]);
    assert!(!g.is_dead(HERO_REF));
    assert_eq!(g.knockout(HERO_REF).map(|k| k.cause), Some(Cause::Essential));
    // In Hardcore it dies, and its teammate state goes with it.
    let mut g = game(&f);
    g.set_hardcore(true);
    ask(&mut g, HERO_REF, SET_PLAYER_TEAMMATE, &[num(1.0)]);
    ask(&mut g, HERO_REF, KILL_ACTOR, &[]);
    assert!(g.is_dead(HERO_REF));
    assert!(g.knockout(HERO_REF).is_none());
    assert!(!g.is_teammate(HERO_REF));
    wait(&mut g, ESSENTIAL_DOWN + GETUP_MAX + 1.0);
    assert!(g.is_dead(HERO_REF), "a dead Hardcore companion does not get up");
    // An essential actor that is not a teammate keeps its protection in Hardcore.
    let mut g = game(&f);
    g.set_hardcore(true);
    ask(&mut g, HERO_REF, KILL_ACTOR, &[]);
    assert!(!g.is_dead(HERO_REF));
}

#[test]
fn an_actor_out_of_fatigue_is_knocked_out_for_a_set_time() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(ask(&mut g, BOXER_REF, GET_ACTOR_VALUE, &[av(FATIGUE)]), 50.0);
    // The boxing gloves' 35 a hit: the first leaves it standing, the second floors it.
    ask(&mut g, BOXER_REF, DAMAGE_ACTOR_VALUE, &[av(FATIGUE), num(35.0)]);
    assert!(g.knockout(BOXER_REF).is_none());
    ask(&mut g, BOXER_REF, DAMAGE_ACTOR_VALUE, &[av(FATIGUE), num(35.0)]);
    assert_eq!(g.knockout(BOXER_REF).map(|k| k.cause), Some(Cause::Fatigue));
    assert_eq!(events(&g, |k| matches!(k, EventKind::KnockedOut { actor, cause: "fatigue" } if *actor == BOXER_REF)), 1);
    assert_eq!(ask(&mut g, BOXER_REF, GET_KNOCKED_STATE, &[]), -1.0);
    assert_eq!(ask(&mut g, BOXER_REF, GET_UNCONSCIOUS, &[]), 0.0);
    // Up after its set time, whether a fight goes on or not, with its fatigue whole.
    g.start_combat(HERO_REF, FormId::PLAYER_REF);
    wait(&mut g, FATIGUE_DOWN - 0.5);
    assert!(g.knockout(BOXER_REF).is_some_and(|k| k.lying()));
    wait(&mut g, 1.0);
    assert!(rising(&g, BOXER_REF));
    // Not drawn, nobody ends its getting up: over after GETUP_MAX.
    wait(&mut g, GETUP_MAX + 0.1);
    assert!(g.knockout(BOXER_REF).is_none());
    assert_eq!(ask(&mut g, BOXER_REF, GET_ACTOR_VALUE, &[av(FATIGUE)]), 50.0);
}

#[test]
fn a_fatigue_knockout_can_still_be_killed_and_an_essential_one_becomes_essential() {
    let f = fixture();
    let mut g = game(&f);
    ask(&mut g, BOXER_REF, DAMAGE_ACTOR_VALUE, &[av(FATIGUE), num(80.0)]);
    assert!(g.knockout(BOXER_REF).is_some());
    let r = g.hit(BOXER_REF, 10_000.0, by_player());
    assert!(r.died);
    wait(&mut g, 0.1);
    assert!(g.knockout(BOXER_REF).is_none() && g.is_dead(BOXER_REF));
    // The essential one out of fatigue, then beaten to zero: an essential knockout.
    ask(&mut g, HERO_REF, DAMAGE_ACTOR_VALUE, &[av(FATIGUE), num(80.0)]);
    assert_eq!(g.knockout(HERO_REF).map(|k| k.cause), Some(Cause::Fatigue));
    let r = g.hit(HERO_REF, 10_000.0, by_player());
    assert!(r.knocked_out && !r.died);
    assert_eq!(g.knockout(HERO_REF).map(|k| k.cause), Some(Cause::Essential));
}

#[test]
fn an_essential_actor_stays_down_while_a_fight_goes_on_near_it() {
    let f = fixture();
    let mut g = game(&f);
    ask(&mut g, HERO_REF, KILL_ACTOR, &[]);
    assert!(g.knockout(HERO_REF).is_some());
    // The boxer, 300 units off, fights the player: past its time the hero stays down.
    g.start_combat(BOXER_REF, FormId::PLAYER_REF);
    wait(&mut g, ESSENTIAL_DOWN + 5.0);
    assert!(g.knockout(HERO_REF).is_some(), "held down by the fight");
    assert!(g.knockout(HERO_REF).is_some_and(|k| k.held));
    // The fight is over: it gets up.
    g.stop_combat(BOXER_REF);
    wait(&mut g, 1.0);
    assert!(rising(&g, HERO_REF));
}

/// DamageActorValue past zero is a death as a hit's is, with nobody to credit (#1568).
#[test]
fn damage_actor_value_past_zero_kills_with_no_killer() {
    let f = fixture();
    let mut g = game(&f);
    let xp = g.player.xp;
    ask(&mut g, BOXER_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(10.0)]);
    assert!(!g.is_dead(BOXER_REF), "health left: it stands");
    ask(&mut g, BOXER_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert!(g.is_dead(BOXER_REF));
    assert_eq!(ask(&mut g, BOXER_REF, GET_DEAD, &[]), 1.0);
    assert_eq!(g.extra[&BOXER_REF].killer, None);
    assert_eq!(g.player.dead_count.get(&FormId(BOXER)), Some(&1));
    assert_eq!(g.player.xp, xp, "no killer, no XP");
    // More damage on the corpse is no second death.
    ask(&mut g, BOXER_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert_eq!(g.player.dead_count.get(&FormId(BOXER)), Some(&1));
}

/// The wound before it is not what killed (#1588).
#[test]
fn a_damage_actor_value_death_has_no_killing_hit() {
    let f = fixture();
    let mut g = game(&f);
    assert!(!g.hit(BOXER_REF, 5.0, HitInfo { location: 3, ..by_player() }).died);
    ask(&mut g, BOXER_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert!(g.is_dead(BOXER_REF));
    assert_eq!(ask(&mut g, BOXER_REF, GET_KILLING_BLOW_LIMB, &[]), -1.0);
    assert_eq!(ask(&mut g, BOXER_REF, GET_CAUSE_OF_DEATH, &[]), -1.0);
    assert_eq!(ask(&mut g, BOXER_REF, GET_HIT_LOCATION, &[]), 3.0);
}

#[test]
fn damage_actor_value_past_zero_knocks_an_essential_actor_out_and_it_gets_up() {
    let f = fixture();
    let mut g = game(&f);
    let max = ask(&mut g, HERO_REF, GET_ACTOR_VALUE, &[av(HEALTH)]);
    ask(&mut g, HERO_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert!(!g.is_dead(HERO_REF));
    assert_eq!(g.knockout(HERO_REF).map(|k| k.cause), Some(Cause::Essential));
    // Again while down: nothing new.
    ask(&mut g, HERO_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert_eq!(events(&g, |k| matches!(k, EventKind::KnockedOut { actor, cause: "essential" } if *actor == HERO_REF)), 1);
    // Up after its time at half its health, however far below zero it was.
    wait(&mut g, ESSENTIAL_DOWN + 1.0);
    assert!(rising(&g, HERO_REF));
    let health = ask(&mut g, HERO_REF, GET_ACTOR_VALUE, &[av(HEALTH)]);
    assert!((health - max * ESSENTIAL_UP_HEALTH).abs() < 1e-6, "{health} of {max}");
}

#[test]
fn damage_actor_value_past_zero_kills_an_essential_teammate_in_hardcore() {
    const SET_PLAYER_TEAMMATE: u16 = 0x11C5;
    let f = fixture();
    let mut g = game(&f);
    g.set_hardcore(true);
    ask(&mut g, HERO_REF, SET_PLAYER_TEAMMATE, &[num(1.0)]);
    ask(&mut g, HERO_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert!(g.is_dead(HERO_REF));
    assert!(g.knockout(HERO_REF).is_none());
}

#[test]
fn damage_actor_value_past_zero_kills_the_player_unless_in_god_mode() {
    let f = fixture();
    let mut g = game(&f);
    g.toggles.god_mode = true;
    ask(&mut g, FormId::PLAYER_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert!(!g.is_dead(FormId::PLAYER_REF));
    let mut g = game(&f);
    ask(&mut g, FormId::PLAYER_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert!(g.is_dead(FormId::PLAYER_REF));
    assert_eq!(ask(&mut g, FormId::PLAYER_REF, GET_DEAD, &[]), 1.0);
}

#[test]
fn damage_actor_value_does_not_kill_a_ghost() {
    const SET_GHOST: u16 = 0x10EC;
    let f = fixture();
    let mut g = game(&f);
    ask(&mut g, BOXER_REF, SET_GHOST, &[num(1.0)]);
    ask(&mut g, BOXER_REF, DAMAGE_ACTOR_VALUE, &[av(HEALTH), num(9999.0)]);
    assert!(!g.is_dead(BOXER_REF));
}
