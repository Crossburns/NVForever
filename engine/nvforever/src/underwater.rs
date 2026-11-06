//! Under-water fog: while the camera is below the water surface of its cell, the
//! camera's distance fog becomes the water type's under-water fog (WATR DNAM under fog
//! near / far [documented: xEdit names]) in its deep colour, lit by the current sun and
//! ambient light like the water body [hypothesis; the light level is tuned to match the
//! water surface's own shading]. Leaving the water hands the fog back to the weather.

use crate::reflection::ReflectionCamera;
use crate::sky::{SkyState, Sun};
use crate::water::DistantWater;
use crate::world::{bevy_to_game, Streamer, WorldData};
use nvf_play::phys::PhysState;
use nvf_play::sim::SimState;
use fnv_sim::FormId;
use bevy::camera::Exposure;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use fnv_world::cell_of;
use fnv_world::sky::{srgb_to_linear, Water};
use std::collections::HashMap;

/// Water types read so far, by FormID.
#[derive(Default)]
pub struct WaterTypes(HashMap<u32, Option<Water>>);

fn linear(c: Color) -> Vec3 {
    let l = c.to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

/// The water surface the camera is under, with its type: outdoors the camera cell's
/// water when the camera is below it; indoors the physics' water the camera is in (placed
/// water volumes), coloured by that volume's own PWAT water type ([`fog_water_type`]).
/// An interior's own XCLW is not water [observed: physics.md §6.6, `swim_h2o_drops`,
/// `sw_guardian`]: e.g. The Tops' 13th floor and presidential suite store XCLW 14,095.9,
/// 5,000+ units above every reference in them, and reading it as a surface drew the
/// under-water fog (WastelandNV's Potomac, −1,000..700 units) round the camera there
/// (world.md §8 "Interior XCLW is not water"). Without physics (the viewer, no `--play`)
/// an interior has no under-water fog.
fn water_above(
    world: &WorldData,
    streamer: &Streamer,
    dw: &mut DistantWater,
    phys: Option<&PhysState>,
    sim: Option<&SimState>,
    placed_types: &mut HashMap<u32, Option<u32>>,
    cam_game: Vec3,
) -> Option<(f32, u32)> {
    let (h, wid) = match streamer.interior {
        Some(id) => {
            let c = world.index.cells.get(&id)?;
            let ws = &world.index.worldspaces[&world.world];
            let p = cam_game.to_array();
            let pw = &phys?.world;
            let h = pw.water_surface_for(p)?;
            // The placed volume that gave the surface (the highest one holding the
            // camera, as `water_surface_for` picks), and its base PWAT's water type.
            let placed = pw
                .placed_water()
                .filter(|w| w.holds_below(p) && w.surface == h)
                .map(|w| w.form_id)
                .next()
                .and_then(|r| {
                    *placed_types.entry(r).or_insert_with(|| {
                        let base = sim?.refs.get(FormId(r))?.base;
                        fnv_world::sky::placeable_water_type(world.lo, world.schema, base.0)
                    })
                });
            (h, fog_water_type(placed, c.water_type, ws.water)?)
        }
        None => dw.surface(world, cell_of(cam_game.x, cam_game.y))?,
    };
    (cam_game.z < h).then_some((h, wid))
}

/// The water type an interior's under-water fog takes: the placed water's own (its PWAT's
/// DNAM "Water", the type the renderer draws that pool with, world.rs
/// `spawn_placed_water`), else the cell's (XCWT), else the worldspace's [hypothesis: the
/// fog matches the pool's look; e.g. Vault 34's pools hold toxic water, not the
/// worldspace's Potomac].
fn fog_water_type(placed: Option<u32>, cell: Option<u32>, worldspace: Option<u32>) -> Option<u32> {
    placed.or(cell).or(worldspace)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn underwater_fog(
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    mut dw: ResMut<DistantWater>,
    mut sky: ResMut<SkyState>,
    ambient: Res<GlobalAmbientLight>,
    sun: Query<&DirectionalLight, With<Sun>>,
    mut cam: Query<(&GlobalTransform, &mut DistanceFog, Option<&Exposure>), (With<Camera3d>, Without<ReflectionCamera>)>,
    phys: Option<Res<PhysState>>,
    sim: Option<Res<SimState>>,
    mut types: Local<WaterTypes>,
    mut placed_types: Local<HashMap<u32, Option<u32>>>,
    mut was_under: Local<bool>,
) {
    let Ok((gt, mut fog, exposure)) = cam.single_mut() else { return };
    let under = water_above(&world, &streamer, &mut dw, phys.as_deref(), sim.as_deref(), &mut placed_types, bevy_to_game(gt.translation()));
    let water = under.and_then(|(_, wid)| {
        types.0.entry(wid).or_insert_with(|| fnv_world::sky::water(world.lo, world.schema, wid)).clone()
    });
    let Some(w) = water else {
        if std::mem::take(&mut *was_under) {
            // Back above the surface: the weather's fog again.
            info!("underwater: the camera is out of the water, the cell's fog again");
            sky.refresh();
        }
        return;
    };
    if !std::mem::replace(&mut *was_under, true) {
        let h = under.map_or(0.0, |(h, _)| h);
        info!("underwater: the camera is under water (surface {h:.1}), the water's under-water fog");
    }
    // The light a diffuse surface facing the sun receives: the display-value sum of the
    // sun and the ambient light (lights.rs `DISPLAY_HELPERS`; light × exposure, Lambert's
    // 1/π for the sun), raised to the display gamma.
    let exposure = exposure.copied().unwrap_or_default().exposure();
    let light = sun
        .single()
        .map(|s| linear(s.color) * s.illuminance / std::f32::consts::PI)
        .unwrap_or(Vec3::ZERO)
        + linear(ambient.color) * ambient.brightness;
    let deep = Vec3::from_array(w.deep.map(srgb_to_linear)) * (light * exposure).max(Vec3::ZERO).powf(2.2);
    let [near, far, _amount] = w.under_fog;
    fog.color = Color::LinearRgba(LinearRgba::rgb(deep.x, deep.y, deep.z));
    fog.falloff = FogFalloff::Linear { start: near, end: far.max(near + 1.0) };
}

#[cfg(test)]
mod tests {
    use super::fog_water_type;

    #[test]
    fn placed_water_fogs_in_its_own_type() {
        // A PWAT's own water type wins over the cell's XCWT and the worldspace's water.
        assert_eq!(fog_water_type(Some(0x10), Some(0x20), Some(0x30)), Some(0x10));
        // A volume whose type cannot be read: the cell's, else the worldspace's.
        assert_eq!(fog_water_type(None, Some(0x20), Some(0x30)), Some(0x20));
        assert_eq!(fog_water_type(None, None, Some(0x30)), Some(0x30));
        assert_eq!(fog_water_type(None, None, None), None);
    }
}
