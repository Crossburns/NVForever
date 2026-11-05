//! Actors in a combat state (`fnv_game::awareness`; docs/design/alert.md §4): what the
//! AI has them do instead of their package.
//!
//! - **Alert and Lost**: the actor searches [documented: GECK wiki Category:Detection,
//!   "Alert and Lost actors search … if their package allows"]. It gets off furniture,
//!   walks to where its latest alarm is (what it noticed, heard or last saw), looks
//!   that way for a few seconds, then searches on: at the alarm again if it has moved
//!   (the hostile was noticed somewhere else), else at a random point around it within
//!   the first search radius. It walks, it does not run [engineering choice]. The Combat
//!   Search page that describes the original's search is not in the owner's export; the
//!   radii are FalloutNV.esm's fCombatSearchInteriorMinRadius and
//!   fCombatSearchExteriorMinRadius [observed: 256 and 512; hypothesis: the radius of
//!   the nearest search ring].
//! - **Combat**: the actor closes in on its target until it is within the distance it
//!   attacks from (`Awarenesses::engage`, which the engine's combat AI sets from its
//!   weapon), running, the way planned again as the target moves; there it stops and
//!   faces it. Without such a distance it holds its ground [design]. The attacks
//!   themselves are the engine's (nvforever `npc_combat`). A target it does not detect
//!   now it closes in on whatever the distance, halving the gap each time the way is
//!   planned, until it sees it again [design]. While it aims down its sights
//!   (`Awarenesses::aiming`, set by the combat AI) it walks instead of running, as the
//!   player does while aiming [design: the owner's wish, playtest list 2026-10-11, "the
//!   player and NPCs should not be able to move at full speed whilst aiming"].
//! - **Continue During Combat**: an actor whose current package has the flag carries
//!   its package out as if nothing had happened (`Game::continues_during_combat`).
//! - **Fleeing** (docs/design/combat-ai.md §12.2) [documented: GECK wiki Combat Flee]:
//!   an actor the combat AI has fleeing (`fnv_game::combat_group::Flight`, any combat
//!   state) runs, in this order of tries, through a door within fCombatFleeMaxDoorDistance
//!   that is not nearer its attacker than it is, to cover between
//!   fCombatFleeMinCoverDistance and fCombatFleeMaxCoverDistance from the attacker (a
//!   navmesh cover edge the attacker is beyond, `fnv_nav::CoverEdge`), or
//!   fCombatFleeNormalDistance straight away from it (turning up to 90° either way when
//!   the navmesh ends there); at the spot it stays, facing the attacker. Cornered (no
//!   way out) it stays where it is [design: the choice among doors and cover, nearest
//!   first]. At its spot or cornered a person hides crouched (`Game::sneaks`,
//!   `Game::at_flee_spot`; combat-ai.md §12.2.1).
//! - **Posts** (§12.1): a place the combat AI wants a fighter at (cover, a peek out of
//!   it, a flank, a step aside, a step back; `fnv_game::combat_group::Post`): it goes
//!   there, running unless it aims down its sights, and stands facing its target. A post
//!   it has no way to is dropped and it closes in as before. So is one whose way ends, or
//!   runs out of time, with the actor still [`POST_ARRIVAL`] or more from it: it holds or
//!   closes in on that same update, and what comes next is the combat AI's to choose.
//! - **Shared searches** (§12.5) [documented: GECK wiki Combat Search]: a hostile only
//!   noticed is searched for midway between the actor and it; the comrades searching for
//!   the same actor's alarm (`Game::search_party`) each take their own sector of the
//!   circle round the search's centre [design: the wiki splits the navmesh area; sectors
//!   stand in], [`SEARCH_POINTS`] points making one complete search; the first within
//!   the minimum search radius, later ones within the medium one; after
//!   iCombatTargetLostRemoveSearchCount complete searches the alarm is dropped
//!   (`Game::searched_once`).
//! - **Warnings** (in Normal; aggro radius and trespass, `Game::warning_to`,
//!   `fnv_game::aggro` "The look"): an actor warning the player stops and turns to it
//!   until the warning is over (it attacks, or the player is out of its radius or of the
//!   owned cell, or out of its sight), then goes back to its package as after a search
//!   [hypothesis].
//! - Back in **Normal**, the actor goes back to its package's place, as after a random
//!   conversation.

