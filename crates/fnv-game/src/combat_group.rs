//! Combat groups: actors fighting together, fleeing, alarms to allies, searches shared
//! out, and fights between actors (docs/design/combat-ai.md §12).
//!
//! What the GECK wiki's combat pages describe [documented: GECK wiki Confidence, Combat
//! Flee, Combat Search, Combat Style; the owner's export `Combat.xml`]:
//! - **Combat groups**: the actors on one side of a fight. Here a group is an actor and
//!   every actor in a combat state that is its ally or friend (`Game::reaction_to`:
//!   faction reactions, a faction or base form in common) and has a foe in common with
//!   it (a target or an alarm's owner) [engineering choice: the wiki names combat groups
//!   but not how they form].
//! - **Group threat ratio** [documented: Confidence]: the sum of the group's combat
//!   strength (health counting armour × damage per second) over the sum of its foes'.
//!   An actor flees when that is below its confidence threshold; one whose combat style
//!   has "Flee Based on Personal Survival" also flees when its personal ratio (its own
//!   health over the foes' average damage per second, against their average health over
//!   its group's average damage per second) is below it. Frenzied actors never flee.
//!   Each confidence boost (an actor at its flee spot attacked or cornered) raises its
//!   rating one level for the rest of the fight ([`Tactics::boosts`]).
//! - **Alarms** [documented: Confidence, Alarms]: an actor that starts fighting sends an
//!   alarm to summon help unless its foe is a minimal threat: group threat ratio at or
//!   above (confidence threshold + 0.5) × the alarm confidence multiplier (the wiki's
//!   example, 3; no game setting of FalloutNV.esm names it [observed]).
//! - **Searches** [documented: Combat Search]: the group searches together, splitting the
//!   area; after iCombatTargetLostRemoveSearchCount complete searches (FalloutNV.esm: 2
//!   [observed]) it drops what it searched for.
//!
//! On top, NVForever's own design [design]:
//! - an actor that **notices** a hostile (Normal → Alert) alarms its allies near it too,
//!   so a camp wakes as one (the owner's list 2: "alerting their friends when they sense
//!   an enemy presence");
//! - **NPC against NPC** hits ([`Game::attacked_by_actor`]): a hit by an actor the victim
//!   would leave alone starts a grudge as the player's hits do (`crate::provocation`),
//!   except a stray hit between allies or friends, or between two actors both fighting
//!   someone else (the owner's exception, applied to everyone);
//! - **posts** ([`Post`]): where the engine's combat AI wants a fighter to stand (cover,
//!   a peek out of it, a flank, a step aside to clear a friend from its line of fire);
//!   `fnv_ai::alert` walks it there.
//!
//! Only actors in the high process take part: the engine measures, fights and alarms
//! only drawn actors near the player (`fnv_ai::process`), and passes them in.

use crate::awareness::{Cause, State};
use crate::combat_style::flees;
use crate::functions::actor::actor_value;
use crate::game::Game;
use crate::provocation::{Grudge, Note, Why, ALLY, FRIEND};
use fnv_sim::FormId;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// An actor's combat strength for the threat ratios: health counting armour, and damage
/// per second (the engine's combat AI rates its weapon) [documented: Confidence].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Strength {
    pub health: f64,
    pub dps: f64,
}

/// Why the combat AI wants a fighter somewhere ([`Post`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PostKind {
    /// Behind cover, out of its target's sight: the run there from elsewhere, turned the
    /// way it goes.
    Cover,
    /// Behind its cover again, back from its peek point (or waiting there to peek): it
    /// stays facing its target and steps sideways in and out, as a person firing from
    /// cover does (owner 2026-10-18: Violet turned between her way and her target at a
    /// caravan) [design].
    Duck,
    /// Out of cover far enough to see and fire at its target.
    Peek,
    /// A place round its target beside its group's other fighters, so they come at it
    /// from several sides and do not stand in each other's way.
    Flank,
    /// A step aside: a friend stands between it and its target.
    Sidestep,
    /// Back from a target nearer than its weapon's minimum range.
    BackOff,
    /// A move sideways across the line to its target after crouching in one place a
    /// while (docs/design/combat-ai.md §14.2).
    Shift,
    /// A melee fighter's bound to cover nearer its target (§14.4).
    Advance,
    /// A melee fighter staying by a gunman of its group (§14.4).
    Guard,
    /// A melee fighter's dodge, left, right, back or forward of the line to its target,
    /// walked for the style's dodge timer (docs/design/combat-ai.md §17.1).
    Dodge,
    /// A run out of a live grenade's blast (§17.7).
    Evade,
}

