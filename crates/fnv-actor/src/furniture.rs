//! Furniture an actor can use (docs/spec-notes/furniture.md): the entry points a
//! furniture model carries (`BSFurnitureMarker`), the FURN records that name the models,
//! and which clips an actor plays to get on, stay on and get off.

use fnv_esm::{LoadOrder, Schema};
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_nif::value::Value as NifValue;
use fnv_sim::extract::{self, first, text, Issues};
use fnv_sim::FormId;
use serde::Serialize;

/// One position of a model's furniture marker [documented: nif.xml `FurniturePosition`,
/// the layout for Fallout 3 / New Vegas files; `schemas/nif/scene.json`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Marker {
    /// In the model's own space, game units.
    pub offset: [f32; 3],
    pub orientation: u16,
    pub ref_1: u8,
    pub ref_2: u8,
}

impl Marker {
    /// The orientation as an angle in radians [hypothesis: the field counts
    /// thousandths of a radian; observed values are 0, 1570, 3141 and 4712, a quarter
    /// turn apart, furniture.md §2].
    pub fn angle(&self) -> f32 {
        self.orientation as f32 / 1000.0
    }
}

/// Every position of every `BSFurnitureMarker` block of a model, in file order.
pub fn markers(file: &NifFile, dec: &Decoded) -> Vec<Marker> {
    let mut out = Vec::new();
    for (i, b) in file.blocks.iter().enumerate() {
        if b.type_name != "BSFurnitureMarker" {
            continue;
        }
        let Some(s) = dec.get(i as i32) else { continue };
        for p in s.list("positions").unwrap_or_default() {
            let NifValue::Struct(p) = p else { continue };
            let Some(&[x, y, z]) = p.floats("offset").and_then(|f| <&[f32; 3]>::try_from(f).ok()) else { continue };
            out.push(Marker {
                offset: [x, y, z],
                orientation: p.int("orientation").unwrap_or(0) as u16,
                ref_1: p.int("position_ref_1").unwrap_or(0) as u8,
                ref_2: p.int("position_ref_2").unwrap_or(0) as u8,
            });
        }
    }
    out
}

/// A furniture base (FURN) [documented: xEdit wbDefinitionsFNV.pas FURN record;
/// `schemas/fnv/objects.json`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Furniture {
    pub id: FormId,
    pub edid: Option<String>,
    pub name: Option<String>,
    /// `MODL` as a VFS path.
    pub model: Option<String>,
    /// `MNAM` ("Marker Flags") as a little-endian number; `None` when absent or not
    /// four bytes long.
    pub marker_flags: Option<u32>,
    pub has_script: bool,
}

impl Furniture {
    /// Whether the model's marker `index` (file order) may be used: its bit of the
    /// marker flags is set [observed: `Chair01L`, `Chair01R` and `Chair01F` share one
    /// model with left, right and front markers and carry bits 0, 1 and 2,
    /// furniture.md §1]. Without flags every marker counts.
    pub fn marker_enabled(&self, index: usize) -> bool {
        match self.marker_flags {
            Some(f) => index < 30 && f & (1 << index) != 0,
            None => true,
        }
    }

    /// Bit 30: furniture to sit on [observed: set on every base whose markers are 10–20,
    /// and on a few with marker 22, furniture.md §1].
    pub fn sit(&self) -> bool {
        self.marker_flags.is_some_and(|f| f & 0x4000_0000 != 0)
    }

    /// Bit 31: furniture to sleep on [observed: set on every base whose markers are
    /// 1–6].
    pub fn sleep(&self) -> bool {
        self.marker_flags.is_some_and(|f| f & 0x8000_0000 != 0)
    }
}

/// The members of form list `list` (its FLST's LNAM entries, as load-order FormIDs);
/// empty when it is not a form list.
pub(crate) fn list_members(lo: &LoadOrder, list: FormId) -> Vec<FormId> {
    let Some(d) = lo.index.get(&list.0).and_then(|d| d.last()) else { return Vec::new() };
    let r = lo.records[d.plugin as usize][d.ordinal as usize];
    if &r.header.sig != b"FLST" {
        return Vec::new();
    }
    let subs = r.subrecords().unwrap_or_default();
    subs.iter()
        .filter(|s| &s.sig == b"LNAM")
        .filter_map(|s| extract::global(lo, d.plugin as usize, u32::from_le_bytes(s.data.get(..4)?.try_into().ok()?)).ok().flatten())
        .collect()
}

/// Every winning, non-deleted FURN.
pub fn furniture(lo: &LoadOrder, schema: &Schema, issues: &mut Issues) -> Vec<Furniture> {
    let mut out = Vec::new();
    for (&g, defs) in &lo.index {
        let Some(d) = defs.last() else { continue };
        let r = lo.records[d.plugin as usize][d.ordinal as usize];
        if &r.header.sig != b"FURN" || r.header.flags & 0x20 != 0 {
            continue;
        }
        let dec = match extract::decode(r, schema) {
            Ok(d) => d,
            Err(e) => {
                issues.note("furniture does not decode", || format!("{}: {e}", FormId(g)));
                continue;
            }
        };
        let mnam = r.subrecords().ok().and_then(|subs| subs.into_iter().find(|s| &s.sig == b"MNAM")).map(|s| s.data);
        let marker_flags = match mnam.as_deref() {
            Some(&[a, b, c, d]) => Some(u32::from_le_bytes([a, b, c, d])),
            Some(other) => {
                issues.note("furniture marker flags are not 4 bytes", || format!("{}: {} bytes", FormId(g), other.len()));
                None
            }
            None => None,
        };
        out.push(Furniture {
            id: FormId(g),
            edid: first(&dec, b"EDID").and_then(text),
            name: first(&dec, b"FULL").and_then(text),
            model: first(&dec, b"MODL").and_then(text).map(|m| crate::idle::meshes_path(m.trim())).filter(|m| m != "meshes\\"),
            marker_flags,
            has_script: first(&dec, b"SCRI").is_some(),
        });
    }
    out.sort_by_key(|f| f.id);
    out
}

