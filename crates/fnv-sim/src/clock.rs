//! Game time, kept in the engine's time globals.
//!
//! The masters define the clock as globals [observed, FalloutNV.esm]: GameYear 0x35
//! (2281), GameMonth 0x36 (9), GameDay 0x37 (18), GameHour 0x38 (12), GameDaysPassed 0x39
//! (5.0), TimeScale 0x3A (30). Scripts read and set them (Layer 5), so the simulation
//! keeps time *in* those globals: they alone are saved and they alone are read. Their f32
//! cannot hold a frame's worth of time for long, so [`FineClock`] carries the digits they
//! lose between frames [design: #1377; simulation.md §5].
//!
//! Rules:
//! - TimeScale is game seconds per real second (30 → one real minute is 30 game
//!   minutes) [hypothesis; GECK wiki "TimeScale" describes it as the game-time ratio].
//! - Advancing by h game hours adds h to GameHour and h / 24 to GameDaysPassed; each
//!   wrap of GameHour past 24 advances GameDay, rolling the month and year over with
//!   Gregorian month lengths but no leap days, GameMonth counting from 0 (January)
//!   [observed: February 2280 and February 2284 have 28 days, docs/spec-notes/simulation.md
//!   §5b and the function probes `fp6_calendar`, 2026-10-17]. [observed in game for
//!   the day and month: day 18 + 30 h → day 19; day 31 of month 9 at 20:00 + 6 h → day
//!   1 of month 10 (xNVSE capture, `L4.sim.trace.goodsprings_weight_and_time`). The
//!   GameHour/GameDaysPassed arithmetic and the year rollover are still hypothesis: the
//!   in-game clock keeps running during menus, so those were not captured.]

/// Editor IDs of the time globals.
pub const GAME_YEAR: &str = "GameYear";
pub const GAME_MONTH: &str = "GameMonth";
pub const GAME_DAY: &str = "GameDay";
pub const GAME_HOUR: &str = "GameHour";
pub const GAME_DAYS_PASSED: &str = "GameDaysPassed";
pub const TIME_SCALE: &str = "TimeScale";

/// Calendar values (all as stored in the globals).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calendar {
    pub year: f32,
    pub month: f32,
    pub day: f32,
    pub hour: f32,
    pub days_passed: f32,
}

/// Days of a month (`month0`: 0 January … 11 December): the Gregorian lengths without
/// leap days, February always 28 [observed: 28 February 2280 and 28 February 2284 + 24 h
/// gave 1 March, simulation.md §5b]. `year` is kept for callers; it changes nothing.
pub fn days_in_month(_year: i64, month0: i64) -> i64 {
    match month0 {
        1 => 28,
        3 | 5 | 8 | 10 => 30,
        _ => 31,
    }
}

impl Calendar {
    /// Advance by `hours` game hours (negative values are ignored), once. Time that goes
    /// on frame by frame takes a [`FineClock`] (`World::advance_hours`).
    pub fn advance(self, hours: f64) -> Calendar {
        let mut fine = FineClock::UNSEEDED;
        fine.advance(self, hours)
    }
}

/// The largest f32 below 24: a stored GameHour stays under 24 while the day has not wrapped.
const LAST_HOUR: f32 = 23.999998;

/// The running GameHour and GameDaysPassed in f64, beside the f32 globals.
///
/// Adding a frame's time to an f32 loses it once it is under half the f32's step: at
/// TimeScale 30 and 60 frames a second GameDaysPassed stopped near day 128, at TimeScale 1
/// it never left the masters' 5.0 [design: the arithmetic of #1377]. So a frame adds to
/// these, and the globals are written from them.
///
/// The globals stay the truth: a stored value other than the one last written was set by
/// someone else (a script, a load) and is taken up, each of the two on its own. Nothing
/// here is saved: a loaded game seeds from the globals and loses under one f32 step.
/// GameDay, GameMonth and GameYear are whole numbers the f32 holds, so they need no copy.
#[derive(Debug, Clone, Copy)]
pub struct FineClock {
    hour: f64,
    days_passed: f64,
    written_hour: f32,
    written_days_passed: f32,
}

impl FineClock {
    /// Nothing written yet: NaN equals no stored value, so the first advance seeds.
    pub const UNSEEDED: FineClock =
        FineClock { hour: 0.0, days_passed: 0.0, written_hour: f32::NAN, written_days_passed: f32::NAN };

