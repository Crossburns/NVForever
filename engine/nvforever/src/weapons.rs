//! Weapons in NPCs' hands (workstream 5 part H, docs/spec-notes/actors.md §9): the weapon
//! an actor carries is drawn on its skeleton's `Weapon` node; the stance's holster, equip,
//! aim and unequip clips play over its locomotion by track priority, and the `Weapon` node
//! moves between holster bone and hand at the clips' `prn:` keys. Nothing decides when to
//! draw yet (Layer 6); `--actor-weapons drawn` and the test aid `--actor-weapon-test` do.

use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_actor::weapon::{clip_path, mount_bones, sneak_clip_path, EquipClip, StanceFacts, WeaponAnim};
use nvf_assets::actor::{bind_clip, ClipSource, PreparedPart, PreparedSkeleton, SpawnedActor};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Layer};
use nvf_play::combat::WeaponBroken;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// How NPCs' weapons are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum WeaponMode {
    /// Not drawn.
    None,
    /// Holstered (at the pelvis or on the back, actors.md §9.2).
    #[default]
    Holstered,
    /// In the hand, at the ready (the stance's aim clip).
    Drawn,
}

/// The skeleton node the weapon model hangs from [observed: `skeleton.nif`, under
/// `Bip01 R Hand`].
pub const WEAPON_NODE: &str = "Weapon";

/// The clips of one weapon stance bound to one skeleton, shared by the actors using them.
pub struct Stance {
    /// The stance prefix (`2hr`, `1hp`, …).
    pub prefix: String,
    pub clips: BTreeMap<EquipClip, Arc<BoundClip>>,
    /// The same clips with only their `Weapon` track: where the weapon hangs while the
    /// body plays something else entirely (a seat's clip, [`WeaponHold::weapon_layers`]).
    pub weapon_only: BTreeMap<EquipClip, Arc<BoundClip>>,
    pub facts: StanceFacts,
    pub holster: String,
    pub drawn: String,
    /// Skeleton node indices of `Weapon` and of every bone the weapon can hang from.
    pub weapon_node: usize,
    pub bones: HashMap<String, usize>,
    /// Where the equip clip leaves the `Weapon` node: its rest pose while in the hand
    /// ([`WeaponHold::rest`]).
    pub weapon_rest: Option<Transform>,
    /// The same for the crouched equip clip (`sneak2hhequip.kf`), where the stance has
    /// one: the crouched clips hold the weapon for that place ([`WeaponHold::rest`]).
    pub sneak_weapon_rest: Option<Transform>,
    /// Where the left hand holds the weapon: its place in the `Weapon` node's frame at the
    /// end of the equip clip, standing and crouched ([`left_grip`]). The attack clips hold it
    /// there too (`2hh`: within 4 units standing, 0.1 crouched [observed:
    /// `nvf-assets/examples/hand_probe.rs`]); `attack_anims::grip_check` logs how far the
    /// shown left hand is from it.
    pub grip: Option<Vec3>,
    pub sneak_grip: Option<Vec3>,
    /// The weapon-model nodes the holster clip switches off (`##FlamerGun`,
    /// `##MinigunGun`): the clips show and hide them ([`gun_shown`]).
    pub holstered_off: Vec<String>,
    /// The stance's walks, runs and turns, played while the weapon is drawn
    /// (`locomotion::Locomotion::set_armed`).
    pub moves: Arc<crate::locomotion::WeaponMoves>,
    /// The skeleton's folder (archive path), for the clips' file names in the log.
    pub folder: String,
    /// The crouched forms of the clips (`sneak1hpequip.kf`, `sneak1hpaim.kf`,
    /// `sneak1hpunequip.kf`, [`sneak_clip_path`]) where the folder has them, played while
    /// the actor sneaks ([`WeaponHold::set_crouched`]), and their `Weapon` tracks alone.
    pub sneak: BTreeMap<EquipClip, Arc<BoundClip>>,
    pub sneak_weapon_only: BTreeMap<EquipClip, Arc<BoundClip>>,
    /// The facts of the crouched draw cycle: each clip's crouched form where there is one,
    /// else the standing clip's.
    pub sneak_facts: StanceFacts,
}

