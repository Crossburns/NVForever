//! Conversations (docs/spec-notes/dialogue.md §4–§6) on a synthetic plugin built in
//! memory, one small test per rule; no game data. Real-data checks: `dialogue_data.rs`.

mod common;

use common::*;
use fnv_esm::raw::Subrecord;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::data::{info_flag, topic_type};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig, GameSnapshot};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value as Json};

const G_OUT: u32 = 0x800;
const G_GATE: u32 = 0x801;
const G_DAYS: u32 = 0x803;
const G_TICKS: u32 = 0x804;
const G_MENU_TICKS: u32 = 0x805;
const QTICK: u32 = 0x904;
const QTICK_SCRIPT: u32 = 0x905;
const T_POOL: u32 = 0x939;
const P1: u32 = 0xF31;
const P2: u32 = 0xF32;
const P_PLAIN: u32 = 0xF33;
const P3: u32 = 0xF34;
const P4: u32 = 0xF35;
const QA: u32 = 0x900;
const QHI: u32 = 0x901;
const QOFF: u32 = 0x902;
const QEQ: u32 = 0x903;
const NPC1: u32 = 0x920;
const NPC2: u32 = 0x921;
const AVIF: u32 = 0x940;
const GREETING: u32 = 0x930;
const T_ASK: u32 = 0x931;
const T_BYE: u32 = 0x932;
const T_HIDDEN: u32 = 0x933;
const T_TOP: u32 = 0x934;
const T_TOP2: u32 = 0x935;
const T_ADDED: u32 = 0x936;
const T_SKILL: u32 = 0x937;
const GOODBYE: u32 = 0x938;
const T_ARGUE: u32 = 0x93A;
const T_REPLY: u32 = 0x93B;
const T_REPLY_NOT: u32 = 0x93C;
const T_LAST: u32 = 0x93D;
const K_OPEN: u32 = 0xF41;
const K_REPLY: u32 = 0xF42;
const K_NOT: u32 = 0xF43;
const K_LAST: u32 = 0xF44;
const HELLO: u32 = 0x93E;
const K_HELLO_LINKED: u32 = 0xF45;
const K_HELLO: u32 = 0xF46;
const K_HELLO_PLAYER: u32 = 0xF48;
const K_REPLY_TOPIC: u32 = 0xF49;
const CONV: u8 = topic_type::CONVERSATION;
const T_SOURCE: u32 = 0x950;
const T_SHARED: u32 = 0x951;
const T_LINKS: u32 = 0x952;
const T_DEAD_END: u32 = 0x953;
const S_LINE: u32 = 0xF51;
const S_LINKS: u32 = 0xF52;
const S_DEAD_END: u32 = 0xF53;
const T_LF_FROM: u32 = 0x954;
const T_LF_PLAIN: u32 = 0x955;
const T_LF_TO: u32 = 0x956;
const S_LF_FROM: u32 = 0xF54;
const S_LF_PLAIN: u32 = 0xF55;
const S_LF_TO: u32 = 0xF56;
const T_CHAT: u32 = 0x957;
const T_ANSWER: u32 = 0x958;
const C_CHAT: u32 = 0xF57;
const C_OPENER: u32 = 0xF58;
const C_REPLY: u32 = 0xF59;
// Ping-pong between actors: in QA (no repeated topics) and in QREP (repeats allowed).
const QREP: u32 = 0x90A;
const T_PING: u32 = 0x959;
const T_PONG: u32 = 0x95A;
const T_RPING: u32 = 0x95B;
const T_RPONG: u32 = 0x95C;
const P_PING: u32 = 0xF5A;
const P_PONG: u32 = 0xF5B;
const P_RPING: u32 = 0xF5C;
const P_RPONG: u32 = 0xF5D;
// Follow-up lines stored under a topic of their own (dialogue.md §3.4).
const T_FU_LEAD: u32 = 0x95D;
const T_FU: u32 = 0x95E;
const T_FU_ONLY: u32 = 0x95F;
const U_LEAD: u32 = 0xF5E;
const U_TARGET: u32 = 0xF5F;
const U_PLAIN: u32 = 0xF60;
const U_ONLY: u32 = 0xF61;
const A_OFF: u32 = 0xA01;
const A_FIRST: u32 = 0xA02;
const A_HIGH: u32 = 0xA03;
const A_LATER: u32 = 0xA04;
const A_EQUAL: u32 = 0xA05;
const R1: u32 = 0xA06;
const R2: u32 = 0xA07;
const R_AFTER: u32 = 0xA08;
const A_BARK: u32 = 0xA09;
const B_ASK: u32 = 0xB01;
const B_FOLLOW: u32 = 0xB02;
const C_BYE: u32 = 0xC01;
const D_HIDDEN: u32 = 0xD01;
const E_DAY: u32 = 0xE01;
const E_NOW: u32 = 0xE02;
const F_ADDED: u32 = 0xF01;
const H_PASS: u32 = 0xF11;
const H_FAIL: u32 = 0xF12;
const I_BYE: u32 = 0xF21;
const CELL: u32 = 0x200;
/// A Voice note (holotape) whose topic's one line, fixed to NPC1, has begin and end
/// result scripts.
const NOTE_TAPE: u32 = 0xA60;
const T_TAPE: u32 = 0xA61;
const S_TAPE: u32 = 0xA62;
// Issue #1107: lines whose end script has ChatterREF force-greet the player.
const T_HANDOFF: u32 = 0xA63;
const T_HANDOFF_STAY: u32 = 0xA64;
const H_HANDOFF: u32 = 0xA65;
const H_HANDOFF_STAY: u32 = 0xA66;
// Issue #964: lines with both follow-ups and choices.
const T_BOTH: u32 = 0xA67;
const T_BOTH_FU: u32 = 0xA68;
const W_BOTH: u32 = 0xA69;
const W_BOTH_FU: u32 = 0xA6A;
const W_BOTH_BYE: u32 = 0xA6B;
const T_KEEP: u32 = 0xA6C;
const W_KEEP: u32 = 0xA6D;
const PATROL_REF: u32 = 0x322;
const NPC1_REF: u32 = 0x320;
const NPC2_REF: u32 = 0x321;

const GET_GLOBAL_VALUE: u16 = 0x4A;
const GET_ACTOR_VALUE: u16 = 0x0E;
const SPEECH: u32 = 43;
const EQ: u8 = 0x00;
const NE: u8 = 0x20;
const GE: u8 = 0x60;
const LT: u8 = 0x80;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// A CTDA: comparison, value, function index, first parameter, run on (0 subject,
/// 1 target).
fn ctda(cmp: u8, value: f32, function: u16, p1: u32, run_on: u32) -> Subrecord {
    let mut d = vec![cmp, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend(0u32.to_le_bytes());
    d.extend(run_on.to_le_bytes());
    d.extend(0u32.to_le_bytes());
    sub(b"CTDA", d)
}

fn gate(cmp: u8, value: f32) -> Subrecord {
    ctda(cmp, value, GET_GLOBAL_VALUE, G_GATE, 0)
}

/// A dialogue line as the editor writes it.
#[derive(Default)]
struct L {
    id: u32,
    quest: u32,
    flags: u16,
    /// DATA type (`topic_type`; 0 Topic, the default).
    ty: u8,
    /// DATA next speaker (0 Target, 1 Self, 2 Either).
    next: u8,
    speaker: Option<u32>,
    conds: Vec<Subrecord>,
    /// (response number, text, emotion).
    responses: Vec<(u8, &'static str, u32)>,
    prompt: Option<&'static str>,
    choices: Vec<u32>,
    /// TCLF ("Link From").
    link_from: Vec<u32>,
    follow: Vec<u32>,
    add: Vec<u32>,
    /// Result scripts: the value TestOut is set to.
    begin: Option<&'static str>,
    end: Option<&'static str>,
    /// The end script is `ChatterREF.StartConversation player` instead.
    end_talk: bool,
    knam: Option<u32>,
}

/// `ChatterREF.StartConversation player` (SCRO 1 ChatterREF, 2 the player).
fn talk_result() -> Vec<Subrecord> {
    let call = [le16(0x1C), le16(1), st(0x1056, &[le16(1), vec![b'r'], le16(2)].concat())].concat();
    unit(0, &call, &[], &[NPC2_REF, FormId::PLAYER_REF.0])
}

fn result(value: Option<&str>) -> Vec<Subrecord> {
    match value {
        Some(v) => unit(0, &set(var(b'G', 1), ex(&[a(v)])), &[], &[G_OUT]),
        None => unit(0, &[], &[], &[]),
    }
}

fn info(l: L) -> Vec<u8> {
    let mut s = vec![sub(b"DATA", vec![l.ty, l.next, l.flags as u8, (l.flags >> 8) as u8]), sub(b"QSTI", u32b(l.quest))];
    s.extend(l.add.iter().map(|t| sub(b"NAME", u32b(*t))));
    for (number, text, emotion) in &l.responses {
        let trdt = [u32b(*emotion), u32b(50), vec![0; 4], vec![*number, 0, 0, 0], u32b(0), vec![0; 4]].concat();
        s.extend([sub(b"TRDT", trdt), sub(b"NAM1", z(text)), sub(b"NAM2", z("")), sub(b"NAM3", z(""))]);
    }
    s.extend(l.conds);
    s.extend(l.choices.iter().map(|t| sub(b"TCLT", u32b(*t))));
    s.extend(l.link_from.iter().map(|t| sub(b"TCLF", u32b(*t))));
    s.extend(l.follow.iter().map(|t| sub(b"TCFU", u32b(*t))));
    s.extend(result(l.begin));
    s.push(sub(b"NEXT", vec![]));
    s.extend(if l.end_talk { talk_result() } else { result(l.end) });
    s.extend(l.prompt.map(|p| sub(b"RNAM", z(p))));
    s.extend(l.speaker.map(|a| sub(b"ANAM", u32b(a))));
    s.extend(l.knam.map(|k| sub(b"KNAM", u32b(k))));
    record(b"INFO", 0, l.id, &s)
}

fn topic(id: u32, edid: &str, name: &str, ty: u8, flags: u8, priority: f32, lines: Vec<L>) -> Vec<u8> {
    topic_sharing(id, edid, name, ty, flags, priority, &[], lines)
}

/// A topic that also lists lines stored under other topics: (quest, line, Info Index)
/// as QSTI, INFC, INFX ("Shared Infos").
#[allow(clippy::too_many_arguments)]
fn topic_sharing(id: u32, edid: &str, name: &str, ty: u8, flags: u8, priority: f32, shared: &[(u32, u32, i32)], lines: Vec<L>) -> Vec<u8> {
    let mut s = vec![sub(b"EDID", z(edid))];
    for &(quest, line, index) in shared {
        s.extend([sub(b"QSTI", u32b(quest)), sub(b"INFC", u32b(line)), sub(b"INFX", index.to_le_bytes().to_vec())]);
    }
    s.extend([sub(b"FULL", z(name)), sub(b"PNAM", f32b(priority)), sub(b"DATA", vec![ty, flags])]);
    let dial = record(b"DIAL", 0, id, &s);
    let children: Vec<u8> = lines.into_iter().flat_map(info).collect();
    [dial, group(id.to_le_bytes(), 7, &children)].concat()
}

fn quest(id: u32, edid: &str, start: bool, priority: u8) -> Vec<u8> {
    record(b"QUST", 0, id, &[sub(b"EDID", z(edid)), sub(b"DATA", [vec![start as u8, priority, 0, 0], f32b(0.0)].concat())])
}

fn glob(id: u32, edid: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", 0, id, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", f32b(v))])
}

