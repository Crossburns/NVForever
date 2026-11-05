//! What a placed reference contributes to the physics world: the collision objects of
//! its base object's model and of a CollisionMarker primitive. One rule shared by
//! `fnvtool phys` and the game (`engine/nvf-play`), so the two never diverge.
//!
//! Whether the reference is in the world at all (deleted, disabled, taken) is the
//! caller's decision: the tools read the plugins' flags, the game asks its simulation.
//! Models are read by the caller too ([`ModelCollision`]); fnv-phys never reads NIF
//! blocks (docs/design/layer4.md §7).

use crate::primitive::{primitive, primitive_object, BoundsMeaning, COLLISION_MARKER};
use fnv_collide::CollisionObject;
use fnv_esm::{LoadOrder, Schema};
use fnv_world::placed::base_object;
use fnv_world::Placement;
use std::sync::Arc;

/// Reads the collision objects of a model (a path relative to the data folder, as
/// `BaseObject::world_model_path` gives it), typically through `fnv_collide::nif`.
pub trait ModelCollision {
    /// The model's solid collision objects (empty when it has none), or why it could not
    /// be read.
    fn objects(&mut self, path: &str) -> Result<Arc<Vec<CollisionObject>>, String>;

    /// The model's water volumes: its phantom shapes on the Water layer (physics.md §6.6),
    /// known after [`ModelCollision::objects`] read the model. None by default.
    fn water(&mut self, _path: &str) -> Arc<Vec<CollisionObject>> {
        Arc::new(Vec::new())
    }
}

/// The collision of one placed reference.
#[derive(Debug, Clone, Default)]
pub struct RefCollision {
    /// Signature of the base object, when it could be read.
    pub base_sig: Option<[u8; 4]>,
    /// Collision objects in the reference's frame: the CollisionMarker primitive, if any,
    /// then the model's objects. Empty when the reference has no collision.
    pub objects: Vec<CollisionObject>,
    /// Water volumes of the model in the reference's frame (placeable water,
    /// [`crate::PhysWorld::add_placed_water`]). Empty for most references.
    pub water: Vec<CollisionObject>,
}

/// The collision objects of a placed reference: a CollisionMarker carries its own box or
/// sphere (physics.md §4.1); otherwise, and in addition, the base object's model, unless
/// it is a SpeedTree (`.spt` files hold no Havok blocks, physics.md §4). With no
/// `models`, model collision is left out.
///
/// A placed actor (base NPC_ or CREA) contributes nothing: its model is its skeleton,
/// whose bodies are the actor's own and move with its bones (physics.md §4).
///
/// `tally` is called once for each outcome, with the names of the `refs.counts` of
/// `fnv.phys.cell/1` (physics.md §7): `no_base`, `collision_primitive`, `bad_primitive`,
/// `actor`, `no_model`, `speedtree`, `awaiting_collision`, `model_without_collision`,
/// `model_unreadable`, `primitive_model` (a primitive whose base names a model: the
/// model is left out, level-design.md §4.1), `open_door_moving_parts` (an open-by-default door whose keyframed
/// parts were left out), `water_volume` (a model with water volumes; then not also
/// `model_without_collision`).
pub fn ref_collision(
    lo: &LoadOrder,
    schema: &Schema,
    pl: &Placement,
    models: Option<&mut dyn ModelCollision>,
    tally: &mut dyn FnMut(&'static str),
) -> RefCollision {
    ref_collision_with(lo, schema, pl, models, tally, RefCollisionOptions::default())
}

/// Whether a base object of this record type is an actor (an NPC or a creature).
pub fn is_actor_base(sig: &[u8; 4]) -> bool {
    matches!(sig, b"NPC_" | b"CREA")
}

/// Choices of [`ref_collision_with`] that differ from the shared rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RefCollisionOptions<'a> {
    /// Keep the keyframed parts of a door flagged Open by Default: the caller poses them
    /// from the door's "Open" sequence ([`crate::PhysWorld::pose_ref_object`],
    /// `fnv_collide::anim`). Off in the shared rule, which has no animation and leaves
    /// them out (`open_door_moving_parts`).
    pub posed_doors: bool,
    /// The model to collide with in place of the base object's: a destroyed reference's
    /// destruction-stage model (DSTD's DMDL, a car's hulk; destruction-stages.md §4), as
    /// the record stores it. None in the shared rule, which has no game state.
    pub model: Option<&'a str>,
}

/// [`ref_collision`] with `options`.
pub fn ref_collision_with(
    lo: &LoadOrder,
    schema: &Schema,
    pl: &Placement,
    models: Option<&mut dyn ModelCollision>,
    tally: &mut dyn FnMut(&'static str),
    options: RefCollisionOptions<'_>,
) -> RefCollision {
    let Some(base) = pl.base.and_then(|b| base_object(lo, schema, b)) else {
        tally("no_base");
        return RefCollision::default();
    };
    let mut out = RefCollision { base_sig: Some(base.sig), objects: Vec::new(), water: Vec::new() };
    if pl.base == Some(COLLISION_MARKER) {
        // Box and Portal Box markers alike: a run into a Portal Box marker stopped at the
        // wall [observed: L4.phys.walk.portal_wall], although a drop onto the 2-unit top
        // of another fell through (physics.md §4.2, open).
        match primitive(lo, schema, pl.form_id).and_then(|p| primitive_object(&p, BoundsMeaning::HalfExtents)) {
            Some(o) => {
                out.objects.push(o);
                tally("collision_primitive");
            }
            None => tally("bad_primitive"),
        }
    }
    if is_actor_base(&base.sig) {
        // The skeleton's bodies are keyframed and on the Biped layer [observed:
        // `fnvtool phys cell --list-refs`]: added at the placement, they stayed in the
        // way where the actor was placed, wherever it had walked to since.
        tally("actor");
        return out;
    }
    match options.model.map_or_else(|| base.world_model_path(), fnv_world::placed::archive_path) {
        None if out.objects.is_empty() => tally("no_model"),
        None => {}
        // A trigger copied from an activator with a model keeps the model in its record;
        // the game shows and collides with only the primitive (level-design.md §4.1).
        Some(_) if !pl.draws_base_model() => tally("primitive_model"),
        Some(m) if m.ends_with(".spt") => tally("speedtree"),
        Some(m) => match models {
            None => tally("awaiting_collision"),
            Some(ms) => {
                let read = ms.objects(&m);
                if read.is_ok() {
                    out.water = ms.water(&m).to_vec();
                    if !out.water.is_empty() {
                        tally("water_volume");
                    }
                }
                match read {
                    Ok(o) if o.is_empty() => {
                        if out.water.is_empty() {
                            tally("model_without_collision");
                        }
                    }
                    Ok(o) if &base.sig == b"DOOR" && pl.open_by_default && !options.posed_doors => {
                        // An open door's moving part is out of the doorway: a player dropped
                        // on a closed-looking gate flagged Open by Default fell straight
                        // through it [observed: B11, gate 0x0010757E]. Its open pose comes
                        // from the door's animation, which is not read; the keyframed parts
                        // are left out [hypothesis], the static ones (posts, frame) kept.
                        let before = out.objects.len();
                        out.objects.extend(o.iter().filter(|c| c.motion != fnv_collide::Motion::Keyframed).cloned());
                        if out.objects.len() - before < o.len() {
                            tally("open_door_moving_parts");
                        }
                    }
                    Ok(o) => out.objects.extend(o.iter().cloned()),
                    Err(_) => tally("model_unreadable"),
                }
            }
        },
    }
    out
}
