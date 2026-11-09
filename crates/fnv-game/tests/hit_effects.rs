//! Hit effects on a synthetic plugin (no game data): a weapon's or a creature's magic
//! item applied by a hit drains health by the second, the target's resistance cuts it, a
//! second hit starts the time again, the drain kills, and a death ends the effects
//! without "No Death Dispel". docs/spec-notes/hit-effects.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::HitInfo;
use fnv_game::functions::{actor_value, set_actor_value_current, Ctx};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const POISON_FX: u32 = 0x850;
const BURN_FX: u32 = 0x851;
const POISON: u32 = 0x860;
const BURNING: u32 = 0x861;
const DEADLY: u32 = 0x862;

const P: FormId = FormId::PLAYER_REF;
const HEALTH: u16 = 16;
const FIRE_RESISTANCE: u16 = 52;
const POISON_RESISTANCE: u16 = 19;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// MGEF DATA (72 bytes): flags, resistance, archetype 0 (Value Modifier) and actor value.
fn mgef(fid: u32, edid: &str, flags: u32, resist: i32, av: i32) -> Vec<u8> {
    let data = [
        u32s(&[flags]),
        f32b(0.0),
        u32s(&[0]),
        (-1i32).to_le_bytes().to_vec(),
        resist.to_le_bytes().to_vec(),
        vec![0; 4],
        vec![0; 4],
        f32b(1.0),
        vec![0; 24],
        vec![0; 8],
        u32s(&[0]),
        av.to_le_bytes().to_vec(),
    ]
    .concat();
    assert_eq!(data.len(), 72);
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

/// EFID + EFIT (magnitude, area, duration, type Touch, actor value).
fn effect(base: u32, magnitude: u32, duration: u32, av: i32) -> Vec<fnv_esm::raw::Subrecord> {
    vec![sub(b"EFID", base.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[magnitude, 0, duration, 1]), av.to_le_bytes().to_vec()].concat())]
}

fn spel(fid: u32, edid: &str, kind: u32, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"SPIT", [u32s(&[kind, 0, 0]), vec![0; 4]].concat())];
    subs.extend(effects.into_iter().flatten());
    record(b"SPEL", 0, fid, &subs)
}

fn ench(fid: u32, edid: &str, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"ENIT", [u32s(&[2, 0, 0]), vec![0; 4]].concat())];
    subs.extend(effects.into_iter().flatten());
    record(b"ENCH", 0, fid, &subs)
}

