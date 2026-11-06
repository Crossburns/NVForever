//! Opt-in, read-only rendered combat check (combat-ai.md §20).
//! `NVF_PLAY_TEST_HELD_GRENADE=actorHex:grenadeHex:held|carried`.
//! Setup belongs to ordinary script calls. This observes both producer state and the
//! receiver's actual projectile insertion; it never changes a combat decision.

use bevy::prelude::*;
use fnv_actor::weapon::AnimType;
use fnv_combat::{CombatForms, projectile::Projectile, vats, vec::{self, V3}};
use fnv_game::Game;
use fnv_sim::{FormId, World};
use super::ActorFire;

const TAIL: f32 = 30.0;
const TIMEOUT: f32 = 110.0;

/// Captured before ranged/melee dispatch, not reconstructed at the receiver.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub clock: f32,
    pub dt: f32,
    pub actor: FormId,
    pub target: FormId,
    pub selected: Option<FormId>,
    pub held: Option<FormId>,
    pub collision_ready: bool,
    pub drawn: bool,
    pub ready: bool,
    pub person: bool,
    pub fleeing: bool,
    pub eyes: V3,
    pub feet: V3,
    pub target_feet: V3,
    pub gravity: f32,
    pub due: Option<f32>,
}

#[derive(Clone, Copy)]
struct Config { actor: FormId, grenade: FormId, held: bool }

#[derive(Clone, Copy)]
struct Ballistics { projectile: FormId, speed: f32, gravity: f32, blast: f32 }

#[derive(Resource)]
pub struct Probe {
    config: Option<Config>,
    started: Option<f32>,
    armed: Option<f32>,
    tracking: bool,
    frame: Option<Frame>,
    pending: bool,
    ballistics: Option<Ballistics>,
    gun: Option<FormId>,
    first_deadline: Option<f32>,
    before: i32,
    after: i32,
    requests: u32,
    accepted: u32,
    gun_shots: u32,
    last_gun_shot: Option<f32>,
    observed: f32,
    depleted: Option<(f32, f32)>,
    next_wait_log: f32,
    timed_out: bool,
    failed: bool,
    passed: bool,
}

impl Default for Probe {
    fn default() -> Self {
        let config = std::env::var("NVF_PLAY_TEST_HELD_GRENADE").ok().and_then(|value| {
            let parts: Vec<_> = value.split(':').collect();
            let parsed = (|| {
                if parts.len() != 3 { return None; }
                let id = |v: &str| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok().map(FormId);
                Some(Config { actor: id(parts[0])?, grenade: id(parts[1])?, held: match parts[2] { "held" => true, "carried" => false, _ => return None } })
            })();
            if parsed.is_none() { error!("held grenade probe: FAIL invalid configuration {value:?}"); }
            parsed
        });
        Self { config, started: None, armed: None, tracking: false, frame: None, pending: false, ballistics: None, gun: None, first_deadline: None, before: 0, after: 0, requests: 0, accepted: 0, gun_shots: 0, last_gun_shot: None, observed: 0.0, depleted: None, next_wait_log: 0.0, timed_out: false, failed: false, passed: false }
    }
}

fn count(world: &World<'_>, actor: FormId, grenade: FormId) -> i32 {
    world.peek(actor).and_then(|s| s.inventory.as_ref()).map_or(0, |i| i.count(grenade))
}

fn angle(a: V3, b: V3) -> f32 {
    let (a, b) = (vec::normalize(a), vec::normalize(b));
    (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0).acos().to_degrees()
}

fn firearm(forms: &CombatForms, weapon: FormId) -> bool {
    let animation = forms.weapons.get(&weapon).and_then(|w| w.anim_type.as_deref()).and_then(AnimType::from_name);
    matches!(animation, Some(AnimType::PistolBallistic | AnimType::PistolEnergy | AnimType::RifleBallistic | AnimType::RifleAutomatic | AnimType::RifleEnergy | AnimType::Handle | AnimType::Launcher))
        && forms.chain(weapon, None).is_some_and(|c| c.projectile.is_some())
}

impl Probe {
    pub fn actor(&self) -> Option<FormId> { self.config.map(|c| c.actor) }

    fn fail(&mut self, why: &str) {
        if !self.failed {
            error!("held grenade probe: FAIL {why}; frame={:?} counts={}->{} requests={} accepted={}", self.frame, self.before, self.after, self.requests, self.accepted);
        }
        self.failed = true;
    }

