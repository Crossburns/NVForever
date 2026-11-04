//! One melee blow's knockback on a resident, from its start to its report, tied to that
//! blow (docs/spec-notes/combat-rules.md, "Knockback"; #1386). The measure is net ground
//! travel from where the feet were when this blow began [design]: `moved` and `walked` say
//! when a script's MoveTo, a walk or an attack clip's travel went into it, and `alive` when
//! the actor died or fell out cold on the way.

use nvf_play::combat::Knockback;
use std::collections::HashMap;

pub(super) struct KnockRun {
    pub k: Knockback,
    /// Seconds left.
    pub left: f32,
    /// Where the feet were when this blow began.
    pub from: [f32; 3],
    /// A script moved the actor during it.
    pub moved: bool,
    /// The actor walked, or an attack clip carried it, during it.
    pub walked: bool,
    /// What the frames asked for: how many, their time and the ground travel in all.
    frames: u32,
    secs: f32,
    travel: f32,
    /// `PhysState::steps` when it began.
    steps_from: u64,
}

impl KnockRun {
    /// The travel this frame owes, `dt` seconds of it (game units).
    pub fn advance(&mut self, dt: f32, walked: bool) -> f32 {
        let d = self.k.step(self.left, dt);
        self.left -= dt;
        self.walked |= walked;
        self.frames += 1;
        self.secs += dt;
        self.travel += d;
        d
    }

    /// The fields that tie a log line to the blow.
    pub fn tag(k: &Knockback) -> String {
        format!("blow={} attacker={:08X} target={:08X} power={}", k.blow, k.attacker, k.form_id, u8::from(k.power))
    }

    /// The line a replaced knockback ends with: it did not complete, so no distance is
    /// reported as if it had.
    pub fn interrupted(&self, by: &Knockback, feet: [f32; 3]) -> String {
        format!(
            "knockback: {} interrupted by blow={} with {:.3} s left, {:.3} units from where it began",
            Self::tag(&self.k),
            by.blow,
            self.left.max(0.0),
            ground_distance(self.from, feet)
        )
    }

    /// The line a knockback ends with, unrounded enough to compare frame rates: the feet's
    /// net ground travel against the distance asked, and what the frames and the physics
    /// steps did meanwhile.
    pub fn report(&self, to: [f32; 3], alive: bool, steps_now: u64) -> String {
        let (asked, got) = (self.k.distance(), ground_distance(self.from, to));
        format!(
            "knockback: {} alive={} moved={} walked={} pushed back {got:.3} units of {asked:.3} ({:.2} %), from ({:.3}, {:.3}, {:.3}) to ({:.3}, {:.3}, {:.3}), frames={} secs={:.4} travel={:.3} steps={}",
            Self::tag(&self.k),
            u8::from(alive),
            u8::from(self.moved),
            u8::from(self.walked),
            got / asked.max(1e-3) * 100.0,
            self.from[0],
            self.from[1],
            self.from[2],
            to[0],
            to[1],
            to[2],
            self.frames,
            self.secs,
            self.travel,
            steps_now.saturating_sub(self.steps_from)
        )
    }
}

fn ground_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// Begin `k` with the feet where they are now. One still going on that actor is replaced
/// [design], and handed back: the new blow's distance is measured from here, not from where
/// the first began, which would divide several blows' travel by the last one's request.
pub(super) fn begin(runs: &mut HashMap<u32, KnockRun>, k: Knockback, feet: [f32; 3], steps: u64) -> Option<KnockRun> {
    let run = KnockRun { k, left: k.secs, from: feet, moved: false, walked: false, frames: 0, secs: 0.0, travel: 0.0, steps_from: steps };
    runs.insert(k.form_id, run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_sim::FormId;

    fn blow(id: u32, force: f32) -> Knockback {
        Knockback::of(7, [0.0, 1.0, 0.0], force, 1.0).unwrap().by(FormId::PLAYER_REF, id, true)
    }

    #[test]
    fn a_replacing_blow_measures_from_where_the_feet_are_now() {
        let mut runs = HashMap::new();
        assert!(begin(&mut runs, blow(1, 100.0), [0.0, 0.0, 0.0], 10).is_none());
        runs.get_mut(&7).unwrap().advance(0.5, false);
        // Pushed 30 units by the first, the second begins.
        let old = begin(&mut runs, blow(2, 34.0), [0.0, 30.0, 0.0], 40).expect("the first is handed back");
        assert_eq!((old.k.blow, old.left), (1, 0.5));
        let run = &runs[&7];
        assert_eq!((run.k.blow, run.from, run.left), (2, [0.0, 30.0, 0.0], 1.0));
        // 17 more against the 17 it asked: 100 %, not 47 units of 17.
        let line = run.report([0.0, 47.0, 0.0], true, 100);
        assert!(line.contains("blow=2 attacker=00000014 target=00000007 power=1 alive=1 moved=0 walked=0 pushed back 17.000 units of 17.000 (100.00 %)"), "{line}");
        assert!(line.ends_with("frames=0 secs=0.0000 travel=0.000 steps=60"), "{line}");
    }

    #[test]
    fn the_replaced_blow_is_told_as_interrupted_not_as_pushed_back() {
        let mut runs = HashMap::new();
        begin(&mut runs, blow(1, 100.0), [0.0, 0.0, 0.0], 0);
        let by = blow(2, 34.0);
        let old = begin(&mut runs, by, [0.0, 30.0, 0.0], 0).unwrap();
        let line = old.interrupted(&by, [0.0, 30.0, 0.0]);
        assert!(line.contains("blow=1 attacker=00000014 target=00000007 power=1 interrupted by blow=2"), "{line}");
        assert!(!line.contains("pushed back"), "{line}");
    }

    #[test]
    fn the_report_says_what_went_into_the_measure() {
        let mut runs = HashMap::new();
        begin(&mut runs, blow(1, 34.0), [0.0, 0.0, 0.0], 0);
        let run = runs.get_mut(&7).unwrap();
        let d = run.advance(0.5, true);
        run.moved = true;
        assert!(d > 0.0 && run.walked);
        let line = run.report([0.0, 5.0, 0.0], false, 30);
        assert!(line.contains("alive=0 moved=1 walked=1 pushed back 5.000 units"), "{line}");
        assert!(line.ends_with("frames=1 secs=0.5000 travel=12.750 steps=30"), "{line}");
    }
}
