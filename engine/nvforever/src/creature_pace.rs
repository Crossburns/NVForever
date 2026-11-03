//! How fast creatures move, and the stance they move in (docs/spec-notes/actors.md §8.10;
//! the owner's playtest 2026-10-16: cazadors moved far too slowly and their walk stopped
//! and started).
//!
//! - **Speed**: a creature walks at the root motion of its `Forward` clip and runs at
//!   that of its `FastForward` clip, times its scale, its Speed Mult (CREA ACBS) and
//!   its skeleton's share ([`skeleton_share`]: 1 but for cazadors). The AI
//!   moves it at those speeds (`fnv_ai::Ai::speeds`).
//!   People walk and run at `fnv_ai::NPC_WALK` and `NPC_RUN` times their body armour's
//!   share ([`person_pace`]), looked at again every second. Before this every actor
//!   walked at 86.8 and ran at 344 units/s: a cazador's walk clip (217 units/s) played
//!   at its slowest rate (0.4), and 344 is the point between its walk and run clips
//!   (the geometric mean of 217 and 546), so a small slow-down swapped the two clips back
//!   and forth: the stop-and-start.
//! - **Clip rate**: the clips play `Speed Mult × the share` faster
//!   (`Locomotion::pace`), as the movement comes from them, and the move is chosen for the
//!   velocity divided by that: a medium cazador walking at 379 units/s plays its walk clip
//!   at 1.75, not its run clip at 0.7.
//! - **Turning**: a creature turns at its CREA `TNAM` × [`TNAM_TURN_MULT`] degrees a
//!   second, walking and standing, whatever its scale ([`creature_turn_rate`];
//!   [`CREATURE_TURN_RATE`] × its scale when TNAM is 0), and turns on the spot only when
//!   its way is more than 60 degrees off; the people's 120 degrees a second and 20
//!   degrees made a fast cazador stop at the start of every short walk.
//! - **Stance**: a creature whose folder has `h2h` movement clips (the cazador, the dog,
//!   the mirelurk king) moves on them while it is not in the Normal detection state
//!   (Alert, Combat, Lost: `locomotion::Locomotion::set_alert`), on its `mt` clips
//!   otherwise; the cazador scurries along the ground while calm and flies when alert
//!   [observed: the owner, 2026-10-16]. Its speeds follow the stance's clips. The
//!   sonic-bark cyberdog, in the dog's folder, keeps its `mt` clips ([`has_alert_stance`]).
//!
//! Speeds are given once the creature is drawn (its clips are then loaded), so one walking
//! off-screen before that moves at the people's speeds.

use crate::limbs::ActorRig;
use crate::locomotion::{ClipSet, Locomotion};
use crate::residents::Residents;
use bevy::prelude::*;
use fnv_actor::locomotion::Move;
use fnv_ai::{ActorSpeeds, Speeds};
use fnv_game::awareness::State;
use fnv_sim::FormId;
use nvf_play::script::ScriptState;

/// The share of the clip pace (root motion × scale × Speed Mult) a creature keeps when
/// its skeleton has none of its own ([`skeleton_share`]): the whole of it, walking and
/// running [observed: the AI observer in the original game, 2026-10-25, campaign
/// `creature_runs` on the Searchlight Airport runway (docs/spec-notes/actors.md §8.10),
/// steady stretches of 2–4 s; charges on the run clip of the stance they take in combat]:
/// a dog (scale 0.8) charged at 363 against its `h2hfastforward` 454.75 × 0.8 = 363.8, a
/// coyote (Speed Mult 110) 500 against 500.2, a radscorpion (0.6, 150) 281 against 283.5,
/// a bark scorpion (0.35, 250) 275 against 275.7, a feral ghoul 388 against 384, deathclaws
/// about 600 against 600 and 450 against 600 × 0.75; walks: radscorpions 67.0 against
/// 67.4, feral ghouls 97 against 95.7, deathclaws 119–121 against 120, mole rats 42.4
/// against 42.7, giant rats (1.1) 61.5 against 61.6, brahmin (1.25) 53.4 against 55.2;
/// a fleeing brahmin up to 234 against 222 (accelerating, 1.8 s). One per skeleton of the
/// base game and add-ons the same way (campaign `creature_runs_all`, actors.md §8.10):
/// within 6 % but for the protectron, among them every other flyer (eyebots, Mister
/// Gutsy, bloatflies) and robots, ants, centaurs, nightstalkers, spore carriers and
/// tunnelers, yao guai, roboscorpions, trogs, holograms and ghost people.
pub const CREATURE_SPEED_FIT: f32 = 1.0;

