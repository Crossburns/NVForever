use crate::{EsmCmd, LoadOrderArgs};
use anyhow::{bail, Context};
use rayon::prelude::*;
use fnv_esm::decode::{decode_record, formid_str, Coverage};
use fnv_esm::inventory::{header_json, inventory_json, stats_json};
use fnv_esm::raw::{visit_records, write_subrecords};
use fnv_esm::survey::Survey;
use fnv_esm::{LoadOrder, Plugin, Schema};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

pub fn run(cmd: EsmCmd) -> anyhow::Result<Value> {
    match cmd {
        EsmCmd::Header { plugin } => Ok(header_json(&Plugin::open(plugin)?)),
        EsmCmd::Stats { plugin } => Ok(stats_json(&Plugin::open(plugin)?)),
        EsmCmd::Inventory { plugin } => Ok(inventory_json(&Plugin::open(plugin)?)),
        EsmCmd::Verify { plugin } => verify(&plugin),
        EsmCmd::Coverage { plugins, schema, max_rows } => {
            let schema = Schema::load_dir(schema)?;
            // Several plugins: one report over all of them (smoke `esm-coverage` pins the
            // ten masters' totals).
            let mut cov = Coverage::default();
            let mut names = Vec::new();
            for plugin in plugins {
                let p = Plugin::open(plugin)?;
                let mut recs = Vec::new();
                visit_records(&p.nodes[1..], &mut |r, _| recs.push(r));
                cov.add_records_par(&recs, &schema);
                names.push(p.name.clone());
            }
            let mut v = cov.to_json(max_rows);
            v["schema"] = json!("fnv.esm.coverage/1");
            if let [one] = &names[..] {
                v["plugin"] = json!(one);
            } else {
                v["plugins"] = json!(names);
            }
            Ok(v)
        }
        EsmCmd::EditMove { plugins, schema, refs, dx, out } => edit_move(&plugins, &Schema::load_dir(schema)?, &refs, dx, &out),
        EsmCmd::EncodeCheck { plugins, schema, max_rows } => encode_check(&plugins, &Schema::load_dir(schema)?, max_rows),
        EsmCmd::Decode { plugin, schema, formid: want_fid, edid } => {
            let schema = Schema::load_dir(schema)?;
            let p = Plugin::open(plugin)?;
            let want_edid = edid.map(|s| s.to_ascii_lowercase());
            let mut found = Vec::new();
            visit_records(&p.nodes[1..], &mut |r, _| {
                let hit = match (want_fid, &want_edid) {
                    (Some(f), _) => r.header.form_id == f,
                    (None, Some(e)) => fnv_esm::inventory::names(r)
                        .ok()
                        .and_then(|(ed, _)| ed)
                        .is_some_and(|ed| ed.to_ascii_lowercase() == *e),
                    _ => false,
                };
                if hit {
                    found.push(decode_record(r, &schema));
                }
            });
            if found.is_empty() {
                bail!("no matching record");
            }
            Ok(json!({"schema": "fnv.esm.decode/1", "plugin": p.name, "records": found}))
        }
        EsmCmd::Loadorder { lo, schema } => loadorder(&lo, schema.as_deref()),
        EsmCmd::Survey { lo, plugin } => survey(&lo, plugin.as_deref()),
        EsmCmd::Fields { lo, schema } => crate::esm_fields::fields(&lo, &schema),
    }
}

/// Structural verification: byte-identical rewrite, every compressed record
/// decompresses, every payload re-serializes identically from its subrecords.
fn verify(path: &Path) -> anyhow::Result<Value> {
    let data = fnv_esm::plugin::map_file(path).with_context(|| path.display().to_string())?;
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let p = Plugin::parse_bytes(data.clone(), name)?;
    let out = p.to_bytes();
    let identical = out == data;
    let first_diff = if identical { None } else { out.iter().zip(&data).position(|(a, b)| a != b) };
    let mut recs = Vec::new();
    visit_records(&p.nodes, &mut |r, _| recs.push(r));
    // Decompress and re-serialize records in parallel; aggregate in file order.
    let checks: Vec<RecordCheck> = recs.par_iter().map(|r| check_record(r)).collect();
    let mut compressed = 0u64;
    let mut decompress_errors = Vec::new();
    let mut subrecord_errors = Vec::new();
    let mut payload_mismatch = Vec::new();
    let mut xxxx = 0u64;
    let mut trailer_anomalies = Vec::new();
    for (r, c) in recs.iter().zip(checks) {
        if r.is_compressed() {
            compressed += 1;
        }
        trailer_anomalies.extend(c.trailer_anomaly);
        match c.subrecords {
            Err(e) => decompress_errors.push(e),
            Ok(Ok((extended, mismatch))) => {
                xxxx += extended;
                if mismatch && payload_mismatch.len() < 50 {
                    payload_mismatch.push(format!("{} {}", r.sig_str(), formid_str(r.header.form_id)));
                }
            }
            Ok(Err(e)) => {
                if subrecord_errors.len() < 50 {
                    subrecord_errors.push(e);
                }
            }
        }
    }
    let records = recs.len();
    Ok(json!({
        "schema": "fnv.esm.verify/1",
        "plugin": p.name,
        "roundtrip_identical": identical,
        "first_difference": first_diff,
        "records": records,
        "compressed_records": compressed,
        "extended_size_subrecords": xxxx,
        "decompress_errors": decompress_errors,
        "zlib_trailer_anomalies": trailer_anomalies,
        "subrecord_errors": subrecord_errors,
        "payload_roundtrip_mismatches": payload_mismatch,
    }))
}

