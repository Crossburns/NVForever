//! The particle systems of a NIF as data (docs/spec-notes/particles.md): what each
//! `NiParticleSystem` emits and how its particles change, read from its modifier and
//! controller blocks [documented: nif.xml block and field names]. What the fields do is
//! mostly a reading of their names [hypothesis], set out in the notes; the simulation
//! is the caller's (`nvforever::particles`).

use crate::model::ModelCtx;
use crate::nif_material::NifMaterial;
use crate::nif_spawn::{shape_material, shape_props};
use bevy::asset::RenderAssetUsages;
use bevy::math::Affine3A;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use crate::anim_play::OwnCycle;
use fnv_nif::anim::{Channel, Keys, TrackData, TransformTrack, ValueTrack};
use fnv_nif::scene::{mesh_data, scene_objects, Decoded, SceneObject, Transform as NifTransform};
use fnv_nif::value::Struct;
use fnv_nif::NifFile;
use std::collections::HashMap;
use std::sync::Arc;

/// A float a controller keys over time (`NiPSysEmitterSpeedCtlr`,
/// `NiPSysGravityStrengthCtlr`, a keyed birth rate…): the controller's own keys on its own
/// cycle, or a sequence's keys on the sequence's time and cycle (particles.md §6.6).
/// The most particles one system keeps alive, whatever its file asks for: a broken
/// file's huge `bs_max_vertices` must not fill memory [hypothesis: vanilla systems ask
/// for far fewer; particles.md quotes 12 and 14].
pub const MAX_PARTICLES: usize = 8192;

/// The most points one particle's strip keeps, whatever its file asks for: a broken
/// file's `max_point_count` (up to 65,535) times [`MAX_PARTICLES`] long-lived particles
/// would fill gigabytes [design; vanilla strips ask for 4 to 33, observed:
/// docs/spec-notes/nif-engine-gap-audit.md].
pub const MAX_STRIP_POINTS: usize = 256;

#[derive(Debug, Clone)]
pub struct FloatKeys {
    pub channel: Channel<1>,
    pub cycle: OwnCycle,
}

impl FloatKeys {
    /// The value `t` seconds after the emitter appeared.
    pub fn at(&self, t: f32) -> Option<f32> {
        self.channel.sample(self.cycle.time(t)).map(|[v]| v).filter(|v| v.is_finite() && *v > -1e30)
    }
}

/// The emitter fields a float controller can drive [documented: nif.xml
/// `NiPSysEmitter…Ctlr` names].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitterParam {
    Speed,
    Radius,
    Declination,
    DeclinationVariation,
    PlanarAngle,
    PlanarAngleVariation,
    LifeSpan,
}

impl EmitterParam {
    fn of_controller(ty: &str) -> Option<EmitterParam> {
        Some(match ty {
            "NiPSysEmitterSpeedCtlr" => EmitterParam::Speed,
            "NiPSysEmitterInitialRadiusCtlr" => EmitterParam::Radius,
            "NiPSysEmitterDeclinationCtlr" => EmitterParam::Declination,
            "NiPSysEmitterDeclinationVarCtlr" => EmitterParam::DeclinationVariation,
            "NiPSysEmitterPlanarAngleCtlr" => EmitterParam::PlanarAngle,
            "NiPSysEmitterPlanarAngleVarCtlr" => EmitterParam::PlanarAngleVariation,
            "NiPSysEmitterLifeSpanCtlr" => EmitterParam::LifeSpan,
            _ => return None,
        })
    }
}

/// Where on a mesh a mesh emitter's particles start [documented: nif.xml EmitFrom].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitFrom {
    Vertices,
    FaceCenter,
    EdgeCenter,
    FaceSurface,
    EdgeSurface,
}

/// Which way a mesh emitter throws its particles [documented: nif.xml VelocityType].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InitialVelocity {
    /// Along the mesh's normal where the particle starts.
    Normals,
    /// Any way.
    Random,
    /// Along this axis, in the mesh's frame.
    Direction(Vec3),
}

/// One mesh of a `NiPSysMeshEmitter`: its place in the file and its geometry, with the
/// running totals of its faces' areas and edges' lengths to pick by (particles.md §6.1).
#[derive(Debug, Clone, Default)]
pub struct EmitterMesh {
    pub frame: Affine3A,
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    pub edges: Vec<[u32; 2]>,
    area_total: Vec<f32>,
    length_total: Vec<f32>,
}

impl EmitterMesh {
    pub fn new(frame: Affine3A, positions: Vec<Vec3>, normals: Vec<Vec3>, triangles: Vec<[u32; 3]>) -> EmitterMesh {
        let n = positions.len() as u32;
        let triangles: Vec<[u32; 3]> = triangles.into_iter().filter(|t| t.iter().all(|&i| i < n)).collect();
        let mut edges: Vec<[u32; 2]> = triangles.iter().flat_map(|&[a, b, c]| [[a.min(b), a.max(b)], [b.min(c), b.max(c)], [a.min(c), a.max(c)]]).collect();
        edges.sort_unstable();
        edges.dedup();
        let p = |i: u32| positions[i as usize];
        let mut sum = 0.0;
        let area_total = triangles
            .iter()
            .map(|&[a, b, c]| {
                sum += (p(b) - p(a)).cross(p(c) - p(a)).length() * 0.5;
                sum
            })
            .collect();
        let mut sum = 0.0;
        let length_total = edges
            .iter()
            .map(|&[a, b]| {
                sum += p(a).distance(p(b));
                sum
            })
            .collect();
        EmitterMesh { frame, positions, normals, triangles, edges, area_total, length_total }
    }

    fn normal(&self, i: u32) -> Option<Vec3> {
        self.normals.get(i as usize).copied().filter(|n| n.length_squared() > 1e-12)
    }

    /// The index of the entry a share `r` (0..1) of the running total falls in.
    fn by_weight(totals: &[f32], r: f32) -> usize {
        let total = totals.last().copied().unwrap_or(0.0);
        if total <= 0.0 {
            return ((r * totals.len() as f32) as usize).min(totals.len().saturating_sub(1));
        }
        totals.partition_point(|&t| t < r * total).min(totals.len() - 1)
    }

    /// A start point and the normal there, in the mesh's frame, picked as `from` says
    /// (particles.md §6.1); `rand` gives numbers in 0..1. On a face the vertex normals are
    /// blended at the point, else the face's own normal is taken.
    pub fn pick(&self, from: EmitFrom, rand: &mut dyn FnMut() -> f32) -> Option<(Vec3, Vec3)> {
        let p = |i: u32| self.positions[i as usize];
        let face_normal = |[a, b, c]: [u32; 3]| (p(b) - p(a)).cross(p(c) - p(a)).normalize_or(Vec3::Z);
        let blend = |ids: &[u32], w: &[f32], fallback: Vec3| {
            let n: Option<Vec3> = ids.iter().zip(w).map(|(&i, &k)| self.normal(i).map(|n| n * k)).sum();
            n.map_or(fallback, |n| n.normalize_or(fallback))
        };
        match from {
            EmitFrom::Vertices => {
                if self.positions.is_empty() {
                    return None;
                }
                let i = ((rand() * self.positions.len() as f32) as usize).min(self.positions.len() - 1) as u32;
                Some((p(i), self.normal(i).map_or(Vec3::Z, Vec3::normalize)))
            }
            EmitFrom::FaceCenter | EmitFrom::FaceSurface => {
                if self.triangles.is_empty() {
                    return None;
                }
                let t = if from == EmitFrom::FaceSurface {
                    self.triangles[Self::by_weight(&self.area_total, rand())]
                } else {
                    self.triangles[((rand() * self.triangles.len() as f32) as usize).min(self.triangles.len() - 1)]
                };
                // Uniform over the triangle: the square-root rule for barycentric weights.
                let (u, v) = if from == EmitFrom::FaceSurface { (rand().sqrt(), rand()) } else { (1.0, 0.5) };
                let w = if from == EmitFrom::FaceSurface { [1.0 - u, u * (1.0 - v), u * v] } else { [1.0 / 3.0; 3] };
                let at = p(t[0]) * w[0] + p(t[1]) * w[1] + p(t[2]) * w[2];
                Some((at, blend(&t, &w, face_normal(t))))
            }
            EmitFrom::EdgeCenter | EmitFrom::EdgeSurface => {
                if self.edges.is_empty() {
                    return None;
                }
                let [a, b] = if from == EmitFrom::EdgeSurface {
                    self.edges[Self::by_weight(&self.length_total, rand())]
                } else {
                    self.edges[((rand() * self.edges.len() as f32) as usize).min(self.edges.len() - 1)]
                };
                let k = if from == EmitFrom::EdgeSurface { rand() } else { 0.5 };
                let along = (p(b) - p(a)).normalize_or(Vec3::X);
                Some((p(a).lerp(p(b), k), blend(&[a, b], &[1.0 - k, k], along.any_orthonormal_vector())))
            }
        }
    }
}

/// A `NiPSysMeshEmitter`'s meshes and how it uses them.
#[derive(Debug, Clone)]
pub struct MeshEmission {
    pub meshes: Vec<EmitterMesh>,
    pub from: EmitFrom,
    pub velocity: InitialVelocity,
}

/// The emitter's values at a time, its float controllers applied.
#[derive(Debug, Clone, Copy)]
pub struct EmitterValues {
    pub speed: f32,
    pub speed_variation: f32,
    pub declination: f32,
    pub declination_variation: f32,
    pub planar_angle: f32,
    pub planar_angle_variation: f32,
    pub radius: f32,
    pub radius_variation: f32,
    pub life: f32,
    pub life_variation: f32,
}

/// A `NiPSys…Emitter`: where particles are born and what they start with.
#[derive(Debug, Clone)]
pub struct Emitter {
    /// The emitter object's place in the file (the root's transform left out).
    pub frame: Affine3A,
    /// The volume particles start in, about the emitter object: a box's width, height
    /// and depth (x, y, z); a sphere's or cylinder's radius in x and y and a cylinder's
    /// height in z.
    pub size: Vec3,
    pub round: bool,
    pub speed: f32,
    pub speed_variation: f32,
    /// Radians from the emitter object's +z, and about it.
    pub declination: f32,
    pub declination_variation: f32,
    pub planar_angle: f32,
    pub planar_angle_variation: f32,
    pub color: [f32; 4],
    pub radius: f32,
    pub radius_variation: f32,
    pub life: f32,
    pub life_variation: f32,
    /// A mesh emitter's meshes (`frame` is then unused unless it has none).
    pub mesh: Option<MeshEmission>,
    /// Fields driven by float controllers (particles.md §6.6).
    pub keyed: Vec<(EmitterParam, FloatKeys)>,
    /// The name of the node `frame` is the place of (the emitter object; a mesh emitter's
    /// system's parent): a drawn model's node of that name, which its sequence may move,
    /// is where it emits from (particles.md §6.7).
    pub object: Option<String>,
}

