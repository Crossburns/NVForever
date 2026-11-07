//! Knockdowns (docs/spec-notes/unconscious.md §5): PushActorAway and explosions lay a
//! live actor down for a moment, GetKnockedState answers as the wiki says, SetUnconscious
//! holds a lying actor down, and a pushed actor's inventory can be opened; on a synthetic
//! plugin.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::unconscious::{Cause, GETUP_MAX, KNOCKDOWN_DOWN, PUSH_RISE, PUSH_SPEED};
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::world::Outcome;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const HERO: u32 = 0x800;
const THUG: u32 = 0x801;
const ROCK: u32 = 0x802;
const THUGS: u32 = 0x803;
const HERO_REF: FormId = FormId(0x300);
const THUG_REF: FormId = FormId(0x301);
const ROCK_REF: FormId = FormId(0x302);
const LEVELED_REF: FormId = FormId(0x303);
const CELL: u32 = 0x200;
/// ACBS flags [documented: xEdit]: Essential, No Knockdowns.
const ESSENTIAL: u32 = 0x2;
const NO_KNOCKDOWNS: u32 = 1 << 26;

const KILL_ACTOR: u16 = 0x108B;
const SET_UNCONSCIOUS: u16 = 0x10F1;
const GET_UNCONSCIOUS: u16 = 0x10F2;
const GET_KNOCKED_STATE: u16 = 0x106B;
const PUSH_ACTOR_AWAY: u16 = 0x118A;

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
    let npcs = [npc(HERO, "TestHero", ESSENTIAL), npc(THUG, "TestThug", 0), npc(ROCK, "TestRock", NO_KNOCKDOWNS)].concat();
    let refs = [placed(HERO_REF, HERO, 0.0), placed(THUG_REF, THUG, 300.0), placed(ROCK_REF, ROCK, 600.0), placed(LEVELED_REF, THUGS, 900.0)].concat();
    let lvlo = [1i16.to_le_bytes().to_vec(), vec![0; 2], THUG.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat();
    let thugs = record(b"LVLN", 0, THUGS, &[sub(b"EDID", z("TestThugs")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]), sub(b"LVLO", lvlo)]);
    let c = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let cells = [c, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs))].concat();
    [tes4(), top(b"NPC_", &npcs), top(b"LVLN", &thugs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells)))].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Knockdown.esm".into()).unwrap()];
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

fn ask(g: &mut Game, on: Option<FormId>, opcode: u16, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode, on, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, args).num()
}

fn knocked_state(g: &mut Game, r: FormId) -> f64 {
    ask(g, Some(r), GET_KNOCKED_STATE, &[])
}

