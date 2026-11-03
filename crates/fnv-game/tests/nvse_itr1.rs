//! ITR NVSE batch 1 (`fnv_game::nvse_itr1`, docs/spec-notes/ext-itr.md) through compiled
//! scripts, on a synthetic plugin (no game data, made-up records): fall damage,
//! crouching, the actor switches, alert, the cameras, keys, race eyes, hair colour,
//! creature combat skill, casino bans, challenges, worldspace offsets, info flags, the
//! radio queue, tile watches, item mods and the pure string functions.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
use serde_json::Value as Json;

const RACE: u32 = 0x810;
const EYES_OK: u32 = 0x811;
const EYES_NOT: u32 = 0x812;
const NPC: u32 = 0x820;
const CREA: u32 = 0x821;
const CASINO: u32 = 0x830;
const CHAL: u32 = 0x831;
const WEAP: u32 = 0x832;
const TOPIC: u32 = 0x840;
const INFO: u32 = 0x841;
const WRLD: u32 = 0x850;
const CELL: u32 = 0x200;
const NPC_REF: u32 = 0x300;
const CREA_REF: u32 = 0x301;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

fn plugin_bytes() -> Vec<u8> {
    let race = record(b"RACE", 0, RACE, &[sub(b"EDID", z("TestRace")), sub(b"DATA", [vec![0; 16], vec![0; 16], u32s(&[0])].concat()), sub(b"ENAM", u32s(&[EYES_NOT]))]);
    let eyes = [
        record(b"EYES", 0, EYES_OK, &[sub(b"EDID", z("EyeOk")), sub(b"DATA", vec![1])]),
        record(b"EYES", 0, EYES_NOT, &[sub(b"EDID", z("EyeNotPlayable")), sub(b"DATA", vec![0])]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, NPC, &[sub(b"EDID", z("TestNpc")), sub(b"RNAM", RACE.to_le_bytes().to_vec()), sub(b"HCLR", vec![10, 20, 30, 0])]),
    ]
    .concat();
    let crea = record(b"CREA", 0, CREA, &[sub(b"EDID", z("TestBeast")), sub(b"DATA", [vec![0u8, 40, 0, 0], vec![0; 6], vec![0; 7]].concat())]);
    // CSNO DATA: shuffle share, payout, 7 reel stops, decks, max winnings 1000, chip,
    // quest, flags.
    let csno = record(b"CSNO", 0, CASINO, &[sub(b"EDID", z("TestCasino")), sub(b"DATA", [f32b(0.5), f32b(1.5), vec![0; 28], u32s(&[1, 1000, 0, 0, 0])].concat())]);
    // CHAL DATA: type, threshold 3, flags 2 (recurring), interval.
    let chal = record(b"CHAL", 0, CHAL, &[sub(b"EDID", z("TestChal")), sub(b"DATA", [u32s(&[0, 3, 2, 0]), vec![0; 8]].concat())]);
    let weap_data = [10i32.to_le_bytes().to_vec(), 100i32.to_le_bytes().to_vec(), f32b(2.0), 5i16.to_le_bytes().to_vec(), vec![6]].concat();
    let weap = record(b"WEAP", 0, WEAP, &[sub(b"EDID", z("Gun")), sub(b"DATA", weap_data)]);
    let trdt = [u32s(&[0, 50]), vec![0; 4], vec![1, 0, 0, 0], u32s(&[0]), vec![0; 4]].concat();
    let info = record(b"INFO", 0, INFO, &[sub(b"EDID", z("TestInfo")), sub(b"DATA", vec![0, 0, 0x05, 0x01]), sub(b"TRDT", trdt), sub(b"NAM1", z("Hello there.")), sub(b"NAM2", z("")), sub(b"NAM3", z(""))]);
    let dial = [record(b"DIAL", 0, TOPIC, &[sub(b"EDID", z("TestTopic")), sub(b"DATA", vec![0, 0])]), group(TOPIC.to_le_bytes(), 7, &info)].concat();
    let wrld = record(b"WRLD", 0, WRLD, &[sub(b"EDID", z("TestWorld")), sub(b"ONAM", [f32b(1.5), f32b(-4.0), f32b(7.0)].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell")), sub(b"DATA", vec![1])]);
    let refs = [placed(b"ACHR", NPC_REF, "TestNpcRef", NPC), placed(b"ACRE", CREA_REF, "TestBeastRef", CREA)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"EYES", &eyes),
        top(b"RACE", &race),
        top(b"NPC_", &npcs),
        top(b"CREA", &crea),
        top(b"WEAP", &weap),
        top(b"CSNO", &csno),
        top(b"CHAL", &chal),
        top(b"DIAL", &dial),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
        top(b"WRLD", &wrld),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Itr.esm".into()).unwrap()];
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
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(CELL)));
    let src = MemSource::default().with("menus\\test.xml", r#"<menu name="TestMenu"> <image name="Pic"> <alpha> 10 </alpha> <string> abc </string> </image> <rect name="Box"> <x> 4 </x> </rect> </menu>"#);
    g.ui_tree.load_menu(&src, "menus\\test.xml").unwrap();
    g.ui_tree.update();
    g
}

fn query(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), Default::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn num(g: &mut Game, expr: &str) -> f64 {
    match query(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        v => panic!("{expr}: {v:?}"),
    }
}

fn text(g: &mut Game, expr: &str) -> String {
    match query(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Str(s) => s,
        v => panic!("{expr}: {v:?}"),
    }
}

/// A form's editor ID, as scripts name it.
fn r(id: u32) -> String {
    match id {
        RACE => "TestRace",
        EYES_OK => "EyeOk",
        EYES_NOT => "EyeNotPlayable",
        NPC => "TestNpc",
        CREA => "TestBeast",
        CASINO => "TestCasino",
        CHAL => "TestChal",
        WEAP => "Gun",
        INFO => "TestInfo",
        WRLD => "TestWorld",
        NPC_REF => "TestNpcRef",
        CREA_REF => "TestBeastRef",
        _ => panic!("no editor ID for {id:X}"),
    }
    .to_string()
}

#[test]
fn the_string_functions_join_reverse_and_trim() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(text(&mut g, "Sv_Join (Ar_List \"a\" 2 \"c\") \", \""), "a, 2, c");
    assert_eq!(text(&mut g, "Sv_Join (Ar_List \"a\" \"b\")"), "ab");
    assert_eq!(text(&mut g, "Sv_Reverse \"abc\""), "cba");
    assert_eq!(text(&mut g, "Sv_TrimStr \"  x y \""), "x y");
}

#[test]
fn fall_damage_multipliers_global_and_per_actor() {
    let f = fixture();
    let mut g = game(&f);
    let npc = r(NPC_REF);
    assert_eq!(num(&mut g, "GetFallDamageMult"), 1.0);
    query(&mut g, "SetFallDamageMult 2");
    query(&mut g, &format!("SetFallDamageMult 0.5 {npc}"));
    assert_eq!(num(&mut g, "GetFallDamageMult"), 2.0);
    assert_eq!(num(&mut g, &format!("GetFallDamageMult {npc}")), 0.5);
    assert_eq!(g.fall_damage_mult(FormId(NPC_REF)), 0.5);
    assert_eq!(g.fall_damage_mult(FormId(CREA_REF)), 2.0);
    // Not a finite number: nothing changes; past f32's range: f32's largest.
    assert_eq!(num(&mut g, "SetFallDamageMult (10 ^ 400)"), 0.0);
    assert_eq!(num(&mut g, "SetFallDamageMult ((-1) ^ 0.5)"), 0.0);
    assert_eq!(num(&mut g, "GetFallDamageMult"), 2.0);
    query(&mut g, &format!("SetFallDamageMult (10 ^ 39) {npc}"));
    assert_eq!(g.fall_damage_mult(FormId(NPC_REF)), f64::from(f32::MAX));
    query(&mut g, &format!("SetFallDamageMult 0.5 {npc}"));
    // Negative: 0; an actor's 1 clears its override.
    query(&mut g, "SetFallDamageMult -3");
    assert_eq!(num(&mut g, "GetFallDamageMult"), 0.0);
    query(&mut g, &format!("SetFallDamageMult 1 {npc}"));
    assert_eq!(num(&mut g, &format!("GetFallDamageMult {npc}")), 0.0);
    query(&mut g, &format!("SetFallDamageMult 3 {npc}"));
    query(&mut g, &format!("ClearFallDamageMult {npc}"));
    assert_eq!(num(&mut g, &format!("GetFallDamageMult {npc}")), 0.0);
    // No actor: everything back to 1, and it outlasts a load (the session).
    query(&mut g, &format!("SetFallDamageMult 3 {npc}"));
    query(&mut g, "ClearFallDamageMult");
    assert_eq!(num(&mut g, &format!("GetFallDamageMult {npc}")), 1.0);
    query(&mut g, "SetFallDamageMult 4");
    let s = g.take_session();
    let mut g2 = game(&f);
    g2.put_session(s);
    assert_eq!(num(&mut g2, "GetFallDamageMult"), 4.0);
}

#[test]
fn crouching_alert_and_the_actor_switches() {
    let f = fixture();
    let mut g = game(&f);
    let (npc, beast) = (r(NPC_REF), r(CREA_REF));
    g.extra.entry(FormId(NPC_REF)).or_default().force_sneak = true;
    assert!(g.sneaks(FormId(NPC_REF)));
    assert_eq!(num(&mut g, &format!("{npc}.DisableCrouching 1")), 1.0);
    assert!(!g.sneaks(FormId(NPC_REF)));
    query(&mut g, &format!("{npc}.ForceCrouch 1"));
    assert!(g.sneaks(FormId(NPC_REF)));
    query(&mut g, &format!("{beast}.ForceCrouch 1"));
    assert!(g.sneaks(FormId(CREA_REF)));
    query(&mut g, &format!("{beast}.ForceCrouch 0"));
    assert!(!g.sneaks(FormId(CREA_REF)));
    for (set, get) in [("SetNoWeaponSearch", "GetNoWeaponSearch"), ("SetPreventWeaponSwitch", "GetPreventWeaponSwitch"), ("SetOnContactWatch", "GetOnContactWatch")] {
        assert_eq!(num(&mut g, &format!("{npc}.{get}")), 0.0);
        assert_eq!(num(&mut g, &format!("{npc}.{set} 1")), 1.0);
        assert_eq!(num(&mut g, &format!("{npc}.{get}")), 1.0, "{get}");
        query(&mut g, &format!("{npc}.{set} 0"));
        assert_eq!(num(&mut g, &format!("{npc}.{get}")), 0.0);
    }
    // Alert: untracked 0, no timer -1, a timer counts down and ends.
    assert_eq!(num(&mut g, &format!("{npc}.GetAlertNS")), 0.0);
    query(&mut g, &format!("{npc}.SetAlertNS 1"));
    assert_eq!(num(&mut g, &format!("{npc}.GetAlertNS")), -1.0);
    assert_eq!(num(&mut g, &format!("{npc}.GetIsAlerted")), 1.0);
    query(&mut g, &format!("{npc}.SetAlertNS 1 10"));
    assert_eq!(num(&mut g, &format!("{npc}.GetAlertNS")), 10.0);
    g.time += 4.0;
    assert_eq!(num(&mut g, &format!("{npc}.GetAlertNS")), 6.0);
    g.time += 7.0;
    g.tick();
    assert_eq!(num(&mut g, &format!("{npc}.GetAlertNS")), 0.0);
    assert_eq!(num(&mut g, &format!("{npc}.GetIsAlerted")), 0.0);
}

#[test]
fn camera_settings_keys_and_eyes() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(num(&mut g, "GetAimZoomFirstPersonOnly"), 0.0);
    assert_eq!(num(&mut g, "SetAimZoomFirstPersonOnly 1"), 1.0);
    assert_eq!(num(&mut g, "GetAimZoomFirstPersonOnly"), 1.0);
    query(&mut g, "SetCameraAngle 1 10 20 90");
    assert_eq!(g.other.itr.camera_angle, Some([10.0, 20.0, 90.0]));
    // Not a finite number: nothing changes.
    query(&mut g, "SetCameraAngle 1 ((-1) ^ 0.5) 0 0
SetCameraAngle 1 0 (10 ^ 400) 0");
    assert_eq!(g.other.itr.camera_angle, Some([10.0, 20.0, 90.0]));
    query(&mut g, "SetCameraAngle 0 0 0 0");
    assert_eq!(g.other.itr.camera_angle, None);
    assert_eq!(num(&mut g, "SetDialogueCameraMode 4"), 0.0);
    assert_eq!(num(&mut g, "SetDialogueCameraMode 2"), 1.0);
    assert_eq!(num(&mut g, "SetDialogueCameraFixedAngle 3"), 3.0);
    assert_eq!(num(&mut g, "SetDialogueCameraFixedAngle 12"), 0.0);
    assert_eq!(num(&mut g, "SetDialogueCameraAngle 5"), 5.0);
    for a in 0..12 {
        if a != 7 {
            query(&mut g, &format!("SetDialogueCameraAngleEnabled {a} 0"));
        }
    }
    assert_eq!(num(&mut g, "SetDialogueCameraAngle -1"), 7.0);
    query(&mut g, "SetDialogueCameraShake 0");
    query(&mut g, "SetDialogueCameraShake -1");
    query(&mut g, "SetDialogueCameraDolly 0.5 2");
    assert_eq!(num(&mut g, "SetDialogueCameraShake (10 ^ 400)"), 0.0);
    assert_eq!(num(&mut g, "SetDialogueCameraDolly (10 ^ 400) 2"), 0.0);
    assert_eq!(num(&mut g, "SetDialogueCameraDolly 0.5 ((-1) ^ 0.5)"), 0.0);
    let cam = &g.other.itr.dialogue_camera;
    assert_eq!((cam.mode, cam.fixed_angle, cam.shake, cam.dolly), (2, 3, 3.0, (0.5, 2.0, true)));
    // Keys: a mask group holds the key until it is released.
    query(&mut g, "DisableKeyEx 34 1");
    query(&mut g, "DisableKeyEx 34 2");
    assert_eq!(num(&mut g, "IsKeyDisabled 34"), 1.0);
    query(&mut g, "EnableKeyEx 34 1");
    assert_eq!(num(&mut g, "IsKeyDisabled 34"), 1.0);
    query(&mut g, "EnableKeyEx 34 2");
    assert_eq!(num(&mut g, "IsKeyDisabled 34"), 0.0);
    // EnableKey frees the key and its groups: a group added later is the only one left.
    query(&mut g, "DisableKeyEx 35 1
EnableKey 35
DisableKeyEx 35 2
EnableKeyEx 35 2");
    assert_eq!(num(&mut g, "IsKeyDisabled 35"), 0.0);
    assert_eq!(num(&mut g, "DisableKeyEx -5"), 0.0);
    // A code past the keys and mouse buttons is taken [observed: itr probes].
    assert_eq!(num(&mut g, "DisableKeyEx 300"), 1.0);
    assert_eq!(num(&mut g, "EnableKeyEx 300"), 1.0);
    // Race eyes: only playable eyes join.
    let (race, ok, not) = (r(RACE), r(EYES_OK), r(EYES_NOT));
    assert_eq!(num(&mut g, &format!("AddRaceEye {race} {not}")), 0.0);
    assert_eq!(num(&mut g, &format!("AddRaceEye {race} {ok}")), 1.0);
    assert_eq!(num(&mut g, &format!("Ar_Size (GetRaceEyes {race})")), 2.0);
    assert_eq!(num(&mut g, &format!("RemoveRaceEye {race} {not}")), 1.0);
    assert_eq!(num(&mut g, &format!("Ar_Size (GetRaceEyes {race})")), 1.0);
    query(&mut g, &format!("ClearRaceEyes {race}"));
    assert_eq!(num(&mut g, &format!("Ar_Size (GetRaceEyes {race})")), 0.0);
    query(&mut g, &format!("SetEyeMesh {ok} \"Characters\\Eye.nif\""));
    assert_eq!(g.other.itr.eye_meshes.get(&FormId(EYES_OK)).cloned(), Some(("Characters\\Eye.nif".to_string(), "Characters\\Eye.nif".to_string())));
    assert_eq!(num(&mut g, &format!("ClearEyeMesh {ok}")), 1.0);
    // Set or not, 1 for an eyes form [observed: itr probes].
    assert_eq!(num(&mut g, &format!("ClearEyeMesh {ok}")), 1.0);
}

#[test]
fn records_hair_combat_skill_casino_challenge_worldspace_and_info_flags() {
    let f = fixture();
    let mut g = game(&f);
    let (npc, beast) = (r(NPC_REF), r(CREA_REF));
    assert_eq!(num(&mut g, &format!("{npc}.GetHairColorAlt")), 10.0 + 20.0 * 256.0 + 30.0 * 65536.0);
    assert_eq!(num(&mut g, &format!("{npc}.GetHairColorAlt 2")), 20.0);
    assert_eq!(num(&mut g, &format!("{npc}.SetHairColorAlt 200 300 40")), 1.0);
    assert_eq!(num(&mut g, &format!("GetHairColorAlt 1 {npc}")), 200.0);
    assert_eq!(num(&mut g, &format!("{npc}.GetHairColorAlt 2")), f64::from(300u32 & 0xFF));
    assert_eq!(num(&mut g, &format!("{beast}.GetHairColorAlt")), 0.0);
    assert_eq!(num(&mut g, &format!("GetCreatureCombatSkill {}", r(CREA))), 40.0);
    assert_eq!(num(&mut g, &format!("{beast}.SetCreatureCombatSkill 300")), 1.0);
    assert_eq!(num(&mut g, &format!("GetCreatureCombatSkill {}", r(CREA))), 255.0);
    assert_eq!(num(&mut g, &format!("SetCreatureCombatSkill 75 {}", r(NPC))), 0.0);
    // Not a number: 0, never NaN.
    num(&mut g, &format!("{beast}.SetCreatureCombatSkill ((-1) ^ 0.5)"));
    assert_eq!(num(&mut g, &format!("GetCreatureCombatSkill {}", r(CREA))), 0.0);
    // Casino: never played 0; banned at the max winnings; lifted to one below it.
    let c = r(CASINO);
    assert_eq!(num(&mut g, &format!("GetCasinoBan {c}")), 0.0);
    assert_eq!(num(&mut g, &format!("SetCasinoBan {c} 1")), 1.0);
    assert_eq!(num(&mut g, &format!("GetCasinoBan {c}")), 1.0);
    assert_eq!(g.player.casino_earnings.get(&FormId(CASINO)), Some(&1000));
    query(&mut g, &format!("SetCasinoBan {c} 0"));
    assert_eq!(num(&mut g, &format!("GetCasinoBan {c}")), 0.0);
    assert_eq!((g.player.casino_earnings.get(&FormId(CASINO)), g.player.casino_levels.get(&FormId(CASINO))), (Some(&999), Some(&3)));
    assert_eq!(num(&mut g, &format!("SetCasinoBan {npc} 1")), 0.0);
    // A recurring challenge of 3: 7 steps complete it twice and leave 1.
    let ch = FormId(CHAL);
    assert_eq!(num(&mut g, &format!("ModChallenge {} 7", r(CHAL))), 1.0);
    let s = g.player.challenges.get(&ch).cloned().unwrap();
    assert_eq!((s.times, s.progress), (2, 1));
    query(&mut g, &format!("ModChallenge {} -5", r(CHAL)));
    assert_eq!(g.player.challenges.get(&ch).unwrap().progress, 0);
    // Worldspace map offsets.
    let w = r(WRLD);
    assert_eq!(num(&mut g, &format!("GetWorldspaceOffsetScale {w}")), 1.5);
    assert_eq!(num(&mut g, &format!("GetWorldspaceOffsetX {w}")), -4.0);
    assert_eq!(num(&mut g, &format!("GetWorldspaceOffsetY {w}")), 7.0);
    assert_eq!(num(&mut g, &format!("GetWorldspaceOffsetY {npc}")), 0.0);
    // Info flags: Goodbye and Say Once in the first byte, Say Once A Day in the second.
    let i = r(INFO);
    assert_eq!(num(&mut g, &format!("GetDialogueInfoFlags {i}")), f64::from(0x0105));
    assert_eq!(num(&mut g, &format!("SetDialogueInfoFlags {i} 4")), 1.0);
    assert_eq!(num(&mut g, &format!("GetDialogueInfoFlags {i}")), 4.0);
    assert_eq!(num(&mut g, &format!("SetDialogueInfoFlags {npc} 4")), 0.0);
}

#[test]
fn the_radio_and_its_queue() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(num(&mut g, "IsRadioPlaying"), 0.0);
    assert_eq!(num(&mut g, "ChangeRadioTrack"), 0.0);
    // A station's line plays: its info, its first response.
    g.radio_line_begins(FormId(NPC_REF), FormId(INFO));
    assert_eq!(num(&mut g, "IsRadioPlaying"), 1.0);
    assert!(matches!(query(&mut g, "SetFunctionValue GetPlayingRadioTrack"), XVal::Ref(x) if x == INFO));
    assert_eq!(text(&mut g, "GetPlayingRadioText"), "Hello there.");
    assert_eq!(num(&mut g, "ChangeRadioTrack"), 1.0);
    g.radio_line_ends(FormId(NPC_REF), FormId(INFO));
    assert_eq!(num(&mut g, "IsRadioPlaying"), 0.0);
    // The queue: relative wav, ogg or mp3 [observed: itr probes], 8 at most.
    assert_eq!(num(&mut g, "QueueRadioTrack \"songs\\a.mp3\""), 1.0);
    assert_eq!(num(&mut g, "QueueRadioTrack \"songs\\a.txt\""), 0.0);
    assert_eq!(num(&mut g, "ClearRadioQueue"), 1.0);
    assert_eq!(num(&mut g, "QueueRadioTrack \"C:\\songs\\a.ogg\""), 0.0);
    for k in 0..8 {
        assert_eq!(num(&mut g, &format!("QueueRadioTrack \"songs\\t{k}.ogg\"")), 1.0);
    }
    assert_eq!(num(&mut g, "QueueRadioTrack \"songs\\t9.ogg\""), 0.0);
    assert_eq!(num(&mut g, "GetRadioQueueSize"), 8.0);
    // No Pip-Boy station on: PlayRadioFile refuses.
    assert_eq!(num(&mut g, "PlayRadioFile \"songs\\now.wav\""), 0.0);
    g.radio.pipboy.on = true;
    g.radio.pipboy.station = Some(FormId(NPC_REF));
    assert_eq!(num(&mut g, "PlayRadioFile \"songs\\now.wav\""), 1.0);
    assert_eq!(g.other.itr.radio_queue.front().map(String::as_str), Some("songs\\now.wav"));
    assert_eq!(g.other.itr.radio_queue.back().map(String::as_str), Some("songs\\t6.ogg"));
    assert_eq!(num(&mut g, "SetRadioQueueLoop 1"), 1.0);
    assert_eq!(num(&mut g, "ClearRadioQueue"), 8.0);
    assert_eq!(num(&mut g, "GetRadioQueueSize"), 0.0);
}

#[test]
fn tile_watches_textures_item_mods_and_the_emissive_colour() {
    let f = fixture();
    let mut g = game(&f);
    let id = num(&mut g, "WatchTileValue \"TestMenu/Pic\" \"alpha\"");
    assert!(id > 0.0);
    assert_eq!(num(&mut g, "WatchTileValue \"TestMenu\\Pic\" \"alpha\""), id);
    // A trait that is not there is watched while its menu is [observed: itr probes];
    // no menu, no watch.
    let nope = num(&mut g, "WatchTileValue \"TestMenu/Nope\" \"alpha\"");
    assert!(nope > id);
    assert_eq!(num(&mut g, "WatchTileValue \"NoSuchMenu/Pic\" \"alpha\""), 0.0);
    // A change fires the event with the id, the path and the new value.
    query(&mut g, "SetEventHandler \"ITR:OnTileValueChange\" (begin function {int i, string_var p, float v}\nSetUIFloat \"TestMenu/Box/x\" v\nend)");
    query(&mut g, "SetUIFloat \"TestMenu/Pic/alpha\" 77");
    g.tick();
    assert_eq!(g.ui_tree.get_float("TestMenu/Box/x"), Some(77.0));
    assert_eq!(num(&mut g, &format!("UnwatchTileValue {id}")), 1.0);
    assert_eq!(num(&mut g, &format!("UnwatchTileValue {id}")), 0.0);
    assert_eq!(num(&mut g, "SetUITexOffset \"TestMenu/Pic\" 1.25 -0.25"), 1.0);
    assert_eq!(g.other.itr.tex_offsets.get("TestMenu/Pic"), Some(&(0.25, 0.75)));
    assert_eq!(num(&mut g, "SetUITexOffset \"TestMenu/Pic\" (10 ^ 400) 0"), 0.0);
    assert_eq!(g.other.itr.tex_offsets.get("TestMenu/Pic"), Some(&(0.25, 0.75)));
    assert_eq!(num(&mut g, "SetUITexOffset \"TestMenu/Box\" 0.5 0"), 0.0);
    assert_eq!(num(&mut g, "SetUIAlphaMap \"TestMenu/Box\" \"Interface\\mask.dds\""), 1.0);
    assert_eq!(num(&mut g, "SetUIAlphaMap \"TestMenu/None\" \"Interface\\mask.dds\""), 0.0);
    // Item mods on an inventory reference.
    let gun = r(WEAP);
    query(&mut g, &format!("player.AddItem {gun} 1"));
    assert_eq!(num(&mut g, &format!("(player.GetFirstRefForItem {gun}).GetItemModFlags")), 0.0);
    query(&mut g, &format!("ref rGun\nlet rGun := player.GetFirstRefForItem {gun}\nrGun.SetItemModFlags 5"));
    assert_eq!(num(&mut g, &format!("(player.GetFirstRefForItem {gun}).GetItemModFlags")), 5.0);
    query(&mut g, &format!("ref rGun\nlet rGun := player.GetFirstRefForItem {gun}\nrGun.SetItemModFlags"));
    assert_eq!(num(&mut g, &format!("(player.GetFirstRefForItem {gun}).GetItemModFlags")), 1.0);
    // The emissive colour needs a weapon in hand.
    assert_eq!(num(&mut g, "SetWeaponEmissiveColor 1 0 0 2"), 0.0);
    assert_eq!(num(&mut g, "ClearWeaponEmissiveColor"), 0.0);
    query(&mut g, &format!("player.EquipItem {gun}"));
    assert_eq!(num(&mut g, "SetWeaponEmissiveColor 1 0 0 ((-1) ^ 0.5)"), 0.0);
    assert_eq!(num(&mut g, "SetWeaponEmissiveColor 1 0 0 2"), 1.0);
    assert_eq!(num(&mut g, "ClearWeaponEmissiveColor"), 1.0);
}
