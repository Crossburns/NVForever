//! Experience on a synthetic plugin (no game data): every source goes through
//! `fnv_game::experience::award`, the Adjust Experience Points entry point of the
//! player's perks changes it (Swift Learner by rank, Skilled), hacked terminals and
//! disarmed mines give their XP. docs/spec-notes/experience.md §2.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::data::TerminalDef;
use fnv_game::experience::{self, Source};
use fnv_game::game::{ActMode, Game, GameConfig};
use fnv_game::terminal;
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const SWIFT_LEARNER: u32 = 0x890;
const SKILLED: u32 = 0x891;
const EDUCATED: u32 = 0x892;
const MINE: u32 = 0x8A0;
const MINE_PROJ: u32 = 0x8A1;
const TERMINAL: u32 = 0x8B0;
const MINE_REF: u32 = 0x300;
const TERMINAL_REF: u32 = 0x301;
const ADJUST_EXPERIENCE_POINTS: u8 = 9;
const MULTIPLY: u8 = 3;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// A perk whose rank n (from 0) multiplies the experience by `factors[n]`, as Swift
/// Learner's entries do in FalloutNV.esm.
fn perk(fid: u32, edid: &str, factors: &[f32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"DATA", vec![0, 1, factors.len() as u8, 1, 0])];
    for (rank, f) in factors.iter().enumerate() {
        subs.extend([
            sub(b"PRKE", vec![2, rank as u8, 0]),
            sub(b"DATA", vec![ADJUST_EXPERIENCE_POINTS, MULTIPLY, 1]),
            sub(b"EPFT", vec![1]),
            sub(b"EPFD", f32b(*f)),
            sub(b"PRKF", Vec::new()),
        ]);
    }
    record(b"PERK", 0, fid, &subs)
}

fn placed(sig: &[u8; 4], fid: u32, base: u32) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

