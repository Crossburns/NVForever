//! Opt-in, phase-gated sandbox interruption smoke aid (#1497).
//!
//! The aid observes the real AI and furniture state. Its only interventions are the
//! public StartCombat/StopCombat operations.
//! In particular it never repairs pending choices, reservations, movement or packages.

use super::{in_loaded_space, Core};
use bevy::prelude::*;
use fnv_ai::{alert::Search, Activity, AiEvent, Doing};
use fnv_game::{ai::SandboxPhase, awareness::State, game::Game};
use fnv_sim::{FormId, RefTable};
use nvf_play::phys::PhysState;

const ENV: &str = "NVF_PLAY_TEST_SANDBOX_INTERRUPT";
const SETUP_SECONDS: f64 = 90.0;
const COMBAT_SECONDS: f64 = 30.0;
const RECOVERY_SECONDS: f64 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    Pending,
    Seated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    Stop,
    Control,
}

#[derive(Clone, Copy, Debug)]
enum Stage {
    Waiting,
    Combat { since: f64, saw_movement: bool, released: bool },
    Recovery { since: f64, choice: Option<Doing> },
    Finished,
}

pub(super) struct Probe {
    actor: FormId,
    target: FormId,
    start: Start,
    end: End,
    stage: Stage,
    begun: Option<f64>,
    package: Option<FormId>,
    /// Last successful choice event, retained before the resident logger drains it.
    /// This identifies the pending control's object without exposing private AI state.
    last_choice: Option<(FormId, Doing)>,
}

fn completed_phase(doing: Doing) -> SandboxPhase {
    match doing {
        Doing::Standing => SandboxPhase::Standing,
        Doing::Wandering => SandboxPhase::Wandering,
        Doing::Sitting(_) => SandboxPhase::Sitting,
        Doing::AtIdleMarker(_) => SandboxPhase::AtIdleMarker,
        Doing::Sleeping(_) => SandboxPhase::Sleeping,
        Doing::Eating(_) => SandboxPhase::Eating,
    }
}

impl Probe {
    pub(super) fn from_env(refs: &RefTable) -> Option<Self> {
        let value = std::env::var(ENV).ok()?;
        let parsed = Self::parse(&value, refs);
        if parsed.is_none() {
            warn!("sandbox interruption: invalid {ENV}; expected actor:pending|seated:target:stop|control (control needs pending)");
        }
        parsed
    }

    fn parse(value: &str, refs: &RefTable) -> Option<Self> {
        let fields: Vec<_> = value.split(':').map(str::trim).collect();
        let [actor, start, target, end] = fields.as_slice() else { return None };
        let resolve = |name: &str| {
            if name.eq_ignore_ascii_case("player") {
                Some(FormId::PLAYER_REF)
            } else {
                refs.by_edid(name).or_else(|| u32::from_str_radix(name.trim_start_matches("0x").trim_start_matches("0X"), 16).ok().map(FormId))
            }
        };
        let actor = resolve(actor)?;
        let target = resolve(target)?;
        let start = match *start { "pending" => Start::Pending, "seated" => Start::Seated, _ => return None };
        let end = match *end { "stop" => End::Stop, "control" => End::Control, _ => return None };
        if actor == FormId::PLAYER_REF || actor == target || (end == End::Control && start != Start::Pending) {
            return None;
        }
        Some(Self { actor, target, start, end, stage: Stage::Waiting, begun: None, package: None, last_choice: None })
    }

    fn snapshot(&self, core: &Core, game: &Game, label: &str) {
        let a = core.ai.agents.get(&self.actor);
        let furniture = game.extra.get(&self.actor).and_then(|e| e.furniture);
        info!(
            "sandbox interruption: {label} actor={} package={:?} time={:.3} awareness={:?} phase={:?} activity={:?} follower={} search={:?} using={:?} physical={:?} on_furniture={} feet={:?}",
            self.actor,
            game.extra.get(&self.actor).and_then(|e| e.current_package),
            game.time,
            game.awareness.state(self.actor),
            a.and_then(|a| a.sandbox.as_ref().map(|s| s.phase())),
            a.map(|a| a.activity),
            a.is_some_and(|a| a.walker.follower.is_some()),
            a.and_then(|a| a.search),
            a.and_then(|a| a.using),
            furniture,
            core.on_furniture.contains(&self.actor.0),
            a.map(|a| a.walker.feet),
        );
    }

