//! The game clock as scripts move it (#1377; simulation.md §5).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{clock, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn glob(fid: u32, name: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(name)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

fn plugin_bytes() -> Vec<u8> {
    let globals = [
        glob(0x901, "GameYear", 2281.0),
        glob(0x902, "GameMonth", 9.0),
        glob(0x903, "GameDay", 18.0),
        glob(0x904, "GameHour", 12.0),
        glob(0x905, "GameDaysPassed", 5.0),
        glob(0x906, "TimeScale", 30.0),
    ]
    .concat();
    [tes4(), top(b"GLOB", &globals)].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "GameClock.esm".into()).unwrap()];
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

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

const FRAME: f64 = 1.0 / 60.0;

#[test]
fn set_game_hour_0_reaches_midnight_after_an_ordinary_frame() {
    let f = fixture();
    let mut g = game(&f);
    g.world.advance_real_seconds(FRAME).unwrap();
    q(&mut g, "SetGameHour 0");
    let c = g.world.calendar().unwrap();
    assert_eq!((c.day, c.hour), (19.0, 0.0));
    assert!((c.days_passed - 5.5).abs() < 1e-5, "{}", c.days_passed);
}

#[test]
fn set_game_hour_to_the_hour_shown_passes_no_day() {
    let f = fixture();
    let mut g = game(&f);
    g.world.advance_real_seconds(FRAME).unwrap();
    let before = g.world.calendar().unwrap();
    q(&mut g, &format!("SetGameHour {}", before.hour));
    assert_eq!(g.world.calendar().unwrap(), before);
    q(&mut g, "SetGameHour");
    assert_eq!(g.world.calendar().unwrap(), before);
}

#[test]
fn a_script_set_of_the_same_value_is_the_time() {
    let f = fixture();
    let mut g = game(&f);
    let (days, _) = g.world.forms.global_by_edid(clock::GAME_DAYS_PASSED).unwrap();
    g.world.set_global(days, 1000.0).unwrap();
    for _ in 0..4 {
        g.world.advance_real_seconds(FRAME).unwrap();
    }
    assert_eq!(g.world.global(days).unwrap(), 1000.0);
    q(&mut g, "Set GameDaysPassed to 1000");
    for _ in 0..2 {
        g.world.advance_real_seconds(FRAME).unwrap();
    }
    assert_eq!(g.world.global(days).unwrap(), 1000.0);
}
