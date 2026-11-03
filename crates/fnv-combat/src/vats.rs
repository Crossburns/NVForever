//! VATS mode's headless rules (docs/design/vats-mode.md): NVForever's own Dead Eye-style
//! VATS, not the original's [design: combat is not a parity target, combat.md §1].
//!
//! - [`shot_ap_cost`]: a shot's AP cost from the records (§7.1): the weapon's "Override -
//!   Action Points" (WEAP `DNAM`) when it has one, else the attack setting of its
//!   animation type ([`ApAttack`], `fActionPointsAttack…`) [hypothesis: which setting each
//!   type uses, from the names].
//! - [`MarkQueue`]: the marks in the order they were made, each reserving its AP; no more
//!   than the AP left and the rounds loaded allow (§7.2).
//! - [`Volley`]: the marked shots leaving one after another at a fixed interval, then the
//!   end of the mode a moment after the last (§9.1); with bursts, each mark's rounds in
//!   turn (§16.1).
//! - [`AttackStyle`]: how a weapon's marks are spent (§16): a shot, a burst
//!   ([`burst_shots`]), a swing beside the mark ([`MeleeRun`]) or a throw ([`lob_dir`]).
//! - [`restored_damage`]: spent AP coming back outside the mode (§7.2) [hypothesis].

use fnv_sim::FormId;
use serde::Serialize;

use crate::data::WeaponStats;
use crate::vec::{self, V3};

/// The attack AP cost of each kind of weapon (`fActionPointsAttack…`), for weapons
/// without their own.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ApAttack {
    pub pistol: f32,
    pub rifle: f32,
    pub handle: f32,
    pub launcher: f32,
    pub one_hand_melee: f32,
    pub two_hand_melee: f32,
    pub unarmed: f32,
    pub grenade: f32,
    pub mine: f32,
}

/// The AP one shot of `w` costs: its override when the record sets the flag, else its
/// animation type's attack setting. An unknown type costs as a rifle [design].
pub fn shot_ap_cost(w: &WeaponStats, a: &ApAttack) -> f32 {
    if let Some(ap) = w.action_points {
        return ap.max(0.0);
    }
    match w.anim_type.as_deref() {
        Some("Hand to Hand") => a.unarmed,
        Some("Melee (1 Hand)") => a.one_hand_melee,
        Some("Melee (2 Hand)") => a.two_hand_melee,
        Some("Pistol - Ballistic (1 Hand)") | Some("Pistol - Energy (1 Hand)") => a.pistol,
        Some("Handle (2 Hand)") => a.handle,
        Some("Launcher (2 Hand)") => a.launcher,
        Some("Grenade Throw (1 Hand)") | Some("Thrown (1 Hand)") => a.grenade,
        Some("Land Mine (1 Hand)") | Some("Mine Drop (1 Hand)") => a.mine,
        _ => a.rifle,
    }
}

/// One marked shot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Mark {
    /// The actor's reference.
    pub target: FormId,
    /// The body part (its index in the actor's BPTD), `None` for the whole actor.
    pub part: Option<u8>,
    /// The AP it reserves.
    pub cost: f32,
}

/// Why a mark was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Refusal {
    /// The AP left after the reservations does not cover another shot.
    ActionPoints,
    /// The rounds loaded do not cover another shot.
    Rounds,
}

impl Refusal {
    /// The HUD's line.
    pub fn message(self) -> &'static str {
        match self {
            Refusal::ActionPoints => "Not enough action points.",
            Refusal::Rounds => "No more rounds loaded.",
        }
    }
}

/// Lets two reservations that add up to the AP exactly pass despite rounding.
const AP_EPSILON: f32 = 1e-3;

/// The marks of one stay in VATS mode, in order.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MarkQueue {
    marks: Vec<Mark>,
    /// AP per shot.
    pub cost: f32,
    /// Rounds one shot takes from the magazine (the weapon's Ammo Use, at least 1).
    pub rounds_per_shot: u32,
    /// Rounds in the magazine.
    pub rounds: u32,
}

impl MarkQueue {
    pub fn new(cost: f32, rounds_per_shot: u32, rounds: u32) -> MarkQueue {
        MarkQueue { marks: Vec::new(), cost: cost.max(0.0), rounds_per_shot: rounds_per_shot.max(1), rounds }
    }

    /// Whether one more mark fits with `ap_now` AP (current, nothing spent yet): the AP
    /// is asked first.
    pub fn check(&self, ap_now: f32) -> Result<(), Refusal> {
        if self.reserved() + self.cost > ap_now + AP_EPSILON {
            return Err(Refusal::ActionPoints);
        }
        if self.rounds_needed() + self.rounds_per_shot > self.rounds {
            return Err(Refusal::Rounds);
        }
        Ok(())
    }

    /// Mark `target` (on `part`): the number of marks after it, or why not.
    pub fn push(&mut self, target: FormId, part: Option<u8>, ap_now: f32) -> Result<usize, Refusal> {
        self.check(ap_now)?;
        self.marks.push(Mark { target, part, cost: self.cost });
        Ok(self.marks.len())
    }

    /// Take the last mark back.
    pub fn pop(&mut self) -> Option<Mark> {
        self.marks.pop()
    }

    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    pub fn len(&self) -> usize {
        self.marks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// The AP the marks hold.
    pub fn reserved(&self) -> f32 {
        self.marks.iter().map(|m| m.cost).sum()
    }

    /// The rounds the marks need.
    pub fn rounds_needed(&self) -> u32 {
        self.marks.len() as u32 * self.rounds_per_shot
    }

    /// The most marks `ap_now` and the rounds allow at all.
    pub fn most(&self, ap_now: f32) -> u32 {
        let by_rounds = self.rounds / self.rounds_per_shot;
        let by_ap = if self.cost > 0.0 { ((ap_now + AP_EPSILON) / self.cost).floor().max(0.0) as u32 } else { u32::MAX };
        by_rounds.min(by_ap)
    }
}

/// How a weapon's marks are spent (§16) [design].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum AttackStyle {
    /// One shot a mark (a shotgun's is all its pellets).
    Single,
    /// One burst a mark: `shots` rounds `interval` game seconds apart (§16.1).
    Burst { shots: u32, interval: f32 },
    /// Close in and swing once a mark (§16.2).
    Melee,
    /// One throw a mark (§16.3).
    Thrown,
}