// ---- which clip: the furniture part of the IDLE tree --------------------------------

/// Condition functions the furniture idles test, as CTDA function indices (script opcode
/// − 0x1000) [observed opcodes: `schemas/script/functions.json`; `GET_ATTACKED` and
/// `IS_CHILD` are xEdit's indices, not seen in a compiled script].
pub mod func {
    pub const GET_SLEEPING: u16 = 0x31;
    pub const GET_STAGE: u16 = 0x3A;
    pub const GET_ATTACKED: u16 = 0x3F;
    pub const GET_IS_SEX: u16 = 0x46;
    pub const GET_IS_ID: u16 = 0x48;
    pub const GET_RANDOM_PERCENT: u16 = 0x4D;
    pub const GET_IS_ALERTED: u16 = 0x5B;
    pub const GET_SITTING: u16 = 0x9F;
    pub const GET_FURNITURE_MARKER_ID: u16 = 0xA0;
    pub const IS_CURRENT_FURNITURE_REF: u16 = 0xA2;
    pub const IS_CURRENT_FURNITURE_OBJ: u16 = 0xA3;
    pub const IS_CHILD: u16 = 0x16D;
    pub const IS_PC_1ST_PERSON: u16 = 0x188;
}

/// The values `GetSitting` and `GetSleeping` take [observed: the IDLE tree picks the
/// loop played on the furniture at 1, the getting-on clip at 2, fidgets at 3 and the
/// getting-off clip at 4, furniture.md §3; ai-observer.md §7 saw 3 on seated actors and
/// 4 on actors getting up].
pub mod state {
    pub const NONE: u8 = 0;
    /// The loop played while on the furniture is looked up.
    pub const LOOP: u8 = 1;
    pub const GETTING_ON: u8 = 2;
    pub const ON: u8 = 3;
    pub const GETTING_OFF: u8 = 4;
}

/// Whether a marker ID is a bed's (the actor sleeps) rather than a seat's [documented:
/// catalogue notes for GetFurnitureMarkerID: 1 to 10 beds, 11 to 20 chairs; observed:
/// the IDLE tree tests GetSleeping only under IDs 1–4]. The one model with marker ID 10, the crucifix
/// (NVCrucifixFurniture, MNAM bit 30 "sit"), is sat on: its IDLE group NVCrucifixIdles
/// gets on, stays and gets off by GetSitting 2, 3 and 4 [observed: the IDLE records;
/// owner's playtest list 7 item 34: the crucified got off their crosses, as the picker
/// found no getting-on clip for a "bed"]. So beds are 1 to 9 here.
pub fn is_bed(marker_id: u8) -> bool {
    (1..=9).contains(&marker_id)
}

/// An actor using furniture, as the IDLE conditions see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct User {
    /// [`state`] value for `GetSitting` (seats) or `GetSleeping` (beds).
    pub state: u8,
    /// The marker's `ref_1`.
    pub marker_id: u8,
    pub furniture_ref: FormId,
    pub furniture_base: FormId,
    /// The actor's base form (`GetIsID`).
    pub actor_base: FormId,
    /// The player seen through the first-person camera.
    pub first_person: bool,
    pub female: bool,
    pub child: bool,
    pub alerted: bool,
    pub attacked: bool,
    /// 0–99.
    pub random_percent: u8,
}

impl User {
    fn value(&self, c: &Cond) -> Option<f32> {
        let yes = |b: bool| Some(b as u8 as f32);
        let bed = is_bed(self.marker_id);
        match c.function {
            func::GET_SITTING => Some(if bed { 0.0 } else { self.state as f32 }),
            func::GET_SLEEPING => Some(if bed { self.state as f32 } else { 0.0 }),
            func::GET_FURNITURE_MARKER_ID => Some(self.marker_id as f32),
            // The parameter can be a form list of furniture: NVDance asks it of
            // DanceMarkerObjects (furniture.md §3).
            func::IS_CURRENT_FURNITURE_OBJ => yes(c.param1 == Some(self.furniture_base) || c.param1_list.contains(&self.furniture_base)),
            func::IS_CURRENT_FURNITURE_REF => yes(c.param1 == Some(self.furniture_ref) || c.param1_list.contains(&self.furniture_ref)),
            func::GET_IS_ID => yes(c.param1 == Some(self.actor_base)),
            func::IS_PC_1ST_PERSON => yes(self.first_person),
            // The parameter is the sex asked about: 0 male, 1 female [documented: GECK
            // wiki GetIsSex, catalogue notes].
            func::GET_IS_SEX => yes((c.param1_raw == 1) == self.female),
            func::IS_CHILD => yes(self.child),
            func::GET_IS_ALERTED => yes(self.alerted),
            func::GET_ATTACKED => yes(self.attacked),
            func::GET_RANDOM_PERCENT => Some(self.random_percent as f32),
            _ => None,
        }
    }
}

