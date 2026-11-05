//! `fnvtool sim …`: Layer 4 core simulation (crate `fnv-sim`).

use crate::esm_cmd::open_load_order;
use crate::SimCmd;
use anyhow::Context;
use fnv_esm::{LoadOrder, Schema};
use crate::phys_cmd::{build, plugin_in_world, Meshes};
use fnv_phys::{layers, CharacterParams, Filter, Owner, PhysParams};
use fnv_sim::actions::{self, Script, Settle};
use fnv_sim::forms::{BaseData, ItemKind};
use fnv_sim::refs::FLAG_PERSISTENT;
use fnv_sim::{FormId, FormStore, RefTable, World};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn run(cmd: SimCmd) -> anyhow::Result<Value> {
    match cmd {
        SimCmd::Forms { lo, schema } => {
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            Ok(forms_report(&lo, &FormStore::build(&lo, &schema), &RefTable::build(&lo, &schema)))
        }
        SimCmd::Items { lo, schema } => {
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            Ok(items(&FormStore::build(&lo, &schema)))
        }
        SimCmd::Containers { lo, schema } => {
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            Ok(containers(&FormStore::build(&lo, &schema)))
        }
        SimCmd::Sample { lo, schema, cases } => {
            let text = std::fs::read_to_string(&cases).with_context(|| cases.display().to_string())?;
            let cases: fnv_sim::sample::Cases =
                serde_json::from_str(&text).with_context(|| cases.display().to_string())?;
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            fnv_sim::sample::run(&FormStore::build(&lo, &schema), &cases).map_err(anyhow::Error::msg)
        }
        SimCmd::Run { lo: lo_args, schema: schema_dir, actions, nif_schema, no_physics } => {
            let text = std::fs::read_to_string(&actions).with_context(|| actions.display().to_string())?;
            let script: Script = serde_json::from_str(&text).with_context(|| actions.display().to_string())?;
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema_dir)?;
            let forms = FormStore::build(&lo, &schema);
            let refs = RefTable::build(&lo, &schema);
            let mut world = World::new(&forms, &refs, actions::config(&script));
            if no_physics {
                return actions::run(&mut world, &script).map_err(anyhow::Error::msg);
            }
            let meshes = Meshes::open(&lo_args.data_dir, &schema_dir, nif_schema.as_deref())?;
            let mut settle = PhysSettle { lo: &lo, schema: &schema, ix: None, meshes };
            actions::run_with(&mut world, &script, Some(&mut settle)).map_err(anyhow::Error::msg)
        }
        SimCmd::Doors { lo, schema } => {
            let plugins = open_load_order(&lo)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema)?;
            let ix = fnv_world::WorldIndex::build(&lo, &schema);
            Ok(doors(&ix, &RefTable::from_index(&lo, &schema, &ix)))
        }
        SimCmd::DoorModels { lo: lo_args, schema: schema_dir, nif_schema } => {
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema_dir)?;
            let nif_dir = nif_schema.unwrap_or_else(|| schema_dir.parent().map(|p| p.join("nif")).unwrap_or_default());
            let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
            let vfs = fnv_vfs::Vfs::open_default(&lo_args.data_dir).with_context(|| lo_args.data_dir.display().to_string())?;
            let forms = FormStore::build(&lo, &schema);
            let refs = RefTable::build(&lo, &schema);
            Ok(door_models(&lo, &schema, &forms, &refs, &vfs, &nif))
        }
        SimCmd::ActivatorModels { lo: lo_args, schema: schema_dir, nif_schema } => {
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema_dir)?;
            let nif_dir = nif_schema.unwrap_or_else(|| schema_dir.parent().map(|p| p.join("nif")).unwrap_or_default());
            let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
            let vfs = fnv_vfs::Vfs::open_default(&lo_args.data_dir).with_context(|| lo_args.data_dir.display().to_string())?;
            let forms = FormStore::build(&lo, &schema);
            let refs = RefTable::build(&lo, &schema);
            Ok(crate::activator_models::activator_models(&lo, &schema, &forms, &refs, &vfs, &nif))
        }
        SimCmd::PlaygroupCalls { lo: lo_args, schema: schema_dir, nif_schema, functions } => {
            let tp = functions.unwrap_or_else(|| schema_dir.join("..").join("script").join("functions.json"));
            let table = fnv_script::FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
            let plugins = open_load_order(&lo_args)?;
            let lo = LoadOrder::new(&plugins)?;
            let schema = Schema::load_dir(&schema_dir)?;
            let nif_dir = nif_schema.unwrap_or_else(|| schema_dir.parent().map(|p| p.join("nif")).unwrap_or_default());
            let nif = fnv_nif::Schema::load_dir(&nif_dir).with_context(|| nif_dir.display().to_string())?;
            let vfs = fnv_vfs::Vfs::open_default(&lo_args.data_dir).with_context(|| lo_args.data_dir.display().to_string())?;
            let forms = FormStore::build(&lo, &schema);
            let refs = RefTable::build(&lo, &schema);
            Ok(crate::activator_models::playgroup_calls(&lo, &schema, &forms, &refs, &vfs, &nif, &table))
        }
    }
}