/// A burst lasts about this long, game seconds (§16.1) [design].
pub const BURST_TIME: f32 = 0.4;
/// The fewest and the most rounds of a burst [design].
pub const BURST_MIN: u32 = 3;
pub const BURST_MAX: u32 = 8;

/// The rounds of one burst of an automatic weapon firing `fire_rate` shots a second:
/// [`BURST_TIME`] of fire, from [`BURST_MIN`] to [`BURST_MAX`].
pub fn burst_shots(fire_rate: f32) -> u32 {
    if fire_rate.is_nan() || fire_rate <= 0.0 {
        return BURST_MIN;
    }
    ((fire_rate * BURST_TIME).round() as u32).clamp(BURST_MIN, BURST_MAX)
}

/// Game seconds between a burst's rounds: the weapon's own rate, [`BURST_TIME`] shared
/// out without one.
pub fn burst_interval(fire_rate: f32) -> f32 {
    if fire_rate > 0.0 {
        1.0 / fire_rate
    } else {
        BURST_TIME / BURST_MIN as f32
    }
}

impl AttackStyle {
    /// The style of `w`: `melee` (melee or unarmed, no projectile) and `thrown` (it spends
    /// itself) as combat has the weapon in hand; else a gun, bursting when automatic.
    pub fn of(w: &WeaponStats, melee: bool, thrown: bool) -> AttackStyle {
        if melee {
            AttackStyle::Melee
        } else if thrown {
            AttackStyle::Thrown
        } else if w.automatic {
            AttackStyle::Burst { shots: burst_shots(w.fire_rate), interval: burst_interval(w.fire_rate) }
        } else {
            AttackStyle::Single
        }
    }

    /// Shots (projectile volleys) one mark fires: a burst's rounds, else 1 (a swing fires
    /// none, but is one attack).
    pub fn shots_per_mark(&self) -> u32 {
        match self {
            AttackStyle::Burst { shots, .. } => *shots,
            _ => 1,
        }
    }

    /// The name in the log's `on` line; `None` for a single shot (its line is unchanged).
    pub fn label(&self) -> Option<String> {
        match self {
            AttackStyle::Single => None,
            AttackStyle::Burst { shots, .. } => Some(format!("burst of {shots}")),
            AttackStyle::Melee => Some("melee".into()),
            AttackStyle::Thrown => Some("thrown".into()),
        }
    }
}

/// What a [`Volley`] step brings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolleyEvent {
    /// Fire round `round` of the mark at index `mark` now (round 0 starts the mark: its AP
    /// is spent then).
    Shot { mark: usize, round: u32 },
    /// All are fired and the pause after the last is over: the mode ends.
    Done,
}

/// The marked shots leaving at a fixed interval: the first at once, at most one a step
/// (so each shot gets its own frame), then the end `after` seconds after the last. With
/// bursts, each mark's rounds leave `burst_interval` apart and the next mark comes
/// `interval` after the last round.
#[derive(Debug, Clone, PartialEq)]
pub struct Volley {
    /// Marks.
    pub shots: usize,
    /// Marks started.
    pub fired: usize,
    rounds: u32,
    /// The round of the mark being fired that comes next (0: the next mark's first).
    round: u32,
    interval: f32,
    burst_interval: f32,
    after: f32,
    /// Seconds since the last shot (the start, before the first).
    since: f32,
    done: bool,
}

impl Volley {
    /// One shot a mark.
    pub fn new(shots: usize, interval: f32, after: f32) -> Volley {
        Volley::bursts(shots, 1, interval, interval, after)
    }

    /// `rounds` shots a mark, `burst_interval` apart.
    pub fn bursts(marks: usize, rounds: u32, interval: f32, burst_interval: f32, after: f32) -> Volley {
        Volley {
            shots: marks,
            fired: 0,
            rounds: rounds.max(1),
            round: 0,
            interval: interval.max(0.0),
            burst_interval: burst_interval.max(0.0),
            after: after.max(0.0),
            since: f32::INFINITY,
            done: false,
        }
    }

    pub fn step(&mut self, dt: f32) -> Option<VolleyEvent> {
        if self.done {
            return None;
        }
        self.since += dt.max(0.0);
        if self.round > 0 || self.fired < self.shots {
            let wait = if self.round == 0 { self.interval } else { self.burst_interval };
            if self.since < wait {
                return None;
            }
            self.since = 0.0;
            if self.round == 0 {
                self.fired += 1;
            }
            let e = VolleyEvent::Shot { mark: self.fired - 1, round: self.round };
            self.round = (self.round + 1) % self.rounds;
            return Some(e);
        }
        if self.since >= self.after || self.shots == 0 {
            self.done = true;
            return Some(VolleyEvent::Done);
        }
        None
    }

    /// Every mark's every round has left.
    pub fn all_fired(&self) -> bool {
        self.fired >= self.shots && self.round == 0
    }

    pub fn done(&self) -> bool {
        self.done
    }
}

/// What a [`MeleeRun`] step asks of the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeleeAction {
    /// Bring the player to mark `i` (the engine teleports them beside it, §16.2).
    Approach(usize),
    /// Swing at mark `i` now (one tap of Use; spend its AP).
    Swing(usize),
    /// A swing is playing: keep the view on its mark.
    Wait(usize),
    /// The pause after the last swing is over: the mode ends.
    Done,
}

/// The limits of a [`MeleeRun`], game seconds (§16.2) [design].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeLimits {
    /// The longest wait for the player to be beside a mark before swinging anyway.
    pub approach: f32,
    /// The longest a swing is waited for.
    pub swing: f32,
    /// From the last swing's end to the end of the mode.
    pub after: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum MeleeStage {
    Approach,
    /// `seen`: the swing was seen playing.
    Swing { seen: bool },
    After,
    Done,
}