fn plugin_bytes() -> Vec<u8> {
    let globs = [
        glob(G_OUT, "TestOut", 0.0),
        glob(G_GATE, "TestGate", 0.0),
        glob(0x802, "GameHour", 8.0),
        glob(G_DAYS, "GameDaysPassed", 0.0),
        glob(G_TICKS, "TestTicks", 0.0),
        glob(G_MENU_TICKS, "TestMenuTicks", 0.0),
    ]
    .concat();
    // A quest whose script counts its GameMode runs in TestTicks and its MenuMode runs in
    // TestMenuTicks, every frame.
    let count = |g: u16| set(var(b'G', g), ex(&[var(b'G', g), a("1"), a("+")]));
    let tick_script = record(
        b"SCPT",
        0,
        QTICK_SCRIPT,
        &[vec![sub(b"EDID", z("TickScript"))], unit(1, &script(&[(0, None, count(1)), (1, None, count(2))]), &[], &[G_TICKS, G_MENU_TICKS])].concat(),
    );
    let tick_quest = record(
        b"QUST",
        0,
        QTICK,
        &[sub(b"EDID", z("QTick")), sub(b"SCRI", u32b(QTICK_SCRIPT)), sub(b"DATA", [vec![1u8, 0, 0, 0], f32b(0.01)].concat())],
    );
    // Start Game Enabled and Allow Repeated Conversation Topics.
    let repeat_quest = record(b"QUST", 0, QREP, &[sub(b"EDID", z("QRepeat")), sub(b"DATA", [vec![0x05, 50, 0, 0], f32b(0.0)].concat())]);
    let quests =
        [quest(QA, "QA", true, 50), quest(QHI, "QHi", true, 80), quest(QOFF, "QOff", false, 90), quest(QEQ, "QEq", true, 50), repeat_quest, tick_quest].concat();
    let npcs = [
        record(b"NPC_", 0, NPC1, &[sub(b"EDID", z("Talker"))]),
        record(b"NPC_", 0, NPC2, &[sub(b"EDID", z("Chatter"))]),
    ]
    .concat();
    let avif = record(b"AVIF", 0, AVIF, &[sub(b"EDID", z("AVSpeech"))]);
    let one = |text: &'static str| vec![(1u8, text, 0u32)];
    let me = Some(NPC1);
    let top_level = 2u8;
    let dial = [
        topic(
            GREETING,
            "GREETING",
            "GREETING",
            topic_type::TOPIC,
            0,
            90.0,
            vec![
                // A stopped quest's line, of the highest priority.
                L { id: A_OFF, quest: QOFF, speaker: me, responses: one("Never"), ..L::default() },
                // The first greeting: Say Once, responses stored out of number order,
                // choices, both result scripts.
                L {
                    id: A_FIRST,
                    quest: QA,
                    flags: info_flag::SAY_ONCE,
                    speaker: me,
                    conds: vec![gate(NE, 9.0)],
                    responses: vec![(2, "Second.", 5), (1, "First", 3)],
                    choices: vec![T_ASK, T_BYE, T_HIDDEN],
                    begin: Some("1"),
                    end: Some("2"),
                    ..L::default()
                },
                // A higher-priority quest's line, stored later.
                L { id: A_HIGH, quest: QHI, speaker: me, conds: vec![gate(EQ, 5.0)], responses: one("Urgent"), ..L::default() },
                L { id: A_LATER, quest: QA, speaker: me, responses: one("Again"), ..L::default() },
                // A quest of the same priority as QA, stored after it.
                L { id: A_EQUAL, quest: QEQ, speaker: me, responses: one("Equal"), ..L::default() },
                // The second actor: two Random lines, the stack closed by Random End.
                L { id: R1, quest: QA, flags: info_flag::RANDOM, speaker: Some(NPC2), responses: one("One"), ..L::default() },
                L { id: R2, quest: QA, flags: info_flag::RANDOM | info_flag::RANDOM_END, speaker: Some(NPC2), responses: one("Two"), ..L::default() },
                L { id: R_AFTER, quest: QA, speaker: Some(NPC2), responses: one("After"), ..L::default() },
                // A greeting with the Goodbye flag: said at the player, no conversation.
                L {
                    id: A_BARK,
                    quest: QHI,
                    flags: info_flag::GOODBYE,
                    speaker: me,
                    conds: vec![gate(EQ, 7.0)],
                    responses: one("Move along"),
                    add: vec![T_ADDED],
                    begin: Some("3"),
                    end: Some("4"),
                    ..L::default()
                },
            ],
        ),
        topic(
            T_ASK,
            "TopicAsk",
            "Ask",
            topic_type::TOPIC,
            0,
            50.0,
            vec![
                L { id: B_ASK, quest: QA, speaker: me, responses: one("Asked"), prompt: Some("Tell me"), follow: vec![B_FOLLOW], add: vec![T_ADDED], ..L::default() },
                L { id: B_FOLLOW, quest: QA, speaker: me, responses: one("And more"), end: Some("7"), ..L::default() },
            ],
        ),
        // A line whose follow-ups are stored under other topics: T_FU's first line is
        // one of them, its second is not; T_FU_ONLY has only a follow-up line.
        topic(
            T_FU_LEAD,
            "TopicFollowLead",
            "Lead",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: U_LEAD, quest: QA, speaker: me, responses: one("Listen"), follow: vec![U_TARGET, U_ONLY], ..L::default() }],
        ),
        topic(
            T_FU,
            "TopicFollowed",
            "Followed",
            topic_type::TOPIC,
            0,
            50.0,
            vec![
                L { id: U_TARGET, quest: QA, speaker: me, responses: one("As I was saying"), ..L::default() },
                L { id: U_PLAIN, quest: QA, speaker: me, responses: one("Fine, then"), ..L::default() },
            ],
        ),
        topic(T_FU_ONLY, "TopicFollowOnly", "Only", topic_type::TOPIC, 0, 50.0, vec![L { id: U_ONLY, quest: QA, speaker: me, responses: one("Only this"), ..L::default() }]),
        topic(
            T_BYE,
            "TopicBye",
            "Bye",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: C_BYE, quest: QA, flags: info_flag::GOODBYE, speaker: me, responses: one("Farewell"), end: Some("9"), ..L::default() }],
        ),
        topic(
            T_HIDDEN,
            "TopicHidden",
            "Hidden",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: D_HIDDEN, quest: QA, speaker: me, conds: vec![gate(EQ, 99.0)], responses: one("Secret"), ..L::default() }],
        ),
        topic(
            T_TOP,
            "TopicDaily",
            "Daily",
            topic_type::TOPIC,
            top_level,
            70.0,
            vec![L { id: E_DAY, quest: QA, flags: info_flag::SAY_ONCE_A_DAY, speaker: me, responses: one("Today"), ..L::default() }],
        ),
        topic(
            T_TOP2,
            "TopicNow",
            "Now",
            topic_type::TOPIC,
            top_level,
            90.0,
            vec![L { id: E_NOW, quest: QA, flags: info_flag::RUN_IMMEDIATELY, speaker: me, responses: one("Now"), begin: Some("20"), end: Some("21"), ..L::default() }],
        ),
        topic(T_ADDED, "TopicAdded", "Added", topic_type::TOPIC, 0, 60.0, vec![L { id: F_ADDED, quest: QA, speaker: me, responses: one("Added"), ..L::default() }]),
        topic(
            T_SKILL,
            "TopicSkill",
            "Skill",
            topic_type::TOPIC,
            top_level,
            10.0,
            vec![
                L {
                    id: H_PASS,
                    quest: QA,
                    speaker: me,
                    conds: vec![ctda(GE, 35.0, GET_ACTOR_VALUE, SPEECH, 1)],
                    responses: one("Convinced"),
                    prompt: Some("Trust me"),
                    knam: Some(AVIF),
                    ..L::default()
                },
                L {
                    id: H_FAIL,
                    quest: QA,
                    speaker: me,
                    conds: vec![ctda(LT, 35.0, GET_ACTOR_VALUE, SPEECH, 1)],
                    responses: one("No"),
                    prompt: Some("Trust me?"),
                    knam: Some(AVIF),
                    ..L::default()
                },
            ],
        ),
        // Two groups of Random lines with a plain line between them: two pools. The first
        // group is said unless TestGate is 31; the plain line only when it is 77.
        topic(
            T_POOL,
            "TopicPool",
            "Pool",
            topic_type::TOPIC,
            0,
            50.0,
            vec![
                L { id: P1, quest: QA, flags: info_flag::RANDOM, speaker: me, conds: vec![gate(NE, 31.0)], responses: one("P1"), ..L::default() },
                L { id: P2, quest: QA, flags: info_flag::RANDOM, speaker: me, conds: vec![gate(NE, 31.0)], responses: one("P2"), ..L::default() },
                L { id: P_PLAIN, quest: QA, speaker: me, conds: vec![gate(EQ, 77.0)], responses: one("Plain"), ..L::default() },
                L { id: P3, quest: QA, flags: info_flag::RANDOM, speaker: me, responses: one("P3"), ..L::default() },
                L { id: P4, quest: QA, flags: info_flag::RANDOM, speaker: me, responses: one("P4"), ..L::default() },
            ],
        ),
        topic(
            GOODBYE,
            "GOODBYE",
            "GOODBYE",
            topic_type::CONVERSATION,
            0,
            50.0,
            vec![L { id: I_BYE, quest: QA, flags: info_flag::GOODBYE, responses: one("See you"), ..L::default() }],
        ),
        // A conversation between the two actors: the first opens with two responses (only
        // the first is said) and links to two topics, of which the second actor can say
        // only one; its reply goes on with itself (next speaker Self) and its last line
        // says goodbye, its Link To ignored.
        topic(
            T_ARGUE,
            "TestArgue",
            "TestArgue",
            topic_type::CONVERSATION,
            0,
            50.0,
            vec![L {
                id: K_OPEN,
                quest: QA,
                ty: CONV,
                speaker: me,
                responses: vec![(1, "Open", 0), (2, "Never said", 0)],
                choices: vec![T_REPLY_NOT, T_REPLY],
                begin: Some("30"),
                end: Some("31"),
                ..L::default()
            }],
        ),
        topic(
            T_REPLY,
            "TestReply",
            "TestReply",
            topic_type::CONVERSATION,
            0,
            50.0,
            vec![
                // A Topic line the second actor could say, stored first: never said
                // between actors.
                L { id: K_REPLY_TOPIC, quest: QA, speaker: Some(NPC2), responses: one("For the player"), ..L::default() },
                L { id: K_REPLY, quest: QA, ty: CONV, next: 1, speaker: Some(NPC2), responses: one("Reply"), choices: vec![T_LAST], end: Some("32"), ..L::default() },
            ],
        ),
        topic(
            T_REPLY_NOT,
            "TestReplyNot",
            "TestReplyNot",
            topic_type::CONVERSATION,
            0,
            50.0,
            vec![L { id: K_NOT, quest: QA, ty: CONV, speaker: Some(NPC2), conds: vec![gate(EQ, 99.0)], responses: one("Not said"), ..L::default() }],
        ),
        topic(
            T_LAST,
            "TestLast",
            "TestLast",
            topic_type::CONVERSATION,
            0,
            50.0,
            vec![L { id: K_LAST, quest: QA, ty: CONV, flags: info_flag::GOODBYE, speaker: Some(NPC2), responses: one("Last"), choices: vec![T_REPLY], ..L::default() }],
        ),
        // HELLO: a line with a Link From (cannot start anything); a Topic line for the
        // player, said only when TestGate is 5; then a Conversation line that starts the
        // same exchange as TestArgue's reply.
        topic(
            HELLO,
            "HELLO",
            "HELLO",
            topic_type::CONVERSATION,
            0,
            50.0,
            vec![
                L { id: K_HELLO_LINKED, quest: QA, ty: CONV, speaker: me, responses: one("Linked"), link_from: vec![T_ARGUE], ..L::default() },
                L { id: K_HELLO_PLAYER, quest: QA, speaker: me, conds: vec![gate(EQ, 5.0)], responses: one("Hello, stranger"), ..L::default() },
                L { id: K_HELLO, quest: QA, ty: CONV, speaker: me, responses: one("Hi"), choices: vec![T_REPLY], ..L::default() },
            ],
        ),
        // Shared lines (INFC / INFX): TopicShared has no line of its own and shares
        // TopicSource's. TopicLinks' line offers it and the hidden topic as its choices;
        // TopicDeadEnd's only choice cannot be said.
        topic(T_SOURCE, "TopicSource", "Where?", topic_type::TOPIC, 0, 50.0, vec![L { id: S_LINE, quest: QA, speaker: me, responses: one("Here"), ..L::default() }]),
        topic_sharing(T_SHARED, "TopicShared", "Where?", topic_type::TOPIC, 0, 50.0, &[(QA, S_LINE, 0)], vec![]),
        topic(
            T_LINKS,
            "TopicLinks",
            "Links",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: S_LINKS, quest: QA, speaker: me, responses: one("Ask away"), choices: vec![T_HIDDEN, T_SHARED], ..L::default() }],
        ),
        topic(
            T_DEAD_END,
            "TopicDeadEnd",
            "Dead end",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: S_DEAD_END, quest: QA, speaker: me, responses: one("Hmm"), choices: vec![T_HIDDEN], ..L::default() }],
        ),
        // Link From: TopicLinkTo's line names TopicLinkFrom (whose own only choice cannot
        // be said) and TopicLinkPlain (no choices of its own).
        topic(
            T_LF_FROM,
            "TopicLinkFrom",
            "Link from",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: S_LF_FROM, quest: QA, speaker: me, responses: one("From"), choices: vec![T_HIDDEN], ..L::default() }],
        ),
        topic(T_LF_PLAIN, "TopicLinkPlain", "Link plain", topic_type::TOPIC, 0, 50.0, vec![L { id: S_LF_PLAIN, quest: QA, speaker: me, responses: one("Plain"), ..L::default() }]),
        topic(
            T_LF_TO,
            "TopicLinkTo",
            "Link to",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: S_LF_TO, quest: QA, speaker: me, responses: one("To"), link_from: vec![T_LF_FROM, T_LF_PLAIN], ..L::default() }],
        ),
        // Between actors: TestChat's line links nowhere; TestAnswer's second line names
        // TestChat in its Link From, its first (an opener) does not.
        topic(T_CHAT, "TestChat", "TestChat", topic_type::CONVERSATION, 0, 50.0, vec![L { id: C_CHAT, quest: QA, ty: CONV, speaker: me, responses: one("Chat"), ..L::default() }]),
        topic(
            T_ANSWER,
            "TestAnswer",
            "TestAnswer",
            topic_type::CONVERSATION,
            0,
            50.0,
            vec![
                L { id: C_OPENER, quest: QA, ty: CONV, flags: info_flag::GOODBYE, speaker: Some(NPC2), responses: one("Opener"), ..L::default() },
                L { id: C_REPLY, quest: QA, ty: CONV, flags: info_flag::GOODBYE, speaker: Some(NPC2), responses: one("Reply"), link_from: vec![T_CHAT], ..L::default() },
            ],
        ),
        topic(T_PING, "TestPing", "TestPing", topic_type::CONVERSATION, 0, 50.0, vec![L { id: P_PING, quest: QA, ty: CONV, responses: one("Ping"), choices: vec![T_PONG], ..L::default() }]),
        topic(T_PONG, "TestPong", "TestPong", topic_type::CONVERSATION, 0, 50.0, vec![L { id: P_PONG, quest: QA, ty: CONV, responses: one("Pong"), choices: vec![T_PING], ..L::default() }]),
        topic(T_RPING, "TestRPing", "TestRPing", topic_type::CONVERSATION, 0, 50.0, vec![L { id: P_RPING, quest: QREP, ty: CONV, responses: one("Ping"), choices: vec![T_RPONG], ..L::default() }]),
        topic(T_RPONG, "TestRPong", "TestRPong", topic_type::CONVERSATION, 0, 50.0, vec![L { id: P_RPONG, quest: QREP, ty: CONV, responses: one("Pong"), choices: vec![T_RPING], ..L::default() }]),
        topic(
            T_HANDOFF,
            "TestHandoff",
            "Handoff",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: H_HANDOFF, quest: QA, flags: info_flag::GOODBYE, speaker: me, responses: one("Go see him"), end_talk: true, ..L::default() }],
        ),
        topic(
            T_HANDOFF_STAY,
            "TestHandoffStay",
            "HandoffStay",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: H_HANDOFF_STAY, quest: QA, speaker: me, responses: one("He can wait"), choices: vec![T_BYE], end_talk: true, ..L::default() }],
        ),
        // Lines with follow-ups and choices: W_BOTH's follow-up is a Goodbye line, W_KEEP's
        // has choices of its own.
        topic(
            T_BOTH,
            "TestBoth",
            "Both",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: W_BOTH, quest: QA, speaker: me, responses: one("Hear me out"), follow: vec![W_BOTH_BYE], choices: vec![T_ASK, T_BYE], ..L::default() }],
        ),
        topic(
            T_KEEP,
            "TestKeep",
            "Keep",
            topic_type::TOPIC,
            0,
            50.0,
            vec![L { id: W_KEEP, quest: QA, speaker: me, responses: one("Listen"), follow: vec![W_BOTH_FU], choices: vec![T_ASK, T_BYE], ..L::default() }],
        ),
        topic(
            T_BOTH_FU,
            "TestBothFollow",
            "BothFollow",
            topic_type::TOPIC,
            0,
            50.0,
            vec![
                L { id: W_BOTH_FU, quest: QA, speaker: me, responses: one("And another thing"), choices: vec![T_BYE], ..L::default() },
                L { id: W_BOTH_BYE, quest: QA, flags: info_flag::GOODBYE, speaker: me, responses: one("That's all"), ..L::default() },
            ],
        ),
        topic(T_TAPE, "TestTape", "TestTape", topic_type::TOPIC, 0, 50.0, vec![L { id: S_TAPE, quest: QA, speaker: me, responses: one("Tape"), begin: Some("6"), end: Some("7"), ..L::default() }]),
    ]
    .concat();
    // DATA 3 Voice; SNAM the speaker, TNAM the topic (the NOTE unions, holotapes.md §1).
    let note = record(b"NOTE", 0, NOTE_TAPE, &[sub(b"EDID", z("TestTapeNote")), sub(b"DATA", vec![3]), sub(b"SNAM", u32b(NPC1)), sub(b"TNAM", u32b(T_TAPE)), sub(b"ONAM", u32b(QA))]);
    let placed = |id: u32, base: u32, edid: &str| record(b"ACHR", 0, id, &[sub(b"EDID", z(edid)), sub(b"NAME", u32b(base)), sub(b"DATA", vec![0; 24])]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TalkCell"))]);
    // A patrol point whose result script (after XPPA) sets TestOut to 9.
    let patrol = record(b"REFR", 0, PATROL_REF, &[vec![sub(b"EDID", z("PatrolREF")), sub(b"NAME", u32b(NPC2)), sub(b"XPRD", f32b(0.0)), sub(b"XPPA", vec![])], result(Some("9")), vec![sub(b"DATA", vec![0; 24])]].concat());
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &[placed(NPC1_REF, NPC1, "TalkerREF"), placed(NPC2_REF, NPC2, "ChatterREF"), patrol].concat()));
    [
        tes4(),
        top(b"GLOB", &globs),
        top(b"SCPT", &tick_script),
        top(b"AVIF", &avif),
        top(b"NPC_", &npcs),
        top(b"QUST", &quests),
        top(b"NOTE", &note),
        top(b"DIAL", &dial),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Talk.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    assert!(data.issues.is_empty(), "{:?}", data.issues);
    Fixture { data, forms, refs }
}

