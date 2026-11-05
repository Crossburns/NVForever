//! Sandboxing: what an actor under a Sandbox (or Wander) package does once it is in the
//! package's area. It looks at what is around it and picks one thing after another: a
//! seat, an idle marker, a walk to somewhere near; a bed at night; a seat at mealtimes.
//!
//! What is documented [GECK wiki, Sandbox Package] and followed here:
//! - the package's flags switch behaviours off (record bits "No Eating", "No Sleeping",
//!   "No Conversation", "No Idle Markers", "No Furniture", "No Wandering");
//! - objects are taken from within the package's radius; an object just used is not
//!   chosen again at once; each kind of behaviour has a weight that drops to zero when
//!   the kind was just chosen and grows back while others are;
//! - sleep: a start time rolled between `fSandboxSleepStartMin` (19) and
//!   `fSandboxSleepStartMax` (1), a duration between `fSandboxSleepDurationMin` (6) and
//!   `fSandboxSleepDurationMax` (10) hours, rolled when the package starts and again
//!   each day;
//! - meals: breakfast starting 6–9, lunch 11–14, dinner 17–20, lasting 0.5–2 hours
//!   (`fSandboxBreakfastMin` …), rolled the same way;
//! - low-energy actors spend more time sitting and eating, high-energy ones wandering
//!   and idling.
//!
//! What is ours, because the documentation gives no numbers ("various gamesettings and
//! formulas"): how an object is scored (nearer is better), how fast weights grow back,
//! how long each behaviour lasts, how long an object with no way to it stays out of the
//! choices (`RETRY_DELAYS`), and that an actor without a bed in its area does not
//! sleep. Not done: conversations between actors, picking up and eating food (an eating
//! actor sits down if a seat is free, as an Eat package's does), and the idles an idle
//! marker lists (the actor goes there and faces its way).

use crate::furniture::Eligibility;
use crate::goal::{self, Goal};
use fnv_game::ai::SandboxPhase;
use fnv_game::game::Game;
use fnv_game::package::{furniture, type_flags, PackageDef};
use fnv_nav::graph::NONE;
use fnv_nav::{NavGraph, Place};
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Reach of a sandbox whose location has no radius of its own, game units [our choice;
/// the default sandbox packages use 512].
pub const DEFAULT_RADIUS: f32 = 512.0;

/// The behaviours a package leaves on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Allowed {
    pub eating: bool,
    pub sleeping: bool,
    pub conversation: bool,
    pub idle_markers: bool,
    pub furniture: bool,
    pub wandering: bool,
}

impl Allowed {
    pub fn of(def: &PackageDef) -> Allowed {
        Allowed {
            eating: !def.sandbox_flag(type_flags::NO_EATING),
            sleeping: !def.sandbox_flag(type_flags::NO_SLEEPING),
            conversation: !def.sandbox_flag(type_flags::NO_CONVERSATION),
            idle_markers: !def.sandbox_flag(type_flags::NO_IDLE_MARKERS),
            furniture: !def.sandbox_flag(type_flags::NO_FURNITURE),
            wandering: !def.sandbox_flag(type_flags::NO_WANDERING),
        }
    }
}

/// What a sandboxing actor is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "doing", content = "object", rename_all = "snake_case")]
pub enum Doing {
    /// Nothing in particular (nothing around to use, or between two things).
    Standing,
    /// Walked to a spot in the area.
    Wandering,
    Sitting(FormId),
    AtIdleMarker(FormId),
    Sleeping(FormId),
    /// A meal, on a seat when one is free.
    Eating(Option<FormId>),
}

impl Doing {
    /// The furniture the behaviour uses.
    pub fn furniture(&self) -> Option<FormId> {
        match self {
            Doing::Sitting(f) | Doing::Sleeping(f) | Doing::Eating(Some(f)) => Some(*f),
            _ => None,
        }
    }

    fn object(&self) -> Option<FormId> {
        match self {
            Doing::AtIdleMarker(m) => Some(*m),
            d => d.furniture(),
        }
    }
}

/// When the current behaviour is over.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Until {
    /// `Game::time`, seconds.
    Time(f64),
    /// GameDaysPassed (sleep and meals run on the game's clock).
    Day(f32),
}

/// A behaviour chosen and what it takes.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub doing: Doing,
    /// Where to go first, if anywhere.
    pub target: Option<Place>,
    /// The way to face there (an idle marker's).
    pub heading: Option<f32>,
    until: Until,
}

