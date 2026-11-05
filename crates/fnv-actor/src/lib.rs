//! Actor appearance (workstream 5, docs/design/static-actors.md): which skeleton, race
//! body and head parts, hair, head parts and worn armour make up a placed NPC, and the
//! FaceGen coefficients that shape its face. Headless and deterministic: the engine
//! assembles the parts into one entity; this crate only decides what they are.
//!
//! - [`records`]: NPC_, RACE, HAIR, EYES, HDPT, ARMO and LVLN read through the Layer 1
//!   schema
//! - [`resolve`]: placed actor → [`resolve::Appearance`] (parts list, worn armour), placed
//!   creature → [`resolve::CreatureAppearance`] (skeleton, NIFZ models, base scale)
//! - [`facegen`]: `.egm`/`.egt` FaceGen morphs and applying coefficients to them
//! - [`tri`]: `.tri` FaceGen morph targets (mouth shapes, expressions, eyelids, brows)
//! - [`mesh`]: how a part mesh joins the skeleton (skinned by bone name, rigid by `Prn`)
//! - [`idle`]: IDLE records (idle animations) as data: model, links, group, raw conditions
//! - [`idle_play`]: one idle animation playing: its loops, clip time and fades
//! - [`anim_sounds`]: the sounds clips name in text keys, footsteps and armour rustle
//! - [`anims`]: where `.kf` files sit relative to skeletons, and what their tracks bind to
//! - [`locomotion`]: idle, walk, run and turn clips, their root motion, and choosing
//!   and blending between them
//! - [`jump`]: an NPC's body in the air: when its jump loop and landing play, which clip
//! - [`weapon`]: the weapon an actor draws, its stance clips and the draw cycle
//! - [`player_clips`]: which clips the player's body plays under direct control (moving,
//!   sneaking, drawing, aiming and holstering a weapon), and the draw/holster sequence
//! - [`first_person`]: what the player's first-person rig draws (arms and hands)
//! - [`furniture`]: furniture an actor can use: the entry points on its model, the clip
//!   the IDLE tree names for getting on, staying on and getting off, and the phases of a use
//! - [`furniture_retry`]: what an actor does after furniture refused it: wait and ask
//!   again, or not ask again until something has changed
//! - [`head_look`]: head tracking: whom an actor looks at, the limits of the turn and its
//!   easing (an NVForever design feature)
//!
//! Facts and rules with confidence tags: docs/spec-notes/actors.md and (animation data)
//! docs/spec-notes/animation.md.

pub mod anim_sounds;
pub mod anims;
pub mod creator;
pub mod facegen;
pub mod first_person;
pub mod furniture;
pub mod furniture_retry;
pub mod head_look;
pub mod idle;
pub mod idle_play;
pub mod jump;
pub mod locomotion;
pub mod mesh;
pub mod player_clips;
pub mod records;
pub mod resolve;
pub mod tri;
pub mod weapon;

pub use records::ActorForms;
pub use resolve::{Appearance, Change, CreatureAppearance, Look, Part, ResolveError, Resolver};