/// The marks of a melee weapon spent in turn (§16.2): bring the player to each until it is
/// within reach (or that takes too long), one swing, wait for it to end, the next.
#[derive(Debug, Clone, PartialEq)]
pub struct MeleeRun {
    pub marks: usize,
    /// The mark being closed on or swung at.
    pub current: usize,
    /// Swings started.
    pub swung: usize,
    stage: MeleeStage,
    t: f32,
    limits: MeleeLimits,
}

impl MeleeRun {
    pub fn new(marks: usize, limits: MeleeLimits) -> MeleeRun {
        let stage = if marks == 0 { MeleeStage::After } else { MeleeStage::Approach };
        MeleeRun { marks, current: 0, swung: 0, stage, t: 0.0, limits }
    }

    /// Advance by `dt`: `close` says the current mark is within reach now, `swinging`
    /// that the player's swing is playing.
    pub fn step(&mut self, dt: f32, close: bool, swinging: bool) -> Option<MeleeAction> {
        let dt = dt.max(0.0);
        match self.stage {
            MeleeStage::Approach => {
                if close || self.t >= self.limits.approach {
                    self.stage = MeleeStage::Swing { seen: false };
                    self.t = 0.0;
                    self.swung += 1;
                    return Some(MeleeAction::Swing(self.current));
                }
                self.t += dt;
                Some(MeleeAction::Approach(self.current))
            }
            MeleeStage::Swing { seen } => {
                self.t += dt;
                let seen = seen || swinging;
                if (seen && !swinging) || self.t >= self.limits.swing {
                    self.current += 1;
                    self.t = 0.0;
                    self.stage = if self.current < self.marks { MeleeStage::Approach } else { MeleeStage::After };
                } else {
                    self.stage = MeleeStage::Swing { seen };
                }
                Some(MeleeAction::Wait(self.current.min(self.marks.saturating_sub(1))))
            }
            MeleeStage::After => {
                self.t += dt;
                if self.t >= self.limits.after {
                    self.stage = MeleeStage::Done;
                    return Some(MeleeAction::Done);
                }
                Some(MeleeAction::Wait(self.marks.saturating_sub(1)))
            }
            MeleeStage::Done => None,
        }
    }

    /// Every mark has been swung at and its swing is over.
    pub fn all_fired(&self) -> bool {
        matches!(self.stage, MeleeStage::After | MeleeStage::Done)
    }

    pub fn done(&self) -> bool {
        self.stage == MeleeStage::Done
    }
}

/// The direction to throw from `from` at `speed` (units/s) under `gravity` (units/s²,
/// downwards) to land on `to`: the low arc (§16.3). `None` when `to` is out of reach;
/// straight at it without gravity.
pub fn lob_dir(from: V3, to: V3, speed: f32, gravity: f32) -> Option<V3> {
    let d = vec::sub(to, from);
    let flat = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if gravity <= 0.0 || flat < 1e-3 {
        return (vec::len(d) > 1e-6).then(|| vec::normalize(d));
    }
    let (v2, h) = (speed * speed, d[2]);
    let disc = v2 * v2 - gravity * (gravity * flat * flat + 2.0 * h * v2);
    if disc < 0.0 || speed <= 0.0 {
        return None;
    }
    let tan = (v2 - disc.sqrt()) / (gravity * flat);
    let (c, s) = (1.0 / (1.0 + tan * tan).sqrt(), tan / (1.0 + tan * tan).sqrt());
    Some([d[0] / flat * c, d[1] / flat * c, s])
}

/// Thrown at 45° towards `to`: for a mark out of the throw's reach.
pub fn lob_far(from: V3, to: V3) -> V3 {
    let d = vec::sub(to, from);
    let flat = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
    let k = std::f32::consts::FRAC_1_SQRT_2;
    [d[0] / flat * k, d[1] / flat * k, k]
}

/// How far a throw at `speed` under `gravity` reaches on flat ground (45°): speed² /
/// gravity; unlimited without gravity.
pub fn throw_reach(speed: f32, gravity: f32) -> f32 {
    if gravity > 0.0 {
        speed * speed / gravity
    } else {
        f32::INFINITY
    }
}

/// The Action Points damage left after `dt` seconds of regeneration: `rate` × `max` a
/// second comes back, down to none [hypothesis: the role of `fActionPointsRestoreRate`].
pub fn restored_damage(damage: f32, max: f32, rate: f32, dt: f32) -> f32 {
    (damage - max.max(0.0) * rate.max(0.0) * dt.max(0.0)).max(0.0)
}

/// The VATS settings the hit chance reads (docs/design/vats-mode.md §17.1): from the load
/// order, the GECK wiki's defaults where it has none.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HitTuning {
    /// `fVATSMaxChance`: the highest chance, percent.
    pub max_chance: f32,
    /// `fVATSHitChanceMult`: on the whole chance.
    pub hit_chance_mult: f32,
    /// `fVATSSkillFactor`: read, not used: the skill acts through the cone (§17.2).
    pub skill_factor: f32,
    /// `fVATSDistanceFactor`: read, not used: the running game reports 0 and the fall
    /// with distance is fitted ([`FALLOFF`]).
    pub distance_factor: f32,
    /// `fVATSSpreadMult`: the gun's spread in VATS.
    pub spread_mult: f32,
    /// `iVATSConcentratedFireBonus`: percent a mark gains for each earlier mark on the
    /// same part, with Concentrated Fire.
    pub concentrated_fire_bonus: f32,
}

impl Default for HitTuning {
    /// The GECK wiki's defaults (VATS Settings).
    fn default() -> HitTuning {
        HitTuning { max_chance: 95.0, hit_chance_mult: 1.0, skill_factor: 1.0, distance_factor: 2.0, spread_mult: 0.5, concentrated_fire_bonus: 5.0 }
    }
}