/// A skeleton's shares of its clips' pace, walking and running ([`clip_speeds`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Share {
    pub walk: f32,
    pub run: f32,
}

/// A skeleton's own shares of its clips' pace ([`CREATURE_SPEED_FIT`] for every other)
/// [observed: the AI observer in the original game]:
/// - cazadors (`nvcazadores`) 0.70 walking and running (2026-10-16: young ones at the
///   Tribal Village walked 228 against a clip pace of 325, medium ones 380 against 542;
///   charging, 576 and 974 against 818 and 1364). Why is open: no game setting gives 0.70,
///   every other flyer keeps the whole pace, and on the airport runway (2026-10-25,
///   `creature_runs_all`, `creature_flee`) a young cazador at Aggression 0 flew at
///   440–443 in three sessions, its `h2hforward` 298.15 × 0.5 × 3.0 = 447 whole, and dashed
///   at 680–1,090 for under a second;
/// - protectrons 0.88 walking, 1 running (2026-10-25: VCrTier0CrProtectron walked 22.0,
///   22.2 and 22.6 in three sessions against its `mtforward` 25.25, and ran a mean of 84.4
///   over 17 s against its `mtfastforward` 83.37).
///
/// The shares fitted before the airport runs (brahmin 1.4, mole rats 0.80, giant rats 0.76,
/// deathclaws and feral ghouls 0.70) came from a room without navmesh, a pack brahmin
/// keeping step with its merchant and a mole rat charging uphill (actors.md §8.10).
pub fn skeleton_share(skeleton: &str) -> Share {
    let path = fnv_vfs::normalize(skeleton);
    let whole = Share { walk: CREATURE_SPEED_FIT, run: CREATURE_SPEED_FIT };
    if path.contains("creatures\\nvcazadores\\") {
        Share { walk: 0.70, run: 0.70 }
    } else if path.contains("creatures\\protectron\\") {
        Share { walk: 0.88, ..whole }
    } else {
        whole
    }
}

/// Whether a creature of `skeleton` takes its folder's alert stance (`h2h` movement clips,
/// [`stance`]): every one but the sonic-bark cyberdog (`creatures\dog\skeleton_sonicbark.nif`)
/// [observed: the AI observer in the original game, 2026-10-25, campaign
/// `creature_runs_all`: NVDLC03CrCyberDogMilitary01Ranged (03015800, scale 1, Speed Mult
/// 100, a ranged attacker) charged at 333.1 on the `mt` run (`mtfastforward` 333.07), not
/// the dog folder's `h2hfastforward` 454.75, on which the plain dog charges (363 at scale
/// 0.8); hypothesis: that a creature attacking with a ranged weapon keeps its `mt` clips
/// when alert, not measured on any other skeleton]. Without it the cyberdog keeps its calm
/// clips, and its calm limp, in combat.
pub fn has_alert_stance(skeleton: &str) -> bool {
    !fnv_vfs::normalize(skeleton).ends_with("creatures\\dog\\skeleton_sonicbark.nif")
}

/// What a creature's CREA `TNAM` "Turning Speed" is multiplied by for its turn rate in
/// degrees per second, walking and standing [observed 2026-10-30: the AI observer in the
/// original game, campaign `o4turn` on the Searchlight Airport runway (issue #915,
/// actors.md §8.10 "TNAM"), headings every game loop: 35 of 38 steady turns of 13
/// creatures were TNAM × 1.25, × 3 or × 0.075 to within 1%, the base scale playing no
/// part (VCrCazadorTier1TypeA, scale 0.8, TNAM 200: 250.0 and 600.1); × 1.25 is the one
/// every creature showed on big turns, walking or standing (CrMoleRat 100: 125.0,
/// NVCrCazador 720: 900.2, CrDeathClaw 75: 93.7 five times)]. × 3, also on big turns
/// towards a target, is left out until what picks it is known (observer-checks 36), and
/// × 0.075 (aim tracking within about 6 degrees) is not a turn the walker makes [design].
pub const TNAM_TURN_MULT: f32 = 1.25;

