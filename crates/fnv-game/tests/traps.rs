//! Activate parents on a synthetic plugin (no game data): a tripwire's default activation
//! sets off its activate children (XAPR) after their delays, with the tripwire as their
//! action reference; an activation that runs only the parent's OnActivate does not; a
//! "Parent Activate Only" child is flagged. docs/spec-notes/traps.md §2.

mod common;

use common::*;
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{ActMode, Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::world::Outcome;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const WIRE: u32 = 0x300;
const GUN: u32 = 0x301;
const WIRE_REF: FormId = FormId(0x400);
const GUN_NOW: FormId = FormId(0x401);
const GUN_LATER: FormId = FormId(0x402);
const LONE_REF: FormId = FormId(0x403);
const P: FormId = FormId::PLAYER_REF;
const GLOBAL: u32 = 0x800;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn acti(fid: u32, edid: &str) -> Vec<u8> {
    record(b"ACTI", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"MODL", z("Traps\\Test.nif"))])
}

fn placed(id: FormId, base: u32, extra: Vec<Subrecord>) -> Vec<u8> {
    let data = [f32b(100.0), f32b(200.0), f32b(10.0), f32b(0.0), f32b(0.0), f32b(0.0)].concat();
    let mut subs = vec![sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)];
    subs.extend(extra);
    record(b"REFR", 0, id.0, &subs)
}

fn xapr(parent: FormId, delay: f32) -> Subrecord {
    sub(b"XAPR", [parent.0.to_le_bytes().to_vec(), f32b(delay)].concat())
}

fn plugin_bytes() -> Vec<u8> {
    let actis = [acti(WIRE, "TrapTripwire01"), acti(GUN, "trapShotgun")].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [
        placed(WIRE_REF, WIRE, vec![]),
        placed(GUN_NOW, GUN, vec![xapr(WIRE_REF, 0.0), sub(b"XAPD", vec![0])]),
        placed(GUN_LATER, GUN, vec![xapr(WIRE_REF, 1.0)]),
        placed(LONE_REF, GUN, vec![sub(b"XAPD", vec![1])]),
    ]
    .concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    let glob = record(b"GLOB", 0, GLOBAL, &[sub(b"EDID", z("TestGlobal")), sub(b"FNAM", vec![b's']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    [tes4(), top(b"GLOB", &glob), top(b"ACTI", &actis), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat())))].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Traps.esm".into()).unwrap()];
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

/// The activations of `r` so far, with who activated it.
fn activations(g: &Game, r: FormId) -> Vec<FormId> {
    g.events.iter().filter_map(|e| match e.kind {
        EventKind::Activated { reference, by, .. } if reference == r => Some(by),
        _ => None,
    }).collect()
}

#[test]
fn xapr_and_xapd_are_read() {
    let f = fixture();
    assert_eq!(f.refs.activate_children[&WIRE_REF], vec![(GUN_NOW, 0.0), (GUN_LATER, 1.0)]);
    assert!(!f.refs.get(GUN_NOW).unwrap().parent_activate_only);
    assert!(f.refs.get(LONE_REF).unwrap().parent_activate_only);
    let g = game(&f);
    assert!(g.parent_activate_only(LONE_REF));
    assert!(!g.parent_activate_only(GUN_NOW));
}

#[test]
fn default_activation_sets_off_children_after_their_delays() {
    let f = fixture();
    let mut g = game(&f);
    g.cfg.frame_seconds = 0.25;
    // The tripwire's `activate self`: the default action, the tripwire its own actor.
    g.activate(WIRE_REF, WIRE_REF, ActMode::Default).unwrap();
    assert_eq!(g.due_children.len(), 2);
    assert!(activations(&g, GUN_NOW).is_empty());
    g.tick();
    // Delay 0: on the same tick, with the tripwire as the action reference.
    assert_eq!(activations(&g, GUN_NOW), vec![WIRE_REF]);
    assert!(activations(&g, GUN_LATER).is_empty());
    for _ in 0..2 {
        g.tick();
    }
    assert!(activations(&g, GUN_LATER).is_empty(), "0.75 s of the 1 s delay");
    g.tick();
    assert_eq!(activations(&g, GUN_LATER), vec![WIRE_REF]);
    assert!(g.due_children.is_empty());
    let set_off = g.events.iter().filter(|e| matches!(e.kind, EventKind::ActivateChild { parent, .. } if parent == WIRE_REF)).count();
    assert_eq!(set_off, 2);
}

#[test]
fn a_child_disabled_meanwhile_is_skipped() {
    let f = fixture();
    let mut g = game(&f);
    g.cfg.frame_seconds = 0.5;
    g.activate(P, WIRE_REF, ActMode::Hand).unwrap();
    assert_eq!(g.due_children.len(), 2, "the hand on a reference without OnActivate is its default action");
    g.world.set_enabled(GUN_LATER, false).unwrap();
    for _ in 0..4 {
        g.tick();
    }
    assert_eq!(activations(&g, GUN_NOW), vec![WIRE_REF]);
    assert!(activations(&g, GUN_LATER).is_empty());
}

/// A save while a child waits for its delay keeps it waiting: after the load it goes off
/// when the delay passes (#1090).
#[test]
fn a_waiting_child_survives_a_save_and_load() {
    let f = fixture();
    let mut g = game(&f);
    g.cfg.frame_seconds = 0.25;
    g.activate(WIRE_REF, WIRE_REF, ActMode::Default).unwrap();
    g.tick();
    assert_eq!(g.due_children.len(), 1, "the 1 s child waits");
    let text = serde_json::to_string(&g.snapshot()).unwrap();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&text).unwrap();
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let mut h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    assert_eq!(h.due_children, g.due_children);
    for _ in 0..2 {
        h.tick();
    }
    assert!(activations(&h, GUN_LATER).is_empty(), "0.75 s of the 1 s delay");
    h.tick();
    assert_eq!(activations(&h, GUN_LATER), vec![WIRE_REF]);
    assert!(h.due_children.is_empty());
}