use crate::carry::{Activity, AiEvent};
use crate::goal;
use crate::Ai;
use fnv_game::awareness::{Cause, State};
use fnv_game::combat_group::{FleeBy, Flight, Post, PostKind, POST_ARRIVAL};
use fnv_game::game::Game;
use fnv_nav::{NavGraph, Place, Space};
use fnv_sim::FormId;
use serde::Serialize;

/// A fighter turns to face its target this many times faster than the standing turn
/// rate (240 degrees a second): the engine's gunmen fire only facing their target
/// (nvforever npc_combat, owner's playtest list 5 item 33), and at the standing rate a
/// turn to a new target held their fire for up to 1.5 s [design].
pub const COMBAT_TURN: f32 = 2.0;

/// Seconds an actor looks about where it searched before it searches on
/// [engineering choice].
pub const LOOK_SECONDS: f64 = 4.0;
/// A walk to a search point that has not arrived after this long ends where it is
/// [engineering choice].
pub const SEARCH_WALK_SECONDS: f64 = 30.0;
/// An alarm that moved this far from the point last searched is searched at again
/// [engineering choice].
pub const RECENTRE: f32 = 64.0;
/// Closing in on a target: the way is planned again when the target has moved this
/// far from where it was planned to, or after this long [design].
pub const CLOSE_REPLAN_DISTANCE: f32 = 96.0;
pub const CLOSE_REPLAN_SECONDS: f64 = 1.5;
/// Closing in ends this share of the stop distance short of the target, inside it
/// [design].
pub const STOP_SHORT: f32 = 0.9;
/// In Combat, a target it does not detect now: it closes in to this share of the
/// distance, again each time the way is planned [design].
pub const BLIND_CLOSE_SHARE: f32 = 0.5;
/// A fighter standing at its distance closes in on a target it no longer detects only
/// once it has gone this long without detecting it, seconds [design: owner's playtest
/// list 7 item 45, a recorded fight between NCR troopers and Fiends (smoke `battle-ncr-fiends`,
/// 2026-10-23): a Fiend 1,500 units from a trooper, within its stop distance, stopped and
/// set off again 3 times a second for 15 s, as its sight of the trooper came and went
/// behind the other fighters; closing in at the first frame unseen made it run and stop
/// on every flicker]. One already closing in goes on.
pub const BLIND_CLOSE_AFTER: f64 = 1.5;
/// Search points that make one complete search of an actor's share [design].
pub const SEARCH_POINTS: u32 = 3;
/// A post that moved less than this from where the way to it was planned is kept, game
/// units [design].
pub const POST_SLACK: f32 = 48.0;
/// One that has arrived goes on counting as there until it is this far from the post, game
/// units; past that it goes back [design: wider than [`POST_ARRIVAL`], so an actor shoved
/// about at the edge does not go back and forth between the two].
pub const POST_RETAINED: f32 = 96.0;
/// A flight or a walk to a post that has not arrived after this long ends where it is
/// [design].
pub const FLEE_WALK_SECONDS: f64 = 30.0;
/// Behind a cover edge a fleeing actor stands this far in from it, game units [design:
/// about an actor's width].
pub const COVER_STANDOFF: f32 = 48.0;

/// Why a walk to a post ended away from it ([`AiEvent::PostFailed`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PostFailure {
    /// [`FLEE_WALK_SECONDS`] passed with the actor still on its way.
    TimedOut,
    /// The way ended, or the actor stopped walking it, too far from the post.
    StoppedShort,
}