/// A creature's turn rate at scale 1, degrees per second, when its TNAM is 0 (templated
/// records whose stats come from elsewhere carry 0; a mod's creature may too): it scales
/// with the creature's size [observed: medium cazadors (scale 1) turned at up to 304
/// degrees a second, young ones (scale 0.5) at up to 137, headings 0.16 s apart;
/// hypothesis: the rule; the fit came before TNAM was measured, [`TNAM_TURN_MULT`]].
pub const CREATURE_TURN_RATE: f32 = 300.0;

/// A creature's turn rate, degrees per second, walking and standing: its `tnam` (CREA
/// TNAM through the template chain, `Game::creature_turning_speed`) × [`TNAM_TURN_MULT`],
/// its `scale` playing no part; [`CREATURE_TURN_RATE`] × `scale` without a TNAM above 0.
pub fn creature_turn_rate(tnam: Option<f32>, scale: f32) -> f32 {
    match tnam.filter(|t| t.is_finite() && *t > 0.0) {
        Some(t) => t * TNAM_TURN_MULT,
        None => CREATURE_TURN_RATE * scale,
    }
}

/// How far off its way a standing creature may be before it turns on the spot instead
/// of turning as it sets off, radians [design: the cazadors made 85-89% of their fast
/// turns while moving].
pub const CREATURE_SPOT_TURN_START: f32 = 60.0 * std::f32::consts::PI / 180.0;

/// Changes between walk and run clips logged, the first ones only.
const SWAPS_LOGGED: usize = 20;

pub struct CreaturePacePlugin;

impl Plugin for CreaturePacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaceStats>()
            .add_systems(Update, (give_speeds.before(crate::residents::drive), people_speeds.after(give_speeds).before(crate::residents::drive), stance.after(nvf_play::PlaySet::Scripts).before(crate::locomotion::walk_test)))
            .add_systems(Last, report);
    }
}

/// Marks an actor [`give_speeds`] has seen, and a creature among them.
#[derive(Component)]
struct Paced;
#[derive(Component)]
struct PacedCreature;

#[derive(Resource, Default)]
struct PaceStats {
    paced: usize,
    /// People whose Speed Mult is not 100 (among `paced`).
    npcs: usize,
    /// Speeds given to people for their armour.
    people: usize,
    alerted: usize,
    calmed: usize,
    /// Frames of paced creatures playing a movement clip, and changes between a walk
    /// and a run clip among them (the stop-and-start of a speed on the switch point).
    moving_frames: usize,
    swaps: usize,
    last: std::collections::HashMap<u32, bool>,
}

/// A clip set's walk and run speeds for a creature: its clips' root motion × `scale` ×
/// `speed_mult` (percent) × its skeleton's `share`. Without a run clip it runs at its walk
/// clip's pace [observed: the AI observer in the original game, 2026-10-25,
/// `creature_flee`: told to run (an Always Run travel package), Dead Money's ghost people
/// ran a mean of 105.4 against their walk's 104.5, its holograms 102.9 against 102.3, Old
/// World Blues' 86.1 against 85.3]; a move without a clip keeps `default`'s speed. The
/// lakelurk has a run clip, `locomotion\mtfastforward.kf` (sequence `Fastforward`,
/// 355.95), and ran a mean of 358 over 3.9 s [observed, the same campaign].
pub fn clip_speeds(set: &ClipSet, scale: f32, speed_mult: f32, share: Share, default: Speeds) -> Speeds {
    let k = scale * speed_mult / 100.0;
    let clip = |m: Move| set.info(m).map(|i| i.speed).filter(|v| *v > 1.0);
    let walk = clip(Move::Forward).map(|v| v * k * share.walk);
    let run = clip(Move::FastForward).map(|v| v * k * share.run).or(walk);
    Speeds { walk: walk.unwrap_or(default.walk), run: run.unwrap_or(default.run) }
}

