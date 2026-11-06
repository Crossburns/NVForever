//! References a script enables that render streaming never drew.
//!
//! Render streaming (`engine/nvforever/src/world.rs`) skips references that are not
//! enabled at the start of a game (initially disabled, or following a disabled enable
//! parent), so `RefVisual` sync can hide what scripts disable but has nothing to show for
//! what they enable (VCG01's Rorschach cards in Doc Mitchell's house, enabled by Doc's
//! lines). While scripts run, this module draws those references itself when the
//! simulation says they are present, the way `dropped.rs` draws created references: the
//! base form's model at the reference's pose (the simulation's, if a script moved it),
//! as children of the cell's `RenderCell` entity, with `RefVisual` on each mesh entity
//! (crosshair bounds and visual sync see them like any other). Collision follows from
//! `RefChanged` (`phys.rs` restores a reference's collision when it becomes present).
//! One that physics then moves (the tutorial's bottles, shot off their fence) is drawn
//! where the simulation holds it ([`follow`]; `crate::moved` only moves render
//! streaming's entities, the children of a cell).
//!
//! Not drawn here: actors (ACHR/ACRE; `actors.rs` builds them), SpeedTrees, markers, and
//! lights' light sources (only a light's model). The enabled-at-start rule is a copy of
//! world.rs's (`WorldData::enabled`) [coordination: one shared function would keep the
//! two from drifting; docs/design/layer5-play.md].

use crate::moved::ref_affine;
use crate::phys::PhysState;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{PlaySet, PlayWorld, RefVisual, RenderCell};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_sim::FormId;
use fnv_world::placed::{base_object, placement, Placement};
use fnv_world::transform::ref_rotation;
use nvf_assets::model::ModelCtx;
use std::collections::{BTreeMap, HashMap};

/// Base types render streaming draws (world.rs `DRAWN`), without TREE (SpeedTree).
const DRAWN: [&[u8; 4]; 24] = [
    b"STAT", b"SCOL", b"MSTT", b"FURN", b"DOOR", b"CONT", b"ACTI", b"TACT", b"TERM", b"LIGH", b"MISC", b"WEAP", b"AMMO", b"ARMO",
    b"ALCH", b"BOOK", b"NOTE", b"KEYM", b"PWAT", b"IMOD", b"CHIP", b"CCRD", b"CMNY", b"PROJ",
];

/// A reference render streaming leaves out at the start but this module can draw.
#[derive(Debug, Clone)]
struct Dormant {
    placement: Placement,
    path: String,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ScriptDrawnRoot(pub u32);

/// A shape drawn here whose colour an external emittance source drives (its shader sets
/// "External_Emittance" and its reference names an XEMI source): the renderer swaps in
/// the driven material (nvforever `emittance.rs`), as render streaming does for
/// references enabled from the start. Without it the Strip's street lamp beams, enabled
/// by script at night, drew white instead of the stripStreetLight colour.
#[derive(Component, Clone, Copy, Debug)]
pub struct WantsEmittance(pub u32);

/// The model a [`ScriptDrawnRoot`] draws (archive path): the engine puts the model's
/// particle emitters under the root (`nvforever::particles`, particles.md §7.4).
#[derive(Component, Clone, Debug)]
pub struct ScriptDrawnModel(pub String);

/// On the rig root of a model drawn here whose rig has named sequences (`Forward`,
/// `Left`, …): its node entities by name, so the engine can play the groups scripts
/// ask for on it as on one render streaming drew (`nvforever::object_anims`). VMS15's
/// rising bomber is enabled by the detonator and then plays `Forward`.
#[derive(Component, Clone, Debug)]
pub struct ScriptDrawnRig {
    pub reference: u32,
    pub path: String,
    pub nodes: HashMap<String, Entity>,
}

#[derive(Resource, Default)]
pub struct ScriptDrawn {
    /// Dormant references per render cell, read when the cell first loads.
    cells: HashMap<RenderCell, Vec<Dormant>>,
    /// Drawn: reference → (root, the cell entity it hangs under).
    pub drawn: BTreeMap<u32, (Entity, Entity)>,
}

pub struct ScriptDrawPlugin;

impl Plugin for ScriptDrawPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptDrawn>().add_systems(Update, (sync, follow).chain().in_set(PlaySet::Sync));
    }
}

