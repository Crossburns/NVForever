//! Where actors that scripts create (PlaceAtMe of an NPC, creature or leveled actor list)
//! first stand (docs/design/frame-rate-audit.md §2).
//!
//! PlaceAtMe puts every reference it makes at the caller's own position (`fnv_game`
//! `place_at_me`: the distance and direction do nothing and the object lands near the
//! caller [documented bug, catalogue notes]). For an actor that is a bad place: the
//! Goodsprings schoolhouse's mantis ootheca (`V22MantisOotheca`, script `0x000EF406`)
//! places 3 nymphs in each of four directions at each of its two destruction stages, 24
//! nymphs at the egg's centre, against the lockers [observed: `fnvtool script decompile`,
//! `world refs`]. Their characters started on or inside the egg and the lockers and the
//! character controller ran out its iterations every step for every one of them: about a
//! second a frame (frame-rate-audit.md §2).
//!
//! So a newly created actor is moved, before the engine first takes it up, to the nearest
//! spot around where it was put that [engineering choice; the game's own rule is not
//! known, it "lands near the caller"]:
//! - has a walkable support under it that is not the caller itself, with room on it for
//!   a person's capsule (the player's, as the arrival rule tests it, `player::safe::stand`)
//!   or, when no spot has that, for a small creature's ([`SMALL`]);
//! - is no more than [`MAX_RISE`] above where it was put (on the floor, not on a desk or
//!   a locker beside it);
//! - is in sight of where it was put (a ray at knee height meets nothing but the caller),
//!   so nothing appears behind a wall;
//! - is at least [`SPACING`] from every other created actor and every actor's character,
//!   so they do not start inside each other.
//!
//! The spots are tried on rings ([`RADII`], [`DIRECTIONS`] a ring), nearest first. An actor
//! with no such spot within the last ring stays where it was put. The caller is the
//! reference standing exactly where the actor was put (PlaceAtMe copies its position);
//! there is none when a script placed it at a marker.

use crate::phys::PhysState;
use crate::player::safe;
use fnv_collide::{Iso, Shape};
use fnv_phys::{layers, Filter, Owner};
use fnv_sim::{FormId, RefTable, World};

/// Rings of candidate spots around where the actor was put, game units [engineering
/// choice: the first is the spot itself; the last is a room's width].
pub const RADII: [f32; 10] = [0.0, 32.0, 64.0, 96.0, 128.0, 160.0, 192.0, 256.0, 320.0, 384.0];
/// Candidate spots on a ring.
pub const DIRECTIONS: usize = 16;
/// Least distance between created actors' spots and other actors' characters, game units
/// [engineering choice: wider than a small creature's capsule, under a person's (radius
/// 22)].
pub const SPACING: f32 = 32.0;
/// The capsule (radius, height) tried when no spot has room for a person's: about a
/// mantis nymph's or a radroach's size [engineering choice].
pub const SMALL: (f32, f32) = (12.0, 40.0);
/// The highest a spot's feet may be above where the actor was put, game units
/// [engineering choice: a step, as the character controller climbs].
pub const MAX_RISE: f32 = 24.0;
/// Height above the floor of the sight test between where the actor was put and a spot
/// [engineering choice: above skirting and low clutter].
const SIGHT_HEIGHT: f32 = 24.0;

/// What [`place`] did with one actor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// The spot where it was put was free.
    Kept,
    /// Moved this far to a free spot.
    Moved { from: [f32; 3], to: [f32; 3] },
    /// No free spot: left where it was put.
    NoSpot,
}

