//! NVForever's own save files (docs/design/save-load.md §4). Not the original's `.fos`:
//! the game neither reads nor writes those; [`fos`] reads them for the extender lab only.
//!
//! A file is
//!
//! ```text
//! magic   "NVFSAVE\0"            8 bytes
//! version u32 LE                 container version (CONTAINER_VERSION)
//! header  u32 LE length + JSON   SaveHeader: readable without the body (the load list)
//! shot    u32 LE length + bytes  thumbnail (PNG), may be empty
//! body    u32 LE length + JSON   the game and engine snapshots (the caller's type)
//! ```
//!
//! Writes are atomic: the file is written beside its place as `.tmp`, flushed, and
//! renamed over the old one; a rolling slot (quick, auto) first keeps the previous file
//! as `.bak`. Every path goes through `fnv_install::guard_write`, so a save never lands in
//! the game install or the original's `My Games` folder.
//!
//! The body is JSON for now (inspectable, what the snapshots' tests already use); the
//! container version is bumped if it changes encoding.

pub mod fos;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"NVFSAVE\0";
pub const CONTAINER_VERSION: u32 = 1;
pub const HEADER_FORMAT: &str = "nvf.save.header/1";
pub const EXTENSION: &str = "nvfsave";
/// The rolling slots' names.
pub const QUICK_NAME: &str = "quicksave";
pub const AUTO_NAME: &str = "autosave";
/// SystemSave's one slot (the original labels it SYS [documented: GECK wiki SystemSave]).
pub const SYSTEM_NAME: &str = "systemsave";

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0}: not an NVForever save")]
    NotASave(PathBuf),
    #[error("{0}: container version {1}, this NVForever reads {CONTAINER_VERSION}")]
    Version(PathBuf, u32),
    #[error("{0}: {1}")]
    Json(PathBuf, serde_json::Error),
    #[error("{0}: truncated")]
    Truncated(PathBuf),
    #[error(transparent)]
    Denied(#[from] fnv_install::InstallError),
    #[error("bad save name {0:?}: letters, digits, spaces, '-' and '_' only")]
    BadName(String),
}

/// How a save was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SaveKind {
    #[default]
    Manual,
    Quick,
    Auto,
    /// A script's SystemSave: one rolling slot, offered for loading only.
    System,
}

/// One plugin of the load order a save was made with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEntry {
    pub name: String,
    pub size: u64,
}

/// Where the player stood, for starting the world there before the body is applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Place {
    /// The worldspace (FormID) outdoors; the worldspace last outside when indoors.
    pub world: Option<u32>,
    /// The interior cell, when inside.
    pub interior: Option<u32>,
    /// The player's feet, game space.
    pub feet: [f32; 3],
    /// Compass heading and view pitch, degrees.
    pub heading_deg: f32,
    pub pitch_deg: f32,
}

/// One of players 2–4 of a co-op save, as the load list shows it (the game's own player is
/// [`SaveHeader::player_name`] and [`SaveHeader::level`]); their state is in the body
/// (docs/design/coop-save.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoopEntry {
    /// The player's number, 2–4.
    pub slot: u8,
    pub name: String,
    pub level: u32,
}

/// What the load list shows, and the checks before loading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveHeader {
    pub format: String,
    pub kind: SaveKind,
    pub nvforever_version: String,
    /// Real time saved, seconds since the Unix epoch.
    pub saved_unix: u64,
    /// Game time played, seconds (the script clock).
    pub play_seconds: f64,
    /// Game date and hour (the GameYear/Month/Day/Hour globals).
    pub game_year: u32,
    pub game_month: u32,
    pub game_day: u32,
    pub game_hour: f32,
    pub player_name: String,
    pub level: u32,
    pub location: String,
    pub place: Place,
    /// The sky's weather (Editor ID), to start the sky with it.
    #[serde(default)]
    pub weather: Option<String>,
    pub plugins: Vec<PluginEntry>,
    /// Players 2–4 of a co-op save; empty (and not written) for one player.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coop: Vec<CoopEntry>,
}

