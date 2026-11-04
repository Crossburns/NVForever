//! kNVSE's SetActorAnimationPath on the drawn actors, slice 2 of
//! docs/design/knvse-anim.md: an actor with replacement paths plays from its own copy of
//! the standing set, the replaced moves taken from the script's files
//! (`ClipSet::with_replacements`); when the script takes them back the shared set
//! returns. The paths come from the game (`fnv_game::nvse_anim::AnimPaths`), after the
//! files kNVSE's AnimGroupOverride folders give the actor (`Game::anim_overrides`,
//! `fnv_game::nvse_anim_folders`).

use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::model::ModelCtx;
use nvf_play::script::ScriptState;
use std::collections::HashMap;
use std::sync::Arc;

pub struct AnimPathsPlugin;

impl Plugin for AnimPathsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, apply.before(crate::locomotion::drive));
    }
}

/// Per actor: the paths applied and the shared standing set to go back to.
type Applied = HashMap<u32, (Vec<String>, Arc<crate::locomotion::ClipSet>)>;

fn apply(script: Option<Res<ScriptState>>, parts: Option<ResMut<ActorParts>>, mut models: ModelCtx, mut actors: Query<(&ActorRig, &mut Locomotion)>, mut applied: Local<Applied>) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else { return };
    let game = &rt.game;
    if game.anim_paths.actor.is_empty() && game.anim_folders.active.is_empty() && applied.is_empty() {
        return;
    }
    let Some(mut parts) = parts else { return };
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for (rig, mut lo) in &mut actors {
        let paths = game.anim_overrides(FormId(rig.reference), false);
        let now = applied.get(&rig.reference).map(|(p, _)| p.clone()).unwrap_or_default();
        if paths == now {
            continue;
        }
        let shared = match applied.get(&rig.reference) {
            Some((_, s)) => s.clone(),
            None => lo.stand_set(),
        };
        if paths.is_empty() {
            lo.replace_stand(shared);
            applied.remove(&rig.reference);
            continue;
        }
        let (set, n) = shared.with_replacements(&mut ctx, &rig.skel, &paths);
        info!("anim paths: {:08X} plays {n} replaced move(s) from {} file(s)", rig.reference, paths.len());
        lo.replace_stand(Arc::new(set));
        applied.insert(rig.reference, (paths, shared));
    }
}
