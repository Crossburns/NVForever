//! `fnvtool ai ...`: AI packages (Layer 6). JSON schemas: docs/spec-notes/packages.md §5.

use crate::esm_cmd::open_load_order;
use crate::WorldArgs;
use anyhow::Context;
use clap::Subcommand;
use fnv_esm::{LoadOrder, Record, Schema};
use fnv_sim::extract::{all, decode, first, local_fid};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AiCmd {
    /// What the load order's AI packages (PACK) hold and how actors use them: types,
    /// flags, locations, targets, schedules, conditions, begin/end/change data, and the
    /// package lists of NPCs and creatures (fnv.ai.survey/1).
    Survey {
        #[command(flatten)]
        w: WorldArgs,
        /// Script function catalogue, for condition function names (default:
        /// `script/functions.json` next to --schema).
        #[arg(long)]
        functions: Option<PathBuf>,
    },
    /// The packages as the game data reads them (type, flags, locations, targets,
    /// schedules, conditions, extras), and whether those tallies agree with the raw
    /// survey (fnv.ai.packages/1).
    Packages {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        functions: Option<PathBuf>,
    },
    /// Packages in full, and actors with their package lists (fnv.ai.show/1).
    Show {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        functions: Option<PathBuf>,
        /// A package by Editor ID or load-order FormID (repeatable).
        #[arg(long)]
        package: Vec<String>,
        /// An actor reference or actor base by Editor ID or load-order FormID
        /// (repeatable): its package list in order.
        #[arg(long)]
        actor: Vec<String>,
    },
    /// Compare with AI observer samples (private captures): the game's package type
    /// code for each record type, the current package against the actor's list, and
    /// the reported package target (fnv.ai.observed/1).
    Observed {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        functions: Option<PathBuf>,
        /// Observer output files (`obs_*.jsonl`).
        #[arg(long, num_args = 1.., required = true)]
        samples: Vec<PathBuf>,
    },
    /// The package each actor chooses at every hour of a day, and why each package of
    /// its list is or is not valid, in a game where nothing has happened yet
    /// (fnv.ai.day/1).
    Day {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        functions: Option<PathBuf>,
        /// A placed actor by Editor ID or load-order FormID (repeatable).
        #[arg(long, required = true)]
        actor: Vec<String>,
    },
    /// Whether every placed actor can get from where it is placed to where each package
    /// of its list sends it, over the navmeshes and through doors (fnv.ai.reach/1).
    Reach {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        functions: Option<PathBuf>,
    },
    /// Actors carrying their packages out over the navmesh, headless: the package
    /// chosen, the walks planned (doors, length), arrivals, places that cannot be
    /// reached, and where each actor is every game hour. With --samples, each actor
    /// starts where an AI observer capture first saw it and is compared with the capture
    /// at every sample (fnv.ai.run/1).
    Run {
        #[command(flatten)]
        w: WorldArgs,
        #[arg(long)]
        functions: Option<PathBuf>,
        /// A placed actor by Editor ID or load-order FormID (repeatable).
        #[arg(long, required = true)]
        actor: Vec<String>,
        /// GameHour at the start (default: the authored clock).
        #[arg(long)]
        start_hour: Option<f64>,
        /// Game hours to run.
        #[arg(long, default_value_t = 24.0)]
        hours: f64,
        /// Seconds per simulation step.
        #[arg(long, default_value_t = 1.0)]
        step: f64,
        /// Game seconds per second (the TimeScale global is 30 in the masters).
        #[arg(long, default_value_t = 30.0)]
        timescale: f64,
        /// Observer output files (`obs_*.jsonl`) to start from and compare with.
        #[arg(long, num_args = 1..)]
        samples: Vec<PathBuf>,
        /// `ACTOR=PACKAGE`: give the actor this package as a script would
        /// (AddScriptPackage), overriding its list; stands in for quest state
        /// (repeatable).
        #[arg(long)]
        force: Vec<String>,
        /// `QUEST:INDEX=VALUE`: set a quest script variable before the run, standing in for
        /// quest state the run does not have (repeatable).
        #[arg(long)]
        quest_var: Vec<String>,
    },
}

fn functions_path(w: &WorldArgs, functions: Option<PathBuf>) -> PathBuf {
    functions.unwrap_or_else(|| w.schema.parent().map(|p| p.join("script/functions.json")).unwrap_or_default())
}

