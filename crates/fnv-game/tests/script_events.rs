//! The script blocks OnOpen, OnClose, OnActorEquip and OnMagicEffectHit on a synthetic
//! plugin (no game data); OnSell is never run, as in the original. docs/spec-notes/
//! script-functions.md §3.3.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::HitInfo;
use fnv_game::game::{event, Game, GameConfig, InstKey};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const BURN_FX: u32 = 0x850;
const OTHER_FX: u32 = 0x851;
const BURNING: u32 = 0x860;
const DOOR_SCRIPT: u32 = 0x901;
const TRAP_SCRIPT: u32 = 0x902;
const NPC_SCRIPT: u32 = 0x903;
const HURT_SCRIPT: u32 = 0x904;
const DOOR: u32 = 0x700;
const TRAP: u32 = 0x710;
const NPC: u32 = 0x720;
const HURT_NPC: u32 = 0x721;
const ITEM: u32 = 0x730;
const BOX: u32 = 0x740;
const DOOR_REF: u32 = 0x300;
const BOX_REF: u32 = 0x302;
const TRAP_REF: u32 = 0x301;
const NPC_REF: u32 = 0x320;
const DAMAGED_REF: u32 = 0x321;
const HIT_REF: u32 = 0x322;
const KILL_ACTOR: u16 = 0x108B;
const DAMAGE_ACTOR_VALUE: u16 = 0x1181;
const HEALTH: u16 = 16;

fn r(i: u16) -> Vec<u8> {
    [vec![b'r'], le16(i)].concat()
}

/// `set <var i> to <var i> + 1`.
fn bump(i: u16) -> Vec<u8> {
    set(var(b's', i), ex(&[var(b's', i), a("1"), a("+")]))
}

/// A function call statement with no parameters.
fn call0(op: u16) -> Vec<u8> {
    st(op, &le16(0))
}

/// `DamageAV Health 9999`.
fn damage_health() -> Vec<u8> {
    st(DAMAGE_ACTOR_VALUE, &[le16(2), le16(HEALTH), vec![b'n'], 9999i32.to_le_bytes().to_vec()].concat())
}

/// A block with one form parameter (the unit's reference `i`).
fn on(i: u16) -> Option<Vec<u8>> {
    Some([le16(1), r(i)].concat())
}