impl SaveHeader {
    pub fn new(kind: SaveKind) -> SaveHeader {
        SaveHeader {
            format: HEADER_FORMAT.into(),
            kind,
            nvforever_version: env!("CARGO_PKG_VERSION").into(),
            saved_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            play_seconds: 0.0,
            game_year: 0,
            game_month: 0,
            game_day: 0,
            game_hour: 0.0,
            player_name: String::new(),
            level: 1,
            location: String::new(),
            place: Place::default(),
            weather: None,
            plugins: Vec::new(),
            coop: Vec::new(),
        }
    }
}

/// A whole save, as read.
#[derive(Debug, Clone)]
pub struct SaveFile {
    pub header: SaveHeader,
    pub shot: Vec<u8>,
    pub body: Vec<u8>,
}

/// A save name as the player types it: kept to characters every file system takes.
pub fn check_name(name: &str) -> Result<&str, SaveError> {
    let n = name;
    let ok = !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')) && n.trim() == n;
    if ok {
        Ok(n)
    } else {
        Err(SaveError::BadName(name.to_string()))
    }
}

/// The file of save `name` in `dir`.
pub fn path_of(dir: &Path, name: &str) -> Result<PathBuf, SaveError> {
    Ok(dir.join(format!("{}.{EXTENSION}", check_name(name)?)))
}

fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// The file's bytes.
pub fn encode(header: &SaveHeader, shot: &[u8], body: &[u8]) -> Vec<u8> {
    let head = serde_json::to_vec(header).expect("a header always serialises");
    let mut out = Vec::with_capacity(8 + 4 + 12 + head.len() + shot.len() + body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    put(&mut out, &head);
    put(&mut out, shot);
    put(&mut out, body);
    out
}

/// Write a save atomically; `keep_backup` keeps the file it replaces as `.bak`.
pub fn write(path: &Path, header: &SaveHeader, shot: &[u8], body: &[u8], keep_backup: bool) -> Result<(), SaveError> {
    fnv_install::guard_write(path, None)?;
    let io = |e| SaveError::Io(path.to_path_buf(), e);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension(format!("{EXTENSION}.tmp"));
    {
        let mut f = fs::File::create(&tmp).map_err(io)?;
        f.write_all(&encode(header, shot, body)).map_err(io)?;
        f.sync_all().map_err(io)?;
    }
    if keep_backup && path.exists() {
        let bak = path.with_extension(format!("{EXTENSION}.bak"));
        let _ = fs::remove_file(&bak);
        fs::rename(path, &bak).map_err(io)?;
    }
    fs::rename(&tmp, path).map_err(io)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn block(&mut self) -> Option<&'a [u8]> {
        let n = u32::from_le_bytes(self.b.get(self.at..self.at + 4)?.try_into().ok()?) as usize;
        let out = self.b.get(self.at + 4..self.at + 4 + n)?;
        self.at += 4 + n;
        Some(out)
    }
}

/// Read a save from its bytes; `with_body: false` stops after the thumbnail.
pub fn decode(path: &Path, b: &[u8], with_body: bool) -> Result<SaveFile, SaveError> {
    if b.len() < 12 || &b[..8] != MAGIC {
        return Err(SaveError::NotASave(path.to_path_buf()));
    }
    let version = u32::from_le_bytes(b[8..12].try_into().expect("4 bytes"));
    if version != CONTAINER_VERSION {
        return Err(SaveError::Version(path.to_path_buf(), version));
    }
    let mut r = Reader { b, at: 12 };
    let trunc = || SaveError::Truncated(path.to_path_buf());
    let header: SaveHeader = serde_json::from_slice(r.block().ok_or_else(trunc)?).map_err(|e| SaveError::Json(path.to_path_buf(), e))?;
    let shot = r.block().ok_or_else(trunc)?.to_vec();
    let body = if with_body { r.block().ok_or_else(trunc)?.to_vec() } else { Vec::new() };
    Ok(SaveFile { header, shot, body })
}

/// Read a whole save.
pub fn read(path: &Path) -> Result<SaveFile, SaveError> {
    let b = fs::read(path).map_err(|e| SaveError::Io(path.to_path_buf(), e))?;
    decode(path, &b, true)
}