/// A small deterministic generator (SplitMix64): a sandbox must not disturb the script
/// runtime's random numbers, and runs must repeat.
#[derive(Debug, Clone)]
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }

    pub(crate) fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// The kinds of behaviour that take turns.
const FURNITURE: usize = 0;
const IDLE_MARKER: usize = 1;
const WANDER: usize = 2;

/// How long an object with no way to it stays out of the choices, seconds of `Game::time`
/// from each failure, longer with each failure in a row and capped at the last [design:
/// our balance of how soon an actor notices a restored way against what failed searches
/// cost; neither the documentation nor the original gives numbers].
const RETRY_DELAYS: [f64; 5] = [60.0, 120.0, 240.0, 480.0, 600.0];

/// An object there was no way to, the last time the actor tried.
#[derive(Debug, Clone, Copy)]
struct FailedObject {
    /// `Game::time` from which it may be chosen again.
    retry_at: f64,
    /// Which of [`RETRY_DELAYS`] its last failure waits.
    stage: usize,
}

pub struct Sandbox {
    pub allowed: Allowed,
    centre: Place,
    /// Objects and wander spots within this of the centre; infinite for an "in cell"
    /// location (the whole cell).
    radius: f32,
    rng: Rng,
    weights: [f32; 3],
    last_object: Option<FormId>,
    /// Objects there was no way to. An entry stays after its wait is over, to keep its
    /// stage, until the object's behaviour begins [design]. Not saved: a load starts it
    /// empty.
    failed_objects: BTreeMap<FormId, FailedObject>,
    /// The day the sleep and meal times were rolled for.
    day: i32,
    /// Start hour and duration in hours.
    sleep: (f32, f32),
    meals: [(f32, f32); 3],
    /// Energy level, 0 to 1.
    energy: f32,
    pub doing: Doing,
    until: Until,
    /// Chosen, and being walked to.
    pub(crate) pending: Option<Choice>,
    /// `Game::time` when the last choice was made.
    chosen_at: f64,
    /// It got up from furniture for its last choice, which has not begun yet
    /// ([`Sandbox::phase`]).
    pub(crate) got_up: bool,
    /// Another behaviour took control; cancellation runs once until the next choice.
    interrupted: bool,
}

/// A sandbox's day as a save keeps it ([`crate::progress`]): where its random numbers
/// stand, the day its sleep and meal times were rolled for, those times, and its energy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub rng: u64,
    pub day: i32,
    pub sleep: (f32, f32),
    pub meals: [(f32, f32); 3],
    pub energy: f32,
}

