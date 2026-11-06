//! Frames of active effects whose scripts change the effect list while it runs, on a
//! synthetic plugin (no game data): a dispel of an effect still to come this frame, of two
//! effects that already ran, a cast that adds one, and a lethal tick whose death ends
//! effects before and at the one ticking, inside its value change. The effects run in list
//! order, a removed one is skipped, an added one waits for the next frame, a value changed
//! by the second keeps its rate, and a call's trace carries its unit's key (#1389;
//! docs/spec-notes/scripts.md §7.6, aid-items.md, hit-effects.md §4).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::{actor_value, add_actor_value_damage, set_actor_value_current};
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const P: FormId = FormId::PLAYER_REF;
const HEALTH: u16 = 16;
const DISPEL: u16 = 0x1142;
const CAST_IMMEDIATE_ON_SELF: u16 = 0x11D9;

// The counter every block raises, and where each block notes the count it saw.
const G_N: u32 = 0x801;
const A_UP: u32 = 0x802;
const A_FIN: u32 = 0x803;
const B_UP: u32 = 0x804;
const B_FIN: u32 = 0x805;
const C_UP: u32 = 0x806;
const C_FIN: u32 = 0x807;
const D_START: u32 = 0x808;
const D_UP: u32 = 0x809;
const E_UP: u32 = 0x80A;
const F_UP: u32 = 0x80B;
const DEATH: u32 = 0x80C;
const W_UP: u32 = 0x80D;
const W_FIN: u32 = 0x80E;
const K1_UP: u32 = 0x80F;
const K2_UP: u32 = 0x810;
const Z_UP: u32 = 0x811;
const Z_FIN: u32 = 0x812;

const FX_A: u32 = 0x850;
const FX_B: u32 = 0x851;
const FX_C: u32 = 0x852;
const FX_D: u32 = 0x853;
const FX_E: u32 = 0x854;
const FX_F: u32 = 0x855;
const FX_HEAL: u32 = 0x856;
const FX_BAD: u32 = 0x857;
const FX_W: u32 = 0x858;
const FX_POISON: u32 = 0x859;
const FX_K1: u32 = 0x85A;
const FX_K2: u32 = 0x85B;
const FX_Z: u32 = 0x85C;

const S_A: u32 = 0x901;
const S_B: u32 = 0x902;
const S_C: u32 = 0x903;
const S_D: u32 = 0x904;
const S_E: u32 = 0x905;
const S_F: u32 = 0x906;
const S_BAD: u32 = 0x907;
const S_W: u32 = 0x908;
const S_K1: u32 = 0x909;
const S_K2: u32 = 0x90A;
const S_Z: u32 = 0x90B;
const S_NPC: u32 = 0x90C;

const SP_A: u32 = 0x860;
const SP_B: u32 = 0x861;
const SP_C: u32 = 0x862;
const SP_D: u32 = 0x863;
const SP_E: u32 = 0x864;
const SP_F: u32 = 0x865;
const SP_HEAL: u32 = 0x866;
const SP_BAD: u32 = 0x867;
const SP_W: u32 = 0x868;
const SP_POISON: u32 = 0x869;
const SP_K1: u32 = 0x86A;
const SP_K2: u32 = 0x86B;
const SP_Z: u32 = 0x86C;

const NPC: u32 = 0x720;
const NPC_REF: u32 = 0x320;

/// MGEF flags: Hostile, Detrimental, No Death Dispel (hit-effects.md §4).
const HOSTILE: u32 = 1;
const DETRIMENTAL: u32 = 4;
const NO_DEATH_DISPEL: u32 = 1 << 28;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn glob(fid: u32, edid: &str) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", f32b(0.0))])
}

/// MGEF DATA (72 bytes): flags, associated item (the script), archetype, actor value.
fn mgef(fid: u32, edid: &str, flags: u32, item: u32, archetype: u32, av: i32) -> Vec<u8> {
    let data = [u32s(&[flags]), f32b(0.0), u32s(&[item]), vec![0xFF; 8], vec![0; 4], vec![0; 4], f32b(1.0), vec![0; 24], vec![0; 8], u32s(&[archetype]), av.to_le_bytes().to_vec()].concat();
    assert_eq!(data.len(), 72);
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

/// A Script effect (archetype 1) running `script`.
fn script_fx(fid: u32, edid: &str, script: u32) -> Vec<u8> {
    mgef(fid, edid, 0, script, 1, -1)
}

/// SPEL (actor effect) with one effect: magnitude, duration, actor value.
fn spel(fid: u32, edid: &str, fx: u32, magnitude: u32, duration: u32, av: i32) -> Vec<u8> {
    let efit = [u32s(&[magnitude, 0, duration, 0]), av.to_le_bytes().to_vec()].concat();
    let subs = [sub(b"EDID", z(edid)), sub(b"SPIT", [u32s(&[0, 0, 0]), vec![0; 4]].concat()), sub(b"EFID", fx.to_le_bytes().to_vec()), sub(b"EFIT", efit)];
    record(b"SPEL", 0, fid, &subs)
}

/// An effect script (SCHR type 0x100) with these blocks and references.
fn scpt(fid: u32, edid: &str, blocks: &[BlockBytes], refs: &[u32]) -> Vec<u8> {
    record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], unit(0x100, &script(blocks), &[], refs)].concat())
}