/// Where an actor in a combat state is ([`crate::Agent::search`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "search", rename_all = "snake_case")]
pub enum Search {
    /// Walking to a point to search, since `since` (`Game::time`): a search centred on
    /// `centre` for the alarm that was at `alarm` then; `points` searched of this
    /// complete search.
    Going { to: [f32; 3], since: f64, centre: [f32; 3], alarm: [f32; 3], points: u32 },
    /// Looking about at `at` until `until`.
    Looking { at: [f32; 3], until: f64, centre: [f32; 3], alarm: [f32; 3], points: u32 },
    /// In Combat: standing, facing its target.
    Holding { target: FormId },
    /// In Normal, giving aggro radius or trespass warnings: standing, facing whom it warns.
    Warning { target: FormId },
    /// In Combat: running up to its target, last planned for where it stood (`to`) at
    /// `since`.
    Closing { target: FormId, to: [f32; 3], since: f64 },
    /// In Combat: going to the post the combat AI chose, planned at `since`.
    Posted { kind: PostKind, target: FormId, to: [f32; 3], since: f64 },
    /// In Combat: at its post, facing its target.
    AtPost { kind: PostKind, target: FormId, at: [f32; 3] },
    /// Running from `from` to its flee spot `to`, since `since`.
    Fleeing { from: FormId, to: [f32; 3], since: f64 },
    /// At its flee spot (or cornered), facing `from`.
    Fled { from: FormId, at: [f32; 3] },
    /// Running to `from` (a weapon, a body, ammunition) at `to` to take up a better
    /// weapon (`crate::rearm`), since `since`.
    Fetching { from: FormId, to: [f32; 3], since: f64 },
}