/// #1615: the player's own activation of something without an OnActivate block has the
/// shared activation's consequences, each once: the receiver's switch, what the player is
/// activating (IsWaterObject), the action reference, the activate children.
#[test]
fn the_players_own_activation_has_every_consequence_once() {
    let f = fixture();
    let mut g = game(&f);
    let done = g.default_activation_by_hand(WIRE_REF).unwrap().expect("not refused");
    assert_eq!(done.activation.outcome, Outcome::Activated);
    assert_eq!(done.item_added_event, None);
    assert!(matches!(g.events[done.activated_event].kind, EventKind::Activated { reference, by, .. } if reference == WIRE_REF && by == P));
    assert_eq!(activations(&g, WIRE_REF), vec![P]);
    assert_eq!((g.radio_switches(WIRE_REF), g.radio_on(WIRE_REF)), (1, true));
    assert_eq!(g.activating(P), Some(WIRE_REF));
    assert_eq!(g.action_refs.get(&WIRE_REF), Some(&P));
    assert_eq!(g.due_children.len(), 2);
    // Switched back: a receiver is a toggle.
    g.default_activation_by_hand(WIRE_REF).unwrap().expect("not refused");
    assert_eq!((g.radio_switches(WIRE_REF), g.radio_on(WIRE_REF)), (2, false));
    assert_eq!(activations(&g, WIRE_REF), vec![P, P]);
}

/// #1615: the extender's OnActivate handlers hear the player's own activation once, as they
/// hear `Game::activate`'s, and also when it is then refused [documented: NVSE Event
/// Handling].
#[test]
fn the_extender_hears_the_players_own_activation_once() {
    let f = fixture();
    let mut g = game(&f);
    g.run_source("SetEventHandler \"OnActivate\" (begin function {ref rTarget, ref rActor}\nset TestGlobal to TestGlobal + 1\nend)", Default::default()).expect("compiles");
    let heard = |g: &Game| g.world.global(FormId(GLOBAL)).unwrap();
    g.default_activation_by_hand(WIRE_REF).unwrap().expect("not refused");
    assert_eq!(heard(&g), 1.0);
    g.activate(P, WIRE_REF, ActMode::Hand).unwrap();
    assert_eq!(heard(&g), 2.0);
    g.extra.entry(GUN_NOW).or_default().destroyed = true;
    assert!(g.default_activation_by_hand(GUN_NOW).unwrap().is_none(), "a destroyed activator refuses");
    assert_eq!(heard(&g), 3.0);
    assert!(activations(&g, GUN_NOW).is_empty());
    assert_eq!(g.radio_switches(GUN_NOW), 0);
}