/// What the game's state answers for a condition function the user of the furniture
/// does not decide: the function's index and its first parameter as a form; `None` when
/// it is not known [observed: Doc Mitchell's own way of getting up from a chair is
/// chosen by `GetStage` of the opening quest, idles.md §6].
pub type World<'a> = &'a dyn Fn(u16, Option<FormId>) -> Option<f32>;

/// A condition with its form parameter resolved against the load order.
#[derive(Debug, Clone, PartialEq)]
struct Cond {
    function: u16,
    /// Operator code (the top three bits of the type byte).
    op: u8,
    or: bool,
    /// None: compared with a global, which is not read here.
    value: Option<f32>,
    param1_raw: u32,
    /// The first parameter as a FormID of the load order, when it maps to one.
    param1: Option<FormId>,
    /// The members of the first parameter when it is a form list (FLST).
    param1_list: Vec<FormId>,
}

impl Cond {
    /// Whether the condition holds; false when its function or value is not known here
    /// [design: an idle that cannot be judged is not chosen]. A function that is not
    /// about the user (a quest's stage) is asked of `world`.
    fn holds(&self, user: &User, world: World) -> bool {
        let (Some(a), Some(b)) = (user.value(self).or_else(|| world(self.function, self.param1)), self.value) else { return false };
        match self.op {
            0 => a == b,
            1 => a != b,
            2 => a > b,
            3 => a >= b,
            4 => a < b,
            5 => a <= b,
            _ => false,
        }
    }
}

/// All conditions hold, where a run of conditions joined by "Or" counts as one that
/// holds when any of the run does [documented: GECK wiki condition rules as summarised in
/// docs/spec-notes/quests-dialogue.md §3]. An "Or" on the last condition joins nothing.
fn all_hold(conds: &[Cond], user: &User, world: World) -> bool {
    let mut i = 0;
    while i < conds.len() {
        let mut any = false;
        loop {
            any |= conds[i].holds(user, world);
            i += 1;
            if !conds[i - 1].or || i == conds.len() {
                break;
            }
        }
        if !any {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, PartialEq)]
struct Node {
    id: FormId,
    edid: Option<String>,
    /// The `.kf` it names, as a VFS path.
    clip: Option<String>,
    conds: Vec<Cond>,
    children: Vec<usize>,
}

/// The clip an IDLE names for a user.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pick {
    pub idle: FormId,
    pub edid: Option<String>,
    /// VFS path of the `.kf`.
    pub clip: String,
}

/// The part of the IDLE tree that serves actors on furniture: the roots that ask for
/// `GetSitting > 0` or `GetSleeping > 0` and everything below them [observed: one such
/// root, `FurnitureIdles`, furniture.md §3].
#[derive(Debug, Default, Clone)]
pub struct UseIdles {
    nodes: Vec<Node>,
    roots: Vec<usize>,
}

impl UseIdles {
    pub fn build(idles: &crate::idle::Idles, lo: &LoadOrder) -> UseIdles {
        let mut out = UseIdles::default();
        for root in idles.children(None) {
            let asks = idles.idles.get(&root).is_some_and(|i| {
                i.conditions.iter().any(|c| {
                    matches!(c.function, func::GET_SITTING | func::GET_SLEEPING) && c.operator() == Some(">") && c.comparison_f32() == Some(0.0)
                })
            });
            if asks {
                let mut path = Vec::new();
                if let Some(n) = out.add(idles, lo, root, &mut path) {
                    out.roots.push(n);
                }
            }
        }
        out
    }

    fn add(&mut self, idles: &crate::idle::Idles, lo: &LoadOrder, id: FormId, path: &mut Vec<FormId>) -> Option<usize> {
        let idle = idles.idles.get(&id)?;
        if path.contains(&id) {
            return None; // a parent cycle
        }
        path.push(id);
        let conds = idle
            .conditions
            .iter()
            .map(|c| {
                let param1 = extract::global(lo, idle.plugin, c.param1_raw).ok().flatten();
                Cond {
                    function: c.function,
                    op: c.type_raw >> 5,
                    or: c.or(),
                    value: c.comparison_f32(),
                    param1_raw: c.param1_raw,
                    param1,
                    param1_list: param1.map(|l| list_members(lo, l)).unwrap_or_default(),
                }
            })
            .collect();
        let clip = (idle.model_kind() == crate::idle::ModelKind::Kf).then(|| idle.model_path()).flatten();
        let children = idles.children(Some(id)).into_iter().filter_map(|c| self.add(idles, lo, c, path)).collect();
        path.pop();
        self.nodes.push(Node { id, edid: idle.edid.clone(), clip, conds, children });
        Some(self.nodes.len() - 1)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every clip this part of the tree names, as VFS paths.
    pub fn clips(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().filter_map(|n| n.clip.as_deref())
    }

    /// The clip for a user: siblings are tried in their stored order and the first whose
    /// conditions hold is entered; inside it the same again; an idle that names a clip
    /// and has no passing child with one is the answer; a passing idle that leads to no
    /// clip is passed over for its next sibling [hypothesis: the order rule is the
    /// owner's "first eligible from the top" for stored lists; the tree puts special
    /// cases before the general one throughout, furniture.md §3].
    pub fn pick(&self, user: &User) -> Option<Pick> {
        self.pick_in(user, &|_, _| None)
    }

    /// [`UseIdles::pick`] in a game: `world` answers the conditions that are about the
    /// game's state, not the user.
    pub fn pick_in(&self, user: &User, world: World) -> Option<Pick> {
        self.roots.iter().find_map(|&r| self.walk(r, user, world))
    }

    fn walk(&self, n: usize, user: &User, world: World) -> Option<Pick> {
        let node = &self.nodes[n];
        if !all_hold(&node.conds, user, world) {
            return None;
        }
        node.children
            .iter()
            .find_map(|&c| self.walk(c, user, world))
            .or_else(|| node.clip.clone().map(|clip| Pick { idle: node.id, edid: node.edid.clone(), clip }))
    }
}

// ---- where: entry points in the world ----------------------------------------------

/// A marker of a placed piece of furniture, in world space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EntryPoint {
    /// Index into the model's markers.
    pub index: usize,
    pub marker_id: u8,
    pub pos: [f32; 3],
    /// The heading an actor has there, radians, clockwise from north (+Y) seen from
    /// above, as placed references' Z rotation is [hypothesis: the marker's angle added
    /// to the reference's heading, furniture.md §2].
    pub heading: f32,
}