impl PostKind {
    pub fn name(self) -> &'static str {
        match self {
            PostKind::Cover => "cover",
            PostKind::Duck => "a duck back behind cover",
            PostKind::Peek => "a peek out of cover",
            PostKind::Flank => "a flank",
            PostKind::Sidestep => "a step aside",
            PostKind::BackOff => "a step back",
            PostKind::Shift => "a shift sideways",
            PostKind::Advance => "a bound to cover",
            PostKind::Guard => "a guard place",
            PostKind::Dodge => "a dodge",
            PostKind::Evade => "a run from a grenade",
        }
    }
}

/// Where the combat AI wants a fighter to stand, facing its target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Post {
    pub at: [f32; 3],
    pub kind: PostKind,
    /// Runs there (else walks).
    pub run: bool,
}

/// How a fleeing actor gets away [documented: Combat Flee, in this order].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum FleeBy {
    /// Through a door that leads elsewhere.
    Door { door: FormId },
    /// To cover away from the attacker.
    Cover,
    /// Away from the attacker.
    Away,
}

/// An actor fleeing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    /// Whom from, and where it was when the flight began.
    pub from: FormId,
    pub from_at: [f32; 3],
    pub since: f64,
    /// Where it runs to and how (set by `fnv_ai` when it plans the way).
    pub spot: Option<([f32; 3], FleeBy)>,
    /// When it reached its flee spot.
    pub arrived: Option<f64>,
}

/// What happened, for the engine's log and the fight summary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GroupNote {
    /// `sender` alarmed `receiver` about `about` (`combat`: the sender started fighting;
    /// else it noticed it).
    Alarm { sender: FormId, receiver: FormId, about: FormId, combat: bool },
    /// `sender` sent no alarm: its group outmatches its foe.
    NoAlarm { sender: FormId, about: FormId, ratio: f64, alarm_confidence: f64 },
    /// `sender` and `about` share `faction`, flagged Special Combat: their fight sends no
    /// alarm (docs/spec-notes/actor-flags.md §8).
    SpecialCombat { sender: FormId, about: FormId, faction: FormId },
    /// `actor` searched its share of the area round `about`'s alarm (`count` searches done
    /// of `needed`).
    Searched { actor: FormId, about: FormId, count: u32, needed: u32 },
}

/// The game settings of fleeing, searching and alarms [documented: Combat Flee, Combat
/// Search, Confidence; the defaults are the wiki's, FalloutNV.esm overrides most
/// (observed 2026-10-12: door 1400, cover 1000–2000, normal 1400, wait 30, remove count 2)].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupSettings {
    pub flee_max_door: f32,
    pub flee_min_cover: f32,
    pub flee_max_cover: f32,
    pub flee_normal: f32,
    pub flee_boost_radius: f32,
    pub flee_wait: f64,
    pub search_interior: [f32; 3],
    pub search_exterior: [f32; 3],
    pub remove_search_count: u32,
    /// The alarm confidence multiplier [documented: Confidence, "say it is 3"; no game
    /// setting found].
    pub alarm_confidence_mult: f64,
}

impl Default for GroupSettings {
    fn default() -> GroupSettings {
        GroupSettings {
            flee_max_door: 2048.0,
            flee_min_cover: 1536.0,
            flee_max_cover: 3072.0,
            flee_normal: 2048.0,
            flee_boost_radius: 512.0,
            flee_wait: 60.0,
            // The wiki gives no defaults for the search radii: FalloutNV.esm's.
            search_interior: [256.0, 512.0, 1024.0],
            search_exterior: [512.0, 1024.0, 2048.0],
            remove_search_count: 1,
            alarm_confidence_mult: 3.0,
        }
    }
}