fn game_seeded(f: &Fixture, seed: u64) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, seed, ..GameConfig::default() });
    g.new_game();
    g
}

fn game(f: &Fixture) -> Game<'_> {
    game_seeded(f, 0)
}

fn actor() -> FormId {
    FormId(NPC1_REF)
}

fn out(g: &Game) -> f32 {
    g.world.global(FormId(G_OUT)).unwrap()
}

fn set_global(g: &mut Game, id: u32, v: f32) {
    g.world.set_global(FormId(id), v).unwrap();
}

/// Frames until the actor has finished speaking (choices offered or conversation over).
fn listen(g: &mut Game) {
    for _ in 0..20_000 {
        if g.dialogue.as_ref().is_none_or(|c| !c.choices.is_empty()) {
            return;
        }
        g.tick();
    }
    panic!("the actor never stopped speaking");
}

fn offered(g: &Game) -> Vec<u32> {
    g.dialogue_choices().iter().map(|c| c.topic.0).collect()
}

fn pick(g: &mut Game, topic: u32) -> FormId {
    let i = g.dialogue_choices().iter().position(|c| c.topic.0 == topic).unwrap_or_else(|| panic!("0x{topic:X} is not offered: {:X?}", offered(g)));
    g.choose(i).unwrap()
}

/// A conversation past the first greeting, at the topic list.
fn at_topic_list<'a>(f: &'a Fixture) -> Game<'a> {
    let mut g = game(f);
    g.said.insert((actor(), FormId(A_FIRST)));
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_LATER)));
    listen(&mut g);
    g
}

fn call(g: &mut Game, opcode: u16, on: u32, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode, on: Some(FormId(on)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, args).num()
}

#[test]
fn the_records_are_read_in_full() {
    let f = fixture();
    let d = &f.data;
    let t = &d.topic_defs[&FormId(T_TOP2)];
    assert_eq!((t.edid.as_deref(), t.name.as_deref(), t.topic_type, t.top_level(), t.priority), (Some("TopicNow"), Some("Now"), topic_type::TOPIC, true, 90.0));
    assert_eq!(d.topic_defs[&FormId(GOODBYE)].topic_type, topic_type::CONVERSATION);
    assert_eq!((d.greeting, d.goodbye), (Some(FormId(GREETING)), Some(FormId(GOODBYE))));
    // Offered order of the top-level topics: priority, highest first.
    assert_eq!(d.top_level, vec![FormId(T_TOP2), FormId(T_TOP), FormId(T_SKILL)]);
    let i = &d.infos[&FormId(A_FIRST)];
    assert!(i.has(info_flag::SAY_ONCE) && i.say_once && !i.goodbye);
    assert_eq!((i.quest, i.speaker, i.topic), (Some(FormId(QA)), Some(FormId(NPC1)), Some(FormId(GREETING))));
    assert_eq!(i.choices, vec![FormId(T_ASK), FormId(T_BYE), FormId(T_HIDDEN)]);
    assert_eq!(i.responses.iter().map(|r| (r.number, r.text.as_str(), r.emotion, r.emotion_value)).collect::<Vec<_>>(), vec![(2, "Second.", 5, 50), (1, "First", 3, 50)]);
    assert_eq!(i.response_chars, vec![7, 5]);
    assert_eq!(i.response_order(), vec![1, 0]);
    assert!(i.begin.is_some() && i.end.is_some());
    let b = &d.infos[&FormId(B_ASK)];
    assert_eq!((b.prompt.as_deref(), &b.follow_ups, &b.add_topics), (Some("Tell me"), &vec![FormId(B_FOLLOW)], &vec![FormId(T_ADDED)]));
    assert_eq!(d.infos[&FormId(H_PASS)].challenge_form, Some(FormId(AVIF)));
    assert!(d.infos[&FormId(E_NOW)].has(info_flag::RUN_IMMEDIATELY) && d.infos[&FormId(E_DAY)].has(info_flag::SAY_ONCE_A_DAY));
    assert!(d.infos[&FormId(R2)].has(info_flag::RANDOM | info_flag::RANDOM_END));
    assert_eq!(d.choice_text(FormId(T_ASK), FormId(B_ASK)), "Tell me");
    assert_eq!(d.choice_text(FormId(T_BYE), FormId(C_BYE)), "Bye");
}

#[test]
fn the_greeting_is_the_first_line_the_actor_can_say() {
    let f = fixture();
    let mut g = game(&f);
    // QOff's line comes first by priority, but its quest is stopped.
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_FIRST)));
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueStarted { actor: a, topic } if a == actor() && topic == FormId(GREETING))));
    // An actor with nothing to say cannot be talked to.
    let mut h = game(&f);
    h.stop_quest(FormId(QA));
    h.stop_quest(FormId(QHI));
    h.stop_quest(FormId(QEQ));
    assert!(h.start_dialogue(actor(), None).is_err());
    assert!(h.dialogue.is_none());
    assert!(h.events.iter().any(|e| matches!(e.kind, EventKind::NoLine { .. })));
}

