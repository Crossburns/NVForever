//! Challenges (CHAL): counters the player's deeds move towards a threshold; at the
//! threshold the challenge is completed and its reward script runs (XP, a perk, a
//! message). docs/spec-notes/challenges.md.
//!
//! What counts is the record's type with its values and forms [documented: xEdit field
//! names, GECK wiki GetChallengeType for the type codes; observed: the meaning of
//! value1–3 per type, read off the 184 challenges of the masters and their descriptions,
//! challenges.md §2]:
//!
//! | type | counts | filters |
//! |---|---|---|
//! | 0 kill from a form list | one per kill | SNAM victims' list, weapon |
//! | 1 kill a specific form | one per kill | SNAM victim, weapon |
//! | 2 kill any in a category | one per kill | value2 creature type + 1, value3 + XNAM weapon |
//! | 3 hit an enemy | one per hit | value1 body part, value2 0 any / 1 critical / 2 knockdown / 3 cripple, weapon |
//! | 4 discover a map marker | one per marker | SNAM marker or XNAM list |
//! | 5 / 9 use an item (from a list) | one per use | SNAM item / list |
//! | 6 / 10 acquire an item (from a list) | the count | SNAM item / list |
//! | 8 do damage | the damage | value1 body part, weapon |
//! | 11 miscellaneous stat | the stat's rise | value1 stat code |
//! | 12 craft using an item | one per recipe made | SNAM ingredient |
//! | 13 scripted | IncrementScriptedChallenge | — |
//!
//! "Weapon" is value2 (the weapon's skill, an actor value code), value3 (its animation
//! type + 1) and XNAM (a weapon list), each when set. Type 7 (use a skill) is in no
//! master and is not counted.
//!
//! The run-time state is `PlayerState::challenges` (saved). Completions queue their
//! reward scripts, run at the start of the next [`Game::tick`] (never from inside the
//! hit or kill that completed them), and every step queues a [`Notice`] for the engine's
//! corner messages and log.

use crate::game::Game;
use crate::misc_stats::LOCATIONS_DISCOVERED;
use fnv_sim::FormId;

/// CHAL DATA type codes [documented: GECK wiki GetChallengeType].
pub mod kind {
    pub const KILL_FROM_LIST: i64 = 0;
    pub const KILL_FORM: i64 = 1;
    pub const KILL_CATEGORY: i64 = 2;
    pub const HIT_ENEMY: i64 = 3;
    pub const DISCOVER_MARKER: i64 = 4;
    pub const USE_ITEM: i64 = 5;
    pub const ACQUIRE_ITEM: i64 = 6;
    pub const USE_SKILL: i64 = 7;
    pub const DO_DAMAGE: i64 = 8;
    pub const USE_ITEM_FROM_LIST: i64 = 9;
    pub const ACQUIRE_FROM_LIST: i64 = 10;
    pub const MISC_STAT: i64 = 11;
    pub const CRAFT: i64 = 12;
    pub const SCRIPTED: i64 = 13;
}

/// CHAL DATA flags [documented: GECK wiki GetChallengeFlags].
pub const START_DISABLED: u32 = 1;
pub const RECURRING: u32 = 2;
pub const SHOW_ZERO_PROGRESS: u32 = 4;

/// Hit kinds of a "hit an enemy" challenge (value2) [observed: 0 with "inflict damage to
/// heads", 1 with "score critical hits", 2 with "knock down", 3 with "cripple"].
pub const HIT_ANY: u32 = 0;
pub const HIT_CRITICAL: u32 = 1;
pub const HIT_KNOCKDOWN: u32 = 2;
pub const HIT_CRIPPLE: u32 = 3;

/// One challenge record.
#[derive(Debug, Clone, PartialEq)]
pub struct ChallengeDef {
    pub form: FormId,
    /// DATA type ([`kind`]); −1 for a code the schema does not name.
    pub kind: i64,
    pub threshold: u32,
    pub flags: u32,
    /// Progress notices come at each multiple of this [documented: GECK wiki
    /// GetChallengeInterval]; 0 none.
    pub interval: u32,
    pub value1: u32,
    pub value2: u32,
    pub value3: u32,
    /// SNAM ("form 1") and XNAM ("form 2") [documented: GECK wiki GetChallengeForm1/2].
    pub form1: Option<FormId>,
    pub form2: Option<FormId>,
    pub name: Option<String>,
    pub description: Option<String>,
}

impl ChallengeDef {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    /// The threshold, at least 1 (Find Vault 34 has 0, its siblings 1) [hypothesis].
    pub fn goal(&self) -> u32 {
        self.threshold.max(1)
    }