impl Stance {
    /// The stance `prefix` for the skeleton at `skeleton` (archive path), or why not; also
    /// a report line.
    pub fn load(ctx: &mut impl ClipSource, skeleton: &str, skel: &PreparedSkeleton, prefix: &str) -> Result<(Stance, String), String> {
        let weapon_node = *skel.by_name.get(WEAPON_NODE).ok_or("the skeleton has no Weapon node")?;
        let s = fnv_vfs::normalize(skeleton);
        let folder = s.rfind('\\').map_or("", |i| &s[..i]).to_string();
        let mut clips = BTreeMap::new();
        let mut facts = StanceFacts::new();
        for c in EquipClip::ALL {
            let path = clip_path(&folder, prefix, c);
            // A creature's stance may be in its `specialanims` folder (Mister Handy and
            // Gutsy, protectrons: `fnv_actor::anims::special_anims_path`).
            if let Some(clip) = ctx.clip(&path).or_else(|| fnv_actor::anims::special_anims_path(&path).and_then(|p| ctx.clip(&p))) {
                facts.insert(c, fnv_actor::weapon::ClipFacts::from_parts(clip.start, clip.stop, &clip.text_keys));
                clips.insert(c, Arc::new(bind_clip(skel, clip)));
            }
        }
        if !clips.contains_key(&EquipClip::Holster) && !clips.contains_key(&EquipClip::Aim) {
            return Err(format!("no {prefix} holster or aim clip in {folder}"));
        }
        let mut sneak = BTreeMap::new();
        let mut sneak_facts = facts.clone();
        for c in EquipClip::ALL {
            let Some(path) = sneak_clip_path(&folder, prefix, c) else { continue };
            if let Some(clip) = ctx.clip(&path) {
                sneak_facts.insert(c, fnv_actor::weapon::ClipFacts::from_parts(clip.start, clip.stop, &clip.text_keys));
                sneak.insert(c, Arc::new(bind_clip(skel, clip)));
            }
        }
        let parent = skel.parents[weapon_node].and_then(|p| skel.names[p].clone()).unwrap_or_default();
        let (holster, drawn) = mount_bones(&facts, &parent);
        let mut bones = HashMap::new();
        let names = facts.values().flat_map(|f| f.prn.iter().map(|(_, b)| b.clone())).chain([holster.clone(), drawn.clone()]);
        for n in names {
            if let Some(&i) = skel.by_name.get(&n) {
                bones.insert(n, i);
            }
        }
        let (moves, moves_line) = crate::locomotion::WeaponMoves::load(ctx, skeleton, skel, prefix);
        let line = format!(
            "{folder} {prefix}: clips {:?}, crouched {:?}, holstered at {holster}, drawn at {drawn}, bones found {}; {moves_line}",
            clips.keys().collect::<Vec<_>>(),
            sneak.keys().collect::<Vec<_>>(),
            bones.len()
        );
        let weapon_only = clips.iter().map(|(c, b)| (*c, Arc::new(weapon_track_only(b, weapon_node)))).collect();
        let sneak_weapon_only = sneak.iter().map(|(c, b)| (*c, Arc::new(weapon_track_only(b, weapon_node)))).collect();
        let weapon_rest = clips.get(&EquipClip::Equip).and_then(|c| track_end(c, weapon_node, skel.locals[weapon_node]));
        let sneak_weapon_rest = sneak.get(&EquipClip::Equip).and_then(|c| track_end(c, weapon_node, skel.locals[weapon_node])).or(weapon_rest);
        let line = match (weapon_rest, sneak.contains_key(&EquipClip::Equip).then_some(sneak_weapon_rest).flatten()) {
            (Some(a), Some(b)) => format!("{line}; weapon rest crouched {:.1} units and {:.0} degrees from standing", a.translation.distance(b.translation), a.rotation.angle_between(b.rotation).to_degrees()),
            _ => line,
        };
        let grip = clips.get(&EquipClip::Equip).and_then(|c| left_grip(skel, c, weapon_node));
        let sneak_grip = sneak.get(&EquipClip::Equip).and_then(|c| left_grip(skel, c, weapon_node)).or(grip);
        let holstered_off = clips.get(&EquipClip::Holster).map_or_else(Vec::new, |c| holstered_off(&c.clip));
        Ok((Stance { prefix: prefix.to_string(), clips, weapon_only, facts, holster, drawn, weapon_node, bones, weapon_rest, sneak_weapon_rest, grip, sneak_grip, holstered_off, moves: Arc::new(moves), folder, sneak, sneak_weapon_only, sneak_facts }, line))
    }

    /// The left hand's grip standing or crouched ([`Stance::grip`]).
    pub fn grip_for(&self, crouched: bool) -> Option<Vec3> {
        if crouched {
            self.sneak_grip
        } else {
            self.grip
        }
    }

    /// The facts of the draw cycle standing or crouched.
    pub fn facts_for(&self, crouched: bool) -> &StanceFacts {
        if crouched {
            &self.sneak_facts
        } else {
            &self.facts
        }
    }

    /// Clip `c` standing or crouched (its crouched form where there is one, else the
    /// standing clip), whole or only its `Weapon` track.
    pub fn clip_for(&self, c: EquipClip, crouched: bool, weapon_only: bool) -> Option<&Arc<BoundClip>> {
        let (sneak, stand) = if weapon_only { (&self.sneak_weapon_only, &self.weapon_only) } else { (&self.sneak, &self.clips) };
        crouched.then(|| sneak.get(&c)).flatten().or_else(|| stand.get(&c))
    }

    /// The file clip `c` plays from, standing or crouched.
    pub fn path_for(&self, c: EquipClip, crouched: bool) -> String {
        match sneak_clip_path(&self.folder, &self.prefix, c).filter(|_| crouched && self.sneak.contains_key(&c)) {
            Some(p) => p,
            None => clip_path(&self.folder, &self.prefix, c),
        }
    }
}