#[test]
fn a_topic_chosen_passes_over_its_follow_up_lines_while_another_line_can_be_said() {
    let f = fixture();
    assert!(f.data.follow_up_targets.contains(&FormId(U_TARGET)) && !f.data.follow_up_targets.contains(&FormId(U_PLAIN)));
    let mut g = game(&f);
    g.player.topics.extend([FormId(T_FU), FormId(T_FU_ONLY)]);
    g.said.insert((actor(), FormId(A_FIRST)));
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_LATER)));
    listen(&mut g);
    let info = |g: &Game, t: u32| g.dialogue_choices().iter().find(|c| c.topic.0 == t).map(|c| c.info);
    // T_FU's first line is a follow-up of U_LEAD: the topic offered says its other line.
    assert_eq!(info(&g, T_FU), Some(FormId(U_PLAIN)));
    // A topic with nothing but a follow-up line still says it.
    assert_eq!(info(&g, T_FU_ONLY), Some(FormId(U_ONLY)));
    assert_eq!(pick(&mut g, T_FU), FormId(U_PLAIN));
    // Said other than as a choice (a greeting, SayTo), the stored first line is taken.
    assert_eq!(g.select_line(actor(), Some(FormId::PLAYER_REF), FormId(T_FU)), Some(FormId(U_TARGET)));
}

#[test]
fn the_higher_priority_quest_wins_and_equal_priorities_keep_the_stored_order() {
    let f = fixture();
    let mut g = game(&f);
    // Both A_FIRST (QA, 50) and A_HIGH (QHi, 80, stored later) qualify.
    set_global(&mut g, G_GATE, 5.0);
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_HIGH)));
    // QA and QEq have the same priority: QA's line is stored first.
    let mut h = game(&f);
    h.said.insert((actor(), FormId(A_FIRST)));
    assert_eq!(h.start_dialogue(actor(), None), Ok(FormId(A_LATER)));
    assert_eq!(
        f.data.lines_by_priority(FormId(GREETING)).iter().map(|i| i.0).collect::<Vec<_>>(),
        vec![A_OFF, A_HIGH, A_BARK, A_FIRST, A_LATER, A_EQUAL, R1, R2, R_AFTER]
    );
}

#[test]
fn a_line_runs_its_begin_script_says_its_responses_in_order_and_runs_its_end_script() {
    let f = fixture();
    let mut g = game(&f);
    g.start_dialogue(actor(), None).unwrap();
    // The begin result ran inside the start; the actor talks; the dialogue menu is up.
    assert_eq!(out(&g), 1.0);
    assert!(g.is_talking(actor()));
    assert_eq!(call(&mut g, 0x1024, NPC1_REF, &[ArgValue::Value(Value::Num(1009.0))]), 1.0);
    // Response 1 ("First", emotion 3) before response 2 ("Second.", emotion 5).
    assert_eq!(g.dialogue_emotion(actor()), 3);
    let f0 = g.frame;
    let first = fnv_game::game::fitted_seconds(5);
    g.frames((first * 60.0) as u64 + 2);
    assert_eq!(g.dialogue_emotion(actor()), 5);
    assert_eq!(out(&g), 1.0);
    listen(&mut g);
    // The line lasted as long as its two responses (1.1 s + 1/16 s per character each).
    let want = (fnv_game::game::fitted_seconds(5) + fnv_game::game::fitted_seconds(7)) * 60.0;
    assert_eq!(g.line_seconds(actor(), FormId(A_FIRST)) * 60.0, want);
    assert!(((g.frame - f0) as f64 - want).abs() <= 2.0, "{} frames for {want}", g.frame - f0);
    assert_eq!(out(&g), 2.0);
    assert!(!g.is_talking(actor()));
    let said: Vec<(u8, usize)> = g.events.iter().filter_map(|e| if let EventKind::ResponseSaid { number, chars, .. } = e.kind { Some((number, chars)) } else { None }).collect();
    assert_eq!(said, vec![(1, 5), (2, 7)]);
    // A menu (a message box) holds the conversation.
    let mut h = game(&f);
    h.start_dialogue(actor(), None).unwrap();
    h.menu = Some(1001);
    h.frames(2000);
    assert!(h.is_talking(actor()));
}

#[test]
fn a_conversation_pauses_the_game_behind_it() {
    let f = fixture();
    let mut g = game(&f);
    let ticks = |g: &Game| (g.world.global(FormId(G_TICKS)).unwrap(), g.world.global(FormId(G_MENU_TICKS)).unwrap());
    g.frames(10);
    assert_eq!(ticks(&g), (10.0, 0.0));
    // A line said outside a conversation pauses nothing.
    g.say(actor(), None, FormId(T_BYE), false).unwrap();
    g.frames(5);
    assert_eq!(ticks(&g), (15.0, 0.0));
    assert!(!g.paused() && !g.in_menu());
    // In the conversation: menu mode. GameMode does not run, game time stands still,
    // MenuMode blocks run, MenuMode reads 1 with and without the code. `Game::menu` is
    // for the other menus.
    let t0 = g.time;
    g.start_dialogue(actor(), None).unwrap();
    assert!(g.paused() && g.in_menu() && g.menu_open(1009) && !g.menu_open(1001));
    assert_eq!((g.menu, g.open_menu_code()), (None, Some(1009)));
    assert_eq!(call(&mut g, 0x1024, NPC1_REF, &[]), 1.0);
    assert_eq!(call(&mut g, 0x1024, NPC1_REF, &[ArgValue::Value(Value::Num(1009.0))]), 1.0);
    assert_eq!(call(&mut g, 0x1024, NPC1_REF, &[ArgValue::Value(Value::Num(1001.0))]), 0.0);
    let f0 = g.frame;
    listen(&mut g);
    let spoken = (g.frame - f0) as f32;
    assert!(spoken > 60.0);
    assert_eq!(ticks(&g), (15.0, spoken));
    assert_eq!(g.time, t0);
    // Choosing does not unpause; the Goodbye line's end does.
    pick(&mut g, T_BYE);
    listen(&mut g);
    assert!(g.dialogue.is_none() && !g.paused());
    assert_eq!(g.time, t0);
    let menu_ticks = ticks(&g).1;
    g.frames(4);
    assert_eq!(ticks(&g), (19.0, menu_ticks));
    assert!((g.time - t0 - 4.0 / 60.0).abs() < 1e-9);
    assert_eq!(call(&mut g, 0x1024, NPC1_REF, &[]), 0.0);
    // A message box over the conversation holds it; both menus read as open.
    let mut h = game(&f);
    h.start_dialogue(actor(), None).unwrap();
    h.menu = Some(1001);
    assert!(h.menu_open(1001) && h.menu_open(1009));
    assert_eq!(h.open_menu_code(), Some(1001));
    h.frames(2000);
    assert!(h.is_talking(actor()));
    h.menu = None;
    listen(&mut h);
    assert!(!h.dialogue_choices().is_empty());
}

#[test]
fn random_lines_that_sit_together_are_a_pool_and_a_plain_line_splits_pools() {
    let f = fixture();
    let listener = Some(FormId::PLAYER_REF);
    let picks = |gate: f32| {
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..32 {
            let mut g = game_seeded(&f, seed);
            set_global(&mut g, G_GATE, gate);
            seen.insert(g.select_line(actor(), listener, FormId(T_POOL)).unwrap().0);
        }
        seen.into_iter().collect::<Vec<u32>>()
    };
    // R R N R R with N not sayable: the first pair is one pool, never mixed with the
    // second pair.
    assert_eq!(picks(0.0), vec![P1, P2]);
    // Nothing sayable in the first pool, the plain line not sayable either: the second
    // pool.
    assert_eq!(picks(31.0), vec![P3, P4]);
    // The plain line sayable: the first pool still comes first.
    assert_eq!(picks(77.0), vec![P1, P2]);
    assert_eq!(f.data.lines_by_priority(FormId(T_POOL)).iter().map(|i| i.0).collect::<Vec<_>>(), vec![P1, P2, P_PLAIN, P3, P4]);
}

#[test]
fn a_lines_choices_are_offered_and_nothing_else_and_a_topic_without_a_line_is_hidden() {
    let f = fixture();
    let mut g = game(&f);
    g.start_dialogue(actor(), None).unwrap();
    listen(&mut g);
    // T_HIDDEN's only line fails its condition; the top-level topics are not listed.
    assert_eq!(offered(&g), vec![T_ASK, T_BYE]);
    let c = g.dialogue_choices().to_vec();
    assert_eq!((c[0].info, c[0].prompt, c[0].chars, g.choice_text(&c[0])), (FormId(B_ASK), true, 7, "Tell me"));
    assert_eq!((c[1].info, c[1].prompt, c[1].goodbye, g.choice_text(&c[1])), (FormId(C_BYE), false, true, "Bye"));
    assert!(g.events.iter().any(|e| matches!(&e.kind, EventKind::ChoicesOffered { choices, .. } if choices.len() == 2)));
    // Choosing while the actor speaks, or an entry that is not there, is refused.
    assert!(g.choose(5).is_err());
    pick(&mut g, T_ASK);
    assert!(g.choose(0).is_err());
}

/// A choice listed for a dim player shows its topic's Dumb Response (`Game::player_dim`,
/// checked against the master in dialogue_data.rs); the fixture's player has no record,
/// so it is not dim.
#[test]
fn a_dim_players_choice_shows_the_topics_dumb_response() {
    let mut f = fixture();
    f.data.topic_defs.get_mut(&FormId(T_ASK)).unwrap().dumb_response = Some("Me ask".into());
    let mut g = game(&f);
    assert!(!g.player_dim(), "no Intelligence to read: not dim");
    g.start_dialogue(actor(), None).unwrap();
    listen(&mut g);
    let c = g.dialogue_choices().to_vec();
    assert_eq!((c[0].topic, c[0].dumb, g.choice_text(&c[0])), (FormId(T_ASK), false, "Tell me"));
    let dim = fnv_game::dialogue::Choice { dumb: true, ..c[0].clone() };
    assert_eq!(g.choice_text(&dim), "Me ask");
    // A topic without one keeps its text whatever the flag.
    assert_eq!(g.choice_text(&fnv_game::dialogue::Choice { dumb: true, ..c[1].clone() }), "Bye");
}

#[test]
fn a_choice_with_only_a_shared_line_is_offered_and_says_that_line() {
    let f = fixture();
    // The shared line counts as a line of the topic that lists it, not of its own.
    assert!(f.data.lines_by_priority(FormId(T_SHARED)).is_empty());
    assert_eq!(f.data.lines_tried(FormId(T_SHARED)), vec![FormId(S_LINE)]);
    assert_eq!(f.data.topics[&FormId(T_SOURCE)], vec![FormId(S_LINE)]);
    let mut g = game(&f);
    assert_eq!(g.start_dialogue(actor(), Some(FormId(T_LINKS))), Ok(FormId(S_LINKS)));
    listen(&mut g);
    // Only the line's choices: the hidden one has nothing to say, the shared one is
    // offered with the topic's own name; no top-level topics, no GOODBYE.
    assert_eq!(offered(&g), vec![T_SHARED]);
    assert_eq!(g.choice_text(&g.dialogue_choices()[0]), "Where?");
    assert_eq!(pick(&mut g, T_SHARED), FormId(S_LINE));
}

