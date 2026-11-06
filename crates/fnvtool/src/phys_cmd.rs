//! `fnvtool phys ...`: the Layer 4 physics world built from the game data (fnv-phys).
//! JSON schemas: docs/spec-notes/physics.md §7.

use crate::esm_cmd::open_load_order;
use crate::{PhysCmd, WorldArgs};
use anyhow::{anyhow, bail, Context};
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_collide::CollisionObject;
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_phys::terrain::height_at;
use fnv_phys::{
    layers, ref_collision_with, CharacterInput, CharacterParams, Filter, ModelCollision, Owner, PhysParams, PhysWorld, RefCollisionOptions, RefPose, Stroke,
    WaterArea,
};
use fnv_world::land::{load_land, LandParts, SIDE, SPACING};
use fnv_world::placed::placement;
use fnv_world::{cell_of, Cell, WorldIndex, CELL_SIZE};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn run(cmd: PhysCmd) -> anyhow::Result<Value> {
    match cmd {
        PhysCmd::Cell { w, cell, world, x, y, radius, drops, seconds, nif_schema, no_meshes, list_refs, drops_file, rays } => {
            let rays = rays
                .iter()
                .map(|r| {
                    let v: Vec<f32> = r.split(',').map(|t| t.trim().parse::<f32>()).collect::<Result<_, _>>().with_context(|| format!("bad ray {r}"))?;
                    match v[..] {
                        [x, y, z, dx, dy, dz] => Ok(([x, y, z], [dx, dy, dz], 10_000.0)),
                        [x, y, z, dx, dy, dz, m] => Ok(([x, y, z], [dx, dy, dz], m)),
                        _ => bail!("a ray is x,y,z,dx,dy,dz[,max]: {r}"),
                    }
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let mut drops = drops.iter().map(|d| parse_point(d)).collect::<anyhow::Result<Vec<_>>>()?;
            let mut seconds = seconds;
            let labels = match &drops_file {
                None => Vec::new(),
                Some(f) => {
                    let text = std::fs::read_to_string(f).with_context(|| f.display().to_string())?;
                    let doc = DropsFile::parse(&serde_json::from_str(&text).with_context(|| f.display().to_string())?)?;
                    if !drops.is_empty() {
                        bail!("give --drop or --drops-file, not both");
                    }
                    seconds = doc.seconds.unwrap_or(seconds);
                    drops = doc.drops.iter().map(|d| d.1).collect();
                    doc.drops.into_iter().map(|d| d.0).collect()
                }
            };
            let mut meshes = if no_meshes { None } else { Meshes::open(&w.lo.data_dir, &w.schema, nif_schema.as_deref())? };
            with_world(&w, |lo, schema, ix| {
                let centre = match (cell, world, x, y) {
                    (Some(c), None, None, None) => ix.cell_by_name(&c).ok_or_else(|| anyhow!("no cell {c}"))?,
                    (None, Some(wn), Some(x), Some(y)) => {
                        let ws = ix.world_by_name(&wn).ok_or_else(|| anyhow!("no worldspace {wn}"))?;
                        let id = ws.grid.get(&(x, y)).ok_or_else(|| anyhow!("{wn} has no cell at {x},{y}"))?;
                        &ix.cells[id]
                    }
                    _ => bail!("give --cell, or --world with --x and --y"),
                };
                let mut v = cell_cmd(lo, schema, ix, centre, radius.max(0), (&drops, seconds), meshes.as_mut(), (list_refs, &rays))?;
                if drops_file.is_some() {
                    let probes = v["probes"].as_array().cloned().unwrap_or_default();
                    v["observations"] = json!(labels
                        .iter()
                        .zip(probes)
                        .map(|(l, p)| json!({"label": l, "feet": p["final_feet"], "support": p["support"]}))
                        .collect::<Vec<_>>());
                }
                Ok(v)
            })
        }
        PhysCmd::Walk { w, script, cell, world, radius, every, nif_schema, no_meshes } => {
            let text = std::fs::read_to_string(&script).with_context(|| script.display().to_string())?;
            let script = Script::parse(&serde_json::from_str(&text).with_context(|| script.display().to_string())?)?;
            let mut meshes = if no_meshes { None } else { Meshes::open(&w.lo.data_dir, &w.schema, nif_schema.as_deref())? };
            with_world(&w, |lo, schema, ix| {
                let centre = match (cell, world) {
                    (Some(c), None) => ix.cell_by_name(&c).ok_or_else(|| anyhow!("no cell {c}"))?,
                    (None, Some(wn)) => {
                        let ws = ix.world_by_name(&wn).ok_or_else(|| anyhow!("no worldspace {wn}"))?;
                        let g = cell_of(script.start[0], script.start[1]);
                        let id = ws.grid.get(&g).ok_or_else(|| anyhow!("{wn} has no cell at {},{}", g.0, g.1))?;
                        &ix.cells[id]
                    }
                    _ => bail!("give --cell or --world"),
                };
                walk_cmd(lo, schema, ix, centre, radius.max(0), &script, every.max(1), meshes.as_mut())
            })
        }
        PhysCmd::Heights { w, world } => with_world(&w, |lo, schema, ix| heights_cmd(lo, schema, ix, &world)),
        PhysCmd::Terrain { w, world, samples } => with_world(&w, |lo, schema, ix| terrain_cmd(lo, schema, ix, &world, samples)),
    }
}

fn with_world(
    w: &WorldArgs,
    f: impl FnOnce(&LoadOrder, &Schema, &WorldIndex) -> anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let plugins = open_load_order(&w.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let ix = WorldIndex::build(&lo, &schema);
    let mut v = f(&lo, &schema, &ix)?;
    v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
    Ok(v)
}

/// A ray: origin, direction, length.
type Ray = ([f32; 3], [f32; 3], f32);

/// A probe drop point: x, y and optionally z, world units.
type DropPoint = (f32, f32, Option<f32>);

/// "x,y" or "x,y,z" in world units.
fn parse_point(s: &str) -> anyhow::Result<(f32, f32, Option<f32>)> {
    let v: Vec<f32> =
        s.split(',').map(|t| t.trim().parse::<f32>()).collect::<Result<_, _>>().with_context(|| format!("bad point {s}"))?;
    match v[..] {
        [x, y] => Ok((x, y, None)),
        [x, y, z] => Ok((x, y, Some(z))),
        _ => bail!("a point is x,y or x,y,z: {s}"),
    }
}

fn fid(v: u32) -> Value {
    json!(formid_str(v))
}

fn owner_json(o: &Owner) -> Value {
    match *o {
        Owner::Terrain { x, y } => json!({"kind": "terrain", "grid": [x, y]}),
        Owner::Ref { form_id, object, layer } => json!({
            "kind": "ref",
            "formid": fid(form_id),
            "object": object,
            "layer": layer,
            "layer_name": layers::name(layer as u32),
        }),
        Owner::Character { id } => json!({"kind": "character", "id": id}),
        Owner::Actor { form_id, layer, part } => json!({"kind": "actor", "formid": fid(form_id), "layer": layer, "layer_name": layers::name(layer as u32), "part": part}),
    }
}

/// Collision objects of models, read through workstream 1's converter
/// (`fnv_collide::nif::collision_objects`) from the game's archives and loose files.
/// fnv-phys itself never reads NIF blocks (docs/design/layer4.md §7).
pub(crate) struct Meshes {
    vfs: fnv_vfs::Vfs,
    schema: fnv_nif::Schema,
    cache: BTreeMap<String, Result<Arc<Vec<CollisionObject>>, String>>,
    /// Water volumes (Water-layer phantoms) of the models read, by path.
    water: BTreeMap<String, Arc<Vec<CollisionObject>>>,
    /// Door models' "Open" and "Close" animation (`fnv_collide::anim`), by path, for the
    /// models read that have both.
    anims: BTreeMap<String, Arc<fnv_collide::anim::ModelAnim>>,
}

impl Meshes {
    /// `None` when no NIF schema is given and none sits next to the ESM schema.
    pub(crate) fn open(data_dir: &Path, esm_schema: &Path, nif_schema: Option<&Path>) -> anyhow::Result<Option<Meshes>> {
        let dir: PathBuf = match nif_schema {
            Some(d) => d.to_path_buf(),
            None => match esm_schema.parent().map(|p| p.join("nif")).filter(|d| d.is_dir()) {
                Some(d) => d,
                None => return Ok(None),
            },
        };
        let schema = fnv_nif::Schema::load_dir(&dir).with_context(|| dir.display().to_string())?;
        let vfs = fnv_vfs::Vfs::open_default(data_dir).with_context(|| data_dir.display().to_string())?;
        Ok(Some(Meshes { vfs, schema, cache: BTreeMap::new(), water: BTreeMap::new(), anims: BTreeMap::new() }))
    }

    /// A door model's animation, when it has "Open" and "Close" sequences that move its
    /// nodes (reads the model if it was not read yet).
    pub(crate) fn door_anim(&mut self, path: &str) -> Option<Arc<fnv_collide::anim::ModelAnim>> {
        let _ = self.objects(path);
        self.anims.get(path).cloned()
    }
}

impl ModelCollision for Meshes {
    /// The solid collision objects of a model (empty when it has none).
    fn objects(&mut self, path: &str) -> Result<Arc<Vec<CollisionObject>>, String> {
        if let Some(r) = self.cache.get(path) {
            return r.clone();
        }
        let r = match self.vfs.read(path) {
            None => Err("not found".to_string()),
            Some(Err(e)) => Err(e.to_string()),
            Some(Ok(data)) => NifFile::parse(&data, Some(&self.schema)).map_err(|e| e.to_string()).map(|file| {
                let dec = Decoded::new(&file, &data, &self.schema);
                let (solid, phantoms, _) = fnv_collide::nif::collision_and_phantoms(&file, &dec);
                let water: Vec<CollisionObject> = phantoms.into_iter().filter(|o| o.layer == layers::WATER).collect();
                if !water.is_empty() {
                    self.water.insert(path.to_string(), Arc::new(water));
                }
                if solid.iter().any(|o| o.motion == fnv_collide::Motion::Keyframed) {
                    let anim = fnv_collide::anim::ModelAnim::new(&file, &dec);
                    if anim.door().is_some() {
                        self.anims.insert(path.to_string(), Arc::new(anim));
                    }
                }
                Arc::new(solid)
            }),
        };
        self.cache.insert(path.to_string(), r.clone());
        r
    }

    fn water(&mut self, path: &str) -> Arc<Vec<CollisionObject>> {
        self.water.get(path).cloned().unwrap_or_default()
    }
}

/// The physics world of a cell and its surroundings, with what went into it.
pub(crate) struct Built<'a> {
    pub(crate) world: PhysWorld,
    cells: Vec<&'a Cell>,
    pub(crate) grids: BTreeMap<(i32, i32), Option<Vec<f32>>>,
    no_heights: usize,
    counts: BTreeMap<&'static str, usize>,
    by_base: BTreeMap<String, usize>,
    collision_source: Option<Value>,
    /// Lowest and highest z of the loaded terrain and collision references' placements.
    z_range: (f32, f32),
    /// Bounds (min x, min y, max x, max y) of the collision references' placements.
    xy_bounds: [f32; 4],
    /// The references added to the world, in FormID order.
    added: Vec<AddedRef>,
}

/// A reference added to the world, for `--list-refs`.
pub(crate) struct AddedRef {
    form_id: u32,
    base: Option<(u32, [u8; 4], Option<String>)>,
    position: [f32; 3],
    scale: f32,
    /// (layer, motion, shell × scale) per collision object.
    objects: Vec<(u8, fnv_collide::Motion, f32)>,
}

impl Built<'_> {
    /// Where to drop a probe by default: an exterior cell's centre, else the middle of
    /// the placements that have collision.
    fn default_drop(&self, centre: &Cell) -> (f32, f32) {
        match centre.grid {
            Some((gx, gy)) => ((gx as f32 + 0.5) * CELL_SIZE, (gy as f32 + 0.5) * CELL_SIZE),
            None if self.xy_bounds[0] <= self.xy_bounds[2] => {
                ((self.xy_bounds[0] + self.xy_bounds[2]) / 2.0, (self.xy_bounds[1] + self.xy_bounds[3]) / 2.0)
            }
            None => (0.0, 0.0),
        }
    }

    /// Range for vertical probe rays: the loaded content with 1,000 units to spare.
    fn ray_span(&self) -> (f32, f32) {
        (self.z_range.1 + 1000.0, self.z_range.0 - 1000.0)
    }

    fn json(&self, schema: &str, centre: &Cell, radius: i32, character: &CharacterParams) -> anyhow::Result<Value> {
        Ok(json!({
            "schema": schema,
            "cell": fid(centre.form_id),
            "edid": centre.edid,
            "world": centre.world.map(fid),
            "grid": centre.grid.map(|(x, y)| json!([x, y])),
            "radius": radius,
            "params": serde_json::to_value(self.world.params)?,
            "character": serde_json::to_value(character)?,
            "cells": self.cells.iter().map(|c| json!({
                "cell": fid(c.form_id),
                "grid": c.grid.map(|(x, y)| json!([x, y])),
                "land": c.grid.and_then(|g| self.grids.get(&g)).is_some_and(|h| h.is_some()),
                "refs": c.refs.len(),
            })).collect::<Vec<_>>(),
            "terrain": {"cells": self.grids.len() - self.no_heights, "no_heights": self.no_heights},
            "water": self.world.water().map(|(a, z)| json!({
                "grid": match a { WaterArea::Exterior(x, y) => json!([x, y]), WaterArea::Interior => Value::Null },
                "surface": z,
            })).chain(self.world.placed_water().map(|w| json!({
                "grid": Value::Null,
                "ref": fid(w.form_id),
                "surface": w.surface,
                "centre": w.centre,
                "half_extents": w.half,
            }))).collect::<Vec<_>>(),
            "refs": {
                "counts": self.counts,
                "by_base_type": self.by_base,
                "collision_source": self.collision_source,
            },
        }))
    }
}

/// Whether a placed reference is in the world: `Ok`, or the `refs.counts` name of why
/// not (e.g. `"initially_disabled"`).
pub(crate) type InWorld<'f> = &'f dyn Fn(&fnv_world::placed::Placement) -> Result<(), &'static str>;

/// The plugins' own rule (a game just started): not deleted, not initially disabled, and
/// enabled through its enable parent.
pub(crate) fn plugin_in_world<'a>(
    lo: &'a LoadOrder,
    schema: &'a Schema,
) -> impl Fn(&fnv_world::placed::Placement) -> Result<(), &'static str> + 'a {
    move |pl| {
        if pl.deleted() {
            Err("deleted")
        } else if pl.initially_disabled() {
            Err("initially_disabled")
        } else if !parent_allows(lo, schema, pl, 0) {
            Err("disabled_by_parent")
        } else {
            Ok(())
        }
    }
}