struct RecordCheck {
    trailer_anomaly: Option<String>,
    /// Err: payload cannot be decompressed. Ok(Err): subrecords do not parse.
    /// Ok(Ok((extended-size subrecords, re-serialization differs))).
    subrecords: Result<Result<(u64, bool), String>, String>,
}

fn check_record(r: &fnv_esm::Record) -> RecordCheck {
    let (payload, trailer) = match r.payload_checked() {
        Ok(x) => x,
        Err(e) => return RecordCheck { trailer_anomaly: None, subrecords: Err(e.to_string()) },
    };
    let trailer_anomaly = (trailer != fnv_inflate::Trailer::Verified).then(|| {
        format!("{} {} at 0x{:x}: {:?}", r.sig_str(), formid_str(r.header.form_id), r.offset, trailer)
    });
    let subrecords = match fnv_esm::raw::parse_subrecords(&payload) {
        Ok(subs) => Ok((subs.iter().filter(|s| s.extended).count() as u64, write_subrecords(&subs) != payload.as_ref())),
        Err((pos, m)) => Err(format!("{} {} at +{pos}: {m}", r.sig_str(), formid_str(r.header.form_id))),
    };
    RecordCheck { trailer_anomaly, subrecords: Ok(subrecords) }
}

pub(crate) fn open_load_order(args: &LoadOrderArgs) -> anyhow::Result<Vec<Plugin>> {
    args.plugins
        .iter()
        .map(|n| {
            let p = args.data_dir.join(n);
            Plugin::open(&p).with_context(|| p.display().to_string())
        })
        .collect()
}

