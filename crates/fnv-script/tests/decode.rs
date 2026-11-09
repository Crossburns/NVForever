//! Synthetic bytecode: every statement form, parameter encoding and expression token of
//! docs/spec-notes/scripts.md §2, built by hand (no game data).

use fnv_script::decompile::{number, word, Decompiler, NoNames};
use fnv_script::ir::*;
use fnv_script::table::{FunctionTable, ParamEnc, TrailerEnc};
use fnv_script::unit::{Header, RefSlot, ScriptKind, ScriptUnit, UnitOwner, VarDecl, VarType};
use fnv_script::{decode, Decoded};

fn table() -> FunctionTable {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    FunctionTable::load(&p).expect("functions.json loads")
}

// Opcodes from functions.json (observed in shipped bytecode).
const DISABLE: u16 = 0x1022;
const SET_STAGE: u16 = 0x1039;
const GET_STAGE_DONE: u16 = 0x103B;
const GET_DEAD: u16 = 0x102E;
const SET_AV: u16 = 0x100F;
const SET_POS: u16 = 0x1007;
const PLAY_IDLE: u16 = 0x1190;
const SHOW_MESSAGE: u16 = 0x1059;
const SHOW_WARNING: u16 = 0x11B9;
const ADD_ITEM: u16 = 0x1002;
const REMOVE_ITEM: u16 = 0x1052;
const SET_SCALE: u16 = 0x113C;

fn le16(v: u16) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// opcode u16, length u16, body.
fn st(op: u16, body: &[u8]) -> Vec<u8> {
    [le16(op), le16(body.len() as u16), body.to_vec()].concat()
}

/// A call statement on a reference: `0x001C` + u16 index, then the call.
fn on(r: u16, call: Vec<u8>) -> Vec<u8> {
    [le16(0x1C), le16(r), call].concat()
}

/// Parameter block: u16 count + parameters.
fn params(n: u16, bytes: &[u8]) -> Vec<u8> {
    [le16(n), bytes.to_vec()].concat()
}

fn n(v: i32) -> Vec<u8> {
    [vec![b'n'], v.to_le_bytes().to_vec()].concat()
}
fn zf(v: f64) -> Vec<u8> {
    [vec![b'z'], v.to_le_bytes().to_vec()].concat()
}
fn tag(t: u8, i: u16) -> Vec<u8> {
    [vec![t], le16(i)].concat()
}
fn string(s: &str) -> Vec<u8> {
    [le16(s.len() as u16), s.as_bytes().to_vec()].concat()
}

/// Expression: each token preceded by a space.
fn ex(tokens: &[Vec<u8>]) -> Vec<u8> {
    tokens.iter().flat_map(|t| [vec![b' '], t.clone()].concat()).collect()
}
fn ascii(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}
fn xcall(op: u16, blob: &[u8]) -> Vec<u8> {
    [vec![b'X'], le16(op), le16(blob.len() as u16), blob.to_vec()].concat()
}

fn if_(skip: u16, e: &[u8]) -> Vec<u8> {
    st(0x16, &[le16(skip), le16(e.len() as u16), e.to_vec()].concat())
}
fn elseif(skip: u16, e: &[u8]) -> Vec<u8> {
    st(0x18, &[le16(skip), le16(e.len() as u16), e.to_vec()].concat())
}
fn else_(skip: u16) -> Vec<u8> {
    st(0x17, &le16(skip))
}
fn endif() -> Vec<u8> {
    st(0x19, &[])
}
fn set(target: &[u8], e: &[u8]) -> Vec<u8> {
    st(0x15, &[target.to_vec(), le16(e.len() as u16), e.to_vec()].concat())
}
/// Begin with the u32 length of everything up to and including the matching End.
fn block(event: u16, event_params: &[u8], body: &[Vec<u8>]) -> Vec<u8> {
    let inner: Vec<u8> = body.concat();
    let end = st(0x11, &[]);
    let len = (inner.len() + end.len()) as u32;
    [st(0x10, &[le16(event), len.to_le_bytes().to_vec(), event_params.to_vec()].concat()), inner, end].concat()
}

fn unit(vars: &[(u32, &str, VarType)], refs: Vec<RefSlot>, bytes: Vec<u8>) -> ScriptUnit {
    ScriptUnit {
        plugin: "Test.esm".into(),
        owner: UnitOwner::Script { form: 0x0100_0800 },
        kind: ScriptKind::Object,
        header: Header { ref_count: refs.len() as u32, compiled_size: bytes.len() as u32, ..Default::default() },
        variables: vars.iter().map(|(i, n, t)| VarDecl { index: *i, name: n.to_string(), ty: t.clone() }).collect(),
        refs,
        has_scda: true,
        bytes,
        source: None,
    }
}