/// Animation sequences of one door model, and its rigid bodies counted by block type,
/// layer and motion system.
fn door_model(vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema, path: &str) -> Result<(Vec<Value>, BTreeMap<String, u64>), String> {
    use fnv_nif::scene::Decoded;
    let data = match vfs.read(path) {
        None => return Err("not found".into()),
        Some(Err(e)) => return Err(e.to_string()),
        Some(Ok(d)) => d,
    };
    let f = fnv_nif::NifFile::parse(&data, Some(nif)).map_err(|e| e.to_string())?;
    let dec = Decoded::new(&f, &data, nif);
    let seqs = fnv_nif::anim::sequences(&f, &dec)
        .into_iter()
        .map(|s| {
            json!({"name": s.name, "start": s.start_time, "stop": s.stop_time, "cycle_type": s.cycle_type,
                "tracks": s.tracks.len(), "text_keys": s.text_keys.iter().map(|(t, k)| json!([t, k])).collect::<Vec<_>>()})
        })
        .collect();
    let mut bodies = BTreeMap::new();
    let anim = fnv_collide::anim::ModelAnim::new(&f, &dec);
    let door = anim.door();
    for o in fnv_collide::nif::collision_objects(&f, &dec).0 {
        bump(&mut bodies, format!("{:?} layer {}", o.motion, o.layer));
        let Some((open, close)) = door else { continue };
        if o.motion != fnv_collide::Motion::Keyframed {
            continue;
        }
        // How far the body's origin and a point a unit off it move: bind against the
        // end of Close (should be ~0 if the stored pose is the closed one) and the end of
        // Open.
        let at = |s: Option<&fnv_collide::anim::AnimSequence>, t: f32| {
            anim.node_to_root(s, t, o.node_block).map(|n| {
                let x = n.iso.compose(&o.body_transform);
                [x.apply([0.0; 3]), x.apply([50.0, 0.0, 0.0]), x.apply([0.0, 50.0, 0.0])]
            })
        };
        let dist = |a: [[f32; 3]; 3], b: [[f32; 3]; 3]| {
            (0..3).map(|i| (0..3).map(|k| (a[i][k] - b[i][k]).powi(2)).sum::<f32>().sqrt()).fold(0.0f32, f32::max)
        };
        if let (Some(bind), Some(c), Some(op)) = (at(None, 0.0), at(Some(close), close.length()), at(Some(open), open.length())) {
            let (bc, bo) = (dist(bind, c), dist(bind, op));
            bump(&mut bodies, format!("keyframed: bind vs close end {}", if bc < 1.0 { "same" } else { "differs" }));
            bump(&mut bodies, format!("keyframed: open end moves {}", if bo >= 1.0 { "yes" } else { "no" }));
        }
    }
    Ok((seqs, bodies))
}

