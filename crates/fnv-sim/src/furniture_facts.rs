//! What the AI may ask about furniture and actor assets. The AI (`fnv_ai`) reads no files
//! and `fnv_actor`, which reads them, does not know the AI: the question lives here,
//! below both, so a runner hands the AI its answers without an adapter
//! (docs/spec-notes/furniture.md §9).

use crate::FormId;

/// Whether an actor's skeleton has clips to use a piece of furniture with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Supported,
    /// The skeleton is known and the assets show nothing to use this furniture with.
    Unsupported,
    /// The skeleton or the assets could not be read: not evidence of missing clips.
    Unknown,
}

/// Facts that do not change during a run. Answers are looked up, never read from files
/// while choosing.
pub trait FurnitureFacts: Send + Sync {
    /// The marker IDs of the model of a furniture base that its marker flags enable.
    /// `None`: not known (no model, or it could not be read).
    fn marker_ids(&self, furniture_base: FormId) -> Option<&[u8]>;

    /// Whether the skeleton `skeleton` (an actor's `MODL`, as written) fits a furniture
    /// base. An empty `skeleton` has not been resolved.
    fn fit(&self, skeleton: &str, furniture_base: FormId) -> Fit;
}