/// Build the static world of `centre` (and, for an exterior cell, the exterior cells
/// within `radius` grid squares of it and of every point of `around`), with the
/// references `in_world` admits.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build<'a>(
    lo: &LoadOrder,
    schema: &Schema,
    ix: &'a WorldIndex,
    centre: &'a Cell,
    radius: i32,
    around: &[(f32, f32)],
    params: PhysParams,
    meshes: Option<&mut Meshes>,
    in_world: InWorld,
) -> Built<'a> {
    build_with(lo, schema, ix, centre, radius, around, params, meshes, in_world, &|_| RefCollisionOptions::default())
}

/// [`build`] with per-reference collision options (`fnv_phys::ref_collision_with`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_with<'a>(
    lo: &LoadOrder,
    schema: &Schema,
    ix: &'a WorldIndex,
    centre: &'a Cell,
    radius: i32,
    around: &[(f32, f32)],
    params: PhysParams,
    mut meshes: Option<&mut Meshes>,
    in_world: InWorld,
    options: &dyn Fn(&fnv_world::placed::Placement) -> RefCollisionOptions<'static>,
) -> Built<'a> {
    let mut world = PhysWorld::new(params);
    let mut cells: Vec<&Cell> = vec![centre];
    let mut grids: BTreeMap<(i32, i32), Option<Vec<f32>>> = BTreeMap::new();
    let ws = centre.world.and_then(|w| ix.worldspaces.get(&w));
    if let (Some(ws), Some(g0)) = (ws, centre.grid) {
        cells.clear();
        let mut squares = std::collections::BTreeSet::new();
        for (gx, gy) in std::iter::once(g0).chain(around.iter().map(|&(x, y)| cell_of(x, y))) {
            for y in gy - radius..=gy + radius {
                for x in gx - radius..=gx + radius {
                    squares.insert((y, x));
                }
            }
        }
        for (y, x) in squares {
            {
                let Some(id) = ws.grid.get(&(x, y)) else { continue };
                let c = &ix.cells[id];
                cells.push(c);
                let h = c.land.and_then(|l| load_land(lo, schema, l, LandParts::HEIGHTS)).and_then(|l| l.heights);
                grids.insert((x, y), h);
            }
        }
    }
    // Water: every loaded exterior cell's surface over its square (`Cell::water_surface`,
    // physics.md §6.6). Not an interior's XCLW: in NVDLC03SLH2OINT the player swam at the
    // height of a placed water object, not at the cell's XCLW [observed: sw_h2o]; placed
    // water comes with its reference below.
    for c in &cells {
        if let (Some((x, y)), Some(w)) = (c.grid, ws) {
            world.set_water(WaterArea::Exterior(x, y), c.water_surface(w.default_water_height));
        }
    }
    let mut no_heights = 0;
    let mut z_range = (f32::INFINITY, f32::NEG_INFINITY);
    let mut xy_bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for (&g, h) in &grids {
        match h {
            Some(h) => {
                world.add_terrain(g, h);
                for &z in h {
                    z_range = (z_range.0.min(z), z_range.1.max(z));
                }
            }
            None => no_heights += 1,
        }
    }

    // References: those of the loaded cells, plus the worldspace's persistent references
    // standing in a loaded grid square.
    let mut ref_ids: Vec<u32> = cells.iter().flat_map(|c| c.refs.iter().copied()).collect();
    let persistent = ws.and_then(|w| w.persistent_cell).and_then(|p| ix.cells.get(&p));
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_base: BTreeMap<String, usize> = BTreeMap::new();
    let mut added = Vec::new();
    if let Some(p) = persistent.filter(|_| !grids.is_empty()) {
        for &r in &p.refs {
            if let Some(pl) = placement(lo, schema, r) {
                if grids.contains_key(&cell_of(pl.position[0], pl.position[1])) {
                    ref_ids.push(r);
                }
            }
        }
    }
    ref_ids.sort_unstable();
    ref_ids.dedup();
    for &r in &ref_ids {
        *counts.entry("placed").or_default() += 1;
        let Some(pl) = placement(lo, schema, r) else {
            *counts.entry("unreadable").or_default() += 1;
            continue;
        };
        if let Err(why) = in_world(&pl) {
            *counts.entry(why).or_default() += 1;
            continue;
        }
        let models = meshes.as_deref_mut().map(|m| m as &mut dyn ModelCollision);
        let rc = ref_collision_with(lo, schema, &pl, models, &mut |what| *counts.entry(what).or_default() += 1, options(&pl));
        if let Some(sig) = rc.base_sig {
            *by_base.entry(String::from_utf8_lossy(&sig).into_owned()).or_default() += 1;
        }
        if !rc.water.is_empty() {
            world.add_placed_water(&RefPose::from_placement(&pl), &rc.water);
        }
        let objects = rc.objects;
        if objects.is_empty() {
            continue;
        }
        let rep = world.add_ref(&RefPose::from_placement(&pl), &objects);
        added.push(AddedRef {
            form_id: pl.form_id,
            base: pl.base.and_then(|b| fnv_world::placed::base_object(lo, schema, b)).map(|b| (b.form_id, b.sig, b.edid)),
            position: pl.position,
            scale: pl.scale,
            objects: objects.iter().map(|o| (o.layer, o.motion, o.shell * pl.scale)).collect(),
        });
        z_range = (z_range.0.min(pl.position[2]), z_range.1.max(pl.position[2]));
        let [x, y, _] = pl.position;
        xy_bounds = [xy_bounds[0].min(x), xy_bounds[1].min(y), xy_bounds[2].max(x), xy_bounds[3].max(y)];
        *counts.entry("with_collision").or_default() += 1;
        *counts.entry("colliders").or_default() += rep.colliders;
        if !rep.errors.is_empty() {
            *counts.entry("object_errors").or_default() += rep.errors.len();
        }
    }
    let collision_source = meshes.as_deref().map(|ms| {
        let unreadable: Vec<Value> =
            ms.cache.iter().filter_map(|(p, r)| r.as_ref().err().map(|e| json!({"model": p, "error": e}))).take(20).collect();
        json!({
            "kind": "fnv_collide::nif",
            "models": ms.cache.len(),
            "models_with_collision": ms.cache.values().filter(|r| r.as_ref().is_ok_and(|o| !o.is_empty())).count(),
            "models_unreadable": ms.cache.values().filter(|r| r.is_err()).count(),
            "unreadable_examples": unreadable,
        })
    });

    if !z_range.0.is_finite() {
        z_range = (0.0, 0.0);
    }
    Built { world, cells, grids, no_heights, counts, by_base, collision_source, z_range, xy_bounds, added }
}