/// Every DOOR base with its model's sequences and its placed references
/// (docs/spec-notes/doors.md). Observational: it decides which doors can animate.
fn door_models(lo: &LoadOrder, schema: &Schema, forms: &FormStore, refs: &RefTable, vfs: &fnv_vfs::Vfs, nif: &fnv_nif::Schema) -> Value {
    #[derive(Default)]
    struct Placed {
        refs: u64,
        teleport: u64,
        in_cell: u64,
        in_cell_locked: u64,
        open_by_default: u64,
    }
    let mut placed: BTreeMap<FormId, Placed> = BTreeMap::new();
    for p in refs.refs.values() {
        if forms.sig(p.base) != Some(*b"DOOR") {
            continue;
        }
        let e = placed.entry(p.base).or_default();
        e.refs += 1;
        if p.teleport.is_some() {
            e.teleport += 1;
        } else {
            e.in_cell += 1;
            if p.lock.is_some() {
                e.in_cell_locked += 1;
            }
            if fnv_world::placed::placement(lo, schema, p.id.0).is_some_and(|pl| pl.open_by_default) {
                e.open_by_default += 1;
            }
        }
    }
    let mut by_sequences: BTreeMap<String, [u64; 3]> = BTreeMap::new(); // bases, refs, in-cell refs
    let mut text_key_sets: BTreeMap<String, u64> = BTreeMap::new();
    let mut body_kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut flags: BTreeMap<String, u64> = BTreeMap::new();
    let mut durations: BTreeMap<String, u64> = BTreeMap::new();
    let mut cycles: BTreeMap<String, u64> = BTreeMap::new();
    let mut rows = Vec::new();
    for b in forms.bases.values().filter(|b| &b.sig == b"DOOR") {
        let Some(d) = fnv_world::placed::door_record(lo, schema, b.id.0) else { continue };
        let pl = placed.remove(&b.id).unwrap_or_default();
        bump(&mut flags, format!("0x{:02X}", d.flags));
        let (seqs, bodies, error) = match d.model.as_deref().map(|m| door_model(vfs, nif, m)) {
            None => (vec![], BTreeMap::new(), Some("no model".to_string())),
            Some(Ok((s, b))) => (s, b, None),
            Some(Err(e)) => (vec![], BTreeMap::new(), Some(e)),
        };
        let mut names: Vec<String> = seqs.iter().map(|s| s["name"].as_str().unwrap_or("?").to_string()).collect();
        names.sort();
        let key = if error.is_some() { "(error)".to_string() } else { names.join("+") };
        let e = by_sequences.entry(key).or_default();
        e[0] += 1;
        e[1] += pl.refs;
        e[2] += pl.in_cell;
        for s in &seqs {
            let name = s["name"].as_str().unwrap_or("?");
            let keys: Vec<&str> = s["text_keys"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|k| k[1].as_str())
                .map(|k| k.split(':').next().unwrap_or(k).trim())
                .collect();
            bump(&mut text_key_sets, format!("{name}: {}", keys.join(",")));
            let dur = s["stop"].as_f64().unwrap_or(0.0) - s["start"].as_f64().unwrap_or(0.0);
            bump(&mut durations, format!("{name}: {dur:.2}"));
            bump(&mut cycles, format!("{name}: {}", s["cycle_type"]));
        }
        for (k, v) in &bodies {
            *body_kinds.entry(k.clone()).or_default() += v;
        }
        rows.push(json!({
            "door": b.id.to_string(), "edid": b.edid, "model": d.model, "flags": d.flags,
            "open_sound": d.open_sound.map(|s| FormId(s).to_string()),
            "close_sound": d.close_sound.map(|s| FormId(s).to_string()),
            "loop_sound": d.loop_sound.map(|s| FormId(s).to_string()),
            "refs": pl.refs, "teleport_refs": pl.teleport, "in_cell_refs": pl.in_cell,
            "in_cell_with_lock": pl.in_cell_locked, "in_cell_open_by_default": pl.open_by_default,
            "sequences": seqs, "bodies": bodies, "error": error,
        }));
    }
    let by_sequences: serde_json::Map<String, Value> = by_sequences
        .into_iter()
        .map(|(k, v)| (k, json!({"bases": v[0], "refs": v[1], "in_cell_refs": v[2]})))
        .collect();
    json!({
        "schema": "fnv.sim.door_models/1",
        "summary": {
            "door_bases": rows.len(),
            "by_sequence_names": by_sequences,
            "text_keys_by_sequence": text_key_sets,
            "durations": durations,
            "cycle_types": cycles,
            "rigid_bodies": body_kinds,
            "flags": flags,
        },
        "doors": rows,
    })
}