impl Emitter {
    fn blank() -> Emitter {
        Emitter {
            frame: Affine3A::IDENTITY,
            size: Vec3::ZERO,
            round: false,
            speed: 0.0,
            speed_variation: 0.0,
            declination: 0.0,
            declination_variation: 0.0,
            planar_angle: 0.0,
            planar_angle_variation: 0.0,
            color: [1.0; 4],
            radius: 1.0,
            radius_variation: 0.0,
            life: 1.0,
            life_variation: 0.0,
            mesh: None,
            keyed: Vec::new(),
            object: None,
        }
    }

    /// Its values `t` seconds after it appeared: each keyed field's controller value in
    /// place of the field's own.
    pub fn values(&self, t: f32) -> EmitterValues {
        self.values_keyed(t, &self.keyed)
    }

    /// Its values at time `t` of a sequence that drives it: the fields that sequence keys
    /// take its keys, the others their own value (particles.md §6.7).
    pub fn values_in(&self, t: f32, s: &SequenceEmission) -> EmitterValues {
        self.values_keyed(t, &s.keyed)
    }

    fn values_keyed(&self, t: f32, keyed: &[(EmitterParam, FloatKeys)]) -> EmitterValues {
        let mut v = EmitterValues {
            speed: self.speed,
            speed_variation: self.speed_variation,
            declination: self.declination,
            declination_variation: self.declination_variation,
            planar_angle: self.planar_angle,
            planar_angle_variation: self.planar_angle_variation,
            radius: self.radius,
            radius_variation: self.radius_variation,
            life: self.life,
            life_variation: self.life_variation,
        };
        for (param, keys) in keyed {
            let Some(x) = keys.at(t) else { continue };
            *match param {
                EmitterParam::Speed => &mut v.speed,
                EmitterParam::Radius => &mut v.radius,
                EmitterParam::Declination => &mut v.declination,
                EmitterParam::DeclinationVariation => &mut v.declination_variation,
                EmitterParam::PlanarAngle => &mut v.planar_angle,
                EmitterParam::PlanarAngleVariation => &mut v.planar_angle_variation,
                EmitterParam::LifeSpan => &mut v.life,
            } = x;
        }
        v
    }
}

/// `NiPSysBombModifier`: a push away from the bomb object (particles.md §6.3).
#[derive(Debug, Clone)]
pub struct Bomb {
    /// The bomb object's place in the file and its axis there (unit).
    pub origin: Vec3,
    pub axis: Vec3,
    pub delta_v: f32,
    pub decay: f32,
    /// 0 none, 1 linear, 2 exponential [documented: nif.xml DecayType].
    pub decay_type: i64,
    /// 0 spherical, 1 cylindrical, 2 planar [documented: nif.xml SymmetryType].
    pub symmetry: i64,
    pub active: ModifierActive,
}

impl Bomb {
    /// The acceleration (units/s²) at `pos`, the bomb placed by `frame` (the file's place in
    /// the world) [hypothesis: `delta_v` per second away from the object, the line along
    /// its axis or the plane across it, scaled down with the distance by the decay type].
    pub fn push(&self, frame: &Affine3A, pos: Vec3) -> Vec3 {
        let axis = frame.transform_vector3(self.axis).normalize_or(Vec3::Z);
        let off = pos - frame.transform_point3(self.origin);
        let away = match self.symmetry {
            1 => off - axis * off.dot(axis),
            2 => axis * off.dot(axis),
            _ => off,
        };
        let d = away.length();
        let k = match self.decay_type {
            1 if self.decay > 0.0 => (1.0 - d / self.decay).max(0.0),
            2 if self.decay > 0.0 => (-d / self.decay).exp(),
            _ => 1.0,
        };
        away.normalize_or_zero() * self.delta_v * k
    }
}

/// `NiPSysPlanarCollider` or `NiPSysSphericalCollider` under a `NiPSysColliderManager`
/// (particles.md §6.4, §7.3), in the file's frame.
#[derive(Debug, Clone)]
pub struct Collider {
    pub origin: Vec3,
    /// The plane's axes (unit) and the rectangle's half width and half height along them.
    pub x_axis: Vec3,
    pub y_axis: Vec3,
    pub half_width: f32,
    pub half_height: f32,
    /// A spherical collider's radius in the file's frame (the plane fields unused).
    pub sphere: Option<f32>,
    pub bounce: f32,
    pub die: bool,
}

impl Collider {
    /// What becomes of a particle that moved from `from` to `to` with velocity `vel`, the
    /// collider placed by `frame` (the file's place in the world): `None` when it did not
    /// cross the rectangle, else `Some(None)` when it dies and `Some(Some((place,
    /// velocity)))` when it bounces.
    #[allow(clippy::type_complexity)]
    pub fn hit(&self, frame: &Affine3A, from: Vec3, to: Vec3, vel: Vec3) -> Option<Option<(Vec3, Vec3)>> {
        if let Some(radius) = self.sphere {
            return self.hit_sphere(frame, radius, from, to, vel);
        }
        let (x, y) = (frame.transform_vector3(self.x_axis), frame.transform_vector3(self.y_axis));
        let (hw, hh) = (self.half_width * x.length(), self.half_height * y.length());
        let (x, y) = (x.normalize_or_zero(), y.normalize_or_zero());
        let origin = frame.transform_point3(self.origin);
        let n = x.cross(y).normalize_or_zero();
        let (a, b) = ((from - origin).dot(n), (to - origin).dot(n));
        if n == Vec3::ZERO || (a > 0.0) == (b > 0.0) || a == b {
            return None;
        }
        let at = from + (to - from) * (a / (a - b));
        let off = at - origin;
        if off.dot(x).abs() > hw || off.dot(y).abs() > hh {
            return None;
        }
        if self.die {
            return Some(None);
        }
        let across = n * vel.dot(n);
        Some(Some((at, vel - across - across * self.bounce)))
    }

    /// A sphere about the collider object: a particle crossing its surface, either way
    /// (a snow globe's flakes stay inside), dies or is put back on the surface with the
    /// part of its velocity along the surface normal turned round and scaled by `bounce`
    /// [hypothesis, particles.md §7.3].
    #[allow(clippy::type_complexity)]
    fn hit_sphere(&self, frame: &Affine3A, radius: f32, from: Vec3, to: Vec3, vel: Vec3) -> Option<Option<(Vec3, Vec3)>> {
        let centre = frame.transform_point3(self.origin);
        let r = radius * frame.transform_vector3(Vec3::X).length();
        let (a, b) = ((from - centre).length() - r, (to - centre).length() - r);
        if r <= 0.0 || (a > 0.0) == (b > 0.0) || a == b {
            return None;
        }
        if self.die {
            return Some(None);
        }
        let n = (from + (to - from) * (a / (a - b)) - centre).normalize_or(Vec3::Z);
        // Back on the surface, a hair to the side it came from.
        let at = centre + n * (r + a.signum() * 1e-3);
        let across = n * vel.dot(n);
        Some(Some((at, vel - across - across * self.bounce)))
    }
}

/// `BSStripParticleSystem`: each particle trails a strip of at most `max_points` points,
/// one recorded every `step` seconds (particles.md §6.2).
#[derive(Debug, Clone, Copy)]
pub struct Strip {
    pub max_points: usize,
    pub step: f32,
}

/// `BSPSysSimpleColorModifier`: three colours over a particle's life, and its fades.
#[derive(Debug, Clone)]
pub struct SimpleColor {
    pub fade_in: f32,
    pub fade_out: f32,
    pub color1_end: f32,
    pub color2_start: f32,
    pub color2_end: f32,
    pub color3_start: f32,
    pub colors: [[f32; 4]; 3],
}

impl SimpleColor {
    /// The colour at `t`, the share of its life a particle has lived [hypothesis: the
    /// first colour until `color1_end`, a blend to the second by `color2_start`, the
    /// second until `color2_end`, a blend to the third by `color3_start`; alpha rises
    /// from nothing over `fade_in` and falls to nothing from `fade_out` to the end].
    /// With no room for the last blend (`color3_start` not after `color2_end`) the
    /// second colour stays [hypothesis: `mpsdirtdust01.nif` has all four percentages 0
    /// and only its second colour is not blank, so that is the colour meant].
    pub fn at(&self, t: f32) -> [f32; 4] {
        let mix = |a: [f32; 4], b: [f32; 4], k: f32| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k.clamp(0.0, 1.0));
        let share = |from: f32, to: f32| (t - from) / (to - from);
        let [c1, c2, c3] = self.colors;
        let mut c: [f32; 4] = if t < self.color2_start && self.color2_start > self.color1_end {
            if t <= self.color1_end { c1 } else { mix(c1, c2, share(self.color1_end, self.color2_start)) }
        } else if t <= self.color2_end || self.color3_start <= self.color2_end {
            c2
        } else {
            mix(c2, c3, share(self.color2_end, self.color3_start))
        };
        if self.fade_in > 0.0 && t < self.fade_in {
            c[3] *= t / self.fade_in;
        }
        if self.fade_out < 1.0 && t > self.fade_out {
            c[3] *= ((1.0 - t) / (1.0 - self.fade_out)).clamp(0.0, 1.0);
        }
        c
    }
}

/// `NiPSysRotationModifier`: radians and radians per second.
#[derive(Debug, Clone, Default)]
pub struct Rotation {
    pub speed: f32,
    pub speed_variation: f32,
    pub angle: f32,
    pub angle_variation: f32,
    pub random_sign: bool,
}

/// `NiPSysGravityModifier`: a pull along an axis (force type 0) or towards a point.
#[derive(Debug, Clone)]
pub struct Gravity {
    /// The gravity object's axis in the file's frame (unit), and its place.
    pub direction: Vec3,
    pub origin: Vec3,
    pub strength: f32,
    /// The strength keyed over the emitter's time (`NiPSysGravityStrengthCtlr`).
    pub strength_keys: Option<FloatKeys>,
    pub spherical: bool,
    /// When it acts ([`ModifierActive`]).
    pub active: ModifierActive,
}

impl Gravity {
    /// Its strength at the emitter's time `t`.
    pub fn strength_at(&self, t: f32) -> f32 {
        self.strength_keys.as_ref().and_then(|k| k.at(t)).unwrap_or(self.strength)
    }
}

/// `NiPSysDragModifier`: the share of the velocity along `axis` lost per step.
#[derive(Debug, Clone)]
pub struct Drag {
    pub axis: Vec3,
    pub percentage: f32,
    /// When it acts ([`ModifierActive`]).
    pub active: ModifierActive,
}

/// A modifier switched on and off over time by a `NiPSysModifierActiveCtlr` (its own
/// bool keys, or those of the model's sequence when the controller carries a blend
/// interpolator) [documented: nif.xml block names; observed: `explosiongrenadefrag.nif`'s
/// smoke drags are on from 0 s and off at 0.33 s of its `SpecialIdle_AreaEffect`].
/// Reading [hypothesis]: the keys are on the emitter's time, as the emitter's own on/off
/// keys are, and a particle feels the modifier while the emitter it came from is at a
/// time the keys say on; empty keys: always on.
#[derive(Debug, Clone, Default)]
pub struct ModifierActive {
    pub keys: Vec<(f32, bool)>,
    /// The keys each sequence of the model gives it, by the sequence's index among the
    /// file's sequences ([`SequenceEmission::index`]), for particles born while a drawn
    /// model plays that sequence (particles.md §6.7).
    pub by_sequence: Vec<(usize, Vec<(f32, bool)>)>,
}

