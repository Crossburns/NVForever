//! What the crosshair points at: the collision ray's hit, or a reference's render bounds
//! when the ray finds no reference (pure, testable without Bevy's app).
//!
//! The rule [hypothesis, docs/spec-notes/simulation.md §5 "Crosshair targeting"]: the
//! target is the activatable reference nearest along the view within [`REACH`] of the eye.
//! Candidates are the reference whose collision the ray hits first, and references whose
//! render bounds (a mesh's box in its own space, i.e. an oriented box in the world) the
//! ray enters before the first collision hit of any kind (walls, floors and tables hide
//! what is behind them). Many small items have no collision, so the bounds are what make
//! them pickable. A box the eye is inside does not count.

use bevy::math::{Affine3A, Vec3};

/// How far from the eye the player can activate, game units [hypothesis: a placeholder of
/// about two metres at ~70 units per metre; no game setting in the masters names an
/// activation distance (simulation.md §5). In game the setting `iActivatePickLength`
/// reads 150 [observed: capture mv_trial, physics.md §6.3]; that it is this distance is
/// [documented: GECK wiki GetCrosshairRef] but unmeasured (job mv_values)].
pub const REACH: f32 = 150.0;

/// The first collision hit along the view (any owner), from `fnv_phys`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// The placed reference hit, when the collider belongs to one.
    pub form_id: Option<u32>,
    pub distance: f32,
}

/// How far behind the surface the ray first hits a door may stand and still be the
/// collision hit, game units. Some buildings' collision has a face a few units in front
/// of their door: the Nipton town hall's mesh lies 8.5 units before its door's slab
/// (0x00130CFF), so the door was never targeted (#1339) [observed 2026-11-01; design:
/// the reach].
pub const DOOR_SLACK: f32 = 16.0;

