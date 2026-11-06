//! `--weapon-review <prefix>`: an NPC (the male skeleton with the body, hands and head)
//! holding a weapon of one animation type (`2hr`, `1hp`, `2ha`, `2hh`, `2hl`, `1hm`, `2hm`,
//! `h2h`, `1gt`, …), stepping through every combination the game's NPCs play with that
//! weapon drawn: standing and moving in the four directions at walking and running pace,
//! each attack standing and while walking each way, all of them from the hip and aimed
//! down the sights where the stance has sights clips (playtest list 5 item 4). The clip
//! names are on screen and in the log, so broken ones can be listed by name.
//!
//! The clips are chosen with the same rules the game's actors use
//! (`fnv_actor::player_clips`: [`move_clips`], [`aim_clips`], [`attack_clips`],
//! [`idle_clips`]; the first candidate that exists under `meshes\characters\_male\`) and
//! blended the same way (`nvf_assets::anim_play::BlendPlayer`, by track priority): the
//! posture's idle or the movement clip at the bottom, the aim over it, the attack on top
//! [design: as `nvforever`'s `locomotion.rs` and `attack_anims.rs` lay them]. Every layer
//! loops at its own length, so an attack repeats; the root plays in place. Aim up and down
//! forms are left out (the level aim only).
//!
//! Keys: A / Shift+A next / previous case, Space pause, F12 screenshot, Esc quit. Mouse
//! as in the browser.

use bevy::prelude::*;
use fnv_actor::player_clips::{aim_clips, attack_clips, idle_clips, move_clips, Aim, Dir, Stance};
use nvf_assets::actor::{bind_clip, blend_player, spawn_actor, ActorCtx, ActorParts, PartUse, PreparedSkeleton};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Layer};
use nvf_assets::model::ModelCtx;
use nvf_assets::NifRoot;
use std::collections::BTreeSet;
use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

/// The human skeleton and the folder its clips are in [observed: `Fallout - Meshes.bsa`].
pub const SKELETON: &str = r"meshes\characters\_male\skeleton.nif";
pub const FOLDER: &str = r"meshes\characters\_male\";
/// The body drawn on it: no armour, so every bone's motion shows.
const BODY: [&str; 4] =
    [r"meshes\characters\_male\upperbody.nif", r"meshes\characters\_male\lefthand.nif", r"meshes\characters\_male\righthand.nif", r"meshes\characters\head\headhuman.nif"];
/// The skeleton node a weapon model hangs from (under `Bip01 R Hand` in `skeleton.nif`).
const WEAPON_NODE: &str = "Weapon";

/// A weapon model of each animation type, when `--weapon-mesh` names none [design: one
/// common weapon of the type; the paths are in `Fallout - Meshes.bsa`].
pub fn default_weapon(prefix: &str) -> Option<&'static str> {
    Some(match prefix {
        "2hr" => r"meshes\weapons\2handrifle\varmintrifle.nif",
        "1hp" => r"meshes\weapons\1handpistol\10mmpistol.nif",
        "2ha" => r"meshes\weapons\2handautomatic\assaultrifle.nif",
        "2hh" => r"meshes\weapons\2handhandle\minigun.nif",
        "2hl" => r"meshes\weapons\2handlauncher\missilelauncher.nif",
        "1hm" => r"meshes\weapons\1handmelee\machete.nif",
        "2hm" => r"meshes\weapons\2handmelee\sledgehammer.nif",
        "1gt" => r"meshes\weapons\1handgrenadethrow\grenadefrag.nif",
        _ => return None,
    })
}

/// One combination to look at: its label and its layers, bottom first.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseSpec {
    pub label: String,
    /// Per layer, the candidate clip paths relative to [`FOLDER`], in order of preference.
    pub layers: Vec<(&'static str, Vec<String>)>,
}