impl ModifierActive {
    /// Whether the modifier acts at the emitter's time `t` (`period` > 0 with `loops`:
    /// the keys repeat).
    pub fn on(&self, t: f32, period: f32, loops: bool) -> bool {
        keys_on(&self.keys, t, period, loops)
    }

    /// Whether it acts at time `t` of sequence `sequence` (its keys there, else its own).
    pub fn on_in(&self, sequence: Option<usize>, t: f32, period: f32, loops: bool) -> bool {
        match sequence.and_then(|i| self.by_sequence.iter().find(|(s, _)| *s == i)) {
            Some((_, keys)) => keys_on(keys, t, period, loops),
            None => self.on(t, period, loops),
        }
    }
}

/// The state of on/off `keys` at `t` (the first key's before it; none: on).
fn keys_on(keys: &[(f32, bool)], t: f32, period: f32, loops: bool) -> bool {
    let t = if loops && period > 0.0 { t.rem_euclid(period) } else { t };
    keys.iter().rev().find(|(k, _)| *k <= t + 1e-4).or(keys.first()).is_none_or(|(_, on)| *on)
}

/// One `NiParticleSystem`.
#[derive(Debug, Clone)]
pub struct ParticleSystemDef {
    pub name: String,
    /// The most particles alive at once (the data block's vertex count).
    pub max: usize,
    pub material: Handle<NifMaterial>,
    pub emitter: Emitter,
    /// Particles born per second while the emitter is on (the emitter controller's
    /// float interpolator).
    pub rate: f32,
    /// The rate keyed over the emitter's time, in place of `rate` (particles.md §6.6).
    pub rate_keys: Option<FloatKeys>,
    /// The emitter controller's on/off keys (time, on) and its stop time, after which
    /// they repeat when the controller cycles.
    pub active: Vec<(f32, bool)>,
    pub period: f32,
    pub loops: bool,
    /// How each sequence of the model that drives its emitter does (particles.md §6.7):
    /// a drawn model playing one of them emits by it, and not at all while it plays
    /// none of them. Empty: no sequence drives it, or the model is not drawn with its
    /// own nodes (an explosion), and `rate`/`active` are the whole story.
    pub sequences: Vec<SequenceEmission>,
    /// The most emitters served at once (`BSPSysMultiTargetEmitterCtlr`); 0: no limit.
    pub max_emitters: usize,
    pub grow_time: f32,
    pub fade_time: f32,
    pub base_scale: f32,
    pub color: Option<SimpleColor>,
    /// `NiPSysColorModifier`: colour keys over the share of its life a particle has lived.
    pub color_keys: Option<Keys<4>>,
    pub rotation: Option<Rotation>,
    pub gravity: Vec<Gravity>,
    pub drag: Vec<Drag>,
    pub bombs: Vec<Bomb>,
    pub colliders: Vec<Collider>,
    /// A strip system's strips.
    pub strip: Option<Strip>,
    /// Parts of the texture a particle can show: (u, width, v, height).
    pub subtextures: Vec<[f32; 4]>,
    /// A node above the emitter that a sequence of the file moves (a dust devil's mover
    /// on its path): the emitter, its meshes and the file's bombs move with it.
    pub mover: Option<Mover>,
    /// The system's material alpha keyed over the emitter's time (`NiAlphaController`):
    /// each particle's alpha is scaled by it at the emitter time it is drawn at; the
    /// material is then built with alpha 1.
    pub alpha_keys: Option<FloatKeys>,
    /// Units over which its particles fade out towards the surface behind them
    /// ([`particle_soft_distance`]); 0: not faded (an opaque or alpha-tested system).
    pub soft: f32,
}

/// A node above a system's emitter that a sequence of the file moves, played on the
/// emitter's time by the sequence's cycle [hypothesis: the model's sequence runs from
/// the emitter's appearance, as the emitter controllers' keys do (§6.6);
/// docs/spec-notes/particles.md §6.8]. `fxdustwhirlwind01.nif`'s
/// `FXDustWhirlWindMover` travels its path in 12 s, again and again.
#[derive(Debug, Clone)]
pub struct Mover {
    /// The node's name, for the log.
    pub node: String,
    pub track: TransformTrack,
    pub cycle: OwnCycle,
    /// The place of the node's parent in the file.
    pub parent: Affine3A,
    /// The node's own transform where the track leaves a part unkeyed.
    pub rest: (Vec3, Quat, f32),
    /// From the file's frame to the node's, at rest.
    pub to_node: Affine3A,
}

impl Mover {
    /// How the file's places move at the emitter's time `clock`: the node's place at that
    /// time over its place at rest.
    pub fn moved(&self, clock: f32) -> Affine3A {
        let t = self.cycle.time(clock);
        let (rt, rr, rs) = self.rest;
        let translation = self.track.translation.sample(t).map(Vec3::from_array).filter(|v| v.is_finite()).unwrap_or(rt);
        let rotation = self.track.rotation_at(t).map(|q| Quat::from_xyzw(q[1], q[2], q[3], q[0])).filter(|q| q.is_finite()).unwrap_or(rr);
        let scale = self.track.scale.sample(t).map(|[v]| v).filter(|v| v.is_finite() && *v > -1e30).unwrap_or(rs);
        self.parent * Affine3A::from_scale_rotation_translation(Vec3::splat(scale), rotation, translation) * self.to_node
    }
}

/// The nearest node at or above block `anchor` that a sequence of the file moves (the
/// first sequence with a keyed transform track for it).
fn mover_of(file: &NifFile, dec: &Decoded, by_block: &HashMap<usize, &SceneObject>, anchor: i32, place: &dyn Fn(i32) -> Affine3A) -> Option<Mover> {
    let mut seqs = fnv_nif::anim::sequences(file, dec);
    seqs.sort_by_key(|q| (sequence_rank(&q.name.as_deref().unwrap_or_default().to_ascii_lowercase()), q.block));
    let mut at = usize::try_from(anchor).ok().and_then(|b| by_block.get(&b)).copied();
    let mut depth = 0;
    while let Some(o) = at.filter(|o| o.parent.is_some()) {
        if let Some(name) = o.name.as_deref() {
            for seq in &seqs {
                let track = seq.tracks.iter().find_map(|tr| match &tr.data {
                    TrackData::Transform(t) if tr.node_name.as_deref() == Some(name) && (t.translation.is_animated() || t.rotation.is_animated() || t.euler.is_some() || t.scale.is_animated()) => Some(t),
                    _ => None,
                });
                if let Some(track) = track {
                    let rest = affine(&o.transform);
                    let (rs, rr, rt) = rest.to_scale_rotation_translation();
                    let parent = place(o.parent.map_or(-1, |p| p as i32));
                    return Some(Mover {
                        node: name.to_string(),
                        track: track.clone(),
                        cycle: OwnCycle { start: 0.0, stop: seq.stop_time - seq.start_time, frequency: 1.0, phase: 0.0, cycle_type: seq.cycle_type },
                        parent,
                        rest: (rt, rr, rs.x),
                        to_node: (parent * rest).inverse(),
                    });
                }
            }
        }
        at = o.parent.and_then(|p| by_block.get(&p)).copied();
        depth += 1;
        if depth > 256 {
            break;
        }
    }
    None
}

impl ParticleSystemDef {
    /// How the file's places have moved at the emitter's time `t` (identity without a
    /// [`Mover`]).
    pub fn moved(&self, t: f32) -> Affine3A {
        self.mover.as_ref().map_or(Affine3A::IDENTITY, |m| m.moved(t))
    }

    /// Its birth rate `t` seconds after the emitter appeared.
    pub fn rate_at(&self, t: f32) -> f32 {
        let r = self.rate_keys.as_ref().and_then(|k| k.at(t)).unwrap_or(self.rate);
        // A broken file's infinite or NaN rate births nothing.
        if r.is_finite() { r.max(0.0) } else { 0.0 }
    }

    /// A particle's colour (before the emitter's) at `t`, the share of its life lived.
    pub fn tint(&self, t: f32) -> [f32; 4] {
        match (&self.color, &self.color_keys) {
            (Some(c), _) => c.at(t),
            (None, Some(k)) => k.sample(t).unwrap_or([1.0; 4]),
            (None, None) => [1.0; 4],
        }
    }

    /// What the log says of it: rate, size and what it has beyond the common modifiers.
    pub fn describe(&self) -> String {
        let rate = if self.rate_keys.is_some() { "keyed rate".to_string() } else { format!("{}/s", self.rate) };
        let mut s = format!("{} {rate}, at most {}", self.name, self.max);
        if let Some(m) = &self.emitter.mesh {
            s += &format!(", mesh emitter ({} mesh(es), {:?}, {:?})", m.meshes.len(), m.from, m.velocity);
        }
        if let Some(st) = &self.strip {
            s += &format!(", strips of {} points every {:.3} s", st.max_points, st.step);
        }
        if !self.bombs.is_empty() {
            s += &format!(", {} bomb(s)", self.bombs.len());
        }
        if !self.colliders.is_empty() {
            s += &format!(", {} collider(s)", self.colliders.len());
        }
        if self.color_keys.is_some() {
            s += ", colour keys";
        }
        if self.alpha_keys.is_some() {
            s += ", keyed alpha";
        }
        if self.soft > 0.0 {
            s += &format!(", soft over {:.1}", self.soft);
        }
        if let Some(m) = &self.mover {
            s += &format!(", moved by {} ({:.1} s)", m.node, m.cycle.stop);
        }
        let keyed: Vec<String> = self.emitter.keyed.iter().map(|(p, _)| format!("{p:?}")).chain(self.gravity.iter().filter(|g| g.strength_keys.is_some()).map(|_| "GravityStrength".to_string())).collect();
        if !keyed.is_empty() {
            s += &format!(", keyed {}", keyed.join("/"));
        }
        s
    }

    /// Whether the emitter is on `age` seconds after it appeared.
    pub fn emitting(&self, age: f32) -> bool {
        let t = if self.loops && self.period > 0.0 { age % self.period } else { age };
        self.active.iter().rev().find(|(time, _)| *time <= t + 1e-4).or(self.active.first()).is_none_or(|(_, on)| *on)
    }

    /// How many of the seconds from `from` to `to` after the emitter appeared it is on:
    /// births are the rate times this, so an emitter on for a moment (an explosion's
    /// burst: 400,000 a second for 0.00001 s) gives its few particles whatever the frame
    /// length.
    pub fn on_time(&self, from: f32, to: f32) -> f32 {
        span_on(&self.active, self.period, self.loops, from, to)
    }

    /// The sequence of the model named `name` that drives this emitter, if any.
    pub fn sequence(&self, name: &str) -> Option<&SequenceEmission> {
        self.sequences.iter().find(|s| s.sequence.eq_ignore_ascii_case(name))
    }