/// Whether render streaming draws a reference at the start of a game (world.rs's rule):
/// not deleted or initially disabled, and its enable parent in the required state.
fn enabled_at_start(play: &PlayWorld, p: &Placement, depth: usize) -> bool {
    if p.deleted() || p.initially_disabled() {
        return false;
    }
    match p.enable_parent {
        None => true,
        Some(_) if depth > 16 => false,
        Some((parent, opposite)) => match placement(play.lo, play.schema, parent) {
            Some(pp) => enabled_at_start(play, &pp, depth + 1) != opposite,
            None => !opposite,
        },
    }
}

fn is_marker(path: &str) -> bool {
    let file = path.rsplit('\\').next().unwrap_or(path);
    file.starts_with("marker") || path.contains("\\markers\\")
}

/// The dormant references of a render cell.
fn dormant_in(play: &PlayWorld, sim: &SimState, cell: RenderCell) -> Vec<Dormant> {
    let ids: Vec<FormId> = match cell {
        RenderCell::Interior(id) => sim.refs.by_cell.get(&FormId(id)).cloned().unwrap_or_default(),
        RenderCell::Exterior { world, x, y } => sim
            .refs
            .world_grid
            .get(&FormId(world))
            .and_then(|g| g.get(&(x, y)))
            .and_then(|c| sim.refs.by_cell.get(c))
            .cloned()
            .unwrap_or_default(),
    };
    ids.into_iter()
        .filter_map(|r| {
            let p = placement(play.lo, play.schema, r.0)?;
            // A primitive enabled by a script still shows nothing (level-design.md §4.1).
            if &p.sig != b"REFR" || p.deleted() || !p.draws_base_model() || enabled_at_start(play, &p, 0) {
                return None;
            }
            let b = base_object(play.lo, play.schema, p.base?)?;
            if !DRAWN.contains(&&b.sig) || b.is_speedtree() {
                return None;
            }
            let path = b.world_model_path().filter(|m| !is_marker(m) && !m.ends_with(".spt"))?;
            Some(Dormant { placement: p, path })
        })
        .collect()
}

/// Reference pose in game space (the simulation's position and rotation once it holds
/// state for the reference).
fn pose(sim: &SimState, p: &Placement) -> Affine3A {
    let (pos, rot) = sim.world.peek(FormId(p.form_id)).map_or((p.position, p.rotation), |s| (s.pos, s.rot));
    let m = Mat3::from_cols_array(&ref_rotation(rot)).transpose(); // row-major
    let scale = sim.script_scale(p.form_id).unwrap_or(p.scale);
    Affine3A::from_mat3_translation(m * scale, Vec3::from_array(pos))
}

type Cells<'w, 's> = Query<'w, 's, (Entity, &'static RenderCell, &'static Transform), Without<ScriptDrawnRoot>>;

/// Move the roots of the references physics moved this frame
/// (`PhysState::settle_placed`), or a script's Rotate turned or SetScale resized
/// (`Game::turned`), to the simulation's pose and scale.
fn follow(sim: Res<SimState>, phys: Res<PhysState>, script: Res<ScriptState>, state: Res<ScriptDrawn>, cells: Cells, mut roots: Query<&mut Transform, With<ScriptDrawnRoot>>) {
    let turned = script.rt.iter().flat_map(|rt| rt.game.turned.iter().map(|r| r.0));
    for id in phys.moved_now.iter().copied().chain(turned) {
        let Some(&(root, cell_e)) = state.drawn.get(&id) else { continue };
        let (Ok((_, _, cell)), Ok(mut t), Some(p)) = (cells.get(cell_e), roots.get_mut(root), sim.refs.get(FormId(id))) else { continue };
        let (pos, rot) = sim.moved_pose(id).unwrap_or((p.pos, p.rot));
        *t = Transform::from_matrix(Mat4::from(Affine3A::from_translation(-cell.translation) * ref_affine(pos, rot, sim.ref_scale(id))));
    }
}


