//! Vendor services (`fnv_game::services`, docs/spec-notes/vendor-services.md) on a plugin
//! built in memory (no game data): the AIDT service flags, the class's with "Autocalc
//! Service", an item's category, and GetOffersServicesNow following the package.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::package::flags;
use fnv_game::services;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x900u32.to_le_bytes()].concat();
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn glob(fid: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

/// A package with these general flags, type and schedule (hour, duration).
fn pack(fid: u32, edid: &str, general: u32, ty: u8, hour: i8, duration: u32) -> Vec<u8> {
    record(
        b"PACK",
        fid,
        &[
            sub(b"EDID", z(edid)),
            sub(b"PKDT", [&general.to_le_bytes()[..], &[ty, 0], &[0; 6]].concat()),
            sub(b"PLDT", [3i32.to_le_bytes(), 0u32.to_le_bytes(), 0i32.to_le_bytes()].concat()),
            sub(b"PSDT", [&[0xFF, 0xFF, 0, hour as u8][..], &duration.to_le_bytes()].concat()),
        ],
    )
}

/// AIDT with these service flags (offset 8).
fn aidt(services: u32) -> Subrecord {
    sub(b"AIDT", [&[0u8; 8][..], &services.to_le_bytes(), &[0u8; 8]].concat())
}

/// ACBS with these flags (offset 0).
fn acbs(flags: u32) -> Subrecord {
    sub(b"ACBS", [&flags.to_le_bytes()[..], &[0u8; 20]].concat())
}

fn npc(fid: u32, edid: &str, extra: Vec<Subrecord>, packages: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    subs.extend(extra);
    subs.extend(packages.iter().map(|p| sub(b"PKID", p.to_le_bytes().to_vec())));
    record(b"NPC_", fid, &subs)
}

fn placed(fid: u32, edid: &str, base: u32) -> Vec<u8> {
    record(b"ACHR", fid, &[sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

/// ALCH with ENIT flags and an ETYP equip type.
fn alch(fid: u32, edid: &str, enit_flags: u8, etyp: i32) -> Vec<u8> {
    let mut enit = 5i32.to_le_bytes().to_vec();
    enit.extend([enit_flags, 0, 0, 0]);
    enit.extend(0u32.to_le_bytes());
    enit.extend(0f32.to_le_bytes());
    enit.extend(0u32.to_le_bytes());
    record(b"ALCH", fid, &[sub(b"EDID", z(edid)), sub(b"ETYP", etyp.to_le_bytes().to_vec()), sub(b"ENIT", enit)])
}

const AUTOCALC_SERVICE: u32 = 1 << 23;
const TRAVEL: u8 = 6;
const SANDBOX: u8 = 12;

const P_SHOP: u32 = 0x820;
const P_HOME: u32 = 0x821;
const CLASS: u32 = 0x830;
const N_BARKEEP: u32 = 0x840;
const N_TRADER: u32 = 0x841;
const N_SPAWN: u32 = 0x842;
const BEER: u32 = 0x850;
const COLA: u32 = 0x851;
const JET: u32 = 0x852;
const STIMPAK: u32 = 0x853;
const PISTOL: u32 = 0x854;
const BULLETS: u32 = 0x855;
const JUNK: u32 = 0x856;
const CELL: u32 = 0x860;
const R_BARKEEP: u32 = 0x870;
const R_TRADER: u32 = 0x871;
const R_SPAWN: u32 = 0x872;
const G_HOUR: u32 = 0x804;

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        glob(0x801, "GameYear", 2281.0),
        glob(0x802, "GameMonth", 9.0),
        glob(0x803, "GameDay", 19.0),
        glob(G_HOUR, "GameHour", 12.0),
        glob(0x805, "GameDaysPassed", 5.0),
        glob(0x806, "TimeScale", 30.0),
    ]
    .concat();
    // The bar is open 8 to 20 (Offers Services); then home.
    let packs = [pack(P_SHOP, "AtBar8x12", flags::OFFERS_SERVICES, TRAVEL, 8, 12), pack(P_HOME, "Home", 0, SANDBOX, -1, 0)].concat();
    // A class selling Weapons and Miscellaneous, after its tag skills and flags.
    let clas = record(b"CLAS", CLASS, &[sub(b"EDID", z("Trader")), sub(b"DATA", [&[0u8; 20][..], &(services::WEAPONS | services::MISC).to_le_bytes(), &[0u8; 4]].concat())]);
    let npcs = [
        npc(0x7, "Player", vec![], &[]),
        npc(N_BARKEEP, "Barkeep", vec![acbs(0), aidt(services::ALCOHOL | services::FOOD)], &[P_SHOP, P_HOME]),
        // Its own AIDT says Armor, but "Autocalc Service" takes the class's.
        npc(N_TRADER, "Trader", vec![acbs(AUTOCALC_SERVICE), aidt(services::ARMOR), sub(b"CNAM", CLASS.to_le_bytes().to_vec())], &[P_HOME]),
        // Template flag "AI Data" (bit 4 of the template flags at ACBS offset 22): the
        // barkeep's services.
        npc(N_SPAWN, "Spawn", vec![sub(b"ACBS", [&[0u8; 22][..], &0x10u16.to_le_bytes()].concat()), aidt(0), sub(b"TPLT", N_BARKEEP.to_le_bytes().to_vec())], &[]),
    ]
    .concat();
    let alchs = [alch(BEER, "Beer", 2, 13), alch(COLA, "NukaCola", 0, 12), alch(JET, "Jet", 0, 10), alch(STIMPAK, "Stimpak", 4, 11)].concat();
    let weap = record(b"WEAP", PISTOL, &[sub(b"EDID", z("Pistol"))]);
    let ammo = record(b"AMMO", BULLETS, &[sub(b"EDID", z("Bullets"))]);
    let misc = record(b"MISC", JUNK, &[sub(b"EDID", z("Junk"))]);
    let refs = [placed(R_BARKEEP, "BarkeepRef", N_BARKEEP), placed(R_TRADER, "TraderRef", N_TRADER), placed(R_SPAWN, "SpawnRef", N_SPAWN)].concat();
    let cell = [record(b"CELL", CELL, &[sub(b"EDID", z("Bar"))]), group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs))].concat();
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"CLAS", &clas),
        top(b"NPC_", &npcs),
        top(b"PACK", &packs),
        top(b"ALCH", &alchs),
        top(b"WEAP", &weap),
        top(b"AMMO", &ammo),
        top(b"MISC", &misc),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Services.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(CELL));
    g.player_entered(Some(FormId(CELL)));
    g
}