/// Build the world around `centre`, then drop a probe character at each drop point.
/// Whether a reference's enable parent leaves it enabled at the start of a game, by the
/// renderer's rule (`engine/nvforever/src/world.rs`): the parent must itself be enabled
/// (not deleted or initially disabled, its own parent followed), inverted for "opposite";
/// a missing parent counts as enabled unless "opposite", a chain deeper than 16 as
/// disabled. Disabled references have no collision [observed by the owner in game,
/// physics.md §4].
fn parent_allows(lo: &LoadOrder, schema: &Schema, p: &fnv_world::placed::Placement, depth: usize) -> bool {
    match p.enable_parent {
        None => true,
        Some(_) if depth > 16 => false,
        Some((parent, opposite)) => match placement(lo, schema, parent) {
            Some(pp) => (!pp.deleted() && !pp.initially_disabled() && parent_allows(lo, schema, &pp, depth + 1)) != opposite,
            None => !opposite,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn cell_cmd(
    lo: &LoadOrder,
    schema: &Schema,
    ix: &WorldIndex,
    centre: &Cell,
    radius: i32,
    (drops, seconds): (&[DropPoint], f32),
    meshes: Option<&mut Meshes>,
    (list_refs, rays): (bool, &[Ray]),
) -> anyhow::Result<Value> {
    // The game loads cells around the player wherever it is [hypothesis: uGridsToLoad],
    // so each probe gets the cells within `radius` of its own square too.
    let around: Vec<(f32, f32)> = drops.iter().map(|d| (d.0, d.1)).collect();
    let mut b = build(lo, schema, ix, centre, radius, &around, PhysParams::default(), meshes, &plugin_in_world(lo, schema));
    let ref_list = list_refs.then(|| ref_list(&mut b));
    let ray_hits: Vec<Value> = rays
        .iter()
        .map(|&(o, d, max)| {
            let hit = b.world.queries().ray(o, d, max, &Filter::default());
            json!({"origin": o, "dir": d, "hit": hit.map(|h| json!({"owner": owner_json(&h.owner), "point": h.point, "normal": h.normal, "distance": h.distance, "shell": h.shell}))})
        })
        .collect();
    let character = CharacterParams::default();
    let probes: Vec<Value> = if drops.is_empty() {
        let (cx, cy) = b.default_drop(centre);
        vec![probe(&mut b, character, (cx, cy, None), seconds)]
    } else {
        drops.iter().map(|&d| probe(&mut b, character, d, seconds)).collect()
    };
    let mut v = b.json("fnv.phys.cell/1", centre, radius, &character)?;
    v["world_stats"] = serde_json::to_value(b.world.stats())?;
    v["probes"] = json!(probes);
    if let Some(l) = ref_list {
        v["ref_list"] = json!(l);
    }
    if !rays.is_empty() {
        v["rays"] = json!(ray_hits);
    }
    Ok(v)
}

/// Every reference in the world with its collision objects' layers and shells, and the
/// first hit of a vertical ray at its position from above the loaded content (all
/// layers): whether something dropped there would meet this reference first.
fn ref_list(b: &mut Built) -> Vec<Value> {
    let (top, bottom) = b.ray_span();
    let added = std::mem::take(&mut b.added);
    let out = added
        .iter()
        .map(|r| {
            let [x, y, _] = r.position;
            let hit = b.world.queries().ray([x, y, top], [0.0, 0.0, -1.0], top - bottom, &Filter::default());
            json!({
                "formid": fid(r.form_id),
                "base": r.base.as_ref().map(|(f, sig, edid)| json!({
                    "formid": fid(*f), "type": String::from_utf8_lossy(sig), "edid": edid,
                })),
                "position": r.position,
                "scale": r.scale,
                "objects": r.objects.iter().map(|&(layer, motion, shell)| json!({
                    "layer": layer,
                    "layer_name": layers::name(layer as u32),
                    "motion": format!("{motion:?}").to_ascii_lowercase(),
                    "shell": shell,
                })).collect::<Vec<_>>(),
                "top_hit": hit.map(|h| json!({"owner": owner_json(&h.owner), "z": h.point[2], "normal": h.normal, "shell": h.shell})),
            })
        })
        .collect();
    b.added = added;
    out
}

/// Drop a character at (x, y): from `z` if given, else 100 units above whatever a
/// downward ray finds (or z = 0 when it finds nothing); simulate `seconds` with no input.
fn probe(b: &mut Built, params: CharacterParams, (x, y, z): (f32, f32, Option<f32>), seconds: f32) -> Value {
    let (top, bottom) = b.ray_span();
    let (world, grids) = (&mut b.world, &b.grids);
    let from = z.map_or(top, |z| z.max(top));
    let ground = world.queries().ray([x, y, from], [0.0, 0.0, -1.0], from - bottom, &Filter::default());
    let start = [x, y, z.unwrap_or_else(|| ground.map_or(0.0, |g| g.point[2]) + 100.0)];
    let terrain_z = grids
        .get(&cell_of(x, y))
        .and_then(|h| h.as_deref())
        .and_then(|h| {
            let (gx, gy) = cell_of(x, y);
            height_at(h, x - gx as f32 * CELL_SIZE, y - gy as f32 * CELL_SIZE)
        });
    let mut c = world.add_character(0, params, start);
    let solid = Filter { exclude: vec![c.key()], exclude_layers: layers::character_ignores(), ..Default::default() };
    let steps = (seconds / world.params.dt).round().max(1.0) as usize;
    let mut landed: Option<(f32, f32)> = None;
    let mut last = None;
    for i in 0..steps {
        let s = world.move_character(&mut c, CharacterInput::default());
        world.step();
        if landed.is_none() {
            if let Some(v) = s.landed_speed {
                landed = Some(((i + 1) as f32 * world.params.dt, v));
            }
        }
        last = Some(s);
    }
    let last = last.expect("at least one step");
    // What the probe rests on: the arrival rule's ray under the feet (physics.md §6.2).
    let f = last.feet;
    let support = world.queries().ray([f[0], f[1], f[2] + params.height / 2.0], [0.0, 0.0, -1.0], 4.5 * params.height, &solid);
    world.remove_character(c);
    json!({
        "start": start,
        "ground_ray": ground.map(|g| json!({"owner": owner_json(&g.owner), "z": g.point[2], "normal": g.normal})),
        "terrain_z": terrain_z,
        "landed_after_s": landed.map(|l| l.0),
        "landing_speed": landed.map(|l| l.1),
        "final_feet": last.feet,
        "grounded": last.grounded,
        "sliding": last.sliding,
        "swimming": last.swimming,
        "water_surface": world.water_surface_at(f[0], f[1]),
        "rest_above_terrain": terrain_z.map(|t| last.feet[2] - t),
        "gap": last.gap,
        "support": support.map(|h| json!({"owner": owner_json(&h.owner), "z": h.point[2], "shell": h.shell})),
        "rest_above_support": support.map(|h| last.feet[2] - h.point[2]),
        "touched": last.touched.iter().map(owner_json).collect::<Vec<_>>(),
    })
}

/// Labelled drops (`fnv.phys.drops/1`): `{"schema", "seconds"?, "drops": [{"label", "at":
/// [x, y] | [x, y, z], ...}]}`; other fields (e.g. what a drop is aimed at) are ignored.
struct DropsFile {
    seconds: Option<f32>,
    drops: Vec<(String, DropPoint)>,
}

impl DropsFile {
    fn parse(v: &Value) -> anyhow::Result<DropsFile> {
        if v["schema"] != json!("fnv.phys.drops/1") {
            bail!("a drops file needs \"schema\": \"fnv.phys.drops/1\"");
        }
        let seconds = match v.get("seconds") {
            None => None,
            Some(s) => Some(s.as_f64().filter(|s| *s > 0.0 && *s <= 600.0).ok_or_else(|| anyhow!("\"seconds\" must be in (0, 600]"))? as f32),
        };
        let mut drops = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for d in v["drops"].as_array().ok_or_else(|| anyhow!("\"drops\" must be a list"))? {
            let label = d["label"].as_str().ok_or_else(|| anyhow!("a drop needs a \"label\""))?.to_string();
            if !seen.insert(label.clone()) {
                bail!("duplicate drop label {label}");
            }
            let at: Vec<f32> = d["at"]
                .as_array()
                .ok_or_else(|| anyhow!("drop {label}: \"at\" must be [x, y] or [x, y, z]"))?
                .iter()
                .map(|x| x.as_f64().map(|x| x as f32).filter(|x| x.is_finite()))
                .collect::<Option<_>>()
                .ok_or_else(|| anyhow!("drop {label}: \"at\" must hold finite numbers"))?;
            let point = match at[..] {
                [x, y] => (x, y, None),
                [x, y, z] => (x, y, Some(z)),
                _ => bail!("drop {label}: \"at\" must be [x, y] or [x, y, z]"),
            };
            drops.push((label, point));
        }
        Ok(DropsFile { seconds, drops })
    }
}

/// A walk script (`fnv.phys.walk-script/1`): where to start, the step rate and input
/// segments, and optional parameter overrides.
struct Script {
    /// x, y and optionally z (feet); without z the walk starts on what a downward ray
    /// finds.
    start: [f32; 2],
    start_z: Option<f32>,
    hz: u32,
    segments: Vec<Segment>,
    params: BTreeMap<String, f64>,
    /// Observation points: `(label, step)`, from `observe`.
    observe: Vec<(String, usize)>,
    /// Loose objects added before the walk (`props`).
    props: Vec<Prop>,
}

/// A model's collision placed in the world before the walk, e.g. a clutter item for the
/// character to push (`"props": [{"label", "model", "at": [x, y, z], "rot"?: [x, y, z]}]`,
/// angles in radians as a reference's). It is added as a reference with FormID
/// 0xFF000000 + its index (above every placed one).
struct Prop {
    label: String,
    model: String,
    at: [f32; 3],
    rot: [f32; 3],
}

struct Segment {
    steps: usize,
    velocity: [f32; 2],
    /// Velocity (x, y, z) used instead of `velocity` while the character swims.
    swim_velocity: Option<[f32; 3]>,
    /// The swim stroke whose speed cycle scales `swim_velocity` (physics.md §6.6).
    stroke: Option<Stroke>,
    jump: bool,
    /// Feet put here (teleport, the swimming state kept, as the game's `SetPos`) before
    /// the segment's first step.
    place: Option<[f32; 3]>,
    /// Horizontal view direction (x, y) for the landing profiles (physics.md §6.4.1).
    view: Option<[f32; 2]>,
}

impl Script {
    fn parse(v: &Value) -> anyhow::Result<Script> {
        if v["schema"] != json!("fnv.phys.walk-script/1") {
            bail!("a walk script needs \"schema\": \"fnv.phys.walk-script/1\"");
        }
        let num = |x: &Value, what: &str| -> anyhow::Result<f32> {
            x.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or_else(|| anyhow!("{what} must be a finite number"))
        };
        let start = v["start"].as_array().filter(|a| a.len() == 2 || a.len() == 3).ok_or_else(|| anyhow!("\"start\" must be [x, y] or [x, y, z]"))?;
        let hz = v.get("hz").map_or(Ok(60), |h| h.as_u64().filter(|h| (1..=1000).contains(h)).ok_or_else(|| anyhow!("\"hz\" must be 1..=1000")))? as u32;
        let mut segments = Vec::new();
        for s in v["segments"].as_array().ok_or_else(|| anyhow!("\"segments\" must be a list"))? {
            let steps = s["steps"].as_u64().ok_or_else(|| anyhow!("a segment needs \"steps\""))? as usize;
            let velocity = match s.get("velocity") {
                None => [0.0, 0.0],
                Some(w) => [num(&w[0], "velocity[0]")?, num(&w[1], "velocity[1]")?],
            };
            let swim_velocity = match s.get("swim_velocity") {
                None => None,
                Some(w) => Some([num(&w[0], "swim_velocity[0]")?, num(&w[1], "swim_velocity[1]")?, num(&w[2], "swim_velocity[2]")?]),
            };
            let stroke = match s.get("stroke").map(|v| v.as_str()) {
                None => None,
                Some(Some("forward")) => Some(Stroke::Forward),
                Some(Some("back")) => Some(Stroke::Back),
                Some(Some("side")) => Some(Stroke::Side),
                Some(_) => bail!("\"stroke\" is \"forward\", \"back\" or \"side\""),
            };
            let place = match s.get("place") {
                None => None,
                Some(w) => Some([num(&w[0], "place[0]")?, num(&w[1], "place[1]")?, num(&w[2], "place[2]")?]),
            };
            segments.push(Segment {
                steps,
                velocity,
                swim_velocity,
                stroke,
                jump: s.get("jump").and_then(Value::as_bool).unwrap_or(false),
                place,
                view: match s.get("view") {
                    None => None,
                    Some(w) => Some([num(&w[0], "view[0]")?, num(&w[1], "view[1]")?]),
                },
            });
        }
        let total: usize = segments.iter().map(|s| s.steps).sum();
        if total > 1_000_000 {
            bail!("script too long: {total} steps");
        }
        let mut params = BTreeMap::new();
        if let Some(p) = v.get("params").and_then(Value::as_object) {
            for (k, x) in p {
                params.insert(k.clone(), x.as_f64().filter(|v| v.is_finite()).ok_or_else(|| anyhow!("param {k} must be a number"))?);
            }
        }
        // "observe": {"from_step": s0, "every": n, "count": k} labels the feet after steps
        // s0 + n, s0 + 2n, ... s0 + kn as "t<seconds since s0, two decimals>".
        let mut observe = Vec::new();
        if let Some(o) = v.get("observe") {
            let field = |k: &str| o[k].as_u64().map(|x| x as usize).ok_or_else(|| anyhow!("\"observe\" needs a whole number \"{k}\""));
            let (from, every, count) = (field("from_step")?, field("every")?, field("count")?);
            if every == 0 || from + every * count > total {
                bail!("\"observe\" must stay within the script's {total} steps");
            }
            for i in 1..=count {
                observe.push((format!("t{:.2}", (i * every) as f64 / hz as f64), from + i * every));
            }
        }
        let mut props = Vec::new();
        for (i, pr) in v.get("props").and_then(Value::as_array).into_iter().flatten().enumerate() {
            let at = pr["at"].as_array().filter(|a| a.len() == 3).ok_or_else(|| anyhow!("prop {i}: \"at\" must be [x, y, z]"))?;
            let rot = match pr.get("rot") {
                None => [0.0; 3],
                Some(r) => [num(&r[0], "rot[0]")?, num(&r[1], "rot[1]")?, num(&r[2], "rot[2]")?],
            };
            props.push(Prop {
                label: pr["label"].as_str().map_or_else(|| format!("prop{i}"), str::to_string),
                model: pr["model"].as_str().ok_or_else(|| anyhow!("prop {i} needs a \"model\" path"))?.to_string(),
                at: [num(&at[0], "at[0]")?, num(&at[1], "at[1]")?, num(&at[2], "at[2]")?],
                rot,
            });
        }
        Ok(Script {
            props,
            observe,
            start: [num(&start[0], "start[0]")?, num(&start[1], "start[1]")?],
            start_z: start.get(2).map(|z| num(z, "start[2]")).transpose()?,
            hz,
            segments,
            params,
        })
    }

    /// World and character parameters: defaults with the script's overrides.
    fn parameters(&self) -> anyhow::Result<(PhysParams, CharacterParams)> {
        let mut w = PhysParams { dt: 1.0 / self.hz as f32, ..Default::default() };
        let mut c = CharacterParams::default();
        for (k, &v) in &self.params {
            let f = v as f32;
            match k.as_str() {
                "gravity" => w.gravity = f,
                "radius" => c.radius = f,
                "height" => c.height = f,
                "bottom_slope" => c.bottom_slope = f,
                "bottom_flat" => c.bottom_flat = f,
                "step_height" => c.step_height = f,
                "step_width" => c.step_width = f,
                "max_slope_deg" => c.max_slope_deg = f,
                "jump_height" => c.jump_height = f,
                "skin" => c.skin = f,
                "max_fall_speed" => c.max_fall_speed = Some(f),
                "push_mass" => c.push_mass = f,
                "swim_depth" => c.swim_depth = f,
                "wade_depth" => c.wade_depth = f,
                "float_depth" => c.float_depth = f,
                "swim_sink_speed" => c.swim_sink_speed = f,
                "swim_jump_mult" => c.swim_jump_mult = f,
                _ => bail!("unknown parameter {k}"),
            }
        }
        Ok((w, c))
    }
}

/// Run a walk script in the world built around its start; every `every`th step is traced.
#[allow(clippy::too_many_arguments)]
fn walk_cmd(
    lo: &LoadOrder,
    schema: &Schema,
    ix: &WorldIndex,
    centre: &Cell,
    radius: i32,
    script: &Script,
    every: usize,
    meshes: Option<&mut Meshes>,
) -> anyhow::Result<Value> {
    let (params, character) = script.parameters()?;
    let mut meshes = meshes;
    let mut b = build(lo, schema, ix, centre, radius, &[], params, meshes.as_deref_mut(), &plugin_in_world(lo, schema));
    let mut props = Vec::new();
    for (i, pr) in script.props.iter().enumerate() {
        let m = meshes.as_deref_mut().ok_or_else(|| anyhow!("props need the meshes (no --no-meshes, and the NIF schema)"))?;
        let objects = m.objects(&pr.model).map_err(|e| anyhow!("prop {}: {}: {e}", pr.label, pr.model))?;
        let id = 0xFF00_0000 + i as u32;
        let pose = RefPose { form_id: id, position: pr.at, rotation: fnv_world::transform::ref_rotation(pr.rot), scale: 1.0 };
        let rep = b.world.add_ref(&pose, &objects);
        if rep.bodies == 0 {
            bail!("prop {}: {} has no collision", pr.label, pr.model);
        }
        let mass: f32 = objects.iter().map(|o| o.mass).sum();
        // One object: its body's pose gives the prop's reference pose (what the game's
        // GetPos reads); otherwise the first body's position is reported.
        let local = (objects.len() == 1 && rep.bodies == 1).then(|| objects[0].node_transform.compose(&objects[0].body_transform));
        props.push((id, json!({"label": pr.label, "model": pr.model, "formid": fid(id), "bodies": rep.bodies, "havok_mass": mass}), local));
    }
    let prop_pos = |w: &fnv_phys::PhysWorld, id: u32, local: &Option<fnv_collide::Iso>| {
        w.ref_bodies(id).first().map(|b| match local {
            Some(l) => ref_pose_of(b, l).0,
            None => b.translation,
        })
    };
    let [x, y] = script.start;
    let start_z = match script.start_z {
        Some(z) => z,
        None => {
            let (top, bottom) = b.ray_span();
            let solid = Filter { exclude_layers: layers::character_ignores(), ..Default::default() };
            let hit = b.world.queries().ray([x, y, top], [0.0, 0.0, -1.0], top - bottom, &solid);
            let hit = hit.ok_or_else(|| anyhow!("nothing below the start {x},{y}"))?;
            hit.point[2] + character.rest_gap(hit.shell)
        }
    };
    let world = &mut b.world;
    let mut c = world.add_character(0, character, [x, y, start_z]);
    let total: usize = script.segments.iter().map(|s| s.steps).sum();
    let (mut step, mut path, mut prev) = (0usize, 0.0f64, c.feet);
    let (mut min_z, mut max_z) = (c.feet[2], c.feet[2]);
    let (mut jumps, mut landings, mut airborne, mut swim_steps) = (0usize, 0usize, 0usize, 0usize);
    let dt = world.params.dt;
    let mut stroke_t = 0.0f32;
    let mut touched: BTreeMap<Owner, usize> = BTreeMap::new();
    let mut trace = Vec::new();
    let mut observations = Vec::new();
    for seg in &script.segments {
        if let Some(p) = seg.place {
            world.place_character(&mut c, p);
            prev = c.feet;
        }
        for i in 0..seg.steps {
            let jump = seg.jump && i == 0;
            let was_grounded = c.grounded;
            // While swimming, the stroke cycle scales the speed by the time since the
            // swimmer started to move (taken at the middle of the step).
            let input = match seg.swim_velocity.filter(|v| c.swimming && v.iter().any(|&x| x != 0.0)) {
                Some([vx, vy, vz]) => {
                    let f = seg.stroke.map_or(1.0, |s| s.factor(stroke_t + dt / 2.0));
                    stroke_t += dt;
                    CharacterInput { velocity: [vx * f, vy * f], jump, vertical: vz * f, view: seg.view }
                }
                None => {
                    stroke_t = 0.0;
                    CharacterInput { velocity: seg.velocity, jump, vertical: 0.0, view: seg.view }
                }
            };
            let s = world.move_character(&mut c, input);
            world.step();
            step += 1;
            jumps += (jump && was_grounded) as usize;
            landings += s.landed_speed.is_some() as usize;
            airborne += !s.grounded as usize;
            swim_steps += s.swimming as usize;
            path += ((s.feet[0] - prev[0]) as f64).hypot((s.feet[1] - prev[1]) as f64);
            prev = s.feet;
            min_z = min_z.min(s.feet[2]);
            max_z = max_z.max(s.feet[2]);
            for o in &s.touched {
                *touched.entry(*o).or_default() += 1;
            }
            for (label, _) in script.observe.iter().filter(|(_, at)| *at == step) {
                let mut o = json!({"label": label, "feet": s.feet, "grounded": s.grounded, "swimming": s.swimming});
                if !props.is_empty() {
                    o["props"] = json!(props.iter().map(|(id, _, l)| prop_pos(world, *id, l)).collect::<Vec<_>>());
                }
                observations.push(o);
            }
            if step.is_multiple_of(every) || step == total {
                let mut t = json!({
                    "step": step,
                    "feet": s.feet,
                    "vertical_speed": c.vertical_speed,
                    "grounded": s.grounded,
                    "sliding": s.sliding,
                    "landed_speed": s.landed_speed,
                    "gap": s.gap,
                    "swimming": s.swimming,
                    "touched": s.touched.iter().map(owner_json).collect::<Vec<_>>(),
                });
                if !props.is_empty() {
                    t["props"] = json!(props.iter().map(|(id, _, l)| prop_pos(world, *id, l)).collect::<Vec<_>>());
                }
                trace.push(t);
            }
        }
    }
    let mut v = b.json("fnv.phys.walk/1", centre, radius, &character)?;
    v["start"] = json!([x, y, start_z]);
    v["summary"] = json!({
        "steps": step,
        "seconds": step as f64 / script.hz as f64,
        "end": c.feet,
        "horizontal_path": path,
        "min_z": min_z,
        "max_z": max_z,
        "jumps": jumps,
        "landings": landings,
        "airborne_steps": airborne,
        "swimming_steps": swim_steps,
        "touched": touched.iter().map(|(o, n)| json!({"owner": owner_json(o), "steps": n})).collect::<Vec<_>>(),
    });
    v["trace"] = json!(trace);
    if !props.is_empty() {
        v["props"] = json!(props
            .iter()
            .map(|(id, p, local)| {
                let mut p = p.clone();
                let b = b.world.ref_bodies(*id).first().copied();
                p["end"] = json!(b.map(|b| b.translation));
                p["end_rotation"] = json!(b.map(|b| b.rotation));
                p["asleep"] = json!(b.map(|b| b.sleeping));
                if let (Some(b), Some(l)) = (b, local) {
                    let (pos, rot) = ref_pose_of(&b, l);
                    p["end_ref"] = json!(pos);
                    p["end_ref_angles_deg"] = json!(fnv_world::transform::ref_angles(&rot).map(f32::to_degrees));
                }
                p
            })
            .collect::<Vec<_>>());
    }
    if !script.observe.is_empty() {
        v["observations"] = json!(observations);
    }
    Ok(v)
}

/// The reference pose (position, row-major rotation) of a body placed at `ref ∘ local`
/// (scale 1): `R = R_body R_localᵀ`, `t = t_body − R t_local`.
fn ref_pose_of(b: &fnv_phys::BodyState, local: &fnv_collide::Iso) -> ([f32; 3], [f32; 9]) {
    use fnv_world::transform::{mul, transpose};
    let r = mul(&b.rotation, &transpose(&local.rotation));
    let t = local.translation;
    let rt = [0, 3, 6].map(|i| r[i] * t[0] + r[i + 1] * t[1] + r[i + 2] * t[2]);
    ([b.translation[0] - rt[0], b.translation[1] - rt[1], b.translation[2] - rt[2]], r)
}

/// Sample points of `phys heights`, in grid steps (128 units) east and north of a cell's
/// south-west corner: four interior vertices, then four points inside grid squares away
/// from both diagonals (physics.md §7).
pub const HEIGHT_POINTS: [[f32; 2]; 8] =
    [[5.0, 9.0], [27.0, 3.0], [13.0, 30.0], [30.0, 22.0], [8.2, 8.6], [24.6, 7.3], [7.7, 23.2], [23.9, 24.4]];

/// Every landscape cell of a worldspace: the physics terrain's height under
/// [`HEIGHT_POINTS`] (vertical rays, each cell in a world of its own).
fn heights_cmd(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, wn: &str) -> anyhow::Result<Value> {
    let ws = ix.world_by_name(wn).ok_or_else(|| anyhow!("no worldspace {wn}"))?;
    let cells: Vec<((i32, i32), u32)> = ws.grid.iter().filter_map(|(&g, c)| Some((g, ix.cells[c].land?))).collect();
    let rows: Vec<Value> = cells
        .par_iter()
        .map(|&(grid, lf)| {
            let Some(h) = load_land(lo, schema, lf, LandParts::HEIGHTS).and_then(|l| l.heights) else {
                return json!({"grid": [grid.0, grid.1], "heights": Value::Null});
            };
            let mut world = PhysWorld::new(PhysParams::default());
            world.add_terrain(grid, &h);
            let (lo_h, hi_h) = h.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
            let (x0, y0) = (grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE);
            let q = world.queries();
            let heights: Vec<Value> = HEIGHT_POINTS
                .iter()
                .map(|&[u, v]| {
                    let from = hi_h + 100.0;
                    let at = [x0 + u * SPACING, y0 + v * SPACING, from];
                    q.ray(at, [0.0, 0.0, -1.0], from - lo_h + 200.0, &Filter::default()).map_or(Value::Null, |hit| json!(hit.point[2]))
                })
                .collect();
            json!({"grid": [grid.0, grid.1], "heights": heights})
        })
        .collect();
    Ok(json!({
        "schema": "fnv.phys.heights/1",
        "world": fid(ws.form_id),
        "edid": ws.edid,
        "points": HEIGHT_POINTS,
        "cells": rows,
    }))
}

/// Deterministic sample points in a cell: `n` scattered points (a fixed low-discrepancy
/// sequence), plus vertices and grid-line midpoints along the diagonal.
fn samples(n: usize) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            // Additive recurrence with the plastic number (R2 sequence).
            let (a1, a2) = (0.754_877_7_f64, 0.569_840_3_f64);
            let u = (0.5 + a1 * (i as f64 + 1.0)).fract() as f32;
            let v = (0.5 + a2 * (i as f64 + 1.0)).fract() as f32;
            (u * CELL_SIZE, v * CELL_SIZE)
        })
        .collect();
    for k in 1..SIDE - 1 {
        let p = k as f32 * SPACING;
        out.push((p, p)); // vertex
        out.push((p + SPACING / 2.0, p)); // on a grid line
    }
    out
}