fn midway(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl Ai {
    /// One update of an actor in a combat state (module notes). Returns whether the state
    /// holds the actor: its package's own doings wait meanwhile.
    pub(crate) fn alert_step(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, dt: f32) -> bool {
        let state = game.awareness.state(actor);
        if !state.in_combat() || game.continues_during_combat(actor) {
            game.rearms.calm(actor);
            // A spectator running from a fight that came too close (`crate::social`).
            if !state.in_combat() && self.spectator_step(game, nav, actor, dt) {
                return true;
            }
            if let Some(target) = game.warning_to(actor).filter(|_| !state.in_combat()) {
                if self.warn_step(game, actor, target, dt) {
                    return true;
                }
            }
            if let Some(s) = self.agents[&actor].search {
                let why = if matches!(s, Search::Warning { .. }) {
                    "its warning is over"
                } else if state.in_combat() {
                    "its package continues during combat"
                } else {
                    "back to Normal"
                };
                self.search_over(game, nav, actor, why);
            }
            return false;
        }
        // Relinquish the sandbox before any combat movement (or an early return
        // without a target). Repeated updates keep the new combat walk intact.
        self.interrupt_sandbox(game, actor);
        // Off furniture first: the engine gets it up, and its walk waits until it has.
        if self.agents[&actor].using.is_some() {
            self.set_using(game, actor, None);
            self.log(game, actor, AiEvent::Furniture { furniture: None });
        }
        // Fleeing comes first, in every combat state.
        if let Some(flight) = game.awareness.tactics.flights.get(&actor).copied() {
            self.flee_step(game, nav, actor, flight, dt);
            return true;
        }
        // Then a better weapon (`crate::rearm`).
        if self.rearm_step(game, nav, actor, dt) {
            return true;
        }
        let Some(aw) = game.awareness.get(actor) else { return true };
        let now = game.time;
        let a = &self.agents[&actor];
        let space = a.walker.space;
        if state == State::Combat {
            // The target it detects; just after losing it, the one detected last.
            let latest = aw.targets.iter().filter(|(_, t)| t.detected.is_some()).max_by(|x, y| x.1.detected.unwrap_or(0.0).total_cmp(&y.1.detected.unwrap_or(0.0)));
            let Some((target, at)) = aw.combat_target().or(latest.map(|(id, t)| (*id, t))).map(|(id, t)| (id, t.at)) else { return true };
            let sees = aw.combat_target().is_some();
            // Unseen only briefly: it counts as seen (`BLIND_CLOSE_AFTER`).
            let last_seen = aw.targets.get(&target).and_then(|t| t.detected);
            let sees = sees || last_seen.is_some_and(|t| now - t < BLIND_CLOSE_AFTER);
            let away = goal::dist(a.walker.feet, at);
            // A fighter on its way stands up: it never crouch-walks across a fight
            // (docs/design/combat-ai.md §14.2).
            if a.activity == Activity::Travelling {
                game.awareness.crouching.remove(&actor);
            }
            // A post the combat AI chose (cover, a flank …) comes before closing in.
            if let Some(post) = game.awareness.tactics.posts.get(&actor).copied() {
                if self.post_step(game, nav, actor, target, at, post, dt) {
                    return true;
                }
            }
            let a = &self.agents[&actor];
            // It stops within its distance and, standing, sets off again only when the
            // target is beyond what it can attack from, so it does not stop and start at
            // the edge.
            let holding = a.search == Some(Search::Holding { target });
            let engage = game.awareness.engage.get(&actor).copied();
            // A target it does not detect now (out of sight behind a rock, say) it closes
            // in on, whatever the distance, halving the gap, until it sees it again
            // [design: an actor that stood where it could not see its target went Lost
            // and back to Combat every few seconds at Cook-Cook's camp].
            let close_in = (!sees && engage.is_some()) || engage.is_some_and(|(stop, resume)| away > if holding { resume } else { stop });
            let stop = if sees { engage.map_or(0.0, |e| e.0) } else { away * BLIND_CLOSE_SHARE };
            let going = a.activity == Activity::Travelling;
            match (close_in, a.search) {
                // On its way, and the way still leads to the target.
                (true, Some(Search::Closing { target: t, to, since })) if t == target && going && goal::dist(to, at) < CLOSE_REPLAN_DISTANCE && now - since < CLOSE_REPLAN_SECONDS => {}
                (true, _) => self.close_in(game, nav, actor, target, at, away, stop),
                (false, Some(Search::Holding { target: t })) if t == target => {}
                (false, _) => {
                    let a = self.agents.get_mut(&actor).expect("listed");
                    a.walker.follower = None;
                    a.activity = Activity::There;
                    a.search = Some(Search::Holding { target });
                    self.log(game, actor, AiEvent::Hold { target, away });
                }
            }
            // Aiming down the sights slows it to a walk, also on a way already planned.
            let aiming = game.awareness.aiming.contains(&actor);
            let own = self.alert_speeds(actor);
            let (walk, run) = (own.walk, own.run * game.awareness.pace_of(actor));
            let a = self.agents.get_mut(&actor).expect("listed");
            if let Some(Search::Closing { target, .. }) = a.search {
                let speed = if aiming { walk } else { run };
                if a.walker.speed != speed {
                    a.walker.speed = speed;
                    self.log(game, actor, AiEvent::Pace { target, aiming });
                }
            }
            if self.agents[&actor].activity != Activity::Travelling {
                self.face(game, actor, Place { space, pos: at }, dt * COMBAT_TURN);
            }
            return true;
        }
        let Some(alarm) = aw.search_alarm().copied() else { return true };
        // A hostile only noticed is searched for midway between the actor and it
        // [documented: Combat Search]; anything else where it happened.
        let centre_for = |feet: [f32; 3]| if alarm.cause == Cause::Noticed { midway(feet, alarm.at) } else { alarm.at };
        let feet = a.walker.feet;
        match a.search {
            Some(Search::Going { to, since, centre, alarm: was, points }) => {
                if a.activity != Activity::Travelling || now - since > SEARCH_WALK_SECONDS {
                    let a = self.agents.get_mut(&actor).expect("listed");
                    a.walker.follower = None;
                    a.activity = Activity::There;
                    a.search = Some(Search::Looking { at: to, until: now + LOOK_SECONDS, centre, alarm: was, points });
                    self.log(game, actor, AiEvent::SearchLook { at: to, seconds: LOOK_SECONDS as f32 });
                }
            }
            Some(Search::Looking { until, centre, alarm: was, points, .. }) => {
                self.face(game, actor, Place { space, pos: alarm.at }, dt);
                if now >= until {
                    if goal::dist(was, alarm.at) > RECENTRE {
                        // The alarm moved (the hostile was noticed elsewhere): search there.
                        let c = centre_for(feet);
                        self.search_at(game, nav, actor, c, c, alarm.at, 0, alarm.owner);
                    } else {
                        // On round its share; each complete search counts.
                        let mut points = points + 1;
                        if points >= SEARCH_POINTS {
                            points = 0;
                            game.searched_once(actor, alarm.owner);
                        }
                        let (index, party) = game.search_party(actor, alarm.owner);
                        let indoors = matches!(space, Space::Interior(_));
                        let gs = game.group_settings();
                        let radii = if indoors { gs.search_interior } else { gs.search_exterior };
                        let r = if game.searches_done(actor, alarm.owner) == 0 { radii[0] } else { radii[1] };
                        let share = std::f32::consts::TAU / party.max(1) as f32;
                        let angle = share * (index as f32 + self.social.rng.range(0.0, 1.0));
                        let d = self.social.rng.range(0.4, 1.0) * r;
                        let to = [centre[0] + d * angle.cos(), centre[1] + d * angle.sin(), centre[2]];
                        self.search_at(game, nav, actor, to, centre, was, points, alarm.owner);
                    }
                }
            }
            _ => {
                let c = centre_for(feet);
                self.search_at(game, nav, actor, c, c, alarm.at, 0, alarm.owner);
            }
        }
        true
    }

    /// An actor giving aggro radius or trespass warnings (`Game::warning_to`) stops where
    /// it is and turns to whom it warns, at the standing turn rate, until the warning is
    /// over; then it goes back to its package as after a search [hypothesis: the
    /// creatures' warning idles want the actor standing, IsMoving != 1]. One on furniture
    /// stays on it. Returns whether the warning holds the actor.
    fn warn_step(&mut self, game: &mut Game, actor: FormId, target: FormId, dt: f32) -> bool {
        let Some(at) = goal::place_of_ref(game, target) else { return false };
        let a = &self.agents[&actor];
        if a.using.is_some() {
            return false;
        }
        if a.search != Some(Search::Warning { target }) {
            let away = goal::dist(a.walker.feet, at.pos);
            self.interrupt_sandbox(game, actor);
            let a = self.agents.get_mut(&actor).expect("listed");
            a.wait_until = None;
            a.walker.follower = None;
            a.activity = Activity::There;
            a.search = Some(Search::Warning { target });
            self.log(game, actor, AiEvent::Warn { target, away });
        }
        self.face(game, actor, at, dt);
        true
    }

    /// Set off to search at `to`, the navmesh point nearest it, in the search centred on
    /// `centre` for the alarm that was at `alarm`, `points` of its complete search done;
    /// without a way there the actor looks about where it stands.
    #[allow(clippy::too_many_arguments)]
    fn search_at(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, to: [f32; 3], centre: [f32; 3], alarm: [f32; 3], points: u32, about: FormId) {
        let params = self.path_params(game, actor);
        let speed = self.alert_speeds(actor).walk;
        let now = game.time;
        let a = self.agents.get_mut(&actor).expect("listed");
        a.wait_until = None;
        let space = a.walker.space;
        let from = a.walker.feet;
        let point = nav.nearest(space, to, 512.0).map(|(_, p)| p);
        a.walker.speed = speed;
        let path = point.and_then(|p| a.walker.go_to(nav, Place { space, pos: p }, &params).ok()).map(|path| path.length);
        match (point, path) {
            (Some(p), Some(length)) => {
                a.activity = Activity::Travelling;
                a.trip_started = now;
                a.trip_from = a.walker.travelled;
                a.search = Some(Search::Going { to: p, since: now, centre, alarm, points });
                self.log(game, actor, AiEvent::Search { about, to: p, away: goal::dist(from, p), length });
            }
            _ => {
                a.walker.follower = None;
                a.activity = Activity::There;
                a.search = Some(Search::Looking { at: from, until: now + LOOK_SECONDS, centre, alarm, points });
                self.log(game, actor, AiEvent::SearchLook { at: from, seconds: LOOK_SECONDS as f32 });
            }
        }
    }

    /// Run towards `target`, at `at`: to the point `stop` units short of it on the line
    /// from the target to the actor (the navmesh point nearest that), so that it ends
    /// within attacking distance and not against the target's body [design; owner
    /// 2026-10-11: a dog gets into range and bites, it does not push the player around].
    /// Without a way there the actor holds its ground.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn close_in(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, target: FormId, at: [f32; 3], away: f32, stop: f32) {
        let params = self.path_params(game, actor);
        let aiming = game.awareness.aiming.contains(&actor);
        let own = self.alert_speeds(actor);
        let speed = if aiming { own.walk } else { own.run * game.awareness.pace_of(actor) };
        let now = game.time;
        let a = self.agents.get_mut(&actor).expect("listed");
        let space = a.walker.space;
        let back = [a.walker.feet[0] - at[0], a.walker.feet[1] - at[1]];
        let len = back[0].hypot(back[1]).max(1e-3);
        let short = (stop * STOP_SHORT).min(len);
        let goal = [at[0] + back[0] / len * short, at[1] + back[1] / len * short, at[2]];
        let point = nav.nearest(space, goal, 512.0).map(|(_, p)| p);
        a.walker.speed = speed;
        let path = point.and_then(|p| a.walker.go_to(nav, Place { space, pos: p }, &params).ok()).map(|path| path.length);
        let first = !matches!(a.search, Some(Search::Closing { target: t, .. }) if t == target);
        match (point, path) {
            (Some(p), Some(length)) => {
                a.activity = Activity::Travelling;
                a.trip_started = now;
                a.trip_from = a.walker.travelled;
                a.search = Some(Search::Closing { target, to: at, since: now });
                if first {
                    self.log(game, actor, AiEvent::Close { target, to: p, away, length, aiming });
                }
            }
            _ => {
                a.walker.follower = None;
                a.activity = Activity::There;
                if a.search != Some(Search::Holding { target }) {
                    a.search = Some(Search::Holding { target });
                    self.log(game, actor, AiEvent::Hold { target, away });
                }
            }
        }
    }

    /// How fast `actor` goes to `post`: running if the post asks for it, walking while
    /// it aims down its sights (module notes).
    fn post_speed(&self, game: &Game, actor: FormId, post: Post) -> f32 {
        let own = self.alert_speeds(actor);
        if post.run && !game.awareness.aiming.contains(&actor) { own.run * game.awareness.pace_of(actor) } else { own.walk }
    }

    /// Go to the combat AI's `post`, or stand there facing `target` (at `at`). Returns
    /// false when the post is dropped: there is no way to it, or the walk there ended
    /// [`POST_ARRIVAL`] or more from it. The caller then holds or closes in; there is no
    /// retry here, the combat AI sees the post gone and chooses again.
    #[allow(clippy::too_many_arguments)]
    fn post_step(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, target: FormId, at: [f32; 3], post: Post, dt: f32) -> bool {
        let now = game.time;
        let a = &self.agents[&actor];
        let space = a.walker.space;
        match a.search {
            Some(Search::Posted { kind, to, since, .. }) if kind == post.kind && flat(to, post.at) < POST_SLACK => {
                let stopped = a.activity != Activity::Travelling;
                if stopped || now - since > FLEE_WALK_SECONDS {
                    let a = self.agents.get_mut(&actor).expect("listed");
                    a.walker.follower = None;
                    a.activity = Activity::There;
                    let feet = a.walker.feet;
                    let off = flat(feet, post.at);
                    // A door it was waiting at stays shut for all it knows: it did not pass.
                    if let Some(door) = a.waiting_at.take() {
                        self.log(game, actor, AiEvent::DoorWaitCancelled { door: FormId(door) });
                    }
                    let a = self.agents.get_mut(&actor).expect("listed");
                    if off >= POST_ARRIVAL {
                        a.search = None;
                        game.awareness.tactics.posts.remove(&actor);
                        let reason = if stopped { PostFailure::StoppedShort } else { PostFailure::TimedOut };
                        self.log(game, actor, AiEvent::PostFailed { post: kind, target, at: post.at, feet, off, elapsed: (now - since) as f32, reason });
                        return false;
                    }
                    a.search = Some(Search::AtPost { kind, target, at: post.at });
                    self.log(game, actor, AiEvent::AtPost { post: kind, target, off, at: post.at, feet });
                } else {
                    // Sights raised or lowered on the way change its pace there.
                    let speed = self.post_speed(game, actor, post);
                    let a = self.agents.get_mut(&actor).expect("listed");
                    if a.walker.speed != speed {
                        a.walker.speed = speed;
                        let aiming = game.awareness.aiming.contains(&actor);
                        self.log(game, actor, AiEvent::Pace { target, aiming });
                    }
                }
                return true;
            }
            // Moved off it (shoved, say), it goes back: the way below ends here or fails.
            Some(Search::AtPost { kind, at: here, .. }) if kind == post.kind && flat(here, post.at) < POST_SLACK && flat(a.walker.feet, post.at) < POST_RETAINED => {
                self.face(game, actor, Place { space, pos: at }, dt * COMBAT_TURN);
                return true;
            }
            _ => {}
        }
        let params = self.path_params(game, actor);
        let speed = self.post_speed(game, actor, post);
        let a = self.agents.get_mut(&actor).expect("listed");
        let from = a.walker.feet;
        let point = nav.nearest(space, post.at, 256.0).map(|(_, p)| p);
        a.walker.speed = speed;
        let path = point.and_then(|p| a.walker.go_to(nav, Place { space, pos: p }, &params).ok()).map(|path| path.length);
        match (point, path) {
            (Some(p), Some(length)) => {
                a.activity = Activity::Travelling;
                a.trip_started = now;
                a.trip_from = a.walker.travelled;
                a.search = Some(Search::Posted { kind: post.kind, target, to: post.at, since: now });
                self.log(game, actor, AiEvent::Post { post: post.kind, target, to: p, away: flat(from, p), length });
                true
            }
            _ => {
                game.awareness.tactics.posts.remove(&actor);
                self.log(game, actor, AiEvent::NoPost { post: post.kind });
                false
            }
        }
    }

    /// One update of a fleeing actor (module notes).
    fn flee_step(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, flight: Flight, dt: f32) {
        let now = game.time;
        // Where the one it flees is now (else where it was when the flight began).
        let threat = game.awareness.get(actor).and_then(|a| a.targets.get(&flight.from)).map_or(flight.from_at, |t| t.at);
        let a = &self.agents[&actor];
        let space = a.walker.space;
        match a.search {
            Some(Search::Fleeing { from, to, since }) if from == flight.from => {
                if a.activity != Activity::Travelling || now - since > FLEE_WALK_SECONDS {
                    let a = self.agents.get_mut(&actor).expect("listed");
                    a.walker.follower = None;
                    a.activity = Activity::There;
                    let feet = a.walker.feet;
                    a.search = Some(Search::Fled { from, at: feet });
                    if let Some(f) = game.awareness.tactics.flights.get_mut(&actor) {
                        f.arrived = Some(now);
                    }
                    let hides = game.sneaks(actor);
                    self.log(game, actor, AiEvent::FleeArrived { from, seconds: (now - since) as f32, off: flat(feet, to), hides });
                }
            }
            // At its spot until the combat AI has it flee again (a new flight: not arrived).
            Some(Search::Fled { from, .. }) if from == flight.from && flight.arrived.is_some() => {
                self.face(game, actor, Place { space, pos: threat }, dt);
            }
            _ => self.flee_from(game, nav, actor, flight.from, threat),
        }
    }

    /// Choose a flee spot away from `threat` and set off there, running (module notes).
    pub(crate) fn flee_from(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, from: FormId, threat: [f32; 3]) {
        // Spectators also enter here while Normal, outside alert_step's takeover.
        self.interrupt_sandbox(game, actor);
        let params = self.path_params(game, actor);
        let gs = game.group_settings();
        let now = game.time;
        let speed = self.alert_speeds(actor).run * game.awareness.pace_of(actor);
        let a = self.agents.get_mut(&actor).expect("listed");
        let (space, feet) = (a.walker.space, a.walker.feet);
        a.wait_until = None;
        a.walker.speed = speed;
        let mut tries: Vec<(Place, FleeBy)> = Vec::new();
        // 1. A door that leads elsewhere, not nearer the threat than the actor is.
        let mut doors: Vec<(f32, Place, FleeBy)> = Vec::new();
        for t in nav.triangles_near(space, feet[0], feet[1], gs.flee_max_door) {
            for e in nav.doors.get(&t).map_or(&[][..], |v| v.as_slice()) {
                let c = nav.centre(t);
                let d = flat(c, feet);
                if d > gs.flee_max_door || flat(c, threat) < flat(feet, threat) {
                    continue;
                }
                if let Some(to_space) = nav.space_of_tri(e.to) {
                    doors.push((d, Place { space: to_space, pos: e.arrival }, FleeBy::Door { door: FormId(e.door) }));
                }
            }
        }
        doors.sort_by(|x, y| x.0.total_cmp(&y.0));
        tries.extend(doors.into_iter().take(3).map(|d| (d.1, d.2)));
        // 2. Cover away from the threat, between the flee cover distances from it.
        let mut cover: Vec<(f32, [f32; 3])> = nav
            .cover_edges_near(space, threat, gs.flee_max_cover)
            .into_iter()
            .filter(|e| {
                let d = flat(e.mid, threat);
                d >= gs.flee_min_cover && d >= flat(feet, threat) && e.faces(threat)
            })
            .map(|e| (flat(e.mid, feet), e.behind(COVER_STANDOFF)))
            .collect();
        cover.sort_by(|x, y| x.0.total_cmp(&y.0));
        tries.extend(cover.into_iter().take(4).filter_map(|(_, p)| nav.nearest(space, p, 128.0)).map(|(_, p)| (Place { space, pos: p }, FleeBy::Cover)));
        // 3. Straight away from it, turning when the navmesh ends there.
        let (dx, dy) = (feet[0] - threat[0], feet[1] - threat[1]);
        let base = if dx.hypot(dy) > 1.0 { dy.atan2(dx) } else { 0.0 };
        for turn in [0.0f32, 30.0, -30.0, 60.0, -60.0, 90.0, -90.0] {
            let h = base + turn.to_radians();
            let p = [feet[0] + gs.flee_normal * h.cos(), feet[1] + gs.flee_normal * h.sin(), feet[2]];
            if let Some((_, p)) = nav.nearest(space, p, 512.0) {
                if flat(p, threat) > flat(feet, threat) {
                    tries.push((Place { space, pos: p }, FleeBy::Away));
                }
            }
        }
        for (place, how) in tries {
            let Ok(length) = a.walker.go_to(nav, place, &params).map(|p| p.length) else { continue };
            a.activity = Activity::Travelling;
            a.trip_started = now;
            a.trip_from = a.walker.travelled;
            a.search = Some(Search::Fleeing { from, to: place.pos, since: now });
            if let Some(f) = game.awareness.tactics.flights.get_mut(&actor) {
                f.spot = Some((place.pos, how));
            }
            let away = if place.space == space { flat(place.pos, threat) } else { f32::INFINITY };
            self.log(game, actor, AiEvent::Flee { from, how, to: place.pos, away, length });
            return;
        }
        // Nowhere to go: it stands, cornered.
        a.walker.follower = None;
        a.activity = Activity::There;
        a.search = Some(Search::Fled { from, at: feet });
        if let Some(f) = game.awareness.tactics.flights.get_mut(&actor) {
            f.arrived = Some(now);
        }
        let hides = game.sneaks(actor);
        self.log(game, actor, AiEvent::Cornered { from, hides });
    }

    /// The search or the stand ends: the actor goes back to its package.
    pub(crate) fn search_over(&mut self, game: &mut Game, nav: &NavGraph, actor: FormId, why: &'static str) {
        let a = self.agents.get_mut(&actor).expect("listed");
        a.search = None;
        if a.activity == Activity::Travelling {
            a.walker.follower = None;
            a.activity = Activity::There;
        }
        self.log(game, actor, AiEvent::SearchOver { why });
        self.back_to_package(game, nav, actor, |package, away| AiEvent::SearchBack { package, away });
    }
}