/// Teleport doors by where they lead. The destination cell is the one `World::activate`
/// puts the actor in (the exterior cell containing the destination, in a worldspace);
/// a cell's worldspace is `RefTable::world_of`.
fn doors(ix: &fnv_world::WorldIndex, refs: &RefTable) -> Value {
    let world_name = |w: FormId| ix.worldspaces.get(&w.0).and_then(|w| w.edid.clone()).unwrap_or_else(|| w.to_string());
    let cell_name = |c: FormId| ix.cells.get(&c.0).and_then(|c| c.edid.clone().or_else(|| c.grid.map(|g| format!("{},{}", g.0, g.1))));
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut pairs: BTreeMap<String, u64> = BTreeMap::new();
    let mut cross = Vec::new();
    for p in refs.refs.values() {
        let Some(tp) = p.teleport else { continue };
        let to_cell = refs.teleport_cell(&tp);
        let (from_w, to_w) = (p.cell.and_then(|c| refs.world_of(c)), to_cell.and_then(|c| refs.world_of(c)));
        let kind = match (from_w, to_w, to_cell) {
            (_, _, None) => "unresolved",
            (None, None, _) => "interior_to_interior",
            (None, Some(_), _) => "interior_to_world",
            (Some(_), None, _) => "world_to_interior",
            (Some(a), Some(b), _) if a == b => "same_world",
            (Some(_), Some(_), _) => "cross_world",
        };
        bump(&mut kinds, kind);
        if let ("cross_world", Some(a), Some(b)) = (kind, from_w, to_w) {
            bump(&mut pairs, format!("{} -> {}", world_name(a), world_name(b)));
            cross.push(json!({
                "door": p.id.to_string(),
                "edid": p.edid,
                "from_world": world_name(a),
                "from_cell": p.cell.map(|c| c.to_string()),
                "from_cell_name": p.cell.and_then(cell_name),
                "pos": p.pos,
                "to_door": tp.door.to_string(),
                "to_world": world_name(b),
                "to_cell": to_cell.map(|c| c.to_string()),
                "to_cell_name": to_cell.and_then(cell_name),
                "to_pos": tp.pos,
                "to_rot": tp.rot,
                "back": refs.get(tp.door).and_then(|d| d.teleport).is_some_and(|t| t.door == p.id),
                "lock": p.lock.map(|l| json!({"level": l.level, "key": l.key.map(|k| k.to_string())})),
            }));
        }
    }
    json!({
        "schema": "fnv.sim.doors/1",
        "teleport_doors": kinds.values().sum::<u64>(),
        "by_kind": kinds,
        "cross_world_by_pair": pairs,
        "cross_world": cross,
    })
}

fn sig_str(s: &[u8; 4]) -> String {
    String::from_utf8_lossy(s).into_owned()
}

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

/// Classification of a FormID that a carried-item entry names.
fn target_class(forms: &FormStore, id: FormId) -> String {
    match forms.get(id).map(|b| &b.data) {
        Some(BaseData::Item(_)) => "item".into(),
        Some(BaseData::LeveledItem(_)) => "leveled_item".into(),
        _ => match forms.sig(id) {
            Some(s) => format!("other:{}", sig_str(&s)),
            None if id.0 < 0x800 => "engine_builtin".into(),
            None => "missing".into(),
        },
    }
}

