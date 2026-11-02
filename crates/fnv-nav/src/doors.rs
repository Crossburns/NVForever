//! Doors that open in place (DOOR references without XTEL: hinged doors, gates inside a
//! cell). The navmesh runs straight through them, with no door link [observed: e.g.
//! 0x0012C97C in OVCentralSewers01; docs/spec-notes/navmesh.md]; the graph learns where
//! they stand from their bounds, so a path that crosses one can stop in front of it
//! while it opens, and a locked one closes the way to actors without its key.
//!
//! Where an actor stops and how the follower waits are engine choices
//! (docs/design/navmesh.md §6.1), not observed game rules.

use crate::graph::{door_lock, DoorLock, NavGraph, Space};
use crate::path::PathParams;
use fnv_esm::{LoadOrder, Schema};
use fnv_world::WorldIndex;
use serde::Serialize;
use std::collections::HashMap;

/// Corners (min, max) of an object's bounds.
pub type Bounds = ([f32; 3], [f32; 3]);

/// A door that opens in place, as placed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InPlaceDoor {
    /// The door reference.
    pub door: u32,
    pub space: Space,
    pub position: [f32; 3],
    /// Row-major rotation (`fnv_world::transform::ref_rotation`).
    pub rotation: [f32; 9],
    pub scale: f32,
    /// The base's OBND corners in the model's frame.
    pub bounds: Bounds,
}

impl InPlaceDoor {
    /// A point in the door's model frame (x, y), from game space.
    fn local_xy(&self, p: [f32; 3]) -> [f32; 2] {
        let d = [p[0] - self.position[0], p[1] - self.position[1], p[2] - self.position[2]];
        let r = &self.rotation;
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        // Rᵀ d / s: rows of R are the model axes in game space.
        [(r[0] * d[0] + r[3] * d[1] + r[6] * d[2]) / s, (r[1] * d[0] + r[4] * d[1] + r[7] * d[2]) / s]
    }

    /// Corners of the bounds' footprint (x, y) in game space.
    pub fn footprint(&self) -> [[f32; 2]; 4] {
        let (lo, hi) = self.bounds;
        let r = &self.rotation;
        let s = self.scale;
        [[lo[0], lo[1]], [hi[0], lo[1]], [hi[0], hi[1]], [lo[0], hi[1]]].map(|[x, y]| {
            let z = 0.5 * (lo[2] + hi[2]);
            [
                self.position[0] + s * (r[0] * x + r[1] * y + r[2] * z),
                self.position[1] + s * (r[3] * x + r[4] * y + r[5] * z),
            ]
        })
    }

    /// Index (0: model x, 1: model y) of the bounds' thin horizontal axis: the door's
    /// normal; the other is its width.
    fn thin_axis(&self) -> usize {
        let (lo, hi) = self.bounds;
        if hi[0] - lo[0] <= hi[1] - lo[1] {
            0
        } else {
            1
        }
    }