fn plugin_bytes() -> Vec<u8> {
    // Flags: 1 Hostile, 4 Detrimental, bit 28 No Death Dispel.
    let mgefs = [mgef(POISON_FX, "Poison", 1 | 4, POISON_RESISTANCE as i32, 16), mgef(BURN_FX, "FireDamage", 1 | 4 | (1 << 28), FIRE_RESISTANCE as i32, 16)].concat();
    let spels = [spel(POISON, "CrPoison", 5, vec![effect(POISON_FX, 4, 5, 16)]), spel(DEADLY, "Deadly", 5, vec![effect(POISON_FX, 60, 5, 16)])].concat();
    let enchs = ench(BURNING, "Burning", vec![effect(BURN_FX, 2, 5, 16)]);
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    [
        tes4(),
        top(b"NPC_", &player),
        top(b"MGEF", &mgefs),
        top(b"SPEL", &spels),
        top(b"ENCH", &enchs),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Hits.esm".into()).unwrap()];
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

/// The player with Health 100 and no resistances.
fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    for (code, v) in [(HEALTH, 100.0), (FIRE_RESISTANCE, 0.0), (POISON_RESISTANCE, 0.0)] {
        set_actor_value_current(&mut g, P, code, v).unwrap();
    }
    g
}

fn health(g: &mut Game) -> f64 {
    actor_value(g, P, HEALTH).unwrap()
}

#[test]
fn records_are_read() {
    let f = fixture();
    let m = &f.data.magic_effects[&FormId(POISON_FX)];
    assert_eq!(m.resistance, Some(POISON_RESISTANCE));
    assert!(!m.has(fnv_game::data::MagicEffectDef::NO_DEATH_DISPEL));
    assert!(f.data.magic_effects[&FormId(BURN_FX)].has(fnv_game::data::MagicEffectDef::NO_DEATH_DISPEL));
}

#[test]
fn a_hit_effect_drains_health_by_the_second() {
    let f = fixture();
    let mut g = game(&f);
    let applied = g.apply_hit_effect(P, FormId(BURNING), None);
    assert_eq!(applied.len(), 1);
    g.frames(60);
    assert!((health(&mut g) - 98.0).abs() < 1e-3, "{}", health(&mut g));
    g.frames(300);
    assert!((health(&mut g) - 90.0).abs() < 1e-3, "{}", health(&mut g));
    assert!(g.effects.is_empty());
}

#[test]
fn resistance_cuts_the_magnitude() {
    let f = fixture();
    let mut g = game(&f);
    set_actor_value_current(&mut g, P, POISON_RESISTANCE, 50.0).unwrap();
    g.apply_hit_effect(P, FormId(POISON), None);
    g.frames(360);
    // 4 a second for 5 seconds, half resisted.
    assert!((health(&mut g) - 90.0).abs() < 1e-3, "{}", health(&mut g));
    // Fully resisted: nothing starts.
    set_actor_value_current(&mut g, P, POISON_RESISTANCE, 100.0).unwrap();
    assert!(g.apply_hit_effect(P, FormId(POISON), None).is_empty());
    assert!(g.effects.is_empty());
}

#[test]
fn a_second_hit_starts_the_time_again() {
    let f = fixture();
    let mut g = game(&f);
    g.apply_hit_effect(P, FormId(BURNING), None);
    g.frames(180);
    let again = g.apply_hit_effect(P, FormId(BURNING), None);
    assert!(again.len() == 1 && again[0].extended);
    assert_eq!(g.effects.len(), 1);
    assert!((g.effects[0].duration - 8.0).abs() < 1e-3, "{}", g.effects[0].duration);
    g.frames(600);
    // 2 a second for 3 + 5 seconds.
    assert!((health(&mut g) - 84.0).abs() < 1e-2, "{}", health(&mut g));
}

#[test]
fn the_drain_kills_and_death_ends_effects_without_no_death_dispel() {
    let f = fixture();
    let mut g = game(&f);
    g.apply_hit_effect(P, FormId(BURNING), None);
    g.apply_hit_effect(P, FormId(DEADLY), None);
    assert_eq!(g.effects.len(), 2);
    g.frames(120);
    assert!(g.is_dead(P));
    // The poison ended with the death; the burn goes on.
    assert_eq!(g.effects.len(), 1);
    assert_eq!(g.effects[0].source, FormId(BURNING));
    // A dead target takes no new effect.
    assert!(g.apply_hit_effect(P, FormId(POISON), None).is_empty());
}

fn ask(g: &mut Game, opcode: u16) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode, on: Some(P), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[]).num()
}

/// A wound, then a drain kills: the wound is not the killing blow (#1588).
#[test]
fn a_drains_death_has_no_killing_hit() {
    const GET_HIT_LOCATION: u16 = 0x1187;
    const GET_CAUSE_OF_DEATH: u16 = 0x118D;
    const GET_KILLING_BLOW_LIMB: u16 = 0x11F0;
    let f = fixture();
    let mut g = game(&f);
    assert!(!g.hit(P, 10.0, HitInfo { attacker: None, weapon: None, location: 3, critical: false, limb: None, penetrated: None }).died);
    g.apply_hit_effect(P, FormId(DEADLY), None);
    g.frames(180);
    assert!(g.is_dead(P));
    assert_eq!(ask(&mut g, GET_KILLING_BLOW_LIMB), -1.0);
    assert_eq!(ask(&mut g, GET_CAUSE_OF_DEATH), -1.0);
    assert_eq!(ask(&mut g, GET_HIT_LOCATION), 3.0);
}

/// The player's death by their own effect, or a companion's, earns no kill credit (#1614).
#[test]
fn a_players_death_by_a_drain_earns_no_credit() {
    use fnv_game::misc_stats::TOTAL_THINGS_KILLED;
    let mate = FormId(0x7777);
    for caster in [P, mate] {
        let f = fixture();
        let mut g = game(&f);
        g.extra.entry(mate).or_default().teammate = true;
        g.apply_hit_effect(P, FormId(DEADLY), Some(caster));
        g.frames(180);
        assert!(g.is_dead(P));
        assert_eq!(g.extra[&P].killer, Some(caster));
        assert_eq!(g.player.xp, 0.0);
        assert_eq!(g.misc_stat(TOTAL_THINGS_KILLED), 0.0);
    }
}