fn run(t: &FunctionTable, bytes: &[u8]) -> Decoded {
    decode(bytes, t)
}

fn text(t: &FunctionTable, u: &ScriptUnit, d: &Decoded) -> Vec<String> {
    Decompiler { unit: u, table: t, names: &NoNames }.lines(&d.program)
}

#[test]
fn script_with_blocks_and_nesting() {
    let t = table();
    // scn; Begin GameMode; if s1 == 0 / set s1 to 1 / elseif f2 > 1.5 / r1.Disable / else / if (nested) endif / endif; End
    let body = vec![
        if_(1, &ex(&[tag(b's', 1), ascii("0"), ascii("==")])),
        set(&tag(b's', 1), &ex(&[ascii("1")])),
        elseif(1, &ex(&[tag(b'f', 2), ascii("1.50"), ascii(">")])),
        on(1, st(DISABLE, &[])),
        else_(2),
        if_(0, &ex(&[xcall(GET_DEAD, &[])])),
        endif(),
        endif(),
    ];
    let bytes = [st(0x1D, &[]), block(0, &[], &body)].concat();
    let d = run(&t, &bytes);
    assert_eq!(d.report.raw_total(), 0);
    assert!(d.report.issues.is_empty(), "{:?}", d.report.issues);
    assert_eq!(d.report.statements, 11);
    let p = &d.program;
    assert!(p.script_name);
    assert_eq!(p.blocks.len(), 1);
    assert_eq!(p.blocks[0].event, 0);
    assert_eq!(p.blocks[0].params, None);
    let Stmt::If { arms, otherwise } = &p.blocks[0].body[0] else { panic!("not an If") };
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[1].body.len(), 1);
    let Stmt::Call { call } = &arms[1].body[0] else { panic!() };
    assert_eq!((call.on, call.opcode, call.has_params), (Some(RefIndex(1)), DISABLE, false));
    assert_eq!(otherwise.as_ref().map(Vec::len), Some(1));
    let u = unit(
        &[(1, "bDone", VarType::Int), (2, "fTimer", VarType::FloatOrRef)],
        vec![RefSlot::Form(0x0001_2345)],
        bytes,
    );
    assert!(fnv_script::check::check(&u, p).is_empty());
    assert_eq!(
        text(&t, &u, &d),
        [
            "scriptname",
            "begin gamemode",
            "if bdone 0 ==",
            "set bdone to 1",
            "elseif ftimer 1.5 >",
            "0x00012345.disable",
            "else",
            "if getdead()",
            "endif",
            "endif",
            "end"
        ]
    );
}

#[test]
fn skip_counts_and_block_length_are_checked() {
    let t = table();
    // The If says 2 statements to skip, but the EndIf is 1 statement on.
    let body = vec![if_(2, &ex(&[ascii("1")])), st(0x1E, &[]), endif()];
    let mut bytes = block(0, &[], &body);
    let d = run(&t, &bytes);
    assert_eq!(d.report.issues.iter().map(|i| i.kind).collect::<Vec<_>>(), ["skip_mismatch"]);
    // A wrong Begin length.
    bytes[4 + 2] ^= 1;
    let d = run(&t, &bytes);
    assert!(d.report.issues.iter().any(|i| i.kind == "block_length_mismatch"));
}

#[test]
fn stray_markers_are_kept_in_place() {
    let t = table();
    // if / else / elseif (after else: chain continuation) / endif / endif (stray)
    let bytes = [
        if_(0, &ex(&[ascii("1")])),
        else_(0),
        elseif(0, &ex(&[ascii("2")])),
        endif(),
        endif(),
    ]
    .concat();
    let d = run(&t, &bytes);
    let kinds: Vec<_> = d.report.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["chain_after_else", "unmatched_endif"]);
    assert_eq!(d.program.top.len(), 2);
    let Stmt::If { otherwise: Some(o), .. } = &d.program.top[0] else { panic!() };
    assert!(matches!(o[0], Stmt::Unmatched { marker: Marker::ElseIf, skip: Some(0), .. }));
    assert!(matches!(d.program.top[1], Stmt::Unmatched { marker: Marker::EndIf, .. }));
    let u = unit(&[], vec![], bytes);
    assert_eq!(text(&t, &u, &d), ["if 1", "else", "elseif 2", "endif", "endif"]);
}