/// The nodes a holster clip's visibility tracks switch off at some time (the `2hh` gun
/// parts, animation.md §9.7).
fn holstered_off(clip: &nvf_assets::anim_play::Clip) -> Vec<String> {
    clip.visibility.iter().filter(|(_, _, b)| b.keys.iter().any(|(_, v)| !v) || b.value == Some(false)).map(|(n, ..)| n.clone()).collect()
}

/// `clip` with only the track of the `Weapon` node (index `weapon_node`).
pub fn weapon_track_only(clip: &BoundClip, weapon_node: usize) -> BoundClip {
    BoundClip { clip: clip.clip.clone(), tracks: clip.tracks.iter().filter(|(i, ..)| *i == weapon_node).cloned().collect(), root: None, root_track: None, carried: None }
}

/// Where `clip` leaves node `node` at its end (`bind` fills what the track leaves out);
/// `None` when the clip does not drive it.
fn track_end(clip: &BoundClip, node: usize, bind: Transform) -> Option<Transform> {
    let (_, t, _) = clip.tracks.iter().find(|(i, ..)| *i == node)?;
    let time = clip.clip.stop;
    Some(Transform {
        translation: t.translation.sample(time).map_or(bind.translation, Vec3::from_array),
        rotation: t.rotation_at(time).map_or(bind.rotation, |[w, x, y, z]| Quat::from_xyzw(x, y, z, w).normalize()),
        scale: Vec3::splat(t.scale.sample(time).map_or(bind.scale.x, |[s]| s)),
    })
}

/// Where `clip` leaves the left hand in the frame of node `weapon` at its end, each node
/// at its track's last key or its bind transform ([`Stance::grip`]).
fn left_grip(skel: &PreparedSkeleton, clip: &BoundClip, weapon: usize) -> Option<Vec3> {
    let hand = *skel.by_name.get(LEFT_HAND)?;
    let world = |mut i: usize| {
        let mut m = Affine3A::IDENTITY;
        loop {
            let t = track_end(clip, i, skel.locals[i]).unwrap_or(skel.locals[i]);
            m = Affine3A::from_scale_rotation_translation(t.scale, t.rotation, t.translation) * m;
            match skel.parents[i] {
                Some(p) => i = p,
                None => return m,
            }
        }
    };
    Some(world(weapon).inverse().transform_point3(world(hand).translation.into()))
}

/// The left hand bone, which holds a two-handed weapon's fore grip.
pub const LEFT_HAND: &str = "Bip01 L Hand";

/// An armed actor's weapon state (on the actor's root, beside its `Locomotion`).
#[derive(Component)]
pub struct WeaponHold {
    pub stance: Arc<Stance>,
    pub anim: WeaponAnim,
    /// Whether the weapon should be drawn.
    pub drawn: bool,
    /// Keep the pose (`--actor-pose-time`).
    pub hold: bool,
    /// The node the `Weapon` node hangs from now.
    pub parent: usize,
    /// Test aid: draw and holster every `period` seconds; the time into the cycle.
    pub test: Option<(f32, f32)>,
    /// The model's shapes under a node the holster clip switches off (the flamer's and
    /// minigun's gun): the shape, that node's name and whether it was last shown.
    pub gun: Vec<(Entity, String, Option<bool>)>,
    /// The model's shapes on a bone of their own (the flamer's tank and the minigun's
    /// backpack on `Bip01 Spine2`): on the body whether the weapon is drawn or not.
    pub back: Vec<Entity>,
    /// The model's shapes under a part the aim or draw clip moves (the pistols' hammers):
    /// the shape, its place holstered (the model's) and in the hand ([`drawn_place`]),
    /// and which of the two it has now. NPCs play no clips on the weapon's parts: they
    /// hold this pose, shots included.
    pub parts: Vec<(Entity, Transform, Transform)>,
    parts_in_hand: Option<bool>,
    /// Crouched (the actor sneaks, [`WeaponHold::set_crouched`]), and whether the clip
    /// fading out (`anim.prev`) is a crouched one.
    pub crouched: bool,
    prev_crouched: bool,
    /// The crouched clip files logged for this actor ([`WeaponHold::crouched_clip_news`]).
    logged: Vec<String>,
    /// A creature's body weapon (`body_weapons.rs`): no model of its own, nothing to hide
    /// or drop.
    pub body: bool,
}

impl WeaponHold {
    pub fn new(stance: Arc<Stance>, drawn: bool, skel_parent: usize) -> WeaponHold {
        WeaponHold { stance, anim: WeaponAnim::new(drawn), drawn, hold: false, parent: skel_parent, test: None, gun: Vec::new(), back: Vec::new(), parts: Vec::new(), parts_in_hand: None, crouched: false, prev_crouched: false, logged: Vec::new(), body: false }
    }