const GET_OFFERS_SERVICES_NOW: u16 = 0x10FF;

fn offers(g: &mut Game, r: u32) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: GET_OFFERS_SERVICES_NOW, on: Some(FormId(r)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[]).num()
}

#[test]
fn the_service_flags_come_from_the_ai_data_or_the_class() {
    let f = fixture();
    let g = game(&f);
    assert_eq!(g.services(FormId(R_BARKEEP)), services::ALCOHOL | services::FOOD);
    assert_eq!(g.services(FormId(R_TRADER)), services::WEAPONS | services::MISC, "Autocalc Service: the class's");
    assert_eq!(g.services(FormId(R_SPAWN)), services::ALCOHOL | services::FOOD, "the AI data's template");
    assert_eq!(g.services(FormId::PLAYER_REF), 0);
}

#[test]
fn a_vendor_sells_only_its_categories() {
    let f = fixture();
    let g = game(&f);
    let cat = |i: u32| services::item_service(&f.data, FormId(i));
    assert_eq!(
        [BEER, COLA, JET, STIMPAK, PISTOL, BULLETS, JUNK].map(cat),
        [Some(services::ALCOHOL), Some(services::FOOD), Some(services::CHEMS), Some(services::STIMPAKS), Some(services::WEAPONS), Some(services::WEAPONS), Some(services::MISC)]
    );
    let (bar, trader) = (FormId(R_BARKEEP), FormId(R_TRADER));
    assert!(g.sells(bar, FormId(BEER)) && g.sells(bar, FormId(COLA)));
    assert_eq!(g.refused_category(bar, FormId(JET)), Some(services::CHEMS));
    assert_eq!(g.refused_category(bar, FormId(PISTOL)), Some(services::WEAPONS));
    assert!(g.sells(trader, FormId(PISTOL)) && g.sells(trader, FormId(BULLETS)) && g.sells(trader, FormId(JUNK)));
    assert!(!g.sells(trader, FormId(BEER)));
    // What is no item is not refused.
    assert!(g.sells(bar, FormId(CELL)));
}

#[test]
fn get_offers_services_now_follows_the_package() {
    let f = fixture();
    let mut g = game(&f);
    // Noon: the bar package, Offers Services.
    assert_eq!(offers(&mut g, R_BARKEEP), 1.0);
    // On its way there it does not offer them; at its goal again it does.
    g.set_ai_phase(FormId(R_BARKEEP), Some(fnv_game::ai::AiPhase::Travelling));
    assert_eq!(offers(&mut g, R_BARKEEP), 0.0);
    g.package_goal_met(FormId(R_BARKEEP));
    assert_eq!(offers(&mut g, R_BARKEEP), 1.0);
    // A script package without the flag.
    let ctx = Ctx { unit: "test".into(), opcode: 0x1097, on: Some(FormId(R_BARKEEP)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[ArgValue::Value(Value::Ref(P_HOME))]);
    assert_eq!(offers(&mut g, R_BARKEEP), 0.0);
    // Evening, the list again: home, no services.
    let mut g = game(&f);
    g.world.set_global(FormId(G_HOUR), 21.0).unwrap();
    g.evaluate_package(FormId(R_BARKEEP));
    assert_eq!(offers(&mut g, R_BARKEEP), 0.0);
    // In combat no services either.
    let mut g = game(&f);
    g.start_combat(FormId(R_BARKEEP), FormId::PLAYER_REF);
    assert_eq!(offers(&mut g, R_BARKEEP), 0.0);
    // The trader's only package has no flag; the player never offers.
    assert_eq!(offers(&mut g, R_TRADER), 0.0);
    assert_eq!(offers(&mut g, 0x14), 0.0);
}

#[test]
fn set_service_flag_changes_the_ai_data_not_the_autocalc_class() {
    let f = fixture();
    let mut g = game(&f);
    g.run_query("Begin Function { }
SetServiceFlag Trader 131072 1
SetServiceFlag Barkeep 131072 1
End
", RunCtx::default()).unwrap();
    assert_eq!(g.services(FormId(R_TRADER)), services::WEAPONS | services::MISC, "Autocalc Service keeps the class's");
    assert!(g.sells(FormId(R_TRADER), FormId(PISTOL)));
    assert_eq!(g.services(FormId(R_BARKEEP)), services::ALCOHOL | services::FOOD | services::REPAIR, "the AI data's gain the flag");
}