/// Summary of what the loaders understood, plus consistency checks on the data
/// (fnv.sim.forms/1). Everything here is observational: it drives the spec notes.
fn forms_report(lo: &LoadOrder, forms: &FormStore, refs: &RefTable) -> Value {
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut item_stats: BTreeMap<String, [u64; 3]> = BTreeMap::new(); // total, value known, weight known
    let mut cnto_targets: BTreeMap<String, u64> = BTreeMap::new();
    let mut cnto_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut lvlo_targets: BTreeMap<String, u64> = BTreeMap::new();
    let mut odd_entries: Vec<String> = Vec::new();
    let (mut lvl_lists, mut lvl_entries, mut lvl_flags) = (0u64, 0u64, BTreeMap::new());
    for b in forms.bases.values() {
        bump(&mut by_kind, sig_str(&b.sig));
        match &b.data {
            BaseData::Item(i) => {
                let e = item_stats.entry(format!("{:?}", i.kind)).or_default();
                e[0] += 1;
                e[1] += i.value.is_some() as u64;
                e[2] += i.weight.is_some() as u64;
            }
            BaseData::Container(c) => {
                for e in &c.entries {
                    let class = target_class(forms, e.item);
                    if (e.count <= 0 || !class.ends_with("item")) && odd_entries.len() < 20 {
                        odd_entries.push(format!("{} {:?} -> {} x{} ({class})", b.id, b.edid, e.item, e.count));
                    }
                    bump(&mut cnto_targets, format!("CONT->{class}"));
                    bump(&mut cnto_counts, if e.count <= 0 { "<=0" } else if e.count == 1 { "1" } else { ">1" });
                }
            }
            BaseData::Actor(a) => {
                for e in &a.entries {
                    let class = target_class(forms, e.item);
                    if (e.count <= 0 || !class.ends_with("item")) && odd_entries.len() < 20 {
                        odd_entries.push(format!("{} {:?} -> {} x{} ({class})", b.id, b.edid, e.item, e.count));
                    }
                    bump(&mut cnto_targets, format!("{}->{class}", sig_str(&b.sig)));
                    bump(&mut cnto_counts, if e.count <= 0 { "<=0" } else if e.count == 1 { "1" } else { ">1" });
                }
            }
            BaseData::LeveledItem(l) => {
                lvl_lists += 1;
                lvl_entries += l.entries.len() as u64;
                for (on, name) in [(l.all_levels, "all_levels"), (l.each, "each"), (l.use_all, "use_all")] {
                    if on {
                        bump(&mut lvl_flags, name);
                    }
                }
                if l.global.is_some() {
                    bump(&mut lvl_flags, "global");
                }
                for e in &l.entries {
                    bump(&mut lvlo_targets, target_class(forms, e.item));
                }
            }
            _ => {}
        }
    }

    // References.
    let mut ref_sigs: BTreeMap<String, u64> = BTreeMap::new();
    let mut ref_base: BTreeMap<String, u64> = BTreeMap::new();
    let mut persist = [[0u64; 2]; 2]; // [flag][group]
    let (mut disabled, mut with_count, mut owned, mut reparented) = (0u64, 0u64, 0u64, 0u64);
    let mut lock_levels: BTreeMap<String, u64> = BTreeMap::new();
    let mut lock_keys: BTreeMap<String, u64> = BTreeMap::new();
    let mut lock_on: BTreeMap<String, u64> = BTreeMap::new();
    let mut tel: BTreeMap<String, u64> = BTreeMap::new();
    let mut tel_samples: Vec<String> = Vec::new();
    let mut ep: BTreeMap<String, u64> = BTreeMap::new();
    let mut pgre: BTreeMap<String, u64> = BTreeMap::new();
    let mut ep_samples: Vec<String> = Vec::new();
    for r in refs.refs.values() {
        bump(&mut ref_sigs, sig_str(&r.sig));
        let base_sig = forms.sig(r.base).map(|s| sig_str(&s));
        bump(
            &mut ref_base,
            base_sig.clone().unwrap_or_else(|| {
                if r.base.0 < 0x800 { format!("engine_builtin:{}", r.base) } else { "missing".into() }
            }),
        );
        persist[(r.flags & FLAG_PERSISTENT != 0) as usize][r.in_persistent_group as usize] += 1;
        disabled += r.initially_disabled() as u64;
        reparented += (r.cell != r.stored_in) as u64;
        with_count += r.count.is_some() as u64;
        owned += r.owner.is_some() as u64;
        if let Some(l) = r.lock {
            bump(&mut lock_levels, format!("{}{}", l.level, if l.key.is_some() { " with key" } else { "" }));
            bump(&mut lock_on, base_sig.clone().unwrap_or_else(|| "missing".into()));
            bump(
                &mut lock_keys,
                match l.key {
                    None => "none".into(),
                    Some(k) => match forms.get(k).map(|b| &b.data) {
                        Some(BaseData::Item(i)) if i.kind == ItemKind::Key => "KEYM".to_string(),
                        _ => format!("other:{}", forms.sig(k).map(|s| sig_str(&s)).unwrap_or_else(|| "missing".into())),
                    },
                },
            );
        }
        if let Some(e) = r.enable_parent {
            bump(&mut ep, if e.opposite { "opposite" } else { "same" });
            // Depth of the chain, and whether it ends at a missing reference or loops.
            let (mut cur, mut depth, mut seen) = (e.reference, 1u32, vec![r.id]);
            let end = loop {
                if seen.contains(&cur) {
                    break "loop";
                }
                seen.push(cur);
                match refs.get(cur) {
                    None if cur == FormId::PLAYER_REF => break "player",
                    None => break "missing parent",
                    Some(p) => match p.enable_parent {
                        None => break "ok",
                        Some(pp) => {
                            cur = pp.reference;
                            depth += 1;
                        }
                    },
                }
            };
            bump(&mut ep, format!("depth {depth}"));
            if end != "ok" {
                bump(&mut ep, end);
                if ep_samples.len() < 10 {
                    ep_samples.push(format!("{} -> {}: {end}", r.id, e.reference));
                }
            }
        }
        if &r.sig == b"PGRE" {
            let class = match forms.get(r.base).map(|b| &b.data) {
                Some(BaseData::Projectile { item: Some(i), .. }) => format!("disarms to {i}"),
                Some(BaseData::Projectile { item: None, can_be_disabled: false }) => "cannot be disabled".to_string(),
                Some(BaseData::Projectile { item: None, .. }) => "no single weapon".to_string(),
                _ => "base not a projectile".to_string(),
            };
            bump(&mut pgre, class);
        }
        if let Some(t) = r.teleport {
            let class = match refs.get(t.door) {
                None => "destination missing".to_string(),
                Some(d) if forms.sig(d.base) != Some(*b"DOOR") => "destination not a door".into(),
                Some(d) => match d.teleport {
                    Some(back) if back.door == r.id => "reciprocal".into(),
                    Some(_) => "destination teleports elsewhere".into(),
                    None => "destination has no teleport".into(),
                },
            };
            if class != "reciprocal" && tel_samples.len() < 10 {
                tel_samples.push(format!("{} -> {}: {class}", r.id, t.door));
            }
            bump(&mut tel, class);
        }
    }

    // Quest items as the loader reads them (the schema's "Quest Item" header flag), and
    // every header flag bit set on winning item records whatever the schema names
    // (observation for types the sources give no quest flag).
    let mut quest_by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut quest_list = Vec::new();
    for b in forms.bases.values() {
        if forms.is_quest_item(b.id) {
            bump(&mut quest_by_type, sig_str(&b.sig));
            quest_list.push(json!({"formid": b.id.to_string(), "sig": sig_str(&b.sig), "edid": b.edid, "name": b.name}));
        }
    }
    let mut header_bits: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    for defs in lo.index.values() {
        let Some(d) = defs.last() else { continue };
        let r = lo.records[d.plugin as usize][d.ordinal as usize];
        if ItemKind::from_sig(&r.header.sig).is_none() {
            continue;
        }
        let e = header_bits.entry(sig_str(&r.header.sig)).or_default();
        bump(e, "records");
        for bit in 0..32 {
            if r.header.flags & (1 << bit) != 0 {
                bump(e, format!("bit {bit:02}"));
            }
        }
    }

    let player_base = forms.get(FormId::PLAYER_BASE).map(|b| json!({"sig": sig_str(&b.sig), "edid": b.edid}));
    let player_ref = refs.get(FormId::PLAYER_REF).map(|r| json!({"sig": sig_str(&r.sig), "base": r.base}));
    let mut issues = forms.issues.clone();
    issues.merge(refs.issues.clone());
    json!({
        "format": "fnv.sim.forms/1",
        "plugins": lo.plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        "bases": by_kind,
        "items": item_stats.iter().map(|(k, v)| (k.clone(), json!({"total": v[0], "value_known": v[1], "weight_known": v[2]}))).collect::<serde_json::Map<_, _>>(),
        "carried_items": {"targets": cnto_targets, "counts": cnto_counts, "unusual_samples": odd_entries},
        "quest_items": {"by_type": quest_by_type, "total": quest_list.len(), "items": quest_list, "item_record_header_bits": header_bits},
        "leveled_items": {"lists": lvl_lists, "entries": lvl_entries, "flags": lvl_flags, "targets": lvlo_targets},
        "globals": forms.globals.len(),
        "settings": forms.settings.len(),
        "player": {"base_0x7": player_base, "ref_0x14_placed": player_ref},
        "references": {
            "total": refs.refs.len(),
            "deleted": refs.deleted,
            "by_record": ref_sigs,
            "by_base_type": ref_base,
            "persistent_flag_vs_group": {
                "flag_and_persistent_group": persist[1][1],
                "flag_only": persist[1][0],
                "group_only": persist[0][1],
                "neither": persist[0][0],
            },
            "initially_disabled_flag": disabled,
            "parent_cell_is_grid_cell_not_storage_cell": reparented,
            "with_count": with_count,
            "owned": owned,
            "locks": {"levels": lock_levels, "keys": lock_keys, "on_base_type": lock_on},
            "teleports": {"classes": tel, "samples": tel_samples},
            "enable_parents": {"classes": ep, "samples": ep_samples},
            "placed_explosives": pgre,
            "merchant_containers": {
                "containers": refs.merchant_containers.len(),
                "actor_links": refs.merchant_containers.values().map(Vec::len).sum::<usize>(),
                "base_respawns_flag": refs
                    .merchant_containers
                    .keys()
                    .filter(|c| {
                        refs.get(**c).and_then(|r| forms.get(r.base)).is_some_and(|b| matches!(&b.data, BaseData::Container(c) if c.respawns))
                    })
                    .count(),
                "without_respawns_flag": merchant_without_respawn(forms, refs),
                "container_base_types": refs.merchant_containers.keys().fold(BTreeMap::<String, u64>::new(), |mut m, c| {
                    let t = refs.get(*c).and_then(|r| forms.sig(r.base)).map(|s| sig_str(&s)).unwrap_or_else(|| "missing".into());
                    *m.entry(t).or_default() += 1;
                    m
                }),
            },
        },
        "issues": issues.to_json(),
    })
}

