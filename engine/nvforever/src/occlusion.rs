//! Occlusion planes (docs/spec-notes/level-design.md §3.2): flat rectangles the level
//! designers put inside opaque walls; what lies wholly in the volume a plane hides from
//! the camera (the plane projected away from the eye) is not drawn, and a portal hidden
//! by a plane is not followed (§3.1). The culling itself is done with the rooms' in
//! `rooms.rs`, which owns the hidden meshes; this module reads the planes and holds the
//! geometry.
//!
//! - **Data** [observed: level-design.md §3.2]: references of base `0x17` (no record)
//!   with `XPRM` type 3, `XOCP` and `XMBO`. The rectangle is read from the reference's
//!   position and rotation (`DATA`) and `XPRM`'s bounds as half extents, its thin axis
//!   left out, as a portal's (`rooms::portal_corners`). `XOCP` repeats them (size equal
//!   to the bounds, position within a few units of `DATA`, rotation an axis and angle
//!   matching `DATA`'s z turn) and is not read; `XORD` links occur only null and are not
//!   read.
//! - **What is tested** [engineering choice, level-design.md §3.2: which references are
//!   multibounds in New Vegas is open]: every placed object's and item's mesh by its own
//!   bounds, in game space and axis-aligned (so never smaller than the mesh). A mesh is
//!   hidden only when all eight corners of its bounds are in one plane's volume; the
//!   volume is convex, so the whole mesh is then behind that plane. Planes are not joined:
//!   a mesh seen past the edge between two planes stays drawn.
//! - **The eye on the plane** (closer than [`EYE_MIN`]): the plane hides nothing.
//! - **Interiors only by default** [engineering choice]: the exterior planes are not all
//!   inside their walls. At Primm, plane 0x00177B3D (z 5,776–6,256) lies in a retaining
//!   wall whose mesh reaches z 6,168, so the plane stands 88 units above it and hid the
//!   letters of a roof sign seen over it [observed, 2026-11-06 screenshots with and
//!   without]. The original tests only multibounds against planes, which would not hide
//!   that sign unless it were in one; per-mesh tests are not safe there. In the interiors
//!   checked the planes fit their walls (RocketLabMid's tile one room's walls exactly,
//!   level-design.md §3.2) and the pictures were the same. `--occlusion-planes-outdoors`
//!   uses the exterior planes too, for measuring.
//!
//! `--no-occlusion-planes` turns it off everywhere; `NVF_PLAY_TEST_OCCLUSION_CYCLE=<seconds>` turns
//! it off and on again every that many seconds of real time, to compare frame times in
//! one run.

use crate::world::WorldData;
use bevy::prelude::*;
use fnv_world::fields::{num, winner, Fields};
use fnv_world::placed::placement;
use fnv_world::transform::ref_rotation;

/// The base form of occlusion planes [observed: level-design.md §3.2].
pub const OCCLUSION_PLANE: u32 = 0x17;
/// An eye closer to a plane than this (game units) is taken as on it: the plane hides
/// nothing [engineering choice].
pub const EYE_MIN: f32 = 1.0;

/// An occlusion plane: its corners (game space, in order round the rectangle).
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    pub id: u32,
    pub center: Vec3,
    pub corners: [Vec3; 4],
    pub normal: Vec3,
}

impl Plane {
    /// The plane of a primitive at `center`, turned by `rot`, with half extents `half`
    /// (the thin axis left out, as a portal's).
    pub fn new(id: u32, center: Vec3, rot: Mat3, half: Vec3) -> Plane {
        let c = crate::rooms::portal_corners(center, rot, half);
        let corners = [c[0], c[1], c[2], c[3]];
        let normal = (corners[1] - corners[0]).cross(corners[3] - corners[0]).normalize_or_zero();
        Plane { id, center, corners, normal }
    }

    /// The volume this plane hides from `eye`; `None` when the eye is on the plane or the
    /// plane has no area.
    pub fn shadow_from(&self, eye: Vec3) -> Option<Shadow> {
        if self.normal == Vec3::ZERO {
            return None;
        }
        let s = self.normal.dot(eye - self.center);
        if s.abs() < EYE_MIN {
            return None;
        }
        // The far side: away from the eye.
        let far = if s > 0.0 { -self.normal } else { self.normal };
        let mut sides = [(Vec3::ZERO, 0.0); 4];
        for (i, side) in sides.iter_mut().enumerate() {
            let a = self.corners[i] - eye;
            let b = self.corners[(i + 1) % 4] - eye;
            let mut m = a.cross(b);
            if m.dot(self.center - eye) < 0.0 {
                m = -m;
            }
            *side = (m, m.dot(eye));
        }
        Some(Shadow { far: (far, far.dot(self.center)), sides })
    }
}

/// The volume a plane hides from one eye: beyond the plane and inside the four planes
/// through the eye and the rectangle's edges. Each half-space is `n · p > d`.
#[derive(Debug, Clone, Copy)]
pub struct Shadow {
    far: (Vec3, f32),
    sides: [(Vec3, f32); 4],
}

