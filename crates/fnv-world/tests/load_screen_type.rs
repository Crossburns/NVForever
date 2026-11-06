//! A loading screen's LSCT (`fnv_world::loadscreen`), on a synthetic plugin built in memory
//! and decoded through the repository schemas. Every value here is made up for the test;
//! the layout is fopdoc's (docs/spec-notes/loading-screens.md §1).

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::loadscreen::{load_screens, TipStyle};

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn text(sig: &[u8; 4], text: &str) -> Subrecord {
    sub(sig, [text.as_bytes(), &[0]].concat())
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [sig.as_slice(), &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [b"GRUP".as_slice(), &((24 + body.len()) as u32).to_le_bytes(), sig, &0i32.to_le_bytes(), &[0; 8], body].concat()
}

/// An LSCT's DATA of type `kind`: the box (11, 22, 33, 44), font 2 in (10, 20, 30) on the
/// right.
fn data(kind: u32) -> Vec<u8> {
    let ints = |v: &[u32]| v.iter().flat_map(|n| n.to_le_bytes()).collect::<Vec<u8>>();
    let floats = |v: &[f32]| v.iter().flat_map(|n| n.to_le_bytes()).collect::<Vec<u8>>();
    let out = [ints(&[kind, 11, 22, 33, 44]), floats(&[0.5]), ints(&[2]), floats(&[10.0, 20.0, 30.0]), ints(&[4]), vec![0; 20], ints(&[5]), floats(&[1.0, 2.0, 3.0]), vec![0; 4], ints(&[1])].concat();
    assert_eq!(out.len(), 88);
    out
}

const TIP: u32 = 0x100;
const OBJECTIVE: u32 = 0x101;
const SHORT: u32 = 0x102;

fn plugin_bytes() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let tes4 = record(b"TES4", 0, &[sub(b"HEDR", hedr)]);
    let lsct = [
        record(b"LSCT", TIP, &[text(b"EDID", "TestTip"), sub(b"DATA", data(3))]),
        record(b"LSCT", OBJECTIVE, &[text(b"EDID", "TestObjective"), sub(b"DATA", data(2))]),
        record(b"LSCT", SHORT, &[text(b"EDID", "TestShort"), sub(b"DATA", data(3)[..40].to_vec())]),
    ]
    .concat();
    let screen = |fid: u32, wmi1: Option<u32>| {
        let mut subs = vec![text(b"EDID", &format!("Screen{fid:X}")), text(b"ICON", "interface\\loading\\test.dds"), text(b"DESC", "A tip.")];
        subs.extend(wmi1.map(|t| sub(b"WMI1", t.to_le_bytes().to_vec())));
        record(b"LSCR", fid, &subs)
    };
    let lscr = [screen(0x200, Some(TIP)), screen(0x201, Some(OBJECTIVE)), screen(0x202, Some(SHORT)), screen(0x203, None), screen(0x204, Some(0x999))].concat();
    [tes4, top(b"LSCT", &lsct), top(b"LSCR", &lscr)].concat()
}

#[test]
fn a_screen_takes_the_box_font_colour_and_alignment_of_its_tip_type() {
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let screens = load_screens(&lo, &schema);
    let tip_of = |fid: u32| screens.iter().find(|s| s.form_id == fid).expect("screen").tip;
    assert_eq!(screens.len(), 5);
    assert_eq!(tip_of(0x200), Some(TipStyle { x: 11.0, y: 22.0, width: 33.0, height: 44.0, font: 2, colour: [10.0, 20.0, 30.0], alignment: 4 }));
    // Another type of LSCT, one that does not decode, none named and one not in the load
    // order: nothing to read, the engine falls back to `TipStyle::FALLBACK`.
    assert_eq!(tip_of(0x201), None);
    assert_eq!(tip_of(0x202), None);
    assert_eq!(tip_of(0x203), None);
    assert_eq!(tip_of(0x204), None);
}