fn push(g: &mut Game, source: Option<FormId>, target: FormId, force: f64) {
    ask(g, source, PUSH_ACTOR_AWAY, &[ArgValue::Value(Value::Ref(target.0)), ArgValue::Value(Value::Num(force))]);
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

fn opened_container(g: &mut Game, r: FormId) -> bool {
    let v = g.default_activation(FormId::PLAYER_REF, r, true).unwrap();
    v.pointer("/result").and_then(Json::as_str) == Some("opened_container")
}

#[test]
fn push_actor_away_lays_an_actor_down_and_throws_it_away_from_the_source() {
    let f = fixture();
    let mut g = game(&f);
    // The hero (x 0) pushes the thug (x 300) with force 10: thrown along +x, a little up.
    push(&mut g, Some(HERO_REF), THUG_REF, 10.0);
    let k = g.knockout(THUG_REF).expect("knocked down");
    assert_eq!(k.cause, Cause::Pushed);
    assert!(k.pushed && k.lying());
    assert_eq!(g.events.iter().filter(|e| matches!(e.kind, EventKind::KnockedOut { actor, cause: "pushed" } if actor == THUG_REF)).count(), 1);
    let v = g.take_shove(THUG_REF).expect("a shove for the engine");
    let speed = 10.0 * PUSH_SPEED;
    assert!((v[0] - speed).abs() < 1e-3 && v[1].abs() < 1e-3 && (v[2] - PUSH_RISE.min(speed)).abs() < 1e-3, "{v:?}");
    assert!(g.take_shove(THUG_REF).is_none(), "taken once");
    // The wiki: GetKnockedState reads 0 for a pushed actor; it does not fight or talk.
    assert_eq!(knocked_state(&mut g, THUG_REF), 0.0);
    assert!(g.is_out_cold(THUG_REF));
    assert!(!g.would_attack(HERO_REF, THUG_REF));
    // Its inventory can be opened until it gets up.
    assert!(opened_container(&mut g, THUG_REF));
    // Up after KNOCKDOWN_DOWN, then its get-up clip (not drawn here: GETUP_MAX).
    wait(&mut g, KNOCKDOWN_DOWN + 0.1);
    assert!(g.knockout(THUG_REF).is_some_and(|k| !k.lying()), "getting up");
    assert!(!opened_container(&mut g, THUG_REF), "getting up: no longer searchable");
    wait(&mut g, GETUP_MAX + 0.1);
    assert!(g.knockout(THUG_REF).is_none() && !g.is_out_cold(THUG_REF));
}

/// #1615: the player's own hand opens a pushed-down actor's inventory, as a script's
/// activation does [documented: GECK wiki PushActorAway notes].
#[test]
fn the_players_own_hand_searches_a_pushed_down_actor() {
    let f = fixture();
    let mut g = game(&f);
    push(&mut g, Some(HERO_REF), THUG_REF, 10.0);
    let done = g.default_activation_by_hand(THUG_REF).unwrap().expect("not refused");
    assert_eq!(done.activation.outcome, Outcome::OpenedContainer { container: THUG_REF });
}

/// The body of an actor placed through a leveled list is searched like any other's, by the
/// player's own hand and by a script's activation.
#[test]
fn a_dead_actor_placed_through_a_leveled_list_opens_its_inventory() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.kill(LEVELED_REF, None));
    let done = g.default_activation_by_hand(LEVELED_REF).unwrap().expect("not refused");
    assert_eq!(done.activation.outcome, Outcome::OpenedContainer { container: LEVELED_REF });
    assert!(opened_container(&mut g, LEVELED_REF));
}

#[test]
fn push_without_a_source_throws_straight_up_and_a_negative_force_pulls() {
    let f = fixture();
    let mut g = game(&f);
    push(&mut g, None, THUG_REF, 5.0);
    let v = g.take_shove(THUG_REF).unwrap();
    assert!(v[0].abs() < 1e-3 && v[1].abs() < 1e-3 && (v[2] - 5.0 * PUSH_SPEED).abs() < 1e-3, "{v:?}");
    // Pushed again while lying: still one knockdown, thrown again, towards the hero.
    push(&mut g, Some(HERO_REF), THUG_REF, -4.0);
    let v = g.take_shove(THUG_REF).unwrap();
    assert!(v[0] < 0.0 && v[2] > 0.0, "{v:?}");
    assert_eq!(g.events.iter().filter(|e| matches!(e.kind, EventKind::KnockedOut { .. })).count(), 1);
}

#[test]
fn push_actor_away_spares_the_dead_and_actors_flagged_no_knockdowns() {
    let f = fixture();
    let mut g = game(&f);
    push(&mut g, Some(HERO_REF), ROCK_REF, 10.0);
    assert!(g.knockout(ROCK_REF).is_none(), "No Knockdowns");
    ask(&mut g, Some(THUG_REF), KILL_ACTOR, &[]);
    push(&mut g, Some(HERO_REF), THUG_REF, 10.0);
    assert!(g.knockout(THUG_REF).is_none(), "dead");
    // Explosions follow the same refusals.
    assert!(!g.knock_down(ROCK_REF, None, [0.0, 0.0, 100.0]));
}