/// Read only the header and thumbnail (the body is not read from disk).
pub fn read_header(path: &Path) -> Result<SaveFile, SaveError> {
    let io = |e| SaveError::Io(path.to_path_buf(), e);
    let mut f = fs::File::open(path).map_err(io)?;
    let len = f.metadata().map_err(io)?.len();
    let mut head = [0u8; 16];
    // A file too short, not a save or of another version is refused before any size in
    // it is trusted; a size past the end of the file is a truncated save (nothing is
    // allocated from it).
    if len < 16 || f.read_exact(&mut head).is_err() || &head[..8] != MAGIC {
        return Err(SaveError::NotASave(path.to_path_buf()));
    }
    let version = u32::from_le_bytes(head[8..12].try_into().expect("4 bytes"));
    if version != CONTAINER_VERSION {
        return Err(SaveError::Version(path.to_path_buf(), version));
    }
    let trunc = || SaveError::Truncated(path.to_path_buf());
    let hlen = u32::from_le_bytes(head[12..16].try_into().expect("4 bytes")) as u64;
    if 16 + hlen + 4 > len {
        return Err(trunc());
    }
    let hlen = hlen as usize;
    let mut rest = vec![0u8; hlen + 4];
    f.read_exact(&mut rest).map_err(io)?;
    let slen = u32::from_le_bytes(rest[hlen..hlen + 4].try_into().expect("4 bytes")) as u64;
    if 16 + hlen as u64 + 4 + slen > len {
        return Err(trunc());
    }
    let mut shot = vec![0u8; slen as usize];
    f.read_exact(&mut shot).map_err(io)?;
    let mut b = head.to_vec();
    b.extend_from_slice(&rest);
    b.extend_from_slice(&shot);
    decode(path, &b, false)
}

/// The saves in `dir`, newest first (files that are not saves are left out).
pub fn list(dir: &Path) -> Vec<(PathBuf, SaveHeader)> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(PathBuf, SaveHeader)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == EXTENSION))
        .filter_map(|p| read_header(&p).ok().map(|s| (p, s.header)))
        .collect();
    v.sort_by(|a, b| b.1.saved_unix.cmp(&a.1.saved_unix).then_with(|| a.0.cmp(&b.0)));
    v
}

/// How the load order a save was made with compares with the current one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginCheck {
    Same,
    /// Plugins were added after all of the save's, which keep their places: every form ID
    /// in the save means what it meant, so it loads as it is. The added plugins' objects
    /// appear, as a save holds only the differences from the plugins (save-load.md §1).
    Appended { added: Vec<String> },
    /// The plugins differ: those missing now, those added, and whether the common ones
    /// changed order. Form IDs are load-order IDs: the save loads through a
    /// [`PluginRemap`] (docs/design/save-load.md §4.1).
    Differs { missing: Vec<String>, added: Vec<String>, reordered: bool },
}

pub fn check_plugins(saved: &[PluginEntry], current: &[PluginEntry]) -> PluginCheck {
    let names = |v: &[PluginEntry]| v.iter().map(|p| p.name.to_ascii_lowercase()).collect::<Vec<_>>();
    let (s, c) = (names(saved), names(current));
    if s == c {
        return PluginCheck::Same;
    }
    if c.starts_with(&s) {
        return PluginCheck::Appended { added: current[s.len()..].iter().map(|p| p.name.clone()).collect() };
    }
    let missing = saved.iter().filter(|p| !c.contains(&p.name.to_ascii_lowercase())).map(|p| p.name.clone()).collect();
    let added = current.iter().filter(|p| !s.contains(&p.name.to_ascii_lowercase())).map(|p| p.name.clone()).collect();
    let common_s: Vec<&String> = s.iter().filter(|n| c.contains(n)).collect();
    let common_c: Vec<&String> = c.iter().filter(|n| s.contains(n)).collect();
    PluginCheck::Differs { missing, added, reordered: common_s != common_c }
}

/// The load-order slot that form IDs of plugins no longer loaded are moved to
/// (docs/design/save-load.md §4.1). A load order of 255 plugins uses it (indices 0x00-0xFE,
/// [documented: GECK wiki FormID: "up to 255 data files"]); a save with missing plugins is
/// then refused rather than let its gone forms alias a real one [hypothesis: our choice].
pub const GONE_INDEX: u8 = 0xFE;
/// Created forms' slot, never remapped (`fnv_sim::FormId::FIRST_DYNAMIC`).
const DYNAMIC_INDEX: u8 = 0xFF;

