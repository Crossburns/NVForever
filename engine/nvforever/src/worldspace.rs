//! Door travel into another worldspace (`--play`; docs/design/layer4-play.md §3): a
//! `nvf_play::Travel` whose `world` differs from the one drawn switches what is drawn to
//! that worldspace, rebuilt with the same setup code the start uses:
//! - `WorldData` points at the new worldspace (its grid cells and persistent references);
//!   `world::travel`, which runs next, drops the loaded cells and render streaming loads
//!   the destination's cells around the camera.
//! - Sky and weather: `SkyState::new` from the new worldspace's climate (or its parent's,
//!   as at the start) with the clock and the command-line choices carried over, and the
//!   sky's entities spawned again by `sky::setup_sky` (stars, moon and sun depend on the
//!   climate). Its image space comes with it (`SkyState::image_space`).
//! - Distant LOD: a fresh `LodState` (its paths are named after the worldspace), with
//!   generated distant terrain for the new worldspace unless switched off.
//! - Water: distant water planes and their per-cell surfaces are dropped (they hold the
//!   old worldspace's default water height and water type).
//!
//! Each worldspace's own records are used. A child worldspace (WRLD `WNAM`) may take data
//! from its parent by its `PNAM` flags [documented: xEdit names, schemas/fnv/world.json];
//! every child worldspace in the ten masters sets only "Use Map Data" and has its own
//! climate, water, image space and land [observed: `fnvtool world list`, `esm decode`],
//! so taking the parent's is not implemented beyond what the start already does.

use crate::lod::{GenState, LodState};
use crate::sky::{self, CloudLayer, MoonDisc, SkyDome, SkyState, Stars, Sun};
use crate::water::DistantWater;
use crate::world::WorldData;
use crate::Args;
use bevy::prelude::*;
use nvf_assets::GameData;

/// The sky's entities, spawned by `sky::setup_sky`.
type SkyEntity = Or<(With<SkyDome>, With<CloudLayer>, With<Sun>, With<Stars>, With<MoonDisc>)>;

/// Switch to the travel's worldspace when it is another one (runs before `world::travel`).
#[allow(clippy::too_many_arguments)]
pub fn switch(
    mut commands: Commands,
    mut msgs: MessageReader<nvf_play::Travel>,
    mut world: ResMut<WorldData>,
    mut sky: ResMut<SkyState>,
    mut lod: ResMut<LodState>,
    mut water: ResMut<DistantWater>,
    sky_entities: Query<Entity, SkyEntity>,
    game: Res<GameData>,
    args: Res<Args>,
) {
    let drawn = world.world;
    let Some(to) = msgs.read().last().and_then(|t| t.world).filter(|&w| w != drawn) else { return };
    let Some(ws) = world.index.worldspaces.get(&to) else {
        warn!("travel into worldspace {to:08X}: not in the world index; staying in {:08X}", world.world);
        return;
    };
    let name = ws.edid.clone().unwrap_or_default();
    info!("render: worldspace {:08X} -> {to:08X} ({name})", world.world);
    world.set_world(to);

    match SkyState::new(&world, args.weather.as_deref(), None, sky.hour, sky.rng) {
        Ok(mut s) => {
            s.shadows = sky.shadows;
            s.shadow_distance = sky.shadow_distance;
            s.shadow_reach = sky.shadow_reach;
            s.shadow_cascades = sky.shadow_cascades;
            s.grade = sky.grade;
            s.time_scale = sky.time_scale;
            s.clock_running = sky.clock_running;
            s.days_passed = sky.days_passed;
            s.clock_seen = sky.clock_seen;
            s.fog_override = sky.fog_override;
            info!("render: {name}: {}", s.describe());
            *sky = s;
            for e in &sky_entities {
                commands.entity(e).despawn();
            }
            commands.run_system_cached(sky::setup_sky);
        }
        Err(e) => warn!("render: sky of {name}: {e}; keeping the previous worldspace's sky"),
    }

    lod.set_enabled(&mut commands, false);
    // Also with --no-lod: the settings page can switch distant LOD on later.
    let gen = if args.no_lodgen {
        None
    } else {
        match crate::lodgen_job(&args, &world, &game) {
            Ok(job) => Some(GenState::new(job, args.lodgen_radius.max(0))),
            Err(e) => {
                warn!("render: generated LOD for {name}: {e}");
                None
            }
        }
    };
    *lod = LodState::new(&world, crate::lod_settings(&args), !args.no_lod, gen).with_origin(crate::lod::grid_origin(&game, &world));

    water.clear(&mut commands);
}
