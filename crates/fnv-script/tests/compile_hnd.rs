//! Source forms High and Dry's scripts use that the compiler once refused
//! (docs/spec-notes/ext-bytecode.md; batch hnd3, 2026-11-09). Each compiles without
//! errors; the snippets are reduced from the mods' own lines.

use fnv_script::compile::{compile, Env, FormInfo};
use fnv_script::{FunctionTable, VarDecl};

fn table() -> FunctionTable {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    FunctionTable::load(&p).expect("functions.json loads")
}

struct World;

impl Env for World {
    fn form(&self, editor_id: &str) -> Option<FormInfo> {
        let (form, sig) = match editor_id.to_ascii_lowercase().as_str() {
            "testquest" => (0x0100_1000, *b"QUST"),
            "testlist" => (0x0100_4000, *b"FLST"),
            "testfaction" => (0x0100_5000, *b"FACT"),
            _ => return None,
        };
        Some(FormInfo { form, sig })
    }
    fn info(&self, form: u32) -> Option<FormInfo> {
        [("testquest", 0x0100_1000), ("testlist", 0x0100_4000), ("testfaction", 0x0100_5000)].iter().find(|(_, f)| *f == form).and_then(|(n, _)| self.form(n))
    }
    fn editor_id(&self, _form: u32) -> Option<String> {
        None
    }
    fn local_form(&self, local: u32) -> Option<u32> {
        Some(local)
    }
    fn script_vars(&self, _form: u32) -> Option<Vec<VarDecl>> {
        None
    }
}

fn errors(src: &str) -> Vec<String> {
    let t = table();
    let c = compile(src, &t, &World, None);
    c.errors().map(|d| format!("{d:?}")).collect()
}

#[test]
fn a_comma_after_the_function_name() {
    // Unofficial Patch NVSE Plus: `rForm = ListGetNthForm, List, iPos` (an expression
    // statement: the commas separate the arguments).
    let e = errors("ref rForm\nint iPos\nbegin GameMode\nrForm = ListGetNthForm, TestList, iPos\nend");
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn get_active_objectives_takes_a_quest() {
    // Main And Pause Menus Overhaul: `ar_size (GetActiveObjectives QuestREF)`.
    let e = errors("ref QuestREF\nint i\nbegin GameMode\nlet i := ((ar_size (GetActiveObjectives QuestREF)) < 0)\nend");
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn an_arrow_lambda_with_a_call_in_parentheses() {
    // RG_PrimmPlayerHome: `callafterseconds 0 ({} => SetStringSetting "s" (Func "x"))`.
    let e = errors("begin GameMode\ncallafterseconds 0 ({} => SetStringSetting \"sTargetTypeOpen\" (playerRef.AuxiliaryVariableGetString \"*sOriginal\"))\nend");
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn an_arrow_lambda_continued_on_the_next_line() {
    // FreePostMojave: `CallWhen ({} =>`, the body's statements on the lines that follow,
    // closed by `)` before the condition lambda.
    let e = errors("int i\nbegin GameMode\nCallWhen ({} =>\n  set i to 1\n  player.RewardXP 250\n) ({} => MenuMode == 0)\nend");
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn an_optional_parameter_before_a_comparison() {
    // JIP Companions Command & Control: `(rAux.CCCInFaction Faction == 0)`.
    let e = errors("ref rAux\nint i\nbegin GameMode\nif (rAux.CCCInFaction TestFaction == 0) || (rAux.GetFactionRank TestFaction > 9)\nset i to 1\nendif\nend");
    assert!(e.is_empty(), "{e:?}");
}