/// Every landscape cell of a worldspace: vertical rays against the terrain collider
/// compared with the analytic surface of the same LAND heights.
fn terrain_cmd(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, wn: &str, n: usize) -> anyhow::Result<Value> {
    let ws = ix.world_by_name(wn).ok_or_else(|| anyhow!("no worldspace {wn}"))?;
    let cells: Vec<((i32, i32), u32)> = ws.grid.iter().filter_map(|(&g, c)| Some((g, ix.cells[c].land?))).collect();
    let pts = samples(n);
    struct CellResult {
        grid: (i32, i32),
        no_heights: bool,
        rays: usize,
        misses: usize,
        wrong_owner: usize,
        sum_err: f64,
        worst: (f32, f32, f32, f32, f32),
    }
    let results: Vec<CellResult> = cells
        .par_iter()
        .map(|&(grid, lf)| {
            let mut r = CellResult { grid, no_heights: false, rays: 0, misses: 0, wrong_owner: 0, sum_err: 0.0, worst: (0.0, 0.0, 0.0, 0.0, -1.0) };
            let Some(h) = load_land(lo, schema, lf, LandParts::HEIGHTS).and_then(|l| l.heights) else {
                r.no_heights = true;
                return r;
            };
            let mut world = PhysWorld::new(PhysParams::default());
            world.add_terrain(grid, &h);
            let (lo_h, hi_h) = h.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
            let (x0, y0) = (grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE);
            let q = world.queries();
            for &(x, y) in &pts {
                let want = height_at(&h, x, y).expect("sample inside the cell");
                r.rays += 1;
                let from = hi_h + 100.0;
                match q.ray([x0 + x, y0 + y, from], [0.0, 0.0, -1.0], from - lo_h + 200.0, &Filter::default()) {
                    None => r.misses += 1,
                    Some(hit) => {
                        if hit.owner != (Owner::Terrain { x: grid.0, y: grid.1 }) {
                            r.wrong_owner += 1;
                        }
                        let e = (hit.point[2] - want).abs();
                        r.sum_err += e as f64;
                        if e > r.worst.4 {
                            r.worst = (x0 + x, y0 + y, hit.point[2], want, e);
                        }
                    }
                }
            }
            r
        })
        .collect();
    let rays: usize = results.iter().map(|r| r.rays).sum();
    let misses: usize = results.iter().map(|r| r.misses).sum();
    let hits = rays - misses;
    let worst = results.iter().filter(|r| r.worst.4 >= 0.0).max_by(|a, b| a.worst.4.total_cmp(&b.worst.4));
    Ok(json!({
        "schema": "fnv.phys.terrain/1",
        "world": fid(ws.form_id),
        "edid": ws.edid,
        "cells": results.iter().filter(|r| !r.no_heights).count(),
        "land_without_heights": results.iter().filter(|r| r.no_heights).map(|r| json!([r.grid.0, r.grid.1])).collect::<Vec<_>>(),
        "samples_per_cell": pts.len(),
        "rays": rays,
        "misses": misses,
        "wrong_owner": results.iter().map(|r| r.wrong_owner).sum::<usize>(),
        "mean_abs_error": if hits > 0 { results.iter().map(|r| r.sum_err).sum::<f64>() / hits as f64 } else { 0.0 },
        "max_abs_error": worst.map(|w| w.worst.4),
        "worst": worst.map(|w| json!({
            "grid": [w.grid.0, w.grid.1],
            "point": [w.worst.0, w.worst.1],
            "physics_z": w.worst.2,
            "analytic_z": w.worst.3,
        })),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_parse() {
        assert_eq!(parse_point("1,-2").unwrap(), (1.0, -2.0, None));
        assert_eq!(parse_point(" 1.5, 2 ,3").unwrap(), (1.5, 2.0, Some(3.0)));
        assert!(parse_point("1").is_err() && parse_point("a,b").is_err() && parse_point("1,2,3,4").is_err());
    }

    #[test]
    fn walk_scripts_parse() {
        let v = json!({
            "schema": "fnv.phys.walk-script/1",
            "start": [10.0, -20.0],
            "segments": [{"steps": 5, "velocity": [100.0, 0.0]}, {"steps": 1, "jump": true}],
            "params": {"step_height": 30.0, "gravity": 700.0},
        });
        let s = Script::parse(&v).unwrap();
        assert_eq!((s.start, s.start_z, s.hz), ([10.0, -20.0], None, 60));
        assert_eq!(s.segments.len(), 2);
        assert!(s.segments[1].jump && s.segments[1].velocity == [0.0, 0.0]);
        let (w, c) = s.parameters().unwrap();
        assert_eq!((w.gravity, c.step_height), (700.0, 30.0));
        assert!(Script::parse(&json!({"start": [0, 0], "segments": []})).is_err());
        let bad = json!({"schema": "fnv.phys.walk-script/1", "start": [0, 0], "segments": [], "params": {"nope": 1}});
        assert!(Script::parse(&bad).unwrap().parameters().is_err());
    }

    #[test]
    fn walk_observations_and_drop_files_parse() {
        let v = json!({
            "schema": "fnv.phys.walk-script/1", "start": [0.0, 0.0], "hz": 60,
            "segments": [{"steps": 30}, {"steps": 90, "velocity": [344.0, 0.0]}],
            "observe": {"from_step": 30, "every": 15, "count": 6},
        });
        let s = Script::parse(&v).unwrap();
        assert_eq!(s.observe.first(), Some(&("t0.25".to_string(), 45)));
        assert_eq!(s.observe.last(), Some(&("t1.50".to_string(), 120)));
        let mut late = v.clone();
        late["observe"]["count"] = json!(7);
        assert!(Script::parse(&late).is_err(), "beyond the last step");
        let d = DropsFile::parse(&json!({"schema": "fnv.phys.drops/1", "seconds": 4,
            "drops": [{"label": "a", "at": [1, 2, 3], "ref": "0x1"}, {"label": "b", "at": [1, 2]}]})).unwrap();
        assert_eq!(d.seconds, Some(4.0));
        assert_eq!(d.drops, vec![("a".to_string(), (1.0, 2.0, Some(3.0))), ("b".to_string(), (1.0, 2.0, None))]);
        let dup = json!({"schema": "fnv.phys.drops/1", "drops": [{"label": "a", "at": [1, 2]}, {"label": "a", "at": [1, 2]}]});
        assert!(DropsFile::parse(&dup).is_err());
    }

    #[test]
    fn height_points_avoid_triangle_edges() {
        for (i, [u, v]) in HEIGHT_POINTS.iter().enumerate() {
            assert!(*u > 0.0 && *v > 0.0 && *u < 32.0 && *v < 32.0);
            let (fu, fv) = (u.fract(), v.fract());
            if i < 4 {
                assert_eq!((fu, fv), (0.0, 0.0), "vertex");
            } else {
                assert!((fu - fv).abs() > 0.05 && (fu + fv - 1.0).abs() > 0.05, "{u},{v} near a diagonal");
            }
        }
    }

    #[test]
    fn samples_stay_inside_the_cell() {
        let s = samples(64);
        assert_eq!(s.len(), 64 + 2 * (SIDE - 2));
        assert!(s.iter().all(|&(x, y)| (0.0..=CELL_SIZE).contains(&x) && (0.0..=CELL_SIZE).contains(&y)));
    }
}