    fn fail(&mut self, core: &Core, game: &Game, why: &str) {
        self.snapshot(core, game, "failure state");
        warn!("sandbox interruption: FAIL actor={} reason={why}", self.actor);
        self.stage = Stage::Finished;
    }

    /// Called after ordinary AI movement and before its events are consumed. No extra
    /// AI update is run, so cancellation and recovery belong to production scheduling.
    pub(super) fn tick(&mut self, core: &Core, game: &mut Game, phys: Option<&PhysState>) {
        if matches!(self.stage, Stage::Finished) {
            return;
        }
        let begun = *self.begun.get_or_insert_with(|| {
            info!("sandbox interruption: armed actor={} start={:?} target={} end={:?}", self.actor, self.start, self.target, self.end);
            game.time
        });
        if self.package.is_some() && game.extra.get(&self.actor).and_then(|e| e.current_package) != self.package {
            self.fail(core, game, "package changed; recovery not established");
            return;
        }
        if game.is_dead(self.actor) || !game.present(self.actor) {
            self.fail(core, game, "actor died or became absent");
            return;
        }
        if !matches!(self.stage, Stage::Waiting) && core.ai.events.iter().any(|e| e.actor == self.actor && matches!(e.event, AiEvent::Package { .. } | AiEvent::Resumed { .. })) {
            self.fail(core, game, "package restarted; retained-sandbox recovery not established");
            return;
        }
        if let Some(package) = game.extra.get(&self.actor).and_then(|e| e.current_package) {
            for e in core.ai.events.iter().filter(|e| e.actor == self.actor) {
                if let AiEvent::Sandbox { doing, .. } = e.event {
                    self.last_choice = Some((package, doing));
                }
            }
        }
        match self.stage {
            Stage::Waiting => {
                if game.time - begun > SETUP_SECONDS {
                    self.fail(core, game, "starting phase was not reached");
                    return;
                }
                // Allow the named resident to be drawn and initial loading/menu work
                // to finish; absence of a ready state is a failed setup, never PASS.
                if game.time - begun < 3.0 || !core.list.iter().any(|r| r.actor == self.actor && r.spawned) {
                    return;
                }
                let Some(a) = core.ai.agents.get(&self.actor) else { return };
                let Some(sandbox) = &a.sandbox else { return };
                if !phys.is_some_and(|p| in_loaded_space(&a.walker, p) && p.ready_at(a.walker.feet)) {
                    return;
                }
                if game.awareness.state(self.actor) != State::Normal || a.search.is_some() {
                    return;
                }
                let physical = game.extra.get(&self.actor).and_then(|e| e.furniture);
                let selected = self.last_choice.filter(|(package, _)| Some(*package) == game.extra.get(&self.actor).and_then(|e| e.current_package)).map(|(_, doing)| doing);
                let ready = match self.start {
                    Start::Pending => {
                        sandbox.phase() == SandboxPhase::ToObject && a.activity == Activity::Travelling && a.walker.follower.is_some()
                            && selected.is_some_and(|doing| matches!(doing, Doing::AtIdleMarker(_) | Doing::Sitting(_) | Doing::Sleeping(_) | Doing::Eating(Some(_))))
                    }
                    Start::Seated => {
                        a.activity == Activity::There && a.walker.follower.is_none() && sandbox.phase() == completed_phase(sandbox.doing)
                            && a.using.is_some() && sandbox.doing.furniture() == a.using && core.on_furniture.contains(&self.actor.0)
                            && physical.is_some_and(|f| f.state == 3 && Some(f.reference) == a.using)
                    }
                };
                if !ready {
                    return;
                }
                self.package = game.extra.get(&self.actor).and_then(|e| e.current_package);
                if self.package.is_none() {
                    self.fail(core, game, "starting actor has no package");
                    return;
                }
                self.snapshot(core, game, "precondition");
                info!("sandbox interruption: selected actor={} doing={selected:?}", self.actor);
                if self.end == End::Control {
                    info!("sandbox interruption: control continues without combat actor={}", self.actor);
                    self.stage = Stage::Recovery { since: game.time, choice: selected };
                } else if game.is_dead(self.target) || !game.present(self.target) {
                    self.fail(core, game, "combat target is dead or absent");
                } else {
                    game.start_combat(self.actor, self.target);
                    info!("sandbox interruption: StartCombat actor={} target={}", self.actor, self.target);
                    self.stage = Stage::Combat { since: game.time, saw_movement: false, released: self.start != Start::Seated };
                }
            }
            Stage::Combat { since, mut saw_movement, mut released } => {
                if game.time - since > COMBAT_SECONDS {
                    self.fail(core, game, "combat takeover or furniture release was not reached");
                    return;
                }
                if game.awareness.state(self.actor) != State::Combat {
                    self.fail(core, game, "combat state changed before StopCombat; direct recovery not established");
                    return;
                }
                let Some(a) = core.ai.agents.get(&self.actor) else { return };
                let approved = matches!(a.search,
                    Some(Search::Holding { target } | Search::Closing { target, .. } | Search::Posted { target, .. } | Search::AtPost { target, .. }) if target == self.target);
                if !approved && (a.search.is_some() || saw_movement) {
                    self.fail(core, game, "another movement owner intervened before StopCombat");
                    return;
                }
                if !saw_movement && approved {
                    saw_movement = true;
                    self.snapshot(core, game, "combat takeover");
                }
                let off_furniture = !core.on_furniture.contains(&self.actor.0) && game.extra.get(&self.actor).and_then(|e| e.furniture).is_none();
                if !released && off_furniture {
                    released = true;
                    self.snapshot(core, game, "furniture released");
                }
                if !saw_movement || !released || (self.start == Start::Seated && !off_furniture) || game.time - since < 1.0 {
                    self.stage = Stage::Combat { since, saw_movement, released };
                    return;
                }
                match self.end {
                    End::Stop => {
                        game.stop_combat(self.actor);
                        info!("sandbox interruption: StopCombat actor={}", self.actor);
                    }
                    End::Control => unreachable!("control never starts combat"),
                }
                self.stage = Stage::Recovery { since: game.time, choice: None };
            }
            Stage::Recovery { since, mut choice } => {
                if game.time - since > RECOVERY_SECONDS {
                    self.fail(core, game, "no completed sandbox activity after recovery");
                    return;
                }
                if game.awareness.state(self.actor) != State::Normal || core.ai.agents.get(&self.actor).is_some_and(|a| a.search.is_some()) {
                    self.fail(core, game, "another alert or search intervened during recovery");
                    return;
                }
                for e in core.ai.events.iter().filter(|e| e.actor == self.actor) {
                    if let AiEvent::Sandbox { doing, .. } = e.event {
                        if self.end == End::Control {
                            self.fail(core, game, "control selected another activity before its original object use completed");
                            return;
                        }
                        if doing != Doing::Standing {
                            choice = Some(doing);
                        }
                    }
                }
                self.stage = Stage::Recovery { since, choice };
                let Some(a) = core.ai.agents.get(&self.actor) else { return };
                let Some(sandbox) = &a.sandbox else { return };
                if a.search.is_some() || a.activity != Activity::There || a.walker.follower.is_some() || sandbox.phase() != completed_phase(sandbox.doing) {
                    return;
                }
                if choice != Some(sandbox.doing) {
                    return;
                }
                let complete = match sandbox.doing {
                    Doing::Standing => false,
                    Doing::AtIdleMarker(_) | Doing::Wandering => true,
                    Doing::Sitting(f) | Doing::Sleeping(f) | Doing::Eating(Some(f)) => {
                        a.using == Some(f) && core.on_furniture.contains(&self.actor.0) && game.extra.get(&self.actor).and_then(|e| e.furniture).is_some_and(|on| on.reference == f && on.state == 3)
                    }
                    Doing::Eating(None) => true,
                };
                if complete {
                    self.snapshot(core, game, "recovered");
                    info!("sandbox interruption: PASS actor={} mode={:?} completed={:?} seconds={:.3}", self.actor, self.end, sandbox.doing, game.time - since);
                    self.stage = Stage::Finished;
                }
            }
            Stage::Finished => {}
        }
    }
}