#[test]
fn parameter_encodings() {
    let t = table();
    assert_eq!(t.param_enc(SET_AV, 0), ParamEnc::Enum("actor_value".into()));
    assert_eq!(t.param_enc(SET_POS, 0), ParamEnc::Axis);
    assert_eq!(t.param_enc(PLAY_IDLE, 0), ParamEnc::String);
    assert_eq!(t.trailer(SHOW_MESSAGE), [TrailerEnc::FormatVars, TrailerEnc::Int32]);
    let calls = [
        st(SET_STAGE, &params(2, &[tag(b'r', 1), n(10)].concat())),
        st(SET_AV, &params(2, &[le16(57), n(0)].concat())), // Assistance 0
        st(SET_POS, &params(2, &[vec![b'Z'], tag(b'f', 2)].concat())),
        st(PLAY_IDLE, &params(1, &string("LooseLookingAround"))),
        st(SET_SCALE, &params(1, &zf(0.5))),
        st(SHOW_MESSAGE, &params(1, &[tag(b'r', 1), le16(2), tag(b's', 1), [tag(b'r', 2), tag(b's', 7)].concat(), 3i32.to_le_bytes().to_vec()].concat())),
        st(SHOW_WARNING, &params(1, &[string("Hi there"), le16(0)].concat())),
        // RemoveItem form, count-variable: `r` then `s` are two parameters.
        st(REMOVE_ITEM, &params(2, &[tag(b'r', 1), tag(b's', 1)].concat())),
        // AddItem form, Quest.var: `r` then `r`+`s` (one remote variable).
        st(ADD_ITEM, &params(2, &[tag(b'r', 1), tag(b'r', 2), tag(b's', 3)].concat())),
        st(ADD_ITEM, &params(2, &[tag(b'r', 1), tag(b'G', 3)].concat())),
    ]
    .concat();
    let d = run(&t, &calls);
    assert_eq!(d.report.raw_total(), 0, "{:?}", d.report.issues);
    let args: Vec<&Vec<Arg>> = d
        .program
        .top
        .iter()
        .map(|s| match s {
            Stmt::Call { call } => &call.args,
            _ => panic!(),
        })
        .collect();
    assert_eq!(args[1], &vec![Arg::Enum(57), Arg::Int(0)]);
    assert_eq!(args[2][0], Arg::Axis('Z'));
    assert_eq!(args[3], &vec![Arg::String("LooseLookingAround".into())]);
    assert_eq!(args[4], &vec![Arg::Float(0.5)]);
    let local = |i, l| Arg::Var(VarRef { on: None, var: VarSlot::Local { index: i, letter: l } });
    assert_eq!(
        args[5],
        &vec![
            Arg::Ref(RefIndex(1)),
            Arg::FormatVars(vec![
                local(1, VarLetter::Short),
                Arg::Var(VarRef { on: Some(RefIndex(2)), var: VarSlot::Local { index: 7, letter: VarLetter::Short } }),
            ]),
            Arg::Int32(3),
        ]
    );
    assert_eq!(args[7], &vec![Arg::Ref(RefIndex(1)), local(1, VarLetter::Short)]);
    assert_eq!(
        args[8][1],
        Arg::Var(VarRef { on: Some(RefIndex(2)), var: VarSlot::Local { index: 3, letter: VarLetter::Short } })
    );
    assert_eq!(args[9][1], Arg::Var(VarRef { on: None, var: VarSlot::Global { global: RefIndex(3) } }));
    let u = unit(
        &[(1, "iCount", VarType::Int), (2, "fZ", VarType::FloatOrRef)],
        vec![RefSlot::Form(0x10), RefSlot::Form(0x20), RefSlot::Form(0x30)],
        calls,
    );
    let lines = text(&t, &u, &d);
    assert_eq!(lines[1], "setactorvalue assistance 0");
    assert_eq!(lines[2], "setpos z fz");
    assert_eq!(lines[3], "playidle looselookingaround");
    assert_eq!(lines[5], "showmessage 0x00000010 icount 0x00000020.s7 3");
    assert_eq!(lines[6], "showwarning \"hi there\"");
    assert_eq!(lines[8], "additem 0x00000010 0x00000020.s3");
    assert_eq!(lines[9], "additem 0x00000010 0x00000030");
}

