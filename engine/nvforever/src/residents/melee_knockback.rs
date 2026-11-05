//! The world half of `NVF_PLAY_TEST_MELEE_KNOCKBACK` (the keys: nvf-play
//! `test_aid::melee_knockback`; #1386). As soon as the scripts run it turns the AI off
//! (SetActorsAI 0) on the target and on every actor of the loaded area, keeping it off for
//! those who arrive later; once the player stands on the ground it makes the one MoveTo; and
//! it says each frame whether the target stands ready for the blow, then watches that this
//! stays so up to the blow and through the push.
//!
//! The target is usually not loaded at the start: a reference far away has no state in the
//! sim's world until something asks for one ([`World::peek`](fnv_sim::World::peek) is
//! `None`, `World::get` makes it), and it is no resident until the MoveTo brings it near.
//! Neither makes it invalid: SetActorsAI only writes the game's per-reference record
//! (`fnv_game` `functions/ai_state.rs`), which needs no state, and `Ai::step_actor` reads
//! that record, so the AI is off before the actor joins and it never takes a step
//! [design]. Whether it can be the target is the placed table's to say.
//!
//! A standing actor has no character (`Core::think`, "no character is kept"): its knockback
//! makes one (`Core::carry`, `PhysState::add_npc`), so "settled" here is the feet still and
//! at the height a character would rest at, with a hit volume there, and any character it
//! has grounded [design].

use super::{in_loaded_space, is_actor, Core, Residents};
use bevy::prelude::*;
use fnv_game::functions::Ctx;
use fnv_game::game::Game;
use fnv_game::trace::EventKind;
use fnv_phys::Filter;
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;
use nvf_play::phys::PhysState;
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;
use nvf_play::MeleeKnockbackAid;

/// How long the feet stay put before they count as settled, seconds [design].
const SETTLE_SECONDS: f32 = 0.5;
/// How far ahead of the target the push's way must be clear, game units [design: well past
/// the 17 units of the Fists' power attack and the 91 seen asked of a stronger blow].
const CORRIDOR: f32 = 128.0;
/// How near that way another actor may not stand, game units [design: two actors' widths].
const CORRIDOR_HALF_WIDTH: f32 = 64.0;
/// Heights over the ground the way is tried at, game units [design: over a slope's rise, to
/// about an actor's head].
const CORRIDOR_HEIGHTS: [f32; 3] = [32.0, 64.0, 110.0];
/// How much the ground may rise or fall along the way, game units [design].
const CORRIDOR_RISE: f32 = 48.0;
/// How far the hit volume's middle may be from the feet: it follows the drawn actor's
/// bounds, which an idle sways [design].
const VOLUME_NEAR: f32 = 32.0;
/// How far the bottom of the hit volume may be from the feet, up or down: the lowest drawn
/// point of a standing pose [design].
const VOLUME_NEAR_Z: f32 = 24.0;
/// How far above or below where a character would rest on what is under them the feet may
/// be: more than this would drop or lift it when its knockback makes it [design].
const SUPPORT_NEAR: f32 = 4.0;
/// How far above and below where the MoveTo aims its support is looked for [design: well
/// over a slope's rise across the 90 units or so of the blow].
const SUPPORT_LOOK: [f32; 2] = [200.0, 400.0];

/// How long a point has stayed put.
#[derive(Default)]
struct Still {
    at: Option<([f32; 3], f32)>,
}

impl Still {
    fn seconds(&mut self, feet: [f32; 3], now: f32) -> f32 {
        match self.at {
            Some((at, since)) if (0..3).all(|i| (at[i] - feet[i]).abs() < 0.5) => now - since,
            _ => {
                self.at = Some((feet, now));
                0.0
            }
        }
    }
}

/// This half's progress.
#[derive(Default)]
pub(crate) struct Setup {
    player: Still,
    target: Still,
    /// The target's AI is off (from the first frame the scripts run).
    target_off: bool,
    /// Where the MoveTo put the target.
    put_at: Option<[f32; 3]>,
    /// Where it stood when it was last found ready.
    ready_at: Option<[f32; 3]>,
    /// The blow's knockback has begun.
    struck: bool,
    /// AI turned off on how many others so far.
    others: usize,
}

/// A script function called as the test aids call them.
fn call(game: &mut Game, name: &str, on: FormId, args: &[ArgValue]) -> Result<(), String> {
    let opcode = game.data.table.by_name(name).ok_or_else(|| format!("{name} is not a script function"))?;
    let ctx = Ctx { unit: "test aid".into(), opcode, on: Some(on), self_ref: Some(on), action_ref: None, quest: None, inst: None, in_condition: false };
    let from = game.events.len();
    game.call_function(&ctx, args);
    match game.events[from..].iter().find_map(|e| if let EventKind::CallFailed { error, .. } = &e.kind { Some(error.clone()) } else { None }) {
        Some(error) => Err(format!("{name}: {error}")),
        None => Ok(()),
    }
}