/// The share of the people's speeds (`fnv_ai::NPC_WALK`, `NPC_RUN`) a person keeps for
/// its body armour (`fnv_game::combat::armor_speed`: medium × 0.925, heavy × 0.85
/// [observed, actors.md §8.10]), on top of its Speed Mult's (`npc_speeds`). [observed
/// against the Speed Mult part, one actor: Cook-Cook, Speed Mult 110, ran 255 in metal
/// armour with the Incinerator drawn, 344 × 0.75 = 258; with his Speed Mult 284; asked in
/// docs/owner-review.md.]
pub fn person_pace(game: &fnv_game::Game, world: &fnv_sim::World, id: FormId) -> f32 {
    fnv_game::combat::armor_speed(game, fnv_game::combat::worn_armor_class(world, id)) as f32
}

/// A drawn person and the share of the people's speeds it was last given.
#[derive(Component)]
struct PersonPace(f32);

/// How often a drawn person's armour is looked at again, seconds: scripts and the
/// person's own outfitting change what it wears [design].
const PERSON_PACE_EVERY: f32 = 1.0;

/// Drawn people walk and run at the people's speeds × [`person_pace`].
fn people_speeds(
    time: Res<Time>,
    mut wait: Local<f32>,
    mut script: Option<ResMut<ScriptState>>,
    mut res: Option<ResMut<Residents>>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    mut people: Query<(&ActorRig, &mut PersonPace)>,
    mut stats: ResMut<PaceStats>,
) {
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = PERSON_PACE_EVERY;
    let (Some(res), Some(sim)) = (res.as_deref_mut(), sim) else { return };
    let default = res.default_speeds();
    let mut given = Vec::new();
    {
        let game = match script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            Some(rt) => &mut rt.game,
            None => match res.own_game() {
                Some(g) => g,
                None => return,
            },
        };
        for (rig, mut pace) in &mut people {
            let id = FormId(rig.reference);
            if id == FormId::PLAYER_REF {
                continue;
            }
            let share = person_pace(game, &sim.world, id);
            if (share - pace.0).abs() < 1e-3 {
                continue;
            }
            pace.0 = share;
            // On top of its Speed Mult's share (`npc_speeds`).
            let f = share * speed_mult(game, FormId(rig.base), id) / 100.0;
            let s = Speeds { walk: default.walk * f, run: default.run * f };
            info!("person pace: {} {id} walks {:.0} and runs {:.0} units/s ({:.1}% of the people's speeds for its armour)", game.data.name(FormId(rig.base)), s.walk, s.run, share * 100.0);
            given.push((id, ActorSpeeds { calm: s, alert: s, turn_rate: None, spot_turn_start: None }));
        }
    }
    for (id, s) in given {
        res.set_speeds(id, s);
        stats.people += 1;
    }
}

fn give_speeds(
    mut commands: Commands,
    mut script: Option<ResMut<ScriptState>>,
    mut res: Option<ResMut<Residents>>,
    mut actors: Query<(Entity, &ActorRig, &mut Locomotion, &GlobalTransform), Without<Paced>>,
    mut stats: ResMut<PaceStats>,
) {
    let Some(res) = res.as_deref_mut() else { return };
    if actors.is_empty() {
        return;
    }
    let default = res.default_speeds();
    let mut given = Vec::new();
    {
        let game = match script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            Some(rt) => &mut rt.game,
            None => match res.own_game() {
                Some(g) => g,
                None => return,
            },
        };
        for (e, rig, mut lo, global) in &mut actors {
            commands.entity(e).insert(Paced);
            let Some(def) = game.data.actors.get(&FormId(rig.base)) else { continue };
            if &def.sig != b"CREA" {
                if let Some(s) = npc_speeds(game, FormId(rig.base), FormId(rig.reference), default, &mut lo) {
                    given.push((FormId(rig.reference), s));
                    stats.npcs += 1;
                }
                // Its armour's share on top: `people_speeds`.
                commands.entity(e).insert(PersonPace(1.0));
                continue;
            }
            commands.entity(e).insert(PacedCreature);
            let id = FormId(rig.reference);
            // Speed Mult as GetBaseAV gives it: through the template chain (Stats), as the
            // leveled cazadors at the Tribal Village take theirs.
            let mult = speed_mult(game, FormId(rig.base), id);
            let scale = global.affine().transform_vector3(Vec3::Z).length().max(0.01);
            // Its skeleton's share; the clips play at that pace.
            let share = skeleton_share(&rig.skeleton);
            lo.pace = mult / 100.0 * share.walk;
            let calm = clip_speeds(&lo.stand, scale, mult, share, default);
            let alert = lo.alert.as_deref().map_or(calm, |a| clip_speeds(a, scale, mult, share, default));
            let tnam = game.creature_turning_speed(FormId(rig.base), id);
            let turn = creature_turn_rate(tnam, scale);
            info!(
                "creature pace: {} {id} walks {:.0} and runs {:.0} units/s, turns {turn:.0} degrees/s{} (scale {scale:.2}, Speed Mult {mult:.0}, TNAM {})",
                game.data.name(FormId(rig.base)),
                calm.walk,
                calm.run,
                if lo.alert.is_some() { format!(", alert {:.0} and {:.0}", alert.walk, alert.run) } else { String::new() },
                tnam.map_or("none".to_string(), |t| format!("{t:.0}"))
            );
            let turn = turn.to_radians();
            given.push((id, ActorSpeeds { calm, alert, turn_rate: Some(turn), spot_turn_start: Some(CREATURE_SPOT_TURN_START) }));
        }
    }
    for (id, s) in given {
        res.set_speeds(id, s);
        stats.paced += 1;
    }
}

