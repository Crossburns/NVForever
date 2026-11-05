//! Encounter zones on a synthetic plugin (no game data): a zone's level set on first
//! entry, leveled actors read at their reference's level, level modifiers, Never Resets.
//! docs/spec-notes/encounter-zones.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::zones::LevelNews;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::zones::LevelAt;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const ZONE: u32 = 0x100;
const SAFE_ZONE: u32 = 0x101;
const RAT: u32 = 0x840;
const WOLF: u32 = 0x841;
const TIERS: u32 = 0x850;
const PELT: u32 = 0x880;
const TAIL: u32 = 0x881;
const ZONE_CELL: u32 = 0x200;
const PLAIN_CELL: u32 = 0x201;
const SAFE_CELL: u32 = 0x202;
/// A tiered list in the zone (minimum 5), one with the modifier Easy, one Boss.
const IN_ZONE: u32 = 0x320;
const EASY: u32 = 0x321;
const BOSS: u32 = 0x322;
/// The list outside every zone.
const OUTSIDE: u32 = 0x323;
/// The list in a zone of its own (reference XEZN) in the plain cell.
const OWN_ZONE: u32 = 0x324;
/// A leveled item list: level 1 a tail, level 5 a pelt, only the highest eligible level.
const LOOT: u32 = 0x860;

const GET_IN_ZONE: u16 = 0x11BE;
const SET_LEVEL: u16 = 0x1133;

fn acbs() -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(0f32.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(0u16.to_le_bytes());
    sub(b"ACBS", d)
}

fn eczn(fid: u32, edid: &str, min: i8, flags: u8) -> Vec<u8> {
    record(b"ECZN", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", [0u32.to_le_bytes().to_vec(), vec![0, min as u8, flags, 0]].concat())])
}

fn placed(fid: u32, base: u32, extra: &[fnv_esm::raw::Subrecord]) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(extra.iter().cloned());
    subs.push(sub(b"DATA", vec![0; 24]));
    record(b"ACRE", 0, fid, &subs)
}

