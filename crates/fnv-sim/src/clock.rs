//! Game time, kept in the engine's time globals.
//!
//! The masters define the clock as globals [observed, FalloutNV.esm]: GameYear 0x35
//! (2281), GameMonth 0x36 (9), GameDay 0x37 (18), GameHour 0x38 (12), GameDaysPassed 0x39
//! (5.0), TimeScale 0x3A (30). Scripts read and set them (Layer 5), so the simulation
//! keeps time *in* those globals: they alone are read. Their f32 cannot hold a frame's
//! worth of time for long, so [`FineClock`] carries the digits they lose between frames
//! [design: #1377; simulation.md §5].
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
        FineClock::default().advance(self, hours)
    }

    /// The date `days` days on, counted as day-by-day steps would count them.
    fn roll_days(self, mut days: i64) -> (i64, i64, i64) {
        let (mut year, mut month, mut day) = (self.year as i64, self.month as i64, self.day as i64);
        while days > 0 {
            // No leap days: every 365 days from a real date come back to it. A script can
            // write a date that is not one (31 February): that takes the first steps.
            if days >= 365 && (0..=11).contains(&month) && (1..=days_in_month(year, month)).contains(&day) {
                year = year.saturating_add(days / 365);
                days %= 365;
                continue;
            }
            // The long straight runs of such a date: a day below 1 only counts up, and a
            // month below 0 has 31 days and no year to roll.
            if day < 1 {
                let steps = days.min(1i64.saturating_sub(day));
                day += steps;
                days -= steps;
                continue;
            }
            if month < 0 && day == 1 && days >= 31 {
                let months = (days / 31).min(0i64.saturating_sub(month));
                month += months;
                days -= months * 31;
                continue;
            }
            days -= 1;
            day = day.saturating_add(1);
            if day > days_in_month(year, month) {
                day = 1;
                month = month.saturating_add(1);
                if month > 11 {
                    month = 0;
                    year = year.saturating_add(1);
                }
            }
        }
        (year, month, day)
    }
}

/// The largest f32 below 24: a stored GameHour stays under 24 while the day has not wrapped.
const LAST_HOUR: f32 = 23.999998;

/// What GameHour stores for an hour of the clock.
fn stored_hour(hour: f64) -> f32 {
    (hour as f32).min(LAST_HOUR)
}

/// A value in f64 and the f32 global written from it.
#[derive(Debug, Clone, Copy)]
struct Carried {
    value: f64,
    written: f32,
}

impl Carried {
    /// A saved value, if it still rounds to the stored global.
    fn restored(saved: Option<&str>, stored: f32, store: fn(f64) -> f32) -> Option<Carried> {
        let value: f64 = saved?.parse().ok()?;
        (value.is_finite() && store(value) == stored).then_some(Carried { value, written: stored })
    }
}

/// The running GameHour and GameDaysPassed in f64, beside the f32 globals.
///
/// Adding a frame's time to an f32 loses it once it is under half the f32's step: at
/// TimeScale 30 and 60 frames a second GameDaysPassed stopped near day 128, at TimeScale 1
/// it never left the masters' 5.0 [design: the arithmetic of #1377]. So a frame adds to
/// these, and the globals are written from them.
///
/// The globals stay the truth, each of the two on its own: a write to one
/// (`World::set_global`) drops its copy even when the value is the same, and a stored value
/// other than the one last written replaces it. The copies are saved beside the globals
/// (`Snapshot::clock_hour`, `Snapshot::clock_days_passed`), so a restored run goes on as
/// the uninterrupted one does; an older save, or a copy that no longer rounds to its
/// global, seeds from the global. GameDay, GameMonth and GameYear are whole numbers the
/// f32 holds, so they need no copy.
#[derive(Debug, Clone, Copy, Default)]
pub struct FineClock {
    hour: Option<Carried>,
    days_passed: Option<Carried>,
}