fn mgef(fid: u32, edid: &str) -> Vec<u8> {
    // MGEF DATA (72 bytes): Value Modifier on Health, hostile and detrimental.
    let data = [u32s(&[1 | 4]), vec![0; 4], u32s(&[0]), (-1i32).to_le_bytes().to_vec(), (-1i32).to_le_bytes().to_vec(), vec![0; 4], vec![0; 4], 1f32.to_le_bytes().to_vec(), vec![0; 24], vec![0; 8], u32s(&[0]), 16i32.to_le_bytes().to_vec()].concat();
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

fn placed(sig: &[u8; 4], fid: u32, base: u32) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

fn plugin_bytes() -> Vec<u8> {
    // Door: nOpened (1), nClosed (2). Trap: nBurnt (1), nOther (2). NPC: nPowder (1),
    // nBurnt (2), nSold (3), nKilled (4), nSeen (5): OnActivate kills it and then sets
    // nKilled, OnDeath copies nKilled into nSeen.
    let scripts = [
        record(b"SCPT", 0, DOOR_SCRIPT, &[vec![sub(b"EDID", z("DoorScript"))], unit(0, &script(&[(event::ON_OPEN, None, bump(1)), (event::ON_CLOSE, None, bump(2))]), &[(1, "nOpened", 1), (2, "nClosed", 1)], &[])].concat()),
        record(
            b"SCPT",
            0,
            TRAP_SCRIPT,
            &[vec![sub(b"EDID", z("TrapScript"))], unit(0, &script(&[(event::ON_MAGIC_EFFECT_HIT, on(1), bump(1)), (event::ON_MAGIC_EFFECT_HIT, on(2), bump(2))]), &[(1, "nBurnt", 1), (2, "nOther", 1)], &[BURN_FX, OTHER_FX])].concat(),
        ),
        record(
            b"SCPT",
            0,
            NPC_SCRIPT,
            &[
                vec![sub(b"EDID", z("NpcScript"))],
                unit(
                    0,
                    &script(&[
                        (event::ON_ACTOR_EQUIP, on(1), bump(1)),
                        (event::ON_MAGIC_EFFECT_HIT, on(2), bump(2)),
                        (event::ON_SELL, None, bump(3)),
                        (event::ON_ACTIVATE, None, [call0(KILL_ACTOR), set(var(b's', 4), ex(&[a("1")]))].concat()),
                        (event::ON_DEATH, None, set(var(b's', 5), ex(&[var(b's', 4)]))),
                    ]),
                    &[(1, "nPowder", 1), (2, "nBurnt", 1), (3, "nSold", 1), (4, "nKilled", 1), (5, "nSeen", 1)],
                    &[ITEM, BURN_FX],
                ),
            ]
            .concat(),
        ),
        // nDamaged (1), nSeen (2): as the NPC's, by damage; OnHit does the same damage.
        record(
            b"SCPT",
            0,
            HURT_SCRIPT,
            &[
                vec![sub(b"EDID", z("HurtScript"))],
                unit(
                    0,
                    &script(&[
                        (event::ON_ACTIVATE, None, [damage_health(), set(var(b's', 1), ex(&[a("1")]))].concat()),
                        (event::ON_DEATH, None, set(var(b's', 2), ex(&[var(b's', 1)]))),
                        (event::ON_HIT, None, damage_health()),
                    ]),
                    &[(1, "nDamaged", 1), (2, "nSeen", 1)],
                    &[],
                ),
            ]
            .concat(),
        ),
    ]
    .concat();
    let door = record(b"DOOR", 0, DOOR, &[sub(b"EDID", z("TestDoor")), sub(b"SCRI", DOOR_SCRIPT.to_le_bytes().to_vec())]);
    // A container running the door's script (nOpened, nClosed).
    let cont = record(b"CONT", 0, BOX, &[sub(b"EDID", z("TestBox")), sub(b"SCRI", DOOR_SCRIPT.to_le_bytes().to_vec()), sub(b"DATA", [vec![0], 0f32.to_le_bytes().to_vec()].concat())]);
    let trap = record(b"ACTI", 0, TRAP, &[sub(b"EDID", z("TestTrap")), sub(b"SCRI", TRAP_SCRIPT.to_le_bytes().to_vec())]);
    let item = record(b"MISC", 0, ITEM, &[sub(b"EDID", z("TestPowder"))]);
    let npc = record(
        b"NPC_",
        0,
        NPC,
        &[
            sub(b"EDID", z("TestNpc")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&100i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", NPC_SCRIPT.to_le_bytes().to_vec()),
        ],
    );
    let hurt_npc = record(
        b"NPC_",
        0,
        HURT_NPC,
        &[
            sub(b"EDID", z("TestHurtNpc")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&100i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", HURT_SCRIPT.to_le_bytes().to_vec()),
        ],
    );
    let mut ench = vec![sub(b"EDID", z("Burning")), sub(b"ENIT", [u32s(&[2, 0, 0]), vec![0; 4]].concat())];
    ench.extend([sub(b"EFID", BURN_FX.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[1, 0, 5, 1]), 16i32.to_le_bytes().to_vec()].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &[placed(b"REFR", DOOR_REF, DOOR), placed(b"REFR", TRAP_REF, TRAP), placed(b"REFR", BOX_REF, BOX), placed(b"ACHR", NPC_REF, NPC), placed(b"ACHR", DAMAGED_REF, HURT_NPC), placed(b"ACHR", HIT_REF, HURT_NPC)].concat()));
    [
        tes4(),
        top(b"MGEF", &[mgef(BURN_FX, "Burn"), mgef(OTHER_FX, "Other")].concat()),
        top(b"ENCH", &record(b"ENCH", 0, BURNING, &ench)),
        top(b"SCPT", &scripts),
        top(b"MISC", &item),
        top(b"ACTI", &trap),
        top(b"CONT", &cont),
        top(b"DOOR", &door),
        top(b"NPC_", &[npc, hurt_npc].concat()),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Events.esm".into()).unwrap()];
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
    g.new_game();
    g.set_loaded([FormId(CELL)].into_iter().collect());
    g
}

fn count(g: &Game, r: u32, i: u16) -> f64 {
    g.var(InstKey::Ref { reference: FormId(r) }, i).num()
}

#[test]
fn a_door_runs_on_open_and_on_close_when_its_open_flag_changes() {
    let f = fixture();
    let mut g = game(&f);
    // The first look only notes the state.
    g.tick();
    assert_eq!((count(&g, DOOR_REF, 1), count(&g, DOOR_REF, 2)), (0.0, 0.0));
    // Opened (by the player, an NPC or SetOpenState: the flag is what is watched).
    g.world.get(FormId(DOOR_REF)).unwrap().open = true;
    g.tick();
    assert_eq!(count(&g, DOOR_REF, 1), 1.0);
    g.tick();
    assert_eq!(count(&g, DOOR_REF, 1), 1.0, "once per opening");
    g.world.get(FormId(DOOR_REF)).unwrap().open = false;
    g.tick();
    assert_eq!((count(&g, DOOR_REF, 1), count(&g, DOOR_REF, 2)), (1.0, 1.0));
}

#[test]
fn a_queued_open_runs_on_the_next_frame() {
    let f = fixture();
    let mut g = game(&f);
    // The engine's container menu opening and closing (`queue_ref_event`).
    g.queue_ref_event(FormId(DOOR_REF), event::ON_OPEN, Some(FormId::PLAYER_REF));
    assert_eq!(count(&g, DOOR_REF, 1), 0.0);
    g.tick();
    assert_eq!(count(&g, DOOR_REF, 1), 1.0);
    g.queue_ref_event(FormId(DOOR_REF), event::ON_CLOSE, Some(FormId::PLAYER_REF));
    g.tick();
    assert_eq!(count(&g, DOOR_REF, 2), 1.0);
}

#[test]
fn the_runners_player_activation_of_a_container_runs_its_on_open_and_on_close() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(serde_json::json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "player_activate", "target": format!("0x{BOX_REF:08X}"), "label": "open"},
            {"op": "get_ref", "ref": format!("0x{BOX_REF:08X}"), "fields": ["var.nOpened", "var.nClosed"], "label": "after"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let obs = out["observations"].as_array().unwrap();
    let by = |l: &str| obs.iter().find(|o| o["label"] == l).unwrap().clone();
    assert_eq!(by("open")["opened"], format!("0x{BOX_REF:08X}"));
    assert_eq!(by("after")["var.nOpened"], 1.0);
    assert_eq!(by("after")["var.nClosed"], 1.0);
}

#[test]
fn a_magic_effect_hit_runs_the_block_of_that_effect() {
    let f = fixture();
    let mut g = game(&f);
    // A weapon's object effect reaching an object (the Vault 22 traps and the flamer).
    assert!(g.magic_effect_hits(FormId(TRAP_REF), &[FormId(BURNING)], Some(FormId::PLAYER_REF)));
    assert_eq!((count(&g, TRAP_REF, 1), count(&g, TRAP_REF, 2)), (1.0, 0.0));
    assert!(!g.magic_effect_hits(FormId(DOOR_REF), &[FormId(BURNING)], None), "no such block");
    // On an actor, through the effect applied by a hit.
    let applied = g.apply_hit_effect(FormId(NPC_REF), FormId(BURNING), Some(FormId::PLAYER_REF));
    assert_eq!(applied.len(), 1);
    assert_eq!(count(&g, NPC_REF, 2), 1.0);
}

#[test]
fn on_actor_equip_runs_on_the_actor_and_on_sell_never_runs() {
    let f = fixture();
    let mut g = game(&f);
    g.equip_events(FormId(NPC_REF), FormId(ITEM), true);
    assert_eq!(count(&g, NPC_REF, 1), 1.0);
    // Another item: the block's parameter does not accept it.
    g.equip_events(FormId(NPC_REF), FormId(TRAP), false);
    assert_eq!(count(&g, NPC_REF, 1), 1.0);
    // Nothing in the engine fires OnSell; the frames do not run it either.
    g.frames(5);
    assert_eq!(count(&g, NPC_REF, 3), 0.0);
}

#[test]
fn on_death_of_a_kill_actor_waits_for_the_killing_script_to_finish() {
    // One for My Baby: the victim's script calls KillActor and then sets Boone's
    // bGotRevenge; its OnDeath clears the accused when bGotRevenge is still 0. OnDeath
    // runs after the killing script, so it sees what the script set after KillActor.
    let f = fixture();
    let mut g = game(&f);
    assert!(g.ref_event(FormId(NPC_REF), event::ON_ACTIVATE, None, None));
    assert!(g.is_dead(FormId(NPC_REF)));
    assert_eq!(count(&g, NPC_REF, 4), 1.0);
    assert_eq!(count(&g, NPC_REF, 5), 1.0, "OnDeath ran before the rest of the killing block");
}

#[test]
fn on_death_of_a_damage_actor_value_waits_for_the_damaging_script_to_finish() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.ref_event(FormId(DAMAGED_REF), event::ON_ACTIVATE, None, None));
    assert!(g.is_dead(FormId(DAMAGED_REF)));
    assert_eq!(count(&g, DAMAGED_REF, 1), 1.0);
    assert_eq!(count(&g, DAMAGED_REF, 2), 1.0, "OnDeath ran before the rest of the damaging block");
}

#[test]
fn a_kill_finished_by_an_on_hit_script_is_the_attackers() {
    // The boxing gloves' script does `DamageAV Health 10` in its OnHit block: the hit that
    // ran the block is what killed.
    let f = fixture();
    let mut g = game(&f);
    let info = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 0, critical: false, limb: None };
    let r = g.hit(FormId(HIT_REF), 1.0, info);
    assert!(r.died, "{r:?}");
    assert!(r.xp > 0.0, "{r:?}");
    assert_eq!(g.extra[&FormId(HIT_REF)].killer, Some(FormId::PLAYER_REF));
}
