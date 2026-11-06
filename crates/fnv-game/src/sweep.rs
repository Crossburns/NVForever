//! The result-script sweep (docs/handoff/mq-walkthrough-2026-10-27.md §7): every quest
//! stage, dialogue line, terminal and package result script of a load order run once,
//! headless and by itself, and the script errors and unknown functions met, grouped by
//! cause.
//!
//! Each script runs in a game of its own, started the way the runner starts one (no
//! new-game quests), on the reference it would run on where the data names one: a placed
//! reference of the line's actor (its own GetIsID actor, else its fixed speaker) for a
//! dialogue line, of the terminal for a terminal entry, of the first placed actor whose AI
//! package list holds the package for a package script; a stage script on its quest. A
//! package only scripts give an actor (AddScriptPackage) has none here, so calls on its
//! implicit reference fail; they are a cause of their own in the summary.
//! What a script does to the state is not checked, only whether it runs: a script that
//! needs the state an earlier scene leaves (a reference variable set, an actor enabled)
//! may fail here and not in play, so the causes are leads, not stalls.
//!
//! A script that never stops calling itself ends at a guard and is a cause like any
//! other: nesting deeper than `crate::game::MAX_DEPTH` is a `script_error`, and a
//! package condition asking for the current package of an actor choosing its first one
//! (GetIsCurrentPackage; `Game::current_package`) is `skipped` (tests/sweep.rs).
//!
//! `arg_unread` (#1122) lists the calls that succeeded although the function could not
//! read an argument they passed (`functions::Unread`): it went on with a default, so the
//! script ran with a value it did not give. They are not counted in `with_problems`.

use crate::coverage::{unit_name, Coverage};
use crate::data::GameData;
use crate::game::{Game, GameConfig, RunCtx};
use crate::trace::EventKind;
use fnv_script::ir::{Arg, Call, Expr, Program, Stmt, Token};
use fnv_script::UnitOwner;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value as Json};
use std::collections::{BTreeMap, HashMap};

pub const FORMAT: &str = "fnv.script.sweep/1";

/// One problem a script met: what kind, and the cause it is grouped under (the
/// function's name and the error with its numbers taken out).
fn cause(kind: &EventKind) -> Option<(&'static str, String)> {
    Some(match kind {
        EventKind::CallFailed { name, error, .. } => ("call_failed", format!("{name}: {}", generic(error))),
        EventKind::ScriptError { error, .. } => ("script_error", generic(error)),
        EventKind::Unhandled { name, .. } => ("unhandled", name.clone()),
        EventKind::Skipped { what, .. } => ("skipped", generic(what)),
        EventKind::ArgUnread { opcode, name, param, got, wanted, .. } => {
            ("arg_unread", format!("{name} ({opcode}) parameter {}: {got} read as {wanted}", param + 1))
        }
        _ => return None,
    })
}

/// An error text with its FormIDs, numbers and quoted names replaced, so that one cause
/// met by many scripts reads the same.
fn generic(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            for d in chars.by_ref() {
                if d == '"' {
                    break;
                }
            }
            out.push_str("\"…\"");
        } else if c.is_ascii_digit() {
            while chars.peek().is_some_and(|d| d.is_ascii_hexdigit() || *d == 'x' || *d == '.') {
                chars.next();
            }
            out.push('#');
        } else {
            out.push(c);
        }
    }
    out
}

/// The first placed reference of each base form.
fn first_refs(refs: &RefTable) -> HashMap<FormId, FormId> {
    let mut m = HashMap::new();
    for (id, p) in &refs.refs {
        m.entry(p.base).or_insert(*id);
    }
    m
}