#[test]
fn expression_tokens() {
    let t = table();
    // set f1 to -(GetStageDone r1 10) + G2 * Z3 ... as postfix tokens
    let e = ex(&[
        xcall(GET_STAGE_DONE, &params(2, &[tag(b'r', 1), n(10)].concat())),
        ascii("~"),
        tag(b'G', 2),
        tag(b'Z', 3),
        ascii("*"),
        ascii("+"),
        [tag(b'r', 1), xcall(GET_DEAD, &[])].concat(),
        [tag(b'r', 1), tag(b'f', 4)].concat(),
        ascii("||"),
        ascii("&&"),
    ]);
    let bytes = set(&tag(b'f', 1), &e);
    let d = run(&t, &bytes);
    assert_eq!(d.report.raw_total(), 0);
    let Stmt::Set { target, expr } = &d.program.top[0] else { panic!() };
    assert_eq!(*target, VarRef { on: None, var: VarSlot::Local { index: 1, letter: VarLetter::Float } });
    assert_eq!(expr.0.len(), 10);
    assert!(matches!(&expr.0[0], Token::Call { call } if call.opcode == GET_STAGE_DONE && call.args.len() == 2));
    assert_eq!(expr.0[1], Token::Op { op: Op::Neg });
    assert_eq!(expr.0[3], Token::Ref { r: RefIndex(3) });
    assert!(matches!(&expr.0[6], Token::Call { call } if call.on == Some(RefIndex(1)) && !call.has_params));
    let u = unit(&[(1, "fOut", VarType::FloatOrRef)], vec![RefSlot::Form(1), RefSlot::Form(2), RefSlot::Form(3)], bytes);
    assert_eq!(
        text(&t, &u, &d),
        ["set fout to getstagedone(0x00000001 10) ~ 0x00000002 0x00000003 * + 0x00000001.getdead() 0x00000001.f4 || &&"]
    );
}

#[test]
fn begin_parameters() {
    let t = table();
    let bytes = [block(0x1A, &params(1, &tag(b'r', 1)), &[]), block(0x01, &params(1, &n(1012)), &[])].concat();
    let d = run(&t, &bytes);
    assert!(d.report.issues.is_empty(), "{:?}", d.report.issues);
    assert_eq!(d.program.blocks[0].params, Some(vec![Arg::Ref(RefIndex(1))]));
    let u = unit(&[], vec![RefSlot::Form(0x14)], bytes);
    assert_eq!(text(&t, &u, &d), ["begin ontriggerenter 0x00000014", "end", "begin menumode 1012", "end"]);
}

#[test]
fn unknown_bytes_are_kept_raw_and_counted() {
    let t = table();
    let bytes = [
        // A parameter that is no tagged value.
        st(SET_STAGE, &params(2, &[tag(b'r', 1), vec![b'?', 1, 2]].concat())),
        // An expression token the decoder does not know.
        if_(0, &[ex(&[ascii("1")]), vec![b' ', b'Q', 9]].concat()),
        endif(),
        // An unknown statement opcode below 0x1000.
        st(0x0042, &[1, 2, 3]),
        // A truncated statement at the end.
        vec![0x15, 0x00, 0x09],
    ]
    .concat();
    let d = run(&t, &bytes);
    assert_eq!((d.report.raw_args, d.report.raw_tokens, d.report.raw_statements), (1, 1, 2));
    let Stmt::Call { call } = &d.program.top[0] else { panic!() };
    assert_eq!(call.args, vec![Arg::Ref(RefIndex(1)), Arg::Raw(vec![b'?', 1, 2])]);
    assert!(matches!(&d.program.top[2], Stmt::Raw { opcode: 0x42, body } if body == &[1, 2, 3]));
    assert!(matches!(&d.program.top[3], Stmt::Raw { opcode: 0xFFFF, body } if body == &[0x15, 0x00, 0x09]));
    // Flattened text keeps every statement, raw ones included.
    let u = unit(&[], vec![RefSlot::Form(1)], bytes);
    assert_eq!(text(&t, &u, &d).len(), 5);
}

#[test]
fn unknown_opcode_decodes_with_tagged_values() {
    let t = table();
    // Past the base game's last command (0x127F GetInCharGen, its command table).
    let bytes = st(0x13F0, &params(1, &n(1)));
    let d = run(&t, &bytes);
    assert_eq!(d.report.raw_total(), 0);
    assert_eq!(d.report.issues[0].kind, "unknown_opcode");
    let u = unit(&[], vec![], bytes);
    assert_eq!(text(&t, &u, &d), ["#0x13f0 1"]);
}

