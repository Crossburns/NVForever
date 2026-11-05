//! Karma, ownership and crime on a synthetic plugin (no game data):
//! docs/spec-notes/crime-karma.md §2–§5.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::crime::{Alignment, Kind, Note};
use fnv_game::detection::{Band, Detection};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::world::Outcome;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const TOWN: u32 = 0x900;
const GANG: u32 = 0x901;
const SHOPKEEPER: u32 = 0x830;
const TOWNSMAN: u32 = 0x831;
const FIEND: u32 = 0x832;
const STRANGER: u32 = 0x833;
/// An NPC made from the Fiend's template (traits from the template), with 0 karma of its
/// own.
const CONVICT: u32 = 0x834;
const CONVICT_REF: u32 = 0x324;
/// A Very Evil NPC (−900) and a Very Good one (900), unseen, in no faction.
const BOSS: u32 = 0x835;
const BOSS_REF: u32 = 0x325;
const SAINT: u32 = 0x836;
const SAINT_REF: u32 = 0x326;
const TRINKET: u32 = 0x850;
const SHOP: u32 = 0x200;
const SHOPKEEPER_REF: u32 = 0x320;
const TOWNSMAN_REF: u32 = 0x321;
const FIEND_REF: u32 = 0x322;
const STRANGER_REF: u32 = 0x323;
const TRINKET_REF: u32 = 0x330;
const GANG_TRINKET_REF: u32 = 0x331;
const SHOP_DOOR: u32 = 0x860;
/// Two teleport doors into the shop: the first flagged No Alarm (XTEL), the second not.
const QUIET_DOOR_REF: u32 = 0x340;
const LOUD_DOOR_REF: u32 = 0x341;

const GET_ACTOR_CRIME_PLAYER_ENEMY: u16 = 0x11CB;
const GET_MINOR_CRIME_COUNT: u16 = 0x1074;
const IS_PC_A_MURDERER: u16 = 0x10B0;
const IS_TRESPASSING: u16 = 0x1091;
const GET_TRESPASS_WARNING_LEVEL: u16 = 0x1090;
const IS_CELL_OWNER: u16 = 0x1118;
const REWARD_KARMA: u16 = 0x1204;
const GET_AV: u16 = 0x100E;
const KARMA: u16 = 23;

/// ACBS with a karma value.
fn acbs(karma: f32) -> fnv_esm::raw::Subrecord {
    acbs_templated(karma, 0)
}