fn cell(fid: u32, edid: &str, zone: Option<u32>, refs: &[u8]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"DATA", vec![1])];
    subs.extend(zone.map(|z| sub(b"XEZN", z.to_le_bytes().to_vec())));
    [record(b"CELL", 0, fid, &subs), group(fid.to_le_bytes(), 6, &group(fid.to_le_bytes(), 9, refs))].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let item = |fid: u32, edid: &str| record(b"MISC", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", [1i32.to_le_bytes(), 1f32.to_le_bytes()].concat())]);
    let carries = |item: u32| sub(b"CNTO", [item.to_le_bytes(), 1i32.to_le_bytes()].concat());
    let creatures = [
        record(b"CREA", 0, RAT, &[sub(b"EDID", z("Rat")), acbs(), carries(TAIL)]),
        record(b"CREA", 0, WOLF, &[sub(b"EDID", z("Wolf")), acbs(), carries(PELT)]),
    ]
    .concat();
    let mut tiers = vec![sub(b"EDID", z("Tiers")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0])];
    for (level, form) in [(1i16, RAT), (5, WOLF)] {
        tiers.push(sub(b"LVLO", [level.to_le_bytes().to_vec(), vec![0; 2], form.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat()));
    }
    let mut loot = vec![sub(b"EDID", z("Loot")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0])];
    for (level, form) in [(1i16, TAIL), (5, PELT)] {
        loot.push(sub(b"LVLO", [level.to_le_bytes().to_vec(), vec![0; 2], form.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat()));
    }
    let xlcm = |m: i32| sub(b"XLCM", m.to_le_bytes().to_vec());
    let xezn = |z: u32| sub(b"XEZN", z.to_le_bytes().to_vec());
    let cells = [
        cell(ZONE_CELL, "ZoneCell", Some(ZONE), &[placed(IN_ZONE, TIERS, &[]), placed(EASY, TIERS, &[xlcm(0)]), placed(BOSS, TIERS, &[xlcm(3)])].concat()),
        cell(PLAIN_CELL, "PlainCell", None, &[placed(OUTSIDE, TIERS, &[]), placed(OWN_ZONE, TIERS, &[xezn(ZONE)])].concat()),
        cell(SAFE_CELL, "SafeCell", Some(SAFE_ZONE), &[]),
    ]
    .concat();
    [
        tes4(),
        top(b"MISC", &[item(PELT, "Pelt"), item(TAIL, "Tail")].concat()),
        top(b"CREA", &creatures),
        top(b"LVLC", &record(b"LVLC", 0, TIERS, &tiers)),
        top(b"LVLI", &record(b"LVLI", 0, LOOT, &loot)),
        // Minimum 5; the safe zone has "Never Resets" and "Match PC Below Minimum Level".
        top(b"ECZN", &[eczn(ZONE, "TestZone", 5, 0), eczn(SAFE_ZONE, "SafeZone", 8, 3)].concat()),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Zones.esm".into()).unwrap()];
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
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn call(g: &mut Game, op: u16, on: u32, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(FormId(on)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    match g.call_function(&ctx, args) {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

#[test]
fn the_records_are_read() {
    let f = fixture();
    let z = f.forms.zones[&FormId(ZONE)];
    assert_eq!((z.min_level, z.never_resets, z.match_pc_below_min), (5, false, false));
    let s = f.forms.zones[&FormId(SAFE_ZONE)];
    assert_eq!((s.min_level, s.never_resets, s.match_pc_below_min), (8, true, true));
    assert_eq!(f.refs.cell_zone(FormId(ZONE_CELL)), Some(FormId(ZONE)));
    assert_eq!(f.refs.zone_of(FormId(IN_ZONE)), Some(FormId(ZONE)));
    assert_eq!(f.refs.zone_of(FormId(OUTSIDE)), None);
    // The reference's own zone over its cell's (none here).
    assert_eq!(f.refs.zone_of(FormId(OWN_ZONE)), Some(FormId(ZONE)));
    assert_eq!(f.refs.get(FormId(EASY)).unwrap().level_modifier, Some(0));
    assert_eq!(f.refs.get(FormId(IN_ZONE)).unwrap().level_modifier, None);
}

#[test]
fn a_zone_takes_its_minimum_level_on_entry_and_keeps_it() {
    let f = fixture();
    let mut g = game(&f);
    // A level-1 player: before entering, the level the zone would take.
    assert_eq!(g.leveled_at(FormId(IN_ZONE)), LevelAt::plain(5));
    g.player_entered(Some(FormId(ZONE_CELL)));
    assert_eq!(g.world.zone_level(FormId(ZONE)), Some(5));
    let news = g.take_level_news();
    assert!(news.contains(&LevelNews::Zone { zone: FormId(ZONE), level: 5, min_level: 5, player_level: 1 }), "{news:?}");
    assert!(news.iter().any(|n| matches!(n, LevelNews::Actor { actor, base: Some(b), at_display: Some(d), .. } if *actor == FormId(IN_ZONE) && *b == FormId(WOLF) && *d == FormId(RAT))), "{news:?}");
    // The tier for level 5: the wolf, its name and carried items.
    assert_eq!(g.display_base(FormId(TIERS), FormId(IN_ZONE)), FormId(WOLF));
    assert_eq!(g.world.item_count(FormId(IN_ZONE), FormId(PELT)).unwrap(), 1);
    // Easy: half the level (2), every entry at or under it: the rat.
    assert_eq!(g.leveled_at(FormId(EASY)), LevelAt { level: 2, all_levels: true });
    assert_eq!(g.display_base(FormId(TIERS), FormId(EASY)), FormId(RAT));
    // Boss: twice the level.
    assert_eq!(g.leveled_at(FormId(BOSS)).level, 10);
    // A later level-up changes neither the zone nor the references fixed in it.
    g.player.level = Some(20);
    g.player_entered(Some(FormId(PLAIN_CELL)));
    g.player_entered(Some(FormId(ZONE_CELL)));
    assert_eq!(g.world.zone_level(FormId(ZONE)), Some(5));
    assert_eq!(g.leveled_at(FormId(IN_ZONE)), LevelAt::plain(5));
    let zone = [ArgValue::Value(Value::Ref(ZONE))];
    assert_eq!((call(&mut g, GET_IN_ZONE, IN_ZONE, &zone), call(&mut g, GET_IN_ZONE, OUTSIDE, &zone)), (1.0, 0.0));
}

/// The tail or the pelt: which entry of the list the world rolls at now.
fn roll(g: &mut Game) -> Vec<(FormId, i32)> {
    g.world.add_item(FormId(OUTSIDE), FormId(LOOT), 1).unwrap()
}

#[test]
fn the_world_rolls_at_the_players_level_as_soon_as_it_changes() {
    let f = fixture();
    let mut g = game(&f);
    g.player_entered(Some(FormId(PLAIN_CELL)));
    assert_eq!(roll(&mut g), vec![(FormId(TAIL), 1)]);
    // SetLevel, with no tick before the roll: a console line or a script's next statement.
    call(&mut g, SET_LEVEL, FormId::PLAYER_REF.0, &[ArgValue::Value(Value::Num(20.0)), ArgValue::Value(Value::Num(0.0))]);
    assert_eq!(g.world.config().player_level, 20);
    assert_eq!(roll(&mut g), vec![(FormId(PELT), 1)]);
    // A level-up the same way.
    let mut g = game(&f);
    g.player_entered(Some(FormId(PLAIN_CELL)));
    g.player.level = Some(4);
    assert_eq!(fnv_game::levelup::advance(&mut g), 5);
    assert_eq!(roll(&mut g), vec![(FormId(PELT), 1)]);
}

#[test]
fn a_tick_brings_the_world_to_the_players_level() {
    let f = fixture();
    let mut g = game(&f);
    g.player_entered(Some(FormId(PLAIN_CELL)));
    // A save from before the level followed: the player at 20, the world's level 1.
    g.player.level = Some(20);
    let s = g.snapshot();
    assert_eq!(s.sim.config.player_level, 1);
    let world = World::restore(&f.forms, &f.refs, &s.sim).unwrap();
    let mut g2 = Game::restore(&f.data, world, GameConfig { strict: true, ..GameConfig::default() }, &s).unwrap();
    g2.tick();
    assert_eq!(g2.world.config().player_level, 20);
    assert_eq!(roll(&mut g2), vec![(FormId(PELT), 1)]);
    assert_eq!(g2.snapshot().sim.config.player_level, 20);
}

#[test]
fn a_game_with_no_level_set_keeps_the_configured_one() {
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig { player_level: 5, ..SimConfig::default() });
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.tick();
    assert_eq!(g.world.config().player_level, 5);
}

#[test]
fn outside_a_zone_the_players_level_counts() {
    let f = fixture();
    let mut g = game(&f);
    g.player_entered(Some(FormId(PLAIN_CELL)));
    assert_eq!(g.leveled_at(FormId(OUTSIDE)), LevelAt::plain(1));
    assert_eq!(g.display_base(FormId(TIERS), FormId(OUTSIDE)), FormId(RAT));
    assert_eq!(g.world.item_count(FormId(OUTSIDE), FormId(TAIL)).unwrap(), 1);
    // A reference with a zone of its own enters it.
    assert_eq!(g.world.zone_level(FormId(ZONE)), Some(5));
    assert_eq!(g.display_base(FormId(TIERS), FormId(OWN_ZONE)), FormId(WOLF));
}

#[test]
fn match_pc_below_minimum_takes_the_players_level_and_never_resets_is_read() {
    let f = fixture();
    let mut g = game(&f);
    g.player.level = Some(3);
    g.player_entered(Some(FormId(SAFE_CELL)));
    assert_eq!(g.world.zone_level(FormId(SAFE_ZONE)), Some(3));
    assert!(g.world.cell_never_resets(FormId(SAFE_CELL)));
    assert!(!g.world.cell_never_resets(FormId(ZONE_CELL)));
    // Fallout 3's SetZoneRespawns turns it either way, and a snapshot keeps it.
    let set = |g: &mut Game, zone: u32, flag: f64| call(g, 0x11EA, 0x14, &[ArgValue::Value(Value::Ref(zone)), ArgValue::Value(Value::Num(flag))]);
    set(&mut g, SAFE_ZONE, 1.0);
    set(&mut g, ZONE, 0.0);
    assert!(!g.world.cell_never_resets(FormId(SAFE_CELL)));
    assert!(g.world.cell_never_resets(FormId(ZONE_CELL)));
    let s = g.snapshot();
    let world = World::restore(&f.forms, &f.refs, &s.sim).unwrap();
    assert!(world.cell_never_resets(FormId(ZONE_CELL)) && !world.cell_never_resets(FormId(SAFE_CELL)));
}

#[test]
fn zone_levels_survive_a_snapshot() {
    let f = fixture();
    let mut g = game(&f);
    g.player_entered(Some(FormId(ZONE_CELL)));
    let s = g.snapshot();
    let world = World::restore(&f.forms, &f.refs, &s.sim).unwrap();
    let g2 = Game::restore(&f.data, world, GameConfig { strict: true, ..GameConfig::default() }, &s).unwrap();
    assert_eq!(g2.world.zone_level(FormId(ZONE)), Some(5));
    assert_eq!(g2.world.fixed_level(FormId(EASY)), Some(LevelAt { level: 2, all_levels: true }));
}