/// The collision hit the target is chosen from: the first hit, or a door found within
/// [`DOOR_SLACK`] behind it when the first hit is not something the player can
/// activate (`activatable`) and the door is.
pub fn door_behind(first: Option<RayHit>, behind: Option<RayHit>, mut activatable: impl FnMut(u32) -> bool) -> Option<RayHit> {
    match (first, behind) {
        (Some(f), Some(b)) if b.distance <= f.distance + DOOR_SLACK && !f.form_id.is_some_and(&mut activatable) && b.form_id.is_some_and(&mut activatable) => Some(b),
        (f, _) => f,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Collision,
    Bounds,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    pub form_id: u32,
    /// Distance from the eye along the view, game units.
    pub distance: f32,
    pub source: Source,
}

/// Choose the target from the collision hit and the render-bounds hits (reference,
/// entry distance). `accept` says whether a reference can be activated now (present in
/// the simulation, of an activatable type); it is asked only for candidates that would
/// win. Ties go to the collision hit.
pub fn choose(
    hit: Option<RayHit>,
    bounds: impl IntoIterator<Item = (u32, f32)>,
    reach: f32,
    mut accept: impl FnMut(u32) -> bool,
) -> Option<Pick> {
    let limit = hit.map_or(reach, |h| h.distance.min(reach));
    let mut best = hit
        .and_then(|h| Some((h.form_id?, h.distance)))
        .filter(|&(id, d)| d <= reach && accept(id))
        .map(|(form_id, distance)| Pick { form_id, distance, source: Source::Collision });
    for (id, d) in bounds {
        if !(0.0..=limit).contains(&d) {
            continue;
        }
        match best {
            Some(b) if b.distance <= d => {}
            Some(b) if b.form_id == id => best = Some(Pick { distance: d, ..b }),
            _ if accept(id) => best = Some(Pick { form_id: id, distance: d, source: Source::Bounds }),
            _ => {}
        }
    }
    best
}

/// Distance along a ray (`origin`, unit `dir`, world space) at which it enters a box
/// given in an entity's own space (`center`, `half` extents) placed by `to_world`. `None`
/// when the ray misses, the box is behind, or `origin` is inside the box.
pub fn ray_box(origin: Vec3, dir: Vec3, to_world: &Affine3A, center: Vec3, half: Vec3) -> Option<f32> {
    if to_world.matrix3.determinant().abs() < 1e-12 {
        return None;
    }
    let inv = to_world.inverse();
    // An affine map keeps the ray's parameter: t in local space is t in world space.
    let o = inv.transform_point3(origin) - center;
    let d = inv.transform_vector3(dir);
    let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
    for i in 0..3 {
        if d[i].abs() < 1e-12 {
            if o[i].abs() > half[i] {
                return None;
            }
            continue;
        }
        let a = (-half[i] - o[i]) / d[i];
        let b = (half[i] - o[i]) / d[i];
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    (t0 <= t1 && t0 >= 0.0).then_some(t0)
}

/// A sphere around the placed box (world centre, radius), for a cheap range test first.
pub fn bounding_sphere(to_world: &Affine3A, center: Vec3, half: Vec3) -> (Vec3, f32) {
    let m = to_world.matrix3;
    let scale = m.x_axis.length().max(m.y_axis.length()).max(m.z_axis.length());
    (to_world.transform_point3(center), half.length() * scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::{Quat, Vec3};

    const ANY: fn(u32) -> bool = |_| true;

    #[test]
    fn collision_hit_on_an_activatable_reference_wins() {
        let p = choose(Some(RayHit { form_id: Some(7), distance: 90.0 }), [], REACH, ANY).unwrap();
        assert_eq!(p, Pick { form_id: 7, distance: 90.0, source: Source::Collision });
    }

    #[test]
    fn beyond_reach_or_not_activatable_is_no_target() {
        assert!(choose(Some(RayHit { form_id: Some(7), distance: 151.0 }), [], REACH, ANY).is_none());
        assert!(choose(Some(RayHit { form_id: Some(7), distance: 50.0 }), [], REACH, |id| id != 7).is_none());
        assert!(choose(Some(RayHit { form_id: None, distance: 50.0 }), [], REACH, ANY).is_none());
        assert!(choose(None, [(3, 151.0)], REACH, ANY).is_none());
    }

    #[test]
    fn bounds_find_items_without_collision() {
        // Nothing collides (the item has no collision; nothing behind within reach).
        let p = choose(None, [(3, 120.0), (4, 80.0), (5, 100.0)], REACH, ANY).unwrap();
        assert_eq!(p, Pick { form_id: 4, distance: 80.0, source: Source::Bounds });
        // An item on a table: the ray enters its box before it hits the table's top.
        let table = RayHit { form_id: Some(100), distance: 95.0 };
        let p = choose(Some(table), [(4, 90.0)], REACH, |id| id != 100).unwrap();
        assert_eq!((p.form_id, p.source), (4, Source::Bounds));
    }

    #[test]
    fn walls_hide_what_is_behind_them() {
        let wall = RayHit { form_id: None, distance: 60.0 };
        assert!(choose(Some(wall), [(4, 80.0)], REACH, ANY).is_none());
        let wall = RayHit { form_id: Some(100), distance: 60.0 };
        assert!(choose(Some(wall), [(4, 80.0)], REACH, |id| id != 100).is_none());
    }

    #[test]
    fn nearest_wins_and_ties_go_to_collision() {
        // An item in front of a door: the item is nearer.
        let door = RayHit { form_id: Some(9), distance: 120.0 };
        assert_eq!(choose(Some(door), [(4, 70.0)], REACH, ANY).unwrap().form_id, 4);
        // The door's own render box at the same distance keeps the collision source.
        let p = choose(Some(door), [(9, 120.0), (9, 119.0)], REACH, ANY).unwrap();
        assert_eq!((p.form_id, p.source), (9, Source::Collision));
        assert_eq!(p.distance, 119.0);
        // Rejected candidates are skipped, not chosen.
        assert_eq!(choose(None, [(1, 10.0), (2, 20.0)], REACH, |id| id == 2).unwrap().form_id, 2);
    }

    #[test]
    fn ray_enters_a_placed_box() {
        // A 10-unit cube scaled ×2 and turned 45° about the up axis, 100 units ahead.
        let t = Affine3A::from_scale_rotation_translation(Vec3::splat(2.0), Quat::from_rotation_y(std::f32::consts::FRAC_PI_4), Vec3::new(0.0, 0.0, -100.0));
        let half = Vec3::splat(5.0);
        let d = ray_box(Vec3::ZERO, Vec3::NEG_Z, &t, Vec3::ZERO, half).unwrap();
        // Corner first: 10 · √2 from the centre after scaling.
        assert!((d - (100.0 - 10.0 * 2f32.sqrt())).abs() < 1e-3, "{d}");
        // Off to the side, behind, and from inside.
        assert!(ray_box(Vec3::new(30.0, 0.0, 0.0), Vec3::NEG_Z, &t, Vec3::ZERO, half).is_none());
        assert!(ray_box(Vec3::ZERO, Vec3::Z, &t, Vec3::ZERO, half).is_none());
        assert!(ray_box(Vec3::new(0.0, 0.0, -100.0), Vec3::NEG_Z, &t, Vec3::ZERO, half).is_none());
        // An off-centre box in the entity's space.
        let d = ray_box(Vec3::new(0.0, 3.0, 0.0), Vec3::NEG_Z, &Affine3A::IDENTITY, Vec3::new(0.0, 3.0, -50.0), Vec3::ONE).unwrap();
        assert!((d - 49.0).abs() < 1e-4);
        // A flat box (a sheet of paper) is still hit face-on.
        let d = ray_box(Vec3::ZERO, Vec3::NEG_Z, &Affine3A::IDENTITY, Vec3::new(0.0, 0.0, -20.0), Vec3::new(1.0, 1.0, 0.0)).unwrap();
        assert!((d - 20.0).abs() < 1e-4);
        // Degenerate transforms are ignored.
        assert!(ray_box(Vec3::ZERO, Vec3::NEG_Z, &Affine3A::from_scale(Vec3::ZERO), Vec3::ZERO, half).is_none());
    }

    #[test]
    fn sphere_covers_the_box() {
        let t = Affine3A::from_scale_rotation_translation(Vec3::new(1.0, 3.0, 1.0), Quat::IDENTITY, Vec3::X * 10.0);
        let (c, r) = bounding_sphere(&t, Vec3::Y, Vec3::ONE);
        assert_eq!(c, Vec3::new(10.0, 3.0, 0.0));
        assert!((r - 3.0 * 3f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn a_door_just_behind_a_wall_face_is_the_hit() {
        let (wall, door, crate_) = (1, 2, 3);
        let at = |id: u32, distance: f32| Some(RayHit { form_id: Some(id), distance });
        let usable = |id: u32| id != wall;
        // The Nipton town hall: its mesh at 89.2, the door 8.5 behind it.
        assert_eq!(door_behind(at(wall, 89.2), at(door, 97.7), usable), at(door, 97.7));
        // Too far behind, or the first hit is usable itself: the first hit stands.
        assert_eq!(door_behind(at(wall, 50.0), at(door, 50.0 + DOOR_SLACK + 1.0), usable), at(wall, 50.0));
        assert_eq!(door_behind(at(crate_, 40.0), at(door, 45.0), usable), at(crate_, 40.0));
        // Ground (no reference) counts as not usable; nothing behind leaves it.
        assert_eq!(door_behind(Some(RayHit { form_id: None, distance: 30.0 }), at(door, 35.0), usable), at(door, 35.0));
        assert_eq!(door_behind(at(wall, 89.2), None, usable), at(wall, 89.2));
    }
}