#[test]
fn canonical_atoms() {
    assert_eq!(number(19.5), "19.5");
    assert_eq!(number(3.0), "3");
    assert_eq!(number(-0.0), "0");
    assert_eq!(number(0.002), "0.002");
    assert_eq!(number(1e-7), "0.0000001");
    assert_eq!(word("SpecialIdle"), "specialidle");
    assert_eq!(word("Corpses Eaten"), "\"corpses eaten\"");
    assert_eq!(word("VB01.bik"), "\"vb01.bik\"");
}

#[test]
fn table_lookups() {
    let t = table();
    assert_eq!(t.by_name("setav"), Some(SET_AV));
    assert_eq!(t.by_name("SetActorValue"), Some(SET_AV));
    assert_eq!(t.block_name(0x1A), Some("OnTriggerEnter"));
    assert_eq!(t.enum_name("actor_value", 57), Some("Assistance"));
    assert_eq!(t.enum_name("anim_group", 3), Some("Forward"));
    // Every opcode listed in the encodings exists in the catalogue (checked at load).
    for f in t.functions.values() {
        assert!(f.opcode >= 0x1000);
    }
}

#[test]
fn ir_serialises() {
    let t = table();
    let bytes = st(SET_STAGE, &params(2, &[tag(b'r', 1), n(10)].concat()));
    let d = run(&t, &bytes);
    let v = serde_json::to_value(&d.program).unwrap();
    assert_eq!(v["top"][0]["stmt"], "call");
    assert_eq!(v["top"][0]["call"]["args"][1], serde_json::json!({"kind": "int", "value": 10}));
    let back: Program = serde_json::from_value(v).unwrap();
    assert_eq!(back, d.program);
}

#[test]
fn console_functions_without_an_opcode_get_one_of_ours() {
    let t = table();
    let tfc = t.by_name("tfc").expect("ToggleFlyCam is in the table");
    assert!(tfc >= fnv_script::table::CONSOLE_OPCODE_BASE);
    assert_eq!(t.get(tfc).unwrap().name, "ToggleFlyCam");
    assert_eq!(t.by_name("SetCameraFOV"), t.by_name("fov"));
    // Functions with a catalogue opcode keep it.
    assert_eq!(t.by_name("Disable"), Some(DISABLE));
}

#[test]
fn ambiguous_parameters_never_backtrack_exponentially() {
    // 40 spots of `r` + u16 + `s` + u16 (each one remote variable or a reference and a
    // local) and a stray byte at the end: no reading uses the bytes exactly, so every
    // combination fails. Failed (offset, parameter) states are remembered (#1092), so this
    // finishes at once instead of trying ~2^40 paths, and falls back to the greedy reading.
    let t = table();
    let spots: Vec<u8> = (0..40u16).flat_map(|i| [tag(b'r', i + 1), tag(b's', i + 1)].concat()).collect();
    let blob = params(60, &[spots, vec![0xEE]].concat());
    let started = std::time::Instant::now();
    let d = run(&t, &block(0x1A, &blob, &[]));
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "took {:?}", started.elapsed());
    assert_eq!(d.report.raw_args, 1, "{:?}", d.report.issues);
    let args = d.program.blocks[0].params.as_ref().expect("event parameters");
    assert_eq!(args.len(), 61); // 60 greedy readings, then the raw tail
    assert_eq!(args[0], Arg::Ref(RefIndex(1)));
    assert_eq!(args[1], Arg::Var(VarRef { on: None, var: VarSlot::Local { index: 1, letter: VarLetter::Short } }));
    assert!(matches!(&args[60], Arg::Raw(_)));

    // The same spots with a count that fits exactly still decode, remote variables first.
    let spots: Vec<u8> = (0..40u16).flat_map(|i| [tag(b'r', i + 1), tag(b's', i + 1)].concat()).collect();
    let d = run(&t, &block(0x1A, &params(40, &spots), &[]));
    assert_eq!(d.report.raw_total(), 0, "{:?}", d.report.issues);
    let args = d.program.blocks[0].params.as_ref().expect("event parameters");
    assert_eq!(args.len(), 40);
    assert!(args.iter().all(|a| matches!(a, Arg::Var(VarRef { on: Some(_), .. }))));
}