    /// A particle's size as a share of its radius, `age` seconds into a life of `life`
    /// [hypothesis: from the base scale to 1 over the grow time, and back to the base
    /// scale over the fade time before it dies].
    pub fn scale(&self, age: f32, life: f32) -> f32 {
        let mut s = 1.0f32;
        if self.grow_time > 0.0 && age < self.grow_time {
            s = s.min(self.base_scale + (1.0 - self.base_scale) * (age / self.grow_time));
        }
        if self.fade_time > 0.0 && life - age < self.fade_time {
            s = s.min(self.base_scale + (1.0 - self.base_scale) * ((life - age) / self.fade_time).max(0.0));
        }
        s
    }
}

/// How many of the seconds from `from` to `to` on/off `keys` say on (none: all of them),
/// the keys repeating every `period` when `loops`.
fn span_on(keys: &[(f32, bool)], period: f32, loops: bool, from: f32, to: f32) -> f32 {
    if keys.is_empty() {
        return (to - from).max(0.0);
    }
    let state = |t: f32| keys.iter().rev().find(|(k, _)| *k <= t).or(keys.first()).is_none_or(|(_, on)| *on);
    // The span split at the keys (and, looping, at the period's ends).
    let looping = loops && period > 0.0;
    let mut on = 0.0;
    let mut t = from;
    let mut steps = 0;
    while t < to && steps < 4096 {
        steps += 1;
        let (base, local) = if looping { let n = (t / period).floor(); (n * period, t - n * period) } else { (0.0, t) };
        let mut next = keys.iter().map(|(k, _)| *k).filter(|k| *k > local).fold(f32::INFINITY, f32::min);
        if looping {
            next = next.min(period);
        }
        let end = (base + next).min(to);
        if state(local) {
            on += end - t;
        }
        if end <= t {
            break;
        }
        t = end;
    }
    on
}

/// How one sequence of a model drives a particle system's emitter: the `NiPSysEmitterCtlr`
/// tracks (birth rate, on/off) and the emitter float controllers' tracks its controlled
/// blocks give that system [observed: `repbay.nif`, REPCONN's rocket bay, whose Forward,
/// Right and Left each give every one of its 56 systems an on/off track; particles.md
/// §6.7]. Times are the sequence's own.
#[derive(Debug, Clone)]
pub struct SequenceEmission {
    /// The sequence's name (the group a script plays) and its index among the file's
    /// `NiControllerSequence` blocks.
    pub sequence: String,
    pub index: usize,
    pub rate: f32,
    pub rate_keys: Option<FloatKeys>,
    /// On/off keys over the sequence's time; a track without keys is its lone value from
    /// 0 [observed: most of `repbay.nif`'s are a lone false]. Empty: on throughout.
    pub active: Vec<(f32, bool)>,
    /// The sequence's length, and whether it repeats (cycle type 0).
    pub period: f32,
    pub loops: bool,
    /// Emitter fields the sequence keys (speed, radius…).
    pub keyed: Vec<(EmitterParam, FloatKeys)>,
}

impl SequenceEmission {
    /// Its birth rate at the sequence's time `t`.
    pub fn rate_at(&self, t: f32) -> f32 {
        self.rate_keys.as_ref().and_then(|k| k.at(t)).unwrap_or(self.rate).max(0.0)
    }

    /// How many of the seconds from `from` to `to` of the sequence's time it is on. A
    /// clamped sequence holds its end: past its length the last key's state goes on
    /// [hypothesis: as a clamped sequence holds its other tracks' end values].
    pub fn on_time(&self, from: f32, to: f32) -> f32 {
        span_on(&self.active, self.period, self.loops, from, to)
    }
}

/// The on/off keys of bool interpolator `interp`: its keys, or its lone value from 0
/// (`None`: an undefined value, as a blend interpolator's) [documented: nif.xml
/// NiBoolInterpolator; observed: `repbay.nif`].
fn bool_keys(dec: &Decoded, interp: i32) -> Vec<(f32, bool)> {
    match fnv_nif::anim::bool_track(dec, interp) {
        Some(t) if !t.keys.is_empty() => t.keys,
        Some(t) => t.value.map(|v| vec![(0.0, v)]).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Every `NiControllerSequence` of the file with its index among them.
fn sequences<'a>(file: &'a NifFile, dec: &'a Decoded) -> impl Iterator<Item = (usize, &'a Struct)> + 'a {
    file.blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.type_name == "NiControllerSequence")
        .filter_map(|(i, _)| dec.blocks[i].as_ref().ok())
        .enumerate()
}

/// How each sequence of the file drives the emitter of the particle system named
/// `system`, whose emitter modifier is named `emitter` (its float controllers' controller
/// ID) (particles.md §6.7).
fn sequence_emissions(file: &NifFile, dec: &Decoded, system: &str, emitter: &str) -> Vec<SequenceEmission> {
    let text = |s: &Struct, k: &str| s.text(k).map(str::to_string);
    let mut out = Vec::new();
    for (index, seq) in sequences(file, dec) {
        let cycle = sequence_cycle(seq);
        let mut found = SequenceEmission {
            sequence: seq.text("name").unwrap_or_default().to_string(),
            index,
            rate: -1.0,
            rate_keys: None,
            active: Vec::new(),
            period: cycle.stop,
            loops: cycle.cycle_type == 0,
            keyed: Vec::new(),
        };
        let mut drives = false;
        for cb in seq.list("controlled_blocks").unwrap_or_default() {
            let fnv_nif::value::Value::Struct(cb) = cb else { continue };
            if text(cb, "node_name").as_deref() != Some(system) {
                continue;
            }
            let interp = cb.block_ref("interpolator").unwrap_or(-1);
            let ty = text(cb, "controller_type").unwrap_or_default();
            if ty == "NiPSysEmitterCtlr" {
                drives = true;
                match text(cb, "interpolator_id").as_deref() {
                    Some("BirthRate") => match float_keys(file, dec, interp, cycle) {
                        Some(k) if matches!(k.channel, Channel::Keys(_)) => {
                            found.rate = 0.0;
                            found.rate_keys = Some(k);
                        }
                        _ => {
                            let v = dec.get(interp).and_then(|s| s.float("value")).unwrap_or(0.0);
                            if v.is_finite() && v > -1e30 {
                                found.rate = v.max(0.0);
                            }
                        }
                    },
                    Some("EmitterActive") => found.active = bool_keys(dec, interp),
                    _ => {}
                }
            } else if let Some(param) = EmitterParam::of_controller(&ty).filter(|_| text(cb, "controller_id").as_deref() == Some(emitter)) {
                if let Some(k) = float_keys(file, dec, interp, cycle) {
                    drives = true;
                    found.keyed.push((param, k));
                }
            }
        }
        if drives {
            out.push(found);
        }
    }
    out
}

/// The on/off keys each sequence of the file gives the `NiPSysModifierActiveCtlr` of the
/// modifier named `modifier` of system `system`, by sequence index (particles.md §6.7).
fn sequence_modifier_actives(file: &NifFile, dec: &Decoded, system: &str, modifier: &str) -> Vec<(usize, Vec<(f32, bool)>)> {
    let text = |s: &Struct, k: &str| s.text(k).map(str::to_string);
    let mut out = Vec::new();
    for (index, seq) in sequences(file, dec) {
        for cb in seq.list("controlled_blocks").unwrap_or_default() {
            let fnv_nif::value::Value::Struct(cb) = cb else { continue };
            if text(cb, "node_name").as_deref() == Some(system)
                && text(cb, "controller_type").as_deref() == Some("NiPSysModifierActiveCtlr")
                && text(cb, "controller_id").as_deref() == Some(modifier)
            {
                out.push((index, bool_keys(dec, cb.block_ref("interpolator").unwrap_or(-1))));
                break;
            }
        }
    }
    out
}

/// The particle systems of one file.
#[derive(Debug, Clone, Default)]
pub struct PreparedParticles {
    pub systems: Vec<ParticleSystemDef>,
    /// Block types met among modifiers and controllers that are not read.
    pub unread: Vec<String>,
}

/// An emitter's birth rate and on/off keys as a sequence of the file drives them.
struct SequenceEmitter {
    rate: f32,
    rate_keys: Option<FloatKeys>,
    active: Vec<(f32, bool)>,
    period: f32,
    loops: bool,
}

/// The `NiPSysEmitterCtlr` tracks of the first sequence that drives the particle system
/// named `system`: interpolator ID `BirthRate` (a float interpolator's value) and
/// `EmitterActive` (bool keys) [observed: the controlled blocks of
/// `explosiongrenadefrag.nif`'s `SpecialIdle_AreaEffect`, whose emitter controllers
/// carry blend interpolators]; the sequence's stop time is the period, and it repeats
/// when its cycle type is 0 [documented: nif.xml NiControllerSequence, CycleType].
fn sequence_emitter(file: &NifFile, dec: &Decoded, system: &str) -> Option<SequenceEmitter> {
    let text = |s: &Struct, k: &str| s.text(k).map(str::to_string);
    for i in sequence_order(file, dec) {
        let Ok(seq) = &dec.blocks[i] else { continue };
        let mut found = SequenceEmitter { rate: -1.0, rate_keys: None, active: Vec::new(), period: seq.float("stop_time").unwrap_or(0.0) - seq.float("start_time").unwrap_or(0.0), loops: seq.int("cycle_type").unwrap_or(0) == 0 };
        for cb in seq.list("controlled_blocks").unwrap_or_default() {
            let fnv_nif::value::Value::Struct(cb) = cb else { continue };
            if text(cb, "node_name").as_deref() != Some(system) || text(cb, "controller_type").as_deref() != Some("NiPSysEmitterCtlr") {
                continue;
            }
            let interp = cb.block_ref("interpolator").unwrap_or(-1);
            match text(cb, "interpolator_id").as_deref() {
                Some("BirthRate") => {
                    // Keys over the sequence's time, or the interpolator's lone value.
                    match float_keys(file, dec, interp, sequence_cycle(seq)) {
                        Some(k) if matches!(k.channel, Channel::Keys(_)) => {
                            found.rate = 0.0;
                            found.rate_keys = Some(k);
                        }
                        _ => {
                            let v = dec.get(interp).and_then(|s| s.float("value")).unwrap_or(0.0);
                            if v.is_finite() && v > -1e30 {
                                found.rate = v.max(0.0);
                            }
                        }
                    }
                }
                Some("EmitterActive") => {
                    found.active = bool_keys(dec, interp);
                }
                _ => {}
            }
        }
        if found.rate >= 0.0 {
            return Some(found);
        }
    }
    None
}

/// A placed model plays its `Idle` sequence, or `SpecialIdle` when it has none
/// [hypothesis: nvf_assets::anim_play picks the same for the drawn model]; the other
/// groups run only when a script or the game starts them. So the sequences that may key
/// a placed model's particle systems are looked at in that order: `Idle`, `SpecialIdle`,
/// then the rest in file order (for effect models whose one sequence is another group,
/// such as an explosion's `SpecialIdle_AreaEffect`). Before 2026-10-23 the first
/// sequence of the file won: `nvbouldersm.nif`'s `Forward` puffed dust each time a
/// boulder was drawn (docs/spec-notes/particles.md §6.9).
fn sequence_order(file: &NifFile, dec: &Decoded) -> Vec<usize> {
    let mut seqs: Vec<(usize, u8)> = file
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.type_name == "NiControllerSequence")
        .map(|(i, _)| {
            let name = dec.blocks[i].as_ref().ok().and_then(|s| s.text("name")).unwrap_or_default().to_ascii_lowercase();
            (i, sequence_rank(&name))
        })
        .collect();
    seqs.sort_by_key(|&(i, rank)| (rank, i));
    seqs.into_iter().map(|(i, _)| i).collect()
}

