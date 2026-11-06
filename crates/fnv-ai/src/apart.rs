//! Actors kept apart from each other [design; owner 2026-10-15: NPCs walked through each
//! other and two would take the same spot and end up inside each other].
//!
//! Where the character controller walks an actor it stops at the others' blocks
//! (`nvf_play::blocking`), but the navmesh walker ([`fnv_nav::Walker`]) meets nobody: it
//! walks the last stretch to every place, everything away from the player, and an actor
//! the controller gave up on. So after each update ([`Ai::keep_apart`]) two living actors
//! standing or walking closer than [`RADIUS`] each are moved apart, at no more than
//! [`SPEED`], along the navmesh: each goes half the way, or all of it when the other may
//! not be moved (walked by physics, rooted). Actors on furniture are left out: seats
//! stand close together. An actor near the end of its path that touches another counts
//! as arrived ([`ARRIVE_BESIDE`]) and stands beside it, instead of walking on the spot
//! towards a place that is taken.
//!
//! How the original keeps actors apart is not captured [hypothesis: a character
//! controller per actor, physics.md §6.8]; nothing here depends on it.

use crate::carry::{Activity, Ai};
use fnv_game::game::Game;
use fnv_nav::{NavGraph, Space};
use fnv_sim::FormId;
use std::collections::HashSet;

/// An actor's radius for keeping apart, game units [design: about the player's 22 less a
/// little, so people standing in a queue or talking may come close].
pub const RADIUS: f32 = 18.0;
/// The most an actor is moved aside, game units per second [design: about a walk].
pub const SPEED: f32 = 80.0;
/// An actor this close to the end of its path (beyond touching) that touches another
/// counts as arrived, game units [design].
pub const ARRIVE_BESIDE: f32 = 2.0 * RADIUS + 16.0;

/// One actor as the rule sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub id: u32,
    pub space: Space,
    pub feet: [f32; 3],
    /// May be moved here (else only others are moved away from it).
    pub movable: bool,
}

/// The horizontal move of each body (same order) that takes overlapping bodies apart in
/// a step of `dt` seconds, and for each body whether it touched another.
pub fn separate(bodies: &[Body], dt: f32) -> (Vec<[f32; 2]>, Vec<bool>) {
    let mut moves = vec![[0.0f32; 2]; bodies.len()];
    let mut touched = vec![false; bodies.len()];
    let most = SPEED * dt;
    for i in 0..bodies.len() {
        for j in i + 1..bodies.len() {
            let (a, b) = (&bodies[i], &bodies[j]);
            // Different floors of one space are not in each other's way.
            if a.space != b.space || (a.feet[2] - b.feet[2]).abs() > 100.0 {
                continue;
            }
            let d = [a.feet[0] - b.feet[0], a.feet[1] - b.feet[1]];
            let dist = d[0].hypot(d[1]);
            let overlap = 2.0 * RADIUS - dist;
            if overlap <= 0.0 {
                continue;
            }
            touched[i] = true;
            touched[j] = true;
            let (share_a, share_b) = match (a.movable, b.movable) {
                (true, true) => (0.5, 0.5),
                (true, false) => (1.0, 0.0),
                (false, true) => (0.0, 1.0),
                (false, false) => continue,
            };
            // Standing on the same point: a way apart that depends on who they are only.
            let n = if dist > 1e-3 {
                [d[0] / dist, d[1] / dist]
            } else {
                let angle = ((a.id ^ b.id) % 360) as f32 * std::f32::consts::PI / 180.0;
                [angle.cos(), angle.sin()]
            };
            let (ga, gb) = ((overlap * share_a).min(most), (overlap * share_b).min(most));
            moves[i] = [moves[i][0] + n[0] * ga, moves[i][1] + n[1] * ga];
            moves[j] = [moves[j][0] - n[0] * gb, moves[j][1] - n[1] * gb];
        }
    }
    // Several pushes on one body add up to no more than a step.
    for m in &mut moves {
        let len = m[0].hypot(m[1]);
        if len > most {
            *m = [m[0] / len * most, m[1] / len * most];
        }
    }
    (moves, touched)
}

/// Separation passes ([`Ai::keep_apart`]) since [`probe::enable`], for attributing their
/// cost (diagnostics only: never read by the AI; a reader takes differences between two
/// reads). Nothing is counted until it is enabled.
pub mod probe {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

    static ON: AtomicBool = AtomicBool::new(false);
    static PASSES: AtomicU64 = AtomicU64::new(0);
    static BODIES: AtomicU64 = AtomicU64::new(0);
    static PAIRS: AtomicU64 = AtomicU64::new(0);

    /// A pass over `n` bodies: `separate` looks at every pair of them.
    pub(crate) fn separated(n: usize) {
        if !ON.load(Relaxed) {
            return;
        }
        let n = n as u64;
        PASSES.fetch_add(1, Relaxed);
        BODIES.fetch_add(n, Relaxed);
        PAIRS.fetch_add(n * n.saturating_sub(1) / 2, Relaxed);
    }

    /// Count from now on (the engine's `NVF_PLAY_TEST_PROFILE_COUNTS`).
    pub fn enable() {
        ON.store(true, Relaxed);
    }

    /// (passes, bodies, pairs) so far.
    pub fn separations() -> (u64, u64, u64) {
        (PASSES.load(Relaxed), BODIES.load(Relaxed), PAIRS.load(Relaxed))
    }
}