/// `set GN to GN + 1`, then `set <global slot> to GN` (GN is reference 1).
fn mark(slot: u16) -> Vec<u8> {
    [set(var(b'G', 1), ex(&[var(b'G', 1), a("1"), a("+")])), set(var(b'G', slot), ex(&[var(b'G', 1)]))].concat()
}

/// A call with one form parameter: the unit's reference `slot`.
fn call_on(op: u16, slot: u16) -> Vec<u8> {
    st(op, &[le16(1), vec![b'r'], le16(slot)].concat())
}

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        glob(G_N, "GN"),
        glob(A_UP, "AUp"),
        glob(A_FIN, "AFin"),
        glob(B_UP, "BUp"),
        glob(B_FIN, "BFin"),
        glob(C_UP, "CUp"),
        glob(C_FIN, "CFin"),
        glob(D_START, "DStart"),
        glob(D_UP, "DUp"),
        glob(E_UP, "EUp"),
        glob(F_UP, "FUp"),
        glob(DEATH, "Death"),
        glob(W_UP, "WUp"),
        glob(W_FIN, "WFin"),
        glob(K1_UP, "K1Up"),
        glob(K2_UP, "K2Up"),
        glob(Z_UP, "ZUp"),
        glob(Z_FIN, "ZFin"),
    ]
    .concat();
    use fnv_game::game::event::{SCRIPT_EFFECT_FINISH as FINISH, SCRIPT_EFFECT_START as START, SCRIPT_EFFECT_UPDATE as UPDATE};
    let scripts = [
        // A: Update and Finish noted.
        scpt(S_A, "AScript", &[(UPDATE, None, mark(2)), (FINISH, None, mark(3))], &[G_N, A_UP, A_FIN]),
        // B: Update noted, then dispels C (still to come this frame) and casts D.
        scpt(
            S_B,
            "BScript",
            &[(UPDATE, None, [mark(2), call_on(DISPEL, 4), call_on(CAST_IMMEDIATE_ON_SELF, 5)].concat()), (FINISH, None, mark(3))],
            &[G_N, B_UP, B_FIN, SP_C, SP_D],
        ),
        // C: Update (never reached) and Finish noted.
        scpt(S_C, "CScript", &[(UPDATE, None, mark(2)), (FINISH, None, mark(3))], &[G_N, C_UP, C_FIN]),
        // D: Start and Update noted.
        scpt(S_D, "DScript", &[(START, None, mark(2)), (UPDATE, None, mark(3))], &[G_N, D_START, D_UP]),
        // E: Update noted, then dispels A and B, which ran before it this frame.
        scpt(S_E, "EScript", &[(UPDATE, None, [mark(2), call_on(DISPEL, 3), call_on(DISPEL, 4)].concat())], &[G_N, E_UP, SP_A, SP_B]),
        // F: Update noted.
        scpt(S_F, "FScript", &[(UPDATE, None, mark(2))], &[G_N, F_UP]),
        // Bad: casts a global, which is no magic item: the call fails.
        scpt(S_BAD, "BadScript", &[(UPDATE, None, call_on(CAST_IMMEDIATE_ON_SELF, 1))], &[G_N]),
        // The lethal tick's neighbours: W before it, K1 and K2 (No Death Dispel) and Z
        // after it.
        scpt(S_W, "WScript", &[(UPDATE, None, mark(2)), (FINISH, None, mark(3))], &[G_N, W_UP, W_FIN]),
        scpt(S_K1, "K1Script", &[(UPDATE, None, mark(2))], &[G_N, K1_UP]),
        scpt(S_K2, "K2Script", &[(UPDATE, None, mark(2))], &[G_N, K2_UP]),
        scpt(S_Z, "ZScript", &[(UPDATE, None, mark(2)), (FINISH, None, mark(3))], &[G_N, Z_UP, Z_FIN]),
        // The NPC's object script: OnDeath noted.
        record(b"SCPT", 0, S_NPC, &[vec![sub(b"EDID", z("NpcScript"))], unit(0, &script(&[(fnv_game::game::event::ON_DEATH, None, mark(2))]), &[], &[G_N, DEATH])].concat()),
    ]
    .concat();
    let mgefs = [
        script_fx(FX_A, "AEffect", S_A),
        script_fx(FX_B, "BEffect", S_B),
        script_fx(FX_C, "CEffect", S_C),
        script_fx(FX_D, "DEffect", S_D),
        script_fx(FX_E, "EEffect", S_E),
        script_fx(FX_F, "FEffect", S_F),
        script_fx(FX_BAD, "BadEffect", S_BAD),
        // Restore Health by the second: a Value Modifier without Recover (aid-items.md §3).
        mgef(FX_HEAL, "RestoreHealth", 0, 0, 0, i32::from(HEALTH)),
        script_fx(FX_W, "WEffect", S_W),
        mgef(FX_K1, "K1Effect", NO_DEATH_DISPEL, S_K1, 1, -1),
        mgef(FX_K2, "K2Effect", NO_DEATH_DISPEL, S_K2, 1, -1),
        script_fx(FX_Z, "ZEffect", S_Z),
        // Damage Health by the second: harmful, so the magnitude is taken as it is.
        mgef(FX_POISON, "Poison", HOSTILE | DETRIMENTAL, 0, 0, i32::from(HEALTH)),
    ]
    .concat();
    let spels = [
        spel(SP_A, "ASpell", FX_A, 0, 10, -1),
        spel(SP_B, "BSpell", FX_B, 0, 10, -1),
        spel(SP_C, "CSpell", FX_C, 0, 10, -1),
        spel(SP_D, "DSpell", FX_D, 0, 10, -1),
        spel(SP_E, "ESpell", FX_E, 0, 10, -1),
        spel(SP_F, "FSpell", FX_F, 0, 10, -1),
        spel(SP_BAD, "BadSpell", FX_BAD, 0, 10, -1),
        // 2 Health a second for 2 s.
        spel(SP_HEAL, "HealSpell", FX_HEAL, 2, 2, i32::from(HEALTH)),
        spel(SP_W, "WSpell", FX_W, 0, 10, -1),
        spel(SP_K1, "K1Spell", FX_K1, 0, 10, -1),
        spel(SP_K2, "K2Spell", FX_K2, 0, 10, -1),
        spel(SP_Z, "ZSpell", FX_Z, 0, 10, -1),
        // 1000 Health a second for 10 s: the first second kills.
        spel(SP_POISON, "PoisonSpell", FX_POISON, 1000, 10, i32::from(HEALTH)),
    ]
    .concat();
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    // An NPC with Health 100 and the OnDeath script, placed in the cell.
    let npc = record(
        b"NPC_",
        0,
        NPC,
        &[
            sub(b"EDID", z("TestNpc")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&100i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", S_NPC.to_le_bytes().to_vec()),
        ],
    );
    let achr = record(b"ACHR", 0, NPC_REF, &[sub(b"NAME", NPC.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &achr));
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"NPC_", &[player, npc].concat()),
        top(b"MGEF", &mgefs),
        top(b"SCPT", &scripts),
        top(b"SPEL", &spels),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Effects.esm".into()).unwrap()];
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

/// The player at Health 50 of 100.
fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    set_actor_value_current(&mut g, P, HEALTH, 100.0).unwrap();
    add_actor_value_damage(&mut g, P, HEALTH, 50.0);
    g
}

