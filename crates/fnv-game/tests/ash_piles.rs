//! Ash and goo piles on a synthetic plugin (no game data): a weapon's critical effect
//! goes on with On Death only when the hit kills, AttachAshPile places the pile at the
//! actor linked to it, counts Disintegrations for the player's kills, places one pile
//! only, and ResurrectActor takes it away. docs/spec-notes/ash-piles.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::ash_piles::CritEffect;
use fnv_game::functions::Ctx;
use fnv_game::game::{ActMode, Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::world::Outcome;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const ASH: u32 = 0x1B;
const GOO: u32 = 0x22;
const RAIDER: u32 = 0x400;
const RAIDER_REF: FormId = FormId(0x401);
const DISINTEGRATE_FX: u32 = 0x850;
const DISINTEGRATE: u32 = 0x860;
const ATTACH_ASH_PILE: u16 = 0x1211;
const P: FormId = FormId::PLAYER_REF;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// MGEF DATA (72 bytes): Value Modifier of Perception, flags Hostile | Detrimental | No
/// Death Dispel (as the shipped Disintegration's).
fn mgef(fid: u32) -> Vec<u8> {
    let flags = 1 | 4 | (1 << 28);
    let data = [u32s(&[flags]), f32b(0.0), u32s(&[0]), (-1i32).to_le_bytes().to_vec(), (-1i32).to_le_bytes().to_vec(), vec![0; 4], vec![0; 4], f32b(1.0), vec![0; 24], vec![0; 8], u32s(&[0]), 6i32.to_le_bytes().to_vec()].concat();
    record(b"MGEF", 0, fid, &[sub(b"EDID", z("Disintegration")), sub(b"DATA", data)])
}

fn spel(fid: u32) -> Vec<u8> {
    let subs = vec![
        sub(b"EDID", z("DisintegrationFXSpell")),
        sub(b"SPIT", [u32s(&[0, 0, 0]), vec![0; 4]].concat()),
        sub(b"EFID", DISINTEGRATE_FX.to_le_bytes().to_vec()),
        sub(b"EFIT", [u32s(&[1, 0, 4, 0]), 6i32.to_le_bytes().to_vec()].concat()),
    ];
    record(b"SPEL", 0, fid, &subs)
}

fn acti(fid: u32, edid: &str, name: &str, model: &str) -> Vec<u8> {
    record(b"ACTI", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FULL", z(name)), sub(b"MODL", z(model))])
}

fn npc(fid: u32, edid: &str) -> Vec<u8> {
    record(b"NPC_", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())])
}