    /// Crouch with the weapon or stand up with it (the actor's locomotion sneaking,
    /// docs/spec-notes/actors.md §9.4): the draw cycle goes on, at the same stage and time,
    /// in the stance's crouched clips where it has them (`sneak1hpaim.kf` …), else in the
    /// standing ones; the old form fades out over the new clip's blend time, as the
    /// locomotion's sets do. The standing aim clips hold the legs upright at priority 25
    /// over the crouched idle's 20 (`1hpaim.kf` against `sneakmtidle.kf`), so a crouched
    /// actor standing still with its weapon drawn stood up from the hips down; the
    /// crouched aim's legs are the crouched idle's pose [observed: kf_layer_survey and
    /// kf_pose_compare, 2026-10-20]. Returns whether it changed.
    pub fn set_crouched(&mut self, on: bool) -> bool {
        if self.crouched == on {
            return false;
        }
        let c = self.anim.stage.clip();
        self.prev_crouched = self.crouched;
        self.crouched = on;
        if self.stance.sneak.contains_key(&c) {
            let fade = self.stance.facts_for(on).get(&c).map_or(fnv_actor::locomotion::DEFAULT_BLEND, |f| f.blend_time);
            self.anim.fade_from_current(fade);
        }
        true
    }

    /// While crouched, the clip and file the draw cycle plays now, the first time that
    /// crouched file plays for this actor (for the log).
    pub fn crouched_clip_news(&mut self) -> Option<(EquipClip, String)> {
        let c = self.anim.stage.clip();
        if !self.crouched || !self.stance.sneak.contains_key(&c) {
            return None;
        }
        let path = self.stance.path_for(c, true);
        if self.logged.contains(&path) {
            return None;
        }
        self.logged.push(path.clone());
        Some((c, path))
    }

    /// The draw cycle's clips to sample: (clip, crouched form, phase, weight), the
    /// current one first; each phase by its own form's length.
    fn specs(&self) -> [Option<(EquipClip, bool, f32, f32)>; 2] {
        let st = &self.stance;
        let w = self.anim.weight();
        let c = self.anim.stage.clip();
        [
            Some((c, self.crouched, WeaponAnim::phase(c, self.anim.time, st.facts_for(self.crouched)), w)),
            self.anim.prev.map(|(p, t)| (p, self.prev_crouched, WeaponAnim::phase(p, t, st.facts_for(self.prev_crouched)), 1.0 - w)),
        ]
    }

    /// The draw cycle's clips (whole, or only their `Weapon` tracks) as player layers 2
    /// and 3.
    fn set_layers(&self, player: &mut BlendPlayer, weapon_only: bool) {
        let st = &self.stance;
        let [a, b] = self.specs();
        let layer = |l: Option<(EquipClip, bool, f32, f32)>| l.and_then(|(c, crouched, phase, weight)| Some(Layer { clip: st.clip_for(c, crouched, weapon_only)?.clone(), phase, weight }));
        player.layers[2] = layer(a);
        player.layers[3] = layer(b);
        self.rest(player);
    }

    /// Advance the test aid and the draw cycle by `dt`.
    pub fn step(&mut self, dt: f32) {
        if let Some((period, t)) = self.test.as_mut() {
            *t += dt;
            // Holstered for the first period, drawn for the second, and so on.
            self.drawn = (*t / period.max(0.1)).floor() as i64 % 2 == 1;
        }
        let stage = self.anim.stage;
        self.anim.step(self.drawn, dt, self.stance.facts_for(self.crouched));
        // A new stage: the clip fading out is of the posture the actor has now.
        if self.anim.stage != stage {
            self.prev_crouched = self.crouched;
        }
    }

    /// Run `secs` ahead in fixed 1/60 s steps (`--actor-pose-time`: a repeatable pose).
    pub fn simulate(&mut self, secs: f32) {
        const DT: f32 = 1.0 / 60.0;
        let mut left = secs;
        while left > 0.0 {
            let dt = left.min(DT);
            self.step(dt);
            left -= dt;
        }
    }

    /// The weapon clips as player layers 2 and 3 (0 and 1 are locomotion's).
    pub fn layers(&self, player: &mut BlendPlayer) {
        self.set_layers(player, false);
    }

    /// In the hand, the `Weapon` node rests where the equip clip leaves it: the aim and
    /// movement clips of some stances (`2hh`: the minigun, the flamer, the gatling laser;
    /// `1hp`) do not drive it, and the equip, attack and reload clips that do agree on
    /// the place [observed: animation.md §9.7; hypothesis: the game leaves the node where
    /// the last clip put it]. Back at its bind transform the weapon turned in the hand
    /// and the left hand missed it [observed in NVForever: owner's playtest 2026-10-16,
    /// Cook-Cook's flamer]. Crouched, the crouched equip clip's place: the crouched aim
    /// and attack clips hold the weapon there, and with the standing place a crouched
    /// Cook-Cook fired with a hand off his flamer [observed in NVForever: #1309].
    /// Holstered, the node keeps its bind transform.
    fn rest(&self, player: &mut BlendPlayer) {
        let rest = if self.crouched { self.stance.sneak_weapon_rest } else { self.stance.weapon_rest };
        player.set_rest(self.stance.weapon_node, rest.filter(|_| self.in_hand()));
    }

