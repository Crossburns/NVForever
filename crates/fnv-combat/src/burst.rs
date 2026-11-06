//! Bursts of an NPC's automatic weapon: the trigger is held for one burst, then let go
//! for a pause before the next (issue #1448: Securitrons held the trigger for as long as
//! they saw their target).
//!
//! How the original paces an NPC's automatic fire is not documented: the WEAP flags
//! Short Burst and Long Burst are stored but their meaning is unknown
//! (docs/spec-notes/item-fields.md), and CSTY has no burst fields. So the rule here is
//! NVForever's [design], tuned by the owner (docs/owner-review.md):
//!
//! - a burst is [`crate::vats::burst_shots`] rounds of the weapon's fire rate (0.4 s of
//!   fire, 3 to 8 rounds), as V.A.T.S. bursts are;
//! - the pause after it is drawn between [`PAUSE_MIN`] and [`PAUSE_MAX`] seconds, each end
//!   scaled by the combat style's semi-automatic firing delay multipliers (min, max).
//!
//! Pure: the caller draws the random unit and steps the clock.

/// The shortest and the longest pause between bursts, seconds, before the combat style's
/// multipliers [design].
pub const PAUSE_MIN: f32 = 0.5;
pub const PAUSE_MAX: f32 = 1.5;

/// One fighter's burst state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Burst {
    /// Rounds fired in the burst under way (0: none under way).
    fired: u32,
    /// Seconds before the trigger may be held again.
    pause: f32,
}

/// A burst that has just ended: its rounds and the pause that follows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ended {
    pub rounds: u32,
    pub pause: f32,
}

/// The pause after a burst for a random `unit` in [0, 1), with the style's multipliers
/// `mult` (min, max; (1, 1) without a style).
pub fn pause(mult: (f32, f32), unit: f32) -> f32 {
    let (lo, hi) = (PAUSE_MIN * mult.0.max(0.0), PAUSE_MAX * mult.1.max(0.0));
    (lo + (hi - lo).max(0.0) * unit.clamp(0.0, 1.0)).max(0.0)
}

impl Burst {
    /// Whether the trigger may be held now (no pause under way).
    pub fn may_fire(&self) -> bool {
        self.pause <= 0.0
    }

    /// `dt` seconds passed.
    pub fn tick(&mut self, dt: f32) {
        self.pause = (self.pause - dt).max(0.0);
    }

    /// A round was fired by a weapon firing `fire_rate` shots a second. When that ends
    /// the burst, the pause starts (drawn with `unit`, scaled by `mult`) and the ended
    /// burst is returned.
    pub fn fired(&mut self, fire_rate: f32, mult: (f32, f32), unit: f32) -> Option<Ended> {
        self.fired += 1;
        if self.fired < crate::vats::burst_shots(fire_rate) {
            return None;
        }
        let ended = Ended { rounds: self.fired, pause: pause(mult, unit) };
        self.fired = 0;
        self.pause = ended.pause;
        Some(ended)
    }

    /// The trigger was let go before the burst ended (lost sight, a wall, a reload): the
    /// next pull starts a fresh burst, without a pause.
    pub fn let_go(&mut self) {
        self.fired = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_is_the_vats_burst_and_then_a_pause() {
        // A Securitron's gun at 10 rounds a second: 4 rounds (0.4 s), then a pause.
        let mut b = Burst::default();
        assert!(b.may_fire());
        for _ in 0..3 {
            assert_eq!(b.fired(10.0, (1.0, 1.0), 0.5), None);
            assert!(b.may_fire());
        }
        let e = b.fired(10.0, (1.0, 1.0), 0.5).expect("the fourth round ends it");
        assert_eq!(e.rounds, 4);
        assert!((e.pause - 1.0).abs() < 1e-6, "halfway between 0.5 and 1.5: {}", e.pause);
        assert!(!b.may_fire());
        b.tick(0.6);
        assert!(!b.may_fire());
        b.tick(0.4);
        assert!(b.may_fire());
        // The next burst counts from one again.
        assert_eq!(b.fired(10.0, (1.0, 1.0), 0.0), None);
    }

    #[test]
    fn burst_sizes_stay_within_three_to_eight_rounds() {
        for (rate, want) in [(2.0, 3), (12.0, 5), (20.0, 8), (60.0, 8), (0.0, 3)] {
            let mut b = Burst::default();
            let mut n = 1;
            while b.fired(rate, (1.0, 1.0), 0.0).is_none() {
                n += 1;
            }
            assert_eq!(n, want, "fire rate {rate}");
        }
    }

    #[test]
    fn the_pause_spans_its_range_scaled_by_the_style() {
        assert_eq!(pause((1.0, 1.0), 0.0), PAUSE_MIN);
        assert_eq!(pause((1.0, 1.0), 1.0), PAUSE_MAX);
        assert_eq!(pause((2.0, 2.0), 0.0), 2.0 * PAUSE_MIN);
        assert_eq!(pause((0.0, 0.0), 0.7), 0.0);
        // A style whose min is above its max: the min holds.
        assert_eq!(pause((4.0, 1.0), 0.9), 4.0 * PAUSE_MIN);
    }

    #[test]
    fn letting_go_starts_a_fresh_burst_without_a_pause() {
        let mut b = Burst::default();
        b.fired(10.0, (1.0, 1.0), 0.0);
        b.fired(10.0, (1.0, 1.0), 0.0);
        b.let_go();
        assert!(b.may_fire());
        for _ in 0..3 {
            assert_eq!(b.fired(10.0, (1.0, 1.0), 0.0), None);
        }
        assert!(b.fired(10.0, (1.0, 1.0), 0.0).is_some());
    }
}