    /// The name the player sees: FULL, else the Editor ID.
    pub fn title(&self, g: &Game) -> String {
        self.name.clone().unwrap_or_else(|| g.data.name(self.form))
    }
}

/// What the engine shows of a challenge step (`Game::challenge_notices`).
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// Progress reached a multiple of the interval (not the threshold).
    Progress { challenge: FormId, progress: u32, threshold: u32 },
    Completed { challenge: FormId },
}

/// The body part of a hit as a challenge's value1 counts it: 1 head, 2 torso, 3 left
/// arm, 4 right arm, 5 left leg, 6 right leg [hypothesis: the order of the limb
/// condition actor values 25–30, which fits every master's use: 1 "heads", 2 "torsos",
/// 4 "right arms"]. From the limb hit when there is one, else from GetHitLocation's
/// index.
pub fn body_part(limb_condition: Option<u16>, location: i32) -> Option<u32> {
    if let Some(c) = limb_condition.filter(|c| (25..=30).contains(c)) {
        return Some(u32::from(c - 24));
    }
    match location {
        0 => Some(2),
        1 | 2 | 13 => Some(1),
        3 | 4 => Some(3),
        5 | 6 => Some(4),
        7..=9 => Some(5),
        10..=12 => Some(6),
        _ => None,
    }
}

/// Unarmed's actor value code, a fist's skill.
const UNARMED: i64 = 45;

