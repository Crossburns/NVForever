//! The console's render toggles (docs/design/console.md): `tg`, `ts`, `tws`, `tt`,
//! `ToggleLODLand`, `ToggleNavmesh`, `tlik` (head tracking), `temo` (faces' emotions)
//! and xNVSE's Con_TFIK (foot IK) asked through
//! `nvf_play::console::RenderToggles`,
//! switched here and reported in the console.
//!
//! - Grass, distant land, the navmesh drawing and head tracking switch what the settings
//!   page switches (`GrassConfig::enabled`, the LOD as `--no-lod`, `NavState::show`,
//!   `HeadLookSettings::enabled`); emotions `LipState::emotions_off`.
//! - Sky, water and trees have no switch of their own: their meshes are kept hidden
//!   ([`keep_hidden`]) while the toggle is off, each frame after the systems that show and
//!   hide them, and given back their visibility when it is on again. Sky: the dome,
//!   clouds, stars and moons (the sun's light stays) [hypothesis: the original's `ts`
//!   leaves a plain horizon, catalogue summary]. Trees: whole SpeedTrees and their
//!   distant billboards [hypothesis: the catalogue says the original's `tt` stops their
//!   animated parts; hiding them is our choice].

use crate::grass::GrassConfig;
use crate::lod::LodState;
use crate::nav::NavState;
use crate::sky::{CloudLayer, MoonDisc, SkyDome, Stars};
use crate::tree::{BarkMaterial, BillboardMaterial, LeafMaterial};
use crate::water::WaterMaterial;
use crate::world::Streamer;
use crate::Args;
use bevy::camera::visibility::VisibilitySystems;
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use fnv_game::functions::toggles::Toggle;
use nvf_play::console::{Console, RenderToggles};
use std::collections::HashMap;

/// The drawings the console hides (true: hidden).
#[derive(Resource, Default)]
pub struct Hidden {
    pub sky: bool,
    pub water: bool,
    pub trees: bool,
    pub leaves: bool,
}

pub fn register(app: &mut App) {
    app.init_resource::<Hidden>()
        .add_systems(Update, apply.after(nvf_play::PlaySet::Ui))
        .add_systems(PostUpdate, (hide_sky, hide_water, (hide_leaves, hide_trees).chain()).before(VisibilitySystems::VisibilityPropagate));
}

fn state(off: bool) -> &'static str {
    if off {
        "off"
    } else {
        "on"
    }
}

fn apply(world: &mut World) {
    let asked = std::mem::take(&mut world.resource_mut::<RenderToggles>().0);
    for t in asked {
        let line = match t {
            Toggle::Grass => {
                let mut g = world.resource_mut::<GrassConfig>();
                g.enabled = !g.enabled;
                format!("Grass {}", state(!g.enabled))
            }
            Toggle::Sky => {
                let mut h = world.resource_mut::<Hidden>();
                h.sky = !h.sky;
                format!("Sky {}", state(h.sky))
            }
            Toggle::Water => {
                let mut h = world.resource_mut::<Hidden>();
                h.water = !h.water;
                format!("Water {}", state(h.water))
            }
            Toggle::Trees => {
                let mut h = world.resource_mut::<Hidden>();
                h.trees = !h.trees;
                format!("Trees {}", state(h.trees))
            }
            Toggle::Leaves => {
                let mut h = world.resource_mut::<Hidden>();
                h.leaves = !h.leaves;
                format!("Tree leaves {}", state(h.leaves))
            }
            Toggle::LodLand => {
                // As the settings page's distant-land switch (`--no-lod`).
                let on = world.resource::<Args>().no_lod;
                world.resource_mut::<Args>().no_lod = !on;
                let outdoors = world.resource::<Streamer>().interior.is_none();
                world.resource_scope(|world, mut lod: Mut<LodState>| lod.set_enabled(&mut world.commands(), on && outdoors));
                world.flush();
                format!("Distant land {}", state(!on))
            }
            Toggle::Navmesh => match world.get_resource_mut::<NavState>() {
                Some(mut n) => {
                    n.show = !n.show;
                    format!("Navmesh drawing {}", state(!n.show))
                }
                None => "No navmesh is loaded in this run".to_string(),
            },
            // Con_TFIK: as the settings page's switch (`--no-foot-ik`).
            Toggle::FootIk => match world.get_resource_mut::<crate::foot_ik::FootIkSettings>() {
                Some(mut f) => {
                    f.enabled = !f.enabled;
                    format!("Foot IK {}", state(!f.enabled))
                }
                None => "No foot IK in this run".to_string(),
            },
            Toggle::LookIk => match world.get_resource_mut::<crate::head_look::HeadLookSettings>() {
                Some(mut h) => {
                    h.enabled = !h.enabled;
                    format!("Head tracking {}", state(!h.enabled))
                }
                None => "No head tracking in this run".to_string(),
            },
            Toggle::Emotions => match world.get_resource_mut::<crate::lips::LipState>() {
                Some(mut l) => {
                    l.emotions_off = !l.emotions_off;
                    format!("Emotions {}", state(l.emotions_off))
                }
                None => "No faces move in this run".to_string(),
            },
            other => format!("{other:?}: not a render toggle"),
        };
        world.resource_mut::<Console>().say(line);
    }
}

/// While `hidden`, every entity of the filter is hidden, its visibility kept (the latest
/// another system gave it); when shown again, each gets that visibility back.
fn keep_hidden<F: QueryFilter>(hidden: bool, saved: &mut HashMap<Entity, Visibility>, mut q: Query<(Entity, &mut Visibility), F>) {
    if hidden {
        for (e, mut v) in &mut q {
            if *v != Visibility::Hidden {
                saved.insert(e, *v);
                *v = Visibility::Hidden;
            }
        }
    } else {
        for (e, v) in saved.drain() {
            if let Ok((_, mut cur)) = q.get_mut(e) {
                *cur = v;
            }
        }
    }
}

type SkyParts = Or<(With<SkyDome>, With<CloudLayer>, With<Stars>, With<MoonDisc>)>;
type TreeParts = Or<(With<MeshMaterial3d<LeafMaterial>>, With<MeshMaterial3d<BarkMaterial>>, With<MeshMaterial3d<BillboardMaterial>>)>;

fn hide_sky(h: Res<Hidden>, mut saved: Local<HashMap<Entity, Visibility>>, q: Query<(Entity, &mut Visibility), SkyParts>) {
    keep_hidden(h.sky, &mut saved, q);
}

fn hide_water(h: Res<Hidden>, mut saved: Local<HashMap<Entity, Visibility>>, q: Query<(Entity, &mut Visibility), With<MeshMaterial3d<WaterMaterial>>>) {
    keep_hidden(h.water, &mut saved, q);
}

/// The SpeedTrees' leaf cards; distant trees' billboards keep theirs [hypothesis: the
/// catalogue speaks of SpeedTrees' leaves only]. Run before [`hide_trees`], so leaves shown
/// again under hidden trees are hidden again in the same frame.
fn hide_leaves(h: Res<Hidden>, mut saved: Local<HashMap<Entity, Visibility>>, q: Query<(Entity, &mut Visibility), With<MeshMaterial3d<LeafMaterial>>>) {
    keep_hidden(h.leaves, &mut saved, q);
}

fn hide_trees(h: Res<Hidden>, mut saved: Local<HashMap<Entity, Visibility>>, q: Query<(Entity, &mut Visibility), TreeParts>) {
    keep_hidden(h.trees, &mut saved, q);
}