/// Load-order resolution report. With --schema, also resolves every FormID-typed
/// field the schema knows about.
fn loadorder(args: &LoadOrderArgs, schema: Option<&Path>) -> anyhow::Result<Value> {
    let plugins = open_load_order(args)?;
    let lo = LoadOrder::new(&plugins)?;
    let schema = schema.map(Schema::load_dir).transpose()?;
    let mut per = Vec::new();
    let mut ref_total = 0u64;
    let mut ref_null = 0u64;
    let mut ref_ok = 0u64;
    let mut ref_bad_index = 0u64;
    let mut ref_missing: BTreeMap<String, u64> = BTreeMap::new();
    let mut ref_missing_examples: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (pi, p) in plugins.iter().enumerate() {
        let recs = &lo.records[pi];
        let self_idx = p.self_index() as u32;
        let mut new_forms = 0u64;
        let mut overrides = 0u64;
        let mut override_local: HashSet<u32> = HashSet::new();
        let mut override_types: BTreeMap<String, u64> = BTreeMap::new();
        for r in recs {
            if r.header.form_id >> 24 == self_idx {
                new_forms += 1;
            } else if r.header.form_id >> 24 < self_idx {
                overrides += 1;
                override_local.insert(r.header.form_id);
                *override_types.entry(r.sig_str()).or_default() += 1;
            }
        }
        let onam: HashSet<u32> = p.header.overridden_forms.iter().copied().collect();
        let onam_overridden = onam.intersection(&override_local).count();
        let mut not_in_onam: BTreeMap<String, u64> = BTreeMap::new();
        for r in recs {
            if override_local.contains(&r.header.form_id) && !onam.contains(&r.header.form_id) {
                *not_in_onam.entry(r.sig_str()).or_default() += 1;
            }
        }
        let unmappable: Vec<Value> = lo
            .unmappable
            .iter()
            .filter(|(x, _)| *x as usize == pi)
            .map(|(_, f)| Value::from(formid_str(*f)))
            .collect();
        // Overrides placed in cell temporary-children groups, and "injected" forms:
        // records using a master's FormID space whose base does not exist in any
        // earlier plugin.
        let mut temp_overrides: HashSet<u32> = HashSet::new();
        let mut injected: BTreeMap<String, u64> = BTreeMap::new();
        visit_records(&p.nodes[1..], &mut |r, stack| {
            if r.header.form_id >> 24 >= self_idx {
                return;
            }
            if stack.last().is_some_and(|g| g.group_type == fnv_esm::raw::group_type::CELL_TEMPORARY_CHILDREN) {
                temp_overrides.insert(r.header.form_id);
            }
            if let Some(g) = lo.to_global(pi, r.header.form_id) {
                let earlier = lo.index.get(&g).is_some_and(|d| d.iter().any(|x| (x.plugin as usize) < pi));
                if !earlier {
                    *injected.entry(r.sig_str()).or_default() += 1;
                }
            }
        });
        let temp_in_onam = temp_overrides.intersection(&onam).count();
        if let Some(schema) = &schema {
            // Decode in parallel; resolve in record order (examples keep the first ones).
            let all_fids: Vec<Vec<u32>> = recs
                .par_iter()
                .map(|r| {
                    let mut fids = Vec::new();
                    Coverage::measure(r, schema, Some(&mut |v: u32| fids.push(v)));
                    fids
                })
                .collect();
            for (r, fids) in recs.iter().zip(all_fids) {
                for v in fids {
                    ref_total += 1;
                    if v == 0 {
                        ref_null += 1;
                        continue;
                    }
                    match lo.to_global(pi, v) {
                        None => {
                            ref_bad_index += 1;
                            let ex = ref_missing_examples.entry("bad_master_index".into()).or_default();
                            if ex.len() < 5 {
                                ex.push(Value::from(format!(
                                    "{}: {} {} -> {}",
                                    p.name,
                                    r.sig_str(),
                                    formid_str(r.header.form_id),
                                    formid_str(v)
                                )));
                            }
                        }
                        Some(g) if lo.exists(g) => ref_ok += 1,
                        Some(g) => {
                            // FormIDs below 0x800 in the first master are engine
                            // built-ins that need not exist as records [hypothesis].
                            let key = if g & 0x00FF_FFFF < 0x800 && (g >> 24) == 0 { "builtin_range" } else { "missing" };
                            let kind = format!("{key}:{}", r.sig_str());
                            *ref_missing.entry(kind.clone()).or_default() += 1;
                            let ex = ref_missing_examples.entry(kind).or_default();
                            if ex.len() < 5 {
                                ex.push(Value::from(format!(
                                    "{}: {} {} -> {} (global {})",
                                    p.name,
                                    r.sig_str(),
                                    formid_str(r.header.form_id),
                                    formid_str(v),
                                    formid_str(g)
                                )));
                            }
                        }
                    }
                }
            }
        }
        per.push(json!({
            "plugin": p.name,
            "load_index": pi,
            "records": recs.len(),
            "new_forms": new_forms,
            "overrides": overrides,
            "unmappable_formids": unmappable,
            "onam_entries": onam.len(),
            "onam_entries_that_are_overrides": onam_overridden,
            "temporary_group_overrides": temp_overrides.len(),
            "temporary_group_overrides_in_onam": temp_in_onam,
            "injected_forms_by_type": injected,
            "overrides_by_type": override_types,
            "overrides_not_in_onam_by_type": not_in_onam,
        }));
    }
    let mut chains: BTreeMap<usize, u64> = BTreeMap::new();
    for defs in lo.index.values() {
        *chains.entry(defs.len()).or_default() += 1;
    }
    let mut v = json!({
        "schema": "fnv.esm.loadorder/1",
        "plugins": per,
        "unique_global_forms": lo.index.len(),
        "definitions_per_form": chains,
    });
    if schema.is_some() {
        v["references"] = json!({
            "formid_fields": ref_total,
            "null": ref_null,
            "resolved": ref_ok,
            "bad_master_index": ref_bad_index,
            "unresolved_by_kind": ref_missing,
            "unresolved_examples": ref_missing_examples,
        });
    }
    Ok(v)
}

fn survey(args: &LoadOrderArgs, only: Option<&str>) -> anyhow::Result<Value> {
    let plugins = open_load_order(args)?;
    let lo = LoadOrder::new(&plugins)?;
    let only = only.map(|s| s.to_ascii_lowercase());
    let mut s = Survey::default();
    let mut errors = Vec::new();
    for (pi, p) in plugins.iter().enumerate() {
        if let Some(o) = &only {
            if p.name.to_ascii_lowercase() != *o {
                continue;
            }
        }
        for r in &lo.records[pi] {
            match r.subrecords() {
                Ok(subs) => s.add(&r.sig_str(), &subs, &lo, pi),
                Err(e) => errors.push(Value::from(e.to_string())),
            }
        }
    }
    let mut v = s.to_json();
    v["errors"] = Value::Array(errors);
    Ok(v)
}

