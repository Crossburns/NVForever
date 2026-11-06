//! Dialogue voice files (docs/design/dialogue.md step D3, docs/spec-notes/voice.md):
//! where the sound of a dialogue line's response lives and how long it is. Headless: no
//! audio is decoded or played here (`engine/nvf-play/src/voice.rs` plays the files).
//!
//! - [`path`]: the file name rule (plugin, voice type, quest, topic, FormID, response
//!   number)
//! - [`length`]: seconds of an Ogg Vorbis or WAV file from its headers
//! - [`index`]: the lines of a load order and the names their paths are built from
//! - [`bank`]: the index over the archives, with lengths; `fnv_game`'s line-length hook
//! - [`coverage`]: how many lines of a load order have a sound (`fnvtool voice coverage`)
//! - [`cry`]: one cry of a long generic death file (issue #1539)
//!
//! - [`lip`]: the `.lip` file beside a sound, decoded: the face animation of the
//!   response as weights per channel and frame (docs/spec-notes/lipsync.md)
//! - [`lip_coverage`]: every `.lip` file decoded and counted (`fnvtool voice lip-coverage`)

pub mod bank;
pub mod coverage;
pub mod cry;
pub mod index;
pub mod length;
pub mod lip;
pub mod lip_coverage;
pub mod path;

pub use bank::VoiceBank;
pub use index::{Line, Response, VoiceIndex};
pub use length::{sound_info, SoundInfo};
pub use lip::Lip;
pub use path::{voice_path, voice_stem};