/// The chance next to the shooter, as a multiple of the base: the spread has not opened
/// yet, so nearly every part reaches the cap (§17.2) [fitted to observations].
pub const CLOSE_MULT: f32 = 7.58;
/// How wide the spread may open at a torso, units, before the chance has fallen to
/// half its close value (§17.2) [fitted to observations].
pub const FOCUS: f32 = 3.3;
/// How steeply the chance falls once the spread is wider than the part (§17.2)
/// [fitted to observations].
pub const FALLOFF: f32 = 1.29;
/// The parts' sizes against a torso's, for [`FOCUS`] (§17.2) [fitted to observations].
pub const SIZE_TORSO: f32 = 1.0;
pub const SIZE_HEAD: f32 = 0.89;
pub const SIZE_ARM: f32 = 0.84;
pub const SIZE_LEG: f32 = 0.91;
pub const SIZE_HELD_WEAPON: f32 = 0.32;
/// A melee mark's skill share, at skill 0 and what 100 skill adds: the player is put
/// beside the mark, so there is no range; melee was not measured (§17.2) [design].
pub const MELEE_SKILL_BASE: f32 = 0.5;
pub const MELEE_SKILL_SPAN: f32 = 1.5;
/// The to-hit chance of an actor marked whole (no body parts), percent [design: as a
/// large part, a dog's torso is 75].
pub const WHOLE_TO_HIT: f32 = 75.0;
/// The to-hit chance of a held weapon whose WEAP gives none, percent [design].
pub const HELD_WEAPON_TO_HIT: f32 = 30.0;

/// A body part's size for the range term, by the condition actor value its BPTD part
/// names (§17.2): the head and brain, the arms, the legs; anything else as a torso.
pub fn part_size(actor_value: Option<&str>) -> f32 {
    match actor_value.map(str::to_ascii_lowercase).as_deref() {
        Some("perceptioncondition" | "braincondition") => SIZE_HEAD,
        Some("leftattackcondition" | "rightattackcondition") => SIZE_ARM,
        Some("leftmobilitycondition" | "rightmobilitycondition") => SIZE_LEG,
        _ => SIZE_TORSO,
    }
}

/// What one mark's chance is worked out from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HitInputs {
    /// The part's base chance, percent: its BPTD To Hit Chance, [`WHOLE_TO_HIT`], or the
    /// held weapon's Base VATS To-Hit Chance.
    pub base: f32,
    /// The part's size against a torso's ([`part_size`], [`SIZE_HELD_WEAPON`]).
    pub size: f32,
    /// The weapon's skill, 0–100: only a melee mark reads it here (a gun's acts
    /// through its cone).
    pub skill: f32,
    /// From the eye to the mark, units.
    pub distance: f32,
    /// The cone the shot would leave in (half-angle, degrees), before `fVATSSpreadMult`;
    /// `None` for a melee mark (the player is put beside it). The skill acts here
    /// (`spread::vats_actor_spread`).
    pub cone_deg: Option<f32>,
}

/// The range's share: 1 when the spread at the target is narrow, falling as it opens past
/// [`FOCUS`] times the part's size; 1 for a melee mark.
pub fn range_term(distance: f32, cone_deg: Option<f32>, size: f32, t: &HitTuning) -> f32 {
    let Some(cone) = cone_deg else { return 1.0 };
    let half = (cone.max(0.0) * t.spread_mult.max(0.0)).min(89.0).to_radians();
    let open = distance.max(0.0) * half.tan();
    1.0 / (1.0 + (open / (FOCUS * size.max(0.01))).powf(FALLOFF))
}

/// The chance before the perks and the cap, percent.
pub fn raw_chance(i: &HitInputs, t: &HitTuning) -> f32 {
    let close = match i.cone_deg {
        Some(_) => CLOSE_MULT,
        None => MELEE_SKILL_BASE + MELEE_SKILL_SPAN * i.skill.clamp(0.0, 100.0) / 100.0,
    };
    i.base.max(0.0) * close * range_term(i.distance, i.cone_deg, i.size, t) * t.hit_chance_mult
}

/// The chance shown and rolled, percent: the perks' Calculate To Hit Chance on the raw
/// chance, Concentrated Fire's bonus for `earlier` marks on the same part when the
/// player has it, between 0 and `fVATSMaxChance`.
pub fn final_chance(raw: f32, perks: impl Fn(f32) -> f32, concentrated: Option<u32>, t: &HitTuning) -> f32 {
    let bonus = concentrated.map_or(0.0, |n| t.concentrated_fire_bonus * n as f32);
    (perks(raw) + bonus).clamp(0.0, t.max_chance.max(0.0))
}

/// A roll `u` in [0, 1) hits at `chance` percent.
pub fn rolled_hit(chance: f32, u: f32) -> bool {
    u * 100.0 < chance
}

/// How far beside a missed mark a shot passes, units: at least [`MISS_MIN`], up to
/// [`MISS_MIN`] + [`MISS_SPAN`] [design: wide of a torso or a head].
pub const MISS_MIN: f32 = 45.0;
pub const MISS_SPAN: f32 = 35.0;
/// How far from a missed mark a throw lands, units [design].
pub const THROW_MISS_MIN: f32 = 120.0;
pub const THROW_MISS_SPAN: f32 = 120.0;

/// The point a missed shot goes through: beside `point` across the line from `eye`, to a
/// side or above, never below (where the legs are) [design]. `u1`, `u2` in [0, 1).
pub fn miss_point(eye: V3, point: V3, u1: f32, u2: f32) -> V3 {
    let f = vec::normalize(vec::sub(point, eye));
    let up = if f[2].abs() < 0.99 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let right = vec::normalize(vec::cross(f, up));
    let up = vec::cross(right, f);
    // From 30° below the right through the top to 30° below the left.
    let phi = (-30.0 + 240.0 * u2.clamp(0.0, 1.0)).to_radians();
    let off = MISS_MIN + MISS_SPAN * u1.clamp(0.0, 1.0);
    vec::add(point, vec::scale(vec::add(vec::scale(right, phi.cos()), vec::scale(up, phi.sin())), off))
}