    /// Called once for the selected actor before it can emit. The previous frame
    /// must already have been acknowledged by the receiver, including empty frames.
    pub fn observe(&mut self, game: &Game<'_>, forms: &CombatForms, frame: Frame) {
        let Some(cfg) = self.config.filter(|c| c.actor == frame.actor) else { return };
        if self.pending { self.fail("producer overwrote an unacknowledged frame"); }
        self.frame = Some(frame);
        self.before = count(&game.world, cfg.actor, cfg.grenade);
        let ballistics = (|| {
            let chain = forms.chain(cfg.grenade, None)?;
            let projectile = chain.projectile?;
            let p = forms.projectiles.get(&projectile)?;
            let blast = forms.explosions.get(&p.explodes()?)?.radius;
            Some(Ballistics { projectile, speed: p.speed, gravity: p.gravity * frame.gravity, blast })
        })();
        self.ballistics = ballistics;
        let distance = vec::len(vec::sub(frame.target_feet, frame.eyes));
        let legal = ballistics.is_some_and(|b| b.speed.is_finite() && b.speed > 0.0 && b.gravity.is_finite() && b.gravity > 0.0 && b.blast.is_finite() && b.blast > 0.0 && distance > b.blast * 1.1 && distance <= vats::throw_reach(b.speed, b.gravity).min(1500.0));
        let intended_weapon = |w| if cfg.held { w == cfg.grenade } else { w != cfg.grenade && firearm(forms, w) };
        let intended = frame.held.is_some_and(intended_weapon);
        let selected = frame.selected.is_some_and(intended_weapon);
        let only_grenades = game.world.peek(cfg.actor).and_then(|s| s.inventory.as_ref()).is_some_and(|i| i.iter().all(|(id, n)| n <= 0 || id == cfg.grenade || !forms.weapons.contains_key(&id)));
        if self.armed.is_none() && frame.clock >= self.next_wait_log {
            info!("held grenade probe: waiting count={} distance={distance:.1} legal_band={legal} only_grenades={only_grenades} combat=true actor_dead={} target_dead={} frame={frame:?}", self.before, game.is_dead(cfg.actor), game.is_dead(frame.target));
            self.next_wait_log = frame.clock + 5.0;
        }
        if !self.tracking && self.before == 2 && selected {
            self.tracking = true;
            info!("held grenade probe: fixture inventory and selected weapon present; observing all subsequent requests");
        }
        if self.armed.is_none() && self.before == 2 && selected && intended && frame.selected == frame.held && frame.drawn && frame.ready && frame.person && !frame.fleeing && frame.collision_ready && legal && frame.target == FormId::PLAYER_REF && (!cfg.held || only_grenades) {
            self.armed = Some(frame.clock);
            self.gun = (!cfg.held).then_some(frame.held).flatten();
            info!("held grenade probe: armed mode={} actor={} held={:?} count=2 clock={:.3} distance={distance:.1} ready=true drawn=true", if cfg.held { "held" } else { "carried" }, cfg.actor, frame.held, frame.clock);
        }
        if self.tracking {
            if !frame.collision_ready { self.fail("actor or target collision space is not ready"); }
            if frame.fleeing || !frame.person || frame.target != FormId::PLAYER_REF { self.fail("invalid actor or target after arming"); }
            if self.before > 0 && (!selected || self.armed.is_some() && !intended || cfg.held && !only_grenades) { self.fail("weapon changed before depletion"); }
            let expected = if cfg.held { 2 - self.accepted as i32 } else { 2 };
            if self.before != expected { self.fail("unexplained inventory change between frames"); }
        }
    }