/// Run the sweep; returns the summary (causes by kind, each with how many scripts met it
/// and a few of them) and the coverage of the scripts run (result scripts only).
pub fn sweep(data: &GameData, forms: &FormStore, refs: &RefTable, samples: usize) -> (Json, Coverage) {
    let lo = &data.load_order;
    let placed = first_refs(refs);
    // Package → the first placed actor whose package list holds it.
    let mut carriers: HashMap<FormId, FormId> = HashMap::new();
    for (id, p) in &refs.refs {
        if &p.sig == b"ACHR" || &p.sig == b"ACRE" {
            for pk in data.base_package_list(p.base, *id) {
                carriers.entry(pk).or_insert(*id);
            }
        }
    }
    // INFO → (quest, actor reference); TERM → its reference.
    let line_ctx = |info: FormId| -> RunCtx {
        let def = data.infos.get(&info);
        let actor = def.and_then(|d| crate::coverage::line_actor(data, d));
        RunCtx { self_ref: actor.and_then(|a| placed.get(&a).copied()), action_ref: None, quest: def.and_then(|d| d.quest) }
    };
    let mut causes: BTreeMap<(&'static str, String), (usize, Vec<String>)> = BTreeMap::new();
    let mut by_kind: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    let mut cov = Coverage::default();
    for (id, u) in data.units.iter().enumerate() {
        if u.program.top.is_empty() {
            continue;
        }
        let (kind, ctx) = match &u.unit.owner {
            UnitOwner::QuestStage { form, .. } => ("stage", RunCtx { quest: Some(FormId(*form)), ..Default::default() }),
            UnitOwner::Info { form, .. } => ("info", line_ctx(FormId(*form))),
            UnitOwner::Terminal { form, .. } => (
                "terminal",
                RunCtx { self_ref: placed.get(&FormId(*form)).copied(), action_ref: Some(FormId::PLAYER_REF), quest: None },
            ),
            UnitOwner::Package { form, .. } => ("package", RunCtx { self_ref: carriers.get(&FormId(*form)).copied(), ..Default::default() }),
            _ => continue,
        };
        let mut g = Game::new(data, World::new(forms, refs, SimConfig::default()), GameConfig::default());
        g.coverage = Some(Coverage::default());
        g.run_result(id, ctx);
        let key = unit_name(lo, &u.key());
        let mut met: Vec<(&'static str, String)> = g.events.iter().filter_map(|e| cause(&e.kind)).collect();
        met.sort();
        met.dedup();
        let e = by_kind.entry(kind).or_default();
        e.0 += 1;
        // An unread argument is reported apart: the script ran, maybe with a wrong value.
        e.1 += usize::from(met.iter().any(|m| m.0 != "arg_unread"));
        for m in met {
            let c = causes.entry(m).or_default();
            c.0 += 1;
            if c.1.len() < samples {
                c.1.push(key.clone());
            }
        }
        if let Some(c) = g.coverage.take() {
            cov.results.extend(c.results);
            cov.stages.extend(c.stages);
            cov.blocks.extend(c.blocks);
            cov.infos.extend(c.infos);
        }
    }
    let mut grouped: BTreeMap<&'static str, Vec<Json>> = BTreeMap::new();
    let mut ranked: Vec<_> = causes.into_iter().collect();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    for ((kind, what), (n, units)) in ranked {
        grouped.entry(kind).or_default().push(json!({"cause": what, "scripts": n, "e.g.": units}));
    }
    let kinds: serde_json::Map<String, Json> =
        by_kind.iter().map(|(k, (n, bad))| (k.to_string(), json!({"run": n, "with_problems": bad}))).collect();
    let totals: serde_json::Map<String, Json> =
        grouped.iter().map(|(k, v)| (k.to_string(), json!({"causes": v.len(), "scripts": v.iter().map(|c| c["scripts"].as_u64().unwrap_or(0)).sum::<u64>()}))).collect();
    (json!({"format": FORMAT, "load_order": lo, "summary": {"scripts": kinds, "problems": totals}, "causes": grouped}), cov)
}

/// The kind of one compiled argument, for a call's shape.
fn arg_shape(a: &Arg) -> char {
    match a {
        Arg::Int(_) | Arg::Float(_) | Arg::Ref(_) | Arg::Var(_) => 'v',
        Arg::Enum(_) => 'e',
        Arg::Axis(_) => 'a',
        Arg::String(_) => 's',
        Arg::FormatVars(_) => 'm',
        Arg::Int32(_) => 'i',
        Arg::Raw(_) => 'r',
    }
}

/// Every call in a program, statements and expressions.
fn program_calls<'p>(p: &'p Program, out: &mut Vec<&'p Call>) {
    fn expr<'p>(e: &'p Expr, out: &mut Vec<&'p Call>) {
        for t in &e.0 {
            if let Token::Call { call } = t {
                out.push(call);
            }
        }
    }
    p.walk(&mut |s| match s {
        Stmt::Set { expr: e, .. } => expr(e, out),
        Stmt::If { arms, .. } => arms.iter().for_each(|a| expr(&a.cond, out)),
        Stmt::Call { call } => out.push(call),
        Stmt::Unmatched { cond: Some(c), .. } => expr(c, out),
        _ => {}
    });
}

