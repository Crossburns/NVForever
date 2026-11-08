//! `fnvtool ext coverage`: what a load order's scripts need that NVForever does not have
//! (fnv.ext.coverage/1). Three checks, all through the engine's own code, none by name
//! matching: every compiled script decodes with nothing left raw; every Script Runner
//! file compiles; every extender function the load order's scripts and Script Runner /
//! user-defined-function files name reaches a handler when the VM calls it (a call the
//! VM answers "not built yet", or that no handler takes, is missing).

use crate::esm_cmd::open_load_order;
use crate::ext_cmd::{bytecode_uses_extender, scan_source, Catalogue};
use crate::LoadOrderArgs;
use anyhow::Context;
use fnv_esm::{LoadOrder, Schema};
use fnv_game::trace::EventKind;
use fnv_script::unit::read_load_order;
use fnv_script::FunctionTable;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Where a function is used: calls and the plugins or files naming it.
#[derive(Default)]
struct Use {
    calls: u64,
    users: BTreeMap<String, u64>,
}

/// Files under `dir` (recursively) with one of `exts`, sorted.
fn files(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    v.sort();
    for p in v {
        if p.is_dir() {
            files(&p, exts, out);
        } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e))) {
            out.push(p);
        }
    }
}

/// The extender's statement keywords: syntax the compiler reads, not functions to call.
const KEYWORDS: [&str; 7] = ["eval", "let", "while", "loop", "foreach", "continue", "break"];

/// How the VM answers a bare call of `name`.
fn probe(g: &mut fnv_game::game::Game<'_>, name: &str) -> (&'static str, String) {
    let before = g.events.len();
    let src = format!("Begin Function {{ }}\nSetFunctionValue ({name})\nEnd\n");
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.run_query(&src, fnv_game::game::RunCtx::default())));
    let r = match r {
        Ok(r) => r,
        Err(_) => return ("panics", "the handler panicked on a call without arguments".into()),
    };
    let mut note = String::new();
    for e in &g.events[before..] {
        match &e.kind {
            EventKind::Unhandled { .. } => return ("missing", "no handler".into()),
            EventKind::Skipped { what, .. } | EventKind::ScriptError { error: what, .. } => {
                if what.contains("not built yet") {
                    return ("missing", what.clone());
                }
                if note.is_empty() {
                    note = what.clone();
                }
            }
            _ => {}
        }
    }
    match r {
        Err(e) if e.starts_with("script did not compile") => ("compile_error", note),
        _ => ("handled", note),
    }
}