/// The attack stems of weapon `prefix` among the clip file names of the folder (`names`,
/// file names only): `2hrattackleft.kf`, `2hrattack3isup.kf` … give `attackleft`,
/// `attack3`; melee variants keep their `_a`/`_b` (`1hmattackright_a`) [observed: the
/// `_male` file list, animation.md §9.6: attacks have `is`, `up` and `down` forms]. Sneak
/// (`sneak…`) and power armour (`pa…`) files are not this prefix's.
pub fn attack_stems<'a>(prefix: &str, names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out = BTreeSet::new();
    for n in names {
        let Some(rest) = n.strip_prefix(prefix).and_then(|r| r.strip_suffix(".kf")) else { continue };
        if !rest.starts_with("attack") {
            continue;
        }
        let rest = rest.strip_suffix("up").or_else(|| rest.strip_suffix("down")).unwrap_or(rest);
        let rest = rest.strip_suffix("is").unwrap_or(rest);
        out.insert(rest.to_string());
    }
    out.into_iter().collect()
}

/// Every case for weapon `prefix` in `stance`, with `sights` cases where asked.
pub fn cases(prefix: &str, stance: Stance, stems: &[String]) -> Vec<CaseSpec> {
    let dirs = [Dir::Forward, Dir::Backward, Dir::Left, Dir::Right];
    let mut out = Vec::new();
    let aims = [("hip", false), ("aimed", true)];
    for (aim_name, sights) in aims {
        let aim = ("aim", aim_clips(stance, prefix, Aim::Level, sights));
        out.push(CaseSpec { label: format!("standing, {aim_name}"), layers: vec![("idle", idle_clips(stance)), aim.clone()] });
        for fast in [false, true] {
            for d in dirs {
                let pace = if fast { "running" } else { "walking" };
                out.push(CaseSpec {
                    label: format!("{pace} {}, {aim_name}", d.name()),
                    layers: vec![("move", move_clips(stance, false, fast, d, Some(prefix))), aim.clone()],
                });
            }
        }
    }
    for stem in stems {
        for (aim_name, sights) in aims {
            let aim = ("aim", aim_clips(stance, prefix, Aim::Level, sights));
            let attack = ("attack", attack_clips(stance, prefix, stem, sights));
            out.push(CaseSpec { label: format!("{stem} standing, {aim_name}"), layers: vec![("idle", idle_clips(stance)), aim.clone(), attack.clone()] });
            for d in dirs {
                out.push(CaseSpec {
                    label: format!("{stem} walking {}, {aim_name}", d.name()),
                    layers: vec![("move", move_clips(stance, false, false, d, Some(prefix))), aim.clone(), attack.clone()],
                });
            }
        }
    }
    out
}