/// What a saved form ID becomes in the current load order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remapped {
    /// Its plugin is loaded (perhaps at another place): the ID there.
    Kept(u32),
    /// Its plugin is not loaded: the ID moved to the [`GONE_INDEX`] slot, low bytes kept.
    Gone(u32),
}

/// Where each plugin of a save's load order is in the current one, by file name (case
/// does not matter, as Windows file names): the mod-index byte of every saved form ID is
/// rewritten from the save's list to the current one [documented: GECK wiki FormID, "the
/// first two digits ... correspond with the position of the data file ... in your load
/// order"; GECK wiki GetFormIDString: "FormIDs may change on game load, dependent on load
/// order"; observed: the original's saves hold their plugin list and resolve form IDs by
/// position in it, docs/spec-notes/save-files.md §4, §6.3].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRemap {
    /// Per saved index: the current index, or `None` for a plugin no longer loaded.
    map: Vec<Option<u8>>,
    /// The save's plugin names, by saved index.
    saved: Vec<String>,
}

impl PluginRemap {
    pub fn new(saved: &[PluginEntry], current: &[PluginEntry]) -> PluginRemap {
        let names: Vec<String> = current.iter().map(|p| p.name.clone()).collect();
        PluginRemap::from_names(saved, &names)
    }

    /// The same from the current plugins' file names.
    pub fn from_names(saved: &[PluginEntry], current: &[String]) -> PluginRemap {
        let map = saved
            .iter()
            .map(|p| current.iter().position(|c| c.eq_ignore_ascii_case(&p.name)).and_then(|i| u8::try_from(i).ok()).filter(|&i| i < DYNAMIC_INDEX))
            .collect();
        PluginRemap { map, saved: saved.iter().map(|p| p.name.clone()).collect() }
    }

    /// Every saved form ID means what it meant (same list, or plugins only appended).
    pub fn is_identity(&self) -> bool {
        self.map.iter().enumerate().all(|(i, m)| *m == Some(i as u8))
    }

    /// The save's plugins no longer loaded.
    pub fn missing(&self) -> Vec<String> {
        self.map.iter().zip(&self.saved).filter(|(m, _)| m.is_none()).map(|(_, n)| n.clone()).collect()
    }

    /// The saved plugins that moved: (name, saved index, current index).
    pub fn moved(&self) -> Vec<(String, u8, u8)> {
        self.map.iter().enumerate().zip(&self.saved).filter_map(|((i, m), n)| m.filter(|&c| c != i as u8).map(|c| (n.clone(), i as u8, c))).collect()
    }

    /// The saved plugin of an ID's index, if the save had one there.
    pub fn plugin_of(&self, id: u32) -> Option<&str> {
        self.saved.get((id >> 24) as usize).map(String::as_str)
    }

    /// One saved form ID in the current load order. Created forms (0xFF) and 0 ("none")
    /// stay; an ID whose index the save's list does not have (one already gone) is gone.
    pub fn form(&self, id: u32) -> Remapped {
        let index = (id >> 24) as u8;
        if id == 0 || index == DYNAMIC_INDEX {
            return Remapped::Kept(id);
        }
        match self.map.get(index as usize).copied().flatten() {
            Some(c) => Remapped::Kept(((c as u32) << 24) | (id & 0x00FF_FFFF)),
            None => Remapped::Gone(((GONE_INDEX as u32) << 24) | (id & 0x00FF_FFFF)),
        }
    }

    /// The current ID of a saved one, `None` when its plugin is gone.
    pub fn map(&self, id: u32) -> Option<u32> {
        match self.form(id) {
            Remapped::Kept(v) => Some(v),
            Remapped::Gone(_) => None,
        }
    }
}

/// An ID in the [`GONE_INDEX`] slot: one of a plugin that was not loaded with the save.
pub fn is_gone(id: u32) -> bool {
    (id >> 24) as u8 == GONE_INDEX
}