/// Merchant containers whose base lacks the Respawns flag, with cell and merchants.
fn merchant_without_respawn(forms: &FormStore, refs: &RefTable) -> Vec<Value> {
    let cell_name: std::collections::HashMap<FormId, &String> = refs.cell_edids.iter().map(|(e, c)| (*c, e)).collect();
    refs.merchant_containers
        .iter()
        .filter_map(|(c, actors)| {
            let r = refs.get(*c)?;
            let b = forms.get(r.base)?;
            if matches!(&b.data, BaseData::Container(cd) if cd.respawns) {
                return None;
            }
            let merchants: Vec<Value> = actors
                .iter()
                .map(|a| {
                    let ar = refs.get(*a);
                    json!({"ref": a, "ref_edid": ar.and_then(|x| x.edid.clone()),
                           "base_edid": ar.and_then(|x| forms.get(x.base)).and_then(|x| x.edid.clone()),
                           "cell": ar.and_then(|x| x.cell),
                           "cell_edid": ar.and_then(|x| x.cell).and_then(|c| cell_name.get(&c).map(|s| s.to_string()))})
                })
                .collect();
            Some(json!({"container": c, "container_edid": r.edid, "base": r.base, "base_edid": b.edid,
                        "cell": r.cell, "cell_edid": r.cell.and_then(|c| cell_name.get(&c).map(|s| s.to_string())),
                        "pos": r.pos, "merchants": merchants}))
        })
        .collect()
}