/// The entry points of a reference placed at `pos` with heading `heading` (its Z
/// rotation; furniture tilted about X or Y is treated as upright) and scale `scale`.
/// A point that comes out non-finite (a NaN marker offset in a broken model, or a NaN
/// placement) is left out, so it never becomes a walk target; the others keep their
/// marker index, which the marker flags are read by.
pub fn entry_points(markers: &[Marker], pos: [f32; 3], heading: f32, scale: f32) -> Vec<EntryPoint> {
    // A heading of h turns model +Y towards +X (clockwise seen from above).
    let (s, c) = heading.sin_cos();
    markers
        .iter()
        .enumerate()
        .map(|(index, m)| {
            let [x, y, z] = m.offset.map(|v| v * scale);
            EntryPoint {
                index,
                marker_id: m.ref_1,
                pos: [pos[0] + x * c + y * s, pos[1] - x * s + y * c, pos[2] + z],
                heading: (heading + m.angle()).rem_euclid(std::f32::consts::TAU),
            }
        })
        .filter(|p| p.pos.iter().all(|v| v.is_finite()) && p.heading.is_finite())
        .collect()
}

/// The entry point an actor at `from` takes: the nearest in the plane among those
/// `usable` allows [design: how the game chooses is not known, furniture.md §5].
pub fn nearest_entry(points: &[EntryPoint], from: [f32; 3], usable: impl Fn(&EntryPoint) -> bool) -> Option<EntryPoint> {
    let d2 = |p: &EntryPoint| (p.pos[0] - from[0]).powi(2) + (p.pos[1] - from[1]).powi(2);
    points.iter().filter(|p| usable(p)).min_by(|a, b| d2(a).total_cmp(&d2(b))).copied()
}

/// Where an actor's root stands and which way it faces (heading as [`EntryPoint`]'s).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Frame {
    pub pos: [f32; 3],
    pub heading: f32,
}

impl Frame {
    /// A point of the actor's own space (X right, Y forward) in the world.
    pub fn point(&self, local: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.heading.sin_cos();
        [self.pos[0] + local[0] * c + local[1] * s, self.pos[1] - local[0] * s + local[1] * c, self.pos[2] + local[2]]
    }
}

/// Where a clip puts the body at one end: the translation of `Bip01` and the turn of
/// `Bip01 NonAccum` about Z (radians, counter-clockwise seen from above).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct BodyAt {
    pub bip01: [f32; 3],
    pub yaw: f32,
}

/// The two ends of a getting-on or getting-off clip.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct ClipEnds {
    pub start: BodyAt,
    pub stop: BodyAt,
}

impl BodyAt {
    /// The body at time `t` of a sequence; a node without a track counts as unmoved.
    pub fn of(seq: &fnv_nif::anim::Sequence, t: f32) -> BodyAt {
        let track = |node: &str| {
            seq.tracks.iter().find_map(|tr| match (&tr.node_name, &tr.data) {
                (Some(n), fnv_nif::anim::TrackData::Transform(tt)) if n == node => Some(tt),
                _ => None,
            })
        };
        let yaw = |node: &str| track(node).and_then(|tt| tt.rotation_at(t)).map_or(0.0, crate::locomotion::yaw_of);
        BodyAt {
            bip01: track("Bip01").and_then(|tt| tt.translation.sample(t)).unwrap_or([0.0; 3]),
            yaw: yaw("Bip01") + yaw("Bip01 NonAccum"),
        }
    }
}

impl ClipEnds {
    pub fn of(seq: &fnv_nif::anim::Sequence) -> ClipEnds {
        ClipEnds { start: BodyAt::of(seq, seq.start_time), stop: BodyAt::of(seq, seq.stop_time) }
    }
}

/// The frames an actor's root takes while using furniture [observed in the clips,
/// furniture.md §4: a getting-on clip carries the body from the entry point onto the
/// furniture in the `Bip01` track and turns it in `Bip01 NonAccum`; the loop played on
/// the furniture has no `Bip01` track and the standing turn in `Bip01 NonAccum`; the
/// getting-off clip starts where the getting-on clip stops. Hypothesis: the game moves
/// the actor between these frames as done here].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct UseFrames {
    /// While getting on and getting off: placed so that the getting-on clip's first
    /// `Bip01` position is the entry point, facing along the entry heading.
    pub clip: Frame,
    /// While on the furniture: where the getting-on clip leaves the body, turned so
    /// that the loop's own turn matches the clip's last one.
    pub on: Frame,
}