/// Where a missed throw is aimed: off `point` across the ground in any direction.
pub fn throw_miss_point(point: V3, u1: f32, u2: f32) -> V3 {
    let phi = std::f32::consts::TAU * u2;
    let off = THROW_MISS_MIN + THROW_MISS_SPAN * u1.clamp(0.0, 1.0);
    [point[0] + off * phi.cos(), point[1] + off * phi.sin(), point[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    const AP: ApAttack =
        ApAttack { pistol: 20.0, rifle: 25.0, handle: 20.0, launcher: 40.0, one_hand_melee: 15.0, two_hand_melee: 30.0, unarmed: 22.0, grenade: 35.0, mine: 10.0 };

    fn weapon(anim: &str, over: Option<f32>) -> WeaponStats {
        WeaponStats { anim_type: Some(anim.into()), action_points: over, ..Default::default() }
    }

    #[test]
    fn the_override_wins_else_the_animation_type() {
        // WeapNV357Revolver: Override - Action Points 26 [observed].
        assert_eq!(shot_ap_cost(&weapon("Pistol - Ballistic (1 Hand)", Some(26.0)), &AP), 26.0);
        assert_eq!(shot_ap_cost(&weapon("Pistol - Ballistic (1 Hand)", None), &AP), 20.0);
        assert_eq!(shot_ap_cost(&weapon("Pistol - Energy (1 Hand)", None), &AP), 20.0);
        assert_eq!(shot_ap_cost(&weapon("Rifle - Automatic (2 Hand)", None), &AP), 25.0);
        assert_eq!(shot_ap_cost(&weapon("Launcher (2 Hand)", None), &AP), 40.0);
        assert_eq!(shot_ap_cost(&weapon("Thrown (1 Hand)", None), &AP), 35.0);
        assert_eq!(shot_ap_cost(&weapon("Mine Drop (1 Hand)", None), &AP), 10.0);
        assert_eq!(shot_ap_cost(&WeaponStats::default(), &AP), 25.0, "no type: as a rifle");
    }

    #[test]
    fn marks_reserve_ap_until_it_runs_out() {
        // 92 AP, 26 a shot: three marks (78), the fourth would need 104.
        let mut q = MarkQueue::new(26.0, 1, 6);
        let t = FormId(0x10);
        assert_eq!(q.push(t, Some(0), 92.0), Ok(1));
        assert_eq!(q.push(t, Some(1), 92.0), Ok(2));
        assert_eq!(q.push(FormId(0x11), None, 92.0), Ok(3));
        assert_eq!(q.reserved(), 78.0);
        assert_eq!(q.push(t, None, 92.0), Err(Refusal::ActionPoints));
        assert_eq!(q.len(), 3);
        assert_eq!(q.most(92.0), 3);
    }

    #[test]
    fn marks_stop_at_the_rounds_loaded() {
        // A .357 with 6 loaded and AP for more: six marks at most (the owner's example).
        let mut q = MarkQueue::new(26.0, 1, 6);
        for i in 0..6 {
            assert_eq!(q.push(FormId(0x10), None, 1000.0), Ok(i + 1));
        }
        assert_eq!(q.push(FormId(0x10), None, 1000.0), Err(Refusal::Rounds));
        assert_eq!(q.most(1000.0), 6);
        // Two rounds a shot: half as many.
        let q = MarkQueue::new(10.0, 2, 5);
        assert_eq!(q.most(1000.0), 2);
    }

    #[test]
    fn exact_ap_is_enough_and_ap_is_asked_first() {
        let mut q = MarkQueue::new(26.0, 1, 1);
        assert_eq!(q.push(FormId(1), None, 26.0), Ok(1));
        // Out of both: the AP is the reason given.
        assert_eq!(q.check(26.0), Err(Refusal::ActionPoints));
        assert_eq!(q.check(100.0), Err(Refusal::Rounds));
    }

    #[test]
    fn unmarking_frees_the_last_mark_and_its_ap() {
        let mut q = MarkQueue::new(20.0, 1, 6);
        q.push(FormId(1), Some(2), 50.0).unwrap();
        q.push(FormId(2), Some(0), 50.0).unwrap();
        assert_eq!(q.push(FormId(3), None, 50.0), Err(Refusal::ActionPoints));
        assert_eq!(q.pop(), Some(Mark { target: FormId(2), part: Some(0), cost: 20.0 }));
        assert_eq!(q.reserved(), 20.0);
        assert_eq!(q.push(FormId(3), None, 50.0), Ok(2));
        assert_eq!(q.marks().iter().map(|m| m.target).collect::<Vec<_>>(), vec![FormId(1), FormId(3)]);
        q.pop();
        q.pop();
        assert!(q.pop().is_none() && q.is_empty() && q.reserved() == 0.0);
    }

    #[test]
    fn the_volley_fires_in_order_at_the_interval_then_ends() {
        let mut v = Volley::new(3, 0.12, 0.3);
        let mut events = Vec::new();
        let mut t = 0.0;
        while !v.done() && t < 5.0 {
            if let Some(e) = v.step(0.01) {
                events.push((e, (t * 100.0f32).round() / 100.0));
            }
            t += 0.01;
        }
        let shots: Vec<_> = events.iter().filter_map(|(e, t)| matches!(e, VolleyEvent::Shot { .. }).then_some(*t)).collect();
        assert_eq!(events.iter().filter_map(|(e, _)| if let VolleyEvent::Shot { mark, .. } = e { Some(*mark) } else { None }).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(shots[0], 0.0, "the first shot at once");
        assert!((shots[1] - 0.12).abs() < 0.015 && (shots[2] - 0.24).abs() < 0.025, "{shots:?}");
        let done = events.last().unwrap();
        assert_eq!(done.0, VolleyEvent::Done);
        assert!((done.1 - 0.54).abs() < 0.03, "{done:?}");
    }

    #[test]
    fn the_volley_fires_once_a_step_however_long_the_step() {
        let mut v = Volley::new(2, 0.12, 0.3);
        assert_eq!(v.step(1.0), Some(VolleyEvent::Shot { mark: 0, round: 0 }));
        assert_eq!(v.step(1.0), Some(VolleyEvent::Shot { mark: 1, round: 0 }));
        assert_eq!(v.step(1.0), Some(VolleyEvent::Done));
        assert_eq!(v.step(1.0), None);
    }

    #[test]
    fn spent_ap_comes_back_at_the_rate() {
        // 92 AP at 0.06 a second: 5.52 AP a second.
        assert!((restored_damage(52.0, 92.0, 0.06, 1.0) - 46.48).abs() < 1e-4);
        assert_eq!(restored_damage(3.0, 92.0, 0.06, 1.0), 0.0);
        assert_eq!(restored_damage(0.0, 92.0, 0.06, 1.0), 0.0);
    }

    fn auto(anim: &str, ap: f32, fire_rate: f32) -> WeaponStats {
        WeaponStats { anim_type: Some(anim.into()), action_points: Some(ap), automatic: true, fire_rate, ammo_use: 1, ..Default::default() }
    }

    #[test]
    fn the_style_follows_the_weapon() {
        // Observed fire rates: the 10mm SMG 9, the assault carbine 12, the minigun 20.
        let smg = auto("Pistol - Ballistic (1 Hand)", 22.0, 9.0);
        assert_eq!(AttackStyle::of(&smg, false, false), AttackStyle::Burst { shots: 4, interval: 1.0 / 9.0 });
        assert_eq!(AttackStyle::of(&auto("Rifle - Automatic (2 Hand)", 20.0, 12.0), false, false).shots_per_mark(), 5);
        assert_eq!(AttackStyle::of(&auto("Handle (2 Hand)", 30.0, 20.0), false, false).shots_per_mark(), 8);
        assert_eq!(AttackStyle::of(&weapon("Pistol - Ballistic (1 Hand)", Some(26.0)), false, false), AttackStyle::Single);
        assert_eq!(AttackStyle::of(&weapon("Melee (1 Hand)", Some(20.0)), true, false), AttackStyle::Melee);
        assert_eq!(AttackStyle::of(&weapon("Grenade Throw (1 Hand)", Some(24.0)), false, true), AttackStyle::Thrown);
        assert_eq!(AttackStyle::Single.label(), None);
        assert_eq!(AttackStyle::Melee.shots_per_mark(), 1);
    }

    #[test]
    fn bursts_are_three_to_eight_rounds() {
        assert_eq!(burst_shots(0.0), BURST_MIN);
        assert_eq!(burst_shots(f32::NAN), BURST_MIN);
        assert_eq!(burst_shots(2.0), BURST_MIN);
        assert_eq!(burst_shots(30.0), BURST_MAX);
        assert_eq!(burst_shots(12.0), 5);
        assert!((burst_interval(0.0) * BURST_MIN as f32 - BURST_TIME).abs() < 1e-6);
    }

    #[test]
    fn a_burst_mark_reserves_one_attack_and_a_whole_burst_of_rounds() {
        // The assault carbine: 20 AP a burst of 5, 24 loaded, 92 AP: 4 bursts by both.
        let mut q = MarkQueue::new(20.0, 5, 24);
        for k in 1..=4 {
            assert_eq!(q.push(FormId(0x10), None, 92.0), Ok(k));
        }
        assert_eq!(q.reserved(), 80.0);
        assert_eq!(q.rounds_needed(), 20);
        // 4 rounds left: no whole burst, and 12 AP left: no attack either; the AP is told.
        assert_eq!(q.push(FormId(0x10), None, 92.0), Err(Refusal::ActionPoints));
        assert_eq!(q.push(FormId(0x10), None, 1000.0), Err(Refusal::Rounds));
        // The minigun: 30 AP a burst of 8 from 240: three marks by the AP.
        assert_eq!(MarkQueue::new(30.0, 8, 240).most(92.0), 3);
    }

    #[test]
    fn melee_and_thrown_marks_reserve_their_attack_cost() {
        // A machete (20 AP, no rounds) with 92 AP: four swings.
        let mut q = MarkQueue::new(20.0, 1, u32::MAX / 2);
        assert_eq!(q.most(92.0), 4);
        for k in 1..=4 {
            assert_eq!(q.push(FormId(1), Some(0), 92.0), Ok(k));
        }
        assert_eq!(q.push(FormId(1), None, 92.0), Err(Refusal::ActionPoints));
        // Three frag grenades carried (24 AP each): three throws, the fourth has nothing.
        let mut q = MarkQueue::new(24.0, 1, 3);
        for k in 1..=3 {
            assert_eq!(q.push(FormId(1), None, 200.0), Ok(k));
        }
        assert_eq!(q.push(FormId(1), None, 200.0), Err(Refusal::Rounds));
        assert_eq!(q.reserved(), 72.0);
    }

    #[test]
    fn a_burst_volley_fires_each_marks_rounds_then_the_next_mark() {
        let mut v = Volley::bursts(2, 3, 0.2, 0.05, 0.3);
        let mut events = Vec::new();
        let mut t = 0.0f32;
        while !v.done() && t < 5.0 {
            if let Some(e) = v.step(0.01) {
                events.push((e, t));
            }
            t += 0.01;
        }
        let shots: Vec<(usize, u32)> = events.iter().filter_map(|(e, _)| if let VolleyEvent::Shot { mark, round } = e { Some((*mark, *round)) } else { None }).collect();
        assert_eq!(shots, vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]);
        let at: Vec<f32> = events.iter().map(|(_, t)| *t).collect();
        assert_eq!(at[0], 0.0, "the first round at once");
        assert!((at[1] - 0.05).abs() < 0.015 && (at[2] - 0.10).abs() < 0.025, "rounds at the burst's rate: {at:?}");
        assert!((at[3] - 0.30).abs() < 0.03, "the next mark SHOT_INTERVAL after the last round: {at:?}");
        assert_eq!(events.last().unwrap().0, VolleyEvent::Done);
        assert!((at[6] - 0.70).abs() < 0.05, "{at:?}");
    }

    #[test]
    fn a_volley_is_not_all_fired_mid_burst() {
        let mut v = Volley::bursts(1, 2, 0.2, 0.05, 0.3);
        assert_eq!(v.step(0.0), Some(VolleyEvent::Shot { mark: 0, round: 0 }));
        assert_eq!(v.fired, 1);
        assert!(!v.all_fired(), "the second round is still to come");
        assert_eq!(v.step(0.1), Some(VolleyEvent::Shot { mark: 0, round: 1 }));
        assert!(v.all_fired());
    }

    const LIMITS: MeleeLimits = MeleeLimits { approach: 2.5, swing: 2.0, after: 0.3 };

    #[test]
    fn melee_comes_to_the_mark_swings_waits_then_takes_the_next() {
        let mut run = MeleeRun::new(2, LIMITS);
        let dt = 0.1;
        // Not yet beside the first mark: bring the player to it.
        assert_eq!(run.step(dt, false, false), Some(MeleeAction::Approach(0)));
        assert_eq!(run.step(dt, false, false), Some(MeleeAction::Approach(0)));
        // Within reach: one swing.
        assert_eq!(run.step(dt, true, false), Some(MeleeAction::Swing(0)));
        assert_eq!(run.swung, 1);
        // The swing starts the next frame, plays, ends: the second mark.
        assert_eq!(run.step(dt, true, false), Some(MeleeAction::Wait(0)));
        assert_eq!(run.step(dt, true, true), Some(MeleeAction::Wait(0)));
        assert_eq!(run.step(dt, true, true), Some(MeleeAction::Wait(0)));
        assert_eq!(run.step(dt, true, false), Some(MeleeAction::Wait(1)));
        assert_eq!(run.current, 1);
        // Already close to the second: it swings at once.
        assert_eq!(run.step(dt, true, false), Some(MeleeAction::Swing(1)));
        assert_eq!(run.step(dt, true, true), Some(MeleeAction::Wait(1)));
        assert_eq!(run.step(dt, true, false), Some(MeleeAction::Wait(1)));
        assert!(run.all_fired() && !run.done());
        assert_eq!(run.step(0.2, false, false), Some(MeleeAction::Wait(1)));
        assert_eq!(run.step(0.2, false, false), Some(MeleeAction::Done));
        assert!(run.done());
        assert_eq!(run.step(0.2, false, false), None);
        assert_eq!(run.swung, 2);
    }

    #[test]
    fn melee_swings_anyway_when_closing_or_the_swing_takes_too_long() {
        let mut run = MeleeRun::new(1, LIMITS);
        let mut n = 0;
        while run.step(0.5, false, false) == Some(MeleeAction::Approach(0)) {
            n += 1;
            assert!(n < 100);
        }
        assert_eq!(n, 5, "2.5 s at 0.5 s a step, then the swing");
        assert_eq!(run.swung, 1);
        // A swing never seen (it could not start): given up after the swing limit.
        let mut waits = 0;
        while !run.all_fired() {
            run.step(0.5, false, false);
            waits += 1;
            assert!(waits < 100);
        }
        assert_eq!(waits, 4, "2 s of waiting at 0.5 s a step");
        assert!(MeleeRun::new(0, LIMITS).all_fired(), "nothing marked: nothing to do");
    }

    /// Where a throw along `dir` at `speed` under `g` is when it has come `flat` across.
    fn height_at(from: V3, dir: V3, speed: f32, g: f32, flat: f32) -> f32 {
        let vh = speed * (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        let t = flat / vh;
        from[2] + speed * dir[2] * t - 0.5 * g * t * t
    }

    #[test]
    fn a_throw_lands_on_the_mark_on_the_low_arc() {
        // The frag grenade: 1,200 units/s under 686.7 units/s² [observed PROJ, world gravity].
        let (speed, g) = (1200.0, 686.7);
        let from = [0.0, 0.0, 120.0];
        for to in [[800.0, 0.0, 30.0], [0.0, -1500.0, 200.0], [300.0, 400.0, 0.0]] {
            let dir = lob_dir(from, to, speed, g).expect("in reach");
            assert!((vec::len(dir) - 1.0).abs() < 1e-4);
            let flat = (to[0] * to[0] + to[1] * to[1]).sqrt();
            assert!((height_at(from, dir, speed, g, flat) - to[2]).abs() < 1.0, "{to:?} {dir:?}");
            // Towards the mark across the ground, and the low arc: under 45°.
            assert!(dir[0] * to[0] + dir[1] * to[1] > 0.0);
            assert!(dir[2] < std::f32::consts::FRAC_1_SQRT_2, "{dir:?}");
        }
        // Out of reach (2,097 units on flat ground): no arc; the 45° throw instead.
        assert!((throw_reach(speed, g) - 2097.0).abs() < 1.0);
        assert_eq!(lob_dir(from, [3000.0, 0.0, 120.0], speed, g), None);
        let far = lob_far(from, [3000.0, 0.0, 120.0]);
        assert!((far[0] - far[2]).abs() < 1e-6 && far[1].abs() < 1e-6);
        // No gravity: straight at it.
        let d = lob_dir(from, [100.0, 0.0, 120.0], speed, 0.0).unwrap();
        assert!((d[0] - 1.0).abs() < 1e-6);
    }
    #[test]
    fn the_hit_chance_falls_with_range_and_part_size() {
        let t = HitTuning { spread_mult: 1.1, ..HitTuning::default() };
        // A human torso (BPTD 60) with a 1° cone: close, far over the cap.
        let at = |distance, size| raw_chance(&HitInputs { base: 60.0, size, skill: 50.0, distance, cone_deg: Some(1.0) }, &t);
        assert!((at(0.0, SIZE_TORSO) - 60.0 * CLOSE_MULT).abs() < 1e-3);
        // Farther, the spread opens past the focus and the chance falls.
        assert!(at(500.0, SIZE_TORSO) > at(1000.0, SIZE_TORSO) && at(1000.0, SIZE_TORSO) > at(3000.0, SIZE_TORSO));
        // A smaller part loses more to the same spread.
        assert!(at(1000.0, SIZE_HEAD) < at(1000.0, SIZE_TORSO));
        // Where the spread at the target is FOCUS × size wide the range halves the chance.
        let d = FOCUS * SIZE_ARM / (1.1f32).to_radians().tan();
        assert!((range_term(d, Some(1.0), SIZE_ARM, &t) - 0.5).abs() < 1e-3);
        // Melee: no range. No spread at all (a perk set it to 0): range never matters.
        assert_eq!(range_term(5000.0, None, SIZE_TORSO, &t), 1.0);
        assert_eq!(range_term(5000.0, Some(0.0), SIZE_TORSO, &t), 1.0);
        assert_eq!(part_size(Some("PerceptionCondition")), SIZE_HEAD);
        assert_eq!(part_size(Some("RightMobilityCondition")), SIZE_LEG);
        assert_eq!(part_size(None), SIZE_TORSO);
        // Melee: the skill's share, no range: skill 0 halves the base, 100 doubles it.
        let swing = |skill| raw_chance(&HitInputs { base: 30.0, size: SIZE_HEAD, skill, distance: 5000.0, cone_deg: None }, &t);
        assert!((swing(0.0) - 15.0).abs() < 1e-4 && (swing(100.0) - 60.0).abs() < 1e-3);
    }

    /// The original's VATS percentages against an NCR trooper, Guns 67, Strength 5
    /// [observed: issue #1361, vats-mode.md §17.2], against ours with the cone our
    /// `vats_actor_spread` gives (0.3 spread units × the skill's share, the hunting
    /// rifle's Strength shortfall, and each gun's Min Spread). The pistol, the hunting
    /// rifle and the heads come within 2 points; the varmint rifle and the sawed-off up
    /// close come out 5–8 low, unexplained (§17.2).
    #[test]
    fn the_chance_matches_the_originals_samples() {
        let t = HitTuning { spread_mult: 1.1, ..HitTuning::default() };
        let cone = |actor: f32, min_spread: f32| crate::spread::half_angle(crate::spread::deviation_deg(actor, min_spread));
        let skill = 1.0 - 0.5 * 0.67;
        // (actor spread, Min Spread, distance, base, size, the original's percent)
        let samples = [
            (0.3 * skill, 0.7, 1050.0, 60.0, SIZE_TORSO, 47.0), // 9mm pistol
            (0.3 * skill, 0.7, 2100.0, 60.0, SIZE_TORSO, 22.0),
            (0.3 * skill, 0.7, 1050.0, 30.0, SIZE_HEAD, 20.0),
            (0.3 * skill, 0.024, 1050.0, 60.0, SIZE_TORSO, 79.0), // varmint rifle
            (0.3 * skill, 0.024, 2100.0, 50.0, SIZE_LEG, 29.0),
            (0.325 * skill, 0.01, 2100.0, 60.0, SIZE_TORSO, 31.0), // hunting rifle
            (0.3 * skill, 4.0, 350.0, 60.0, SIZE_TORSO, 52.0), // sawed-off shotgun
            (0.3 * skill, 4.0, 1050.0, 60.0, SIZE_TORSO, 10.0),
            (0.3 * skill, 0.024, 1050.0, 42.0, SIZE_HELD_WEAPON, 16.0), // a Service Rifle held
        ];
        for (actor, min_spread, distance, base, size, seen) in samples {
            let ours = final_chance(raw_chance(&HitInputs { base, size, skill: 67.0, distance, cone_deg: Some(cone(actor, min_spread)) }, &t), |c| c, None, &t);
            assert!((ours - seen).abs() <= 8.0, "{distance} units, base {base}: ours {ours:.1}, the original's {seen}");
        }
    }

    #[test]
    fn perks_concentrated_fire_and_the_cap() {
        let t = HitTuning::default();
        // Commando's Multiply 1.25 on a raw 40 %.
        assert!((final_chance(40.0, |v| v * 1.25, None, &t) - 50.0).abs() < 1e-4);
        // Concentrated Fire: +5 for each earlier mark on the part [documented example 63 → 68 → 73].
        assert!((final_chance(63.0, |v| v, Some(1), &t) - 68.0).abs() < 1e-4);
        assert!((final_chance(63.0, |v| v, Some(2), &t) - 73.0).abs() < 1e-4);
        // Never above fVATSMaxChance nor below 0.
        assert_eq!(final_chance(120.0, |v| v, None, &t), 95.0);
        assert_eq!(final_chance(10.0, |_| -5.0, None, &t), 0.0);
        assert!(rolled_hit(50.0, 0.49) && !rolled_hit(50.0, 0.5));
        assert!(!rolled_hit(0.0, 0.0) && rolled_hit(95.0, 0.9499));
    }

    #[test]
    fn a_miss_passes_wide_and_never_below() {
        let eye = [0.0, 0.0, 100.0];
        let point = [1000.0, 0.0, 100.0];
        for (u1, u2) in [(0.0, 0.0), (1.0, 1.0), (0.5, 0.5), (0.3, 0.9), (0.9, 0.1)] {
            let m = miss_point(eye, point, u1, u2);
            let off = vec::len(vec::sub(m, point));
            assert!((MISS_MIN - 1e-3..=MISS_MIN + MISS_SPAN + 1e-3).contains(&off), "{off}");
            // Across the line, not along it.
            assert!(vec::dot(vec::sub(m, point), [1.0, 0.0, 0.0]).abs() < 1e-3);
            // Never more than 30° below the side.
            assert!(m[2] - point[2] >= -off * 0.5 - 1e-3, "{m:?}");
        }
        let land = throw_miss_point([0.0, 0.0, 10.0], 0.0, 0.25);
        assert!((vec::len(vec::sub(land, [0.0, 0.0, 10.0])) - THROW_MISS_MIN).abs() < 1e-3);
        assert_eq!(land[2], 10.0);
    }
}