#[test]
fn a_topic_is_offered_after_the_topics_its_lines_link_from() {
    let f = fixture();
    assert_eq!(f.data.linked_from[&FormId(T_LF_FROM)], vec![FormId(T_LF_TO)]);
    // A line with choices: Link From joins them (its own choice cannot be said).
    let mut g = game(&f);
    g.start_dialogue(actor(), Some(FormId(T_LF_FROM))).unwrap();
    listen(&mut g);
    assert_eq!(offered(&g), vec![T_LF_TO]);
    assert_eq!(pick(&mut g, T_LF_TO), FormId(S_LF_TO));
    // A line without choices: the topic list, with the linked topic in it by priority.
    let mut h = game(&f);
    h.start_dialogue(actor(), Some(FormId(T_LF_PLAIN))).unwrap();
    listen(&mut h);
    assert_eq!(offered(&h), vec![T_TOP2, T_TOP, T_LF_TO, T_SKILL, GOODBYE]);
}

#[test]
fn a_line_whose_choices_cannot_be_said_offers_nothing_else() {
    let f = fixture();
    let mut g = game(&f);
    g.start_dialogue(actor(), Some(FormId(T_DEAD_END))).unwrap();
    listen(&mut g);
    // Not the topic list: the conversation ends.
    assert!(g.dialogue.is_none());
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueEnded { reason: "no_choices", .. })));
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::ChoicesOffered { .. })));
}

#[test]
fn say_once_lines_are_said_once_per_actor() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_FIRST)));
    assert!(g.end_dialogue());
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_LATER)));
}

/// A Say Once line another actor said is not said again (observer check 31: the Yes Man
/// mainframe did not repeat the Tops Yes Man's first-meeting greeting; #1113).
#[test]
fn say_once_lines_said_by_another_actor_are_not_said_again() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.can_say(actor(), Some(FormId(0x14)), FormId(A_FIRST)));
    g.said.insert((FormId(0x00FF_0001), FormId(A_FIRST)));
    assert!(!g.can_say(actor(), Some(FormId(0x14)), FormId(A_FIRST)));
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_LATER)));
}

#[test]
fn added_topics_follow_ups_and_the_topic_list() {
    let f = fixture();
    let mut g = game(&f);
    g.start_dialogue(actor(), None).unwrap();
    listen(&mut g);
    assert_eq!(pick(&mut g, T_ASK), FormId(B_ASK));
    assert!(g.player.topics.is_empty());
    listen(&mut g);
    // The line's Add Topics list was added when it ended, and its follow-up line was
    // said next without a choice; the follow-up's end script ran.
    assert!(g.player.topics.contains(&FormId(T_ADDED)));
    let lines: Vec<u32> = g.events.iter().filter_map(|e| if let EventKind::LineStarted { info, .. } = e.kind { Some(info.0) } else { None }).collect();
    assert_eq!(lines, vec![A_FIRST, B_ASK, B_FOLLOW]);
    assert_eq!(out(&g), 7.0);
    // The follow-up has no choices: the top-level topics and the added topic, by
    // priority (90, 70, 60, 10), then the actor's GOODBYE since none of them says
    // goodbye.
    assert_eq!(offered(&g), vec![T_TOP2, T_TOP, T_ADDED, T_SKILL, GOODBYE]);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::TopicAdded { topic, by: Some(by) } if topic == FormId(T_ADDED) && by == FormId(B_ASK))));
}

#[test]
fn a_goodbye_line_ends_the_conversation() {
    let f = fixture();
    let mut g = game(&f);
    g.start_dialogue(actor(), None).unwrap();
    listen(&mut g);
    assert_eq!(pick(&mut g, T_BYE), FormId(C_BYE));
    assert!(g.dialogue.is_some());
    listen(&mut g);
    assert!(g.dialogue.is_none());
    assert_eq!(out(&g), 9.0);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueEnded { reason: "goodbye", lines: 2, .. })));
    assert_eq!(call(&mut g, 0x1024, NPC1_REF, &[ArgValue::Value(Value::Num(1009.0))]), 0.0);
    // The generic GOODBYE of the topic list ends it too.
    let mut h = at_topic_list(&f);
    assert_eq!(pick(&mut h, GOODBYE), FormId(I_BYE));
    listen(&mut h);
    assert!(h.dialogue.is_none());
}

/// Issue #1107: `StartConversation player` in the end script of a Goodbye line starts
/// the new conversation once the old one has closed (`Game::hold_force_greet`).
#[test]
fn a_goodbye_lines_end_script_starts_the_next_conversation_after_it_closes() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.start_dialogue(actor(), Some(FormId(T_HANDOFF))), Ok(FormId(H_HANDOFF)));
    listen(&mut g);
    assert_eq!(g.dialogue_actor(), Some(FormId(NPC2_REF)));
    // The old conversation ended (Goodbye) before the new one started, with the second
    // actor's greeting.
    let order: Vec<(&str, u32)> = g
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::DialogueEnded { actor, .. } => Some(("ended", actor.0)),
            EventKind::DialogueStarted { actor, .. } => Some(("started", actor.0)),
            _ => None,
        })
        .collect();
    assert_eq!(order, vec![("started", NPC1_REF), ("ended", NPC1_REF), ("started", NPC2_REF)]);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueEnded { reason: "goodbye", .. })));
    let c = g.dialogue.as_ref().unwrap();
    assert!(c.then.is_none() && !c.ending);
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. })));
}

/// When the line does not close the conversation the force greet is refused, as any
/// StartConversation while a conversation runs, and does not come back later.
#[test]
fn a_force_greet_from_a_line_that_goes_on_is_refused() {
    let f = fixture();
    let mut g = game(&f);
    g.start_dialogue(actor(), Some(FormId(T_HANDOFF_STAY))).unwrap();
    listen(&mut g);
    assert_eq!(g.dialogue_actor(), Some(actor()));
    assert_eq!(offered(&g), vec![T_BYE]);
    assert!(g.dialogue.as_ref().unwrap().then.is_none());
    pick(&mut g, T_BYE);
    listen(&mut g);
    assert!(g.dialogue.is_none());
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueStarted { actor, .. } if actor == FormId(NPC2_REF))));
}

/// Issue #964: a line with both choices and a Goodbye follow-up offers its choices (the
/// follow-up would leave them unreachable); a follow-up that is no Goodbye line is said
/// first, as before, and offers its own choices.
#[test]
fn a_lines_choices_pass_over_a_goodbye_follow_up() {
    let f = fixture();
    let started = |g: &Game| -> Vec<u32> { g.events.iter().filter_map(|e| if let EventKind::LineStarted { info, .. } = e.kind { Some(info.0) } else { None }).collect() };
    let mut g = game(&f);
    g.start_dialogue(actor(), Some(FormId(T_BOTH))).unwrap();
    listen(&mut g);
    assert_eq!(started(&g), vec![W_BOTH]);
    assert_eq!(offered(&g), vec![T_ASK, T_BYE]);
    let mut h = game(&f);
    h.start_dialogue(actor(), Some(FormId(T_KEEP))).unwrap();
    listen(&mut h);
    assert_eq!(started(&h), vec![W_KEEP, W_BOTH_FU]);
    assert_eq!(offered(&h), vec![T_BYE]);
}

/// A fight breaking out while an actor says its Goodbye greeting at the player ends the
/// line at once as if said to the end: its end script runs, its Add Topics apply, and the
/// actor is no longer held for it (`Game::greets_player`).
#[test]
fn a_goodbye_greeting_ends_as_said_when_its_speaker_fights() {
    let f = fixture();
    let mut g = game(&f);
    set_global(&mut g, G_GATE, 7.0);
    g.frames(3);
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_BARK)));
    g.frames(2);
    assert!(g.greets_player(actor()) && out(&g) == 3.0);
    g.start_combat(actor(), FormId::PLAYER_REF);
    assert!(!g.greets_player(actor()));
    g.frames(1);
    assert!(g.speech.is_empty());
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { info, cut: false, .. } if info == FormId(A_BARK))));
    assert_eq!(out(&g), 4.0);
    assert!(g.player.topics.contains(&FormId(T_ADDED)));
    // Its speaker disabled mid-line: the same, the line ends as said.
    let mut h = game(&f);
    set_global(&mut h, G_GATE, 7.0);
    h.frames(3);
    assert_eq!(h.start_dialogue(actor(), None), Ok(FormId(A_BARK)));
    h.frames(2);
    h.world.set_enabled(actor(), false).unwrap();
    h.frames(1);
    assert!(h.speech.is_empty() && out(&h) == 4.0 && h.player.topics.contains(&FormId(T_ADDED)));
    assert!(!h.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { cut: true, .. })));
}

#[test]
fn a_goodbye_greeting_is_said_at_the_player_without_a_conversation() {
    let f = fixture();
    let mut g = game(&f);
    set_global(&mut g, G_GATE, 7.0);
    let ticks = |g: &Game| (g.world.global(FormId(G_TICKS)).unwrap(), g.world.global(FormId(G_MENU_TICKS)).unwrap());
    g.frames(3);
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_BARK)));
    // No conversation: no menu mode, no pause, nothing to choose or to leave.
    assert!(g.dialogue.is_none() && !g.paused() && !g.in_menu() && !g.menu_open(1009));
    assert!(g.dialogue_choices().is_empty() && !g.end_dialogue());
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueStarted { .. })));
    assert_eq!(call(&mut g, 0x1024, NPC1_REF, &[ArgValue::Value(Value::Num(1009.0))]), 0.0);
    // The actor says the line at the player; its begin script ran.
    assert!(matches!(g.speech.as_slice(), [s] if s.speaker == actor() && s.target == Some(FormId::PLAYER_REF) && s.info == FormId(A_BARK) && s.greeting));
    // The engine holds it, turned to the player, while it says it.
    assert!(g.greets_player(actor()));
    assert_eq!((call(&mut g, 0x108D, NPC1_REF, &[]), call(&mut g, 0x1032, NPC1_REF, &[])), (1.0, 1.0));
    assert_eq!(out(&g), 3.0);
    // The game goes on while it is heard: GameMode blocks run, game time passes.
    let (t0, f0) = (g.time, g.frame);
    for _ in 0..20_000 {
        if g.speech.is_empty() {
            break;
        }
        g.tick();
    }
    let heard = (g.frame - f0) as f32;
    assert!(g.speech.is_empty() && heard > 30.0 && !g.greets_player(actor()));
    assert_eq!(ticks(&g), (3.0 + heard, 0.0));
    assert!(g.time > t0);
    // Its end: the end script and the Add Topics list, and no conversation event.
    assert_eq!(out(&g), 4.0);
    assert!(g.player.topics.contains(&FormId(T_ADDED)));
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { info, cut: false, .. } if info == FormId(A_BARK))));
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueEnded { .. })));
    // Talked to again while it speaks: the line ends as if said (a Goodbye greeting at
    // the player is never cut, `Game::cut_line`) and the greeting starts afresh.
    g.start_dialogue(actor(), None).unwrap();
    g.frames(2);
    let said = |g: &Game| g.events.iter().filter(|e| matches!(e.kind, EventKind::LineEnded { info, cut: false, .. } if info == FormId(A_BARK))).count();
    let before = said(&g);
    assert_eq!(g.start_dialogue(actor(), None), Ok(FormId(A_BARK)));
    assert_eq!(g.speech.len(), 1);
    assert_eq!(said(&g), before + 1);
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { info, cut: true, .. } if info == FormId(A_BARK))));
    // StartConversation without a topic is the same greeting, but the actor starts it:
    // a force greet holds the player in a conversation until the line is over, and the
    // Goodbye flag ends it then (`Game::force_greet`).
    let mut h = game(&f);
    set_global(&mut h, G_GATE, 7.0);
    call(&mut h, 0x1056, NPC1_REF, &[ArgValue::Value(Value::Ref(FormId::PLAYER_REF.0))]);
    assert!(h.dialogue.as_ref().is_some_and(|c| c.line.as_ref().is_some_and(|l| l.info == FormId(A_BARK))) && h.speech.is_empty());
    assert!(h.in_menu() && h.menu_open(1009));
    for _ in 0..20_000 {
        if h.dialogue.is_none() {
            break;
        }
        h.tick();
    }
    assert!(h.dialogue.is_none());
    assert!(h.events.iter().any(|e| matches!(e.kind, EventKind::DialogueEnded { reason: "goodbye", lines: 1, .. })));
    assert_eq!(out(&h), 4.0);
    // So does a force greet by a Dialogue package (`fnv_ai`).
    let mut m = game(&f);
    set_global(&mut m, G_GATE, 7.0);
    assert_eq!(m.force_greet(actor(), None), Ok(FormId(A_BARK)));
    assert!(m.dialogue.is_some() && m.speech.is_empty());
    // A greeting without the flag is a conversation as before.
    let mut k = game(&f);
    k.start_dialogue(actor(), None).unwrap();
    assert!(k.dialogue.is_some() && k.speech.is_empty());
}