/// ACBS with a karma value and template flags (0x01 Use Traits [documented: xEdit]).
fn acbs_templated(karma: f32, template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(karma.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn snam(faction: u32) -> fnv_esm::raw::Subrecord {
    sub(b"SNAM", [faction.to_le_bytes().to_vec(), vec![0, 0, 0, 0]].concat())
}

/// AIDT: aggression, and Assistance 0.
fn aidt(aggression: u8) -> fnv_esm::raw::Subrecord {
    let mut d = vec![aggression, 0, 50, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    d.extend(0i32.to_le_bytes());
    sub(b"AIDT", d)
}

fn placed(sig: &[u8; 4], fid: u32, base: u32, extra: Vec<fnv_esm::raw::Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", base.to_le_bytes().to_vec())];
    subs.extend(extra);
    subs.push(sub(b"DATA", vec![0; 24]));
    record(sig, 0, fid, &subs)
}

/// A quest whose Detection-type topics a witness can say: the NoCrime topics (#974).
const QUEST: u32 = 0x910;
const NO_CRIME_TOPICS: [(u32, &str); 2] = [(0xA00, "StealNoCrime"), (0xA01, "AssaultNoCrime")];

/// A topic of type Combat (the GECK's Combat tab) with one line, no conditions.
fn topic(id: u32, edid: &str) -> Vec<u8> {
    let dial = record(b"DIAL", 0, id, &[sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![fnv_game::data::topic_type::COMBAT, 0])]);
    let trdt = [0u32.to_le_bytes().to_vec(), 50u32.to_le_bytes().to_vec(), vec![0; 4], vec![1, 0, 0, 0], 0u32.to_le_bytes().to_vec(), vec![0; 4]].concat();
    let info = record(
        b"INFO",
        0,
        id + 0x100,
        &[
            sub(b"DATA", vec![fnv_game::data::topic_type::COMBAT, 0, 0, 0]),
            sub(b"QSTI", QUEST.to_le_bytes().to_vec()),
            sub(b"TRDT", trdt),
            sub(b"NAM1", z(edid)),
            sub(b"NAM2", z("")),
            sub(b"NAM3", z("")),
            sub(b"NEXT", vec![]),
        ],
    );
    [dial, group(id.to_le_bytes(), 7, &info)].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let quest = record(b"QUST", 0, QUEST, &[sub(b"EDID", z("CrimeTalk")), sub(b"DATA", [vec![1, 50, 0, 0], 0f32.to_le_bytes().to_vec()].concat())]);
    let dial: Vec<u8> = NO_CRIME_TOPICS.iter().flat_map(|&(id, edid)| topic(id, edid)).collect();
    let factions = [
        // Track Crime (second byte, bit 0).
        record(b"FACT", 0, TOWN, &[sub(b"EDID", z("Town")), sub(b"DATA", vec![0, 1, 0, 0])]),
        // Evil (first byte, bit 1) and Track Crime.
        record(b"FACT", 0, GANG, &[sub(b"EDID", z("Gang")), sub(b"DATA", vec![2, 1, 0, 0])]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), acbs(0.0)]),
        record(b"NPC_", 0, SHOPKEEPER, &[sub(b"EDID", z("Shopkeeper")), acbs(0.0), snam(TOWN), aidt(0)]),
        record(b"NPC_", 0, TOWNSMAN, &[sub(b"EDID", z("Townsman")), acbs(500.0), snam(TOWN), aidt(0)]),
        record(b"NPC_", 0, FIEND, &[sub(b"EDID", z("Fiend")), acbs(-500.0), snam(GANG), aidt(0)]),
        record(b"NPC_", 0, STRANGER, &[sub(b"EDID", z("Stranger")), acbs(0.0), aidt(0)]),
        record(b"NPC_", 0, CONVICT, &[sub(b"EDID", z("Convict")), acbs_templated(0.0, 0x01), sub(b"TPLT", FIEND.to_le_bytes().to_vec()), aidt(0)]),
        record(b"NPC_", 0, BOSS, &[sub(b"EDID", z("Boss")), acbs(-900.0), aidt(0)]),
        record(b"NPC_", 0, SAINT, &[sub(b"EDID", z("Saint")), acbs(900.0), aidt(0)]),
    ]
    .concat();
    let misc = record(b"MISC", 0, TRINKET, &[sub(b"EDID", z("Trinket")), sub(b"DATA", [5i32.to_le_bytes(), 1f32.to_le_bytes()].concat())]);
    let door = record(b"DOOR", 0, SHOP_DOOR, &[sub(b"EDID", z("ShopDoor"))]);
    // XTEL: destination door, position and rotation, No Alarm (u32) [xEdit REFR XTEL].
    let xtel = |to: u32, no_alarm: u32| sub(b"XTEL", [to.to_le_bytes().to_vec(), vec![0; 24], no_alarm.to_le_bytes().to_vec()].concat());
    // An interior cell, not public, owned by the shopkeeper.
    let cell = record(b"CELL", 0, SHOP, &[sub(b"EDID", z("Shop")), sub(b"DATA", vec![0x01]), sub(b"XOWN", SHOPKEEPER.to_le_bytes().to_vec())]);
    let refs = [
        placed(b"ACHR", SHOPKEEPER_REF, SHOPKEEPER, vec![]),
        placed(b"ACHR", TOWNSMAN_REF, TOWNSMAN, vec![]),
        placed(b"ACHR", FIEND_REF, FIEND, vec![]),
        placed(b"ACHR", STRANGER_REF, STRANGER, vec![]),
        placed(b"ACHR", CONVICT_REF, CONVICT, vec![]),
        placed(b"ACHR", BOSS_REF, BOSS, vec![]),
        placed(b"ACHR", SAINT_REF, SAINT, vec![]),
        placed(b"REFR", TRINKET_REF, TRINKET, vec![]),
        placed(b"REFR", GANG_TRINKET_REF, TRINKET, vec![sub(b"XOWN", GANG.to_le_bytes().to_vec()), sub(b"XRNK", 1i32.to_le_bytes().to_vec())]),
        placed(b"REFR", QUIET_DOOR_REF, SHOP_DOOR, vec![xtel(LOUD_DOOR_REF, 1)]),
        placed(b"REFR", LOUD_DOOR_REF, SHOP_DOOR, vec![xtel(QUIET_DOOR_REF, 0)]),
    ]
    .concat();
    let children = group(SHOP.to_le_bytes(), 6, &group(SHOP.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"FACT", &factions),
        top(b"QUST", &quest),
        top(b"DIAL", &dial),
        top(b"NPC_", &npcs),
        top(b"MISC", &misc),
        top(b"DOOR", &door),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Crime.esm".into()).unwrap()];
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
    g.player_entered(Some(FormId(SHOP)));
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(SHOP));
    g
}