fn plugin_bytes() -> Vec<u8> {
    let actis = [acti(ASH, "DefaultAshPile1", "Ash Pile", "Effects\\AshPile01.NIF"), acti(GOO, "DefaultAshPile2", "Goo Pile", "Effects\\GooPile01.NIF")].concat();
    let npcs = [npc(0x7, "Player"), npc(RAIDER, "Raider")].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let data = [f32b(100.0), f32b(200.0), f32b(10.0), f32b(0.0), f32b(0.0), f32b(1.5)].concat();
    let refs = record(b"ACHR", 0, RAIDER_REF.0, &[sub(b"NAME", RAIDER.to_le_bytes().to_vec()), sub(b"DATA", data)]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"ACTI", &actis),
        top(b"NPC_", &npcs),
        top(b"MGEF", &mgef(DISINTEGRATE_FX)),
        top(b"SPEL", &spel(DISINTEGRATE)),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Piles.esm".into()).unwrap()];
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

fn call(g: &mut Game, on: FormId, op: u16, args: &[ArgValue]) {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    let before = g.events.len();
    g.call_function(&ctx, args);
    for e in &g.events[before..] {
        if let EventKind::CallFailed { name, error, .. } | EventKind::Unhandled { name, unit: error, .. } = &e.kind {
            panic!("0x{op:04X} {name}: {error}");
        }
    }
}

fn num(v: f64) -> ArgValue {
    ArgValue::Value(Value::Num(v))
}

#[test]
fn on_death_crit_effects_need_a_kill() {
    let f = fixture();
    let mut g = game(&f);
    let spell = FormId(DISINTEGRATE);
    assert_eq!(g.apply_crit_effect(RAIDER_REF, spell, true, false, Some(P)), CritEffect::NotKilled);
    assert_eq!(g.apply_crit_effect(P, spell, false, false, None), CritEffect::NotApplied);
    assert!(g.kill(RAIDER_REF, Some(P)));
    let CritEffect::Applied(ids) = g.apply_crit_effect(RAIDER_REF, spell, true, true, Some(P)) else { panic!("not applied") };
    assert_eq!(ids.len(), 1);
    // Applied after the death: No Death Dispel keeps it on the corpse.
    assert!(g.effects.iter().any(|e| e.target == RAIDER_REF && e.source == spell));
    // An effect for the living is not given to a corpse.
    assert_eq!(g.apply_crit_effect(RAIDER_REF, spell, false, false, Some(P)), CritEffect::NotApplied);
}

#[test]
fn attach_ash_pile_places_one_linked_pile() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.kill(RAIDER_REF, Some(P)));
    call(&mut g, RAIDER_REF, ATTACH_ASH_PILE, &[num(2.0)]);
    let pile = g.extra[&RAIDER_REF].ash_pile.expect("a pile");
    let s = g.world.peek(pile).unwrap();
    assert_eq!(s.base, FormId(GOO));
    assert_eq!(s.pos, [100.0, 200.0, 10.0]);
    assert_eq!(s.rot, [0.0, 0.0, 1.5]);
    assert_eq!(g.ash_pile_source(pile), Some(RAIDER_REF));
    assert_eq!(g.misc_stat(fnv_game::misc_stats::DISINTEGRATIONS), 1.0);
    // A second call leaves the one pile.
    call(&mut g, RAIDER_REF, ATTACH_ASH_PILE, &[]);
    assert_eq!(g.extra[&RAIDER_REF].ash_pile, Some(pile));
    assert_eq!(g.misc_stat(fnv_game::misc_stats::DISINTEGRATIONS), 1.0);
    // Brought back to life, the actor's pile goes.
    g.resurrect(RAIDER_REF, false);
    assert!(g.world.peek(pile).unwrap().deleted());
    assert_eq!(g.extra[&RAIDER_REF].ash_pile, None);
}

/// #1615: every activation of a pile opens the inventory of the body it came from, the
/// player's own (once, by the shared default action) and a script's; a pile taken away by
/// ResurrectActor opens nothing.
#[test]
fn activating_a_pile_opens_the_body_it_came_from() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.kill(RAIDER_REF, Some(P)));
    let pile = g.attach_ash_pile(RAIDER_REF, 1).unwrap();
    let from = g.events.len();
    let done = g.default_activation_by_hand(pile).unwrap().expect("not refused");
    assert_eq!(done.activation.outcome, Outcome::OpenedContainer { container: RAIDER_REF });
    assert_eq!(done.activated_event, from);
    let activated = |g: &Game| g.events.iter().filter(|e| matches!(e.kind, EventKind::Activated { reference, .. } if reference == pile)).count();
    assert_eq!(activated(&g), 1);
    assert_eq!(g.activating(P), Some(pile));
    let v = g.activate(P, pile, ActMode::Default).unwrap();
    assert_eq!(v.pointer("/result").and_then(Json::as_str), Some("opened_container"));
    assert_eq!(activated(&g), 2);
    g.resurrect(RAIDER_REF, false);
    let v = g.activate(P, pile, ActMode::Hand).unwrap();
    assert_ne!(v.pointer("/result").and_then(Json::as_str), Some("opened_container"));
}

#[test]
fn the_default_pile_is_ash() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.ash_pile_base(1), FormId(ASH));
    assert_eq!(g.ash_pile_base(7), FormId(ASH));
    assert_eq!(g.ash_pile_base(2), FormId(GOO));
    // Killed by someone else: no Disintegration for the player.
    assert!(g.kill(RAIDER_REF, None));
    call(&mut g, RAIDER_REF, ATTACH_ASH_PILE, &[]);
    let pile = g.extra[&RAIDER_REF].ash_pile.unwrap();
    assert_eq!(g.world.peek(pile).unwrap().base, FormId(ASH));
    assert_eq!(g.misc_stat(fnv_game::misc_stats::DISINTEGRATIONS), 0.0);
}