/// 0 for `Idle`, 1 for `SpecialIdle`, 2 for any other sequence name (lower case).
fn sequence_rank(name: &str) -> u8 {
    match name {
        "idle" => 0,
        "specialidle" => 1,
        _ => 2,
    }
}

/// The soft-intersection fade distance of a system's particles: the shapes' rule
/// (`nif_material::soft_distance`, a quarter of the largest side, at most 160 units)
/// applied to its largest particle, twice the emitter's radius plus its variation.
/// Its blood mist (radius 50 ± 10) fades over 30 units, a drop (1.25 ± 0.5) over 1
/// [engineering choice; nif.md §6 "Soft intersections of particles"].
pub fn particle_soft_distance(v: &EmitterValues) -> f32 {
    crate::nif_material::soft_distance(2.0 * (v.radius + v.radius_variation.abs()).max(0.0))
}

/// The `NiAlphaController` keys a particle system's own material has: its own
/// interpolator's keys on its own cycle, or the blend interpolator's from a sequence
/// (controlled block named after the system, controller type `NiAlphaController`)
/// [observed: `sanddust02.nif`'s `PCloud01`, stored alpha 0, keyed 0 → 0.05 → 0 in its
/// `Idle`; docs/spec-notes/particles.md §6.9].
fn material_alpha_keys(file: &NifFile, dec: &Decoded, material: Option<i32>, system: &str) -> Option<FloatKeys> {
    let type_of = |r: i32| usize::try_from(r).ok().and_then(|i| file.blocks.get(i)).map(|b| b.type_name.as_str());
    let mut c = material.and_then(|m| dec.get(m)).and_then(|m| m.block_ref("controller")).unwrap_or(-1);
    let mut steps = 0;
    while let (Some(ty), Some(ctl)) = (type_of(c), dec.get(c)) {
        if ty == "NiAlphaController" {
            if let Some(k) = float_keys(file, dec, ctl.block_ref("interpolator").unwrap_or(-1), controller_cycle(ctl)).filter(|k| matches!(k.channel, Channel::Keys(_))) {
                return Some(k);
            }
            break;
        }
        c = ctl.block_ref("next_controller").unwrap_or(-1);
        steps += 1;
        if steps > 64 {
            break;
        }
    }
    let text = |s: &Struct, k: &str| s.text(k).map(str::to_string);
    for i in sequence_order(file, dec) {
        let Ok(seq) = &dec.blocks[i] else { continue };
        for cb in seq.list("controlled_blocks").unwrap_or_default() {
            let fnv_nif::value::Value::Struct(cb) = cb else { continue };
            if text(cb, "node_name").as_deref() == Some(system) && text(cb, "controller_type").as_deref() == Some("NiAlphaController") {
                if let Some(k) = float_keys(file, dec, cb.block_ref("interpolator").unwrap_or(-1), sequence_cycle(seq)).filter(|k| matches!(k.channel, Channel::Keys(_))) {
                    return Some(k);
                }
            }
        }
    }
    None
}

/// A sequence's own timing as a controller's: from 0 to its length, its cycle type.
fn sequence_cycle(seq: &Struct) -> OwnCycle {
    let (start, stop) = (seq.float("start_time").unwrap_or(0.0), seq.float("stop_time").unwrap_or(0.0));
    OwnCycle { start: 0.0, stop: stop - start, frequency: 1.0, phase: 0.0, cycle_type: seq.int("cycle_type").unwrap_or(0) }
}

/// A controller's own timing (NiTimeController fields [documented: nif.xml]).
fn controller_cycle(ctl: &Struct) -> OwnCycle {
    OwnCycle {
        start: ctl.float("start_time").unwrap_or(0.0),
        stop: ctl.float("stop_time").unwrap_or(0.0),
        frequency: ctl.float("frequency").unwrap_or(1.0),
        phase: ctl.float("phase").unwrap_or(0.0),
        cycle_type: (ctl.int("flags").unwrap_or(0) >> 1) & 3,
    }
}

/// The float interpolator at `interp` as keys on `cycle` (`None`: not a float interpolator).
fn float_keys(file: &NifFile, dec: &Decoded, interp: i32, cycle: OwnCycle) -> Option<FloatKeys> {
    let ty = usize::try_from(interp).ok().and_then(|i| file.blocks.get(i)).map(|b| b.type_name.as_str())?;
    match fnv_nif::anim::value_track(dec, interp, ty)? {
        ValueTrack::Float(channel) => Some(FloatKeys { channel, cycle }),
        ValueTrack::Point3(_) => None,
    }
}

/// The float keys a sequence of the file gives the controller of type `controller` that
/// drives the modifier named `modifier` of the particle system named `system` (controlled
/// blocks whose controller ID is the modifier's name [observed: `explosiongenericl01.nif`'s
/// gravity strength]).
fn sequence_float(file: &NifFile, dec: &Decoded, system: &str, controller: &str, modifier: &str) -> Option<FloatKeys> {
    let text = |s: &Struct, k: &str| s.text(k).map(str::to_string);
    for i in sequence_order(file, dec) {
        let Ok(seq) = &dec.blocks[i] else { continue };
        for cb in seq.list("controlled_blocks").unwrap_or_default() {
            let fnv_nif::value::Value::Struct(cb) = cb else { continue };
            if text(cb, "node_name").as_deref() == Some(system) && text(cb, "controller_type").as_deref() == Some(controller) && text(cb, "controller_id").as_deref() == Some(modifier) {
                if let Some(k) = float_keys(file, dec, cb.block_ref("interpolator").unwrap_or(-1), sequence_cycle(seq)) {
                    return Some(k);
                }
            }
        }
    }
    None
}

/// The bool keys a sequence of the file gives the `NiPSysModifierActiveCtlr` of the
/// modifier named `modifier` of the particle system named `system` (controlled blocks
/// whose controller ID is the modifier's name [observed: `explosiongrenadefrag.nif`]).
fn sequence_modifier_active(file: &NifFile, dec: &Decoded, system: &str, modifier: &str) -> Option<Vec<(f32, bool)>> {
    let text = |s: &Struct, k: &str| s.text(k).map(str::to_string);
    for i in sequence_order(file, dec) {
        let Ok(seq) = &dec.blocks[i] else { continue };
        for cb in seq.list("controlled_blocks").unwrap_or_default() {
            let fnv_nif::value::Value::Struct(cb) = cb else { continue };
            if text(cb, "node_name").as_deref() == Some(system)
                && text(cb, "controller_type").as_deref() == Some("NiPSysModifierActiveCtlr")
                && text(cb, "controller_id").as_deref() == Some(modifier)
            {
                let keys = bool_keys(dec, cb.block_ref("interpolator").unwrap_or(-1));
                return Some(keys);
            }
        }
    }
    None
}

fn affine(t: &NifTransform) -> Affine3A {
    let r = &t.rotation;
    let m = Mat3::from_cols(Vec3::new(r[0], r[3], r[6]), Vec3::new(r[1], r[4], r[7]), Vec3::new(r[2], r[5], r[8]));
    Affine3A::from_mat3_translation(m * t.scale, Vec3::from_array(t.translation))
}

fn rgba(s: &Struct, key: &str) -> [f32; 4] {
    s.floats(key).filter(|c| c.len() >= 4).map_or([1.0; 4], |c| [c[0], c[1], c[2], c[3]])
}

/// A mesh with the attributes a particle mesh has, for the material to be built against.
pub fn particle_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; 3]);
    m.insert_indices(bevy::mesh::Indices::U32(vec![0, 1, 2]));
    m
}