impl GroupSettings {
    pub fn read(g: &Game) -> GroupSettings {
        let d = GroupSettings::default();
        let s = |name: &str, v: f32| crate::combat::setting(g, name, v as f64) as f32;
        GroupSettings {
            flee_max_door: s("fCombatFleeMaxDoorDistance", d.flee_max_door),
            flee_min_cover: s("fCombatFleeMinCoverDistance", d.flee_min_cover),
            flee_max_cover: s("fCombatFleeMaxCoverDistance", d.flee_max_cover),
            flee_normal: s("fCombatFleeNormalDistance", d.flee_normal),
            flee_boost_radius: s("fCombatFleeBoostConfidenceTargetRadius", d.flee_boost_radius),
            flee_wait: s("fCombatFleeWaitTime", d.flee_wait as f32) as f64,
            search_interior: [
                s("fCombatSearchInteriorMinRadius", d.search_interior[0]),
                s("fCombatSearchInteriorMediumRadius", d.search_interior[1]),
                s("fCombatSearchInteriorMaxRadius", d.search_interior[2]),
            ],
            search_exterior: [
                s("fCombatSearchExteriorMinRadius", d.search_exterior[0]),
                s("fCombatSearchExteriorMediumRadius", d.search_exterior[1]),
                s("fCombatSearchExteriorMaxRadius", d.search_exterior[2]),
            ],
            remove_search_count: s("iCombatTargetLostRemoveSearchCount", d.remove_search_count as f32).max(1.0) as u32,
            alarm_confidence_mult: d.alarm_confidence_mult,
        }
    }
}

/// Allies within this distance of an actor that starts fighting or notices a hostile hear
/// its alarm, game units [design: the wiki gives none. Half a cell (2,048) until the
/// owner's playtest list 5 item 31, a whole cell (4,096) until #972; now the farthest
/// pair of combat allies the original listed in the Goodsprings gunfight, 9,026 units
/// (median 1,044, 95th percentile 4,864) [observed: observer-jip-audit-2026-10-25.md
/// §3.5], rounded up to a quarter cell; docs/spec-notes/combat-groups.md §1].
pub const ALARM_RADIUS: f32 = 9216.0;

/// The groups' state: strengths, posts, flights, boosts, searches done. Not saved
/// (fights are not saved).
#[derive(Debug, Clone, Default)]
pub struct Tactics {
    /// Each fighter's strength, set by the engine's combat AI while it fights (and the
    /// player's while someone fights it).
    pub strengths: BTreeMap<FormId, Strength>,
    /// Where the combat AI wants fighters to stand.
    pub posts: BTreeMap<FormId, Post>,
    pub flights: BTreeMap<FormId, Flight>,
    /// Confidence levels gained in this fight.
    pub boosts: BTreeMap<FormId, u8>,
    /// Complete searches each actor has made for each alarm owner.
    pub searched: BTreeMap<(FormId, FormId), u32>,
    /// When each fighter first went to Combat in this fight ([`Game::first_starter`]).
    pub started: BTreeMap<FormId, f64>,
    pub notes: Vec<GroupNote>,
    pub settings: Option<GroupSettings>,
}

impl Tactics {
    /// Forget what `actor` had in its fight (back in Normal, dead, gone).
    pub fn calm(&mut self, actor: FormId) {
        self.strengths.remove(&actor);
        self.posts.remove(&actor);
        self.flights.remove(&actor);
        self.boosts.remove(&actor);
        self.searched.retain(|(a, _), _| *a != actor);
        self.started.remove(&actor);
    }

    pub fn take_notes(&mut self) -> Vec<GroupNote> {
        std::mem::take(&mut self.notes)
    }
}

/// The group threat ratio: the sum of my group's strengths over the sum of the foes'
/// [documented: Confidence, "Sum( Ally Combat Strength ) / Sum( Enemy Combat Strength )"].
/// `None` without foes of known strength.
pub fn group_threat_ratio(mine: &[Strength], theirs: &[Strength]) -> Option<f64> {
    let enemy: f64 = theirs.iter().map(|s| s.health * s.dps).sum();
    if theirs.is_empty() {
        return None;
    }
    let ours: f64 = mine.iter().map(|s| s.health * s.dps).sum();
    Some(if enemy <= 0.0 { f64::INFINITY } else { ours / enemy })
}

/// The personal survival threat ratio [documented: Confidence]: (my health / the foes'
/// average DPS) over (the foes' average health / my group's average DPS).
pub fn personal_threat_ratio(me: Strength, mine: &[Strength], theirs: &[Strength]) -> Option<f64> {
    if theirs.is_empty() || mine.is_empty() {
        return None;
    }
    let avg = |v: &[Strength], f: fn(&Strength) -> f64| v.iter().map(f).sum::<f64>() / v.len() as f64;
    let (their_dps, their_health, our_dps) = (avg(theirs, |s| s.dps), avg(theirs, |s| s.health), avg(mine, |s| s.dps));
    let my_time = if their_dps <= 0.0 { f64::INFINITY } else { me.health / their_dps };
    let their_time = if our_dps <= 0.0 { f64::INFINITY } else { their_health / our_dps };
    Some(if their_time.is_infinite() { 0.0 } else if their_time <= 0.0 { f64::INFINITY } else { my_time / their_time })
}