/// SetActorsAI 0.
fn ai_off(game: &mut Game, actor: FormId) -> Result<(), String> {
    call(game, "SetActorsAI", actor, &[ArgValue::Value(Value::Num(0.0))])
}

/// Whether a reference can be the target. Not whether it is loaded or has a state yet: see
/// the module's notes (the first run failed on that, the trooper being far away).
fn can_be_target(placed_actor: bool, dead: bool) -> Result<(), &'static str> {
    if !placed_actor {
        Err("the target is not a placed actor")
    } else if dead {
        Err("the target is dead")
    } else {
        Ok(())
    }
}

/// Whether feet at height `feet` stand where a character would rest, at height `rest`.
fn supported(feet: f32, rest: f32) -> bool {
    (feet - rest).abs() <= SUPPORT_NEAR
}

/// Whether no other actor stands within `half_width` of the way from `feet` along the unit
/// vector `dir`, `len` long.
fn clear_of_others(feet: [f32; 2], dir: [f32; 2], len: f32, half_width: f32, others: impl IntoIterator<Item = [f32; 2]>) -> bool {
    others.into_iter().all(|o| {
        let (x, y) = (o[0] - feet[0], o[1] - feet[1]);
        let along = (x * dir[0] + y * dir[1]).clamp(0.0, len);
        (x - dir[0] * along).hypot(y - dir[1] * along) > half_width
    })
}

/// Whether the push's way from the ground at `feet` along `dir` has no collision in it and
/// ground under it.
fn way_is_clear(phys: &mut PhysState, feet: [f32; 3], dir: [f32; 2]) -> Result<(), &'static str> {
    let things = || Filter { no_actors: true, exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), ..Default::default() };
    let z = feet[2];
    for h in CORRIDOR_HEIGHTS {
        if phys.ray([feet[0], feet[1], z + h], [dir[0], dir[1], 0.0], CORRIDOR, things()).is_some() {
            return Err("a clear way behind the target");
        }
    }
    for share in [0.25, 0.5, 0.75, 1.0] {
        let at = [feet[0] + dir[0] * CORRIDOR * share, feet[1] + dir[1] * CORRIDOR * share, z];
        if phys.ground_below(at, CORRIDOR_RISE, CORRIDOR_RISE).is_none() {
            return Err("level ground behind the target");
        }
    }
    Ok(())
}

/// The target's feet when it stands ready for the blow, or what it waits for.
fn standing_ready(core: &Core, game: &Game, phys: &mut PhysState, target: FormId, put_at: [f32; 3]) -> Result<[f32; 3], &'static str> {
    let id = target.0;
    if !game.present(target) {
        return Err("the target to be present");
    }
    if game.extra.get(&target).is_some_and(|e| e.dead || e.restrained || e.out_cold()) {
        return Err("a living, conscious, unrestrained target");
    }
    if !game.ai_off(target) {
        return Err("the target's AI to be off");
    }
    if game.awareness.state(target).in_combat() {
        return Err("the target to be out of combat");
    }
    if !core.list.iter().any(|r| r.actor == target && r.joined && r.spawned && !r.failed && !r.moved) {
        return Err("the target to be drawn where it was put");
    }
    let Some(a) = core.ai.agents.get(&target) else { return Err("the target to be a resident") };
    let feet = a.walker.feet;
    let near = |p: [f32; 3], r: f32| (p[0] - feet[0]).hypot(p[1] - feet[1]) <= r;
    if !near(put_at, 1.0) {
        return Err("the relocation to be acknowledged");
    }
    if a.walker.moving || a.walker.follower.is_some() {
        return Err("the target to stop walking");
    }
    if core.carried.contains_key(&id) || core.knocks.contains_key(&id) || core.on_furniture.contains(&id) || core.staggering.contains(&id) {
        return Err("no attack carry, knockback, furniture or stagger on the target");
    }
    if phys.npcs.get(&id).is_some_and(|b| !b.ch.grounded || b.velocity != [0.0, 0.0]) {
        return Err("the target's character to settle");
    }
    if !in_loaded_space(&a.walker, phys) || !phys.ready_around(feet) {
        return Err("collision around the target");
    }
    if !phys.ground_below(feet, 48.0, SUPPORT_NEAR).is_some_and(|g| supported(feet[2], g.point[2] + g.shell.max(0.0))) {
        return Err("the target's feet on what is under them");
    }
    if !phys.world.actor_volume(id).is_some_and(|v| near(v.feet, VOLUME_NEAR) && (v.feet[2] - feet[2]).abs() <= VOLUME_NEAR_Z) || phys.world.actor_aim_point(id).is_none() {
        return Err("the target's hit volume at its feet");
    }
    let Some(player) = phys.player.as_ref().map(|c| c.feet) else { return Err("the player's character") };
    let (dx, dy) = (feet[0] - player[0], feet[1] - player[1]);
    let len = dx.hypot(dy);
    if len < 1.0 {
        return Err("the target apart from the player");
    }
    let dir = [dx / len, dy / len];
    let others = core.ai.agents.iter().filter(|(actor, o)| **actor != target && o.walker.space == a.walker.space).map(|(_, o)| [o.walker.feet[0], o.walker.feet[1]]);
    if !clear_of_others([feet[0], feet[1]], dir, CORRIDOR, CORRIDOR_HALF_WIDTH, others) {
        return Err("no other actor on the way behind the target");
    }
    way_is_clear(phys, feet, dir)?;
    Ok(feet)
}

