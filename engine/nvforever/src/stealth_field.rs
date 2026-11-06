//! The Stealth Boy's look on the player (docs/spec-notes/aid-items.md §9,
//! docs/spec-notes/refraction.md §5): while the player's Chameleon actor value is 1 or
//! more (`fnv_game::aid`, the Stealth Boy's "Stealth Field" effect), the player's body
//! and arms rig are drawn refracting: each mesh shows what lies behind it, bent by its own
//! normals and normal map, with the normal map scrolling so the bend shimmers
//! (`nvf_assets::nif_material`, "Refraction" and "Fire refraction").
//!
//! [hypothesis: the Stealth Field's MGEF names no effect shader (archetype and actor value
//! Chameleon only) [observed], so the look is the engine's; strength [`STRENGTH`] and
//! period [`PERIOD`] are chosen near the game's own refracting shapes (0.04 for Mister
//! Handy's jet, 0.056 for the Incinerator's burst; docs/spec-notes/refraction.md).] Every
//! mesh gets a refracting copy of its material; the copies are dropped and the materials
//! put back when the effect ends. Detection does not read this: it reads the Chameleon
//! value itself (`fnv_game::detection`).
//!
//! The Pip-Boy's screen and its glare are the one exception: while the menus are on it
//! (`pipboy_arm.rs`) they are drawn plain, so the menus can be read and used; the rest of
//! the arm and the Pip-Boy stay see-through, and both refract again once the Pip-Boy is
//! down (owner, 2026-10-23) [design].
//!
//! Log lines: `stealth field: on (Chameleon 75): <n> meshes see-through (refracting,
//! strength 0.030)` and `stealth field: off`.

use crate::pipboy_arm::PipBoyPart;
use crate::player_body::PlayerBody;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::nif_material::NifMaterial;
use nvf_play::pipboy::PipBoyScreen;
use nvf_play::script::ScriptState;
use std::collections::HashMap;

/// The Chameleon actor value [documented: xEdit ActorValue, `schemas/fnv/common.json`].
const CHAMELEON: u16 = 49;

/// The screen shift of the field: screen UV per unit of view-space normal [hypothesis,
/// docs/spec-notes/refraction.md §5]; the material's strength is this over
/// `nif_material::REFRACTION_SCREEN`.
pub const STRENGTH: f32 = 0.03;

/// Seconds for the normal maps to scroll once, the shimmer [hypothesis].
pub const PERIOD: f32 = 2.0;

/// The cut of a blended part (hair, eyelashes) drawn refracting [hypothesis].
const BLEND_CUT: f32 = 0.5;

/// The meshes made see-through and their own materials.
#[derive(Resource, Default)]
pub struct StealthField {
    pub on: bool,
    originals: HashMap<Entity, Handle<NifMaterial>>,
}

pub struct StealthFieldPlugin;

impl Plugin for StealthFieldPlugin {
    fn build(&self, app: &mut App) {
        // After the Pip-Boy has put the menus on its screen or taken them off, so the
        // screen is skipped exactly while the menus are on it.
        app.init_resource::<StealthField>().add_systems(Update, apply.after(nvf_play::player::BodySet).after(crate::pipboy_arm::show_screen));
    }
}

impl StealthField {
    /// Give `e` its own material back and stop making it see-through: the material it
    /// had before the field, or `None` when the field had not touched it. The
    /// see-through copy is dropped.
    pub fn take_original(&mut self, e: Entity, current: &Handle<NifMaterial>, materials: &mut Assets<NifMaterial>) -> Option<Handle<NifMaterial>> {
        let original = self.originals.remove(&e)?;
        materials.remove(current);
        Some(original)
    }

    /// As the field does when it makes `e` see-through, for a test of what takes it back.
    #[cfg(test)]
    pub(crate) fn keep_original(&mut self, e: Entity, original: Handle<NifMaterial>) {
        self.originals.insert(e, original);
    }
}

/// The see-through copy of a material: refracting, keeping the outline of its cut.
pub fn see_through(m: &NifMaterial) -> NifMaterial {
    let mut m = m.clone();
    let cut = match m.base.alpha_mode {
        AlphaMode::Mask(cut) => cut,
        AlphaMode::Opaque => m.extension.alpha_cut,
        _ => m.extension.alpha_cut.max(BLEND_CUT),
    };
    nvf_assets::nif_spawn::refraction_material(&mut m, STRENGTH / nvf_assets::nif_material::REFRACTION_SCREEN, PERIOD);
    m.extension.alpha_cut = cut;
    m
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    script: Option<Res<ScriptState>>,
    body: Res<PlayerBody>,
    pip_screen: Option<Res<PipBoyScreen>>,
    parts: Query<&PipBoyPart>,
    mut field: ResMut<StealthField>,
    children: Query<&Children>,
    mut meshes: Query<&mut MeshMaterial3d<NifMaterial>>,
    mut materials: ResMut<Assets<NifMaterial>>,
) {
    let chameleon = script.as_ref().and_then(|s| s.rt.as_ref()).map_or(0.0, |rt| fnv_game::functions::actor_value_temporary(&rt.game, FormId::PLAYER_REF, CHAMELEON));
    let want = chameleon >= 1.0;
    if want {
        // Every frame: a weapon drawn or an outfit changed brings new meshes.
        let mut fresh = 0;
        let menus_on_screen = pip_screen.is_some_and(|s| s.on_model);
        for root in body.roots() {
            for e in children.iter_descendants(root) {
                if field.originals.contains_key(&e) || (menus_on_screen && matches!(parts.get(e), Ok(PipBoyPart::Screen | PipBoyPart::Glare))) {
                    continue;
                }
                let Ok(mut handle) = meshes.get_mut(e) else { continue };
                let Some(copy) = materials.get(&handle.0).map(see_through) else { continue };
                let copy = materials.add(copy);
                field.originals.insert(e, std::mem::replace(&mut handle.0, copy));
                fresh += 1;
            }
        }
        if !field.on {
            info!("stealth field: on (Chameleon {chameleon:.0}): {fresh} meshes see-through (refracting, strength {STRENGTH:.3})");
            field.on = true;
        }
    } else if field.on {
        for (e, original) in field.originals.drain() {
            if let Ok(mut handle) = meshes.get_mut(e) {
                materials.remove(&handle.0);
                handle.0 = original;
            }
        }
        field.on = false;
        info!("stealth field: off");
    }
    // Meshes despawned meanwhile.
    field.originals.retain(|e, _| meshes.contains(*e));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_refracts_and_keeps_its_cut() {
        let m = nvf_assets::nif_material::nif_material(StandardMaterial { base_color: Color::WHITE, alpha_mode: AlphaMode::Mask(0.4), ..default() });
        let c = see_through(&m);
        assert!(c.extension.refract);
        assert!(matches!(c.base.alpha_mode, AlphaMode::Opaque));
        assert!((c.extension.refraction * nvf_assets::nif_material::REFRACTION_SCREEN - STRENGTH).abs() < 1e-6);
        assert!((c.extension.fire_period - PERIOD).abs() < 1e-6);
        assert!((c.extension.alpha_cut - 0.4).abs() < 1e-6);
        let hair = nvf_assets::nif_material::nif_material(StandardMaterial { alpha_mode: AlphaMode::Blend, ..default() });
        assert!((see_through(&hair).extension.alpha_cut - BLEND_CUT).abs() < 1e-6);
    }
}