fn call(g: &mut Game, op: u16, on: Option<u32>, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: on.map(FormId), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    match g.call_function(&ctx, args) {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

fn karma(g: &mut Game) -> f64 {
    call(g, GET_AV, Some(0x14), &[ArgValue::Enum(KARMA)])
}

/// `w` sees the player now.
fn sees(g: &mut Game, w: u32) {
    let t = g.time;
    g.detection.pairs.insert((FormId(w), FormId::PLAYER_REF), (Detection { value: 10.0, band: Band::Detected, ..Default::default() }, t));
}

fn crimes(g: &mut Game) -> Vec<(Kind, Vec<FormId>)> {
    g.crimes.take_notes().into_iter().filter_map(|n| if let Note::Crime { kind, hostile, .. } = n { Some((kind, hostile)) } else { None }).collect()
}

#[test]
fn the_records_are_read() {
    let f = fixture();
    let c = f.data.cells[&FormId(SHOP)];
    assert_eq!((c.owner, c.public, c.interior), (Some(FormId(SHOPKEEPER)), false, true));
    assert_eq!(f.data.faction_flags[&FormId(TOWN)], 256);
    assert_eq!(f.data.faction_flags[&FormId(GANG)], 256 | 2);
    assert_eq!(f.data.owner_ranks[&FormId(GANG_TRINKET_REF)], 1);
}

#[test]
fn ownership_comes_from_the_reference_then_the_cell() {
    let f = fixture();
    let mut g = game(&f);
    let o = g.ownership(FormId(TRINKET_REF)).unwrap();
    assert_eq!((o.owner, o.from_cell), (FormId(SHOPKEEPER), true));
    let o = g.ownership(FormId(GANG_TRINKET_REF)).unwrap();
    assert_eq!((o.owner, o.rank, o.from_cell), (FormId(GANG), Some(1), false));
    assert!(g.owned_by_other(FormId(TRINKET_REF)).is_some());
    assert!(g.counts_as_owner(FormId(SHOPKEEPER_REF), &g.ownership(FormId(TRINKET_REF)).unwrap()));
    // The player in the gang at rank 1 owns the gang's trinket; at rank 0 not.
    g.extra.entry(FormId::PLAYER_REF).or_default().factions.insert(FormId(GANG), 0);
    assert!(g.owned_by_other(FormId(GANG_TRINKET_REF)).is_some());
    g.extra.entry(FormId::PLAYER_REF).or_default().factions.insert(FormId(GANG), 1);
    assert!(g.owned_by_other(FormId(GANG_TRINKET_REF)).is_none());
    // SetOwnership with no owner: the player's.
    g.world.get(FormId(TRINKET_REF)).unwrap().extra.owner = Some(FormId::PLAYER_BASE);
    assert!(g.owned_by_other(FormId(TRINKET_REF)).is_none());
    assert_eq!(call(&mut g, IS_CELL_OWNER, None, &[ArgValue::Value(Value::Ref(SHOP)), ArgValue::Value(Value::Ref(SHOPKEEPER_REF))]), 1.0);
}

#[test]
fn an_unseen_theft_costs_karma_and_turns_nobody() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.player_stole(Some(FormId(TRINKET_REF)), None, FormId(TRINKET), 1));
    assert_eq!(karma(&mut g), -5.0);
    assert!(crimes(&mut g).iter().all(|(k, h)| *k == Kind::Theft && h.is_empty()));
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    assert_eq!(call(&mut g, GET_MINOR_CRIME_COUNT, Some(SHOPKEEPER), &[]), 1.0);
    // Stealing from an Evil faction costs no karma.
    assert!(g.player_stole(Some(FormId(GANG_TRINKET_REF)), None, FormId(TRINKET), 1));
    assert_eq!(karma(&mut g), -5.0);
}