    /// Advance the `stored` calendar by `hours` game hours and answer what to store
    /// (negative values are ignored).
    pub fn advance(&mut self, stored: Calendar, hours: f64) -> Calendar {
        if hours <= 0.0 || !hours.is_finite() {
            return stored;
        }
        if stored.hour != self.written_hour {
            self.hour = stored.hour as f64;
        }
        if stored.days_passed != self.written_days_passed {
            self.days_passed = stored.days_passed as f64;
        }
        self.days_passed += hours / 24.0;
        self.hour += hours;
        let (mut year, mut month, mut day) = (stored.year as i64, stored.month as i64, stored.day as i64);
        if self.hour >= 24.0 {
            let rest = self.hour.rem_euclid(24.0);
            let days = ((self.hour - rest) / 24.0).round() as i64;
            self.hour = rest;
            // No leap days: every 365 days come back to the same date.
            year = year.saturating_add(days / 365);
            for _ in 0..days % 365 {
                day += 1;
                if day > days_in_month(year, month) {
                    day = 1;
                    month += 1;
                    if month > 11 {
                        month = 0;
                        year = year.saturating_add(1);
                    }
                }
            }
        }
        self.written_hour = (self.hour as f32).min(LAST_HOUR);
        self.written_days_passed = self.days_passed as f32;
        Calendar {
            year: year as f32,
            month: month as f32,
            day: day as f32,
            hour: self.written_hour,
            days_passed: self.written_days_passed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollover() {
        let c = Calendar { year: 2281.0, month: 9.0, day: 31.0, hour: 23.0, days_passed: 5.0 };
        let n = c.advance(2.0);
        assert_eq!((n.year, n.month, n.day, n.hour), (2281.0, 10.0, 1.0, 1.0));
        assert!((n.days_passed - (5.0 + 2.0 / 24.0)).abs() < 1e-6);
        let ny = Calendar { year: 2281.0, month: 11.0, day: 31.0, hour: 12.0, days_passed: 0.0 }.advance(24.0);
        assert_eq!((ny.year, ny.month, ny.day, ny.hour), (2282.0, 0.0, 1.0, 12.0));
        let leap = Calendar { year: 2284.0, month: 1.0, day: 28.0, hour: 0.0, days_passed: 0.0 }.advance(24.0);
        assert_eq!((leap.month, leap.day), (2.0, 1.0), "no 29 February, even in 2284");
        assert_eq!(c.advance(-5.0), c);
        assert_eq!(c.advance(f64::NAN), c);
        let long = c.advance(24.0 * 400.0);
        assert!((long.days_passed - 405.0).abs() < 1e-3);
    }

    const START: Calendar = Calendar { year: 2281.0, month: 9.0, day: 18.0, hour: 12.0, days_passed: 5.0 };

    /// Days since a fixed date, read from GameYear, GameMonth, GameDay and GameHour.
    fn days_by_date(c: &Calendar) -> f64 {
        let before_month: i64 = (0..c.month as i64).map(|m| days_in_month(0, m)).sum();
        (c.year as f64) * 365.0 + before_month as f64 + c.day as f64 + c.hour as f64 / 24.0
    }

    /// `elapsed` game days have passed since `start`, by GameDaysPassed and by the date.
    fn assert_kept_time(start: &Calendar, now: &Calendar, elapsed: f64, what: &str) {
        // Two f32 steps of the stored value: the closest it can be asked to come.
        let tolerance = 2.0 * (now.days_passed.next_up() - now.days_passed) as f64;
        let by_global = (now.days_passed as f64) - (start.days_passed as f64);
        assert!((by_global - elapsed).abs() <= tolerance, "{what}: GameDaysPassed moved {by_global}, time {elapsed}");
        // GameHour's own step, under a hundredth of a game second, is far under this.
        let by_date = days_by_date(now) - days_by_date(start);
        assert!((by_date - elapsed).abs() <= 1e-5, "{what}: the date moved {by_date}, time {elapsed}");
    }

    /// `frames` frames of `dt` real seconds at `scale`: the true time they took, in game days.
    fn run_frames(clock: &mut FineClock, c: &mut Calendar, dt: f64, scale: f64, frames: u64) -> f64 {
        let hours = dt * scale / 3600.0;
        for _ in 0..frames {
            *c = clock.advance(*c, hours);
        }
        frames as f64 * hours / 24.0
    }

    /// The bug is a frame's time against the stored value's step, so frames are run at the
    /// days where that step grew past them, with whole hours between (the whole way at
    /// TimeScale 1 would be billions of frames; the test below goes the whole way at the
    /// default).
    #[test]
    fn frames_keep_time_past_day_1000() {
        for scale in [1.0, 30.0] {
            for fps in [60.0, 240.0] {
                let what = format!("TimeScale {scale} at {fps} frames a second");
                let (mut clock, mut c) = (FineClock::UNSEEDED, START);
                let mut elapsed = 0.0;
                // A game hour of frames at TimeScale 1, ten at the default.
                let frames = (3600.0 / scale * fps) as u64 * if scale > 1.0 { 10 } else { 1 };
                for day in [5.0, 32.0, 64.0, 128.0, 512.0, 1024.0, 1500.0] {
                    let hop = (day - START.days_passed as f64 - elapsed) * 24.0;
                    c = clock.advance(c, hop);
                    elapsed += hop.max(0.0) / 24.0;
                    elapsed += run_frames(&mut clock, &mut c, 1.0 / fps, scale, frames);
                    assert_kept_time(&START, &c, elapsed, &what);
                    assert!((0.0..24.0).contains(&c.hour), "{what}: hour {}", c.hour);
                }
                assert!(c.days_passed > 1000.0, "{what}");
            }
        }
    }

    /// Frame by frame all the way from the masters' day 5 past day 1000, at the default
    /// TimeScale: 173 million frames, a few seconds even unoptimised.
    #[test]
    fn frames_from_the_start_past_day_1000() {
        let (mut clock, mut c) = (FineClock::UNSEEDED, START);
        let elapsed = run_frames(&mut clock, &mut c, 1.0 / 60.0, 30.0, 1001 * 172_800);
        assert!(c.days_passed > 1000.0);
        assert_kept_time(&START, &c, elapsed, "TimeScale 30 at 60 frames a second");
    }

    /// A GameHour about to wrap stays under 24 in the store; the wrap comes with the date.
    #[test]
    fn hour_stays_under_24_until_the_day_wraps() {
        let (mut clock, mut c) = (FineClock::UNSEEDED, Calendar { hour: 23.0, ..START });
        c = clock.advance(c, 1.0 - 1e-9);
        assert_eq!((c.day, c.hour), (18.0, LAST_HOUR));
        assert_eq!(LAST_HOUR.next_up(), 24.0);
        c = clock.advance(c, 2e-9);
        assert_eq!(c.day, 19.0);
        assert!(c.hour < 1e-6);
    }

    #[test]
    fn whole_steps_and_big_jumps_keep_the_date() {
        // Stepped day by day and jumped at once come to the same date.
        let mut stepped = START;
        for _ in 0..1000 {
            stepped = stepped.advance(24.0);
        }
        let jumped = START.advance(24.0 * 1000.0);
        assert_eq!((jumped.year, jumped.month, jumped.day, jumped.hour), (stepped.year, stepped.month, stepped.day, stepped.hour));
        assert_eq!(jumped.days_passed, 1005.0);
        assert_kept_time(&START, &jumped, 1000.0, "a jump of 1000 days");
        // A year of 365 days comes back to the same date.
        let year = START.advance(24.0 * 365.0);
        assert_eq!((year.year, year.month, year.day, year.hour), (2282.0, 9.0, 18.0, 12.0));
        // A jump no one could step through ends at once.
        let far = START.advance(1e15);
        assert!((0.0..24.0).contains(&far.hour) && far.day >= 1.0 && far.month <= 11.0);
    }

    #[test]
    fn a_stored_value_someone_else_wrote_wins_on_its_own() {
        let (mut clock, mut c) = (FineClock::UNSEEDED, Calendar { days_passed: 1000.0, ..START });
        c = clock.advance(c, 1e-6);
        let hour_before = c.hour;
        // GameDaysPassed set: taken up, and the hour goes on from its own digits.
        c.days_passed = 42.5;
        c = clock.advance(c, 24.0);
        assert!((c.days_passed - 43.5).abs() < 1e-5);
        assert!((c.hour - hour_before).abs() < 1e-5);
        // GameHour set: taken up, and GameDaysPassed goes on.
        c.hour = 3.25;
        c = clock.advance(c, 1.0);
        assert!((c.hour - 4.25).abs() < 1e-5);
        assert!((c.days_passed as f64 - (43.5 + 1.0 / 24.0)).abs() < 1e-5);
        // Time that does not pass leaves everything as stored; the next that does takes it up.
        c.days_passed = 7.0;
        assert_eq!(clock.advance(c, 0.0), c);
        assert_eq!(clock.advance(c, f64::NAN), c);
        assert!((clock.advance(c, 24.0).days_passed - 8.0).abs() < 1e-5);
    }
}