/// Open the load order and build the game data (Layer 5's static data, which holds the
/// typed packages) and the reference table.
pub(crate) fn with_data(
    w: &WorldArgs,
    functions: Option<PathBuf>,
    f: impl FnOnce(&LoadOrder, &Schema, &fnv_game::GameData, &fnv_sim::RefTable, &HashMap<u32, String>) -> anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let plugins = open_load_order(&w.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let path = functions_path(w, functions);
    let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    let fjson: Value = serde_json::from_str(&text).with_context(|| path.display().to_string())?;
    let table = fnv_script::FunctionTable::load(&path).with_context(|| path.display().to_string())?;
    let refs = fnv_sim::RefTable::build(&lo, &schema);
    let data = fnv_game::GameData::build(&lo, &schema, table, &fjson, &refs);
    let mut v = f(&lo, &schema, &data, &refs, &function_names(&path))?;
    v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
    Ok(v)
}

pub fn run(cmd: AiCmd) -> anyhow::Result<Value> {
    match cmd {
        AiCmd::Survey { w, functions } => {
            let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
            let plugins = open_load_order(&w.lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let names = function_names(&functions_path(&w, functions));
            let mut v = survey(&lo, &schema, &names);
            v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
            Ok(v)
        }
        AiCmd::Packages { w, functions } => {
            with_data(&w, functions, |lo, schema, data, refs, names| Ok(crate::ai_packages::packages(data, refs, &survey(lo, schema, names))))
        }
        AiCmd::Show { w, functions, package, actor } => {
            with_data(&w, functions, |_, _, data, refs, _| crate::ai_packages::show(data, refs, &package, &actor))
        }
        AiCmd::Observed { w, functions, samples } => {
            with_data(&w, functions, |lo, schema, data, refs, _| {
                let forms = fnv_sim::FormStore::build(lo, schema);
                crate::ai_packages::observed(lo, data, &forms, refs, &samples)
            })
        }
        AiCmd::Run { w, functions, actor, start_hour, hours, step, timescale, samples, force, quest_var } => with_data(&w, functions, |lo, schema, data, refs, _| {
            let furniture_facts = crate::ai_run::furniture_facts(lo, schema, &w);
            let args = crate::ai_run::RunArgs { actors: actor, force, quest_vars: quest_var, start_hour, hours, step, timescale, samples, furniture_facts };
            crate::ai_run::run(lo, schema, data, refs, &args, &|n| crate::ai_packages::find_form(data, refs, n))
        }),
        AiCmd::Reach { w, functions } => with_data(&w, functions, |lo, schema, data, refs, _| crate::ai_run::reach(lo, schema, data, refs)),
        AiCmd::Day { w, functions, actor } => with_data(&w, functions, |lo, schema, data, refs, _| {
            let forms = fnv_sim::FormStore::build(lo, schema);
            crate::ai_choice::day(data, &forms, refs, &actor, &|n| crate::ai_packages::find_form(data, refs, n))
        }),
    }
}

/// Condition function index (opcode − 0x1000) → name, from the script catalogue.
fn function_names(path: &std::path::Path) -> HashMap<u32, String> {
    let Ok(text) = std::fs::read_to_string(path) else { return HashMap::new() };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return HashMap::new() };
    let list = v.get("functions").and_then(Value::as_array).cloned().or_else(|| v.as_array().cloned()).unwrap_or_default();
    let mut out = HashMap::new();
    for f in list {
        let Some(name) = f.get("name").and_then(Value::as_str) else { continue };
        for op in f.get("opcodes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if let Ok(n) = u32::from_str_radix(op.trim_start_matches("0x"), 16) {
                if n >= 0x1000 {
                    out.insert(n - 0x1000, name.to_string());
                }
            }
        }
    }
    out
}

fn fid(v: u32) -> String {
    format!("0x{v:08X}")
}

/// The winning definition of every record of these types: (load-order FormID →
/// plugin index, record).
pub(crate) fn winners<'a>(lo: &LoadOrder<'a>, sigs: &[&[u8; 4]]) -> BTreeMap<u32, (usize, &'a Record)> {
    let mut out = BTreeMap::new();
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs {
            if sigs.contains(&&r.header.sig) {
                if let Some(g) = lo.to_global(pi, r.header.form_id) {
                    out.insert(g, (pi, *r));
                }
            }
        }
    }
    out
}

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

fn flag_names(v: Option<&Value>) -> Vec<String> {
    v.and_then(|f| f.get("set")).and_then(Value::as_array).into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect()
}

fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(x) => x.to_string(),
        None => "-".into(),
    }
}