#[test]
fn the_owner_who_sees_a_theft_turns_on_the_player() {
    let f = fixture();
    let mut g = game(&f);
    // An unaggressive shopkeeper and a townsman of the same faction both see it; only the
    // owner turns (a minor crime).
    sees(&mut g, SHOPKEEPER_REF);
    sees(&mut g, TOWNSMAN_REF);
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    g.player_stole(Some(FormId(TRINKET_REF)), None, FormId(TRINKET), 1);
    assert_eq!(crimes(&mut g), vec![(Kind::Theft, vec![FormId(SHOPKEEPER_REF)])]);
    assert!(g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    assert!(!g.would_attack(FormId(TOWNSMAN_REF), FormId::PLAYER_REF));
    assert_eq!(call(&mut g, GET_ACTOR_CRIME_PLAYER_ENEMY, Some(SHOPKEEPER_REF), &[]), 1.0);
    assert_eq!(call(&mut g, GET_MINOR_CRIME_COUNT, Some(SHOPKEEPER_REF), &[]), 1.0);
    // StopCombat calms it.
    g.stop_combat(FormId(SHOPKEEPER_REF));
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
}

#[test]
fn killing_by_alignment_and_murder() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.alignment(FormId(FIEND_REF)), Alignment::Evil);
    assert_eq!(g.alignment(FormId(TOWNSMAN_REF)), Alignment::Good);
    // An Evil actor (−500 of its own): +fKarmaModKillingEvilActor, no murder (observed:
    // a Fiend and a Powder Ganger at −500 gave +100, capture o6_02, crime-karma.md §4.1).
    assert!(g.kill(FormId(FIEND_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), 100.0);
    assert_eq!(call(&mut g, IS_PC_A_MURDERER, None, &[]), 0.0);
    // A Good townsman, seen by the shopkeeper of the same crime-tracking faction: karma
    // lost, murder, the shopkeeper turns and the town becomes the player's enemy.
    sees(&mut g, SHOPKEEPER_REF);
    g.crimes.take_notes();
    assert!(g.kill(FormId(TOWNSMAN_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), 0.0);
    assert_eq!(call(&mut g, IS_PC_A_MURDERER, None, &[]), 1.0);
    let notes = g.crimes.take_notes();
    assert!(notes.iter().any(|n| matches!(n, Note::FactionEnemy { faction } if *faction == FormId(TOWN))));
    assert!(notes.iter().any(|n| matches!(n, Note::Crime { kind: Kind::Murder, hostile, .. } if hostile == &vec![FormId(SHOPKEEPER_REF)])));
    assert!(g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    // A Neutral stranger murdered unseen: a quarter of the setting.
    assert!(g.kill(FormId(STRANGER_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), -25.0);
    // A convict made from the Fiend's template is as Evil as the Fiend to scripts, but
    // holds 0 itself: killing it gives no karma (the NCRCF convict of 2026-10-19 gave
    // none), and is no murder.
    assert_eq!(g.alignment(FormId(CONVICT_REF)), Alignment::Evil);
    assert_eq!(g.alignment(FormId(CONVICT)), Alignment::Evil);
    assert_eq!(g.kill_alignment(FormId(CONVICT_REF)), Alignment::Neutral);
    assert!(g.kill(FormId(CONVICT_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), -25.0);
    assert_eq!(g.crimes.take_notes().iter().filter(|n| matches!(n, Note::Crime { kind: Kind::Murder, .. })).count(), 1);
    // Very Evil: + the setting; Very Good: − the setting (observed: a Sunny copy, 900).
    assert_eq!(g.alignment(FormId(BOSS_REF)), Alignment::VeryEvil);
    assert!(g.kill(FormId(BOSS_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), 75.0);
    assert_eq!(g.alignment(FormId(SAINT_REF)), Alignment::VeryGood);
    assert!(g.kill(FormId(SAINT_REF), Some(FormId::PLAYER_REF)));
    assert_eq!(karma(&mut g), -25.0);
}

#[test]
fn kill_karma_by_alignment() {
    use fnv_game::crime::{kill_karma, Settings};
    let s = Settings::default();
    for (a, fight, murder) in [
        (Alignment::VeryEvil, 100.0, 100.0),
        (Alignment::Evil, 100.0, 100.0),
        (Alignment::Neutral, 0.0, -25.0),
        (Alignment::Good, -100.0, -100.0),
        (Alignment::VeryGood, -100.0, -100.0),
    ] {
        assert_eq!(kill_karma(a, false, true, &s), fight, "{a:?} in a fight");
        assert_eq!(kill_karma(a, true, true, &s), murder, "{a:?} murdered");
        // Creatures give none.
        assert_eq!(kill_karma(a, true, false, &s), 0.0, "{a:?} creature");
    }
}

#[test]
fn a_seen_trespasser_is_warned_twice_then_attacked() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(call(&mut g, IS_TRESPASSING, Some(0x14), &[]), 1.0);
    sees(&mut g, SHOPKEEPER_REF);
    let keeper = FormId(SHOPKEEPER_REF);
    assert_eq!(g.warning_to(keeper), None);
    g.crime_tick();
    assert_eq!(call(&mut g, GET_TRESPASS_WARNING_LEVEL, Some(SHOPKEEPER_REF), &[ArgValue::Value(Value::Ref(0x14))]), 1.0);
    // Warning, it draws its weapon, stops and faces the player (not alerted).
    assert_eq!(g.warning_to(keeper), Some(FormId::PLAYER_REF));
    assert!(g.draws_weapon(keeper) && !g.is_alerted(keeper));
    // Too soon for the next warning; it still warns while it sees the player.
    g.time += 1.0;
    sees(&mut g, SHOPKEEPER_REF);
    g.crime_tick();
    assert_eq!(call(&mut g, GET_TRESPASS_WARNING_LEVEL, Some(SHOPKEEPER_REF), &[ArgValue::Value(Value::Ref(0x14))]), 1.0);
    assert_eq!(g.warning_to(keeper), Some(FormId::PLAYER_REF));
    g.time += 5.0;
    sees(&mut g, SHOPKEEPER_REF);
    g.crime_tick();
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    g.time += 5.0;
    sees(&mut g, SHOPKEEPER_REF);
    assert_eq!(g.crime_tick(), 1);
    assert!(g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF));
    // It fights now: no longer a warning.
    assert_eq!(g.warning_to(keeper), None);
    // A public cell is no trespass.
    g.forms.public_cells.insert(FormId(SHOP), true);
    assert_eq!(call(&mut g, IS_TRESPASSING, Some(0x14), &[]), 0.0);
}

#[test]
fn a_no_alarm_door_excuses_trespassing_in_its_cell() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(call(&mut g, IS_TRESPASSING, Some(0x14), &[]), 1.0);
    // In through the door flagged No Alarm: not trespassing, so no warning.
    g.arrived_through(FormId(QUIET_DOOR_REF), Some(FormId(SHOP)));
    assert!(g.crimes.take_notes().contains(&Note::NoAlarm { door: FormId(QUIET_DOOR_REF), cell: FormId(SHOP) }));
    assert_eq!(call(&mut g, IS_TRESPASSING, Some(0x14), &[]), 0.0);
    sees(&mut g, SHOPKEEPER_REF);
    g.crime_tick();
    assert_eq!(call(&mut g, GET_TRESPASS_WARNING_LEVEL, Some(SHOPKEEPER_REF), &[ArgValue::Value(Value::Ref(0x14))]), 0.0);
    // In through the other door: trespassing again.
    g.arrived_through(FormId(LOUD_DOOR_REF), Some(FormId(SHOP)));
    assert!(g.crimes.take_notes().is_empty());
    assert_eq!(call(&mut g, IS_TRESPASSING, Some(0x14), &[]), 1.0);
}

/// #1615: the door the player uses by hand is the door arrived through, and the cell is
/// left to the engine's loading, unlike a script's `Activate player`.
#[test]
fn the_players_own_door_counts_and_leaves_the_cell_to_the_engine() {
    let f = fixture();
    let mut g = game(&f);
    let shop = FormId(SHOP);
    g.set_loaded(Default::default());
    let done = g.default_activation_by_hand(FormId(QUIET_DOOR_REF)).unwrap().expect("not refused");
    assert!(matches!(done.activation.outcome, Outcome::Teleported { cell: Some(c), .. } if c == shop));
    assert!(g.crimes.take_notes().contains(&Note::NoAlarm { door: FormId(QUIET_DOOR_REF), cell: shop }));
    assert_eq!(call(&mut g, IS_TRESPASSING, Some(0x14), &[]), 0.0);
    assert!(g.loaded_cells.is_empty(), "the engine's loading says when the player is there");
    g.player_entered(Some(shop));
    assert!(g.loaded_cells.contains(&shop));
    // A script's activation has no loading of its own to wait for.
    g.set_loaded(Default::default());
    g.default_activation(FormId::PLAYER_REF, FormId(LOUD_DOOR_REF), false).unwrap();
    assert!(g.loaded_cells.contains(&shop));
}

#[test]
fn reward_karma_leaves_a_note() {
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, REWARD_KARMA, None, &[ArgValue::Value(Value::Num(-75.0))]);
    assert_eq!(karma(&mut g), -75.0);
    assert!(matches!(g.crimes.take_notes().as_slice(), [Note::Karma { change, now, .. }] if *change == -75.0 && *now == -75.0));
}

/// What `actor` said as a Detection-style line since the last look.
fn said(g: &mut Game, actor: u32) -> Vec<&'static str> {
    let barks = std::mem::take(&mut g.awareness.barks);
    barks.into_iter().filter(|b| b.1 == FormId(actor) && b.2.is_some()).map(|b| b.0).collect()
}