    /// Called even if the actor had no dispatch. A newly scheduled first deadline
    /// fills an absent snapshot only; a next deadline cannot replace the due throw.
    pub fn finish_frame(&mut self, game: &Game<'_>, clock: f32, deadline: Option<(FormId, f32)>, fighting: bool) {
        let Some(cfg) = self.config else { return };
        let start = *self.started.get_or_insert(clock);
        if self.pending { self.fail("receiver did not acknowledge prior frame"); }
        if let Some(f) = self.frame.as_mut().filter(|f| f.clock == clock) {
            if f.due.is_none() { f.due = deadline.filter(|(target, _)| *target == f.target).map(|(_, due)| due); }
            if self.tracking && cfg.held && self.first_deadline.is_none() {
                if let Some(due) = f.due {
                    self.first_deadline = Some(due);
                    info!("held grenade probe: first deadline={due:.3} observed clock={clock:.3}");
                }
            }
        } else {
            self.frame = None;
            if self.tracking { self.fail("selected actor has no dispatch snapshot"); }
        }
        self.after = count(&game.world, cfg.actor, cfg.grenade);
        if self.armed.is_none() && self.frame.is_none() && clock >= self.next_wait_log {
            info!("held grenade probe: waiting no dispatch actor={} clock={clock:.3} count={} selected={:?} combat={fighting} actor_dead={} player_dead={} deadline={deadline:?}; rendered/drawn/ready/collision/distance/legal_band/target unavailable", cfg.actor, self.after, game.held_weapon(cfg.actor).map(|(w, _)| w), game.is_dead(cfg.actor), game.is_dead(FormId::PLAYER_REF));
            self.next_wait_log = clock + 5.0;
        }
        if self.tracking {
            if !fighting || game.is_dead(cfg.actor) || game.is_dead(FormId::PLAYER_REF) { self.fail("combat ended or actor/target died"); }
            if !cfg.held && self.after != 2 { self.fail("carried control consumed a grenade"); }
        }
        if clock - start > TIMEOUT && !self.passed && !self.timed_out {
            self.timed_out = true;
            error!("held grenade probe: FAIL timeout before complete lifecycle; tracking={} armed={:?} combat={fighting} clock={clock:.3} count={} requests={} accepted={} observed_seconds={:.3} frame={:?}", self.tracking, self.armed, self.after, self.requests, self.accepted, self.observed, self.frame);
            self.failed = true;
        }
        // Before fixture activation, fight can run during loading while its
        // receiver is Playing-gated. Only actual lifecycle frames need an ack.
        self.pending = self.tracking;
    }

    /// Receiver boundary, before chain/projectile validation. A token is a request
    /// counter, so failures before projectile construction cannot disappear.
    pub fn request(&mut self, shot: &ActorFire, world: &World<'_>) -> Option<u32> {
        let cfg = self.config.filter(|c| c.actor == shot.shooter)?;
        // Initial scene combat can precede the ordinary script fixture setup. Its
        // emissions are outside the check; once the fixture appears this never resets.
        if !self.tracking { return None; }
        self.requests += 1;
        info!("held grenade probe: request={} actor={} weapon={} origin={:?} aim={:?} cone={} inventory={} frame={:?}", self.requests, shot.shooter, shot.weapon, shot.origin, shot.aim, shot.cone, count(world, cfg.actor, cfg.grenade), self.frame);
        if !self.pending || self.frame.is_none() || self.armed.is_none() { self.fail("request without an armed emission snapshot"); }
        if let Some(f) = self.frame {
            if cfg.held {
                if shot.weapon != cfg.grenade || self.depleted.is_some() || self.accepted >= 2 { self.fail("unexpected ranged emission in held lifecycle"); }
                if !f.ready || !f.drawn || f.fleeing || f.selected != Some(cfg.grenade) || f.held != Some(cfg.grenade) { self.fail("grenade emitted without ready held weapon"); }
                if !f.due.is_some_and(|due| f.clock + 0.001 >= due) { self.fail("grenade emitted before its deadline or without deadline"); }
                let expected = 2 - self.accepted as i32 - 1;
                if self.after != expected || count(world, cfg.actor, cfg.grenade) != expected { self.fail("grenade request did not consume exactly one item"); }
                if let Some(b) = self.ballistics {
                    let distance = vec::len(vec::sub(f.target_feet, f.eyes));
                    if !(distance > b.blast * 1.1 && distance <= vats::throw_reach(b.speed, b.gravity).min(1500.0)) { self.fail("grenade emitted outside its legal band"); }
                    let lob = vats::lob_dir(shot.origin, f.target_feet, b.speed, b.gravity);
                    if !lob.is_some_and(|lob| angle(lob, shot.aim) < 0.1 && angle(lob, vec::sub(f.target_feet, shot.origin)) > 2.1) { self.fail("request is not a discriminating low lob"); }
                    if vec::len(vec::sub(shot.origin, f.eyes)) > 0.1 || !shot.cone.is_finite() || !(0.0..=2.001).contains(&shot.cone) { self.fail("unexpected launch origin or spread"); }
                } else { self.fail("missing ballistic data"); }
            } else if Some(shot.weapon) != self.gun || shot.weapon == cfg.grenade {
                self.fail("carried control emitted another weapon");
            }
        }
        Some(self.requests)
    }