fn glob_value(g: &Game, id: u32) -> f32 {
    g.world.global(FormId(id)).unwrap()
}

/// The count each block saw, in the order of the globals.
fn marks(g: &Game) -> [f32; 10] {
    [A_UP, A_FIN, B_UP, B_FIN, C_UP, C_FIN, D_START, D_UP, E_UP, F_UP].map(|id| glob_value(g, id))
}

/// The spells whose effects finished since event `from`, in order.
fn finished(g: &Game, from: usize) -> Vec<u32> {
    g.events[from..]
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::EffectFinished { source, .. } => Some(source.0),
            _ => None,
        })
        .collect()
}

#[test]
fn effects_run_in_list_order_while_their_scripts_remove_and_add_effects() {
    let f = fixture();
    let mut g = game(&f);
    for spell in [SP_A, SP_B, SP_HEAL, SP_C, SP_E, SP_F] {
        assert_eq!(g.apply_magic_item(P, FormId(spell), Some(P), false).len(), 1, "{spell:X} took");
    }
    let sources = |g: &Game| g.effects.iter().map(|e| e.source.0).collect::<Vec<_>>();
    assert_eq!(sources(&g), [SP_A, SP_B, SP_HEAL, SP_C, SP_E, SP_F]);
    let health = |g: &mut Game| actor_value(g, P, HEALTH).unwrap();
    assert!((health(&mut g) - 50.0).abs() < 1e-6);

    // Frame 1. A runs (1); B runs (2), its dispel ends C, whose Finish runs (3), and its
    // cast starts D (4); the heal gives 2; C is skipped; E runs (5) and ends A (6) and B
    // (7), which ran before it; F, now two places further up the list, runs (8). D,
    // added during the frame, does not run its Update yet.
    let from = g.events.len();
    g.advance_effects(1.0);
    //                     A_UP A_FIN B_UP B_FIN C_UP C_FIN D_START D_UP E_UP F_UP
    assert_eq!(marks(&g), [1.0, 6.0, 2.0, 7.0, 0.0, 3.0, 4.0, 0.0, 5.0, 8.0]);
    assert_eq!(finished(&g, from), [SP_C, SP_A, SP_B]);
    assert_eq!(sources(&g), [SP_HEAL, SP_E, SP_F, SP_D]);
    assert!((health(&mut g) - 52.0).abs() < 1e-6, "{}", health(&mut g));

    // Frame 2: the heal gives its last 2 and ends (its 2 s are up); E (9), F (10) and D
    // (11) run, E dispelling nothing now.
    let from = g.events.len();
    g.advance_effects(1.0);
    assert_eq!(marks(&g), [1.0, 6.0, 2.0, 7.0, 0.0, 3.0, 4.0, 11.0, 9.0, 10.0]);
    assert_eq!(finished(&g, from), [SP_HEAL]);
    assert_eq!(sources(&g), [SP_E, SP_F, SP_D]);
    assert!((health(&mut g) - 54.0).abs() < 1e-6, "{}", health(&mut g));

    // Frame 3: E (12), F (13), D (14); the health holds.
    g.advance_effects(1.0);
    assert_eq!(marks(&g), [1.0, 6.0, 2.0, 7.0, 0.0, 3.0, 4.0, 14.0, 12.0, 13.0]);
    assert!((health(&mut g) - 54.0).abs() < 1e-6, "{}", health(&mut g));
    assert_eq!(glob_value(&g, G_N), 14.0);
}