/// Per (record type, subrecord) outcome of decode then encode (`fnv_esm::encode::round_trip`).
#[derive(Default)]
struct EncodeTally {
    same: u64,
    not_decoded: u64,
    refused: u64,
    differs: u64,
    example: Option<String>,
}

fn encode_check(plugins: &[std::path::PathBuf], schema: &Schema, max_rows: usize) -> anyhow::Result<Value> {
    use fnv_esm::encode::{round_trip, RoundTrip};
    let mut by: BTreeMap<(String, String), EncodeTally> = BTreeMap::new();
    let mut names = Vec::new();
    for plugin in plugins {
        let p = Plugin::open(plugin)?;
        let mut recs = Vec::new();
        visit_records(&p.nodes[1..], &mut |r, _| recs.push(r));
        // The source bytes (first 32) ride along with a failure for the report.
        let outcomes: Vec<Vec<(String, RoundTrip, String)>> = recs
            .par_iter()
            .map(|r| match r.subrecords() {
                Ok(subs) => subs
                    .iter()
                    .map(|s| {
                        let o = round_trip(&r.header.sig, s, schema);
                        let hex = if matches!(o, RoundTrip::Refused(_) | RoundTrip::Differs(..)) { fnv_util::sha256::to_hex(&s.data[..s.data.len().min(32)]) } else { String::new() };
                        (s.sig_str(), o, hex)
                    })
                    .collect(),
                Err(_) => Vec::new(),
            })
            .collect();
        for (r, subs) in recs.iter().zip(outcomes) {
            for (sig, o, hex) in subs {
                let t = by.entry((r.sig_str(), sig)).or_default();
                let note = match o {
                    RoundTrip::Same => {
                        t.same += 1;
                        None
                    }
                    RoundTrip::NotDecoded => {
                        t.not_decoded += 1;
                        None
                    }
                    RoundTrip::Refused(e) => {
                        t.refused += 1;
                        Some(format!("refused: {e}"))
                    }
                    RoundTrip::Differs(at, len) => {
                        t.differs += 1;
                        Some(format!("differs at byte {at}, encoded {len} bytes"))
                    }
                };
                if let (None, Some(n)) = (&t.example, note) {
                    t.example = Some(format!("{} {}: {n}; source starts {hex}", p.name, formid_str(r.header.form_id)));
                }
            }
        }
        names.push(p.name.clone());
    }
    let sum = |f: fn(&EncodeTally) -> u64| by.values().map(f).sum::<u64>();
    let mut rows: Vec<_> = by.iter().filter(|(_, t)| t.refused + t.differs > 0).collect();
    rows.sort_by(|a, b| (b.1.refused + b.1.differs).cmp(&(a.1.refused + a.1.differs)).then(a.0.cmp(b.0)));
    let failing_pairs = rows.len();
    let rows: Vec<Value> = rows
        .into_iter()
        .take(max_rows)
        .map(|((rt, st), t)| json!({"record": rt, "sub": st, "same": t.same, "refused": t.refused, "differs": t.differs, "example": t.example}))
        .collect();
    Ok(json!({
        "schema": "fnv.esm.encode_check/1",
        "plugins": names,
        "summary": {
            "same": sum(|t| t.same),
            "not_decoded": sum(|t| t.not_decoded),
            "refused": sum(|t| t.refused),
            "differs": sum(|t| t.differs),
            "pairs": by.len(),
            "failing_pairs": failing_pairs,
        },
        "failing": rows,
    }))
}

fn edit_move(paths: &[std::path::PathBuf], schema: &Schema, refs: &[u32], dx: f32, out: &Path) -> anyhow::Result<Value> {
    use fnv_esm::edit::{write_placements, NewPlugin, Placement};
    let plugins: Vec<Plugin> = paths.iter().map(Plugin::open).collect::<Result<_, _>>()?;
    let lo = LoadOrder::new(&plugins)?;
    let mut edits = Vec::new();
    for &id in refs {
        let (_, r) = lo.winner(id).with_context(|| format!("{id:08X} is not in the load order"))?;
        let data = r.subrecords()?.into_iter().find(|s| &s.sig == b"DATA").context("no DATA")?.data;
        let f = |i: usize| f32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        edits.push(Placement { form_id: id, pos: [f(0) + dx, f(1), f(2)], rot: [f(3), f(4), f(5)], disabled: false });
    }
    let name = out.file_name().context("no file name")?.to_string_lossy().into_owned();
    let w = write_placements(&lo, schema, &NewPlugin { name, ..Default::default() }, &edits).map_err(anyhow::Error::msg)?;
    std::fs::write(out, &w.bytes)?;
    Ok(json!({"out": out.display().to_string(), "bytes": w.bytes.len(), "masters": w.masters, "references": w.references, "warnings": w.warnings}))
}