    /// The middle of the bounds' footprint at the door's base, in game space.
    pub fn centre(&self) -> [f32; 3] {
        let (lo, hi) = self.bounds;
        let m = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), lo[2]];
        let r = &self.rotation;
        let s = self.scale;
        [
            self.position[0] + s * (r[0] * m[0] + r[1] * m[1] + r[2] * m[2]),
            self.position[1] + s * (r[3] * m[0] + r[4] * m[1] + r[5] * m[2]),
            self.position[2] + s * (r[6] * m[0] + r[7] * m[1] + r[8] * m[2]),
        ]
    }

    /// Unit horizontal normal of the door's face (its thin axis), in game space.
    pub fn normal(&self) -> [f32; 2] {
        let k = self.thin_axis();
        let r = &self.rotation;
        let v = [r[k], r[3 + k]];
        let len = v[0].hypot(v[1]).max(1e-6);
        [v[0] / len, v[1] / len]
    }

    /// Unit horizontal direction of the door's width (the other axis), in game space,
    /// and the bounds' extent along it from [`InPlaceDoor::centre`] (scaled).
    pub fn width_axis(&self) -> ([f32; 2], f32) {
        let k = 1 - self.thin_axis();
        let r = &self.rotation;
        let v = [r[k], r[3 + k]];
        let len = v[0].hypot(v[1]).max(1e-6);
        let (lo, hi) = self.bounds;
        ([v[0] / len, v[1] / len], 0.5 * self.scale * (hi[k] - lo[k]))
    }

    /// Half the door's thickness (its thin axis), scaled.
    pub fn half_thickness(&self) -> f32 {
        let (lo, hi) = self.bounds;
        let k = self.thin_axis();
        0.5 * self.scale * (hi[k] - lo[k])
    }

    /// The door's width: the larger horizontal extent of its bounds, scaled (how far
    /// its leaf can reach when it swings open).
    pub fn swing(&self) -> f32 {
        let (lo, hi) = self.bounds;
        self.scale * (hi[0] - lo[0]).max(hi[1] - lo[1])
    }

    /// How far before its bounds an actor with clearance `radius` stops to open it.
    pub fn stand_off(&self, radius: f32) -> f32 {
        radius + STOP_MARGIN.max(self.swing())
    }

    /// Game-space z range of the bounds.
    pub(crate) fn z_range(&self) -> (f32, f32) {
        let (lo, hi) = self.bounds;
        let mut zs = Vec::with_capacity(8);
        for x in [lo[0], hi[0]] {
            for y in [lo[1], hi[1]] {
                for z in [lo[2], hi[2]] {
                    let r = &self.rotation;
                    zs.push(self.position[2] + self.scale * (r[6] * x + r[7] * y + r[8] * z));
                }
            }
        }
        (zs.iter().copied().fold(f32::INFINITY, f32::min), zs.iter().copied().fold(f32::NEG_INFINITY, f32::max))
    }

    /// Where segment `a`–`b` enters the door's footprint, as a fraction of the segment
    /// (0 when `a` is inside), or `None` when it misses (seen from above).
    pub fn entry(&self, a: [f32; 3], b: [f32; 3]) -> Option<f32> {
        let (lo, hi) = self.bounds;
        let (p, q) = (self.local_xy(a), self.local_xy(b));
        let d = [q[0] - p[0], q[1] - p[1]];
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for k in 0..2 {
            if d[k].abs() < 1e-6 {
                if p[k] < lo[k] || p[k] > hi[k] {
                    return None;
                }
                continue;
            }
            let (mut ta, mut tb) = ((lo[k] - p[k]) / d[k], (hi[k] - p[k]) / d[k]);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return None;
            }
        }
        Some(t0)
    }
}

/// How far in front of a door's bounds an actor stops to open it, beyond its clearance
/// radius, at least, game units [engine choice; how close the game's NPCs come is not
/// known]. A hinged door swings through a quarter circle as wide as the door, on a side
/// the records do not tell, so the stop is also at least the door's width away
/// ([`InPlaceDoor::swing`]): an actor waiting there is clear of it whichever way it opens.
pub const STOP_MARGIN: f32 = 24.0;

/// Every placed DOOR reference without XTEL whose base has bounds, in the space of its
/// cell (deleted references left out). Which of them can actually open (their model has
/// "Open" and "Close" sequences) is for the caller to say: fnv-nav does not read models.
pub fn in_place_doors(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex) -> Vec<InPlaceDoor> {
    use rayon::prelude::*;
    use fnv_world::fields::{winner, Fields};
    let mut ids: Vec<(u32, u32)> = ix.refs.iter().filter(|(_, e)| &e.sig == b"REFR").map(|(&r, e)| (r, e.cell)).collect();
    ids.sort_unstable();
    // Cheap test first: the base's record signature and XTEL, without decoding the rest.
    let doors: Vec<(u32, u32, u32)> = ids
        .par_iter()
        .filter_map(|&(r, cell)| {
            let (pi, rec) = winner(lo, r)?;
            let f = Fields::new(rec, pi, schema).ok()?;
            let base = f.formid(b"NAME", lo)?;
            (&winner(lo, base)?.1.header.sig == b"DOOR" && f.raw(b"XTEL").is_none()).then_some((r, cell, base))
        })
        .collect();
    let mut bases: HashMap<u32, Option<Bounds>> = HashMap::new();
    let mut out = Vec::new();
    for (r, cell, base) in doors {
        let Some(c) = ix.cells.get(&cell) else { continue };
        let Some(pl) = fnv_world::placed::placement(lo, schema, r).filter(|pl| !pl.deleted()) else { continue };
        let bounds = *bases.entry(base).or_insert_with(|| fnv_world::placed::door_record(lo, schema, base).and_then(|d| d.bounds));
        let Some(bounds) = bounds else { continue };
        let space = match c.world {
            Some(w) => Space::World(w),
            None => Space::Interior(c.form_id),
        };
        out.push(InPlaceDoor {
            door: r,
            space,
            position: pl.position,
            rotation: fnv_world::transform::ref_rotation(pl.rotation),
            scale: pl.scale,
            bounds,
        });
    }
    out
}