/// While Use is held and the swing plays: that nothing has changed since the target was
/// found ready. Later than this a knockback's report says so itself (`moved`, `walked`,
/// `alive`).
fn still_standing(core: &Core, game: &Game, target: FormId, put_at: [f32; 3], ready_at: [f32; 3]) -> Result<(), &'static str> {
    let same = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= 1.0);
    if !game.present(target) || game.extra.get(&target).is_some_and(|e| e.dead || e.restrained || e.out_cold()) {
        return Err("the target is gone, dead, out cold or restrained");
    }
    if !game.ai_off(target) {
        return Err("the target's AI came back on");
    }
    if !game.world.peek(target).is_some_and(|s| same(s.pos, put_at)) {
        return Err("a script moved the target");
    }
    match core.ai.agents.get(&target) {
        Some(a) if a.walker.moving || !same(a.walker.feet, ready_at) => Err("the target moved"),
        Some(_) => Ok(()),
        None => Err("the target is no longer a resident"),
    }
}

/// Turn the AI off on the actors of the loaded area that still have it on.
fn keep_ai_off(core: &Core, game: &mut Game, phys: &PhysState, target: FormId, others: &mut usize) {
    let awake: Vec<FormId> = core
        .list
        .iter()
        .filter(|r| r.joined && r.actor != target)
        .filter(|r| core.ai.agents.get(&r.actor).is_some_and(|a| in_loaded_space(&a.walker, phys)))
        .filter(|r| !game.ai_off(r.actor) && !game.is_dead(r.actor))
        .map(|r| r.actor)
        .collect();
    for actor in awake {
        match ai_off(game, actor) {
            Ok(()) => {
                *others += 1;
                info!("melee knockback aid: AI off on nearby actor {:08X} ({others} so far)", actor.0);
            }
            Err(e) => warn!("melee knockback aid: nearby actor {:08X}: {e}", actor.0),
        }
    }
}