fn plugin_bytes() -> Vec<u8> {
    // Educated: Adjust Gained Skill Points (10), Add (2) 2, as in FalloutNV.esm.
    let educated = record(
        b"PERK",
        0,
        EDUCATED,
        &[
            sub(b"EDID", z("Educated")),
            sub(b"DATA", vec![0, 1, 1, 1, 0]),
            sub(b"PRKE", vec![2, 0, 0]),
            sub(b"DATA", vec![10, 2, 1]),
            sub(b"EPFT", vec![1]),
            sub(b"EPFD", f32b(2.0)),
            sub(b"PRKF", Vec::new()),
        ],
    );
    let perks = [perk(SWIFT_LEARNER, "SwiftLearner", &[1.1, 1.2, 1.3]), perk(SKILLED, "TraitSkilled", &[0.9]), educated].concat();
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    // A mine: the weapon (DNAM projectile at +36) and the projectile placed as PGRE.
    let mut dnam = vec![0u8; 120];
    dnam[36..40].copy_from_slice(&MINE_PROJ.to_le_bytes());
    let weap_data = [&10i32.to_le_bytes()[..], &100i32.to_le_bytes(), &0.5f32.to_le_bytes(), &10i16.to_le_bytes(), &[12u8]].concat();
    let mine = record(b"WEAP", 0, MINE, &[sub(b"EDID", z("TestMine")), sub(b"DATA", weap_data), sub(b"DNAM", dnam)]);
    let proj = record(b"PROJ", 0, MINE_PROJ, &[sub(b"EDID", z("TestMineProjectile"))]);
    let stat = record(b"STAT", 0, TERMINAL, &[sub(b"EDID", z("TestTerminal"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [placed(b"PGRE", MINE_REF, MINE_PROJ), placed(b"REFR", TERMINAL_REF, TERMINAL)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"NPC_", &player),
        top(b"PERK", &perks),
        top(b"STAT", &stat),
        top(b"WEAP", &mine),
        top(b"PROJ", &proj),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

/// The plugin, with the terminal's TERM data put in by hand: a Hard terminal (code 3).
fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Xp.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let mut data = GameData::build(&lo, &schema, table, &j, &refs);
    data.terminals.insert(FormId(TERMINAL), TerminalDef { difficulty: 3, ..TerminalDef::default() });
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

#[test]
fn swift_learner_adds_a_tenth_for_each_rank_and_skilled_takes_a_tenth() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(experience::award(&mut g, Source::Script, 100.0), 100.0);
    // Rank 1, 2 and 3: only the highest rank's entry applies (1.1, 1.2, 1.3).
    for (rank, want) in [(1, 110.0), (2, 120.0), (3, 130.0)] {
        g.player.perks.insert(FormId(SWIFT_LEARNER), rank);
        assert_eq!(experience::award(&mut g, Source::Kill, 100.0), want, "rank {rank}");
    }
    // With Skilled as well: 100 × 1.3 × 0.9 = 117.
    g.player.perks.insert(FormId(SKILLED), 1);
    assert_eq!(experience::award(&mut g, Source::Discovery, 100.0), 117.0);
    // Fractions are dropped: 54 × 1.3 × 0.9 = 63.18.
    assert_eq!(experience::award(&mut g, Source::PickLock, 54.0), 63.0);
    assert_eq!(g.player.xp, 100.0 + 110.0 + 120.0 + 130.0 + 117.0 + 63.0);
    // A loss is not changed; nothing is nothing.
    assert_eq!(experience::award(&mut g, Source::Script, -10.0), -10.0);
    assert_eq!(experience::award(&mut g, Source::Script, 0.0), 0.0);
}

#[test]
fn reward_xp_goes_through_the_perks() {
    use fnv_game::functions::Ctx;
    use fnv_script::vm::{ArgValue, Value};
    let f = fixture();
    let mut g = game(&f);
    g.player.perks.insert(FormId(SKILLED), 1);
    let ctx = Ctx { unit: "test".into(), opcode: 0x1177, on: None, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[ArgValue::Value(Value::Num(50.0))]);
    assert_eq!(g.player.xp, 45.0);
}

#[test]
fn a_hacked_terminal_gives_its_class_xp_and_a_locked_out_one_none() {
    use fnv_game::hacking::Difficulty;
    let f = fixture();
    let mut g = game(&f);
    // The wiki's defaults (the test plugin sets none): 20, 30, 40, 50, 60.
    let each: Vec<f64> = [Difficulty::VeryEasy, Difficulty::Easy, Difficulty::Average, Difficulty::Hard, Difficulty::VeryHard, Difficulty::RequiresKey]
        .into_iter()
        .map(|d| terminal::xp(&g, d))
        .collect();
    assert_eq!(each, vec![20.0, 30.0, 40.0, 50.0, 60.0, 0.0]);
    assert_eq!(terminal::hacked(&mut g, FormId(TERMINAL_REF), false), 0.0);
    assert_eq!(g.player.xp, 0.0);
    // The Hard terminal, with Swift Learner rank 1: 55.
    g.player.perks.insert(FormId(SWIFT_LEARNER), 1);
    assert_eq!(terminal::hacked(&mut g, FormId(TERMINAL_REF), true), 55.0);
    assert_eq!(g.player.xp, 55.0);
    assert_eq!(g.terminals.access[&FormId(TERMINAL_REF)], terminal::Access::Hacked);
}

#[test]
fn a_mine_the_player_disarms_gives_its_xp() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(experience::mine_disarm_xp(&g), 5.0);
    let v = g.activate(FormId::PLAYER_REF, FormId(MINE_REF), ActMode::Hand).unwrap();
    assert!(v.to_string().contains("disarmed"), "{v}");
    assert_eq!(g.player.xp, 5.0);
}

/// #1615: by the player's own hand too, once.
#[test]
fn a_mine_disarmed_by_the_players_own_hand_gives_its_xp_once() {
    let f = fixture();
    let mut g = game(&f);
    g.default_activation_by_hand(FormId(MINE_REF)).unwrap().expect("not refused");
    assert_eq!(g.player.xp, 5.0);
    assert_eq!(g.misc_stat(fnv_game::misc_stats::MINES_DISARMED), 1.0);
}

/// Educated's Adjust Gained Skill Points entry adds two points a level
/// (docs/spec-notes/levelup.md §9).
#[test]
fn educated_adds_two_skill_points_a_level() {
    let f = fixture();
    let mut g = game(&f);
    let base = fnv_game::levelup::points_for(&mut g, 3);
    g.player.perks.insert(FormId(EDUCATED), 1);
    assert_eq!(fnv_game::levelup::points_for(&mut g, 3), base + 2);
    let sheet = fnv_game::levelup::SkillSheet::of_player(&mut g, 3);
    assert_eq!(sheet.points, base + 2);
}