#[allow(clippy::too_many_arguments)]
fn sync(
    mut commands: Commands,
    play: Res<PlayWorld>,
    sim: Res<SimState>,
    script: Res<ScriptState>,
    cells: Cells,
    roots: Query<(), With<ScriptDrawnRoot>>,
    mut state: ResMut<ScriptDrawn>,
    mut models: ModelCtx,
    fades: Option<Res<crate::fade::Fades>>,
) {
    if script.rt.is_none() {
        return;
    }
    let st = &mut *state;
    let loaded: HashMap<RenderCell, (Entity, Vec3)> = cells.iter().map(|(e, c, t)| (*c, (e, t.translation))).collect();
    // Drawn under a cell entity that went (render streaming, travel): gone with it.
    st.drawn.retain(|_, (root, cell_e)| roots.contains(*root) && loaded.values().any(|(e, _)| e == cell_e));
    for (&cell, &(cell_e, origin)) in &loaded {
        let dormant = st.cells.entry(cell).or_insert_with(|| dormant_in(&play, &sim, cell));
        for d in dormant.iter() {
            let id = d.placement.form_id;
            // One fading out (Disable 1, `crate::fade`) stays until its fade is over
            // [from the branch claude/script-params-engine].
            let want = sim.present(id) || fades.as_ref().is_some_and(|f| f.fading_out(id));
            match st.drawn.get(&id) {
                Some(&(root, e)) if !want || e != cell_e => {
                    commands.entity(root).try_despawn();
                    st.drawn.remove(&id);
                    info!("reference 0x{id:08X} no longer drawn by scripts (disabled or moved cell)");
                }
                Some(_) => {}
                None if want => {
                    let Some(model) = models.model(&d.path) else {
                        warn_once!("reference 0x{id:08X}: model {} could not be loaded", d.path);
                        continue;
                    };
                    let t = Transform::from_matrix(Mat4::from(Affine3A::from_translation(-origin) * pose(&sim, &d.placement)));
                    let root = commands.spawn((Name::new(format!("Enabled {id:08X}")), ScriptDrawnRoot(id), ScriptDrawnModel(d.path.clone()), t, Visibility::default(), ChildOf(cell_e))).id();
                    // A rig that only plays the model as an effect (`Rig::effect`, a mine's light
                    // controller) is not drawn as one: as render streaming, a reference's own node
                    // hierarchy is for an idle, a door or script groups, and the flat shapes keep a
                    // mine's light parts (`crate::mines::MineLight`).
                    let shapes = match model.rig.as_ref().filter(|rig| rig.idle.is_some() || rig.door.is_some() || !rig.groups.is_empty()) {
                        Some(rig) => {
                            let (r, drawn, nodes) = rig.spawn_nodes(&mut commands, root, Transform::IDENTITY, &d.path, 0.0);
                            commands.entity(r).insert(RefVisual(id));
                            if !rig.groups.is_empty() {
                                commands.entity(r).insert(ScriptDrawnRig { reference: id, path: d.path.clone(), nodes });
                            }
                            for e in &drawn {
                                commands.entity(*e).insert(RefVisual(id));
                            }
                            drawn.len()
                        }
                        None => {
                            for s in &model.shapes {
                                let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(s.transform)), RefVisual(id), ChildOf(root))).id();
                                if let Some(x) = d.placement.emittance.filter(|_| s.external_emittance) {
                                    commands.entity(e).insert(WantsEmittance(x));
                                }
                                if let Some(l) = crate::mines::MineLight::of(s.name.as_deref(), &s.ancestors) {
                                    commands.entity(e).insert(l);
                                }
                            }
                            model.shapes.len()
                        }
                    };
                    info!("reference 0x{id:08X} drawn (enabled at run time): {}, {shapes} shapes, in {cell:?}", d.path);
                    st.drawn.insert(id, (root, cell_e));
                }
                None => {}
            }
        }
    }
}