/// An actor's Speed Mult (percent) as GetBaseAV gives it, through the template chain
/// (Stats). A reference a script made (PlaceAtMe) is not in the game's world outside
/// `with_world`: its base's stats source is read instead.
fn speed_mult(game: &mut fnv_game::Game<'_>, base: FormId, id: FormId) -> f32 {
    let v = if fnv_game::functions::base_of(game, id).is_some() {
        game.base_actor_value(id, fnv_game::actor_values::SPEED_MULTIPLIER).ok()
    } else {
        game.template_source(base, id, |d| d.template_stats).map(|(_, d)| d.speed_mult as f64)
    };
    v.filter(|v| *v > 0.0).map_or(100.0, |v| v as f32)
}

/// A person's speeds: the configured walking and running speeds × its Speed Mult
/// (percent, NPC_ ACBS through the template chain as GetBaseAV gives it): "A Speed actor
/// value is used as a multiplier times a standard movement speed. The Player Character
/// would have a Speed of 100%" [documented: GECK wiki Stats Tab - NPC]. Its clips play
/// that much faster so its feet keep up, as a creature's do [design: the wiki says the
/// original's do not ("the animations only sync up with a speed of 100%"); the owner's
/// pick between the two is asked in docs/owner-review.md]. `None` at 100 (most people):
/// they keep the defaults.
fn npc_speeds(game: &mut fnv_game::Game<'_>, base: FormId, id: FormId, default: Speeds, lo: &mut Locomotion) -> Option<ActorSpeeds> {
    if id == FormId::PLAYER_REF {
        return None;
    }
    let mult = speed_mult(game, base, id);
    if (mult - 100.0).abs() < 0.5 {
        return None;
    }
    let f = mult / 100.0;
    lo.pace = f;
    let calm = Speeds { walk: default.walk * f, run: default.run * f };
    info!("npc pace: {} {id} walks {:.0} and runs {:.0} units/s (Speed Mult {mult:.0})", game.data.name(id), calm.walk, calm.run);
    Some(ActorSpeeds { calm, alert: calm, turn_rate: None, spot_turn_start: None })
}