impl Game<'_> {
    fn challenge_list(&self, kind: i64) -> Vec<FormId> {
        let mut v: Vec<FormId> = self.data.challenge_defs.values().filter(|d| d.kind == kind).map(|d| d.form).collect();
        v.sort();
        v
    }

    fn in_list(&self, list: Option<FormId>, form: FormId) -> bool {
        list.and_then(|l| self.data.form_lists.get(&l)).is_some_and(|l| l.contains(&form))
    }

    /// Whether `weapon` (None: fists) passes the challenge's weapon filters: value2 the
    /// weapon's skill (when `skill_filter`), value3 its animation type + 1, XNAM a list
    /// holding it [observed: challenges.md §2].
    fn weapon_passes(&self, d: &ChallengeDef, weapon: Option<FormId>, skill_filter: bool) -> bool {
        let (skill, anim) = match weapon {
            Some(w) => (self.data.weapon_skills.get(&w).copied(), self.data.weapon_anim_types.get(&w).copied()),
            None => (Some(UNARMED), Some(0)),
        };
        if skill_filter && d.value2 != 0 && skill != Some(i64::from(d.value2)) {
            return false;
        }
        if d.value3 != 0 && anim.map(|a| a + 1) != Some(i64::from(d.value3)) {
            return false;
        }
        if d.form2.is_some() && !weapon.is_some_and(|w| self.in_list(d.form2, w)) {
            return false;
        }
        true
    }

    /// The weapon the player last hit `victim` with, else the one it holds.
    fn player_weapon_on(&mut self, victim: FormId) -> Option<FormId> {
        match self.extra.get(&victim).and_then(|e| e.last_hit) {
            Some(h) if h.attacker == Some(FormId::PLAYER_REF) => h.weapon,
            _ => crate::functions::equipped_weapon_of(self, FormId::PLAYER_REF),
        }
    }

    /// The player killed `victim` (called while it still counts as alive).
    pub(crate) fn challenge_kill(&mut self, victim: FormId) {
        let data = self.data;
        let weapon = self.player_weapon_on(victim);
        let base = crate::functions::base_of(self, victim);
        let creature_type = self.stats_def(victim).and_then(|d| d.creature_type);
        let is_creature = base.and_then(|b| self.data.sig(b)).is_some_and(|s| &s == b"CREA");
        for kind in [kind::KILL_FROM_LIST, kind::KILL_FORM, kind::KILL_CATEGORY] {
            for c in self.challenge_list(kind) {
                let d = &data.challenge_defs[&c];
                let victim_ok = match kind {
                    kind::KILL_FROM_LIST => d.form1.is_none() || [Some(victim), base].iter().flatten().any(|&f| self.in_list(d.form1, f)),
                    kind::KILL_FORM => d.form1.is_some_and(|f| f == victim || Some(f) == base),
                    _ => d.value2 == 0 || (is_creature && creature_type.map(|t| t + 1) == Some(i64::from(d.value2))),
                };
                if victim_ok && self.weapon_passes(d, weapon, kind != kind::KILL_CATEGORY) {
                    self.challenge_step(c, 1.0);
                }
            }
        }
    }

    /// The player hit `target` (an actor that took the hit): `dealt` health damage,
    /// the part hit, whether it was critical and whether it crippled.
    pub(crate) fn challenge_hit(&mut self, target: FormId, weapon: Option<FormId>, part: Option<u32>, dealt: f64, critical: bool, crippled: bool) {
        let data = self.data;
        if target == FormId::PLAYER_REF {
            return;
        }
        for c in self.challenge_list(kind::HIT_ENEMY) {
            let d = &data.challenge_defs[&c];
            let what = match d.value2 {
                HIT_ANY => dealt > 0.0,
                HIT_CRITICAL => critical,
                HIT_CRIPPLE => crippled,
                _ => false,
            };
            if what && (d.value1 == 0 || part == Some(d.value1)) && self.weapon_passes(d, weapon, false) {
                self.challenge_step(c, 1.0);
            }
        }
        if dealt <= 0.0 {
            return;
        }
        for c in self.challenge_list(kind::DO_DAMAGE) {
            let d = &data.challenge_defs[&c];
            if (d.value1 == 0 || part == Some(d.value1)) && self.weapon_passes(d, weapon, true) {
                self.challenge_step(c, dealt);
            }
        }
    }

    /// The player knocked `target` down.
    pub(crate) fn challenge_knockdown(&mut self, target: FormId) {
        let data = self.data;
        let weapon = self.player_weapon_on(target);
        for c in self.challenge_list(kind::HIT_ENEMY) {
            let d = &data.challenge_defs[&c];
            if d.value2 == HIT_KNOCKDOWN && d.value1 == 0 && self.weapon_passes(d, weapon, false) {
                self.challenge_step(c, 1.0);
            }
        }
    }

    /// The player used (ate, drank, injected) `item`.
    pub(crate) fn challenge_used(&mut self, item: FormId) {
        let data = self.data;
        for c in self.challenge_list(kind::USE_ITEM) {
            if data.challenge_defs[&c].form1 == Some(item) {
                self.challenge_step(c, 1.0);
            }
        }
        for c in self.challenge_list(kind::USE_ITEM_FROM_LIST) {
            if self.in_list(data.challenge_defs[&c].form1, item) {
                self.challenge_step(c, 1.0);
            }
        }
    }

    /// `count` of `item` came into the player's inventory.
    pub(crate) fn challenge_acquired(&mut self, item: FormId, count: i32) {
        let data = self.data;
        if count <= 0 {
            return;
        }
        for c in self.challenge_list(kind::ACQUIRE_ITEM) {
            if data.challenge_defs[&c].form1 == Some(item) {
                self.challenge_step(c, f64::from(count));
            }
        }
        for c in self.challenge_list(kind::ACQUIRE_FROM_LIST) {
            if self.in_list(data.challenge_defs[&c].form1, item) {
                self.challenge_step(c, f64::from(count));
            }
        }
    }

    /// The player found the map marker `marker` (the engine's discovery): "discover a
    /// map marker" challenges, and those on Locations Discovered, which is counted from
    /// the found markers rather than by `count_stat` (`Game::misc_stat`). JIP's
    /// location-discover event runs first, with the marker's reference [documented:
    /// SetOnLocationDiscoverEventHandler].
    pub fn challenge_marker_found(&mut self, marker: FormId) {
        self.jip_event("locationdiscover", None, vec![fnv_script::vm::XVal::Ref(marker.0)]);
        let data = self.data;
        for c in self.challenge_list(kind::DISCOVER_MARKER) {
            let d = &data.challenge_defs[&c];
            if d.form1 == Some(marker) || self.in_list(d.form2, marker) {
                self.challenge_step(c, 1.0);
            }
        }
        self.challenge_stat(LOCATIONS_DISCOVERED, 1.0);
    }

    /// The player's statistic `code` rose by `n` (`Game::count_stat`).
    pub(crate) fn challenge_stat(&mut self, code: u16, n: f64) {
        let data = self.data;
        if n <= 0.0 {
            return;
        }
        for c in self.challenge_list(kind::MISC_STAT) {
            if data.challenge_defs[&c].value1 == u32::from(code) {
                self.challenge_step(c, n);
            }
        }
    }

    /// The player made a recipe `times` times with these ingredients (the engine's
    /// recipe menu).
    pub fn challenge_crafted(&mut self, ingredients: &[FormId], times: u32) {
        let data = self.data;
        for c in self.challenge_list(kind::CRAFT) {
            let d = &data.challenge_defs[&c];
            if d.form1.is_none_or(|f| ingredients.contains(&f) || ingredients.iter().any(|&i| self.in_list(Some(f), i))) {
                self.challenge_step(c, f64::from(times));
            }
        }
    }

    /// One step of `amount` towards challenge `c`. A challenge flagged Start Disabled
    /// moves only once unlocked (UnlockChallenge); a completed one only when recurring.
    /// At the threshold it is completed: a recurring one starts again from 0, the others
    /// stay at the threshold [documented: GECK wiki GetChallengeProgress notes]. Each
    /// completion counts in Challenges Completed and queues its reward script.
    /// Fractions (damage) are kept until they make a whole step [hypothesis].
    pub fn challenge_step(&mut self, c: FormId, amount: f64) {
        let data = self.data;
        let Some(d) = data.challenge_defs.get(&c) else { return };
        let (goal, interval, flags) = (d.goal(), d.interval, d.flags);
        let s = self.player.challenges.entry(c).or_default();
        let recurring = flags & RECURRING != 0 && !s.not_recurring;
        if (flags & START_DISABLED != 0 && !s.unlocked) || (s.completed && !recurring) || amount <= 0.0 {
            return;
        }
        let total = s.carry + amount;
        let whole = total.floor();
        s.carry = total - whole;
        if whole < 1.0 {
            return;
        }
        let before = s.progress;
        let after = before.saturating_add(whole.min(f64::from(u32::MAX)) as u32);
        let name = d.title(self);
        if after >= goal {
            let s = self.player.challenges.entry(c).or_default();
            s.completed = true;
            s.progress = if recurring { 0 } else { goal };
            s.carry = 0.0;
            s.times = s.times.saturating_add(1);
            tracing::info!("challenge: {name} {goal}/{goal}, completed");
            self.challenge_queue.rewards.push(c);
            self.challenge_queue.notices.push(Notice::Completed { challenge: c });
            self.count_stat(crate::misc_stats::CHALLENGES_COMPLETED, 1.0);
            self.jg_event(crate::nvse_jip3::JgEvent::ChallengeComplete, Some(c));
        } else {
            self.player.challenges.entry(c).or_default().progress = after;
            let crossed = interval > 0 && after / interval > before / interval;
            if crossed {
                self.challenge_queue.notices.push(Notice::Progress { challenge: c, progress: after, threshold: goal });
            }
            if crossed || d.kind != kind::DO_DAMAGE {
                tracing::info!("challenge: {name} {after}/{goal}");
            }
        }
        if self.challenge_queue.notices.len() > 64 {
            self.challenge_queue.notices.drain(..32);
        }
    }

    /// The reward scripts of the challenges completed since the last frame: each runs its
    /// ScriptEffectStart block once on the player [observed: every master's reward
    /// script is an effect script with only that block, RewardXP and AddPerk on
    /// `player`], in its own variables, dropped after.
    pub(crate) fn run_challenge_rewards(&mut self) {
        for c in std::mem::take(&mut self.challenge_queue.rewards) {
            let Some(script) = self.data.script_of_base(c) else { continue };
            let key = crate::game::InstKey::Challenge { challenge: c };
            let ctx = crate::game::RunCtx { self_ref: Some(FormId::PLAYER_REF), action_ref: None, quest: None };
            self.run_event(key, script, crate::game::event::SCRIPT_EFFECT_START, None, ctx);
            self.instances.remove(&key);
        }
    }

    /// Take the notices queued since the last call (the engine's corner messages).
    pub fn take_challenge_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.challenge_queue.notices)
    }

    /// Set a challenge's progress (a test aid): below the threshold, not completed.
    pub fn set_challenge_progress(&mut self, c: FormId, progress: u32) -> bool {
        let Some(goal) = self.data.challenge_defs.get(&c).map(ChallengeDef::goal) else { return false };
        let s = self.player.challenges.entry(c).or_default();
        s.progress = progress.min(goal.saturating_sub(1));
        s.carry = 0.0;
        true
    }
}

/// Pending reward scripts and notices (not saved: a save between a completion and the
/// next frame loses at most the reward of that frame).
#[derive(Debug, Clone, Default)]
pub struct Queue {
    pub rewards: Vec<FormId>,
    pub notices: Vec<Notice>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_parts_follow_the_limb_values_then_the_location() {
        assert_eq!(body_part(Some(25), 0), Some(1));
        assert_eq!(body_part(Some(28), 0), Some(4));
        assert_eq!(body_part(None, 1), Some(1));
        assert_eq!(body_part(None, 0), Some(2));
        assert_eq!(body_part(None, 11), Some(6));
        assert_eq!(body_part(None, -1), None);
    }
}