    /// Whether the weapon hangs from the hand now.
    pub fn in_hand(&self) -> bool {
        let st = &self.stance;
        self.anim.parent(&st.holster, &st.drawn, st.facts_for(self.crouched)) == st.drawn
    }

    /// Only where the weapon hangs, as player layers 2 and 3: for an actor whose body plays
    /// a clip of its own over everything (on furniture). The stance's clips hold the
    /// `Weapon` node's place under the bone it hangs from (the holster clip: on the hip
    /// or the back); without them the node went back to its bind transform, which is its
    /// place under the right hand, and the holstered weapon floated off the seated body
    /// at a wrong angle [observed in NVForever, owner's playtest 2026-10-11; the seat
    /// clips have no `Weapon` track: `dynamicidle_chairsit.kf`, `chair_forwardenter.kf`].
    pub fn weapon_layers(&self, player: &mut BlendPlayer) {
        self.set_layers(player, true);
    }

    /// The node the weapon should hang from now, if the skeleton has that bone.
    pub fn parent_node(&self) -> Option<usize> {
        let st = &self.stance;
        st.bones.get(self.anim.parent(&st.holster, &st.drawn, st.facts_for(self.crouched))).copied()
    }
}

/// On each drawn shape of the weapon model in an actor's hand or holster: whose it is
/// and which WEAP (the hit volume of a drawn weapon, `limbs.rs`).
#[derive(Component, Clone, Copy, Debug)]
pub struct HeldWeapon {
    pub reference: u32,
    pub weapon: fnv_sim::FormId,
}

/// The muzzle of the weapon drawn on an actor: an empty entity under the actor's `Weapon`
/// node, where the weapon model's `ProjectileNode` is (the node shots are drawn from,
/// animation.md §9.9, as the player's), so it moves with the weapon. Its node's place
/// in the model at rest [design: the clips that move a weapon's `##` parts are not
/// applied to it].
#[derive(Component, Clone, Copy, Debug)]
pub struct Muzzle {
    pub reference: u32,
}

/// The nodes shots leave from, by either of their names (animation.md §9.9).
pub const MUZZLE_NODES: [&str; 2] = ["ProjectileNode", "##ProjectileNode"];

/// Put a [`Muzzle`] under the actor's `Weapon` node where the weapon model's
/// `ProjectileNode` is; nothing when the model has none.
pub fn spawn_muzzle(commands: &mut Commands, actor: &SpawnedActor, weapon_node: usize, model: &nvf_assets::model::PreparedModel, reference: u32) -> Option<Entity> {
    let path = nvf_assets::model::node_path(&model.nodes, &MUZZLE_NODES)?;
    let at = path.iter().fold(model.root, |m, &i| m * model.nodes[i].local);
    let e = commands.spawn((Name::new("Muzzle"), Muzzle { reference }, Transform::from_matrix(Mat4::from(at)), Visibility::Inherited)).id();
    commands.entity(actor.nodes[weapon_node]).add_child(e);
    Some(e)
}

/// A weapon that broke leaves its actor's hand (docs/spec-notes/combat-rules.md §17):
/// its model is taken off the skeleton and the weapon clips stop. The weapon itself now
/// lies on the ground as a dropped reference (`nvf_play::dropped` draws it).
pub fn drop_broken(
    mut commands: Commands,
    mut broken: MessageReader<WeaponBroken>,
    held: Query<(Entity, &HeldWeapon)>,
    mut holds: Query<(Entity, &crate::limbs::ActorRig, &mut BlendPlayer, &WeaponHold)>,
) {
    for b in broken.read() {
        for (e, _) in held.iter().filter(|(_, h)| h.reference == b.form_id) {
            commands.entity(e).despawn();
        }
        // A body weapon is part of the creature (it does not wear, `nvf_play::combat`).
        for (e, _, mut player, _) in holds.iter_mut().filter(|(_, rig, _, h)| rig.reference == b.form_id && !h.body) {
            commands.entity(e).remove::<WeaponHold>();
            (player.layers[2], player.layers[3]) = (None, None);
        }
    }
}

/// The shapes of a weapon model as spawned on an actor ([`spawn_model`]).
#[derive(Default)]
pub struct SpawnedWeapon {
    /// Every shape.
    pub all: Vec<Entity>,
    /// Those under a node of `gun_nodes`, with its name ([`WeaponHold::gun`]).
    pub gun: Vec<(Entity, String)>,
    /// Those on a bone of their own ([`WeaponHold::back`]).
    pub back: Vec<Entity>,
    /// Those under a part the aim or draw clip moves: the shape, its place in the model
    /// and its place in the hand ([`WeaponHold::parts`]).
    pub posed: Vec<(Entity, Transform, Transform)>,
}

/// On a weapon shape that hangs from a bone of its own (the flamer's tank): not part of
/// the weapon in the hand (its hit volume, `limbs.rs`).
#[derive(Component)]
pub struct OnBack;