/// The alarm confidence: (confidence + 0.5) × the multiplier [documented: Confidence].
pub fn alarm_confidence(threshold: f64, mult: f64) -> f64 {
    (threshold + 0.5) * mult
}

/// A flee check's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FleeCheck {
    pub group_ratio: Option<f64>,
    pub personal_ratio: Option<f64>,
    /// The threshold after boosts, before this fight's confidence modifier.
    pub threshold: f64,
    /// The rating used (0 Cowardly … 4 Foolhardy) after boosts.
    pub rating: i64,
    pub flees: bool,
    /// It flees by the personal survival check (the group's passed).
    pub personal: bool,
}

impl Game<'_> {
    pub fn group_settings(&mut self) -> GroupSettings {
        match self.awareness.tactics.settings {
            Some(s) => s,
            None => {
                let s = GroupSettings::read(self);
                self.awareness.tactics.settings = Some(s);
                s
            }
        }
    }

    /// A name for the side `actor` fights on, for logs: the Editor ID of its faction
    /// that names the most others in its reactions (NCRFactionNV rather than
    /// GeneralIdleFaction), else its base form's.
    pub fn side_name(&self, actor: FormId) -> String {
        if actor == FormId::PLAYER_REF {
            return "the player".into();
        }
        let factions = crate::functions::actor::factions_of(self, actor);
        let reactions = |f: &FormId| self.data.faction_reactions.get(f).map_or(0, |v| v.len());
        factions
            .iter()
            .enumerate()
            .max_by_key(|(i, f)| (reactions(f), std::cmp::Reverse(*i)))
            .map(|(_, f)| f)
            .and_then(|f| self.data.edid_of.get(f).cloned())
            .or_else(|| self.world.placed.get(actor).and_then(|p| self.data.edid_of.get(&p.base).cloned()))
            .unwrap_or_else(|| actor.to_string())
    }

    /// Whom `actor` fights or hunts: its targets and its alarms' owners.
    pub fn foes_of(&self, actor: FormId) -> BTreeSet<FormId> {
        self.awareness.get(actor).map_or_else(BTreeSet::new, |a| a.targets.keys().copied().chain(a.alarms.iter().map(|x| x.owner)).collect())
    }

    /// Whether two actors stand together in a fight: allies or friends (faction
    /// reactions, a faction or base in common).
    pub fn comrades(&self, a: FormId, b: FormId) -> bool {
        a == b || matches!(self.reaction_to(a, b), ALLY | FRIEND)
    }

    /// `actor`'s combat group (module notes), itself first, then the others by FormID:
    /// every actor in a combat state that is its comrade and has a foe in common with it.
    pub fn combat_group(&self, actor: FormId) -> Vec<FormId> {
        let mine = self.foes_of(actor);
        let mut out = vec![actor];
        for (&id, a) in &self.awareness.actors {
            if id == actor || !a.state.in_combat() || !self.comrades(actor, id) {
                continue;
            }
            if a.targets.keys().chain(a.alarms.iter().map(|x| &x.owner)).any(|f| mine.contains(f)) {
                out.push(id);
            }
        }
        out
    }

    /// The group threat ratio and the personal one of `actor` from the strengths the
    /// engine set: its group's against every foe of the group whose strength is known.
    pub fn threat_ratios(&self, actor: FormId) -> (Option<f64>, Option<f64>) {
        let s = &self.awareness.tactics.strengths;
        let group = self.combat_group(actor);
        let foes: BTreeSet<FormId> = group.iter().flat_map(|m| self.foes_of(*m)).collect();
        let mine: Vec<Strength> = group.iter().filter_map(|m| s.get(m).copied()).collect();
        let theirs: Vec<Strength> = foes.iter().filter_map(|f| s.get(f).copied()).collect();
        let me = s.get(&actor).copied();
        (group_threat_ratio(&mine, &theirs), me.and_then(|me| personal_threat_ratio(me, &mine, &theirs)))
    }

    /// A flee check of `actor` with this fight's confidence `modifier` (module notes;
    /// `crate::combat_style::flees` applies the modifier). Frenzied actors (Aggression 3,
    /// docs/spec-notes/factions.md §4) never flee [documented: "Frenzied enemies will not flee"].
    pub fn flee_check(&mut self, actor: FormId, modifier: f64) -> FleeCheck {
        let base = actor_value(self, actor, crate::actor_values::CONFIDENCE).unwrap_or(2.0).round() as i64;
        let rating = (base + self.awareness.tactics.boosts.get(&actor).copied().unwrap_or(0) as i64).min(4);
        let threshold = crate::combat_style::confidence_threshold(rating as f64, |name, d| crate::combat::setting(self, name, d));
        let frenzied = actor_value(self, actor, crate::actor_values::AGGRESSION).unwrap_or(0.0) >= 3.0;
        let (group_ratio, personal_ratio) = self.threat_ratios(actor);
        let survival = self.combat_style(actor).is_some_and(|s| s.flee_personal_survival);
        let by_group = group_ratio.is_some_and(|r| flees(r, threshold, modifier));
        let by_self = !by_group && survival && personal_ratio.is_some_and(|r| flees(r, threshold, modifier));
        FleeCheck { group_ratio, personal_ratio, threshold, rating, flees: !frenzied && (by_group || by_self), personal: by_self }
    }

    /// `actor` starts fleeing from `from`, at `at` (again, from its flee spot, when it
    /// fled already: it looks for a new spot).
    pub fn start_flight(&mut self, actor: FormId, from: FormId, at: [f32; 3]) {
        let now = self.time;
        self.awareness.tactics.flights.insert(actor, Flight { from, from_at: at, since: now, spot: None, arrived: None });
        self.awareness.tactics.posts.remove(&actor);
        // "Flee: played when an actor begins to flee. Timing: always plays" [documented:
        // GECK wiki Combat Tab].
        self.detection_bark(actor, Some(from), "Flee");
    }

    /// A fleeing actor at its flee spot was attacked or cornered: its confidence goes up a
    /// level for this fight [documented: Confidence, Flee Behavior]. Returns the new
    /// number of boosts.
    pub fn boost_confidence(&mut self, actor: FormId) -> u8 {
        let b = self.awareness.tactics.boosts.entry(actor).or_default();
        *b = b.saturating_add(1);
        *b
    }

    /// Whether `actor`, in a combat state, flees and has reached its flee spot (or is
    /// cornered): there a person hides crouched (docs/design/combat-ai.md §12.2.1).
    pub fn at_flee_spot(&self, actor: FormId) -> bool {
        self.awareness.state(actor).in_combat() && self.awareness.tactics.flights.get(&actor).is_some_and(|f| f.arrived.is_some())
    }

    /// A faction flagged Special Combat that `a` and `b` both belong to, if any
    /// (docs/spec-notes/actor-flags.md §8).
    pub fn special_combat_faction(&self, a: FormId, b: FormId) -> Option<FormId> {
        let theirs = crate::functions::factions_of(self, b);
        crate::functions::factions_of(self, a)
            .into_iter()
            .filter(|f| theirs.contains(f))
            .find(|f| self.data.faction_flags.get(f).is_some_and(|fl| fl & crate::crime::faction_flags::SPECIAL_COMBAT != 0))
    }

    /// `actor` stops fleeing and fights again.
    pub fn end_flight(&mut self, actor: FormId) {
        self.awareness.tactics.flights.remove(&actor);
    }

    /// The first of `foe`'s side to start this fight: the member of its combat group that
    /// went to Combat earliest (ties by FormID), `foe` itself when none of them is
    /// fighting [observed: in the Goodsprings gunfight every townsperson's OnStartCombat
    /// named GSPG03Ref, the first Ganger to start, observer-jip-audit-2026-10-25.md §3.5;
    /// docs/spec-notes/combat-groups.md §2]. A member dead or out cold (knocked out, or
    /// set unconscious) is passed over: it is not fighting [design].
    pub fn first_starter(&self, foe: FormId) -> FormId {
        let started = &self.awareness.tactics.started;
        self.combat_group(foe).into_iter().filter(|m| *m == foe || (!self.is_dead(*m) && !self.is_out_cold(*m))).filter_map(|m| started.get(&m).map(|t| (*t, m))).min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))).map_or(foe, |(_, m)| m)
    }

    /// Who may hear an alarm, and where: the actors measured this round (`near`), then
    /// the other actors of the loaded cells where the world has them (not drawn yet, or
    /// out of the high process) [design: #972, the original's allies reached past the
    /// actors near the player; docs/spec-notes/combat-groups.md §1].
    fn alarm_hearers(&self, near: &BTreeMap<FormId, [f32; 3]>) -> BTreeMap<FormId, [f32; 3]> {
        let mut out = near.clone();
        for r in self.package_actors() {
            if out.contains_key(&r) || !self.cell_of(r).is_some_and(|c| self.loaded_cells.contains(&c)) {
                continue;
            }
            if let Some(pos) = self.world.peek(r).map(|s| s.pos).or_else(|| self.world.placed.get(r).map(|p| p.pos)) {
                out.insert(r, pos);
            }
        }
        out
    }

    /// Before a round's [`Game::settle_awareness`]: each fighter in Combat learns when a
    /// member of its combat group (a comrade with the same target) last detected each
    /// target it does not detect itself, and where; it neither loses nor forgets a target
    /// a mate still detects (`Awareness::settle`) and closes in on where the mate saw it
    /// [hypothesis: the original's fighters stayed in combat with a hidden player for half
    /// an hour while only some of them detected it, observer-jip-audit-2026-10-25.md §3.3;
    /// docs/spec-notes/combat-groups.md §3].
    pub fn share_sightings(&mut self) {
        // Who detected each target, latest first.
        let mut seen: BTreeMap<FormId, Vec<(f64, FormId, [f32; 3])>> = BTreeMap::new();
        for (&id, a) in &self.awareness.actors {
            if a.state != State::Combat {
                continue;
            }
            for (&t, x) in &a.targets {
                if let Some(d) = x.detected {
                    seen.entry(t).or_default().push((d, id, x.at));
                }
            }
        }
        for v in seen.values_mut() {
            v.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        }
        let mut mates: BTreeMap<(FormId, FormId), bool> = BTreeMap::new();
        let mut learnt = Vec::new();
        for (&id, a) in &self.awareness.actors {
            if a.state != State::Combat {
                continue;
            }
            for (&t, x) in &a.targets {
                if x.band == crate::detection::Band::Detected {
                    continue;
                }
                let own = x.detected.unwrap_or(f64::NEG_INFINITY);
                let mate = seen.get(&t).into_iter().flatten().take_while(|(d, ..)| *d > own).find(|(_, m, _)| *m != id && *mates.entry((id, *m)).or_insert_with(|| self.comrades(id, *m)));
                if let Some(&(d, _, at)) = mate {
                    learnt.push((id, t, d, at));
                }
            }
        }
        for (id, t, d, at) in learnt {
            let a = self.awareness.actors.get_mut(&id).expect("listed");
            a.shared.insert(t, d);
            if let Some(x) = a.targets.get_mut(&t) {
                x.at = at;
            }
        }
    }

    /// After a round of measurements and [`Game::settle_awareness`]: every actor that
    /// started fighting (→ Combat) or noticed a hostile (Normal → Alert) since the
    /// engine last took the changes alarms its comrades within [`ALARM_RADIUS`] (module
    /// notes): those measured in this round (`near`: drawn, in the high process) and the
    /// other actors of the loaded cells (`alarm_hearers`). The alarm names the
    /// first of the foe's side to start ([`Game::first_starter`]). An alarm passed on by
    /// an ally is not passed on again. Returns how many were alarmed.
    pub fn raise_alarms(&mut self, near: &BTreeMap<FormId, [f32; 3]>) -> usize {
        let changes: Vec<_> = self.awareness.changes.iter().filter(|c| matches!(c.to, State::Combat | State::Alert) && c.from != State::Combat && c.cause != Some(Cause::Ally)).copied().collect();
        let mut alarmed = 0;
        if changes.is_empty() {
            return alarmed;
        }
        let s = self.group_settings();
        let hearers = self.alarm_hearers(near);
        for c in changes {
            let (Some(about), Some(from)) = (c.about, near.get(&c.actor).copied()) else { continue };
            let at = c.at.or_else(|| near.get(&about).copied()).unwrap_or(from);
            let starter = self.first_starter(about);
            let starter_at = hearers.get(&starter).copied().unwrap_or(at);
            let combat = c.to == State::Combat;
            // "If a faction is flagged as Special Combat, members can attack one another
            // without sending a combat alarm" [documented: GECK wiki Category:Factions].
            if let Some(faction) = self.special_combat_faction(c.actor, about) {
                self.awareness.tactics.notes.push(GroupNote::SpecialCombat { sender: c.actor, about, faction });
                continue;
            }
            // Fighting: an alarm only when the foe is more than a minimal threat.
            if combat {
                let (ratio, _) = self.threat_ratios(c.actor);
                let threshold = self.confidence_threshold(c.actor);
                let ac = alarm_confidence(threshold, s.alarm_confidence_mult);
                if let Some(r) = ratio.filter(|r| *r >= ac) {
                    self.awareness.tactics.notes.push(GroupNote::NoAlarm { sender: c.actor, about, ratio: r, alarm_confidence: ac });
                    continue;
                }
            }
            for (&receiver, &where_) in &hearers {
                if receiver == c.actor || receiver == about || receiver == FormId::PLAYER_REF || (where_[0] - from[0]).hypot(where_[1] - from[1]) > ALARM_RADIUS {
                    continue;
                }
                if self.is_dead(receiver) || !self.present(receiver) || self.awareness.state(receiver) == State::Combat {
                    continue;
                }
                // The foe's first starter, unless that is this receiver or its comrade; the
                // foe it was called about stays `foe`, for its grudges.
                let foe = about;
                let (about, at) = if receiver == starter || self.comrades(receiver, starter) { (about, at) } else { (starter, starter_at) };
                let relation = self.reaction_to(receiver, c.actor);
                let assistance = actor_value(self, receiver, crate::actor_values::ASSISTANCE).unwrap_or(0.0) as i64;
                // Its own kind (a faction or base in common) always hears it [design];
                // allies and friends by their Assistance [documented: GetFactionCombatReaction].
                let kin = self.kin(receiver, c.actor);
                if !kin && !crate::provocation::helps(assistance, relation) {
                    continue;
                }
                if self.awareness.get(receiver).is_some_and(|a| a.alarms.iter().any(|x| x.owner == about)) {
                    continue;
                }
                // A grudge against the foe it was called about, and against the starter it
                // is sent after when that is another, so it can still fight the foe once the
                // starter is gone. Never against its own allies and friends; a coward stays
                // out (`crate::provocation`, item 14 of playtest list 4).
                let grudges: Vec<FormId> = [foe, about].into_iter().filter(|&x| !self.would_attack(receiver, x)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
                if !grudges.is_empty() {
                    if grudges.iter().any(|&x| self.comrades(receiver, x)) || self.stays_out(receiver, c.actor) {
                        continue;
                    }
                    let now = self.time;
                    for x in grudges {
                        self.provocations.grudges.insert((receiver, x), Grudge { why: Why::Assist { victim: c.actor }, since: now });
                    }
                }
                let t = self.awareness.timers.unwrap_or_default();
                let now = self.time;
                self.awareness.actors.entry(receiver).or_default().alarm(about, at, 0.0, Cause::Ally, now, &t);
                self.awareness.tactics.notes.push(GroupNote::Alarm { sender: c.actor, receiver, about, combat });
                alarmed += 1;
            }
        }
        alarmed
    }

    /// `actor` was hit by `by`, another actor (not the player), and lives (module notes):
    /// it may turn on `by`. Returns whether it now holds a grudge. Called by
    /// [`Game::attacked_by`] for hits by actors other than the player.
    pub fn attacked_by_actor(&mut self, actor: FormId, by: FormId) -> bool {
        if self.provocations.holds(actor, by) || self.would_attack(actor, by) || self.is_dead(by) {
            return false;
        }
        if self.comrades(actor, by) {
            self.provocations.notes.push(Note::Ignored { actor, by, why: "a stray hit by an ally or friend" });
            return false;
        }
        // The owner's exception, for everyone: both already fighting someone else.
        let fights_other = |g: &Game, a: FormId, b: FormId| g.awareness.get(a).filter(|x| x.state == State::Combat).and_then(|x| x.combat_target()).is_some_and(|(t, _)| t != b);
        if fights_other(self, actor, by) && fights_other(self, by, actor) {
            self.provocations.notes.push(Note::Ignored { actor, by, why: "both were fighting someone else" });
            return false;
        }
        let now = self.time;
        let relation = self.reaction_to(actor, by);
        self.provocations.grudges.insert((actor, by), Grudge { why: Why::Assault, since: now });
        self.provocations.notes.push(Note::Assaulted { actor, by, relation, hits: 1 });
        self.detection_bark(actor, Some(by), "Assault");
        true
    }

    /// Who searches with `actor` for `about` (its search party: comrades searching for the
    /// same owner's alarm), and `actor`'s place among them: (index, count).
    pub fn search_party(&self, actor: FormId, about: FormId) -> (usize, usize) {
        let mut party: Vec<FormId> = self
            .awareness
            .actors
            .iter()
            .filter(|(id, a)| a.state.searching() && a.search_alarm().is_some_and(|x| x.owner == about) && self.comrades(actor, **id))
            .map(|(id, _)| *id)
            .collect();
        if !party.contains(&actor) {
            party.push(actor);
            party.sort();
        }
        (party.iter().position(|x| *x == actor).unwrap_or(0), party.len())
    }

    /// How many complete searches `actor` has made for `about`.
    pub fn searches_done(&self, actor: FormId, about: FormId) -> u32 {
        self.awareness.tactics.searched.get(&(actor, about)).copied().unwrap_or(0)
    }

    /// `actor` has searched its share round `about`'s alarm once more. After
    /// iCombatTargetLostRemoveSearchCount searches it drops `about`'s alarms that are not
    /// being refreshed now (a hostile still noticed keeps its alarm) [documented: Combat
    /// Search, "N complete searches … before dropping out of combat (assuming the player
    /// remains below the detection threshold)"]. Returns whether it dropped them.
    pub fn searched_once(&mut self, actor: FormId, about: FormId) -> bool {
        let needed = self.group_settings().remove_search_count;
        let n = self.awareness.tactics.searched.entry((actor, about)).or_default();
        *n += 1;
        let count = *n;
        self.awareness.tactics.notes.push(GroupNote::Searched { actor, about, count, needed });
        if count < needed {
            return false;
        }
        let now = self.time;
        if let Some(a) = self.awareness.actors.get_mut(&actor) {
            a.alarms.retain(|x| x.owner != about || (x.cause == Cause::Noticed && now - x.refreshed < 1.0));
            if a.targets.get(&about).is_some_and(|t| t.detected.is_none_or(|d| now - d > 1.0)) {
                a.targets.remove(&about);
            }
        }
        self.awareness.tactics.searched.remove(&(actor, about));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(health: f64, dps: f64) -> Strength {
        Strength { health, dps }
    }

    #[test]
    fn group_threat_ratio_sums_both_sides() {
        // Two troopers (100 health, 10 DPS) against one fiend (100, 10): 2.
        assert_eq!(group_threat_ratio(&[s(100.0, 10.0), s(100.0, 10.0)], &[s(100.0, 10.0)]), Some(2.0));
        // One against three: a third.
        let r = group_threat_ratio(&[s(100.0, 10.0)], &[s(100.0, 10.0); 3]).unwrap();
        assert!((r - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(group_threat_ratio(&[s(100.0, 10.0)], &[]), None);
        assert_eq!(group_threat_ratio(&[s(100.0, 10.0)], &[s(100.0, 0.0)]), Some(f64::INFINITY));
    }

    #[test]
    fn personal_survival_follows_the_wiki() {
        // My health 20 against foes averaging 10 DPS: 2 s; their 100 health against my
        // group's average 10 DPS: 10 s. Ratio 0.2: a cautious actor (0.375) flees though
        // its group (two even fighters against one) does not.
        let mine = [s(20.0, 10.0), s(100.0, 10.0)];
        let theirs = [s(100.0, 10.0)];
        let p = personal_threat_ratio(s(20.0, 10.0), &mine, &theirs).unwrap();
        assert!((p - 0.2).abs() < 1e-9);
        let g = group_threat_ratio(&mine, &theirs).unwrap();
        assert!(!flees(g, 0.375, 0.0) && flees(p, 0.375, 0.0));
    }

    #[test]
    fn alarms_follow_alarm_confidence() {
        // The wiki's examples: foolhardy against an even match sends one (1 < 1.5);
        // brave (0.5 in the example) against a weak foe (ratio 4) does not (4 >= 3).
        assert_eq!(alarm_confidence(0.0, 3.0), 1.5);
        assert!(1.0 < alarm_confidence(0.0, 3.0));
        assert!(4.0 >= alarm_confidence(0.5, 3.0));
    }

    #[test]
    fn calm_forgets_the_fight() {
        let mut t = Tactics::default();
        let (a, b) = (FormId(1), FormId(2));
        t.strengths.insert(a, s(1.0, 1.0));
        t.boosts.insert(a, 1);
        t.searched.insert((a, b), 1);
        t.posts.insert(a, Post { at: [0.0; 3], kind: PostKind::Cover, run: true });
        t.flights.insert(a, Flight { from: b, from_at: [0.0; 3], since: 0.0, spot: None, arrived: None });
        t.calm(a);
        assert!(t.strengths.is_empty() && t.boosts.is_empty() && t.searched.is_empty() && t.posts.is_empty() && t.flights.is_empty());
    }
}