impl ModelCtx<'_> {
    /// The particle systems of the file at `path` (read on first use; a failure is
    /// remembered).
    pub fn particles(&mut self, path: &str) -> Option<Arc<PreparedParticles>> {
        let key = fnv_vfs::normalize(path);
        if let Some(p) = self.cache.particles.get(&key) {
            return p.clone();
        }
        let p = match crate::guard::guarded(|| self.prepare_particles(&key)) {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                crate::guard::failed(&mut self.cache.failures, "particles", format!("{key}: {e}"));
                None
            }
        };
        self.cache.particles.insert(key, p.clone());
        p
    }

    fn prepare_particles(&mut self, path: &str) -> Result<PreparedParticles, String> {
        let bytes = self.data.read(path).ok_or("not in any archive")?;
        let schema = self.data.schema.clone();
        let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, &bytes, &schema);
        let objects = scene_objects(&file, &dec);
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        // A node's place in the file, the root's own transform left out.
        let place = |block: i32| -> Affine3A {
            let mut m = Affine3A::IDENTITY;
            let mut at = usize::try_from(block).ok().and_then(|b| by_block.get(&b));
            let mut depth = 0;
            while let Some(o) = at.filter(|o| o.parent.is_some()) {
                m = affine(&o.transform) * m;
                at = o.parent.and_then(|p| by_block.get(&p));
                depth += 1;
                if depth > 256 {
                    break;
                }
            }
            m
        };
        let type_of = |r: i32| usize::try_from(r).ok().and_then(|i| file.blocks.get(i)).map(|b| b.type_name.as_str());
        let mut out = PreparedParticles::default();
        for (i, b) in file.blocks.iter().enumerate() {
            // A strip system is a particle system by its fields (particles.md §6.2).
            let strips = b.type_name == "BSStripParticleSystem";
            if b.type_name != "NiParticleSystem" && !strips {
                continue;
            }
            let Ok(ps) = &dec.blocks[i] else { continue };
            // Its properties, with those of the nodes above it.
            let parent = objects.iter().find(|o| o.is_node && o.children.contains(&i)).map(|o| o.block);
            let fake = SceneObject { block: i, type_name: b.type_name.clone(), name: None, parent, transform: NifTransform::IDENTITY, is_node: false, children: Vec::new() };
            let mut props = shape_props(&file, &dec, &fake, &by_block);
            // A keyed material alpha scales each particle's alpha at draw time; the
            // material itself is then opaque in alpha (its stored value is often 0).
            let material_block = ps.refs("properties").unwrap_or_default().iter().copied().find(|&r| type_of(r) == Some("NiMaterialProperty"));
            let alpha_keys = material_alpha_keys(&file, &dec, material_block, ps.text("name").unwrap_or_default());
            if alpha_keys.is_some() {
                props.material_alpha = Some(1.0);
            }
            let mut template = particle_mesh();
            // Its soft distance is set once its emitter's radius is read (below).
            let (mut material, _missing) = shape_material(&props, &mut template, &mut self.textures, &self.data, &mut self.images, self.formats.as_deref());
            let data = ps.block_ref("data").and_then(|r| dec.get(r));
            let mut def = ParticleSystemDef {
                name: ps.text("name").unwrap_or_default().to_string(),
                max: (data.and_then(|d| d.int("bs_max_vertices")).unwrap_or(0).max(0) as usize).min(MAX_PARTICLES),
                material: Handle::default(),
                emitter: Emitter::blank(),
                rate: 0.0,
                rate_keys: None,
                active: Vec::new(),
                period: 0.0,
                loops: true,
                sequences: Vec::new(),
                max_emitters: 0,
                grow_time: 0.0,
                fade_time: 0.0,
                base_scale: 1.0,
                color: None,
                color_keys: None,
                rotation: None,
                gravity: Vec::new(),
                drag: Vec::new(),
                bombs: Vec::new(),
                colliders: Vec::new(),
                strip: strips.then(|| Strip { max_points: data.and_then(|d| d.int("max_point_count")).unwrap_or(2).clamp(2, MAX_STRIP_POINTS as i64) as usize, step: 1.0 / 30.0 }),
                subtextures: data
                    .and_then(|d| d.floats("subtexture_offsets"))
                    .map(|f| f.as_chunks::<4>().0.to_vec())
                    .unwrap_or_default(),
                mover: None,
                alpha_keys,
                soft: 0.0,
            };
            // The modifiers' on/off keys: a `NiPSysModifierActiveCtlr` in the system's
            // controller chain names its modifier; a blend interpolator leaves the keys to
            // the model's sequence.
            let system_name = def.name.clone();
            let mut switched: HashMap<String, ModifierActive> = HashMap::new();
            // Float controllers by (controller type, modifier name): their own keys, or a
            // blend interpolator's from the model's sequence (particles.md §6.6).
            let mut floats: HashMap<(String, String), FloatKeys> = HashMap::new();
            {
                let mut c = ps.block_ref("controller").unwrap_or(-1);
                let mut steps = 0;
                while let (Some(ty), Some(ctl)) = (type_of(c), dec.get(c)) {
                    if ty == "NiPSysModifierActiveCtlr" {
                        if let Some(name) = ctl.text("modifier_name").map(str::to_string) {
                            let own = ctl.block_ref("interpolator").and_then(|r| fnv_nif::anim::bool_track(&dec, r)).map(|t| t.keys).unwrap_or_default();
                            let keys = if own.is_empty() { sequence_modifier_active(&file, &dec, &system_name, &name).unwrap_or_default() } else { own };
                            let by_sequence = sequence_modifier_actives(&file, &dec, &system_name, &name);
                            switched.insert(name, ModifierActive { keys, by_sequence });
                        }
                    } else if ty == "NiPSysGravityStrengthCtlr" || EmitterParam::of_controller(ty).is_some() {
                        if let Some(name) = ctl.text("modifier_name").map(str::to_string) {
                            let own = float_keys(&file, &dec, ctl.block_ref("interpolator").unwrap_or(-1), controller_cycle(ctl));
                            if let Some(k) = own.or_else(|| sequence_float(&file, &dec, &system_name, ty, &name)) {
                                floats.insert((ty.to_string(), name), k);
                            }
                        }
                    }
                    c = ctl.block_ref("next_controller").unwrap_or(-1);
                    steps += 1;
                    if steps > 64 {
                        break;
                    }
                }
            }
            let modifier_active = |m: &Struct| m.text("name").and_then(|n| switched.get(n)).cloned().unwrap_or_default();
            let mut has_emitter = false;
            let mut emitter_name = String::new();
            // The block the emitter starts particles from (its object, or its first mesh).
            let mut anchor = -1;
            for &r in ps.refs("modifiers").unwrap_or_default() {
                let (Some(ty), Some(m)) = (type_of(r), dec.get(r)) else { continue };
                let f = |key: &str| m.float(key).unwrap_or(0.0);
                if m.get("active").is_some_and(|v| matches!(v, fnv_nif::value::Value::Bool(false))) {
                    continue;
                }
                match ty {
                    "NiPSysBoxEmitter" | "NiPSysSphereEmitter" | "NiPSysCylinderEmitter" => {
                        has_emitter = true;
                        emitter_name = m.text("name").unwrap_or_default().to_string();
                        let round = ty != "NiPSysBoxEmitter";
                        anchor = m.block_ref("emitter_object").unwrap_or(-1);
                        def.emitter = Emitter {
                            frame: place(m.block_ref("emitter_object").unwrap_or(-1)),
                            object: dec.get(m.block_ref("emitter_object").unwrap_or(-1)).and_then(|o| o.text("name")).map(str::to_string),
                            size: if round { Vec3::new(f("radius"), f("radius"), f("height")) } else { Vec3::new(f("width"), f("height"), f("depth")) },
                            round,
                            speed: f("speed"),
                            speed_variation: f("speed_variation"),
                            declination: f("declination"),
                            declination_variation: f("declination_variation"),
                            planar_angle: f("planar_angle"),
                            planar_angle_variation: f("planar_angle_variation"),
                            color: rgba(m, "initial_color"),
                            radius: f("initial_radius"),
                            radius_variation: f("radius_variation"),
                            life: f("life_span"),
                            life_variation: f("life_span_variation"),
                            mesh: None,
                            keyed: Vec::new(),
                        };
                    }
                    "NiPSysMeshEmitter" => {
                        has_emitter = true;
                        emitter_name = m.text("name").unwrap_or_default().to_string();
                        anchor = m.refs("emitter_meshes").and_then(|r| r.first().copied()).unwrap_or(parent.map_or(-1, |p| p as i32));
                        let meshes: Vec<EmitterMesh> = m
                            .refs("emitter_meshes")
                            .unwrap_or_default()
                            .iter()
                            .filter_map(|&g| {
                                let md = mesh_data(dec.get(g)?.block_ref("data").and_then(|d| dec.get(d))?);
                                let v3 = |a: &[[f32; 3]]| a.iter().map(|p| Vec3::from_array(*p)).collect::<Vec<_>>();
                                Some(EmitterMesh::new(place(g), v3(&md.positions), v3(&md.normals), md.triangles))
                            })
                            .collect();
                        let from = match m.int("emission_type").unwrap_or(3) {
                            0 => EmitFrom::Vertices,
                            1 => EmitFrom::FaceCenter,
                            2 => EmitFrom::EdgeCenter,
                            4 => EmitFrom::EdgeSurface,
                            _ => EmitFrom::FaceSurface,
                        };
                        let velocity = match m.int("initial_velocity_type").unwrap_or(0) {
                            1 => InitialVelocity::Random,
                            2 => InitialVelocity::Direction(m.floats("emission_axis").filter(|a| a.len() >= 3).map_or(Vec3::X, |a| Vec3::new(a[0], a[1], a[2]))),
                            _ => InitialVelocity::Normals,
                        };
                        def.emitter = Emitter {
                            // No meshes: from the system's parent node, named `<system>-Emitter`
                            // as the box emitters' objects are [design, particles.md §6.1].
                            frame: place(parent.map_or(-1, |p| p as i32)),
                            object: parent.and_then(|p| dec.get(p as i32)).and_then(|o| o.text("name")).map(str::to_string),
                            speed: f("speed"),
                            speed_variation: f("speed_variation"),
                            declination: f("declination"),
                            declination_variation: f("declination_variation"),
                            planar_angle: f("planar_angle"),
                            planar_angle_variation: f("planar_angle_variation"),
                            color: rgba(m, "initial_color"),
                            radius: f("initial_radius"),
                            radius_variation: f("radius_variation"),
                            life: f("life_span"),
                            life_variation: f("life_span_variation"),
                            mesh: (!meshes.is_empty()).then_some(MeshEmission { meshes, from, velocity }),
                            ..Emitter::blank()
                        };
                    }
                    "NiPSysBombModifier" => {
                        let frame = place(m.block_ref("bomb_object").unwrap_or(-1));
                        let axis = m.floats("bomb_axis").filter(|a| a.len() >= 3).map_or(Vec3::Z, |a| Vec3::new(a[0], a[1], a[2]));
                        def.bombs.push(Bomb {
                            origin: frame.translation.into(),
                            axis: frame.transform_vector3(axis).normalize_or(Vec3::Z),
                            delta_v: f("delta_v"),
                            decay: f("decay"),
                            decay_type: m.int("decay_type").unwrap_or(0),
                            symmetry: m.int("symmetry_type").unwrap_or(0),
                            active: modifier_active(m),
                        });
                    }
                    "NiPSysColliderManager" => {
                        // The manager's chain of colliders; planar and spherical ones are read.
                        let mut c = m.block_ref("collider").unwrap_or(-1);
                        let mut steps = 0;
                        while let (Some(cty), Some(col)) = (type_of(c), dec.get(c)) {
                            if cty == "NiPSysPlanarCollider" {
                                let frame = place(col.block_ref("collider_object").unwrap_or(-1));
                                let axis = |k: &str, d: Vec3| col.floats(k).filter(|a| a.len() >= 3).map_or(d, |a| Vec3::new(a[0], a[1], a[2]));
                                let (x, y) = (frame.transform_vector3(axis("x_axis", Vec3::X)), frame.transform_vector3(axis("y_axis", Vec3::Y)));
                                let cf = |k: &str| col.float(k).unwrap_or(0.0);
                                def.colliders.push(Collider {
                                    origin: frame.translation.into(),
                                    x_axis: x.normalize_or(Vec3::X),
                                    y_axis: y.normalize_or(Vec3::Y),
                                    half_width: cf("width") * 0.5 * x.length(),
                                    half_height: cf("height") * 0.5 * y.length(),
                                    sphere: None,
                                    bounce: cf("bounce"),
                                    die: matches!(col.get("die_on_collide"), Some(fnv_nif::value::Value::Bool(true))),
                                });
                            } else if cty == "NiPSysSphericalCollider" {
                                // The object's place times its own scale gives the sphere's.
                                let frame = place(col.block_ref("collider_object").unwrap_or(-1));
                                def.colliders.push(Collider {
                                    origin: frame.translation.into(),
                                    x_axis: Vec3::X,
                                    y_axis: Vec3::Y,
                                    half_width: 0.0,
                                    half_height: 0.0,
                                    sphere: Some(col.float("radius").unwrap_or(0.0) * frame.transform_vector3(Vec3::X).length()),
                                    bounce: col.float("bounce").unwrap_or(0.0),
                                    die: matches!(col.get("die_on_collide"), Some(fnv_nif::value::Value::Bool(true))),
                                });
                            } else if !out.unread.iter().any(|u| u == cty) {
                                out.unread.push(cty.to_string());
                            }
                            c = col.block_ref("next_collider").unwrap_or(-1);
                            steps += 1;
                            if steps > 16 {
                                break;
                            }
                        }
                    }
                    "NiPSysColorModifier" => {
                        def.color_keys = m.block_ref("data").and_then(|d| dec.get(d)).and_then(|d| d.strukt("data")).and_then(Keys::<4>::from_group).filter(|k| !k.is_empty());
                    }
                    "BSPSysStripUpdateModifier" => {
                        if let Some(st) = def.strip.as_mut() {
                            st.step = f("update_delta_time").max(1e-3);
                        }
                    }
                    "NiPSysGrowFadeModifier" => {
                        def.grow_time = f("grow_time");
                        def.fade_time = f("fade_time");
                        def.base_scale = f("base_scale");
                    }
                    "BSPSysSimpleColorModifier" => {
                        let c = m.floats("colors").unwrap_or_default();
                        if c.len() >= 12 {
                            def.color = Some(SimpleColor {
                                fade_in: f("fade_in_percent"),
                                fade_out: f("fade_out_percent"),
                                // The schema's names for the four percentages, in file order.
                                color1_end: f("color1_end_percent"),
                                color2_start: f("color1_start_percent"),
                                color2_end: f("color2_end_percent"),
                                color3_start: f("color2_start_percent"),
                                colors: std::array::from_fn(|k| [c[k * 4], c[k * 4 + 1], c[k * 4 + 2], c[k * 4 + 3]]),
                            });
                        }
                    }
                    "NiPSysRotationModifier" => {
                        def.rotation = Some(Rotation {
                            speed: f("rotation_speed"),
                            speed_variation: f("rotation_speed_variation"),
                            angle: f("rotation_angle"),
                            angle_variation: f("rotation_angle_variation"),
                            random_sign: matches!(m.get("random_rot_speed_sign"), Some(fnv_nif::value::Value::Bool(true))),
                        });
                    }
                    "NiPSysGravityModifier" => {
                        let frame = place(m.block_ref("gravity_object").unwrap_or(-1));
                        let axis = m.floats("gravity_axis").filter(|a| a.len() >= 3).map_or(Vec3::Z, |a| Vec3::new(a[0], a[1], a[2]));
                        def.gravity.push(Gravity {
                            direction: frame.transform_vector3(axis).normalize_or_zero(),
                            origin: frame.translation.into(),
                            strength: f("strength"),
                            strength_keys: m.text("name").and_then(|n| floats.get(&("NiPSysGravityStrengthCtlr".to_string(), n.to_string()))).cloned(),
                            spherical: m.int("force_type").unwrap_or(0) == 1,
                            active: modifier_active(m),
                        });
                    }
                    "NiPSysDragModifier" => {
                        let frame = place(m.block_ref("drag_object").unwrap_or(-1));
                        let axis = m.floats("drag_axis").filter(|a| a.len() >= 3).map_or(Vec3::X, |a| Vec3::new(a[0], a[1], a[2]));
                        def.drag.push(Drag { axis: frame.transform_vector3(axis).normalize_or_zero(), percentage: f("percentage"), active: modifier_active(m) });
                    }
                    // Ageing, moving and bounds are what any simulation does; a spawn
                    // modifier only acts when the age-death modifier spawns on death.
                    "NiPSysAgeDeathModifier" | "NiPSysPositionModifier" | "NiPSysBoundUpdateModifier" | "NiPSysSpawnModifier" => {}
                    other => {
                        if !out.unread.iter().any(|u| u == other) {
                            out.unread.push(other.to_string());
                        }
                    }
                }
            }
            def.emitter.keyed = floats
                .iter()
                .filter(|((_, name), _)| *name == emitter_name)
                .filter_map(|((ty, _), k)| Some((EmitterParam::of_controller(ty)?, k.clone())))
                .collect();
            // The controllers: the emitter's birth rate and when it is on.
            let mut c = ps.block_ref("controller").unwrap_or(-1);
            let mut steps = 0;
            while let (Some(ty), Some(ctl)) = (type_of(c), dec.get(c)) {
                match ty {
                    "BSPSysMultiTargetEmitterCtlr" | "NiPSysEmitterCtlr" => {
                        let interp = ctl.block_ref("interpolator").unwrap_or(-1);
                        let rate = dec.get(interp).and_then(|s| s.float("value")).unwrap_or(0.0);
                        // A value at the bottom of the float range stands for "none".
                        def.rate = if rate.is_finite() && rate > -1e30 { rate.max(0.0) } else { 0.0 };
                        // Keys over the controller's time (the gore sprays, particles.md §6.6).
                        def.rate_keys = float_keys(&file, &dec, interp, controller_cycle(ctl)).filter(|k| matches!(k.channel, Channel::Keys(_)));
                        def.active = ctl.block_ref("visibility_interpolator").and_then(|r| fnv_nif::anim::bool_track(&dec, r)).map(|t| t.keys).unwrap_or_default();
                        def.period = ctl.float("stop_time").unwrap_or(0.0);
                        // TimeController flags bits 1–2: the cycle type, 0 loops.
                        def.loops = (ctl.int("flags").unwrap_or(0) >> 1) & 3 == 0;
                        def.max_emitters = ctl.int("max_emitters").unwrap_or(0).max(0) as usize;
                    }
                    // Read with the modifiers above.
                    "NiPSysUpdateCtlr" | "NiPSysModifierActiveCtlr" | "NiPSysGravityStrengthCtlr" => {}
                    other if EmitterParam::of_controller(other).is_some() => {}
                    other => {
                        if !out.unread.iter().any(|u| u == other) {
                            out.unread.push(other.to_string());
                        }
                    }
                }
                c = ctl.block_ref("next_controller").unwrap_or(-1);
                steps += 1;
                if steps > 64 {
                    break;
                }
            }
            // How each of the model's sequences drives it, for a drawn model playing one
            // (particles.md §6.7); a sequence that gives no birth rate leaves the
            // controller's own.
            def.sequences = sequence_emissions(&file, &dec, &def.name, &emitter_name);
            for s in &mut def.sequences {
                if s.rate < 0.0 && s.rate_keys.is_none() {
                    s.rate = def.rate;
                    s.rate_keys = def.rate_keys.clone();
                }
            }
            // An emitter controller of a model's own sequence leaves its rate and on/off
            // keys to the sequence (its interpolators are blend interpolators). Without a
            // drawn model to say which plays (an explosion), the first sequence's.
            if (def.rate <= 0.0 && def.rate_keys.is_none()) || def.active.is_empty() {
                if let Some(se) = sequence_emitter(&file, &dec, &def.name) {
                    if def.rate <= 0.0 && def.rate_keys.is_none() {
                        def.rate = se.rate;
                        def.rate_keys = se.rate_keys;
                    }
                    if def.active.is_empty() {
                        def.active = se.active;
                    }
                    def.period = se.period;
                    def.loops = se.loops;
                }
            }
            def.mover = mover_of(&file, &dec, &by_block, anchor, &place);
            // A particle fades out towards the surface behind it over a quarter of its
            // largest width, as an effect shape does over a quarter of its largest side
            // (the template quad is one unit across, so the particle's own size is used):
            // over the shapes' full 160 units, the blood mist puffed a few units off a
            // body drew at a few percent and no hit on an actor showed its spray
            // (#1074; nif.md §6 "Soft intersections of particles").
            if material.extension.soft.x > 0.0 {
                // A keyed radius as it starts, or the field's own if that is larger.
                let v = def.emitter.values(0.0);
                let v = EmitterValues { radius: v.radius.max(def.emitter.radius), radius_variation: v.radius_variation.abs().max(def.emitter.radius_variation.abs()), ..v };
                material.extension.soft.x = particle_soft_distance(&v);
                def.soft = material.extension.soft.x;
            }
            def.material = self.materials.add(material);
            if has_emitter && def.max > 0 {
                out.systems.push(def);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_then_special_idle_key_placed_emitters() {
        let mut names = ["forward", "specialidle", "idle", "backward"].map(|n| (sequence_rank(n), n));
        names.sort();
        assert_eq!(names.map(|(_, n)| n), ["idle", "specialidle", "backward", "forward"]);
    }

    fn def() -> ParticleSystemDef {
        ParticleSystemDef {
            name: String::new(),
            max: 8,
            material: Handle::default(),
            emitter: Emitter::blank(),
            rate: 10.0,
            rate_keys: None,
            active: vec![(0.0, true), (0.5, false)],
            period: 1.0,
            loops: true,
            sequences: Vec::new(),
            max_emitters: 2,
            grow_time: 0.5,
            fade_time: 0.25,
            base_scale: 0.2,
            color: None,
            color_keys: None,
            rotation: None,
            gravity: Vec::new(),
            drag: Vec::new(),
            bombs: Vec::new(),
            colliders: Vec::new(),
            strip: None,
            subtextures: Vec::new(),
            mover: None,
            alpha_keys: None,
            soft: 0.0,
        }
    }

    #[test]
    fn emitter_keys_repeat_and_sizes_grow_and_fade() {
        let d = def();
        assert!(d.emitting(0.0) && d.emitting(0.4) && !d.emitting(0.6) && d.emitting(1.2) && !d.emitting(1.7));
        let once = ParticleSystemDef { loops: false, ..def() };
        assert!(once.emitting(0.2) && !once.emitting(1.2));
        // Grows from the base scale to full size, then back down before it dies.
        assert!((d.scale(0.0, 2.0) - 0.2).abs() < 1e-6);
        assert!((d.scale(0.25, 2.0) - 0.6).abs() < 1e-6);
        assert_eq!(d.scale(1.0, 2.0), 1.0);
        assert!((d.scale(1.875, 2.0) - 0.6).abs() < 1e-6);
        assert!((d.scale(2.0, 2.0) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_switched_modifier_acts_while_its_keys_say_on() {
        // The frag grenade's smoke drags: on from 0, off at 0.33 s, clamped (5.03 s).
        let drag = ModifierActive { keys: vec![(0.0, true), (0.333, false), (5.033, false)], ..Default::default() };
        assert!(drag.on(0.0, 5.033, false) && drag.on(0.3, 5.033, false));
        assert!(!drag.on(0.4, 5.033, false) && !drag.on(9.0, 5.033, false));
        // Looping keys repeat after the period.
        assert!(drag.on(5.1, 5.033, true));
        // No keys: always on.
        assert!(ModifierActive::default().on(3.0, 0.0, false));
    }

    fn emission(sequence: &str, index: usize, active: Vec<(f32, bool)>, period: f32) -> SequenceEmission {
        SequenceEmission { sequence: sequence.into(), index, rate: 1200.0, rate_keys: None, active, period, loops: false, keyed: Vec::new() }
    }

    #[test]
    fn a_drawn_model_emits_by_the_sequence_it_plays() {
        // repbay.nif's PFire01: off all through Forward (a lone false), on from 0.5 to
        // 1 s of Right (23.3 s, clamped).
        let d = ParticleSystemDef {
            sequences: vec![emission("Forward", 0, vec![(0.0, false)], 19.97), emission("Right", 1, vec![(0.0, false), (0.5, true), (1.0, false), (23.33, false)], 23.33)],
            ..def()
        };
        assert_eq!(d.sequence("forward").map(|s| s.index), Some(0));
        assert!(d.sequence("Left").is_none());
        let fwd = d.sequence("Forward").unwrap();
        assert_eq!(fwd.on_time(0.0, 30.0), 0.0);
        let right = d.sequence("Right").unwrap();
        assert!((right.on_time(0.0, 2.0) - 0.5).abs() < 1e-5);
        assert!((right.on_time(0.75, 0.8) - 0.05).abs() < 1e-5);
        assert_eq!(right.rate_at(0.6), 1200.0);
        // A clamped sequence holds its end: PCloud04's Forward turns it on at 18.3 s and
        // it stays on past the sequence's end until another sequence replaces it.
        let cloud = emission("Forward", 0, vec![(0.0, false), (18.33, true), (19.97, true)], 19.97);
        assert!((cloud.on_time(20.0, 21.0) - 1.0).abs() < 1e-5);
        assert_eq!(cloud.on_time(0.0, 18.0), 0.0);
    }

    #[test]
    fn a_modifier_follows_the_sequence_a_particle_was_born_under() {
        let bomb = ModifierActive { keys: vec![(0.0, true)], by_sequence: vec![(1, vec![(0.0, false), (2.0, true)])] };
        assert!(!bomb.on_in(Some(1), 1.0, 23.3, false));
        assert!(bomb.on_in(Some(1), 3.0, 23.3, false));
        // Another sequence (no keys of its own) or none: the modifier's own keys.
        assert!(bomb.on_in(Some(0), 1.0, 20.0, false));
        assert!(bomb.on_in(None, 1.0, 20.0, false));
    }

    #[test]
    fn a_burst_gives_its_particles_whatever_the_frame() {
        // On for 0.00001 s at the start, once (an explosion's burst).
        let burst = ParticleSystemDef { active: vec![(0.0, true), (1e-5, false), (5.0, false)], period: 5.0, loops: false, ..def() };
        assert!((burst.on_time(0.0, 0.016) - 1e-5).abs() < 1e-9);
        assert!((burst.on_time(0.0, 0.1) - 1e-5).abs() < 1e-9);
        assert_eq!(burst.on_time(0.016, 0.032), 0.0);
        // The looping keys of `def`: on, off at 0.5 of a period of 1.
        let d = def();
        assert!((d.on_time(0.25, 1.25) - 0.5).abs() < 1e-5);
        assert!((d.on_time(0.0, 2.0) - 1.0).abs() < 1e-5);
        let always = ParticleSystemDef { active: Vec::new(), ..def() };
        assert!((always.on_time(3.0, 3.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn particles_fade_over_a_quarter_of_their_width() {
        let v = |radius: f32, radius_variation: f32| EmitterValues { radius, radius_variation, ..Emitter::blank().values(0.0) };
        // The blood mist and spray of `mpsbloodsplat01.nif`; a particle of no size keeps
        // the shapes' full distance; a huge smoke puff is capped at it.
        assert_eq!(particle_soft_distance(&v(50.0, 10.0)), 30.0);
        assert_eq!(particle_soft_distance(&v(1.25, 0.5)), 1.0);
        assert_eq!(particle_soft_distance(&v(0.0, 0.0)), crate::nif_material::SOFT_DISTANCE);
        assert_eq!(particle_soft_distance(&v(1_000.0, 0.0)), crate::nif_material::SOFT_DISTANCE);
    }

    #[test]
    fn simple_colour_over_a_life() {
        let c = SimpleColor { fade_in: 0.1, fade_out: 0.8, color1_end: 0.2, color2_start: 0.4, color2_end: 0.6, color3_start: 1.0, colors: [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]] };
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert_eq!(c.at(0.15), [1.0, 0.0, 0.0, 1.0]);
        assert!(near(c.at(0.3), [0.5, 0.5, 0.0, 1.0]), "{:?}", c.at(0.3));
        assert_eq!(c.at(0.5), [0.0, 1.0, 0.0, 1.0]);
        // Half way from the second colour to the third, and half faded out.
        let late = c.at(0.9);
        assert!((late[1] - 0.25).abs() < 1e-5 && (late[2] - 0.75).abs() < 1e-5 && (late[3] - 0.5).abs() < 1e-5);
        assert!((c.at(0.05)[3] - 0.5).abs() < 1e-6);
        // All four percentages 0 (the dirt dust): the second colour throughout.
        let dust = SimpleColor { fade_in: 0.0, fade_out: 1.0, color1_end: 0.0, color2_start: 0.0, color2_end: 0.0, color3_start: 0.0, ..c };
        assert_eq!(dust.at(0.0), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(dust.at(0.7), [0.0, 1.0, 0.0, 1.0]);
    }

    /// A unit square in the xy plane, two triangles, normals up.
    fn square() -> EmitterMesh {
        let p = vec![Vec3::ZERO, Vec3::X, Vec3::new(1.0, 1.0, 0.0), Vec3::Y];
        EmitterMesh::new(Affine3A::IDENTITY, p, vec![Vec3::Z; 4], vec![[0, 1, 2], [0, 2, 3]])
    }

    #[test]
    fn a_mesh_emitter_starts_particles_on_its_mesh() {
        let m = square();
        assert_eq!(m.edges.len(), 5);
        let mut seed = 0x1234_5678u32;
        let mut rand = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        for from in [EmitFrom::Vertices, EmitFrom::FaceCenter, EmitFrom::EdgeCenter, EmitFrom::FaceSurface, EmitFrom::EdgeSurface] {
            for _ in 0..50 {
                let (p, n) = m.pick(from, &mut rand).unwrap();
                assert!(p.z.abs() < 1e-6 && (-1e-6..=1.0 + 1e-6).contains(&p.x) && (-1e-6..=1.0 + 1e-6).contains(&p.y), "{from:?} {p}");
                assert!((n - Vec3::Z).length() < 1e-5, "{from:?} {n}");
            }
        }
        // A face centre is a third of the way between its corners.
        let (c, _) = m.pick(EmitFrom::FaceCenter, &mut || 0.0).unwrap();
        assert!((c - Vec3::new(2.0, 1.0, 0.0) / 3.0).length() < 1e-6);
    }

    const I: Affine3A = Affine3A::IDENTITY;

    #[test]
    fn the_bomb_pushes_away_less_with_distance() {
        let b = Bomb { origin: Vec3::ZERO, axis: Vec3::Z, delta_v: 1000.0, decay: 100.0, decay_type: 1, symmetry: 0, active: ModifierActive::default() };
        assert!((b.push(&I, Vec3::new(25.0, 0.0, 0.0)) - Vec3::new(750.0, 0.0, 0.0)).length() < 1e-3);
        assert_eq!(b.push(&I, Vec3::new(0.0, 200.0, 0.0)), Vec3::ZERO);
        // Cylindrical: away from the axis, not along it.
        let c = Bomb { symmetry: 1, ..b.clone() };
        let p = c.push(&I, Vec3::new(0.0, 50.0, 40.0));
        assert!(p.z.abs() < 1e-4 && (p.y - 500.0).abs() < 1e-3, "{p}");
        // Planar: across the plane only.
        let pl = Bomb { symmetry: 2, ..b };
        let p = pl.push(&I, Vec3::new(30.0, 0.0, -50.0));
        assert!(p.x.abs() < 1e-4 && (p.z + 500.0).abs() < 1e-3, "{p}");
    }

    #[test]
    fn a_planar_collider_bounces_or_kills_within_its_rectangle() {
        let c = Collider { origin: Vec3::new(0.0, 0.0, -50.0), x_axis: Vec3::X, y_axis: Vec3::Y, half_width: 100.0, half_height: 100.0, sphere: None, bounce: 0.5, die: false };
        let (at, v) = c.hit(&I, Vec3::new(0.0, 0.0, -40.0), Vec3::new(0.0, 0.0, -60.0), Vec3::new(10.0, 0.0, -200.0)).unwrap().unwrap();
        assert!((at.z + 50.0).abs() < 1e-4 && (v - Vec3::new(10.0, 0.0, 100.0)).length() < 1e-3, "{at} {v}");
        // Outside the rectangle, or not crossing: nothing.
        assert!(c.hit(&I, Vec3::new(300.0, 0.0, -40.0), Vec3::new(300.0, 0.0, -60.0), Vec3::NEG_Z).is_none());
        assert!(c.hit(&I, Vec3::new(0.0, 0.0, -40.0), Vec3::new(0.0, 0.0, -45.0), Vec3::NEG_Z).is_none());
        let kill = Collider { die: true, ..c.clone() };
        assert_eq!(kill.hit(&I, Vec3::new(0.0, 0.0, -40.0), Vec3::new(0.0, 0.0, -60.0), Vec3::NEG_Z), Some(None));
        // Placed upside down by its model's place: the plane is then at z = +50.
        let flipped = Affine3A::from_rotation_x(std::f32::consts::PI);
        assert!(c.hit(&flipped, Vec3::new(0.0, 0.0, 60.0), Vec3::new(0.0, 0.0, 40.0), Vec3::NEG_Z).is_some());
    }

    #[test]
    fn a_spherical_collider_keeps_particles_inside() {
        let c = Collider { origin: Vec3::new(0.0, 0.0, 10.0), x_axis: Vec3::X, y_axis: Vec3::Y, half_width: 0.0, half_height: 0.0, sphere: Some(5.0), bounce: 0.5, die: false };
        // Falling out of the bottom of the sphere: back on its surface, thrown up at half speed.
        let (at, v) = c.hit(&I, Vec3::new(0.0, 0.0, 6.0), Vec3::new(0.0, 0.0, 4.0), Vec3::new(0.0, 0.0, -20.0)).unwrap().unwrap();
        assert!((at.z - 5.0).abs() < 1e-2 && (v.z - 10.0).abs() < 1e-3, "{at} {v}");
        // Moving inside without reaching the surface: nothing; a scaled frame scales it.
        assert!(c.hit(&I, Vec3::new(0.0, 0.0, 9.0), Vec3::new(0.0, 0.0, 7.0), Vec3::NEG_Z).is_none());
        let big = Affine3A::from_scale(Vec3::splat(2.0));
        assert!(c.hit(&big, Vec3::new(0.0, 0.0, 12.0), Vec3::new(0.0, 0.0, 11.0), Vec3::NEG_Z).is_none());
    }

    #[test]
    fn keyed_values_replace_the_fields() {
        let keys = |pts: &[(f32, f32)], stop: f32| FloatKeys {
            channel: Channel::Keys(Keys {
                kind: fnv_nif::anim::KeyKind::Linear,
                keys: pts.iter().map(|&(time, v)| fnv_nif::anim::Key { time, value: [v], forward: [0.0], backward: [0.0], tbc: [0.0; 3] }).collect(),
            }),
            cycle: OwnCycle { start: 0.0, stop, frequency: 1.0, phase: 0.0, cycle_type: 2 },
        };
        // The limb blood spray's rate: 150 a second, up to 210, then none by 3 s.
        let d = ParticleSystemDef { rate_keys: Some(keys(&[(0.0, 150.0), (1.567, 150.0), (1.833, 210.0), (3.0, 0.0)], 4.367)), ..def() };
        assert_eq!(d.rate_at(1.0), 150.0);
        assert!((d.rate_at(1.833) - 210.0).abs() < 1e-3);
        assert_eq!(d.rate_at(9.0), 0.0);
        let mut e = Emitter::blank();
        e.speed = 5.0;
        e.keyed.push((EmitterParam::Speed, keys(&[(0.0, 190.0), (4.0, 6.0)], 4.0)));
        assert!((e.values(2.0).speed - 98.0).abs() < 1e-3);
        assert_eq!(e.values(2.0).radius, 1.0);
    }
}