/// Every inventory item base (fnv.sim.items/1), contract in docs/spec-notes/simulation.md.
fn items(forms: &FormStore) -> Value {
    let out: Vec<Value> = forms
        .bases
        .values()
        .filter_map(|b| {
            let BaseData::Item(i) = &b.data else { return None };
            Some(json!({
                "formid": b.id,
                "sig": sig_str(&b.sig),
                "edid": b.edid,
                "name": b.name,
                "value": i.value,
                "weight": i.weight,
                "health": i.health,
                "biped_flags": i.slots,
            }))
        })
        .collect();
    json!({"format": "fnv.sim.items/1", "count": out.len(), "items": out})
}

/// Every container base with its authored contents (fnv.sim.containers/1). Leveled
/// entries are listed as authored, not resolved (their resolution is random).
fn containers(forms: &FormStore) -> Value {
    let mut out = Vec::new();
    for b in forms.bases.values() {
        let BaseData::Container(c) = &b.data else { continue };
        let entries: Vec<_> = c
            .entries
            .iter()
            .map(|e| json!({"item": e.item, "count": e.count, "class": target_class(forms, e.item)}))
            .collect();
        out.push(json!({
            "formid": b.id,
            "edid": b.edid,
            "name": b.name,
            "respawns": c.respawns,
            "script": b.script,
            "entries": entries,
        }));
    }
    json!({"format": "fnv.sim.containers/1", "count": out.len(), "containers": out})
}

