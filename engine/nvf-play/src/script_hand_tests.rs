//! The player's activation by hand (`Runtime::by_hand`) on a synthetic plugin: no game data,
//! hand-written bytecode (docs/spec-notes/scripts.md §2). #1615.

use super::*;
use fnv_esm::raw::{write_subrecords, Subrecord};

const P: FormId = FormId::PLAYER_REF;
const GLOBAL: u32 = 0x800;
const BLOCK_SCRIPT: u32 = 0x900;
const PLAIN_SCRIPT: u32 = 0x901;
const ITEM_SCRIPT: u32 = 0x902;
const ACTI_PLAIN: u32 = 0x700;
const ACTI_BLOCK: u32 = 0x701;
const ACTI_SCRIPTED: u32 = 0x702;
const DOOR: u32 = 0x710;
const ITEM: FormId = FormId(0x720);
const HERE: FormId = FormId(0x200);
const THERE: FormId = FormId(0x201);
/// No script at all, as `NewVegasRadio`.
const RADIO_REF: FormId = FormId(0x300);
const BLOCK_REF: FormId = FormId(0x301);
/// A script, but no OnActivate block.
const PLAIN_REF: FormId = FormId(0x302);
const ITEM_REF: FormId = FormId(0x303);
const PARENT_ONLY_REF: FormId = FormId(0x304);
/// Flagged No Alarm, into `THERE`.
const DOOR_REF: FormId = FormId(0x305);
const FAR_DOOR_REF: FormId = FormId(0x306);

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn le16(v: u16) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)])
}

/// A statement: opcode, length, body.
fn st(op: u16, body: &[u8]) -> Vec<u8> {
    [le16(op), le16(body.len() as u16), body.to_vec()].concat()
}

/// An object script with an empty block for each event, and no variables.
fn scpt(fid: u32, edid: &str, events: &[u16]) -> Vec<u8> {
    let mut scda = st(0x1D, &[]);
    for event in events {
        let end = st(0x11, &[]);
        scda.extend(st(0x10, &[le16(*event), (end.len() as u32).to_le_bytes().to_vec()].concat()));
        scda.extend(end);
    }
    let mut schr = vec![0u8; 4];
    schr.extend(0u32.to_le_bytes());
    schr.extend((scda.len() as u32).to_le_bytes());
    schr.extend(0u32.to_le_bytes());
    schr.extend(0u16.to_le_bytes());
    schr.extend(1u16.to_le_bytes());
    record(b"SCPT", 0, fid, &[sub(b"EDID", z(edid)), sub(b"SCHR", schr), sub(b"SCDA", scda)])
}

fn acti(fid: u32, edid: &str, script: Option<u32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"MODL", z("Hand\\Test.nif"))];
    subs.extend(script.map(|s| sub(b"SCRI", s.to_le_bytes().to_vec())));
    record(b"ACTI", 0, fid, &subs)
}

fn placed(id: FormId, base: u32, extra: Vec<Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])];
    subs.extend(extra);
    record(b"REFR", 0, id.0, &subs)
}

/// XTEL: destination door, position and rotation, No Alarm (u32) [documented: xEdit REFR
/// XTEL].
fn xtel(to: FormId, no_alarm: u32) -> Subrecord {
    sub(b"XTEL", [to.0.to_le_bytes().to_vec(), vec![0; 24], no_alarm.to_le_bytes().to_vec()].concat())
}

/// An interior cell with its references.
fn cell(id: FormId, edid: &str, refs: &[u8]) -> Vec<u8> {
    let c = record(b"CELL", 0, id.0, &[sub(b"EDID", z(edid)), sub(b"DATA", vec![0x01])]);
    [c, group(id.0.to_le_bytes(), 6, &group(id.0.to_le_bytes(), 9, refs))].concat()
}