impl Ai {
    /// Move apart the actors that stand or walk inside each other (see the module).
    /// `fixed`: actors whose position another system decides this frame (the character
    /// controller's, those on or getting off furniture); they are kept clear of but not
    /// moved. Returns how many actors were moved.
    pub fn keep_apart(&mut self, game: &Game, nav: &NavGraph, dt: f32, fixed: &HashSet<u32>) -> usize {
        if !self.cfg.keep_apart || dt <= 0.0 {
            return 0;
        }
        let bodies: Vec<Body> = self
            .agents
            .iter()
            .filter(|(id, a)| a.using.is_none() && game.present(**id) && !game.extra.get(*id).is_some_and(|e| e.dead))
            .map(|(id, a)| Body { id: id.0, space: a.walker.space, feet: a.walker.feet, movable: !a.rooted && !fixed.contains(&id.0) })
            .collect();
        probe::separated(bodies.len());
        let (moves, touched) = separate(&bodies, dt);
        let mut moved = 0;
        for ((b, m), t) in bodies.iter().zip(moves).zip(touched) {
            let a = self.agents.get_mut(&FormId(b.id)).expect("listed");
            if t && a.activity == Activity::Travelling {
                // Near the end of its path and touching someone: arrived, beside them.
                if let Some(f) = a.walker.follower.as_mut() {
                    let last = f.path.legs.len().checked_sub(1);
                    if let Some(leg) = last.filter(|&l| f.leg == l && f.path.legs[l].door.is_none()).map(|l| &f.path.legs[l]) {
                        let end = leg.points.last().copied().unwrap_or(b.feet);
                        if (end[0] - b.feet[0]).hypot(end[1] - b.feet[1]) < ARRIVE_BESIDE {
                            f.next = leg.points.len();
                        }
                    }
                }
            }
            if m == [0.0, 0.0] {
                continue;
            }
            let to = [b.feet[0] + m[0], b.feet[1] + m[1], b.feet[2]];
            // Only onto the navmesh: a wall or a ledge keeps the actor where it is.
            if let Some(tri) = nav.locate(b.space, to, 64.0) {
                a.walker.feet = [to[0], to[1], nav.height_on(tri, to[0], to[1])];
                a.walker.tri = tri;
                moved += 1;
            }
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;
    const HERE: Space = Space::Interior(1);

    fn body(id: u32, x: f32, y: f32, movable: bool) -> Body {
        Body { id, space: HERE, feet: [x, y, 0.0], movable }
    }

    /// Run the rule for `steps` steps; the bodies' positions after.
    fn run(mut bodies: Vec<Body>, steps: usize) -> Vec<Body> {
        for _ in 0..steps {
            let (moves, _) = separate(&bodies, DT);
            for (b, m) in bodies.iter_mut().zip(moves) {
                b.feet = [b.feet[0] + m[0], b.feet[1] + m[1], b.feet[2]];
            }
        }
        bodies
    }

    fn gap(a: &Body, b: &Body) -> f32 {
        (a.feet[0] - b.feet[0]).hypot(a.feet[1] - b.feet[1])
    }

    #[test]
    fn two_on_one_spot_end_up_touching_side_by_side() {
        let b = run(vec![body(0x10, 0.0, 0.0, true), body(0x11, 0.0, 0.0, true)], 60);
        assert!((gap(&b[0], &b[1]) - 2.0 * RADIUS).abs() < 0.5, "{}", gap(&b[0], &b[1]));
        // Each went half the way.
        assert!((b[0].feet[0].hypot(b[0].feet[1]) - RADIUS).abs() < 0.5);
    }

    #[test]
    fn apart_already_nothing_moves() {
        let (moves, touched) = separate(&[body(1, 0.0, 0.0, true), body(2, 2.0 * RADIUS + 1.0, 0.0, true)], DT);
        assert_eq!(moves, vec![[0.0, 0.0]; 2]);
        assert_eq!(touched, vec![false; 2]);
    }

    #[test]
    fn one_that_may_not_move_pushes_the_other_all_the_way() {
        let b = run(vec![body(1, 0.0, 0.0, false), body(2, 10.0, 0.0, true)], 60);
        assert_eq!(b[0].feet, [0.0, 0.0, 0.0]);
        assert!((b[1].feet[0] - 2.0 * RADIUS).abs() < 0.5 && b[1].feet[1].abs() < 1e-4);
    }

    #[test]
    fn moved_aside_at_a_walk_not_at_once() {
        let (moves, _) = separate(&[body(1, 0.0, 0.0, true), body(2, 0.0, 0.0, true)], DT);
        for m in moves {
            assert!(m[0].hypot(m[1]) <= SPEED * DT + 1e-4);
        }
    }

    #[test]
    fn other_floors_and_spaces_are_not_in_the_way() {
        let mut up = body(2, 0.0, 0.0, true);
        up.feet[2] = 200.0;
        let mut other = body(3, 0.0, 0.0, true);
        other.space = Space::Interior(2);
        let (moves, touched) = separate(&[body(1, 0.0, 0.0, true), up, other], DT);
        assert_eq!(moves, vec![[0.0, 0.0]; 3]);
        assert_eq!(touched, vec![false; 3]);
    }

    #[test]
    fn a_crowd_spreads_out() {
        let crowd: Vec<Body> = (0..5).map(|i| body(0x20 + i, i as f32, 0.5 * i as f32, true)).collect();
        let b = run(crowd, 600);
        for i in 0..b.len() {
            for j in i + 1..b.len() {
                assert!(gap(&b[i], &b[j]) > 2.0 * RADIUS - 1.0, "{i} {j}: {}", gap(&b[i], &b[j]));
            }
        }
    }
}