#[test]
fn leaving_cuts_the_line_without_its_end_script() {
    let f = fixture();
    let mut g = game(&f);
    g.start_dialogue(actor(), None).unwrap();
    g.frames(3);
    assert!(g.end_dialogue());
    assert!(!g.end_dialogue());
    g.frames(600);
    assert_eq!(out(&g), 1.0);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { cut: true, .. })));
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueEnded { reason: "left", .. })));
    // An actor that is gone ends it as well.
    let mut h = game(&f);
    h.start_dialogue(actor(), None).unwrap();
    h.extra.entry(actor()).or_default().dead = true;
    h.tick();
    assert!(h.dialogue.is_none());
    assert!(h.start_dialogue(actor(), None).is_err());
}

#[test]
fn skipping_a_response_ends_it_early_and_the_line_still_runs_its_end_script() {
    let f = fixture();
    let mut g = game(&f);
    // The first greeting has two responses ("First" then "Second.") and both scripts.
    g.start_dialogue(actor(), None).unwrap();
    g.frames(3);
    assert_eq!((out(&g), g.dialogue_emotion(actor())), (1.0, 3));
    // Skipping the first response starts the second with its full length.
    assert!(g.skip_response());
    let line = g.dialogue.as_ref().and_then(|c| c.line.clone()).unwrap();
    assert_eq!((line.response, line.remaining), (1, fnv_game::game::fitted_seconds(7)));
    assert_eq!((out(&g), g.dialogue_emotion(actor())), (1.0, 5));
    // Skipping the last one ends the line as if it had been said: the end script ran,
    // the line is not cut and its choices are offered, with no frame in between.
    let frame = g.frame;
    assert!(g.skip_response());
    assert_eq!((g.frame, out(&g)), (frame, 2.0));
    assert_eq!(offered(&g), vec![T_ASK, T_BYE]);
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::LineEnded { cut: true, .. })));
    let said: Vec<u8> = g.events.iter().filter_map(|e| if let EventKind::ResponseSaid { number, .. } = e.kind { Some(number) } else { None }).collect();
    assert_eq!(said, vec![1, 2]);
    // Nothing to skip while the player chooses.
    assert!(!g.skip_response());
    // A skipped Goodbye line ends the conversation, end script included.
    pick(&mut g, T_BYE);
    assert!(g.skip_response());
    assert!(g.dialogue.is_none());
    assert_eq!(out(&g), 9.0);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::DialogueEnded { reason: "goodbye", lines: 2, .. })));
    assert!(!g.skip_response());
    // A menu holds the conversation: nothing is skipped under it.
    let mut h = game(&f);
    h.start_dialogue(actor(), None).unwrap();
    h.menu = Some(1001);
    assert!(!h.skip_response());
    assert_eq!(h.dialogue.as_ref().and_then(|c| c.line.as_ref()).map(|l| l.response), Some(0));
}

#[test]
fn random_lines_form_a_stack_from_which_one_is_picked() {
    let f = fixture();
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..24 {
        let mut g = game_seeded(&f, seed);
        let line = g.start_dialogue(FormId(NPC2_REF), None).unwrap();
        // The line after the Random End is never reached.
        assert!(line == FormId(R1) || line == FormId(R2), "{line}");
        seen.insert(line);
    }
    assert_eq!(seen.len(), 2);
}

#[test]
fn run_immediately_runs_both_results_when_the_line_is_picked() {
    let f = fixture();
    let mut g = at_topic_list(&f);
    assert_eq!(pick(&mut g, T_TOP2), FormId(E_NOW));
    assert_eq!(out(&g), 21.0);
    set_global(&mut g, G_OUT, 0.0);
    listen(&mut g);
    assert_eq!(out(&g), 0.0);
    // The entry is now shaded: the actor has said that line.
    let c = g.dialogue_choices().iter().find(|c| c.topic == FormId(T_TOP2)).unwrap();
    assert!(c.darkened);
    assert!(!g.dialogue_choices().iter().find(|c| c.topic == FormId(T_SKILL)).unwrap().darkened);
}

#[test]
fn say_once_a_day_lines_come_back_after_the_timer() {
    let f = fixture();
    let mut g = at_topic_list(&f);
    assert_eq!(pick(&mut g, T_TOP), FormId(E_DAY));
    listen(&mut g);
    assert!(!offered(&g).contains(&T_TOP));
    // 11 game hours later it is still held back, 12 hours later it is offered again.
    set_global(&mut g, G_DAYS, 11.0 / 24.0);
    pick(&mut g, T_TOP2);
    listen(&mut g);
    assert!(!offered(&g).contains(&T_TOP));
    set_global(&mut g, G_DAYS, 0.5);
    pick(&mut g, T_TOP2);
    listen(&mut g);
    assert!(offered(&g).contains(&T_TOP));
    // Another actor's count is its own.
    assert!(g.said_day[&actor()].contains_key(&FormId(E_DAY)));
}

#[test]
fn a_skill_line_is_tagged_and_routed_by_its_conditions() {
    let f = fixture();
    let mut g = at_topic_list(&f);
    let c = g.dialogue_choices().iter().find(|c| c.topic == FormId(T_SKILL)).cloned().unwrap();
    let ch = c.challenge.clone().unwrap();
    let have = ch.have.unwrap();
    assert_eq!((ch.form, ch.needed, ch.difficulty), (FormId(AVIF), Some(35.0), None));
    assert_eq!(c.info, if have >= 35.0 { FormId(H_PASS) } else { FormId(H_FAIL) });
    // The other side of the threshold gives the other line.
    let other = if have >= 35.0 { 10.0 } else { 60.0 };
    fnv_game::functions::set_actor_value_current(&mut g, FormId::PLAYER_REF, SPEECH as u16, other).unwrap();
    pick(&mut g, T_TOP2);
    listen(&mut g);
    let c = g.dialogue_choices().iter().find(|c| c.topic == FormId(T_SKILL)).cloned().unwrap();
    assert_eq!(c.info, if other >= 35.0 { FormId(H_PASS) } else { FormId(H_FAIL) });
    assert_eq!(c.challenge.unwrap().have, Some(other));
}

/// A New Vegas skill line gives no XP of its own (its result script would); a Fallout 3
/// style speech challenge (the flag and a difficulty) gives `iXPRewardSpeechChallenge
/// <class>` when it is said, nothing for a class no setting gives
/// (docs/spec-notes/experience.md §2).
#[test]
fn a_speech_challenge_gives_its_xp_and_a_skill_line_none() {
    let mut f = fixture();
    let skill_xp = {
        let mut g = at_topic_list(&f);
        fnv_game::functions::set_actor_value_current(&mut g, FormId::PLAYER_REF, SPEECH as u16, 60.0).unwrap();
        pick(&mut g, T_TOP2);
        listen(&mut g);
        assert_eq!(pick(&mut g, T_SKILL), FormId(H_PASS));
        g.player.xp
    };
    assert_eq!(skill_xp, 0.0);
    let mut challenge = |difficulty: u32| {
        let line = f.data.infos.get_mut(&FormId(H_PASS)).unwrap();
        line.flags |= info_flag::SPEECH_CHALLENGE;
        line.challenge_difficulty = Some(difficulty);
        let mut g = at_topic_list(&f);
        fnv_game::functions::set_actor_value_current(&mut g, FormId::PLAYER_REF, SPEECH as u16, 60.0).unwrap();
        pick(&mut g, T_TOP2);
        listen(&mut g);
        assert_eq!(pick(&mut g, T_SKILL), FormId(H_PASS));
        g.player.xp
    };
    // Very Easy: the master's 5 (the default here, the test plugin sets none).
    assert_eq!(challenge(1), 5.0);
    // Hard (the masters' three lines): no setting, nothing.
    assert_eq!(challenge(4), 0.0);
}

/// A New Vegas Speech check counts a Speech Success when its pass line is chosen and a
/// Speech Failure for its fail line (stats 12 and 31), with no XP of its own; a check
/// of another skill counts neither (#967; misc-stats.md §3).
#[test]
fn a_speech_check_counts_a_success_or_a_failure() {
    use fnv_game::misc_stats::{SPEECH_FAILURES, SPEECH_SUCCESSES};
    let f = fixture();
    let run = |speech: f64| {
        let mut g = at_topic_list(&f);
        fnv_game::functions::set_actor_value_current(&mut g, FormId::PLAYER_REF, SPEECH as u16, speech).unwrap();
        pick(&mut g, T_TOP2);
        listen(&mut g);
        assert_eq!(g.dialogue_choices().iter().find(|c| c.topic == FormId(T_SKILL)).unwrap().challenge.as_ref().unwrap().skill, Some(SPEECH as u16));
        let line = pick(&mut g, T_SKILL);
        (line, g.misc_stat(SPEECH_SUCCESSES), g.misc_stat(SPEECH_FAILURES), g.player.xp)
    };
    assert_eq!(run(60.0), (FormId(H_PASS), 1.0, 0.0, 0.0));
    assert_eq!(run(35.0), (FormId(H_PASS), 1.0, 0.0, 0.0));
    assert_eq!(run(10.0), (FormId(H_FAIL), 0.0, 1.0, 0.0));
    // The same pair testing another skill (Barter, 32): nothing counted.
    let mut f = f;
    for id in [H_PASS, H_FAIL] {
        let line = f.data.infos.get_mut(&FormId(id)).unwrap();
        line.conditions[0].params = vec![ArgValue::Enum(32)];
    }
    let mut g = at_topic_list(&f);
    pick(&mut g, T_TOP2);
    listen(&mut g);
    pick(&mut g, T_SKILL);
    assert_eq!((g.misc_stat(SPEECH_SUCCESSES), g.misc_stat(SPEECH_FAILURES)), (0.0, 0.0));
}

#[test]
fn start_conversation_and_the_dialogue_functions() {
    let f = fixture();
    let mut g = game(&f);
    let player = ArgValue::Value(Value::Ref(FormId::PLAYER_REF.0));
    assert_eq!(call(&mut g, 0x1032, NPC1_REF, &[]), 0.0);
    // StartConversation player: the greeting at once.
    call(&mut g, 0x1056, NPC1_REF, std::slice::from_ref(&player));
    assert_eq!(g.dialogue_actor(), Some(actor()));
    assert_eq!(call(&mut g, 0x1032, NPC1_REF, &[]), 1.0);
    assert_eq!(call(&mut g, 0x108D, NPC1_REF, &[]), 1.0);
    assert_eq!(call(&mut g, 0x11B3, NPC1_REF, &[]), 3.0);
    // A second call while it runs does nothing.
    let lines = g.dialogue.as_ref().unwrap().lines;
    call(&mut g, 0x1056, NPC2_REF, std::slice::from_ref(&player));
    assert_eq!((g.dialogue_actor(), g.dialogue.as_ref().unwrap().lines), (Some(actor()), lines));
    assert!(!g.events.iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. })));
    listen(&mut g);
    assert_eq!(call(&mut g, 0x108D, NPC1_REF, &[]), 0.0);
    // With a topic: that topic instead of the greeting.
    let mut h = game(&f);
    call(&mut h, 0x1056, NPC1_REF, &[player.clone(), ArgValue::Value(Value::Ref(T_BYE))]);
    assert_eq!(h.dialogue.as_ref().unwrap().line.as_ref().unwrap().info, FormId(C_BYE));
    // Two NPCs: no conversation with the player (`two_actors_talk_to_each_other`); with no
    // topic the speaker starts with HELLO, the first line whose Link From is blank.
    let mut k = game(&f);
    call(&mut k, 0x1056, NPC1_REF, &[ArgValue::Value(Value::Ref(NPC2_REF))]);
    assert!(k.dialogue.is_none());
    assert_eq!(k.speech.iter().map(|s| (s.info.0, s.conversation)).collect::<Vec<_>>(), vec![(K_HELLO, Some(1))]);
    assert!(!k.events.iter().any(|e| matches!(&e.kind, EventKind::Stub { .. } | EventKind::CallFailed { .. })));
    // AddTopic: the topic is offered in the topic list.
    let mut a = game(&f);
    call(&mut a, 0x1058, NPC1_REF, &[ArgValue::Value(Value::Ref(T_ADDED))]);
    a.said.insert((actor(), FormId(A_FIRST)));
    a.start_dialogue(actor(), None).unwrap();
    listen(&mut a);
    assert!(offered(&a).contains(&T_ADDED));
}