impl FineClock {
    /// The copies of a snapshot, each kept only if it still rounds to its global.
    pub fn restored(hour: Option<&str>, days_passed: Option<&str>, stored: &Calendar) -> FineClock {
        FineClock {
            // The clock never carries 24 or more, and `stored_hour` would hide one.
            hour: Carried::restored(hour, stored.hour, stored_hour).filter(|c| c.value < 24.0),
            days_passed: Carried::restored(days_passed, stored.days_passed, |v| v as f32),
        }
    }

    /// The copies for a snapshot: GameHour's and GameDaysPassed's, as decimal strings, which
    /// come back exactly through JSON.
    pub fn saved(&self) -> (Option<String>, Option<String>) {
        let text = |c: Option<Carried>| c.filter(|c| c.value.is_finite()).map(|c| c.value.to_string());
        (text(self.hour), text(self.days_passed))
    }

    /// GameHour was written: the stored value is the time, even if it is the same.
    pub fn forget_hour(&mut self) {
        self.hour = None;
    }

    /// GameDaysPassed was written: the stored value is the time, even if it is the same.
    pub fn forget_days_passed(&mut self) {
        self.days_passed = None;
    }

    /// The hour now. A stored GameHour that is no number counts as hour 0, so that the clock
    /// goes on from it [design].
    fn hour_now(&self, stored: f32) -> f64 {
        match self.hour {
            Some(c) if c.written == stored => c.value,
            _ if stored.is_finite() => stored as f64,
            _ => 0.0,
        }
    }

    fn days_passed_now(&self, stored: f32) -> f64 {
        match self.days_passed {
            Some(c) if c.written == stored => c.value,
            _ => stored as f64,
        }
    }

    /// Advance the `stored` calendar by `hours` game hours and answer what to store
    /// (negative values are ignored).
    pub fn advance(&mut self, stored: Calendar, hours: f64) -> Calendar {
        if hours <= 0.0 || !hours.is_finite() {
            return stored;
        }
        let days_passed = self.days_passed_now(stored.days_passed) + hours / 24.0;
        let mut hour = self.hour_now(stored.hour) + hours;
        let mut days = 0;
        if hour >= 24.0 {
            let rest = hour.rem_euclid(24.0);
            days = ((hour - rest) / 24.0).round() as i64;
            hour = rest;
        }
        self.write(stored, hour, days_passed, days)
    }

    /// Move the `stored` calendar forward to `hour` (the next day's when it has passed
    /// today), landing on it exactly: answers what to store and the game hours that passed,
    /// 0 when the clock shows that hour already. Time from the stored GameHour would land
    /// short of it, by what the store rounds away.
    pub fn advance_to_hour(&mut self, stored: Calendar, hour: f64) -> (Calendar, f64) {
        let to = hour.rem_euclid(24.0);
        // A value too small for its own rem_euclid comes back as 24.
        let to = if to >= 24.0 { 0.0 } else { to };
        let now = self.hour_now(stored.hour);
        if stored_hour(to) == stored.hour {
            return (stored, 0.0);
        }
        let mut hours = to - now;
        let next_day = hours < 0.0;
        if next_day {
            hours += 24.0;
        }
        if hours <= 0.0 || !hours.is_finite() {
            return (stored, 0.0);
        }
        let days_passed = self.days_passed_now(stored.days_passed) + hours / 24.0;
        (self.write(stored, to, days_passed, next_day as i64), hours)
    }