/// Spawn the weapon model's shapes under the actor's `Weapon` node (rigid, in the node's
/// own frame [hypothesis, actors.md §9.2]). A shape under a node whose `UPB` string
/// (`bones`, by node name) names a bone of the skeleton hangs from that bone instead,
/// with its place in the whole model (the flamer's tank and the minigun's backpack say
/// `Bip01 Spine2`), as the player body does [hypothesis: that is the string's meaning;
/// animation.md §9.7]. Shapes under a node named in `gun_nodes` are listed apart, and so
/// are those under a node the `held` clips move, with their place in the hand
/// ([`drawn_place`]).
#[allow(clippy::too_many_arguments)]
pub fn spawn_model(
    commands: &mut Commands,
    actor: &SpawnedActor,
    skel: &PreparedSkeleton,
    weapon_node: usize,
    part: &PreparedPart,
    bones: &HashMap<String, String>,
    gun_nodes: &[String],
    held: &[&nvf_assets::anim_play::Clip],
) -> SpawnedWeapon {
    let mut out = SpawnedWeapon::default();
    for s in part.shapes.iter().filter(|s| !s.cap) {
        let names = || s.ancestors.iter().filter_map(|(n, _)| n.as_deref()).chain(s.name.as_deref());
        let bone = names().find_map(|n| skel.by_name.get(bones.get(n)?.trim()).copied());
        let tr = Transform::from_matrix(Mat4::from(part.root * s.transform));
        let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), tr)).id();
        // Named, so the holder's clips can drive its glow (`##FrontBulbs:0` in
        // `1hpattack3.kf`; actor_materials.rs).
        if let Some(n) = &s.name {
            commands.entity(e).insert((nvf_assets::actor::ShapeName(n.to_ascii_lowercase()), s.stored_look()));
        }
        commands.entity(actor.nodes[bone.unwrap_or(weapon_node)]).add_child(e);
        if bone.is_some() {
            commands.entity(e).insert(OnBack);
            out.back.push(e);
        } else if let Some(g) = names().find_map(|n| gun_nodes.iter().find(|g| g.eq_ignore_ascii_case(n))) {
            out.gun.push((e, g.clone()));
        }
        if let Some(drawn) = bone.is_none().then(|| drawn_place(held, s)).flatten() {
            out.posed.push((e, tr, Transform::from_matrix(Mat4::from(part.root * drawn))));
        }
        out.all.push(e);
    }
    out
}

/// Shape `s`'s place below the model's root with the weapon in the hand: each part above
/// it where the first of the `held` clips (the aim clip, then the draw clip) that keys it
/// leaves it at its end; `None` when they leave all of them at their place. The aim
/// clips hold the .357 revolver's, the Cowboy Repeater's and the lever-action shotgun's
/// hammers cocked; the 9mm pistol's aim clip does not key its hammer and its draw clip
/// cocks it, and the game's 9mm is held cocked [observed: animation.md §9.8; hypothesis:
/// the game leaves a part where the last clip put it, §9.7].
fn drawn_place(held: &[&nvf_assets::anim_play::Clip], s: &nvf_assets::actor::PartShape) -> Option<bevy::math::Affine3A> {
    use bevy::math::Affine3A;
    let mut moved = false;
    // `ancestors` runs from the shape's parent up; the product is outermost first.
    let m = s.ancestors.iter().rev().fold(Affine3A::IDENTITY, |acc, (name, local)| {
        let bind = Transform::from_matrix(Mat4::from(*local));
        let pose = name.as_deref().and_then(|n| held.iter().find_map(|c| nvf_assets::anim_blend::track_end(c, n, bind)));
        // Only a part away from its place in the model counts (the aim clips hold most
        // parts there).
        moved |= pose.is_some_and(|p| p.translation.distance(bind.translation) > 0.01 || p.rotation.angle_between(bind.rotation) > 0.1f32.to_radians());
        acc * pose.map_or(*local, |p| Affine3A::from_scale_rotation_translation(p.scale, p.rotation, p.translation))
    });
    moved.then_some(m * s.own)
}

/// Whether the gun shapes under node `name` ([`WeaponHold::gun`]) show now: as the
/// current stage's clip's visibility track for that node says at the clip's time (off
/// in `2hhholster.kf`, on from 0.500 s of `2hhequip.kf`, off from 0.500 s of
/// `2hhunequip.kf`; observed, animation.md §9.7); a clip with no track for it (the aim
/// clip) leaves it as it was (`last`), shown at first, as the player body does
/// [hypothesis: animation.md §9.7].
pub fn gun_shown(hold: &WeaponHold, name: &str, last: Option<bool>) -> bool {
    let c = hold.anim.stage.clip();
    let facts = hold.stance.facts_for(hold.crouched);
    let track = hold.stance.clip_for(c, hold.crouched, false).and_then(|b| {
        let (_, _, t) = b.clip.visibility.iter().find(|(n, ..)| n.eq_ignore_ascii_case(name))?;
        let len = facts.get(&c).map_or(0.0, |f| f.duration);
        t.at(b.clip.start + WeaponAnim::phase(c, hold.anim.time, facts) * len)
    });
    track.or(last).unwrap_or(true)
}