fn setting(game: &Game, name: &str, default: f32) -> f32 {
    game.world.forms.setting_f32(name).unwrap_or(default)
}

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl Sandbox {
    /// The sandbox of `actor` under `def`, starting at `at` (where the actor arrived).
    pub fn start(game: &Game, actor: FormId, package: FormId, def: &PackageDef, goal: &Goal, at: Place) -> Sandbox {
        let (centre, radius) = match goal {
            Goal::Near { place, radius } => (*place, if *radius > 0.0 { *radius } else { DEFAULT_RADIUS }),
            Goal::Inside { .. } => (at, f32::INFINITY),
            _ => (at, def.location.map(|l| l.radius as f32).filter(|r| *r > 0.0).unwrap_or(DEFAULT_RADIUS)),
        };
        let energy = game.world.placed.get(actor).and_then(|p| game.data.actors.get(&p.base)).and_then(|a| a.energy).map_or(0.5, |e| (e as f32 / 100.0).clamp(0.0, 1.0));
        let mut s = Sandbox {
            allowed: Allowed::of(def),
            centre,
            radius,
            rng: Rng((actor.0 as u64) << 32 | package.0 as u64),
            weights: [1.0; 3],
            last_object: None,
            failed_objects: BTreeMap::new(),
            day: i32::MIN,
            sleep: (0.0, 0.0),
            meals: [(0.0, 0.0); 3],
            energy,
            doing: Doing::Standing,
            until: Until::Time(f64::NEG_INFINITY),
            pending: None,
            got_up: false,
            interrupted: false,
            chosen_at: 0.0,
        };
        s.roll(game);
        s
    }

    /// Roll the day's sleep and meal times (when the package starts, then once a day).
    fn roll(&mut self, game: &Game) {
        let Ok(c) = game.world.calendar() else { return };
        let day = c.days_passed.floor() as i32;
        if day == self.day {
            return;
        }
        self.day = day;
        // The start may be after midnight (19:00 to 01:00): the maximum wraps.
        let (lo, mut hi) = (setting(game, "fSandboxSleepStartMin", 19.0), setting(game, "fSandboxSleepStartMax", 1.0));
        if hi < lo {
            hi += 24.0;
        }
        self.sleep = (self.rng.range(lo, hi).rem_euclid(24.0), self.rng.range(setting(game, "fSandboxSleepDurationMin", 6.0), setting(game, "fSandboxSleepDurationMax", 10.0)));
        let meal = [("fSandboxBreakfastMin", 6.0, "fSandboxBreakfastMax", 9.0), ("fSandboxLunchMin", 11.0, "fSandboxLunchMax", 14.0), ("fSandboxDinnerMin", 17.0, "fSandboxDinnerMax", 20.0)];
        for (i, (a, da, b, db)) in meal.into_iter().enumerate() {
            let start = self.rng.range(setting(game, a, da), setting(game, b, db));
            let hours = self.rng.range(setting(game, "fSandboxMealDurationMin", 0.5), setting(game, "fSandboxMealDurationMax", 2.0));
            self.meals[i] = (start, hours);
        }
    }

    /// The day as a save keeps it ([`crate::progress`]).
    pub fn schedule(&self) -> Schedule {
        Schedule { rng: self.rng.0, day: self.day, sleep: self.sleep, meals: self.meals, energy: self.energy }
    }

    /// Put back the day a save kept: the same times today, and the same rolls on the
    /// days after ([`crate::progress`]).
    pub fn set_schedule(&mut self, s: &Schedule) {
        self.rng = Rng(s.rng);
        self.day = s.day;
        self.sleep = s.sleep;
        self.meals = s.meals;
        self.energy = s.energy;
    }

    /// The day's sleep window (start hour, hours) and meals, for reports and tests.
    pub fn times(&self) -> ((f32, f32), [(f32, f32); 3]) {
        (self.sleep, self.meals)
    }

    /// Whether the behaviour in hand is over.
    pub fn due(&self, game: &Game) -> bool {
        match self.until {
            Until::Time(t) => game.time >= t,
            Until::Day(d) => game.world.calendar().map_or(true, |c| c.days_passed >= d),
        }
    }

    /// Give up the current choice when another behaviour takes control [design].
    /// The next package update chooses afresh, retaining the day, random sequence and
    /// object-selection history. This is neither an arrival nor a failed route.
    /// Returns true only on the first interruption, so the caller can relinquish the
    /// sandbox's movement without clearing the interrupting behaviour's later walks.
    pub(crate) fn interrupt(&mut self) -> bool {
        if self.interrupted {
            return false;
        }
        self.interrupted = true;
        self.pending = None;
        self.got_up = false;
        self.doing = Doing::Standing;
        self.until = Until::Time(f64::NEG_INFINITY);
        true
    }

    /// Hours left of a window that started at `start` and lasts `hours`, at `hour`.
    fn left(start: f32, hours: f32, hour: f32) -> Option<f32> {
        let since = (hour - start).rem_euclid(24.0);
        (since < hours).then_some(hours - since)
    }

    /// The objects of the area: references of the centre's cell within the radius, at
    /// about the actor's height, present, not waiting out a failed way to them, not given
    /// to another actor.
    fn objects(&self, game: &Game, taken: &BTreeMap<FormId, FormId>, actor: FormId, at: Place) -> Vec<(FormId, [f32; 3], f32)> {
        let refs = game.world.placed;
        let Some(cell) = goal::cell_of_place(refs, &self.centre) else { return Vec::new() };
        refs.by_cell
            .get(&cell)
            .into_iter()
            .flatten()
            .filter(|r| self.failed_objects.get(r).is_none_or(|f| game.time >= f.retry_at) && taken.get(r).is_none_or(|a| *a == actor))
            .filter_map(|r| {
                let p = refs.get(*r)?;
                let near = self.radius.is_infinite() || hdist(p.pos, self.centre.pos) <= self.radius;
                (near && (p.pos[2] - at.pos[2]).abs() < 200.0 && game.present(*r)).then_some((*r, p.pos, p.rot[2]))
            })
            .collect()
    }

    /// A spot of the navmesh to wander to: a triangle of the actor's walkable area
    /// within the radius, picked at random.
    fn wander_spot(&mut self, nav: &NavGraph, at: Place) -> Option<Place> {
        let here = nav.locate(at.space, at.pos, 256.0).or_else(|| nav.nearest(at.space, at.pos, 256.0).map(|n| n.0))?;
        let area = nav.area[here as usize];
        if area == NONE {
            return None;
        }
        // The whole cell is at most this far away [our choice for "in cell"].
        let reach = if self.radius.is_infinite() { 2048.0 } else { self.radius };
        let centre = self.centre.pos;
        let spots: Vec<u32> = nav
            .triangles_near(at.space, centre[0], centre[1], reach)
            .into_iter()
            .filter(|t| nav.area[*t as usize] == area && nav.usable(*t))
            .filter(|t| {
                let c = nav.centre(*t);
                hdist(c, centre) <= reach && (c[2] - at.pos[2]).abs() < 400.0 && hdist(c, at.pos) > 64.0
            })
            .collect();
        if spots.is_empty() {
            return None;
        }
        let pick = spots[((self.rng.next() * spots.len() as f32) as usize).min(spots.len() - 1)];
        Some(Place { space: at.space, pos: nav.centre(pick) })
    }

    /// Choose what to do next, standing at `at`.
    pub fn next(&mut self, game: &Game, nav: &NavGraph, taken: &BTreeMap<FormId, FormId>, actor: FormId, at: Place, eligibility: &mut Eligibility) -> Choice {
        self.interrupted = false;
        self.roll(game);
        self.chosen_at = game.time;
        let cal = game.world.calendar().ok();
        let (hour, days) = cal.map_or((12.0, 0.0), |c| (c.hour, c.days_passed));
        let data = game.data;
        let refs = game.world.placed;
        let objects = self.objects(game, taken, actor, at);
        // Of the seats and beds only: the idle markers are in the same list.
        let excluded = eligibility.excluded_casual(game, actor, objects.iter().map(|o| o.0));
        let flags = |r: FormId| refs.get(r).filter(|_| !excluded.contains(&r)).and_then(|p| data.furniture.get(&p.base)).copied();
        let nearest = |want: u32| {
            objects.iter().filter(|o| flags(o.0).is_some_and(|f| f & want != 0)).min_by(|a, b| hdist(a.1, at.pos).total_cmp(&hdist(b.1, at.pos))).copied()
        };
        let place = |pos: [f32; 3]| Some(Place { space: at.space, pos });
        // Night: to bed, if there is one.
        if self.allowed.sleeping {
            if let (Some(left), Some(bed)) = (Self::left(self.sleep.0, self.sleep.1, hour), nearest(furniture::SLEEP)) {
                return Choice { doing: Doing::Sleeping(bed.0), target: place(bed.1), heading: None, until: Until::Day(days + left / 24.0) };
            }
        }
        // Mealtimes: on a seat when one is free.
        if self.allowed.eating {
            if let Some(left) = self.meals.iter().find_map(|m| Self::left(m.0, m.1, hour)) {
                let seat = nearest(furniture::SIT).filter(|_| self.allowed.furniture);
                return Choice { doing: Doing::Eating(seat.map(|s| s.0)), target: seat.and_then(|s| place(s.1)), heading: None, until: Until::Day(days + left / 24.0) };
            }
        }
        // Otherwise one of the behaviours that take turns. Candidates per kind, without
        // the object just used.
        let fresh = |o: &&(FormId, [f32; 3], f32)| Some(o.0) != self.last_object;
        let seats: Vec<&(FormId, [f32; 3], f32)> = objects.iter().filter(|o| flags(o.0).is_some_and(|f| f & furniture::SIT != 0)).filter(fresh).collect();
        let markers: Vec<&(FormId, [f32; 3], f32)> = objects.iter().filter(|o| refs.get(o.0).is_some_and(|p| data.idle_markers.contains_key(&p.base))).filter(fresh).collect();
        let spot = if self.allowed.wandering { self.wander_spot(nav, at) } else { None };
        // Energy: the less of it, the more sitting; the more, the more walking about.
        let e = self.energy;
        let lean = [1.5 - e, 0.5 + e, 0.5 + e];
        let open = [self.allowed.furniture && !seats.is_empty(), self.allowed.idle_markers && !markers.is_empty(), spot.is_some()];
        let mut w = [0.0f32; 3];
        for k in 0..3 {
            if open[k] {
                w[k] = self.weights[k] * lean[k];
            }
        }
        // A kind just chosen has weight zero; it is still taken when nothing else is open.
        if w.iter().sum::<f32>() <= 0.0 {
            for k in 0..3 {
                if open[k] {
                    w[k] = 1.0;
                }
            }
        }
        let total: f32 = w.iter().sum();
        if total <= 0.0 {
            return Choice { doing: Doing::Standing, target: None, heading: None, until: Until::Time(game.time + 10.0) };
        }
        let mut roll = self.rng.next() * total;
        let kind = (0..3).find(|k| {
            roll -= w[*k];
            roll < 0.0
        });
        let kind = kind.unwrap_or_else(|| (0..3).rfind(|k| w[*k] > 0.0).unwrap_or(WANDER));
        for k in 0..3 {
            self.weights[k] = if k == kind { 0.0 } else { (self.weights[k] + 0.5).min(1.0) };
        }
        // Within a kind, nearer objects are likelier.
        let jitter = self.rng.range(0.75, 1.25) as f64;
        let chance = self.rng.next();
        let wander_stay = self.rng.range(4.0, 12.0) as f64;
        let pick = |list: &[&(FormId, [f32; 3], f32)]| -> (FormId, [f32; 3], f32) {
            let score = |o: &&(FormId, [f32; 3], f32)| 1.0 / (1.0 + hdist(o.1, at.pos) / 256.0);
            let mut roll = chance * list.iter().map(score).sum::<f32>();
            **list
                .iter()
                .find(|o| {
                    roll -= score(o);
                    roll < 0.0
                })
                .unwrap_or(&list[list.len() - 1])
        };
        match kind {
            FURNITURE => {
                let o = pick(&seats);
                let seconds = (45.0 + 120.0 * (1.0 - e) as f64) * jitter;
                Choice { doing: Doing::Sitting(o.0), target: place(o.1), heading: None, until: Until::Time(game.time + seconds) }
            }
            IDLE_MARKER => {
                let o = pick(&markers);
                // The marker's idle timer times its idles, when it has them.
                let listed = refs.get(o.0).and_then(|p| data.idle_markers.get(&p.base)).map(|c| c.timer as f64 * c.idles.len().max(1) as f64).filter(|s| *s > 0.0);
                let seconds = listed.unwrap_or(30.0).clamp(15.0, 90.0) * jitter;
                Choice { doing: Doing::AtIdleMarker(o.0), target: place(o.1), heading: Some(o.2), until: Until::Time(game.time + seconds) }
            }
            _ => Choice { doing: Doing::Wandering, target: spot, heading: None, until: Until::Time(game.time + wander_stay) },
        }
    }

    /// The behaviour of `choice` begins now (the actor is at its place).
    /// What the actor does, as GetCurrentAIProcedure reads it (`fnv_game::ai::SandboxPhase`;
    /// the codes, docs/spec-notes/packages.md §4, procedure codes): up from furniture until the next
    /// thing begins; walking to a spot of the area as wandering, to something to use as
    /// such; else what it does.
    pub fn phase(&self) -> SandboxPhase {
        if self.got_up {
            return SandboxPhase::GotUp;
        }
        let doing = match &self.pending {
            Some(c) if !matches!(c.doing, Doing::Standing | Doing::Wandering) => return SandboxPhase::ToObject,
            Some(c) => c.doing,
            None => self.doing,
        };
        match doing {
            Doing::Standing => SandboxPhase::Standing,
            Doing::Wandering => SandboxPhase::Wandering,
            Doing::Sitting(_) => SandboxPhase::Sitting,
            Doing::AtIdleMarker(_) => SandboxPhase::AtIdleMarker,
            Doing::Sleeping(_) => SandboxPhase::Sleeping,
            Doing::Eating(_) => SandboxPhase::Eating,
        }
    }

    pub(crate) fn begin(&mut self, game: &Game, choice: &Choice) {
        self.pending = None;
        self.got_up = false;
        self.doing = choice.doing;
        self.last_object = choice.doing.object().or(self.last_object);
        if let Some(object) = choice.doing.object() {
            self.failed_objects.remove(&object);
        }
        // The time a timed behaviour lasts counts from getting there.
        self.until = match choice.until {
            Until::Time(t) if choice.target.is_some() => Until::Time(game.time + (t - self.chosen_at).max(1.0)),
            u => u,
        };
    }

    /// Nothing came of a choice (no way to its object): stand a little, then choose again;
    /// the object waits out its next delay before it can be chosen.
    pub(crate) fn failed(&mut self, game: &Game, choice: &Choice) {
        self.pending = None;
        self.got_up = false;
        if let Some(object) = choice.doing.object() {
            let stage = self.failed_objects.get(&object).map_or(0, |f| (f.stage + 1).min(RETRY_DELAYS.len() - 1));
            self.failed_objects.insert(object, FailedObject { retry_at: game.time + RETRY_DELAYS[stage], stage });
        }
        self.doing = Doing::Standing;
        self.until = Until::Time(game.time + 5.0);
    }
}