impl Shadow {
    /// Whether `p` is inside (strictly).
    pub fn hides_point(&self, p: Vec3) -> bool {
        self.far.0.dot(p) > self.far.1 && self.sides.iter().all(|(n, d)| n.dot(p) > *d)
    }

    /// Whether every point is inside, so what they bound is (the volume is convex).
    pub fn hides_points(&self, ps: &[Vec3]) -> bool {
        !ps.is_empty() && ps.iter().all(|p| self.hides_point(*p))
    }

    /// Whether the world box `min`–`max` (game space) is wholly inside.
    pub fn hides_box(&self, min: Vec3, max: Vec3) -> bool {
        (0..8).all(|i| {
            let p = Vec3::new(if i & 1 == 0 { min.x } else { max.x }, if i & 2 == 0 { min.y } else { max.y }, if i & 4 == 0 { min.z } else { max.z });
            self.hides_point(p)
        })
    }
}

/// The occlusion planes among the references `refs` (module notes).
pub fn read(world: &WorldData, refs: impl IntoIterator<Item = u32>) -> Vec<Plane> {
    let mut planes = Vec::new();
    for r in refs {
        let Some((pi, rec)) = winner(world.lo, r) else { continue };
        let Ok(f) = Fields::new(rec, pi, world.schema) else { continue };
        if f.formid(b"NAME", world.lo).map(|b| b & 0x00FF_FFFF) != Some(OCCLUSION_PLANE) {
            continue;
        }
        let Some(bounds) = f.get(b"XPRM").and_then(|v| v.get("bounds").cloned()) else { continue };
        let Some(half) = (|| Some(Vec3::new(num(&bounds, "x")? as f32, num(&bounds, "y")? as f32, num(&bounds, "z")? as f32).abs()))() else { continue };
        let Some(p) = placement(world.lo, world.schema, r) else { continue };
        let rot = Mat3::from_cols_array(&ref_rotation(p.rotation)).transpose();
        planes.push(Plane::new(r, Vec3::from_array(p.position), rot, half));
    }
    planes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wall plane at x = 100, 200 wide (y) and 100 high (z), facing x.
    fn wall() -> Plane {
        Plane::new(1, Vec3::new(100.0, 0.0, 0.0), Mat3::IDENTITY, Vec3::new(0.5, 100.0, 50.0))
    }

    #[test]
    fn a_plane_hides_what_lies_wholly_behind_it() {
        let s = wall().shadow_from(Vec3::ZERO).unwrap();
        // Behind the wall, within its shadow (which widens with distance).
        assert!(s.hides_box(Vec3::new(150.0, -50.0, -20.0), Vec3::new(200.0, 50.0, 20.0)));
        assert!(s.hides_box(Vec3::new(400.0, 250.0, -20.0), Vec3::new(420.0, 300.0, 20.0)), "far behind, off to the side but still in the widening shadow");
        // In front of the wall, across it, or past its edge: drawn.
        assert!(!s.hides_box(Vec3::new(50.0, -10.0, -10.0), Vec3::new(60.0, 10.0, 10.0)));
        assert!(!s.hides_box(Vec3::new(90.0, -10.0, -10.0), Vec3::new(110.0, 10.0, 10.0)));
        assert!(!s.hides_box(Vec3::new(150.0, 90.0, -10.0), Vec3::new(160.0, 200.0, 10.0)), "sticks out past the edge");
        assert!(!s.hides_box(Vec3::new(150.0, -10.0, 40.0), Vec3::new(160.0, 10.0, 90.0)), "sticks out over the top");
        // Behind the eye: drawn.
        assert!(!s.hides_box(Vec3::new(-200.0, -10.0, -10.0), Vec3::new(-150.0, 10.0, 10.0)));
    }

    #[test]
    fn a_plane_works_from_either_side_and_not_from_on_it() {
        let p = wall();
        let s = p.shadow_from(Vec3::new(300.0, 0.0, 0.0)).unwrap();
        assert!(s.hides_box(Vec3::new(20.0, -10.0, -10.0), Vec3::new(50.0, 10.0, 10.0)));
        assert!(!s.hides_box(Vec3::new(150.0, -10.0, -10.0), Vec3::new(200.0, 10.0, 10.0)));
        assert!(p.shadow_from(Vec3::new(100.2, 0.0, 0.0)).is_none());
    }

    #[test]
    fn a_turned_plane_follows_its_rotation() {
        // Turned a quarter about z: the wall lies along x at y = 100.
        let p = Plane::new(2, Vec3::new(0.0, 100.0, 0.0), Mat3::from_rotation_z(std::f32::consts::FRAC_PI_2), Vec3::new(0.5, 100.0, 50.0));
        let s = p.shadow_from(Vec3::ZERO).unwrap();
        assert!(s.hides_box(Vec3::new(-20.0, 150.0, -10.0), Vec3::new(20.0, 180.0, 10.0)));
        assert!(!s.hides_box(Vec3::new(150.0, -10.0, -10.0), Vec3::new(200.0, 10.0, 10.0)));
    }
}