/// A case with its clips found.
struct Case {
    label: String,
    /// (layer role, clip path, bound clip) of each layer found.
    layers: Vec<(&'static str, String, Arc<BoundClip>)>,
    /// Layers none of whose candidates exists: (role, first candidate).
    missing: Vec<(&'static str, String)>,
}

/// What `--weapon-review` shows.
#[derive(Resource)]
pub struct Review {
    pub prefix: String,
    pub stance: Stance,
    pub weapon: Option<String>,
    pub start_case: usize,
    built: bool,
    cases: Vec<Case>,
    index: usize,
    time: f32,
    pub paused: bool,
    player: Option<BlendPlayer>,
    scratch: Vec<(usize, Transform)>,
    error: Option<String>,
    skipped_same: usize,
}

impl Review {
    pub fn new(prefix: &str, sneak: bool, weapon: Option<String>, start_case: usize) -> Review {
        Review {
            prefix: prefix.to_ascii_lowercase(),
            stance: if sneak { Stance::Sneak } else { Stance::Stand },
            weapon,
            start_case,
            built: false,
            cases: Vec::new(),
            index: 0,
            time: 0.0,
            paused: false,
            player: None,
            scratch: Vec::new(),
            error: None,
            skipped_same: 0,
        }
    }

    fn log_case(&self) {
        let Some(c) = self.cases.get(self.index) else { return };
        let clips: Vec<String> = c.layers.iter().map(|(r, p, _)| format!("{r} {p}")).collect();
        let missing: Vec<String> = c.missing.iter().map(|(r, p)| format!("{r} {p}")).collect();
        info!(
            "weapon review: case {}/{}: {}: {}{}",
            self.index + 1,
            self.cases.len(),
            c.label,
            clips.join(", "),
            if missing.is_empty() { String::new() } else { format!("; missing: {}", missing.join(", ")) }
        );
    }
}

/// Build the actor and the cases once.
pub fn build(mut review: ResMut<Review>, mut parts: ResMut<ActorParts>, mut models: ModelCtx, mut commands: Commands, mut cams: Query<&mut crate::camera::Orbit>) {
    if review.built {
        return;
    }
    review.built = true;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let Some(skel) = ctx.skeleton(SKELETON) else {
        review.error = Some(format!("{SKELETON} could not be read"));
        return;
    };
    let body: Vec<PartUse> = BODY
        .iter()
        .filter_map(|p| ctx.part(p))
        .map(|part| PartUse { part, materials: Vec::new(), layers: Vec::new(), meshes: Vec::new(), hidden_shapes: Vec::new(), by_ancestor: false })
        .collect();
    let root = commands.spawn((NifRoot, Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)), Visibility::default())).id();
    let actor = spawn_actor(&mut commands, root, Transform::IDENTITY, &skel, &body);
    let weapon = review.weapon.clone().or_else(|| default_weapon(&review.prefix).map(str::to_string));
    match (weapon.as_deref(), skel.by_name.get(WEAPON_NODE)) {
        (Some(w), Some(&node)) => match ctx.part(w) {
            Some(part) => {
                for s in part.shapes.iter().filter(|s| !s.cap) {
                    let tr = Transform::from_matrix(Mat4::from(part.root * s.transform));
                    let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), tr)).id();
                    commands.entity(actor.nodes[node]).add_child(e);
                }
                info!("weapon review: {w} in the hand");
            }
            None => warn!("weapon review: weapon model {w} could not be read"),
        },
        (None, _) => info!("weapon review: no weapon model for {} (give one with --weapon-mesh)", review.prefix),
        (_, None) => warn!("weapon review: the skeleton has no {WEAPON_NODE} node"),
    }
    let names: Vec<String> = ctx.models.data.vfs.paths(|p| p.starts_with(FOLDER) && p.ends_with(".kf"));
    let file_names: Vec<&str> = names.iter().filter_map(|p| p.strip_prefix(FOLDER)).filter(|n| !n.contains('\\')).collect();
    let prefix = review.prefix.clone();
    let stems = attack_stems(&prefix, file_names.iter().copied());
    let specs = cases(&prefix, review.stance, &stems);
    let mut built: Vec<Case> = Vec::new();
    for spec in specs {
        let mut case = Case { label: spec.label.clone(), layers: Vec::new(), missing: Vec::new() };
        for (role, candidates) in &spec.layers {
            match find_clip(&mut ctx, &skel, candidates) {
                Some((path, clip)) => case.layers.push((role, path, clip)),
                None => case.missing.push((role, candidates.first().cloned().unwrap_or_default())),
            }
        }
        // An aimed case with no sights clips is the hip case again: left out.
        let same = |c: &Case| c.layers.iter().map(|l| &l.1).eq(case.layers.iter().map(|l| &l.1));
        if built.iter().any(same) {
            review.skipped_same += 1;
            continue;
        }
        built.push(case);
    }
    info!(
        "weapon review: {} ({:?}): {} cases ({} attacks: {}; {} aimed cases left out, the same clips as from the hip)",
        prefix,
        review.stance,
        built.len(),
        stems.len(),
        stems.join(" "),
        review.skipped_same
    );
    for (i, c) in built.iter().enumerate() {
        let clips: Vec<&str> = c.layers.iter().map(|l| l.1.as_str()).collect();
        let missing: Vec<String> = c.missing.iter().map(|(r, p)| format!("{r} {p}")).collect();
        info!(
            "weapon review: [{}] {}: {}{}",
            i + 1,
            c.label,
            clips.join(" + "),
            if missing.is_empty() { String::new() } else { format!(" (missing: {})", missing.join(", ")) }
        );
    }
    review.cases = built;
    review.index = review.start_case.saturating_sub(1).min(review.cases.len().saturating_sub(1));
    review.player = Some(blend_player(&actor, &skel));
    review.log_case();
    for mut o in &mut cams {
        o.target = Vec3::new(0.0, 70.0, 0.0);
        o.distance = 260.0;
    }
}