/// Creatures with an alert stance take it outside the Normal detection state.
fn stance(script: Option<Res<ScriptState>>, mut actors: Query<(&ActorRig, &mut Locomotion), With<PacedCreature>>, mut stats: ResMut<PaceStats>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    for (rig, mut lo) in &mut actors {
        let id = FormId(rig.reference);
        let mv = lo.blend.cur.mv;
        if mv.is_moving() {
            stats.moving_frames += 1;
            let fast = matches!(mv, Move::FastForward | Move::FastBackward | Move::FastLeft | Move::FastRight);
            if stats.last.insert(rig.reference, fast).is_some_and(|was| was != fast) {
                stats.swaps += 1;
                if stats.swaps <= SWAPS_LOGGED {
                    let v = lo.velocity[0].hypot(lo.velocity[1]);
                    info!(
                        "creature pace: {} {id} changes to its {} clip at {v:.0} units/s in its own scale ({:.0} at its clips' pace){}",
                        game.data.name(FormId(rig.base)),
                        if fast { "run" } else { "walk" },
                        v / lo.pace.max(0.01),
                        if game.awareness.state(id) != State::Normal { ", alert" } else { "" }
                    );
                }
            }
        }
        if lo.alert.is_none() {
            continue;
        }
        let want = game.awareness.state(id) != State::Normal;
        if lo.set_alert(want) {
            if want {
                stats.alerted += 1;
            } else {
                stats.calmed += 1;
            }
            info!("creature pace: {} {id} {}", game.data.name(FormId(rig.base)), if want { "takes its alert stance" } else { "is calm again" });
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<PaceStats>) {
    if exit.read().next().is_some() {
        info!(
            "creature pace summary: {} creatures given their own speeds ({} people by Speed Mult), people given their armour's speeds {} times; alert stance taken {} times and left {} times; {} frames of their moves, {} changes between walk and run clips",
            stats.paced - stats.npcs, stats.npcs, stats.people, stats.alerted, stats.calmed, stats.moving_frames, stats.swaps
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::locomotion::ClipInfo;

    #[test]
    fn creatures_turn_at_their_tnam() {
        // Measured in the original (campaign o4turn): steady big turns at TNAM × 1.25,
        // the base scale playing no part.
        let near = |got: f32, seen: f32| assert!((got - seen).abs() <= seen * 0.01, "{got} against {seen}");
        near(creature_turn_rate(Some(100.0), 1.0), 125.0); // CrMoleRat
        near(creature_turn_rate(Some(150.0), 1.0), 187.5); // CrCoyote
        near(creature_turn_rate(Some(75.0), 1.0), 93.75); // CrDeathClaw
        near(creature_turn_rate(Some(720.0), 1.0), 900.2); // NVCrCazador
        near(creature_turn_rate(Some(200.0), 0.8), 250.0); // VCrCazadorTier1TypeA
        near(creature_turn_rate(Some(90.0), 1.05), 112.5); // VCrNightStalkerTier1TypeB
        // TNAM 0 (or none): the fitted rate × scale.
        assert_eq!(creature_turn_rate(Some(0.0), 0.5), CREATURE_TURN_RATE * 0.5);
        assert_eq!(creature_turn_rate(None, 1.0), CREATURE_TURN_RATE);
        assert_eq!(creature_turn_rate(Some(f32::NAN), 1.0), CREATURE_TURN_RATE);
    }

    #[test]
    fn skeletons_keep_their_measured_shares() {
        let s = |p: &str| skeleton_share(p);
        let whole = Share { walk: 1.0, run: 1.0 };
        assert_eq!(s("creatures\\nvcazadores\\skeleton.nif"), Share { walk: 0.70, run: 0.70 });
        assert_eq!(s("meshes/Creatures/NVCazadores/Skeleton.nif").run, 0.70);
        assert_eq!(s("Creatures\\Protectron\\skeleton.nif"), Share { walk: 0.88, ..whole });
        for other in ["Creatures\\MirelurkKing\\Skeleton.nif","Creatures\\Brahmin\\Skeleton.nif", "Creatures\\MoleRat\\Skeleton.nif", "creatures\\NVgiantrat\\skeleton.nif",
                      "Creatures\\Dog\\Skeleton.nif", "Creatures\\DeathClaw\\Skeleton.nif", "Creatures\\Ghoul\\Skeleton.nif",
                      "Creatures\\MisterGutsy\\Skeleton.nif", "Creatures\\SentryBot\\Skeleton.nif",
                      "NVDLC03\\Creatures\\Roboscorpion\\skeleton_RoboScopion.nif", "NVDLC01\\Creatures\\Ghosts\\skeleton.nif"] {
            assert_eq!(s(other), whole, "{other}");
        }
        // Measured in the original (campaign creature_runs): the dog (scale 0.8) charges on
        // its h2hfastforward at 363 and walks 45.9; the coyote (Speed Mult 110) charges at
        // 500; the radscorpion (0.6, 150) runs 281 and walks 67.0; the feral ghoul runs 388
        // and walks 97; the deathclaw (1.0) runs about 600 and walks 120.
        let d = Speeds { walk: 85.3, run: 344.0 };
        let near = |got: f32, seen: f32| assert!((got - seen).abs() <= seen * 0.03, "{got} against {seen}");
        let dog = clip_speeds(&set(57.36, Some(454.75)), 0.8, 100.0, s("creatures\\dog\\skeleton.nif"), d);
        near(dog.run, 363.0);
        near(dog.walk, 45.9);
        near(clip_speeds(&set(57.36, Some(454.75)), 1.0, 110.0, s("creatures\\dog\\skeleton.nif"), d).run, 500.0);
        let rad = clip_speeds(&set(74.9, Some(315.04)), 0.6, 150.0, s("creatures\\radscorpion\\skeleton.nif"), d);
        near(rad.run, 281.0);
        near(rad.walk, 67.0);
        let ghoul = clip_speeds(&set(95.72, Some(384.0)), 1.0, 100.0, s("creatures\\ghoul\\skeleton.nif"), d);
        near(ghoul.run, 388.0);
        near(ghoul.walk, 97.0);
        let claw = clip_speeds(&set(120.0, Some(599.99)), 1.0, 100.0, s("creatures\\deathclaw\\skeleton.nif"), d);
        near(claw.run, 600.0);
        near(claw.walk, 120.0);
        // Measured (campaign creature_flee): the protectron walks 22.2 and runs 84.4; ghost
        // people (scale 1.25), with no run clip, run at their walk's pace, 105.4; the
        // lakelurk runs 358 on its mtfastforward (355.95).
        let pro = clip_speeds(&set(25.25, Some(83.37)), 1.0, 100.0, s("creatures\\protectron\\skeleton.nif"), d);
        near(pro.walk, 22.2);
        near(pro.run, 84.4);
        let ghost = clip_speeds(&set(83.62, None), 1.25, 100.0, s("nvdlc01\\creatures\\ghosts\\skeleton.nif"), d);
        near(ghost.run, 105.4);
        near(clip_speeds(&set(30.16, Some(355.95)), 1.0, 100.0, s("creatures\\mirelurkking\\skeleton.nif"), d).run, 358.0);
        // Measured (campaign creature_runs_all): the alien walks 86–91 and runs 420.8 on its
        // forwardwalk (89.89) and fastforward (419.9); the nightstalker walks 100.6 and
        // runs 498.8 on its mtfoward (100.7) and mtfastfoward (500.0); the sentry bot
        // walks 96.2 and runs 383.7 on 96.0 and 384.0.
        let alien = clip_speeds(&set(89.89, Some(419.9)), 1.0, 100.0, s("dlc05\\creatures\\alien\\skeleton.nif"), d);
        near(alien.walk, 88.5);
        near(alien.run, 420.8);
        let stalker = clip_speeds(&set(100.7, Some(500.0)), 1.0, 100.0, s("creatures\\nightstalker\\skeleton.nif"), d);
        near(stalker.walk, 100.6);
        near(stalker.run, 498.8);
        let sentry = clip_speeds(&set(96.0, Some(384.0)), 1.0, 100.0, s("creatures\\sentrybot\\skeleton.nif"), d);
        near(sentry.walk, 96.2);
        near(sentry.run, 383.7);
    }

    #[test]
    fn the_sonic_bark_cyberdog_keeps_its_calm_clips_when_alert() {
        // Observed: NVDLC03CrCyberDogMilitary01Ranged charged at 333.1 on the mt run; the
        // plain dog charges on the h2h run.
        assert!(!has_alert_stance("meshes\\creatures\\dog\\skeleton_sonicbark.nif"));
        assert!(!has_alert_stance("Creatures\\Dog\\Skeleton_SonicBark.NIF"));
        for other in ["meshes\\creatures\\dog\\skeleton.nif", "creatures\\nvcazadores\\skeleton.nif", "creatures\\mirelurkking\\skeleton.nif"] {
            assert!(has_alert_stance(other), "{other}");
        }
        let d = Speeds { walk: 85.3, run: 344.0 };
        let share = skeleton_share("creatures\\dog\\skeleton_sonicbark.nif");
        assert!((clip_speeds(&set(57.36, Some(333.07)), 1.0, 100.0, share, d).run - 333.1).abs() < 333.1 * 0.03);
    }
    use nvf_assets::anim_play::{BoundClip, Clip};
    use std::sync::Arc;

    fn set(walk: f32, run: Option<f32>) -> ClipSet {
        let bound = Arc::new(BoundClip {
            clip: Arc::new(Clip {
                name: String::new(),
                start: 0.0,
                stop: 1.0,
                frequency: 1.0,
                cycle_type: 0,
                tracks: Vec::new(),
                priorities: Vec::new(),
                skipped: 0,
                accum_root: None,
                text_keys: Vec::new(),
                visibility: Vec::new(),
                emitters: Vec::new(),
                materials: Vec::new(),
                free: Vec::new(),
                free_visibility: Vec::new(),
                lights: Vec::new(),
            }),
            tracks: Vec::new(),
            root: None,
            root_track: None,
            carried: None,
        });
        let clip = |speed: f32| crate::locomotion::SetClip { bound: bound.clone(), info: ClipInfo { duration: 1.0, speed, blend_time: 0.1, foot_phase: 0.0 }, path: String::new() };
        let mut s = ClipSet::default();
        s.clips.insert(Move::Forward, clip(walk));
        if let Some(r) = run {
            s.clips.insert(Move::FastForward, clip(r));
        }
        s
    }

    #[test]
    fn cazadors_move_at_the_observed_speeds() {
        let d = Speeds { walk: 86.8, run: 344.0 };
        // The cazador's mtforward.kf and mtfastforward.kf (216.73 and 545.54 units/s).
        let caz = set(216.73, Some(545.54));
        let share = skeleton_share("creatures\\nvcazadores\\skeleton.nif");
        let young = clip_speeds(&caz, 0.5, 300.0, share, d);
        let medium = clip_speeds(&caz, 1.0, 250.0, share, d);
        // Observed: walks 228 and 380, charges up to 576 and 974.
        assert!((young.walk - 228.0).abs() < 5.0, "{young:?}");
        assert!((medium.walk - 380.0).abs() < 5.0, "{medium:?}");
        assert!((young.run - 576.0).abs() < 10.0, "{young:?}");
        assert!((medium.run - 974.0).abs() < 30.0, "{medium:?}");
        // The medium cazador's walk plays its walk clip, at 1.75 its rate; without the
        // pace it would count as running.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        lo.velocity = [0.0, medium.walk];
        lo.step(0.016);
        assert_eq!(lo.blend.cur.mv, Move::FastForward);
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        lo.pace = 2.5 * share.walk;
        lo.velocity = [0.0, medium.walk];
        lo.step(0.016);
        assert_eq!(lo.blend.cur.mv, Move::Forward);
        assert!((lo.blend.cur.rate - 1.75).abs() < 0.01, "{}", lo.blend.cur.rate);
        // Near the switch point (343.9) the gait holds: walk until 395, run down to 299.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        let mut seen = Vec::new();
        for v in [216.0, 360.0, 420.0, 330.0, 250.0] {
            lo.velocity = [0.0, v];
            // Past any cross-fade, so the chosen move is taken.
            for _ in 0..60 {
                lo.step(0.016);
            }
            seen.push(lo.blend.cur.mv);
        }
        assert_eq!(seen, [Move::Forward, Move::Forward, Move::FastForward, Move::FastForward, Move::Forward]);
        // A one-frame spike to three times the walk does not change the gait.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        let mut gaits = Vec::new();
        for v in [216.0; 30].into_iter().chain([650.0]).chain([216.0; 30]) {
            lo.velocity = [0.0, v];
            lo.step(1.0 / 60.0);
            gaits.push(lo.blend.cur.mv);
        }
        assert!(gaits.iter().all(|m| *m == Move::Forward), "{gaits:?}");
        // No run clip: the walk's pace.
        assert!((clip_speeds(&set(100.0, None), 1.0, 100.0, share, d).run - 70.0).abs() < 0.01);
    }
}