#[test]
fn a_failed_call_names_its_unit_by_the_units_key() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.apply_magic_item(P, FormId(SP_BAD), Some(P), false).len(), 1);
    let from = g.events.len();
    g.advance_effects(1.0);
    let units: Vec<&str> = g.events[from..]
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::CallFailed { unit, .. } => Some(unit.as_str()),
            _ => None,
        })
        .collect();
    // The key as the unit builds it (`ScriptUnit::key`), the plugin's name and the owner.
    let key = f.data.script(FormId(S_BAD)).unwrap().unit.key();
    assert_eq!(key, "Effects.esm|SCPT 0x00000907");
    assert_eq!(units, [key.as_str()]);
}

#[test]
fn a_lethal_tick_whose_death_ends_effects_leaves_the_rest_in_order() {
    let f = fixture();
    let mut g = game(&f);
    let npc = FormId(NPC_REF);
    for spell in [SP_W, SP_POISON, SP_K1, SP_K2, SP_Z] {
        assert_eq!(g.apply_magic_item(npc, FormId(spell), None, false).len(), 1, "{spell:X} took");
    }
    let death_marks = |g: &Game| [DEATH, W_UP, W_FIN, K1_UP, K2_UP, Z_UP, Z_FIN].map(|id| glob_value(g, id));
    let sources = |g: &Game| g.effects.iter().filter(|e| e.target == npc).map(|e| e.source.0).collect::<Vec<_>>();
    assert_eq!(sources(&g), [SP_W, SP_POISON, SP_K1, SP_K2, SP_Z]);

    // Frame 1. W runs (1). The poison's tick kills inside its value change: OnDeath runs
    // (2), then the death ends W (its Finish, 3), the poison itself and Z (its Finish,
    // 4), in list order, and keeps K1 and K2. The poison's change and Update then find
    // no effect (its old place now holds K2); K1, now before the cursor, runs (5), then
    // K2 (6); Z is skipped.
    let from = g.events.len();
    g.advance_effects(1.0);
    assert!(g.is_dead(npc));
    //                     DEATH W_UP W_FIN K1_UP K2_UP Z_UP Z_FIN
    assert_eq!(death_marks(&g), [2.0, 1.0, 3.0, 5.0, 6.0, 0.0, 4.0]);
    assert_eq!(finished(&g, from), [SP_W, SP_POISON, SP_Z]);
    assert_eq!(sources(&g), [SP_K1, SP_K2]);

    // Frame 2: K1 (7) and K2 (8) go on on the body; no second death.
    g.advance_effects(1.0);
    assert_eq!(death_marks(&g), [2.0, 1.0, 3.0, 7.0, 8.0, 0.0, 4.0]);
    assert_eq!(glob_value(&g, G_N), 8.0);
}