/// One argument shape of a function in the call probe: the unit of its first call site,
/// that call as a statement, how many call sites have it, a few of their units.
type Shape = (usize, Stmt, usize, Vec<usize>);

/// The call probe (#1122): every implemented function called by any compiled script of
/// the load order (object, quest and effect scripts too, which [`sweep`] does not run),
/// once per distinct shape of its arguments (which parameters are values, codes, axes,
/// strings, message variables), as a statement by itself in a game of its own, on the
/// player, with the script's variables at 0. Lists the calls that succeeded although the
/// function could not read an argument they passed (`EventKind::ArgUnread`): per
/// function, parameter and kinds, the call sites of those shapes and a few of their
/// scripts. A shape whose first call fails for want of context is not judged. Only
/// vanilla call statements and expression tokens are probed: calls inside the
/// extender's expressions (Let, Eval, While, ForEach: `Token::X`) are not, though they
/// are traced as `ArgUnread` when a script runs them.
pub fn probe_calls(data: &GameData, forms: &FormStore, refs: &RefTable, samples: usize) -> Json {
    let lo = &data.load_order;
    // Shape (opcode, argument kinds) → the first call site, how many, their scripts.
    let mut shapes: BTreeMap<(u16, String), Shape> = BTreeMap::new();
    for (id, u) in data.units.iter().enumerate() {
        let mut calls = Vec::new();
        program_calls(&u.program, &mut calls);
        for c in calls {
            if !matches!(crate::functions::lookup(c.opcode), Some(crate::functions::Kind::Impl(_))) {
                continue;
            }
            let shape: String = c.args.iter().map(arg_shape).collect();
            if shape.contains('r') {
                continue;
            }
            let e = shapes.entry((c.opcode, shape)).or_insert_with(|| (id, Stmt::Call { call: c.clone() }, 0, Vec::new()));
            e.2 += 1;
            if e.3.len() < samples && !e.3.contains(&id) {
                e.3.push(id);
            }
        }
    }
    // Cause → (call sites, scripts).
    let mut found: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    // Shapes not judged: their probe failed (no context), by function and shape.
    let mut failed: Vec<String> = Vec::new();
    for ((op, shape), (unit, stmt, sites, units)) in &shapes {
        let mut g = Game::new(data, World::new(forms, refs, SimConfig::default()), GameConfig::default());
        let ctx = RunCtx { self_ref: Some(FormId::PLAYER_REF), action_ref: Some(FormId::PLAYER_REF), quest: None };
        g.run_stmts(*unit, std::slice::from_ref(stmt), ctx);
        if let Some(EventKind::CallFailed { name, error, .. }) = g.events.iter().map(|e| &e.kind).find(|k| matches!(k, EventKind::CallFailed { .. })) {
            failed.push(format!("{name} (0x{op:04X}) {shape}: {}", generic(error)));
        }
        for e in &g.events {
            if let Some(("arg_unread", what)) = cause(&e.kind) {
                let f = found.entry(what).or_default();
                f.0 += sites;
                for u in units {
                    let k = unit_name(lo, &data.unit(*u).key());
                    if f.1.len() < samples && !f.1.contains(&k) {
                        f.1.push(k);
                    }
                }
            }
        }
    }
    let mut ranked: Vec<_> = found.into_iter().collect();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    let list: Vec<Json> = ranked.into_iter().map(|(what, (n, units))| json!({"cause": what, "calls": n, "e.g.": units})).collect();
    json!({"shapes": shapes.len(), "not_judged": failed, "causes": list.len(), "calls": list.iter().map(|c| c["calls"].as_u64().unwrap_or(0)).sum::<u64>(), "arg_unread": list})
}
