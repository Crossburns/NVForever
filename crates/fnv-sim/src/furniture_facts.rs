//! What the AI may ask about furniture and actor assets. The AI (`fnv_ai`) reads no files
//! and `fnv_actor`, which reads them, does not know the AI: the question lives here,
//! below both, so a runner hands the AI its answers without an adapter
//! (docs/spec-notes/furniture.md §9).

use crate::FormId;

/// Whether an actor's skeleton has clips to use a piece of furniture with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Supported,
    /// A creature whose skeleton is known, and the tree gives it nothing to use this
    /// furniture with. Never said of a person.
    Unsupported,
    /// The skeleton or the assets could not be read: not evidence of missing clips.
    Unknown,
}

/// What the actor asking is, by its own record (its base, or a leveled base's pick),
/// whatever template its model comes from. `Person`: an `NPC_`. `Creature`: a `CREA`.
/// `Unresolved`: neither could be found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    Person,
    Creature,
    Unresolved,
}

/// Facts that do not change during a run. Answers are looked up, never read from files
/// while choosing.
pub trait FurnitureFacts: Send + Sync {
    /// The marker IDs of the model of a furniture base that its marker flags enable.
    /// `None`: not known (no model, or it could not be read).
    fn marker_ids(&self, furniture_base: FormId) -> Option<&[u8]>;

    /// Whether an actor of this kind on the skeleton `skeleton` (its `MODL`, as written)
    /// fits a furniture base. An empty `skeleton` has not been resolved.
    fn fit(&self, kind: ActorKind, skeleton: &str, furniture_base: FormId) -> Fit;
}