/// A witness that ignores crime says the act's NoCrime topic and does not turn on the
/// player [documented: GECK wiki Combat Tab, StealNoCrime, AssaultNoCrime; #974].
#[test]
fn a_witness_that_ignores_crime_says_so() {
    let f = fixture();
    let mut g = game(&f);
    // The lines' quest (start-game enabled) runs from a new game.
    g.new_game();
    g.player_entered(Some(FormId(SHOP)));
    g.extra.entry(FormId(SHOPKEEPER_REF)).or_default().ignore_crime = true;
    sees(&mut g, SHOPKEEPER_REF);
    g.player_stole(Some(FormId(TRINKET_REF)), None, FormId(TRINKET), 1);
    assert_eq!(said(&mut g, SHOPKEEPER_REF), vec!["StealNoCrime"]);
    assert!(!g.would_attack(FormId(SHOPKEEPER_REF), FormId::PLAYER_REF), "it does not turn");
    // An assaulted victim that ignores crime says AssaultNoCrime.
    g.extra.entry(FormId(TOWNSMAN_REF)).or_default().ignore_crime = true;
    g.player_assaulted(FormId(TOWNSMAN_REF));
    assert_eq!(said(&mut g, TOWNSMAN_REF), vec!["AssaultNoCrime"]);
    // One that does not ignore crime says no NoCrime line.
    g.player_assaulted(FormId(STRANGER_REF));
    assert!(said(&mut g, STRANGER_REF).is_empty());
}
