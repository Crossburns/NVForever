//! Layer 4 core simulation for the Fallout: New Vegas reimplementation: headless,
//! deterministic, and free of rendering or engine dependencies (spec section 7:
//! "deterministic, headless-testable simulation core").
//!
//! - [`form`]: global FormIDs
//! - [`forms`]: base forms the simulation needs (items, containers, doors, actors,
//!   leveled lists, globals), read through the Layer 1 schema
//! - [`refs`]: placed references with their extra data
//! - [`furniture_facts`]: what the AI may ask about furniture and actor assets
//! - [`settings`]: game settings (GMST); [`clock`]: game time in the time globals
//! - [`inventory`], [`leveled`]: item stacks and leveled-list resolution
//! - [`world`]: runtime state as a delta over the plugins, activation, snapshots
//! - [`actions`]: scripted action sequences and traces (the parity scenario format)
//! - [`sample`]: leveled-list outcome histograms (distribution scenarios)
//! - [`zones`]: encounter zones and the level a reference reads leveled lists at
//!
//! Design and status: `docs/design/layer4.md`; behaviour notes with confidence tags:
//! `docs/spec-notes/simulation.md`.

pub mod actions;
pub mod clock;
pub mod extract;
pub mod form;
pub mod forms;
pub mod furniture_facts;
pub mod inventory;
pub mod leveled;
pub mod refs;
pub mod sample;
pub mod settings;
pub mod world;
pub mod zones;

pub use form::FormId;
pub use forms::FormStore;
pub use inventory::{Inventory, Pick};
pub use refs::RefTable;
pub use world::{SimConfig, Snapshot, World};