fn find_clip(ctx: &mut ActorCtx, skel: &PreparedSkeleton, candidates: &[String]) -> Option<(String, Arc<BoundClip>)> {
    candidates.iter().find_map(|c| {
        let path = format!("{FOLDER}{c}");
        if !ctx.models.data.vfs.exists(&path) {
            return None;
        }
        ctx.clip(&path).map(|clip| (c.clone(), Arc::new(bind_clip(skel, clip))))
    })
}

pub fn keys(input: Res<ButtonInput<KeyCode>>, mut review: ResMut<Review>) {
    let n = review.cases.len();
    if n == 0 {
        return;
    }
    if input.just_pressed(KeyCode::KeyA) {
        let back = input.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        review.index = if back { (review.index + n - 1) % n } else { (review.index + 1) % n };
        review.time = 0.0;
        review.log_case();
    }
    if input.just_pressed(KeyCode::Space) {
        review.paused = !review.paused;
    }
}

/// Pose the actor: every layer loops at its own length.
pub fn play(time: Res<Time>, mut review: ResMut<Review>, mut transforms: Query<&mut Transform>) {
    if !review.paused {
        review.time += time.delta_secs();
    }
    let r = &mut *review;
    let (Some(player), Some(case)) = (r.player.as_mut(), r.cases.get(r.index)) else { return };
    player.layers = Default::default();
    for (slot, (_, _, clip)) in player.layers.iter_mut().zip(&case.layers) {
        let len = clip.clip.length();
        let phase = if len > 0.0 { (r.time % len) / len } else { 0.0 };
        *slot = Some(Layer { clip: clip.clone(), phase, weight: 1.0 });
    }
    player.pose(&mut r.scratch, &mut transforms);
}

pub fn overlay(review: Res<Review>, mut text: Query<&mut Text, With<crate::Overlay>>) {
    let Ok(mut t) = text.single_mut() else { return };
    let mut s = format!("weapon review: {} ({:?})\n", review.prefix, review.stance);
    if let Some(e) = &review.error {
        s += &format!("error: {e}\n");
    }
    if let Some(c) = review.cases.get(review.index) {
        s += &format!("case {}/{}: {}\n", review.index + 1, review.cases.len(), c.label);
        for (role, path, clip) in &c.layers {
            let len = clip.clip.length();
            let at = if len > 0.0 { review.time % len } else { 0.0 };
            s += &format!("  {role}: {path}  t={at:.2} / {len:.2}\n");
        }
        for (role, path) in &c.missing {
            s += &format!("  {role}: missing ({path})\n");
        }
    }
    if review.paused {
        s += "paused\n";
    }
    s += "A / Shift+A next/prev case  Space pause  F12 screenshot  Esc quit";
    t.0 = s;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_stems_drop_the_sights_and_pitch_forms() {
        let names = ["2hrattackleft.kf", "2hrattackleftis.kf", "2hrattackleftisup.kf", "2hrattack3down.kf", "2hraim.kf", "sneak2hrattack4.kf", "1hmattackright_a.kf"];
        assert_eq!(attack_stems("2hr", names), ["attack3", "attackleft"]);
        assert_eq!(attack_stems("1hm", names), ["attackright_a"]);
    }

    #[test]
    fn cases_cover_moves_and_moving_attacks_from_the_hip_and_aimed() {
        let c = cases("2hr", Stance::Stand, &["attackleft".to_string()]);
        // Standing and 8 moves, twice; one attack standing and walking 4 ways, twice.
        assert_eq!(c.len(), 2 * 9 + 2 * 5);
        assert_eq!(c[1].label, "walking forward, hip");
        assert_eq!(c[1].layers[0].1[0], r"locomotion\2hrforward.kf");
        let aimed = c.iter().find(|c| c.label == "attackleft walking left, aimed").unwrap();
        assert_eq!(aimed.layers[1].1[0], "2hraimis.kf");
        assert_eq!(aimed.layers[2].1[0], "2hrattackleftis.kf");
        assert_eq!(aimed.layers[0].1[0], r"locomotion\2hrleft.kf");
    }
}