/// Distribution summary of a list of numbers.
fn spread(v: &mut [i64]) -> Value {
    if v.is_empty() {
        return Value::Null;
    }
    v.sort_unstable();
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    json!({"count": v.len(), "min": v[0], "median": at(0.5), "p90": at(0.9), "max": v[v.len() - 1]})
}

fn survey(lo: &LoadOrder, schema: &Schema, fnames: &HashMap<u32, String>) -> Value {
    let packs = winners(lo, &[b"PACK"]);
    let sig_of = |g: u32| lo.winner(g).map(|(_, r)| r.sig_str()).unwrap_or_else(|| "missing".into());
    let map = |pi: usize, local: u32| if local == 0 { None } else { lo.to_global(pi, local) };
    let mut per_plugin: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut pkdt_sizes: BTreeMap<String, u64> = BTreeMap::new();
    let mut flags_by_type: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut behavior: BTreeMap<String, u64> = BTreeMap::new();
    let mut specific_by_type: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut loc_types: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut loc_forms: BTreeMap<String, u64> = BTreeMap::new();
    let mut radii: Vec<i64> = Vec::new();
    let mut target_types: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut target_forms: BTreeMap<String, u64> = BTreeMap::new();
    let (mut months, mut dows, mut dates, mut hours) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let mut durations: Vec<i64> = Vec::new();
    let mut scheduled_by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut cond_counts: Vec<i64> = Vec::new();
    let mut cond_funcs: BTreeMap<String, u64> = BTreeMap::new();
    let mut run_on: BTreeMap<String, u64> = BTreeMap::new();
    let mut events: BTreeMap<String, u64> = BTreeMap::new();
    let mut present: BTreeMap<String, u64> = BTreeMap::new();
    let mut status: BTreeMap<String, u64> = BTreeMap::new();
    let mut type_of: HashMap<u32, String> = HashMap::new();
    let mut edid_of: HashMap<u32, String> = HashMap::new();
    let mut unnamed_bits: BTreeMap<String, u64> = BTreeMap::new();
    let mut unresolved_targets: BTreeMap<String, u64> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pld2_copies: BTreeMap<String, u64> = BTreeMap::new();
    for (&g, &(pi, r)) in &packs {
        bump(&mut per_plugin, lo.plugins[pi].name.clone());
        let Ok(d) = decode(r, schema) else {
            bump(&mut status, "undecodable");
            continue;
        };
        for x in &d {
            bump(&mut present, String::from_utf8_lossy(&x.sig).into_owned());
            if x.status != fnv_esm::decode::SubStatus::Ok {
                bump(&mut status, format!("{} {:?}", String::from_utf8_lossy(&x.sig), x.status));
            }
        }
        let edid = first(&d, b"EDID").and_then(Value::as_str).unwrap_or("").to_string();
        let pkdt = first(&d, b"PKDT");
        let ty = text(pkdt.and_then(|p| p.get("type")));
        type_of.insert(g, ty.clone());
        edid_of.insert(g, edid.clone());
        bump(&mut by_type, ty.clone());
        let ex = examples.entry(ty.clone()).or_default();
        if ex.len() < 4 {
            ex.push(format!("{} {edid}", fid(g)));
        }
        if let Some(s) = r.subrecords().ok().and_then(|ss| ss.into_iter().find(|s| &s.sig == b"PKDT")) {
            bump(&mut pkdt_sizes, s.data.len().to_string());
        }
        let ft = flags_by_type.entry(ty.clone()).or_default();
        bump(ft, "(packages)");
        for f in flag_names(pkdt.and_then(|p| p.get("general_flags"))) {
            bump(ft, f);
        }
        // Bits of the general flags that no source names for FNV (11, 14-16, 29-31).
        let raw = pkdt.and_then(|p| p.get("general_flags")).and_then(|f| f.get("value")).and_then(Value::as_u64).unwrap_or(0);
        for b in [11u32, 14, 15, 16, 29, 30, 31] {
            if raw & (1 << b) != 0 {
                bump(&mut unnamed_bits, format!("bit{b}"));
            }
        }
        for f in flag_names(pkdt.and_then(|p| p.get("fallout_behavior_flags"))) {
            bump(&mut behavior, f);
        }
        if let Some(bits) = pkdt.and_then(|p| p.get("type_specific_flags_raw")).and_then(Value::as_u64) {
            let st = specific_by_type.entry(ty.clone()).or_default();
            for b in 0..16 {
                if bits & (1 << b) != 0 {
                    bump(st, format!("bit{b}"));
                }
            }
        }
        // Locations: the first and the second. A record stores PLD2 twice (xEdit lists
        // it twice); the copies are compared and the location counted once.
        let copies: Vec<&Value> = all(&d, b"PLD2").collect();
        if !copies.is_empty() {
            bump(&mut pld2_copies, format!("{} per package", copies.len()));
            bump(&mut pld2_copies, if copies.windows(2).all(|w| w[0] == w[1]) { "copies identical" } else { "copies differ" });
            for w in d.windows(2).filter(|w| &w[1].sig == b"PLD2") {
                bump(&mut pld2_copies, format!("after {}", String::from_utf8_lossy(&w[0].sig)));
            }
        }
        for (k, sig) in [(1, b"PLDT"), (2, b"PLD2")] {
            for l in all(&d, sig).take(1) {
                let lt = text(l.get("type"));
                bump(loc_types.entry(format!("{ty} / location {k}")).or_default(), lt.clone());
                if let Some(rad) = l.get("radius").and_then(Value::as_i64) {
                    radii.push(rad);
                }
                if matches!(lt.as_str(), "Near reference" | "In cell" | "Object ID") {
                    let raw = l.get("location_raw").and_then(Value::as_u64).unwrap_or(0) as u32;
                    bump(&mut loc_forms, format!("{lt}: {}", map(pi, raw).map_or("null".to_string(), sig_of)));
                }
            }
        }
        for (k, sig) in [(1, b"PTDT"), (2, b"PTD2")] {
            for t in all(&d, sig) {
                let tt = text(t.get("type"));
                bump(target_types.entry(format!("{ty} / target {k}")).or_default(), tt.clone());
                if matches!(tt.as_str(), "Specific Reference" | "Object ID") {
                    let raw = t.get("target_raw").and_then(Value::as_u64).unwrap_or(0) as u32;
                    let sig = map(pi, raw).map_or("null".to_string(), sig_of);
                    if sig == "missing" {
                        bump(&mut unresolved_targets, fid(map(pi, raw).unwrap_or(raw)));
                    }
                    bump(&mut target_forms, format!("{tt}: {sig}"));
                }
            }
        }
        if let Some(s) = first(&d, b"PSDT") {
            let n = |k: &str| s.get(k).and_then(Value::as_i64).unwrap_or(-99);
            bump(&mut months, n("month").to_string());
            bump(&mut dows, text(s.get("day_of_week")));
            bump(&mut dates, n("date").to_string());
            bump(&mut hours, n("time").to_string());
            durations.push(n("duration_hours"));
            if n("time") >= 0 || n("duration_hours") > 0 || n("month") >= 0 || n("date") > 0 || text(s.get("day_of_week")) != "Any" {
                bump(&mut scheduled_by_type, ty.clone());
            }
        }
        // Package conditions: the CTDA before the begin / end / change groups.
        let mut n = 0i64;
        for x in &d {
            if matches!(&x.sig, b"POBA" | b"POEA" | b"POCA") {
                break;
            }
            if &x.sig == b"CTDA" {
                n += 1;
                if let Some(v) = &x.value {
                    let f = v.get("function").and_then(Value::as_u64).unwrap_or(u64::MAX) as u32;
                    bump(&mut cond_funcs, fnames.get(&f).cloned().unwrap_or_else(|| format!("#{f}")));
                    bump(&mut run_on, text(v.get("run_on")));
                }
            }
        }
        cond_counts.push(n);
        // Begin / end / change: which carry an idle, a topic or a compiled script.
        let mut section = "";
        for x in &d {
            let form_set = || x.value.as_ref().and_then(local_fid).is_some_and(|f| f != 0);
            let compiled = || x.value.as_ref().and_then(|v| v.get("compiled_size")).and_then(Value::as_u64).is_some_and(|n| n > 0);
            match &x.sig {
                b"POBA" => section = "begin",
                b"POEA" => section = "end",
                b"POCA" => section = "change",
                b"INAM" if !section.is_empty() && form_set() => bump(&mut events, format!("{section}: idle")),
                b"TNAM" if !section.is_empty() && form_set() => bump(&mut events, format!("{section}: topic")),
                b"SCHR" if !section.is_empty() && compiled() => bump(&mut events, format!("{section}: script")),
                _ => {}
            }
        }
    }
    // Package lists of actor bases.
    let actors = winners(lo, &[b"NPC_", b"CREA"]);
    let mut list_lens: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut referenced: HashSet<u32> = HashSet::new();
    let mut first_types: BTreeMap<String, u64> = BTreeMap::new();
    let mut first_named: BTreeMap<String, u64> = BTreeMap::new();
    let mut listed_by: BTreeMap<u32, u64> = BTreeMap::new();
    let mut template_packages = 0u64;
    let mut missing = 0u64;
    let mut with_list: HashSet<u32> = HashSet::new();
    for (&g, &(pi, r)) in &actors {
        let Ok(d) = decode(r, schema) else { continue };
        let list: Vec<u32> = all(&d, b"PKID").filter_map(local_fid).filter_map(|l| map(pi, l)).collect();
        let kind = r.sig_str();
        list_lens.entry(kind).or_default().push(list.len() as i64);
        if !list.is_empty() {
            with_list.insert(g);
        }
        for p in &list {
            referenced.insert(*p);
            *listed_by.entry(*p).or_default() += 1;
            if !packs.contains_key(p) {
                missing += 1;
            }
        }
        if let Some(p) = list.first() {
            bump(&mut first_types, type_of.get(p).cloned().unwrap_or_else(|| "missing".into()));
            bump(&mut first_named, format!("{} {}", edid_of.get(p).cloned().unwrap_or_default(), type_of.get(p).cloned().unwrap_or_default()));
        }
        let uses_template = first(&d, b"ACBS")
            .and_then(|a| a.get("template_flags"))
            .map(|f| flag_names(Some(f)).iter().any(|n| n == "AI Packages"))
            .unwrap_or(false);
        template_packages += (uses_template && first(&d, b"TPLT").is_some()) as u64;
    }
    // Placed actors whose base lists packages.
    let placed = winners(lo, &[b"ACHR", b"ACRE"]);
    let mut placed_with = 0u64;
    for &(pi, r) in placed.values() {
        let Ok(d) = decode(r, schema) else { continue };
        if first(&d, b"NAME").and_then(local_fid).and_then(|l| map(pi, l)).is_some_and(|b| with_list.contains(&b)) {
            placed_with += 1;
        }
    }
    let top = |m: BTreeMap<String, u64>, n: usize| {
        let mut v: Vec<(String, u64)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.into_iter().take(n).map(|(k, c)| json!([k, c])).collect::<Vec<_>>()
    };
    let most_listed: BTreeMap<String, u64> =
        listed_by.iter().map(|(p, n)| (format!("{} {} {}", fid(*p), edid_of.get(p).cloned().unwrap_or_default(), type_of.get(p).cloned().unwrap_or_default()), *n)).collect();
    let mut top_funcs: Vec<(String, u64)> = cond_funcs.into_iter().collect();
    top_funcs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    json!({
        "schema": "fnv.ai.survey/1",
        "packages": packs.len(),
        "per_plugin": per_plugin,
        "decode_status_not_ok": status,
        "subrecords_present": present,
        "pkdt_sizes": pkdt_sizes,
        "by_type": by_type,
        "examples_by_type": examples,
        "general_flags_by_type": flags_by_type,
        "general_flag_bits_unnamed": unnamed_bits,
        "fallout_behavior_flags": behavior,
        "type_specific_bits_by_type": specific_by_type,
        "location_types": loc_types,
        "location2_copies": pld2_copies,
        "location_forms": loc_forms,
        "location_radius": spread(&mut radii),
        "target_types": target_types,
        "target_forms": target_forms,
        "unresolved_target_values": top(unresolved_targets, 10),
        "schedule": {
            "month": months, "day_of_week": dows, "date": dates, "hour": hours,
            "duration_hours": spread(&mut durations),
            "scheduled_by_type": scheduled_by_type,
        },
        "conditions": {
            "per_package": spread(&mut cond_counts),
            "top_functions": top_funcs.iter().take(30).map(|(n, c)| json!([n, c])).collect::<Vec<_>>(),
            "run_on": run_on,
        },
        "begin_end_change": events,
        "actors": {
            "list_length_by_record": list_lens.iter_mut().map(|(k, v)| (k.clone(), spread(v))).collect::<serde_json::Map<_, _>>(),
            "first_package_type": first_types,
            "most_common_first_packages": top(first_named, 15),
            "most_listed_packages": top(most_listed, 15),
            "template_supplies_packages": template_packages,
            "list_entries_not_a_package": missing,
            "packages_never_listed": packs.keys().filter(|p| !referenced.contains(p)).count(),
            "placed_actors": placed.len(),
            "placed_actors_whose_base_lists_packages": placed_with,
        },
    })
}