#[test]
fn two_actors_talk_to_each_other() {
    let f = fixture();
    let mut g = game(&f);
    let (a, b) = (FormId(NPC1_REF), FormId(NPC2_REF));
    let r = |id: u32| ArgValue::Value(Value::Ref(id));
    call(&mut g, 0x1056, NPC1_REF, &[r(NPC2_REF), r(T_ARGUE)]);
    // No menu: the player only overhears; the first line's begin script ran.
    assert!(g.dialogue.is_none() && !g.in_menu());
    assert!(g.in_actor_conversation(a) && g.in_actor_conversation(b));
    assert_eq!(out(&g), 30.0);
    let s = g.speech[0].clone();
    assert_eq!((s.speaker, s.target, s.info, s.conversation), (a, Some(b), FormId(K_OPEN), Some(1)));
    // Only the first response is said.
    assert_eq!(s.remaining, g.response_seconds(a, FormId(K_OPEN), 0));
    assert!(s.remaining < g.line_seconds(a, FormId(K_OPEN)));
    // Either side starting another conversation meanwhile is refused.
    call(&mut g, 0x1056, NPC2_REF, &[r(NPC1_REF), r(T_ARGUE)]);
    let started = |g: &Game| g.events.iter().filter(|e| matches!(e.kind, EventKind::ActorConversationStarted { .. })).count();
    assert_eq!(started(&g), 1);
    for _ in 0..20_000 {
        if g.speech.is_empty() {
            break;
        }
        g.tick();
    }
    // The reply comes from the second actor (next speaker Target), only from the topic it
    // can say; then the same actor again (Self) with the Goodbye line, which ends it.
    let lines: Vec<(FormId, Option<FormId>, u32)> =
        g.events.iter().filter_map(|e| if let EventKind::LineStarted { speaker, target, info, .. } = e.kind { Some((speaker, target, info.0)) } else { None }).collect();
    assert_eq!(lines, vec![(a, Some(b), K_OPEN), (b, Some(a), K_REPLY), (b, Some(a), K_LAST)]);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::ActorConversationEnded { speaker, target, reason: "goodbye", lines: 3 } if speaker == b && target == a)));
    assert_eq!(out(&g), 32.0);
    assert!(!g.in_actor_conversation(a) && g.dialogue.is_none());
}

#[test]
fn actors_talk_to_each_other_only_with_conversation_lines() {
    let f = fixture();
    let mut g = game(&f);
    let (a, b) = (FormId(NPC1_REF), FormId(NPC2_REF));
    // The Topic line for the player can be said now, and is stored before the
    // Conversation line.
    set_global(&mut g, G_GATE, 5.0);
    assert_eq!(g.conversation_opener(a, b), Some(FormId(K_HELLO)));
    call(&mut g, 0x1056, NPC1_REF, &[ArgValue::Value(Value::Ref(NPC2_REF))]);
    assert_eq!(g.speech.iter().map(|s| s.info.0).collect::<Vec<_>>(), vec![K_HELLO]);
    for _ in 0..20_000 {
        if g.speech.is_empty() {
            break;
        }
        g.tick();
    }
    // The reply skips the Topic line its topic holds for the player.
    let said: Vec<u32> = g.events.iter().filter_map(|e| if let EventKind::LineStarted { info, .. } = e.kind { Some(info.0) } else { None }).collect();
    assert_eq!(said, vec![K_HELLO, K_REPLY, K_LAST]);
    // A hello to the player takes the Topic line.
    assert_eq!(g.say_hello(a), Some(FormId(K_HELLO_PLAYER)));
}

#[test]
fn between_actors_a_link_from_reply_answers_a_line_that_links_nowhere() {
    let f = fixture();
    let mut g = game(&f);
    let (a, b) = (FormId(NPC1_REF), FormId(NPC2_REF));
    let r = |id: u32| ArgValue::Value(Value::Ref(id));
    call(&mut g, 0x1056, NPC1_REF, &[r(NPC2_REF), r(T_CHAT)]);
    for _ in 0..20_000 {
        if g.speech.is_empty() {
            break;
        }
        g.tick();
    }
    // The answer is the line that names TestChat, not the topic's first line.
    let lines: Vec<(FormId, u32)> = g.events.iter().filter_map(|e| if let EventKind::LineStarted { speaker, info, .. } = e.kind { Some((speaker, info.0)) } else { None }).collect();
    assert_eq!(lines, vec![(a, C_CHAT), (b, C_REPLY)]);
    assert!(g.events.iter().any(|e| matches!(e.kind, EventKind::ActorConversationEnded { reason: "goodbye", lines: 2, .. })));
}

#[test]
fn a_hello_to_the_player_is_one_line() {
    let f = fixture();
    let mut g = game(&f);
    let a = FormId(NPC1_REF);
    // The line with a Link From is passed over; the hello is no conversation and no menu.
    assert_eq!(g.say_hello(a), Some(FormId(K_HELLO)));
    assert!(g.dialogue.is_none() && !g.in_menu() && !g.in_actor_conversation(a));
    let s = g.speech[0].clone();
    assert_eq!((s.speaker, s.target, s.hello, s.conversation), (a, Some(FormId::PLAYER_REF), true, None));
    for _ in 0..20_000 {
        if g.speech.is_empty() {
            break;
        }
        g.tick();
    }
    // Its links are not followed: nobody answers.
    let lines = g.events.iter().filter(|e| matches!(e.kind, EventKind::LineStarted { .. })).count();
    assert_eq!(lines, 1);
    // An actor with no HELLO line says nothing.
    assert_eq!(g.say_hello(FormId(NPC2_REF)), None);
}

#[test]
fn say_to_uses_the_same_selection() {
    let f = fixture();
    let mut g = game(&f);
    set_global(&mut g, G_GATE, 5.0);
    assert_eq!(g.say(actor(), Some(FormId::PLAYER_REF), FormId(GREETING), true), Some(FormId(A_HIGH)));
    // A SayTo line is no conversation.
    assert!(g.dialogue.is_none());
    assert_eq!(call(&mut g, 0x1032, NPC1_REF, &[]), 0.0);
}

#[test]
fn lines_said_to_someone_turn_the_two_to_each_other() {
    let f = fixture();
    let (a, b, player) = (FormId(NPC1_REF), FormId(NPC2_REF), FormId::PLAYER_REF);
    let r = |id: u32| ArgValue::Value(Value::Ref(id));
    let n = |x: f64| ArgValue::Value(Value::Num(x));
    let turns = |g: &Game| g.facings().iter().map(|f| (f.who, f.to, f.body, f.speaking)).collect::<Vec<_>>();
    // SayTo: the speaker turns head and body, the target looks back (head only).
    let mut g = game(&f);
    set_global(&mut g, G_GATE, 5.0);
    call(&mut g, 0x1034, NPC1_REF, &[r(player.0), r(GREETING)]);
    assert_eq!(turns(&g), vec![(a, player, true, true), (player, a, false, false)]);
    assert!(g.facings().iter().all(|f| f.info == FormId(A_HIGH)));
    // noTargetLookFlag 1: the target is not made to look; the speaker still turns.
    let mut g = game(&f);
    set_global(&mut g, G_GATE, 5.0);
    call(&mut g, 0x1034, NPC1_REF, &[r(player.0), r(GREETING), n(0.0), n(1.0)]);
    assert_eq!(turns(&g), vec![(a, player, true, true)]);
    // Say has no listener: nobody turns.
    let mut g = game(&f);
    set_global(&mut g, G_GATE, 5.0);
    call(&mut g, 0x1033, NPC1_REF, &[r(GREETING)]);
    assert!(g.is_talking(a) && turns(&g).is_empty());
    // A conversation between actors: both turn head and body, line after line.
    let mut g = game(&f);
    call(&mut g, 0x1056, NPC1_REF, &[r(NPC2_REF), r(T_ARGUE)]);
    assert_eq!(turns(&g), vec![(a, b, true, true), (b, a, true, false)]);
    // headTrack 0: nobody turns, in the replies too.
    let mut g = game(&f);
    call(&mut g, 0x1056, NPC1_REF, &[r(NPC2_REF), r(T_ARGUE), r(0), r(0), n(0.0)]);
    assert!(g.in_actor_conversation(a) && turns(&g).is_empty());
    let first = g.speech[0].info;
    for _ in 0..20_000 {
        if g.speech.first().is_none_or(|s| s.info != first) {
            break;
        }
        g.tick();
    }
    assert_eq!(g.speech.first().map(|s| s.speaker), Some(b), "the reply is said");
    assert!(turns(&g).is_empty());
}

#[test]
fn a_conversation_survives_a_snapshot() {
    let f = fixture();
    let mut g = at_topic_list(&f);
    pick(&mut g, T_TOP);
    g.frames(5);
    let snap = g.snapshot();
    let text = serde_json::to_string(&snap).unwrap();
    let back: GameSnapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(back, snap);
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let mut h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    assert_eq!(h.dialogue, g.dialogue);
    assert_eq!(h.said_day, g.said_day);
    listen(&mut g);
    listen(&mut h);
    assert_eq!(offered(&h), offered(&g));
    assert_eq!(h.frame, g.frame);
}

#[test]
fn the_runner_talks_and_prints_a_dialogue_trace() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.dialogue.actions/1",
        "actions": [
            {"op": "talk", "actor": "TalkerREF", "label": "greeting"},
            {"op": "wait_talk", "label": "first list"},
            {"op": "choose", "prompt": "tell me", "label": "asked"},
            {"op": "choose", "topic": "TopicNow"},
            {"op": "choose", "index": 4, "label": "bye"},
            {"op": "wait_talk", "label": "over"},
            {"op": "choose", "index": 0, "label": "too late"}
        ]
    }))
    .unwrap();
    let t = fnv_game::runner::run_dialogue(&f.data, &f.forms, &f.refs, &input, false).unwrap();
    assert_eq!(t["format"], "fnv.dialogue.trace/1");
    let obs = |label: &str| t["observations"].as_array().unwrap().iter().find(|o| o["label"] == label).cloned().unwrap();
    assert_eq!(obs("greeting")["info"], FormId(A_FIRST).to_string());
    assert_eq!((obs("first list")["state"].as_str(), obs("first list")["choices"].as_u64()), (Some("choosing"), Some(2)));
    assert_eq!(obs("asked")["info"], FormId(B_ASK).to_string());
    assert_eq!(obs("bye")["info"], FormId(I_BYE).to_string());
    assert_eq!(obs("over")["state"], "ended");
    assert!(obs("too late")["errors"]["action"].as_str().unwrap().contains("no choices"));
    let kinds: Vec<&str> = t["events"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    for k in ["dialogue_started", "line_started", "response_said", "result_script", "line_ended", "choices_offered", "choice_made", "topic_added", "dialogue_ended"] {
        assert!(kinds.contains(&k), "{k} missing in {kinds:?}");
    }
    // Lengths, not texts, unless asked.
    let line = t["events"].as_array().unwrap().iter().find(|e| e["kind"] == "line_started").unwrap();
    assert_eq!((line["chars"].as_u64(), line["topic_edid"].as_str(), line["responses"].as_u64()), (Some(12), Some("GREETING"), Some(2)));
    assert!(!t.to_string().contains("Second."));
    let with_text = fnv_game::runner::run_dialogue(&f.data, &f.forms, &f.refs, &input, true).unwrap();
    assert!(with_text.to_string().contains("Second.") && with_text.to_string().contains("Tell me"));
    assert_eq!(t["final"]["dialogue"], Json::Null);
    // The same document is not a script actions document.
    assert!(fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).is_err());
}