/// One frame, after the residents' own.
pub(crate) fn arrange(mut res: ResMut<Residents>, mut aid: ResMut<MeleeKnockbackAid>, script: Option<ResMut<ScriptState>>, sim: Option<ResMut<SimState>>, phys: Option<ResMut<PhysState>>, time: Res<Time>, mut setup: Local<Setup>) {
    let res = &mut *res;
    let (aid, setup) = (&mut *aid, &mut *setup);
    if !res.started {
        aid.arranged = Err("the world to load");
        return;
    }
    let (Some(mut script), Some(mut sim), Some(mut phys)) = (script, sim, phys) else {
        aid.arranged = Err("the scripts and physics");
        return;
    };
    let Some(rt) = script.rt.as_mut() else {
        aid.arranged = Err("the scripts");
        return;
    };
    let core = &res.core;
    let phys = &mut *phys;
    let now = time.elapsed_secs();
    let target = aid.target;
    with_world(rt, &mut sim, |rt| {
        let game = &mut rt.game;
        keep_ai_off(core, game, phys, target, &mut setup.others);
        if aid.failed() {
            return;
        }
        // First of all, before it is a resident or anything can walk it.
        if !setup.target_off {
            if let Err(why) = can_be_target(is_actor(game.world.placed, target), game.is_dead(target)) {
                aid.fail(why);
                return;
            }
            if let Err(e) = ai_off(game, target) {
                aid.fail(&e);
                return;
            }
            info!("melee knockback aid: AI off on the target {:08X}", target.0);
            setup.target_off = true;
        }
        if !aid.waiting() {
            if let Some(run) = core.knocks.get(&target.0) {
                if !setup.struck {
                    setup.struck = true;
                    // MoveTo in the frames before this is in `from`, not in `moved`.
                    if setup.ready_at.is_none_or(|at| (run.from[0] - at[0]).hypot(run.from[1] - at[1]) > 1.0) {
                        aid.fail("the knockback began away from where the target was found ready");
                    }
                } else if !game.ai_off(target) {
                    aid.fail("the target's AI came back on during its knockback");
                }
            } else if let (false, Some(put_at), Some(ready_at)) = (setup.struck, setup.put_at, setup.ready_at) {
                if let Err(why) = still_standing(core, game, target, put_at, ready_at) {
                    aid.fail(&format!("the setup broke before the blow: {why}"));
                }
            }
            return;
        }
        let Some(put_at) = setup.put_at else {
            let Some(player) = phys.player.as_ref().filter(|c| c.grounded).map(|c| c.feet) else {
                aid.arranged = Err("the player on the ground");
                return;
            };
            if setup.player.seconds(player, now) < SETTLE_SECONDS || !phys.ready_around(player) {
                aid.arranged = Err("the player to settle");
                return;
            }
            // MoveTo goes from where the game has the player. Its height comes from what
            // is under the place, so the target rests there, not at the player's height
            // (the third number is on top of that).
            let Some(from) = game.world.peek(FormId::PLAYER_REF).map(|s| s.pos) else {
                aid.arranged = Err("the player's place in the game");
                return;
            };
            let [ox, oy, oz] = aid.offset;
            let Some(ground) = phys.ground_below([from[0] + ox, from[1] + oy, from[2]], SUPPORT_LOOK[0], SUPPORT_LOOK[1]) else {
                aid.fail("nothing under the place the target is to go");
                return;
            };
            let rest = ground.point[2] + ground.shell.max(0.0);
            let offsets = [ox, oy, rest - from[2] + oz].map(|v| ArgValue::Value(Value::Num(f64::from(v))));
            let [x, y, z] = offsets;
            if let Err(e) = call(game, "MoveTo", target, &[ArgValue::Value(Value::Ref(FormId::PLAYER_REF.0)), x, y, z]) {
                aid.fail(&e);
                return;
            }
            let Some(pos) = game.world.peek(target).map(|s| s.pos) else {
                aid.fail("the target is gone after its MoveTo");
                return;
            };
            info!("melee knockback aid: the one MoveTo of {:08X}, to ({:.1}, {:.1}, {:.1})", target.0, pos[0], pos[1], pos[2]);
            setup.put_at = Some(pos);
            aid.arranged = Err("the MoveTo to take effect");
            return;
        };
        match standing_ready(core, game, phys, target, put_at) {
            Ok(feet) if setup.target.seconds(feet, now) >= SETTLE_SECONDS => {
                if setup.ready_at.is_none() {
                    info!("melee knockback aid: target {:08X} in place at ({:.1}, {:.1}, {:.1}), the way behind it clear, {} other actors' AI off", target.0, feet[0], feet[1], feet[2], setup.others);
                }
                setup.ready_at = Some(feet);
                aid.arranged = Ok(());
            }
            Ok(_) => aid.arranged = Err("the target to settle"),
            Err(why) => aid.arranged = Err(why),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_far_away_is_a_valid_one() {
        // Being unloaded is no input: the reference's kind and life are all that count.
        assert_eq!(can_be_target(true, false), Ok(()));
        assert!(can_be_target(false, false).is_err());
        assert!(can_be_target(true, true).is_err());
    }

    #[test]
    fn feet_float_or_sink_only_a_little() {
        assert!(supported(100.0, 100.0));
        assert!(supported(103.0, 100.0));
        assert!(supported(97.0, 100.0));
        assert!(!supported(105.0, 100.0));
        assert!(!supported(95.0, 100.0));
    }

    #[test]
    fn an_actor_near_the_way_blocks_it() {
        let (feet, north) = ([0.0, 0.0], [0.0, 1.0]);
        // Beside the way, past its end, behind the start.
        assert!(clear_of_others(feet, north, 128.0, 64.0, [[65.0, 50.0], [0.0, 193.0], [0.0, -65.0]]));
        assert!(!clear_of_others(feet, north, 128.0, 64.0, [[63.0, 50.0]]));
        assert!(!clear_of_others(feet, north, 128.0, 64.0, [[0.0, 191.0]]));
        assert!(!clear_of_others(feet, north, 128.0, 64.0, [[0.0, -63.0]]));
        assert!(clear_of_others(feet, north, 128.0, 64.0, []));
    }

    #[test]
    fn feet_are_still_until_they_move() {
        let mut s = Still::default();
        assert_eq!(s.seconds([1.0, 2.0, 3.0], 10.0), 0.0);
        assert_eq!(s.seconds([1.0, 2.0, 3.2], 10.5), 0.5);
        assert_eq!(s.seconds([1.0, 3.0, 3.0], 11.0), 0.0);
        assert_eq!(s.seconds([1.0, 3.0, 3.0], 12.0), 1.0);
    }
}