    fn write(&mut self, stored: Calendar, hour: f64, days_passed: f64, days: i64) -> Calendar {
        let c = Calendar { hour: stored_hour(hour), days_passed: days_passed as f32, ..stored };
        self.hour = Some(Carried { value: hour, written: c.hour });
        self.days_passed = Some(Carried { value: days_passed, written: c.days_passed });
        let (year, month, day) = stored.roll_days(days);
        Calendar { year: year as f32, month: month as f32, day: day as f32, ..c }
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
    /// TimeScale 1 would be billions of frames; the ignored test below goes the whole way at
    /// the default).
    #[test]
    fn frames_keep_time_past_day_1000() {
        for scale in [1.0, 30.0] {
            for fps in [60.0, 240.0] {
                let what = format!("TimeScale {scale} at {fps} frames a second");
                let (mut clock, mut c) = (FineClock::default(), START);
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
    /// TimeScale: 173 million frames. Run it with
    /// `cargo test -p fnv-sim --lib --release frames_from_the_start -- --ignored`.
    #[test]
    #[ignore = "173 million frames: see the note above"]
    fn frames_from_the_start_past_day_1000() {
        let (mut clock, mut c) = (FineClock::default(), START);
        let elapsed = run_frames(&mut clock, &mut c, 1.0 / 60.0, 30.0, 1001 * 172_800);
        assert!(c.days_passed > 1000.0);
        assert_kept_time(&START, &c, elapsed, "TimeScale 30 at 60 frames a second");
    }

    /// A GameHour about to wrap stays under 24 in the store; the wrap comes with the date.
    #[test]
    fn hour_stays_under_24_until_the_day_wraps() {
        let (mut clock, mut c) = (FineClock::default(), Calendar { hour: 23.0, ..START });
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
        let (mut clock, mut c) = (FineClock::default(), Calendar { days_passed: 1000.0, ..START });
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

    /// The day roll before the jump's shortcut, stepped one day at a time.
    fn stepped_day_by_day(c: Calendar, hours: f64) -> (f32, f32, f32, f32) {
        let mut hour = c.hour as f64 + hours;
        let (mut year, mut month, mut day) = (c.year as i64, c.month as i64, c.day as i64);
        while hour >= 24.0 {
            hour -= 24.0;
            day += 1;
            if day > days_in_month(year, month) {
                day = 1;
                month += 1;
                if month > 11 {
                    month = 0;
                    year += 1;
                }
            }
        }
        (year as f32, month as f32, day as f32, hour as f32)
    }

    #[test]
    fn the_day_roll_matches_stepping_day_by_day_on_any_date() {
        let mut dates: Vec<(f32, f32)> = Vec::new();
        for month in 0..12 {
            for day in 1..=days_in_month(2281, month) {
                dates.push((month as f32, day as f32));
            }
        }
        // Dates only a script can write.
        dates.extend([(1.0, 29.0), (1.0, 30.0), (1.0, 31.0), (3.0, 31.0), (11.0, 40.0), (0.0, 0.0), (5.0, -3.0), (12.0, 5.0), (13.0, 40.0), (-1.0, 5.0)]);
        for (month, day) in dates {
            for jump in [1.0, 28.0, 31.0, 364.0, 365.0, 366.0, 729.0, 730.0, 731.0, 1000.0, 5000.0] {
                let c = Calendar { year: 2281.0, month, day, hour: 12.0, days_passed: 5.0 };
                let n = c.advance(jump * 24.0);
                assert_eq!((n.year, n.month, n.day, n.hour), stepped_day_by_day(c, jump * 24.0), "{month} {day} + {jump}");
            }
        }
        let feb31 = Calendar { year: 2281.0, month: 1.0, day: 31.0, hour: 12.0, days_passed: 5.0 }.advance(24.0 * 365.0);
        assert_eq!((feb31.year, feb31.month, feb31.day), (2282.0, 1.0, 28.0));
    }

    #[test]
    fn a_game_hour_that_is_no_number_does_not_stop_the_clock() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            // Seeded from it, and written over a running clock.
            let mut clock = FineClock::default();
            let mut c = clock.advance(Calendar { hour: bad, ..START }, 1.0);
            assert_eq!((c.day, c.hour), (18.0, 1.0), "{bad}");
            c = clock.advance(c, 1.0);
            assert_eq!(c.hour, 2.0, "{bad}");
            c.hour = bad;
            c = clock.advance(c, 1.0);
            assert_eq!(c.hour, 1.0, "{bad}");
            c = clock.advance(c, 1.0);
            assert_eq!(c.hour, 2.0, "{bad}");
            assert_eq!(c.days_passed, 5.0 + 4.0 / 24.0, "{bad}");
        }
    }

    #[test]
    fn advancing_to_an_hour_lands_on_it() {
        let frame = 1.0 / 60.0 * 30.0 / 3600.0;
        // Past today's: midnight comes, with the date, and the time passed matches.
        let (mut clock, mut c) = (FineClock::default(), START);
        c = clock.advance(c, frame);
        let (to, hours) = clock.advance_to_hour(c, 0.0);
        assert_eq!((to.day, to.hour), (19.0, 0.0));
        assert!((hours - (12.0 - frame)).abs() < 1e-9, "{hours}");
        assert!((to.days_passed as f64 - 5.5).abs() < 1e-5);
        // And it stays exact from there.
        assert_eq!(clock.advance(to, 1.0).hour, 1.0);
        // Later today: the same day.
        let (mut clock, mut c) = (FineClock::default(), START);
        c = clock.advance(c, frame);
        let (to, _) = clock.advance_to_hour(c, 20.0);
        assert_eq!((to.day, to.hour), (18.0, 20.0));
        // An hour past 24 counts round, and 24 is midnight.
        let (to, _) = FineClock::default().advance_to_hour(START, 25.0);
        assert_eq!((to.day, to.hour), (19.0, 1.0));
        let (to, _) = FineClock::default().advance_to_hour(START, 24.0);
        assert_eq!((to.day, to.hour), (19.0, 0.0));
        // The hour the clock shows is no move, whatever digits it carries: not a whole day.
        let (mut clock, mut c) = (FineClock::default(), START);
        for _ in 0..1000 {
            c = clock.advance(c, frame);
            let (same, hours) = clock.advance_to_hour(c, c.hour as f64);
            assert_eq!((same, hours), (c, 0.0));
        }
        // As the day-by-day clock did: 30 written and 10 asked is 4 hours on, next day; and
        // from an hour too far past 24, no move.
        let (to, hours) = FineClock::default().advance_to_hour(Calendar { hour: 30.0, ..START }, 10.0);
        assert_eq!((to.day, to.hour, hours), (19.0, 10.0, 4.0));
        let (to, hours) = FineClock::default().advance_to_hour(Calendar { hour: 50.0, ..START }, 10.0);
        assert_eq!((to.day, to.hour, hours), (18.0, 50.0, 0.0));
        // Not a number: no move.
        let (to, hours) = FineClock::default().advance_to_hour(START, f64::NAN);
        assert_eq!((to, hours), (START, 0.0));
    }

    #[test]
    fn a_saved_copy_counts_only_while_it_rounds_to_its_global() {
        let (mut clock, mut c) = (FineClock::default(), Calendar { days_passed: 1000.0, ..START });
        c = clock.advance(c, 1e-6);
        let (hour, days) = clock.saved();
        let same = FineClock::restored(hour.as_deref(), days.as_deref(), &c);
        assert_eq!(same.saved(), clock.saved());
        // Each on its own: a global that moved, an older save, text that is no number.
        let moved = FineClock::restored(hour.as_deref(), days.as_deref(), &Calendar { days_passed: 7.0, ..c });
        assert_eq!(moved.saved(), (hour.clone(), None));
        let moved = FineClock::restored(hour.as_deref(), days.as_deref(), &Calendar { hour: 7.0, ..c });
        assert_eq!(moved.saved(), (None, days.clone()));
        assert_eq!(FineClock::restored(None, None, &c).saved(), (None, None));
        assert_eq!(FineClock::restored(Some("x"), Some("NaN"), &c).saved(), (None, None));
    }

    #[test]
    fn a_saved_hour_of_24_or_more_falls_back_to_the_global() {
        let c = Calendar { hour: LAST_HOUR, ..START };
        for bad in ["24", "48", "1e300"] {
            let mut clock = FineClock::restored(Some(bad), None, &c);
            assert_eq!(clock.saved(), (None, None), "{bad}");
            let n = clock.advance(c, 1.0);
            assert_eq!(n.day, 19.0, "{bad}: one day, not two");
            assert!((n.hour - 1.0).abs() < 1e-5, "{bad}: {}", n.hour);
        }
        // Just under 24 is what the clock really carries; a negative hour a script wrote
        // goes on as it did.
        let just_under = "23.9999999";
        assert_eq!(FineClock::restored(Some(just_under), None, &c).saved().0.as_deref(), Some(just_under));
        let negative = Calendar { hour: -5.0, ..START };
        let mut clock = FineClock::restored(Some("-5"), None, &negative);
        assert_eq!(clock.saved().0.as_deref(), Some("-5"));
        assert_eq!(clock.advance(negative, 1.0).hour, -4.0);
    }

    #[test]
    fn the_hour_shown_is_the_stored_global_not_a_clamped_one() {
        // GameHour written past 24: the request is a real move, about 18 hours on.
        let written = Calendar { hour: 30.0, ..START };
        let (to, hours) = FineClock::default().advance_to_hour(written, LAST_HOUR as f64);
        assert_eq!((to.day, to.hour), (19.0, LAST_HOUR));
        assert!((hours - 18.0).abs() < 1e-4, "{hours}");
        // And written as exactly 24.
        let (to, hours) = FineClock::default().advance_to_hour(Calendar { hour: 24.0, ..START }, LAST_HOUR as f64);
        assert_eq!((to.day, to.hour), (19.0, LAST_HOUR));
        assert!((hours - 24.0).abs() < 1e-4, "{hours}");
        // Really showing it: no move.
        let shown = Calendar { hour: LAST_HOUR, ..START };
        assert_eq!(FineClock::default().advance_to_hour(shown, LAST_HOUR as f64), (shown, 0.0));
    }

    #[test]
    fn a_far_out_date_skips_its_straight_runs() {
        // Against the steps, as far as they can be run here.
        let c = |month: f32, day: f32| Calendar { month, day, ..START };
        for (month, day) in [(5.0, -1000.0), (-50.0, 1.0), (-50.0, -20.0), (-3.0, 40.0), (-3.0, 31.0), (20.0, -10.0)] {
            for jump in [1.0, 30.0, 31.0, 32.0, 1000.0, 1001.0, 1002.0, 1549.0, 1550.0, 1551.0, 2600.0, 2601.0, 3000.0] {
                let n = c(month, day).advance(jump * 24.0);
                assert_eq!((n.year, n.month, n.day, n.hour), stepped_day_by_day(c(month, day), jump * 24.0), "{month} {day} + {jump}");
            }
        }
        let far = c(-200.0, -5_000_000.0);
        let jump = (5_000_001.0 + 200.0 * 31.0 + 400.0) * 24.0;
        let n = far.advance(jump);
        assert_eq!((n.year, n.month, n.day, n.hour), stepped_day_by_day(far, jump));

        // Beyond them: a billion days from day -1,000,000,000 end at once, on what the
        // steps count to by hand.
        let day = -1_000_000_000.0;
        assert_eq!(c(5.0, day).roll_days(1_000_000_000), (2281, 5, 0));
        assert_eq!(c(5.0, day).roll_days(1_000_000_001), (2281, 5, 1));
        assert_eq!(c(5.0, day).roll_days(1_000_000_001 + 365), (2282, 5, 1));
        assert_eq!(c(-1_000_000_000.0, 1.0).roll_days(31_000_000_000), (2281, 0, 1));
        // Too far to step at all: the extremes neither hang nor overflow.
        assert_eq!(c(f32::MAX, f32::MAX).roll_days(1), (2282, 0, 1));
        assert_eq!(c(f32::MIN, f32::MIN).roll_days(1), (2281, i64::MIN, i64::MIN + 1));
        assert_eq!(c(5.0, f32::MIN).roll_days(i64::MAX), (2281, 5, -1));
    }
}