/// The player is knocked down like anyone (unconscious.md §5.4): PushActorAway and
/// explosions lay the player down for the knockdown's time, then the getting up waits for
/// the engine's clip; enemies may still attack; the player is never knocked out.
#[test]
fn the_player_is_knocked_down_but_never_knocked_out() {
    let f = fixture();
    let mut g = game(&f);
    let me = FormId::PLAYER_REF;
    push(&mut g, Some(HERO_REF), me, 10.0);
    let k = g.knockout(me).expect("the player lies pushed");
    assert_eq!(k.cause, Cause::Pushed);
    assert!(k.pushed && k.lying());
    assert_eq!(knocked_state(&mut g, me), 0.0, "pushed reads 0");
    let shove = g.take_shove(me).expect("thrown");
    assert!(shove[2] > 0.0);
    // A blow that would knock out (essential, fatigue) does not.
    assert!(!g.knock_out(me, Cause::Essential, None));
    assert!(!g.knock_out(me, Cause::Fatigue, None));
    assert_eq!(g.knockout(me).map(|k| k.cause), Some(Cause::Pushed));
    // Up after the knockdown's time; getting up until the engine's clip is over.
    // The player's own time down [observed], shorter than an NPC's.
    wait(&mut g, fnv_game::unconscious::PLAYER_KNOCKDOWN_DOWN - 0.3);
    assert!(g.knockout(me).is_some_and(|k| k.lying()));
    wait(&mut g, 0.6);
    let k = g.knockout(me).expect("getting up");
    assert!(!k.lying());
    g.finish_getting_up(me);
    assert!(g.knockout(me).is_none());
    // An explosion: GetKnockedState 2.
    assert!(g.knock_down(me, None, [0.0, 0.0, 100.0]));
    assert_eq!(knocked_state(&mut g, me), 2.0);
    g.end_knockout(me);
    // Not seated and not talking only.
    g.set_furniture_use(me, Some(fnv_game::game::FurnitureUse { reference: HERO_REF, marker_id: 0, state: 3 }));
    assert!(!g.can_knock_down(me));
    g.set_furniture_use(me, None);
    assert!(g.can_knock_down(me));
}

#[test]
fn an_explosion_knockdown_reads_2_and_an_essential_knocked_out_while_pushed_reads_0() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.knock_down(THUG_REF, Some(FormId::PLAYER_REF), [100.0, 0.0, 50.0]));
    assert_eq!(knocked_state(&mut g, THUG_REF), 2.0);
    assert!(!opened_container(&mut g, THUG_REF), "only a push makes it searchable");
    // No conversation while down, but not told as unconscious.
    assert!(g.start_dialogue(THUG_REF, None).is_err());
    assert_eq!(g.events.iter().filter(|e| matches!(e.kind, EventKind::TalkedToUnconscious { .. })).count(), 0);
    // The wiki: an essential knocked out before it recovers from a push reads 0.
    push(&mut g, None, HERO_REF, 2.0);
    ask(&mut g, Some(HERO_REF), KILL_ACTOR, &[]);
    assert_eq!(g.knockout(HERO_REF).map(|k| k.cause), Some(Cause::Essential));
    assert_eq!(knocked_state(&mut g, HERO_REF), 0.0);
    // A knockdown does not replace a knockout: the essential stays essential.
    assert!(!g.knock_down(HERO_REF, None, [0.0; 3]));
    assert_eq!(g.knockout(HERO_REF).map(|k| k.cause), Some(Cause::Essential));
}

#[test]
fn set_unconscious_holds_a_pushed_actor_down_until_it_is_cleared() {
    // The thugs that fake death (SCPT 0x00118A77): SetUnconscious 1, pushed with force 1,
    // and SetUnconscious 0 once hit and GetKnockedState reads 0.
    let f = fixture();
    let mut g = game(&f);
    ask(&mut g, Some(THUG_REF), SET_UNCONSCIOUS, &[num(1.0)]);
    push(&mut g, Some(HERO_REF), THUG_REF, 1.0);
    assert_eq!(ask(&mut g, Some(THUG_REF), GET_UNCONSCIOUS, &[]), 1.0);
    assert_eq!(knocked_state(&mut g, THUG_REF), 0.0);
    wait(&mut g, KNOCKDOWN_DOWN * 4.0);
    assert!(g.knockout(THUG_REF).is_some_and(|k| k.lying()), "held down while unconscious");
    ask(&mut g, Some(THUG_REF), SET_UNCONSCIOUS, &[num(0.0)]);
    wait(&mut g, 0.1);
    assert!(g.knockout(THUG_REF).is_some_and(|k| !k.lying()), "gets up once cleared");
    // SetUnconscious alone (robots in pods, sleepers) lays nobody down.
    ask(&mut g, Some(HERO_REF), SET_UNCONSCIOUS, &[num(1.0)]);
    assert!(g.knockout(HERO_REF).is_none() && g.is_out_cold(HERO_REF));
    assert_eq!(knocked_state(&mut g, HERO_REF), 1.0);
}