    /// Called immediately after the real Flying insertion, for EVERY projectile.
    pub fn accepted(&mut self, token: Option<u32>, shot: &ActorFire, projectile: &Projectile, projectile_form: FormId, visual_id: u64) {
        let Some(token) = token else { return };
        info!("held grenade probe: accepted request={token} actor={} weapon={} projectile={projectile_form} visual_id={visual_id} velocity={:?} gravity={} shooter={:?}", shot.shooter, shot.weapon, projectile.vel, projectile.gravity, projectile.shooter);
        let Some(cfg) = self.config else { return };
        if cfg.held {
            if let (Some(f), Some(b)) = (self.frame, self.ballistics) {
                let lob = vats::lob_dir(projectile.pos, f.target_feet, b.speed, b.gravity);
                let finite = projectile.vel.iter().all(|v| v.is_finite()) && projectile.gravity.is_finite();
                if !finite || projectile_form != b.projectile || (vec::len(projectile.vel) - b.speed).abs() > 0.1 || (projectile.gravity - b.gravity).abs() > 0.01 || !lob.is_some_and(|lob| angle(lob, projectile.vel) <= 2.1) || projectile.shooter != Some(cfg.actor.0 as u64) {
                    self.fail("accepted grenade has invalid low-lob physics or shooter");
                }
            } else { self.fail("accepted grenade lacks ballistic snapshot"); }
            self.accepted += 1;
        } else {
            self.gun_shots += 1;
            self.last_gun_shot = self.frame.map(|f| f.clock);
        }
    }

    pub fn finish_request(&mut self, token: Option<u32>, created: usize) {
        if token.is_some() && (created == 0 || self.config.is_some_and(|c| c.held) && created != 1) { self.fail("request did not create the required projectile count"); }
    }

    /// The only PASS site: after all messages and all their projectiles have been
    /// consumed, including the last quiet frame of the depletion observation.
    pub fn receiver_done(&mut self, clock: f32) {
        let Some(cfg) = self.config else { return };
        if !self.pending {
            if self.tracking { self.fail("missing producer observation frame"); }
            return;
        }
        self.pending = false;
        let Some(f) = self.frame else { return };
        if self.tracking && (!clock.is_finite() || !(0.0..=1.0).contains(&(clock - f.clock))) { self.fail("unobserved gap in combat simulation"); }
        if !f.dt.is_finite() || !(0.0..=1.0).contains(&f.dt) { self.fail("invalid observed simulation interval"); }
        if self.armed.is_none() { return; }
        // Only completed producer/receiver intervals count, never time spent in a
        // menu, dialogue, an absent actor update or waiting for the receiver.
        if !self.failed && self.armed != Some(f.clock) { self.observed += f.dt; }
        if !cfg.held && self.armed.is_some_and(|t| f.clock - t > 10.0) && !self.last_gun_shot.is_some_and(|t| f.clock - t <= 10.0) {
            self.fail("carried control stopped producing accepted gunfire");
        }
        if cfg.held {
            if self.after != 2 - self.accepted as i32 { self.fail("inventory and accepted projectiles disagree"); }
            if self.after == 0 && self.accepted == 2 && self.depleted.is_none() {
                self.depleted = Some((f.clock, self.observed));
                info!("held grenade probe: depleted clock={:.3} accepted=2 count=0; observing {TAIL:.0}s", f.clock);
            }
        }
        let complete = if cfg.held {
            self.depleted.is_some_and(|(_, at)| self.observed - at >= TAIL) && self.requests == 2 && self.accepted == 2 && self.after == 0
        } else {
            self.observed >= 35.0 && self.gun_shots >= 2 && self.last_gun_shot.is_some_and(|t| f.clock - t < 10.0) && self.after == 2
        };
        if complete && !self.failed && !self.passed {
            self.passed = true;
            info!("held grenade probe: PASS mode={} requests={} accepted={} gun_projectiles={} count={} clock={:.3} observed_seconds={:.3}", if cfg.held { "held" } else { "carried" }, self.requests, self.accepted, self.gun_shots, self.after, f.clock, self.observed);
        }
    }
}