pub(crate) fn coverage(lo: &LoadOrderArgs, schema_dir: &Path, functions: &Path) -> anyhow::Result<Value> {
    // `@file` names a plugin list (one per line, `#` comments and MO2's `*` marks allowed):
    // a 200-plugin load order does not fit on one command line.
    let mut plugins = Vec::new();
    for p in &lo.plugins {
        match p.strip_prefix('@') {
            Some(f) => {
                let text = std::fs::read_to_string(f).with_context(|| f.to_string())?;
                plugins.extend(text.lines().map(|l| l.trim().trim_start_matches('*')).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string));
            }
            None => plugins.push(p.clone()),
        }
    }
    let lo = &LoadOrderArgs { data_dir: lo.data_dir.clone(), plugins };
    // Mods' files as the engine sees them: the Data folder and its archives, read-only.
    let vfs = std::sync::Arc::new(fnv_vfs::Vfs::open_for_plugins(&lo.data_dir, &lo.plugins).ok());
    let v2 = vfs.clone();
    let lookup: fnv_game::mod_files::Lookup = std::sync::Arc::new(move |p: &str| match vfs.as_ref() {
        Some(v) if v.loose_path(p).is_some() => 1,
        Some(v) if v.exists(p) => 2,
        _ => 0,
    });
    let reader: fnv_game::mod_files::Reader = std::sync::Arc::new(move |p: &str| v2.as_ref().as_ref().and_then(|v| v.read(p)).and_then(Result::ok));
    fnv_game::mod_files::set_roots(fnv_game::mod_files::Roots { read: vec![lo.data_dir.clone()], config: None, write: None, lookup: Some(lookup), reader: Some(reader) });

    let cat = Catalogue::load(functions)?;
    let ftext = std::fs::read_to_string(functions).with_context(|| functions.display().to_string())?;
    let fjson: Value = serde_json::from_str(&ftext).with_context(|| functions.display().to_string())?;
    let table = FunctionTable::load(functions).with_context(|| functions.display().to_string())?;
    let plugins = open_load_order(lo)?;
    let order = LoadOrder::new(&plugins)?;
    let schema = Schema::load_dir(schema_dir)?;
    let (units, _) = read_load_order(&order, &schema);

    // 1. The plugins' scripts: the functions their sources name, and the units that use
    // the extender with no source to compile (NVForever runs those from source only).
    let mut used: BTreeMap<String, Use> = BTreeMap::new();
    let mut no_source: BTreeMap<String, u64> = BTreeMap::new();
    for u in &units {
        match &u.source {
            Some(src) => {
                let locals: HashSet<String> = u.variables.iter().map(|v| v.name.to_ascii_lowercase()).collect();
                for (k, n) in scan_source(src, &locals, &cat).functions {
                    let e = used.entry(k).or_default();
                    e.calls += n;
                    *e.users.entry(u.plugin.clone()).or_default() += n;
                }
            }
            None if u.has_scda && bytecode_uses_extender(&u.bytes, &table) => *no_source.entry(u.plugin.clone()).or_default() += 1,
            None => {}
        }
    }

    // 2. Script Runner and user-defined-function files: their functions, by the same scan.
    let mut sr = Vec::new();
    files(&lo.data_dir.join("NVSE").join("Plugins").join("Scripts"), &["txt"], &mut sr);
    files(&lo.data_dir.join("NVSE").join("user_defined_functions"), &["txt", "gek"], &mut sr);
    for p in &sr {
        let Ok(bytes) = std::fs::read(p) else { continue };
        let src: String = bytes.iter().map(|&b| b as char).collect();
        let rel = p.strip_prefix(&lo.data_dir).unwrap_or(p).display().to_string();
        for (k, n) in scan_source(&src, &HashSet::new(), &cat).functions {
            let e = used.entry(k).or_default();
            e.calls += n;
            *e.users.entry(rel.clone()).or_default() += n;
        }
    }

    let forms = fnv_sim::FormStore::build(&order, &schema);
    let refs = fnv_sim::RefTable::build(&order, &schema);
    let data = fnv_game::GameData::build(&order, &schema, table, &fjson, &refs);
    let fresh = || {
        let world = fnv_sim::World::new(&forms, &refs, fnv_sim::SimConfig::default());
        let mut g = fnv_game::game::Game::new(&data, world, fnv_game::game::GameConfig::default());
        g.new_game();
        g
    };
    let mut g = fresh();
    let compile: Vec<Value> = g
        .compile_script_runner()
        .into_iter()
        .filter_map(|r| r.result.err().map(|e| json!({"file": r.path, "error": e})))
        .collect();

    // 3. Each function the VM is asked for, called bare.
    let mut by_status: BTreeMap<&str, u64> = BTreeMap::new();
    let mut rows: Vec<(u64, u64, Value)> = Vec::new();
    for (name, u) in used.iter().filter(|(n, _)| !KEYWORDS.contains(&n.to_ascii_lowercase().as_str())) {
        let (status, note) = probe(&mut g, name);
        if status == "panics" {
            g = fresh();
        }
        *by_status.entry(status).or_default() += 1;
        if status != "handled" {
            let origin = cat.origin(name);
            let top: Vec<&String> = u.users.keys().take(6).collect();
            rows.push((u.users.len() as u64, u.calls, json!({"name": name, "origin": origin, "status": status, "users": u.users.len(), "calls": u.calls, "note": note, "used_by": top})));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    // Source compiles that failed, by plugin (the key starts with the plugin's name).
    let mut by_plugin: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for f in &data.source_failures {
        let plugin = units.iter().map(|u| u.plugin.as_str()).find(|p| f.starts_with(*p)).unwrap_or("?");
        by_plugin.entry(plugin.to_string()).or_default().push(f);
    }
    let mut fails: Vec<_> = by_plugin.into_iter().collect();
    fails.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    Ok(json!({
        "format": "fnv.ext.coverage/2",
        "summary": {
            "plugins": order.plugins.len(),
            "units_run_from_source": data.issues.count("script_compiled_from_source"),
            "units_source_does_not_compile": data.source_failures.len(),
            "units_extender_without_source": no_source.values().sum::<u64>(),
            "script_runner_files": sr.len(),
            "script_runner_compile_failures": compile.len(),
            "extender_functions_used": used.len(),
            "functions": by_status,
        },
        "functions_not_handled": rows.into_iter().map(|r| r.2).collect::<Vec<_>>(),
        "source_failures_by_plugin": fails.iter().map(|(p, v)| json!({"plugin": p, "units": v.len(), "samples": v.iter().take(4).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "extender_without_source": no_source,
        "script_runner_compile": compile,
    }))
}