/// Teleport arrivals settled by physics: the destination cell's collision world (an
/// exterior cell with the squares around it, as `fnvtool phys cell` builds it), with the
/// references the simulation has in the world, and `fnv_phys`'s arrival rule
/// (physics.md §6.2), the one the game uses after a door.
struct PhysSettle<'a> {
    lo: &'a LoadOrder<'a>,
    schema: &'a Schema,
    /// Built on the first teleport.
    ix: Option<fnv_world::WorldIndex>,
    meshes: Option<Meshes>,
}

impl Settle for PhysSettle<'_> {
    fn settle(&mut self, w: &World, cell: Option<FormId>, pos: [f32; 3]) -> Option<([f32; 3], Value)> {
        let (lo, schema) = (self.lo, self.schema);
        let ix = self.ix.get_or_insert_with(|| fnv_world::WorldIndex::build(lo, schema));
        let centre = ix.cells.get(&cell?.0)?;
        let plugin = plugin_in_world(lo, schema);
        // The simulation decides for references it knows; the plugins' rule otherwise.
        let in_world = |pl: &fnv_world::placed::Placement| -> Result<(), &'static str> {
            let id = FormId(pl.form_id);
            if pl.deleted() || w.peek(id).is_some_and(|s| s.deleted()) {
                return Err("deleted");
            }
            match w.is_enabled(id) {
                Ok(true) => Ok(()),
                Ok(false) => Err("disabled"),
                Err(_) => plugin(pl),
            }
        };
        let mut b = build(lo, schema, ix, centre, 1, &[], PhysParams::default(), self.meshes.as_mut(), &in_world);
        let filter = Filter { exclude_layers: layers::character_ignores(), ..Default::default() };
        let a = b.world.arrival(pos, &CharacterParams::default(), &filter);
        let support = a.support.map(|h| {
            let owner = match h.owner {
                Owner::Terrain { x, y } => json!({"kind": "terrain", "grid": [x, y]}),
                Owner::Ref { form_id, object, layer } => {
                    json!({"kind": "ref", "formid": FormId(form_id), "object": object, "layer": layer})
                }
                Owner::Character { id } => json!({"kind": "character", "id": id}),
                Owner::Actor { form_id, layer, part } => json!({"kind": "actor", "formid": FormId(form_id), "layer": layer, "part": part}),
            };
            json!({"owner": owner, "z": h.point[2], "shell": h.shell})
        });
        Some((a.feet, support.unwrap_or(Value::Null)))
    }
}