impl UseFrames {
    /// `loop_yaw`: the `Bip01 NonAccum` turn of the loop played on the furniture (the
    /// standing turn when there is no loop).
    pub fn new(entry: &EntryPoint, get_on: &ClipEnds, loop_yaw: f32) -> UseFrames {
        let at_entry = Frame { pos: entry.pos, heading: entry.heading };
        let s = get_on.start.bip01;
        let clip = Frame { pos: at_entry.point([-s[0], -s[1], 0.0]), heading: entry.heading };
        let e = get_on.stop.bip01;
        let on = Frame { pos: clip.point([e[0], e[1], 0.0]), heading: turned(entry.heading, get_on.stop.yaw - loop_yaw) };
        UseFrames { clip, on }
    }

    /// Where the actor stands once a getting-off clip has ended. `standing_yaw`: the
    /// `Bip01 NonAccum` turn of a standing actor (the getting-on clip's first).
    pub fn after(&self, get_off: &ClipEnds, standing_yaw: f32) -> Frame {
        let e = get_off.stop.bip01;
        Frame { pos: self.clip.point([e[0], e[1], 0.0]), heading: turned(self.clip.heading, get_off.stop.yaw - standing_yaw) }
    }
}

/// A heading (clockwise) after the body turned by `yaw` (counter-clockwise).
fn turned(heading: f32, yaw: f32) -> f32 {
    (heading - yaw).rem_euclid(std::f32::consts::TAU)
}

// ---- when: getting on, staying, getting off ----------------------------------------

/// What an actor on furniture is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Playing the clip that takes it onto the furniture.
    GettingOn,
    On,
    /// Playing the clip that takes it off.
    GettingOff,
}

/// One actor's use of one piece of furniture, from the moment it stands at the entry
/// point until it is back there. The caller plays the clips; this keeps the phase, tells
/// when each ends, and answers the script functions.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Use {
    pub furniture_ref: FormId,
    pub furniture_base: FormId,
    pub entry: EntryPoint,
    pub phase: Phase,
    /// Seconds into the current phase.
    pub t: f32,
    /// Length of the clip of the current phase (getting on or off), seconds.
    pub clip_len: f32,
    /// Asked to leave while still getting on: it leaves once on.
    pub leave_wanted: bool,
}

/// What [`Use::advance`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseEvent {
    /// The getting-on clip ended: the actor is on the furniture.
    GotOn,
    /// The getting-off clip ended: the actor is free again, at the entry point.
    GotOff,
}

impl Use {
    /// Start getting on, with the length of the clip that does it (0 when there is
    /// none: the actor is on at the next [`Use::advance`]).
    pub fn begin(furniture_ref: FormId, furniture_base: FormId, entry: EntryPoint, clip_len: f32) -> Use {
        Use { furniture_ref, furniture_base, entry, phase: Phase::GettingOn, t: 0.0, clip_len: clip_len.max(0.0), leave_wanted: false }
    }

    /// The `GetSitting` / `GetSleeping` value of the phase.
    pub fn state(&self) -> u8 {
        match self.phase {
            Phase::GettingOn => state::GETTING_ON,
            Phase::On => state::ON,
            Phase::GettingOff => state::GETTING_OFF,
        }
    }

    pub fn is_bed(&self) -> bool {
        is_bed(self.entry.marker_id)
    }

    /// Ask the actor to get off. Returns true when the getting-off clip should start
    /// now (the caller then calls [`Use::begin_leaving`] with its length); while the
    /// actor is still getting on, the wish is kept until it is on [design].
    pub fn leave(&mut self) -> bool {
        match self.phase {
            Phase::On => true,
            Phase::GettingOn => {
                self.leave_wanted = true;
                false
            }
            Phase::GettingOff => false,
        }
    }

    pub fn begin_leaving(&mut self, clip_len: f32) {
        self.phase = Phase::GettingOff;
        self.t = 0.0;
        self.clip_len = clip_len.max(0.0);
        self.leave_wanted = false;
    }