/// The caller of a PlaceAtMe that put an actor at `pos` in `cell`: a reference of the cell
/// (placed or created, not `actor`) standing at exactly that position.
fn caller(world: &World, refs: &RefTable, cell: Option<FormId>, pos: [f32; 3], actor: FormId) -> Option<FormId> {
    let at = |id: FormId| world.peek(id).filter(|s| !s.deleted() && dist2(s.pos, pos) < 0.25).map(|_| id);
    let placed = cell.and_then(|c| refs.by_cell.get(&c)).into_iter().flatten().copied();
    let created = world.created().map(|(id, _)| id);
    placed.chain(created).filter(|&id| id != actor).find_map(at)
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

fn dist2_xy(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

/// Whether nothing but `caller` stands between `from` and `to` at [`SIGHT_HEIGHT`].
fn in_sight(phys: &mut PhysState, from: [f32; 3], to: [f32; 3], caller: Option<FormId>) -> bool {
    let a = [from[0], from[1], from[2] + SIGHT_HEIGHT];
    let b = [to[0], to[1], to[2] + SIGHT_HEIGHT];
    let d = dist2(a, b).sqrt();
    if d < 1.0 {
        return true;
    }
    let dir = [(b[0] - a[0]) / d, (b[1] - a[1]) / d, (b[2] - a[2]) / d];
    let mut f = Filter { exclude_layers: layers::character_ignores().union(layers::actor_volumes()), no_actors: true, ..Default::default() };
    if let Some(c) = phys.player.as_ref() {
        f.exclude.push(c.key());
    }
    let mut from_at = a;
    let mut left = d;
    // Through the caller (the egg the actors come out of), up to a few times.
    for _ in 0..4 {
        match phys.world.queries().ray(from_at, dir, left, &f) {
            None => return true,
            Some(hit) if matches!(hit.owner, Owner::Ref { form_id, .. } if Some(FormId(form_id)) == caller) => {
                let step = hit.distance + 1.0;
                from_at = [from_at[0] + dir[0] * step, from_at[1] + dir[1] * step, from_at[2] + dir[2] * step];
                left -= step;
                if left <= 0.0 {
                    return true;
                }
            }
            Some(_) => return false,
        }
    }
    false
}

/// What may support or block a created actor: everything but the layers characters pass
/// through, actors' hit volumes and the player's capsule (other characters block).
fn filter(phys: &PhysState) -> Filter {
    let mut f = Filter { exclude_layers: layers::character_ignores().union(layers::actor_volumes()), ..Default::default() };
    if let Some(c) = &phys.player {
        f.exclude.push(c.key());
    }
    f
}

/// Feet where a capsule `radius` wide and `height` tall stands at `at`: as
/// `player::safe::stand` with the player's capsule, here of any size.
fn stand_small(phys: &mut PhysState, at: [f32; 3], not_on: Option<u32>, (radius, height): (f32, f32)) -> Option<[f32; 3]> {
    let p = safe::params(phys);
    let f = filter(phys);
    let up = height.max(p.height / 2.0);
    let hit = phys.world.queries().ray([at[0], at[1], at[2] + up], [0.0, 0.0, -1.0], up + 4.0 * height, &f)?;
    if matches!(hit.owner, Owner::Ref { form_id, .. } if Some(form_id) == not_on) || hit.normal[2] < p.max_slope_deg.to_radians().cos() {
        return None;
    }
    let feet = [at[0], at[1], hit.point[2] + p.rest_gap(hit.shell)];
    let lift = 2.0 * p.skin;
    let capsule = Shape::Capsule { a: [0.0, 0.0, radius + lift], b: [0.0, 0.0, (height - radius).max(radius + lift)], radius };
    let blocked = phys.world.queries().overlap(&capsule, &Iso { translation: feet, ..Iso::IDENTITY }, &f).map_or(true, |o| !o.is_empty());
    (!blocked).then_some(feet)
}

/// The candidate spots around `pos`, nearest first ([`RADII`], [`DIRECTIONS`]).
pub fn candidates(pos: [f32; 3]) -> Vec<[f32; 3]> {
    let mut out = vec![pos];
    out.extend(safe::ring_spots(pos, &RADII[1..], DIRECTIONS));
    out
}

/// Whether feet at `spot` are low enough for an actor put at `pos` ([`MAX_RISE`]).
pub fn low_enough(spot: [f32; 3], pos: [f32; 3]) -> bool {
    spot[2] <= pos[2] + MAX_RISE
}

/// Whether `spot` keeps [`SPACING`] from every spot in `taken` (feet, horizontally).
pub fn spaced(spot: [f32; 3], taken: &[[f32; 3]]) -> bool {
    taken.iter().all(|t| dist2_xy(spot, *t) >= SPACING * SPACING)
}

/// Move each of `actors` (just created, not yet taken up by the engine) to a free spot
/// near where it was put (module notes). Returns what was done with each.
pub fn place(world: &mut World, refs: &RefTable, phys: &mut PhysState, actors: &[FormId]) -> Vec<(FormId, Placement)> {
    if actors.is_empty() || phys.player.is_none() {
        return Vec::new();
    }
    phys.world.update_queries();
    // Characters, and the actors created before (most stand without a character).
    let mut taken: Vec<[f32; 3]> = phys.npcs.values().map(|b| b.ch.feet).collect();
    let forms = world.forms;
    taken.extend(world.created().filter(|(id, s)| !s.deleted() && !actors.contains(id) && fnv_game::ai::is_actor_base(forms, s.base)).map(|(_, s)| s.pos));
    let mut out = Vec::new();
    for &actor in actors {
        let Some((cell, pos)) = world.peek(actor).map(|s| (s.cell, s.pos)) else { continue };
        if !phys.ready_at(pos) {
            continue;
        }
        let caller = caller(world, refs, cell, pos, actor);
        let not_on = caller.map(|c| c.0);
        let spots = candidates(pos);
        let mut found = spots.iter().find_map(|&at| {
            let feet = safe::stand(phys, at, not_on)?;
            (low_enough(feet, pos) && spaced(feet, &taken) && in_sight(phys, pos, feet, caller)).then_some(feet)
        });
        if found.is_none() {
            found = spots.iter().find_map(|&at| {
                let feet = stand_small(phys, at, not_on, SMALL)?;
                (low_enough(feet, pos) && spaced(feet, &taken) && in_sight(phys, pos, feet, caller)).then_some(feet)
            });
        }
        let placement = match found {
            Some(feet) if dist2(feet, pos) < 1.0 => Placement::Kept,
            Some(feet) => Placement::Moved { from: pos, to: feet },
            None => Placement::NoSpot,
        };
        if let Some(feet) = found {
            taken.push(feet);
            if let Ok(s) = world.get(actor) {
                s.pos = feet;
            }
        }
        out.push((actor, placement));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_start_at_the_spot_and_go_outwards() {
        let c = candidates([100.0, 200.0, 0.0]);
        assert_eq!(c.len(), 1 + (RADII.len() - 1) * DIRECTIONS);
        assert_eq!(c[0], [100.0, 200.0, 0.0]);
        let r = |p: [f32; 3]| dist2_xy(p, [100.0, 200.0, 0.0]).sqrt();
        assert!(c.windows(2).all(|w| r(w[0]) <= r(w[1]) + 1e-3), "nearest first");
        assert!((r(*c.last().unwrap()) - RADII[RADII.len() - 1]).abs() < 1e-3);
    }

    #[test]
    fn spots_stay_near_the_floor() {
        // The ootheca's centre is 10 above the schoolhouse floor; a desk top is 75 above it.
        assert!(low_enough([0.0, 0.0, 3329.0], [0.0, 0.0, 3339.0]));
        assert!(!low_enough([0.0, 0.0, 3404.0], [0.0, 0.0, 3339.0]));
    }

    #[test]
    fn spots_keep_their_distance() {
        let taken = [[0.0, 0.0, 0.0]];
        assert!(!spaced([SPACING - 1.0, 0.0, 0.0], &taken));
        assert!(spaced([SPACING, 0.0, 50.0], &taken), "only the horizontal distance counts");
        assert!(spaced([1.0, 1.0, 0.0], &[]));
    }
}