/// Whether triangle `t` (corners) and the door's footprint overlap seen from above
/// (separating axes of both), and its heights meet the door's (a navmesh lies at most a
/// little below a door's base [engine choice: 64 units]).
fn overlaps(d: &InPlaceDoor, t: [[f32; 3]; 3]) -> bool {
    box_overlaps(d.footprint(), d.z_range(), t, 64.0, 0.0)
}

/// Whether triangle `t` (corners) and a box's footprint `f` overlap seen from above
/// (separating axes of both), and the triangle's heights meet the box's z range `zlo`
/// .. `zhi`, from `below` under its base up to `top_clear` under its top.
pub(crate) fn box_overlaps(f: [[f32; 2]; 4], (zlo, zhi): (f32, f32), t: [[f32; 3]; 3], below: f32, top_clear: f32) -> bool {
    let (tlo, thi) = t.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), p| (a.min(p[2]), b.max(p[2])));
    if thi < zlo - below || tlo > zhi - top_clear {
        return false;
    }
    let tri: [[f32; 2]; 3] = t.map(|p| [p[0], p[1]]);
    let axes = |poly: &[[f32; 2]]| -> Vec<[f32; 2]> {
        (0..poly.len()).map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            [a[1] - b[1], b[0] - a[0]]
        }).collect()
    };
    for ax in axes(&f).into_iter().chain(axes(&tri)) {
        let proj = |poly: &[[f32; 2]]| poly.iter().map(|p| p[0] * ax[0] + p[1] * ax[1]).fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
        let ((a0, a1), (b0, b1)) = (proj(&f), proj(&tri));
        if a1 < b0 || b1 < a0 {
            return false;
        }
    }
    true
}

impl NavGraph {
    /// Learn where doors that open in place stand: each door's triangles (those under
    /// its bounds) and its lock (`lock`, the authored XLOC). Replaces what was learnt
    /// before. A door no triangle lies under is left out.
    pub fn set_in_place_doors(&mut self, doors: Vec<InPlaceDoor>, lock: impl Fn(u32) -> Option<DoorLock>) {
        for d in self.in_place.keys() {
            self.door_locks.remove(d);
        }
        self.in_place.clear();
        self.door_tris.clear();
        for d in doors {
            let (lo, hi) = d.bounds;
            let reach = d.scale.max(0.0) * (0..3).map(|k| lo[k].abs().max(hi[k].abs())).fold(0.0, f32::max) * 1.5;
            let tris: Vec<u32> = self
                .triangles_near(d.space, d.position[0], d.position[1], reach)
                .into_iter()
                .filter(|&t| self.usable(t) && overlaps(&d, self.corners(t)))
                .collect();
            if tris.is_empty() {
                continue;
            }
            for t in tris {
                self.door_tris.entry(t).or_default().push(d.door);
            }
            if let Some(l) = lock(d.door) {
                self.door_locks.insert(d.door, l);
            }
            self.in_place.insert(d.door, d);
        }
        self.stats.in_place_doors = self.in_place.len();
    }

    /// [`NavGraph::set_in_place_doors`] with every DOOR reference without XTEL of the
    /// load order and its authored lock.
    pub fn learn_in_place_doors(&mut self, lo: &LoadOrder, schema: &Schema, ix: &WorldIndex) {
        self.set_in_place_doors(in_place_doors(lo, schema, ix), |d| door_lock(lo, schema, d));
    }

    /// Whether an actor with `p` may cross triangle `t`: no locked door it cannot open
    /// (without the key, not its own) stands on it.
    pub fn door_tri_open(&self, t: u32, p: &PathParams) -> bool {
        let Some(doors) = self.door_tris.get(&t) else { return true };
        doors.iter().all(|d| match self.door_locks.get(d) {
            _ if p.refused_doors.contains(d) => false,
            None => true,
            Some(_) if p.open_locked || p.owned_doors.contains(d) => true,
            Some(l) => l.key.is_some_and(|k| p.keys.contains(&k)),
        })
    }
}