/// What [`remap_json`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemapReport {
    /// Form IDs rewritten to another index.
    pub moved: usize,
    /// Entries removed because a plugin is gone, by that plugin: map entries keyed by one
    /// of its forms, list entries that are one, pairs and tuples holding one.
    pub dropped: BTreeMap<String, usize>,
    /// Fields left holding a gone ID (moved to the [`GONE_INDEX`] slot), by plugin; the
    /// game layer drops what holds them where it can (`GameSnapshot::drop_gone`).
    pub gone: BTreeMap<String, usize>,
}

impl RemapReport {
    pub fn dropped_total(&self) -> usize {
        self.dropped.values().sum()
    }
    pub fn gone_total(&self) -> usize {
        self.gone.values().sum()
    }
}

/// A form ID as `fnv_sim::FormId` writes it: `0x` and eight upper-case hex digits.
fn form_text(s: &str) -> Option<u32> {
    let hex = s.strip_prefix("0x")?;
    let ok = hex.len() == 8 && hex.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
    if ok {
        u32::from_str_radix(hex, 16).ok()
    } else {
        None
    }
}

/// A script variable holding a reference (`fnv_script::Value::Ref`, written `{"ref": n}`).
fn script_ref(o: &serde_json::Map<String, serde_json::Value>) -> Option<u32> {
    if o.len() != 1 {
        return None;
    }
    o.get("ref")?.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// A string that is a form ID of a gone plugin.
fn gone_text(v: &serde_json::Value, r: &PluginRemap) -> Option<u32> {
    let id = form_text(v.as_str()?)?;
    matches!(r.form(id), Remapped::Gone(_)).then_some(id)
}

/// A list (a tuple, as serde writes one) holding a gone form ID, directly or in lists
/// within it.
fn tuple_with_gone(v: &serde_json::Value, r: &PluginRemap) -> Option<u32> {
    v.as_array()?.iter().find_map(|e| gone_text(e, r).or_else(|| tuple_with_gone(e, r)))
}

/// Rewrite every form ID of a save body (its JSON) from the save's load order to the
/// current one (docs/design/save-load.md §4.1). Form IDs are recognised by how the
/// snapshots write them: strings `0x%08X` (values and map keys) and script variables'
/// `{"ref": n}`. Where a gone plugin's form is a map key, a list entry, or in a pair or
/// tuple that is a list entry, that entry is removed; anywhere else (a field) the ID is
/// moved to the [`GONE_INDEX`] slot, where nothing loads. Raw numbers elsewhere are not
/// form IDs to this walk: their owners remap them ([`PluginRemap::map`]).
pub fn remap_json(v: &mut serde_json::Value, r: &PluginRemap) -> RemapReport {
    let mut report = RemapReport::default();
    walk(v, r, &mut report);
    report
}

fn count(map: &mut BTreeMap<String, usize>, r: &PluginRemap, id: u32) {
    *map.entry(r.plugin_of(id).unwrap_or("?").to_string()).or_default() += 1;
}

/// One form ID's new value, counted.
fn remap_one(id: u32, r: &PluginRemap, report: &mut RemapReport) -> u32 {
    match r.form(id) {
        Remapped::Kept(n) => {
            if n != id {
                report.moved += 1;
            }
            n
        }
        Remapped::Gone(n) => {
            count(&mut report.gone, r, id);
            n
        }
    }
}

fn walk(v: &mut serde_json::Value, r: &PluginRemap, report: &mut RemapReport) {
    use serde_json::Value;
    match v {
        Value::String(s) => {
            if let Some(id) = form_text(s) {
                let new = remap_one(id, r, report);
                if new != id {
                    *s = format!("0x{new:08X}");
                }
            }
        }
        Value::Array(items) => {
            items.retain(|e| match gone_text(e, r).or_else(|| tuple_with_gone(e, r)) {
                Some(id) => {
                    count(&mut report.dropped, r, id);
                    false
                }
                None => true,
            });
            for e in items {
                walk(e, r, report);
            }
        }
        Value::Object(o) => {
            if let Some(id) = script_ref(o) {
                let new = remap_one(id, r, report);
                if new != id {
                    o.insert("ref".into(), Value::from(new));
                }
                return;
            }
            let entries = std::mem::take(o);
            for (k, mut e) in entries {
                let key = match form_text(&k) {
                    Some(id) => match r.form(id) {
                        Remapped::Kept(n) if n == id => k,
                        Remapped::Kept(n) => {
                            report.moved += 1;
                            format!("0x{n:08X}")
                        }
                        Remapped::Gone(_) => {
                            count(&mut report.dropped, r, id);
                            continue;
                        }
                    },
                    None => k,
                };
                walk(&mut e, r, report);
                o.insert(key, e);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> SaveHeader {
        let mut h = SaveHeader::new(SaveKind::Quick);
        // Fixed, so two calls a clock second apart still compare equal.
        h.saved_unix = 1_700_000_000;
        h.player_name = "Courier".into();
        h.place = Place { world: Some(0xDA726), interior: None, feet: [1.0, 2.0, 3.0], heading_deg: 90.0, pitch_deg: -5.0 };
        h.plugins = vec![PluginEntry { name: "FalloutNV.esm".into(), size: 10 }];
        h
    }

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fnv-save-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_save_reads_back_as_written() {
        let d = dir("roundtrip");
        let p = path_of(&d, "My save 1").unwrap();
        write(&p, &header(), b"png", b"{\"body\":1}", false).unwrap();
        let s = read(&p).unwrap();
        assert_eq!(s.header, header());
        assert_eq!(s.shot, b"png");
        assert_eq!(s.body, b"{\"body\":1}");
        let h = read_header(&p).unwrap();
        assert_eq!(h.header, header());
        assert!(h.body.is_empty());
        assert_eq!(list(&d).len(), 1);
        let _ = fs::remove_dir_all(&d);
    }

    /// Players 2–4 of a co-op save read back; a one-player header writes no `coop` and
    /// one written before the field existed reads as one player.
    #[test]
    fn coop_players_in_the_header() {
        let mut h = header();
        assert!(!serde_json::to_string(&h).unwrap().contains("coop"));
        h.coop = vec![CoopEntry { slot: 2, name: "Boone".into(), level: 3 }, CoopEntry { slot: 4, name: "Cass".into(), level: 7 }];
        let d = dir("coop");
        let p = path_of(&d, "coop").unwrap();
        write(&p, &h, b"", b"{}", false).unwrap();
        assert_eq!(read_header(&p).unwrap().header, h);
        let mut old = serde_json::to_value(&h).unwrap();
        old.as_object_mut().unwrap().remove("coop");
        assert!(serde_json::from_value::<SaveHeader>(old).unwrap().coop.is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    /// A header whose sizes run past the end of the file, or a file that is not a save,
    /// is refused before anything is allocated from those sizes.
    #[test]
    fn read_header_trusts_no_size_past_the_file() {
        let d = dir("sizes");
        fs::create_dir_all(&d).unwrap();
        let p = d.join("bad.nvfsave");
        let mut b = MAGIC.to_vec();
        b.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&p, &b).unwrap();
        assert!(matches!(read_header(&p), Err(SaveError::Truncated(_))));
        let mut b = MAGIC.to_vec();
        b.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(b"{}");
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&p, &b).unwrap();
        assert!(matches!(read_header(&p), Err(SaveError::Truncated(_))));
        fs::write(&p, b"not a save at all, but long enough").unwrap();
        assert!(matches!(read_header(&p), Err(SaveError::NotASave(_))));
        fs::write(&p, b"short").unwrap();
        assert!(matches!(read_header(&p), Err(SaveError::NotASave(_))));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_rolling_slot_keeps_the_previous_file_and_no_tmp_is_left() {
        let d = dir("bak");
        let p = path_of(&d, QUICK_NAME).unwrap();
        write(&p, &header(), b"", b"1", true).unwrap();
        write(&p, &header(), b"", b"2", true).unwrap();
        assert_eq!(read(&p).unwrap().body, b"2");
        assert_eq!(read(&p.with_extension("nvfsave.bak")).unwrap().body, b"1");
        assert!(!p.with_extension("nvfsave.tmp").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn other_files_and_versions_are_refused() {
        let p = Path::new("x.nvfsave");
        assert!(matches!(decode(p, b"FOSAVEGAME..", true), Err(SaveError::NotASave(_))));
        let mut b = encode(&header(), b"", b"{}");
        b[8] = 9;
        assert!(matches!(decode(p, &b, true), Err(SaveError::Version(_, 9))));
        let b = encode(&header(), b"", b"{}");
        assert!(matches!(decode(p, &b[..b.len() - 1], true), Err(SaveError::Truncated(_))));
    }

    #[test]
    fn names_are_kept_to_safe_characters() {
        assert!(check_name("Before Primm_2-b").is_ok());
        for bad in ["", "../x", "a/b", "c:", " lead", "x".repeat(65).as_str()] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn plugin_lists_are_compared_by_name() {
        let e = |n: &str| PluginEntry { name: n.into(), size: 0 };
        let a = vec![e("FalloutNV.esm"), e("DeadMoney.esm")];
        assert_eq!(check_plugins(&a, &[e("falloutnv.esm"), e("DeadMoney.esm")]), PluginCheck::Same);
        assert_eq!(
            check_plugins(&a, &[e("DeadMoney.esm"), e("FalloutNV.esm"), e("New.esp")]),
            PluginCheck::Differs { missing: vec![], added: vec!["New.esp".into()], reordered: true }
        );
        assert_eq!(check_plugins(&a, &[e("FalloutNV.esm")]), PluginCheck::Differs { missing: vec!["DeadMoney.esm".into()], added: vec![], reordered: false });
        // Added after the saved ones: loads; added before one of them: does not.
        assert_eq!(
            check_plugins(&a, &[e("FalloutNV.esm"), e("deadmoney.esm"), e("CM4A1.esp")]),
            PluginCheck::Appended { added: vec!["CM4A1.esp".into()] }
        );
        assert_eq!(
            check_plugins(&a, &[e("FalloutNV.esm"), e("New.esm"), e("DeadMoney.esm")]),
            PluginCheck::Differs { missing: vec![], added: vec!["New.esm".into()], reordered: false }
        );
    }
    fn plugins(names: &[&str]) -> Vec<PluginEntry> {
        names.iter().map(|n| PluginEntry { name: n.to_string(), size: 0 }).collect()
    }

    #[test]
    fn form_ids_follow_their_plugin_by_name() {
        let saved = plugins(&["FalloutNV.esm", "A.esm", "B.esp", "C.esp"]);
        // B moved before A, C gone, D new.
        let r = PluginRemap::new(&saved, &plugins(&["falloutnv.esm", "B.esp", "D.esp", "A.esm"]));
        assert!(!r.is_identity());
        assert_eq!(r.form(0x0001_2345), Remapped::Kept(0x0001_2345));
        assert_eq!(r.form(0x0100_0ABC), Remapped::Kept(0x0300_0ABC));
        assert_eq!(r.form(0x0200_0ABC), Remapped::Kept(0x0100_0ABC));
        assert_eq!(r.form(0x0300_0ABC), Remapped::Gone(0xFE00_0ABC));
        assert_eq!(r.map(0x0300_0ABC), None);
        // Created forms and "none" stay; an index the save never had is gone.
        assert_eq!(r.form(0xFF00_0007), Remapped::Kept(0xFF00_0007));
        assert_eq!(r.form(0), Remapped::Kept(0));
        assert_eq!(r.form(0x0900_0001), Remapped::Gone(0xFE00_0001));
        assert_eq!(r.missing(), vec!["C.esp".to_string()]);
        assert_eq!(r.moved(), vec![("A.esm".to_string(), 1, 3), ("B.esp".to_string(), 2, 1)]);
        // Appended plugins leave every saved ID as it is.
        assert!(PluginRemap::new(&saved, &plugins(&["FalloutNV.esm", "A.esm", "B.esp", "C.esp", "D.esp"])).is_identity());
    }

    #[test]
    fn a_save_body_made_with_one_list_loads_into_another() {
        let saved = plugins(&["FalloutNV.esm", "A.esm", "B.esp"]);
        let r = PluginRemap::new(&saved, &plugins(&["FalloutNV.esm", "B.esp"]));
        // A synthetic body shaped as the snapshots write theirs: refs by ID, an inventory,
        // a set of pairs, a list of IDs, fields, script variables, created forms.
        let mut body = serde_json::json!({
            "game": {
                "sim": {
                    "refs": {
                        "0x00001234": {"base": "0x00000AAA", "cell": "0x0100CCCC", "inventory": {"items": {"0x01000001": 2, "0x02000002": 1, "0x0000000F": 50}}},
                        "0x01005555": {"base": "0x01000AAA", "cell": null},
                        "0x02006666": {"base": "0x02000BBB", "cell": "0x00003333"},
                        "0xFF000001": {"base": "0x02000BBB", "cell": "0x0100CCCC"}
                    },
                    "next_dynamic": 4278190082u32
                },
                "said": [["0x00000014", "0x01000002"], ["0x02000003", "0x00000014"]],
                "form_lists": {"0x00000100": ["0x01000001", "0x02000001", "0x00000002"]},
                "instances": [[{"quest": {"quest": "0x01000777"}}, {"script": "0x01000778", "vars": {"0": {"ref": 0x0200_0009u32}, "1": {"num": 3.0}, "2": {"ref": 0x0100_0009u32}, "3": {"ref": 0}}}]],
                "weather_shown": "0x02000044",
                "note": "0xnot a form",
                "frame": 16777217
            }
        });
        let report = remap_json(&mut body, &r);
        let g = &body["game"];
        let refs = g["sim"]["refs"].as_object().unwrap();
        // A's reference is gone; B's moved from 0x02 to 0x01; the base game's stays.
        assert_eq!(refs.keys().cloned().collect::<Vec<_>>(), vec!["0x00001234", "0x01006666", "0xFF000001"]);
        assert_eq!(refs["0x00001234"]["cell"], "0xFE00CCCC", "a field naming a gone form is moved to the gone slot");
        assert_eq!(refs["0x00001234"]["inventory"]["items"], serde_json::json!({"0x01000002": 1, "0x0000000F": 50}));
        assert_eq!(refs["0x01006666"]["base"], "0x01000BBB");
        assert_eq!(refs["0xFF000001"]["base"], "0x01000BBB");
        assert_eq!(g["sim"]["next_dynamic"], 4278190082u32, "plain numbers are not form IDs");
        assert_eq!(g["frame"], 16777217);
        assert_eq!(g["said"], serde_json::json!([["0x01000003", "0x00000014"]]), "a pair holding a gone form goes whole");
        assert_eq!(g["form_lists"]["0x00000100"], serde_json::json!(["0x01000001", "0x00000002"]));
        assert_eq!(g["weather_shown"], "0x01000044");
        assert_eq!(g["note"], "0xnot a form");
        let vars = &g["instances"][0][1]["vars"];
        assert_eq!(vars["0"], serde_json::json!({"ref": 0x0100_0009u32}));
        assert_eq!(vars["2"], serde_json::json!({"ref": 0xFE00_0009u32}));
        assert_eq!(vars["3"], serde_json::json!({"ref": 0}));
        assert_eq!(g["instances"][0][0]["quest"]["quest"], "0xFE000777");
        // Dropped: the reference 0x01005555, the item 0x01000001, the pair, the list entry.
        assert_eq!(report.dropped, [("A.esm".to_string(), 4)].into());
        // Left as gone IDs: two cells, the quest, the quest script, the script variable.
        assert_eq!(report.gone, [("A.esm".to_string(), 5)].into());
        assert!(report.moved >= 7, "{report:?}");
        assert!(g.to_string().find("0x02").is_none(), "no ID keeps B's old index");
    }

    #[test]
    fn the_same_list_changes_nothing() {
        let saved = plugins(&["FalloutNV.esm", "A.esm"]);
        let r = PluginRemap::new(&saved, &saved);
        assert!(r.is_identity());
        let mut body = serde_json::json!({"x": {"0x01000001": ["0x01000002", {"ref": 0x0100_0003u32}]}});
        let before = body.clone();
        assert_eq!(remap_json(&mut body, &r), RemapReport::default());
        assert_eq!(body, before);
    }
}