fn plugin() -> fnv_esm::Plugin {
    let scripts = [scpt(BLOCK_SCRIPT, "BlockScript", &[event::ON_ACTIVATE]), scpt(PLAIN_SCRIPT, "PlainScript", &[event::ON_ADD]), scpt(ITEM_SCRIPT, "ItemScript", &[event::ON_ADD])].concat();
    let actis = [acti(ACTI_PLAIN, "PlainActivator", None), acti(ACTI_BLOCK, "BlockActivator", Some(BLOCK_SCRIPT)), acti(ACTI_SCRIPTED, "ScriptedActivator", Some(PLAIN_SCRIPT))].concat();
    let misc = record(
        b"MISC",
        0,
        ITEM.0,
        &[sub(b"EDID", z("Trinket")), sub(b"SCRI", ITEM_SCRIPT.to_le_bytes().to_vec()), sub(b"DATA", [5i32.to_le_bytes(), 1f32.to_le_bytes()].concat())],
    );
    let door = record(b"DOOR", 0, DOOR, &[sub(b"EDID", z("HandDoor"))]);
    let player = record(b"NPC_", 0, FormId::PLAYER_BASE.0, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let glob = record(b"GLOB", 0, GLOBAL, &[sub(b"EDID", z("TestGlobal")), sub(b"FNAM", vec![b's']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let here = [
        placed(RADIO_REF, ACTI_PLAIN, vec![]),
        placed(BLOCK_REF, ACTI_BLOCK, vec![]),
        placed(PLAIN_REF, ACTI_SCRIPTED, vec![]),
        placed(ITEM_REF, ITEM.0, vec![]),
        placed(PARENT_ONLY_REF, ACTI_PLAIN, vec![sub(b"XAPD", vec![1])]),
        placed(DOOR_REF, DOOR, vec![xtel(FAR_DOOR_REF, 1)]),
    ]
    .concat();
    let there = placed(FAR_DOOR_REF, DOOR, vec![xtel(DOOR_REF, 0)]);
    let cells = [cell(HERE, "Here", &here), cell(THERE, "There", &there)].concat();
    let bytes = [
        tes4(),
        top(b"GLOB", &glob),
        top(b"SCPT", &scripts),
        top(b"ACTI", &actis),
        top(b"MISC", &misc),
        top(b"DOOR", &door),
        top(b"NPC_", &player),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat();
    fnv_esm::Plugin::parse(&bytes, "Hand.esm".into()).unwrap()
}

/// The runtime over the fixture, the player in `HERE`.
fn runtime() -> ScriptState {
    let mut state = ScriptState::over_plugins(&[plugin()]);
    let rt = state.rt.as_mut().unwrap();
    rt.game.player_entered(Some(HERE));
    rt.player_cell = Some(HERE);
    state
}

/// The indices of the activations of `r` by the player so far.
fn activated_at(rt: &Runtime, r: FormId) -> Vec<usize> {
    let of = |e: &fnv_game::trace::Event| matches!(e.kind, EventKind::Activated { reference, by, .. } if reference == r && by == P);
    rt.game.events.iter().enumerate().filter(|(_, e)| of(e)).map(|(i, _)| i).collect()
}

/// How many times a block of the script's has run.
fn block_runs(rt: &Runtime, block: &str, script: u32) -> usize {
    let key = rt.game.data.script(FormId(script)).unwrap().key();
    rt.game.events.iter().filter(|e| matches!(&e.kind, EventKind::BlockRun { unit, block: b, .. } if *unit == key && b == block)).count()
}

/// The handler counts in the fixture's global (the syntax is checked in fnv-game's
/// `the_extender_hears_the_players_own_activation_once`).
fn listen_for_activations(rt: &mut Runtime) {
    let handler = "SetEventHandler \"OnActivate\" (begin function {ref rTarget, ref rActor}\nset TestGlobal to TestGlobal + 1\nend)";
    rt.game.run_source(handler, Default::default()).unwrap();
}

fn heard(rt: &Runtime) -> f32 {
    rt.game.world.global(FormId(GLOBAL)).unwrap()
}

/// The issue: an unscripted receiver switches on and off by the player's hand, and the
/// activation's event stays out of the frame's effects (`activate.rs` applies the outcome).
#[test]
fn the_hand_switches_an_unscripted_receiver() {
    let mut state = runtime();
    let rt = state.rt.as_mut().unwrap();
    let done = rt.by_hand(RADIO_REF).unwrap();
    assert!(matches!(done, ByHand::Default(a) if a.outcome == Outcome::Activated && a.script.is_none()));
    assert_eq!((rt.game.radio_switches(RADIO_REF), rt.game.radio_on(RADIO_REF)), (1, true));
    assert_eq!(rt.game.activating(P), Some(RADIO_REF));
    assert!(matches!(rt.by_hand(RADIO_REF).unwrap(), ByHand::Default(_)));
    assert_eq!((rt.game.radio_switches(RADIO_REF), rt.game.radio_on(RADIO_REF)), (2, false));
    let at = activated_at(rt, RADIO_REF);
    assert_eq!(at.len(), 2, "one event per activation");
    assert!(at.iter().all(|i| rt.skip.contains(i)));
}

/// An OnActivate block runs instead of the default action: no switch, no `Activated`.
#[test]
fn a_block_stands_in_for_the_default_action() {
    let mut state = runtime();
    let rt = state.rt.as_mut().unwrap();
    assert!(matches!(rt.by_hand(BLOCK_REF).unwrap(), ByHand::Script));
    assert_eq!(block_runs(rt, "OnActivate", BLOCK_SCRIPT), 1);
    assert_eq!(rt.game.radio_switches(BLOCK_REF), 0);
    assert!(activated_at(rt, BLOCK_REF).is_empty());
}

/// A script without OnActivate does not run on activation: the outcome names none, and the
/// default action is all there is.
#[test]
fn a_script_without_the_block_gets_the_default_action() {
    let mut state = runtime();
    let rt = state.rt.as_mut().unwrap();
    let done = rt.by_hand(PLAIN_REF).unwrap();
    assert!(matches!(done, ByHand::Default(a) if a.outcome == Outcome::Activated && a.script.is_none()));
    assert_eq!(rt.game.radio_switches(PLAIN_REF), 1);
}

/// The extender's OnActivate hears each activation once, with or without a block.
#[test]
fn the_extender_hears_the_hand_once() {
    let mut state = runtime();
    let rt = state.rt.as_mut().unwrap();
    listen_for_activations(rt);
    rt.by_hand(RADIO_REF).unwrap();
    assert_eq!(heard(rt), 1.0);
    rt.by_hand(BLOCK_REF).unwrap();
    assert_eq!(heard(rt), 2.0);
}

/// Through a door: the door counts as the one arrived through at once, and the new cell
/// loads only when the engine has the player there (`Runtime::follow_player`).
#[test]
fn a_door_leaves_entering_the_cell_to_the_engine() {
    let mut state = runtime();
    let rt = state.rt.as_mut().unwrap();
    let done = rt.by_hand(DOOR_REF).unwrap();
    assert!(matches!(done, ByHand::Default(a) if matches!(a.outcome, Outcome::Teleported { cell: Some(c), .. } if c == THERE)));
    assert_eq!(rt.game.crimes.no_alarm_cell, Some(THERE), "the player's own door is arrived through");
    assert!(!rt.game.loaded_cells.contains(&THERE), "not until the engine has moved the player");
    let at = activated_at(rt, DOOR_REF);
    assert_eq!(at.len(), 1);
    assert!(rt.skip.contains(&at[0]), "activate.rs asks for the travel; the event must not ask again");
    rt.follow_player();
    assert!(rt.game.loaded_cells.contains(&THERE));
}

/// A pickup: one item, one `ItemAdded`, one OnAdd, and the inventory's next look sees
/// nothing new to tell.
#[test]
fn a_pickup_adds_once() {
    let mut state = runtime();
    let rt = state.rt.as_mut().unwrap();
    let done = rt.by_hand(ITEM_REF).unwrap();
    assert!(matches!(done, ByHand::Default(a) if matches!(a.outcome, Outcome::PickedUp { item, .. } if item == ITEM)));
    assert_eq!(rt.game.world.peek(P).and_then(|s| s.inventory.as_ref()).map(|i| i.count(ITEM)), Some(1));
    let added: Vec<usize> = rt.game.events.iter().enumerate().filter(|(_, e)| matches!(e.kind, EventKind::ItemAdded { item, .. } if item == ITEM)).map(|(i, _)| i).collect();
    assert_eq!(added.len(), 1);
    assert!(rt.skip.contains(&added[0]));
    assert_eq!(block_runs(rt, "OnAdd", ITEM_SCRIPT), 1);
    rt.observe_inventory();
    assert_eq!(block_runs(rt, "OnAdd", ITEM_SCRIPT), 1);
}

/// "Parent Activate Only" stays a gate of the hand path: no default action, no switch.
#[test]
fn a_parent_activate_only_reference_refuses_the_hand() {
    let mut state = runtime();
    let rt = state.rt.as_mut().unwrap();
    assert!(matches!(rt.by_hand(PARENT_ONLY_REF).unwrap(), ByHand::Refused));
    assert!(activated_at(rt, PARENT_ONLY_REF).is_empty());
    assert_eq!(rt.game.radio_switches(PARENT_ONLY_REF), 0);
}

/// A test aid's activation has nobody else to apply the outcome: its events become effects.
#[test]
fn a_test_aids_activation_keeps_its_events() {
    let mut state = runtime();
    let rt = state.rt.as_mut().unwrap();
    assert!(matches!(rt.activate_as_player(RADIO_REF).unwrap(), ByHand::Default(_)));
    assert_eq!(rt.game.radio_switches(RADIO_REF), 1);
    let at = activated_at(rt, RADIO_REF);
    assert_eq!(at.len(), 1);
    assert!(!rt.skip.contains(&at[0]));
}