    /// Advance by `dt` seconds.
    pub fn advance(&mut self, dt: f32) -> Option<UseEvent> {
        self.t += dt.max(0.0);
        match self.phase {
            Phase::GettingOn if self.t >= self.clip_len => {
                self.phase = Phase::On;
                self.t = 0.0;
                Some(UseEvent::GotOn)
            }
            Phase::GettingOff if self.t >= self.clip_len => Some(UseEvent::GotOff),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(state: u8, marker_id: u8) -> User {
        User {
            state,
            marker_id,
            furniture_ref: FormId(0x200),
            furniture_base: FormId(0x100),
            actor_base: FormId(0x300),
            first_person: false,
            female: false,
            child: false,
            alerted: false,
            attacked: false,
            random_percent: 50,
        }
    }

    fn cond(function: u16, op: u8, value: f32) -> Cond {
        Cond { function, op, or: false, value: Some(value), param1_raw: 0, param1: None, param1_list: Vec::new() }
    }

    fn node(id: u32, clip: Option<&str>, conds: Vec<Cond>, children: Vec<usize>) -> Node {
        Node { id: FormId(id), edid: None, clip: clip.map(str::to_string), conds, children }
    }

    /// A tree shaped like the game's chair idles: fast exits before the plain ones, a
    /// group that passes but leads nowhere, and a marker test on each clip.
    fn tree() -> UseIdles {
        let nodes = vec![
            node(1, Some("left_enter"), vec![cond(func::GET_FURNITURE_MARKER_ID, 0, 11.0)], vec![]),
            node(2, Some("front_enter"), vec![cond(func::GET_FURNITURE_MARKER_ID, 0, 14.0)], vec![]),
            node(3, None, vec![cond(func::GET_SITTING, 0, 2.0)], vec![0, 1]),
            node(4, Some("front_exit_fast"), vec![cond(func::GET_FURNITURE_MARKER_ID, 0, 14.0)], vec![]),
            node(5, None, vec![Cond { or: true, ..cond(func::GET_IS_ALERTED, 0, 1.0) }, cond(func::GET_ATTACKED, 0, 1.0)], vec![3]),
            node(6, Some("front_exit"), vec![cond(func::GET_FURNITURE_MARKER_ID, 0, 14.0)], vec![]),
            node(7, Some("left_exit"), vec![cond(func::GET_FURNITURE_MARKER_ID, 0, 11.0)], vec![]),
            node(8, None, vec![cond(func::GET_SITTING, 0, 4.0)], vec![4, 5, 6]),
            node(9, Some("first_person_loop"), vec![cond(func::IS_PC_1ST_PERSON, 0, 1.0)], vec![]),
            node(10, Some("loop"), vec![], vec![]),
            node(11, None, vec![cond(func::GET_SITTING, 0, 1.0)], vec![8, 9]),
            // An idle whose function is not known here: never chosen.
            node(12, Some("unknown"), vec![cond(0x7FF, 0, 1.0)], vec![]),
            node(13, None, vec![Cond { or: true, ..cond(func::GET_SITTING, 2, 0.0) }, cond(func::GET_SLEEPING, 2, 0.0)], vec![11, 7, 2, 10]),
        ];
        UseIdles { nodes, roots: vec![12] }
    }

    fn clip(t: &UseIdles, u: &User) -> Option<String> {
        t.pick(u).map(|p| p.clip)
    }

    /// No game: nothing but the user is known.
    const NO_WORLD: World = &|_, _| None;

    #[test]
    fn a_quest_stage_chooses_a_special_way_of_getting_up() {
        // The shape of Doc Mitchell's chair exit: before the plain exit, for the stages
        // 10 to 50 of one quest.
        let quest = FormId(0x900);
        let staged = |op, v| Cond { param1: Some(quest), ..cond(func::GET_STAGE, op, v) };
        let nodes = vec![
            node(1, Some("help_up"), vec![staged(3, 10.0), staged(5, 50.0), cond(func::GET_FURNITURE_MARKER_ID, 0, 14.0)], vec![]),
            node(2, Some("front_exit"), vec![cond(func::GET_FURNITURE_MARKER_ID, 0, 14.0)], vec![]),
            node(3, None, vec![cond(func::GET_SITTING, 0, 4.0)], vec![0, 1]),
        ];
        let t = UseIdles { nodes, roots: vec![2] };
        let u = user(state::GETTING_OFF, 14);
        let at = |stage: f32| t.pick_in(&u, &|f, q| (f == func::GET_STAGE && q == Some(quest)).then_some(stage)).map(|p| p.clip);
        assert_eq!(at(45.0).as_deref(), Some("help_up"));
        assert_eq!(at(10.0).as_deref(), Some("help_up"));
        assert_eq!(at(55.0).as_deref(), Some("front_exit"));
        assert_eq!(at(0.0).as_deref(), Some("front_exit"));
        // Without a game the stage is not known: the plain exit.
        assert_eq!(clip(&t, &u).as_deref(), Some("front_exit"));
    }

    #[test]
    fn picks_by_state_and_marker() {
        let t = tree();
        assert_eq!(clip(&t, &user(state::GETTING_ON, 11)).as_deref(), Some("left_enter"));
        assert_eq!(clip(&t, &user(state::GETTING_ON, 14)).as_deref(), Some("front_enter"));
        assert_eq!(clip(&t, &user(state::GETTING_OFF, 11)).as_deref(), Some("left_exit"));
        assert_eq!(clip(&t, &user(state::LOOP, 11)).as_deref(), Some("loop"));
        assert_eq!(clip(&t, &User { first_person: true, ..user(state::LOOP, 11) }).as_deref(), Some("first_person_loop"));
        // Seated with nothing special to play, and not on furniture at all.
        assert_eq!(clip(&t, &user(state::ON, 11)), None);
        assert_eq!(clip(&t, &user(state::NONE, 11)), None);
        // No clip for a marker the tree does not know.
        assert_eq!(clip(&t, &user(state::GETTING_ON, 12)), None);
    }

    #[test]
    fn earlier_siblings_win_and_dead_ends_fall_through() {
        let t = tree();
        let alerted = User { alerted: true, ..user(state::GETTING_OFF, 14) };
        assert_eq!(clip(&t, &alerted).as_deref(), Some("front_exit_fast"));
        assert_eq!(clip(&t, &user(state::GETTING_OFF, 14)).as_deref(), Some("front_exit"));
        // The fast group passes for an attacked actor at the left marker but has no clip
        // for it: the plain exit after it is taken.
        let attacked = User { attacked: true, ..user(state::GETTING_OFF, 11) };
        assert_eq!(clip(&t, &attacked).as_deref(), Some("left_exit"));
    }

    #[test]
    fn beds_answer_get_sleeping() {
        let u = user(state::ON, 1);
        assert_eq!(u.value(&cond(func::GET_SLEEPING, 0, 0.0)), Some(3.0));
        assert_eq!(u.value(&cond(func::GET_SITTING, 0, 0.0)), Some(0.0));
        let u = user(state::ON, 11);
        assert_eq!(u.value(&cond(func::GET_SITTING, 0, 0.0)), Some(3.0));
        assert_eq!(u.value(&cond(func::GET_SLEEPING, 0, 0.0)), Some(0.0));
    }

    #[test]
    fn or_runs_group_and_a_trailing_or_joins_nothing() {
        let u = user(state::ON, 11);
        let t = cond(func::GET_SITTING, 0, 3.0);
        let f = cond(func::GET_SITTING, 0, 0.0);
        let or = |c: &Cond| Cond { or: true, ..c.clone() };
        assert!(all_hold(&[or(&f), t.clone()], &u, NO_WORLD));
        assert!(!all_hold(&[or(&f), f.clone(), t.clone()], &u, NO_WORLD));
        assert!(all_hold(&[t.clone(), or(&f), or(&f), t.clone()], &u, NO_WORLD));
        assert!(all_hold(&[or(&t)], &u, NO_WORLD));
        assert!(!all_hold(&[t.clone(), or(&f)], &u, NO_WORLD));
        assert!(all_hold(&[], &u, NO_WORLD));
    }

    #[test]
    fn form_parameters_compare_resolved_ids() {
        let u = user(state::ON, 11);
        let c = |function, id: u32| Cond { param1: Some(FormId(id)), ..cond(function, 0, 1.0) };
        assert!(c(func::IS_CURRENT_FURNITURE_OBJ, 0x100).holds(&u, NO_WORLD));
        assert!(!c(func::IS_CURRENT_FURNITURE_OBJ, 0x101).holds(&u, NO_WORLD));
        assert!(c(func::IS_CURRENT_FURNITURE_REF, 0x200).holds(&u, NO_WORLD));
        assert!(c(func::GET_IS_ID, 0x300).holds(&u, NO_WORLD));
        // A form list of furniture (NVDance's DanceMarkerObjects): any member matches.
        let list = |members: Vec<u32>| Cond { param1_list: members.into_iter().map(FormId).collect(), ..c(func::IS_CURRENT_FURNITURE_OBJ, 0x500) };
        assert!(list(vec![0x101, 0x100]).holds(&u, NO_WORLD));
        assert!(!list(vec![0x101]).holds(&u, NO_WORLD));
        // GetIsSex Female on a male actor.
        assert!(!Cond { param1_raw: 1, ..cond(func::GET_IS_SEX, 0, 1.0) }.holds(&u, NO_WORLD));
        assert!(Cond { param1_raw: 0, ..cond(func::GET_IS_SEX, 0, 1.0) }.holds(&u, NO_WORLD));
    }

    #[test]
    fn non_finite_markers_are_no_entry_points() {
        let m = [
            Marker { offset: [f32::NAN, 0.0, 0.0], orientation: 0, ref_1: 11, ref_2: 11 },
            Marker { offset: [0.0, 54.0, -26.0], orientation: 0, ref_1: 14, ref_2: 14 },
        ];
        let p = entry_points(&m, [0.0; 3], 0.0, 1.0);
        assert_eq!(p.iter().map(|p| (p.index, p.marker_id)).collect::<Vec<_>>(), vec![(1, 14)]);
        assert!(entry_points(&m, [0.0; 3], f32::NAN, 1.0).is_empty());
    }

    #[test]
    fn entry_points_turn_with_the_reference() {
        let m = [
            Marker { offset: [-50.0, 0.0, -26.0], orientation: 1570, ref_1: 11, ref_2: 11 },
            Marker { offset: [0.0, 54.0, -26.0], orientation: 3141, ref_1: 14, ref_2: 14 },
        ];
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        // Unturned: offsets as they are.
        let p = entry_points(&m, [1000.0, 2000.0, 100.0], 0.0, 1.0);
        assert!(close(p[0].pos[0], 950.0) && close(p[0].pos[1], 2000.0) && close(p[0].pos[2], 74.0));
        assert!(close(p[0].heading, 1.57));
        // Turned a quarter clockwise (north becomes east): the front marker (+Y) lies
        // to the east, the left one (−X) to the north.
        let p = entry_points(&m, [0.0, 0.0, 0.0], std::f32::consts::FRAC_PI_2, 2.0);
        assert!(close(p[1].pos[0], 108.0) && close(p[1].pos[1], 0.0));
        assert!(close(p[0].pos[0], 0.0) && close(p[0].pos[1], 100.0));
        assert!(close(p[1].heading, m[1].angle() + std::f32::consts::FRAC_PI_2));
        assert_eq!((p[0].marker_id, p[1].marker_id, p[1].index), (11, 14, 1));
        // The nearest usable point.
        let near = nearest_entry(&p, [100.0, 10.0, 0.0], |_| true).unwrap();
        assert_eq!(near.marker_id, 14);
        let near = nearest_entry(&p, [100.0, 10.0, 0.0], |e| e.marker_id != 14).unwrap();
        assert_eq!(near.marker_id, 11);
        assert!(nearest_entry(&p, [0.0; 3], |_| false).is_none());
    }

    /// The numbers of `chair01.nif` and the chair clips (furniture.md §4): from the left
    /// and from the front the actor ends on the seat facing the chair's front, and after
    /// getting off stands at the entry point with its back to the chair.
    #[test]
    fn frames_put_the_actor_on_the_seat_and_back() {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        let near = |a: f32, b: f32, tol: f32| (a - b).abs() < tol;
        let same_turn = |a: f32, b: f32| {
            let d = (a - b).rem_euclid(TAU);
            d < 0.05 || d > TAU - 0.05
        };
        let markers = [
            Marker { offset: [-51.87, -0.50, -25.98], orientation: 1570, ref_1: 11, ref_2: 11 },
            Marker { offset: [-0.29, 54.56, -25.98], orientation: 3141, ref_1: 14, ref_2: 14 },
        ];
        let (loop_yaw, standing_yaw) = (1.59, 1.44);
        let at = |x: f32, y: f32, yaw: f32| BodyAt { bip01: [x, y, 0.0], yaw };
        let left_on = ClipEnds { start: at(0.0, 0.0, 1.44), stop: at(0.1, 50.7, -3.13) };
        let left_off = ClipEnds { start: at(0.1, 50.9, -3.12), stop: at(-0.5, -0.8, -1.70) };
        let front_on = ClipEnds { start: at(0.0, -1.1, 1.44), stop: at(0.0, 54.3, -1.55) };
        // The chair unturned at the origin, then turned a quarter and moved.
        for (pos, heading) in [([0.0; 3], 0.0), ([500.0, -200.0, 30.0], FRAC_PI_2)] {
            let chair = Frame { pos, heading };
            let points = entry_points(&markers, pos, heading, 1.0);
            let seat = chair.point([0.0, 0.0, -25.98]);
            for (p, on) in [(&points[0], &left_on), (&points[1], &front_on)] {
                let f = UseFrames::new(p, on, loop_yaw);
                assert!(near(f.on.pos[0], seat[0], 2.5) && near(f.on.pos[1], seat[1], 2.5) && near(f.on.pos[2], seat[2], 0.01), "{f:?}");
                assert!(same_turn(f.on.heading, heading), "{f:?}");
            }
            let f = UseFrames::new(&points[0], &left_on, loop_yaw);
            assert_eq!(f.clip.pos, points[0].pos);
            let out = f.after(&left_off, standing_yaw);
            assert!(near(out.pos[0], points[0].pos[0], 1.5) && near(out.pos[1], points[0].pos[1], 1.5));
            assert!(same_turn(out.heading, points[0].heading + PI), "{out:?}");
        }
        // A clip that starts away from its own origin (the stool's): the clip frame is
        // set back so that the body starts on the entry point.
        let stool = [Marker { offset: [20.5, -41.0, -24.1], orientation: 0, ref_1: 15, ref_2: 15 }];
        let p = entry_points(&stool, [0.0; 3], 0.0, 1.0);
        let on = ClipEnds { start: at(20.5, -41.0, 1.43), stop: at(1.5, 6.2, 1.58) };
        let f = UseFrames::new(&p[0], &on, 1.58);
        assert!(near(f.clip.pos[0], 0.0, 0.01) && near(f.clip.pos[1], 0.0, 0.01));
        assert!(near(f.on.pos[0], 1.5, 0.01) && near(f.on.pos[1], 6.2, 0.01) && same_turn(f.on.heading, 0.0));
    }

    #[test]
    fn marker_flags_enable_markers_and_tell_seats_from_beds() {
        let f = |flags| Furniture { id: FormId(1), edid: None, name: None, model: None, marker_flags: flags, has_script: false };
        let chair_r = f(Some(0x4000_0002));
        assert!(!chair_r.marker_enabled(0) && chair_r.marker_enabled(1) && !chair_r.marker_enabled(2));
        assert!(chair_r.sit() && !chair_r.sleep());
        let bed = f(Some(0x8000_0003));
        assert!(bed.marker_enabled(0) && bed.marker_enabled(1) && bed.sleep() && !bed.sit());
        // The type bits are not markers.
        assert!(!bed.marker_enabled(31) && !chair_r.marker_enabled(30));
        assert!(f(None).marker_enabled(5) && !f(None).sit());
    }

    #[test]
    fn a_use_runs_on_stay_off() {
        let entry = EntryPoint { index: 0, marker_id: 11, pos: [0.0; 3], heading: 0.0 };
        let mut u = Use::begin(FormId(0x200), FormId(0x100), entry, 1.0);
        assert_eq!((u.phase, u.state()), (Phase::GettingOn, state::GETTING_ON));
        assert_eq!(u.advance(0.6), None);
        // Asked to leave too early: remembered, not started.
        assert!(!u.leave());
        assert_eq!(u.advance(0.6), Some(UseEvent::GotOn));
        assert_eq!((u.phase, u.state(), u.leave_wanted), (Phase::On, state::ON, true));
        assert_eq!(u.advance(100.0), None);
        assert!(u.leave());
        u.begin_leaving(1.5);
        assert_eq!((u.phase, u.state(), u.leave_wanted), (Phase::GettingOff, state::GETTING_OFF, false));
        assert!(!u.leave());
        assert_eq!(u.advance(1.0), None);
        assert_eq!(u.advance(0.6), Some(UseEvent::GotOff));
        // No clip: on at once.
        let mut u = Use::begin(FormId(0x200), FormId(0x100), entry, 0.0);
        assert_eq!(u.advance(0.0), Some(UseEvent::GotOn));
    }
}