/// Hang the `Weapon` node of every armed actor from the bone its draw cycle names (the
/// draw cycle itself advances in `locomotion::drive`), and show or hide its gun shapes
/// ([`gun_shown`]).
pub fn reparent(mut commands: Commands, mut q: Query<(&mut WeaponHold, &BlendPlayer)>) {
    for (mut w, player) in &mut q {
        let shown: Vec<bool> = w.gun.iter().map(|(_, name, last)| gun_shown(&w, name, *last)).collect();
        for ((e, _, last), shown) in w.gun.iter_mut().zip(shown) {
            if *last != Some(shown) {
                commands.entity(*e).try_insert(if shown { Visibility::Inherited } else { Visibility::Hidden });
                *last = Some(shown);
            }
        }
        let in_hand = w.in_hand();
        if w.parts_in_hand != Some(in_hand) {
            for (e, holstered, drawn) in &w.parts {
                commands.entity(*e).try_insert(if in_hand { *drawn } else { *holstered });
            }
            w.parts_in_hand = Some(in_hand);
        }
        let Some(p) = w.parent_node() else { continue };
        if p != w.parent {
            commands.entity(player.nodes[p]).add_child(player.nodes[w.stance.weapon_node]);
            w.parent = p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::weapon::{ClipFacts, Stage};

    fn stance() -> Arc<Stance> {
        let f = |duration: f32, bone: &str| ClipFacts { duration, blend_time: 0.2, prn: vec![(0.5, bone.to_string())], switch: None };
        let facts = StanceFacts::from([
            (EquipClip::Holster, f(0.1, "Bip01 Pelvis")),
            (EquipClip::Equip, f(0.4, "Bip01 R Hand")),
            (EquipClip::Aim, f(2.0, "Bip01 R Hand")),
            (EquipClip::Unequip, f(0.5, "Bip01 Pelvis")),
        ]);
        let bones = HashMap::from([("Bip01 Pelvis".to_string(), 3), ("Bip01 R Hand".to_string(), 7)]);
        Arc::new(Stance { prefix: "1hp".into(), clips: BTreeMap::new(), weapon_only: BTreeMap::new(), sneak_facts: facts.clone(), facts, holster: "Bip01 Pelvis".into(), drawn: "Bip01 R Hand".into(), weapon_node: 8, bones, weapon_rest: None, sneak_weapon_rest: None, grip: None, sneak_grip: None, holstered_off: Vec::new(), moves: Arc::default(), folder: r"meshes\characters\_male".into(), sneak: BTreeMap::new(), sneak_weapon_only: BTreeMap::new() })
    }

    #[test]
    fn test_aid_draws_and_holsters() {
        let mut w = WeaponHold::new(stance(), false, 7);
        w.test = Some((1.0, 0.0));
        assert_eq!(w.parent_node(), Some(3));
        let mut stages = vec![w.anim.stage];
        for _ in 0..240 {
            w.simulate(1.0 / 60.0);
            if stages.last() != Some(&w.anim.stage) {
                stages.push(w.anim.stage);
            }
        }
        // Four seconds: holstered for a second, drawn for one (the equip clip first), holstered
        // (after the unequip clip), drawn again.
        assert_eq!(stages, [Stage::Holstered, Stage::Equipping, Stage::Drawn, Stage::Unequipping, Stage::Holstered, Stage::Equipping, Stage::Drawn]);
        // Repeatable: the same time gives the same state.
        let (mut a, mut b) = (WeaponHold::new(stance(), false, 7), WeaponHold::new(stance(), false, 7));
        a.test = Some((1.0, 0.0));
        b.test = Some((1.0, 0.0));
        a.simulate(1.1);
        b.simulate(1.1);
        assert_eq!(a.anim, b.anim);
        // Early in the draw the weapon is still at the hip; past the clip's `prn:` key (half
        // way) it is in the hand.
        assert_eq!(a.anim.stage, Stage::Equipping);
        assert_eq!(a.parent_node(), Some(3));
        a.simulate(0.2);
        assert_eq!(a.anim.stage, Stage::Equipping);
        assert_eq!(a.parent_node(), Some(7));
    }

    #[test]
    fn crouching_takes_the_crouched_clips_where_the_stance_has_them() {
        let clip = |stop: f32| {
            let mut c = nvf_assets::anim_play::Clip::from_controllers(Vec::new(), Vec::new(), Vec::new());
            c.stop = stop;
            Arc::new(BoundClip { clip: Arc::new(c), tracks: Vec::new(), root: None, root_track: None, carried: None })
        };
        // As `1hp*.kf` and `sneak1hp*.kf`: crouched forms of the equip, aim and unequip
        // clips, none of the holster clip (actors.md §9.4).
        let mut st = Arc::try_unwrap(stance()).ok().expect("one owner");
        for c in EquipClip::ALL {
            st.clips.insert(c, clip(st.facts[&c].duration));
        }
        for c in [EquipClip::Equip, EquipClip::Aim, EquipClip::Unequip] {
            st.sneak.insert(c, clip(st.facts[&c].duration));
        }
        let mut w = WeaponHold::new(Arc::new(st), true, 7);
        let mut player = BlendPlayer::new(vec![Entity::PLACEHOLDER; 10], Arc::new(vec![Transform::IDENTITY; 10]));
        let shown = |w: &WeaponHold, player: &mut BlendPlayer| {
            w.layers(player);
            let is = |l: &Option<Layer>, c: &Arc<BoundClip>| l.as_ref().is_some_and(|l| Arc::ptr_eq(&l.clip, c));
            let st = &w.stance;
            let name = |l: &Option<Layer>| {
                EquipClip::ALL.into_iter().find_map(|c| {
                    if st.sneak.get(&c).is_some_and(|b| is(l, b)) {
                        Some(format!("sneak {c:?}"))
                    } else {
                        st.clips.get(&c).filter(|b| is(l, b)).map(|_| format!("{c:?}"))
                    }
                })
            };
            (name(&player.layers[2]), name(&player.layers[3]))
        };
        w.step(1.0);
        assert_eq!(shown(&w, &mut player), (Some("Aim".into()), None));
        assert_eq!(w.crouched_clip_news(), None);
        // Crouching with it drawn: the crouched aim fades in over the standing one.
        assert!(w.set_crouched(true));
        assert!(!w.set_crouched(true));
        w.step(0.1);
        assert_eq!(shown(&w, &mut player), (Some("sneak Aim".into()), Some("Aim".into())));
        assert_eq!(w.crouched_clip_news(), Some((EquipClip::Aim, r"meshes\characters\_male\sneak1hpaim.kf".to_string())));
        assert_eq!(w.crouched_clip_news(), None, "each file once");
        w.step(0.2);
        assert_eq!(shown(&w, &mut player), (Some("sneak Aim".into()), None));
        // Putting it away crouched: the crouched unequip over the crouched aim, then the
        // standing holster clip (no crouched one), the weapon at the hip.
        w.drawn = false;
        w.step(0.01);
        assert_eq!(shown(&w, &mut player), (Some("sneak Unequip".into()), Some("sneak Aim".into())));
        w.step(1.0);
        w.step(0.3);
        assert_eq!(w.anim.stage, Stage::Holstered);
        assert_eq!(shown(&w, &mut player), (Some("Holster".into()), None));
        assert_eq!(w.parent_node(), Some(3));
        // Standing up holstered changes no clip: nothing fades.
        assert!(w.set_crouched(false));
        assert_eq!(w.anim.prev, None);
        // Drawing standing, crouching half way: the draw goes on in the crouched equip.
        w.drawn = true;
        w.step(0.01);
        w.step(0.1);
        assert!(w.set_crouched(true));
        assert_eq!(w.anim.stage, Stage::Equipping);
        assert_eq!(shown(&w, &mut player), (Some("sneak Equip".into()), Some("Equip".into())));
    }

    #[test]
    fn gun_shows_as_the_clips_say() {
        use fnv_nif::anim::BoolTrack;
        // As `2hhholster.kf` / `2hhequip.kf` / `2hhunequip.kf` (animation.md §9.7): off,
        // on from half way through the draw, off from half way through putting it away.
        let clip = |stop: f32, keys: Vec<(f32, bool)>| {
            let mut c = nvf_assets::anim_play::Clip::from_controllers(Vec::new(), Vec::new(), Vec::new());
            c.stop = stop;
            if !keys.is_empty() {
                c.visibility.push(("##FlamerGun".into(), 0, BoolTrack { value: None, keys }));
            }
            Arc::new(BoundClip { clip: Arc::new(c), tracks: Vec::new(), root: None, root_track: None, carried: None })
        };
        let mut st = Arc::try_unwrap(stance()).ok().expect("one owner");
        st.clips.insert(EquipClip::Holster, clip(0.1, vec![(0.0, false)]));
        st.clips.insert(EquipClip::Equip, clip(0.4, vec![(0.0, false), (0.2, true)]));
        st.clips.insert(EquipClip::Aim, clip(2.0, Vec::new()));
        st.clips.insert(EquipClip::Unequip, clip(0.5, vec![(0.0, true), (0.25, false)]));
        st.holstered_off = holstered_off(&st.clips[&EquipClip::Holster].clip);
        assert_eq!(st.holstered_off, ["##FlamerGun"]);
        let mut w = WeaponHold::new(Arc::new(st), false, 7);
        let mut last = None;
        let mut seen = Vec::new();
        for i in 0..200 {
            w.drawn = (40..120).contains(&i);
            w.step(1.0 / 60.0);
            let shown = gun_shown(&w, "##FlamerGun", last);
            if last != Some(shown) {
                seen.push((w.anim.stage, shown));
            }
            last = Some(shown);
        }
        // Hidden holstered and early in the draw; shown from half way through it and on
        // through the aim clip, which has no track; hidden again half way through putting
        // it away.
        assert_eq!(seen, [(Stage::Holstered, false), (Stage::Equipping, true), (Stage::Unequipping, false)]);
    }
}