/// The runner's `wait_talk` after a Goodbye greeting (no conversation opens) waits for
/// the line to end, so its end script has run before the next step (issue #1114: a
/// following conversation used to cut Carlitos's and Joana's lines); `end_talk` with no
/// conversation running does nothing.
#[test]
fn the_runners_wait_talk_hears_a_goodbye_greeting_out_and_end_talk_after_the_end_does_nothing() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "set_global", "name": "TestGate", "value": 7},
            {"op": "talk", "actor": "TalkerREF", "label": "bark"},
            {"op": "get_global", "name": "TestOut", "label": "begun"},
            {"op": "wait_talk", "label": "heard"},
            {"op": "get_global", "name": "TestOut", "label": "after"},
            {"op": "end_talk"},
            {"op": "get_global", "name": "TestOut", "label": "still"}
        ]
    }))
    .unwrap();
    let t = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let all = t["observations"].as_array().unwrap();
    let obs = |label: &str| all.iter().find(|o| o["label"] == label).cloned().unwrap();
    assert_eq!(obs("bark")["info"], FormId(A_BARK).to_string());
    assert_eq!(obs("begun")["value"], 3.0);
    assert_eq!(obs("heard")["state"], "ended");
    // The end script ran during the wait.
    assert_eq!(obs("after")["value"], 4.0);
    assert_eq!(obs("still")["value"], 4.0);
    assert!(all.iter().all(|o| o.get("errors").is_none()), "{all:?}");
}

/// The runner's `player_activate` of an actor ends in the default action of talking
/// (`Outcome::Talk`), and the conversation opens with the greeting, as the play loop's
/// activation does (nvf-play `talk_after`).
#[test]
fn the_runners_player_activation_of_an_actor_opens_the_conversation() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.dialogue.actions/1",
        "actions": [
            {"op": "player_activate", "target": "TalkerREF", "label": "activated"},
            {"op": "wait_talk", "label": "list"}
        ]
    }))
    .unwrap();
    let t = fnv_game::runner::run_dialogue(&f.data, &f.forms, &f.refs, &input, false).unwrap();
    let obs = |label: &str| t["observations"].as_array().unwrap().iter().find(|o| o["label"] == label).cloned().unwrap();
    assert_eq!(obs("activated")["info"], FormId(A_FIRST).to_string());
    assert_eq!((obs("list")["state"].as_str(), obs("list")["choices"].as_u64()), (Some("choosing"), Some(2)));
}

/// "If this box is unchecked, conversations will never use the same topic twice (in the
/// same conversation)" [documented: GECK wiki Quest Data Tab]: ping-pong in a quest
/// without Allow Repeated Conversation Topics stops after the pong; in one with it, it
/// goes on.
#[test]
fn a_conversation_uses_a_topic_once_unless_its_quest_allows_repeats() {
    let f = fixture();
    let r = |id: u32| ArgValue::Value(Value::Ref(id));
    let lines = |g: &Game| g.events.iter().filter(|e| matches!(e.kind, EventKind::LineStarted { .. })).count();
    let mut g = game(&f);
    call(&mut g, 0x1056, NPC1_REF, &[r(NPC2_REF), r(T_PING)]);
    for _ in 0..20_000 {
        if g.speech.is_empty() {
            break;
        }
        g.tick();
    }
    assert_eq!(lines(&g), 2, "ping, pong, and no second ping");
    let mut g = game(&f);
    call(&mut g, 0x1056, NPC1_REF, &[r(NPC2_REF), r(T_RPING)]);
    for _ in 0..2_000 {
        g.tick();
    }
    assert!(lines(&g) >= 4, "the repeating quest's ping-pong goes on: {} lines", lines(&g));
}

/// Playing a Voice note runs its line's result scripts: the begin script as it starts,
/// the end script as it ends (`Game::holotape_script`; Dead Money's Dog command tape is
/// how its quest reaches NVDLC01MQ01a 40, docs/spec-notes/holotapes.md §3). The runner's
/// `play_note` runs both.
#[test]
fn a_voice_note_runs_its_lines_result_scripts() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(g.holotape_script(FormId(NOTE_TAPE), 0), Some(FormId(S_TAPE)));
    assert_eq!(out(&g), 6.0, "the begin script");
    assert_eq!(g.holotape_script(FormId(NOTE_TAPE), 1), Some(FormId(S_TAPE)));
    assert_eq!(out(&g), 7.0, "the end script");
    // A topic changed while the tape plays: it ends with the line it began.
    assert_eq!(g.holotape_script(FormId(NOTE_TAPE), 0), Some(FormId(S_TAPE)));
    g.run_query("Begin Function { }
SetNoteTopic TestTapeNote TestPing
End
", Default::default()).unwrap();
    assert_eq!(g.holotape_script(FormId(NOTE_TAPE), 1), Some(FormId(S_TAPE)));
    // Not a voice note: nothing runs.
    assert_eq!(g.holotape_script(FormId(G_OUT), 0), None);
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "actions": [{"op": "play_note", "note": "TestTapeNote", "label": "tape"}, {"op": "get_global", "name": "TestOut", "label": "out"}]
    }))
    .unwrap();
    let t = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let obs = t["observations"].as_array().unwrap();
    assert_eq!(obs[0]["info"], json!(FormId(S_TAPE).to_string()));
    assert_eq!(obs[1]["value"], json!(7.0));
}

/// The runner's `patrol_point` stands in for an actor walking to a patrol point: the
/// point's result script runs (`Game::patrol_point_reached`; Dead Money's vault path
/// marker sets bElijahArrivesVault this way).
#[test]
fn the_runner_reaches_a_patrol_point() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(json!({
        "format": "fnv.script.actions/1",
        "actions": [{"op": "patrol_point", "actor": "TalkerREF", "point": "PatrolREF"}, {"op": "get_global", "name": "TestOut", "label": "out"}]
    }))
    .unwrap();
    let t = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    assert_eq!(t["steps"][0].get("error"), None, "{}", t["steps"][0]);
    assert_eq!(t["observations"][0]["value"], json!(9.0));
}

/// The extender's note functions (#984): the getters read the record, the setters change
/// the note for the session, and the read flag is saved (docs/spec-notes/holotapes.md §5).
#[test]
fn note_functions_read_and_change_a_note() {
    use fnv_script::vm::XVal;
    let f = fixture();
    let mut g = game(&f);
    let q = |g: &mut Game, body: &str| g.run_query(&format!("array_var aQ
Begin Function {{ }}
{body}
End
"), fnv_game::game::RunCtx::default());
    let n = |v: f64| Ok(XVal::Num(v));
    assert_eq!(q(&mut g, "SetFunctionValue (GetNoteType TestTapeNote)"), n(3.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetNoteSpeaker TestTapeNote)"), Ok(XVal::Ref(NPC1)));
    assert_eq!(q(&mut g, "SetFunctionValue (GetNoteTopic TestTapeNote)"), Ok(XVal::Ref(T_TAPE)));
    assert_eq!(q(&mut g, "let aQ := GetNoteQuestList TestTapeNote
SetFunctionValue (Ar_Size aQ) * 10 + (aQ[0] == QA)"), n(11.0));
    // A setter changes the note for the session; the tape it plays follows.
    assert_eq!(q(&mut g, "SetFunctionValue (SetNoteTopic TestTapeNote TestPing)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetNoteTopic TestTapeNote)"), Ok(XVal::Ref(T_PING)));
    assert_eq!(
        g.note_tape(FormId(NOTE_TAPE)),
        Some(fnv_game::holotapes::Tape::Voice { speaker: FormId(NPC1), info: FormId(P_PING) }),
        "the new topic's line that names no speaker"
    );
    assert_eq!(q(&mut g, "SetFunctionValue (AddNoteQuest TestTapeNote QHi) + (AddNoteQuest TestTapeNote QHi) + (RemoveNoteQuest TestTapeNote QA)"), n(3.0));
    assert_eq!(q(&mut g, "let aQ := GetNoteQuestList TestTapeNote
SetFunctionValue (Ar_Size aQ) * 10 + (aQ[0] == QHi)"), n(11.0));
    assert_eq!(q(&mut g, "SetFunctionValue (SetNoteType TestTapeNote 9) * 10 + (SetNoteType Talker 1)"), n(0.0));
    // The read flag: the Pip-Boy marks it (saved); SetNoteRead without serialize lasts the
    // session, with it the save.
    assert_eq!(q(&mut g, "SetFunctionValue (GetNoteRead TestTapeNote)"), n(0.0));
    g.mark_note_read(FormId(NOTE_TAPE));
    assert_eq!(q(&mut g, "SetFunctionValue (GetNoteRead TestTapeNote)"), n(1.0));
    q(&mut g, "SetNoteRead TestTapeNote 0").unwrap();
    assert_eq!(q(&mut g, "SetFunctionValue (GetNoteRead TestTapeNote)"), n(0.0));
    let snap: GameSnapshot = serde_json::from_str(&serde_json::to_string(&g.snapshot()).unwrap()).unwrap();
    assert!(snap.player.notes_read.contains(&FormId(NOTE_TAPE)), "the session change is not in the save");
    q(&mut g, "SetNoteRead TestTapeNote 0 1").unwrap();
    assert!(!g.snapshot().player.notes_read.contains(&FormId(NOTE_TAPE)));
    // The note's one SNAM is read by its type: made a Sound note, the speaker is its
    // sound, for the getter and the tape alike.
    assert_eq!(q(&mut g, "SetFunctionValue (SetNoteType TestTapeNote 0)"), n(1.0));
    assert_eq!(q(&mut g, "SetFunctionValue (GetNoteSound TestTapeNote)"), Ok(XVal::Ref(NPC1)));
    assert_eq!(g.note_tape(FormId(NOTE_TAPE)), Some(fnv_game::holotapes::Tape::Sound(FormId(NPC1))));
}

/// Known topics are kept newest first: one line's Add Topics go to the front together in
/// record order, a line's topic learned again moves to the front, and `AddTopic` on a
/// known topic leaves it in place (owner's choice 2026-10-31, dialogue.md §3.5). Saves
/// holding the old sorted list still load.
#[test]
fn known_topics_newest_first() {
    use fnv_game::game::KnownTopics;
    let ids = |k: &KnownTopics| k.iter().map(|t| t.0).collect::<Vec<_>>();
    let mut k = KnownTopics::default();
    assert_eq!(k.learn(&[FormId(0xC8), FormId(0xC7)]), vec![FormId(0xC8), FormId(0xC7)]);
    assert!(k.insert(FormId(0xCA)));
    assert_eq!(k.learn(&[FormId(0xCC), FormId(0xCB)]).len(), 2);
    assert_eq!(ids(&k), vec![0xCC, 0xCB, 0xCA, 0xC8, 0xC7]);
    // AddTopic again: stays where it is, and not reported as new.
    assert!(!k.insert(FormId(0xC8)));
    assert_eq!(ids(&k), vec![0xCC, 0xCB, 0xCA, 0xC8, 0xC7]);
    // A line's Add Topics again: to the front.
    assert!(k.learn(&[FormId(0xC8)]).is_empty());
    assert_eq!(ids(&k), vec![0xC8, 0xCC, 0xCB, 0xCA, 0xC7]);
    let old: KnownTopics = serde_json::from_value(json!([FormId(1).to_string(), FormId(2).to_string(), FormId(3).to_string()])).unwrap();
    assert_eq!(ids(&old), vec![1, 2, 3]);
}
