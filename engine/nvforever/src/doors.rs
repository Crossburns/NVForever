//! Drawing doors that open in place: a DOOR reference without XTEL whose model has
//! "Open" and "Close" sequences is spawned with its node hierarchy (world.rs) and posed
//! every frame its state changes from `nvf_play::doors::Doors`, the clock the physics
//! and waiting actors follow too (docs/design/navmesh.md §6.1, docs/spec-notes/doors.md).
//! At rest a door holds the end of "Open" when open, and its stored pose when closed,
//! which is the end of "Close" for 606 of 610 keyframed door bodies [observed, doors.md
//! §1]; the Lucky 38's outer door's "Close" holds it open (`DoorMotion::closed`, #1541).

use bevy::prelude::*;
use nvf_assets::anim_play::Playing;
use nvf_assets::model::PreparedModel;
use nvf_play::doors::{initial_open, DoorTiming, Doors};
use nvf_play::sim::SimState;
use std::collections::HashMap;
use std::sync::Arc;

/// On the root of a drawn door that opens in place.
#[derive(Component)]
pub struct DoorRig {
    pub form_id: u32,
    model: Arc<PreparedModel>,
    source: String,
    /// The rig's node entities by name.
    nodes: HashMap<String, Entity>,
    /// ONAM "Open by Default" of the placement.
    open_by_default: bool,
    /// The sequence (true: "Open") and time last shown.
    shown: Option<(bool, f32)>,
}

impl DoorRig {
    pub fn new(form_id: u32, model: Arc<PreparedModel>, source: &str, nodes: HashMap<String, Entity>, open_by_default: bool) -> DoorRig {
        DoorRig { form_id, model, source: source.to_string(), nodes, open_by_default, shown: None }
    }

    fn timing(&self) -> Option<DoorTiming> {
        let d = self.model.rig.as_ref()?.door.as_ref()?;
        Some(DoorTiming::new((d.open.start, d.open.stop, &d.open.text_keys), (d.close.start, d.close.stop, &d.close.text_keys)))
    }
}

/// Know newly drawn doors in [`Doors`] (as the simulation has them, else as placed).
pub fn register(rigs: Query<&DoorRig, Added<DoorRig>>, mut doors: ResMut<Doors>, sim: Option<Res<SimState>>) {
    for rig in &rigs {
        if let Some(t) = rig.timing() {
            doors.register(rig.form_id, initial_open(sim.as_deref(), rig.form_id, rig.open_by_default), t);
        }
    }
}

/// Test aid `--open-doors`: open each door once, when it is first drawn.
pub fn open_all(rigs: Query<&DoorRig, Added<DoorRig>>, mut doors: ResMut<Doors>) {
    for rig in &rigs {
        doors.set_open(rig.form_id, true);
    }
}

/// Pose each drawn door whose state changed since it was last shown.
pub fn pose(doors: Res<Doors>, mut rigs: Query<&mut DoorRig>, mut transforms: Query<&mut Transform>) {
    for mut rig in &mut rigs {
        let Some(want) = doors.get(rig.form_id).map(|d| d.motion.pose()) else { continue };
        if rig.shown == Some(want) {
            continue;
        }
        let Some(clips) = rig.model.rig.as_ref().and_then(|r| r.door.as_ref()) else { continue };
        if doors.get(rig.form_id).is_some_and(|d| d.motion.closed()) {
            // Closed: the nodes the sequences move go back to their stored pose.
            let stored = rig.model.rig.as_ref().map(|r| r.nodes.as_slice()).unwrap_or_default();
            for (name, &e) in &rig.nodes {
                let moved = [&clips.open, &clips.close].iter().any(|c| c.tracks.iter().any(|(n, _)| n == name));
                let Some(node) = stored.iter().find(|n| n.name.as_deref() == Some(name.as_str())).filter(|_| moved) else { continue };
                if let Ok(mut t) = transforms.get_mut(e) {
                    *t = node.local;
                }
            }
            rig.shown = Some(want);
            continue;
        }
        let clip = if want.0 { &clips.open } else { &clips.close };
        let mut p = Playing::new(&rig.source, clip, |n| rig.nodes.get(n).copied());
        p.time = clip.start + want.1;
        p.step(0.0, true, &mut transforms);
        rig.shown = Some(want);
    }
}
