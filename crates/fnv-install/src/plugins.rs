//! NVForever's own list of plugins, `plugins.txt` in its home (owner decision 2026-10-16:
//! its own list rather than the original game's).
//!
//! The original game keeps the plugins it loads in `%LOCALAPPDATA%\FalloutNV\plugins.txt`,
//! one file name per line [observed: the owner's PC], and orders them by the files'
//! modified dates [hypothesis: modding tools' documentation, not checked]. NVForever never
//! reads or writes that file, so turning a mod on for one game leaves the other as it was.
//! The one exception is a start from Mod Organizer 2, which puts its profile's list at
//! that path for the programs it starts ([`crate::mo2`]).
//!
//! The format here is our own choice:
//!
//! ```text
//! ; comments start with ; or #
//! *FalloutNV.esm
//! *DeadMoney.esm
//! SomeMod.esp
//! ```
//!
//! Every plugin in the game's `Data` folder has a line; a `*` in front means it is loaded,
//! and the lines' order is the load order. A plugin that appears in `Data` later gets a
//! line of its own at the next start: off, except the game's own masters
//! (`FalloutNV.esm`, [`ADDON_MASTERS`]), which are on as they were before this list
//! existed. A new `.esm` goes after the last `.esm` line, so masters stay before the `.esp`
//! files [hypothesis: the extension stands for the header's master flag, true of the
//! game's own files]. Lines are never removed: a plugin no longer in `Data` is skipped
//! with a warning, and comes back on if the file returns.
//!
//! Plugins in NVForever's own mods folder (`<home>\mods\`, [`EngineHome::mods`]) count as
//! present too, and get their lines the same way. A plugin there is opened instead of a
//! plugin of the same name in `Data` ([`plugin_path`]), so a mod can replace a file of the
//! game's without touching it [design].

use crate::{child_ci, guard_write, EngineHome, GameInstall, InstallError, ADDON_MASTERS};
use std::path::{Path, PathBuf};

/// The list's file in the home.
pub const PLUGINS_FILE: &str = "plugins.txt";

const HEADER: &str = "; NVForever's plugins, in load order (top loads first). A * in front of a plugin turns\n\
; it on; take the * away to turn it off, or move a line to change the order. Plugins new\n\
; in the game's Data folder or NVForever's mods folder are added at the next start, turned off.\n\
; NVForever has its own list: the original game's plugins.txt is not read or changed.\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginList {
    pub entries: Vec<PluginEntry>,
}

/// What [`sync`] found: the plugins to load, in order, and what the player should hear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginSync {
    /// The active plugins present in `Data` or the mods folder, in load order;
    /// `FalloutNV.esm` first.
    pub load: Vec<String>,
    /// Those of `load` opened from the mods folder ([`plugin_path`]).
    pub from_mods: Vec<String>,
    /// Plugins given a line this time (new in `Data`, or the list was just made).
    pub added: Vec<String>,
    /// Active lines whose file is in neither `Data` nor the mods folder.
    pub missing: Vec<String>,
    /// `FalloutNV.esm` was off or not first in the file; it is loaded first anyway.
    pub base_forced: bool,
    /// The file was written (made, or lines added).
    pub written: bool,
}

fn is_plugin(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".esm") || l.ends_with(".esp")
}

fn is_esm(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".esm")
}

/// The game's own masters, which a new line turns on.
fn is_game_master(name: &str) -> bool {
    name.eq_ignore_ascii_case("FalloutNV.esm") || ADDON_MASTERS.iter().any(|m| m.eq_ignore_ascii_case(name))
}

/// Where a game master sorts among the others: `FalloutNV.esm`, then [`ADDON_MASTERS`].
fn master_rank(name: &str) -> usize {
    if name.eq_ignore_ascii_case("FalloutNV.esm") {
        return 0;
    }
    ADDON_MASTERS.iter().position(|m| m.eq_ignore_ascii_case(name)).map_or(usize::MAX, |i| i + 1)
}

/// The `.esm` and `.esp` files in `dir`, as named there.
fn plugins_in(dir: &Path) -> std::io::Result<Vec<String>> {
    Ok(std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_plugin(n))
        .collect())
}

fn sort_plugins(names: &mut [String]) {
    names.sort_by_key(|n| (master_rank(n), !is_esm(n), n.to_ascii_lowercase()));
}

/// The `.esm` and `.esp` files in the game's `Data`, as named there: the game's masters in
/// their order, then the other `.esm` files, then the `.esp` files, each by name.
pub fn data_plugins(game: &GameInstall) -> Result<Vec<String>, InstallError> {
    let mut names = plugins_in(&game.data).map_err(|e| InstallError::Io(game.data.clone(), e))?;
    sort_plugins(&mut names);
    Ok(names)
}

/// The plugins of `Data` and of the mods folder `mods` together, in [`data_plugins`]'
/// order; a name in both is listed once, as the mods folder names it. A missing mods
/// folder has no plugins.
pub fn present_plugins(game: &GameInstall, mods: &Path) -> Result<Vec<String>, InstallError> {
    let mut names = match plugins_in(mods) {
        Ok(n) => n,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(InstallError::Io(mods.to_path_buf(), e)),
    };
    for d in plugins_in(&game.data).map_err(|e| InstallError::Io(game.data.clone(), e))? {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&d)) {
            names.push(d);
        }
    }
    sort_plugins(&mut names);
    Ok(names)
}

/// The file to open for plugin `name`: the mods folder's copy when `mods` has one (any
/// case), else `Data`'s (any case), else `data/name`.
pub fn plugin_path(data: &Path, mods: Option<&Path>, name: &str) -> PathBuf {
    mods.and_then(|m| child_ci(m, name)).or_else(|| child_ci(data, name)).unwrap_or_else(|| data.join(name))
}

impl PluginList {
    pub fn parse(text: &str) -> PluginList {
        let mut entries: Vec<PluginEntry> = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
                continue;
            }
            let (active, name) = match t.strip_prefix('*') {
                Some(rest) => (true, rest.trim()),
                None => (false, t),
            };
            // The first line of a name wins.
            if !name.is_empty() && !entries.iter().any(|e| e.name.eq_ignore_ascii_case(name)) {
                entries.push(PluginEntry { name: name.to_string(), active });
            }
        }
        PluginList { entries }
    }

    pub fn to_text(&self) -> String {
        let mut s = HEADER.to_string();
        for e in &self.entries {
            s.push_str(if e.active { "*" } else { "" });
            s.push_str(&e.name);
            s.push('\n');
        }
        s
    }

    fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Gives each of `present` (in [`data_plugins`]' order) without a line one: a game
    /// master on, anything else off; an `.esm` after the last `.esm` line, an `.esp` at
    /// the end. Returns the names added.
    pub fn add_new(&mut self, present: &[String]) -> Vec<String> {
        let mut added = Vec::new();
        for name in present {
            if self.contains(name) {
                continue;
            }
            let entry = PluginEntry { name: name.clone(), active: is_game_master(name) };
            if is_esm(name) {
                let at = self.entries.iter().rposition(|e| is_esm(&e.name)).map_or(0, |i| i + 1);
                self.entries.insert(at, entry);
            } else {
                self.entries.push(entry);
            }
            added.push(name.clone());
        }
        added
    }

    /// The active plugins of `present`, in the list's order, named as in `Data`;
    /// `FalloutNV.esm` first whatever the list says. Also the active names not present, and
    /// whether `FalloutNV.esm` had to be put first.
    pub fn load_order(&self, present: &[String]) -> (Vec<String>, Vec<String>, bool) {
        let mut load = Vec::new();
        let mut missing = Vec::new();
        for e in self.entries.iter().filter(|e| e.active) {
            match present.iter().find(|p| p.eq_ignore_ascii_case(&e.name)) {
                Some(p) => load.push(p.clone()),
                None => missing.push(e.name.clone()),
            }
        }
        let base = present.iter().find(|p| p.eq_ignore_ascii_case("FalloutNV.esm")).cloned().unwrap_or_else(|| "FalloutNV.esm".into());
        let forced = load.first() != Some(&base);
        if forced {
            load.retain(|p| p != &base);
            load.insert(0, base);
        }
        (load, missing, forced)
    }
}

impl EngineHome {
    pub fn plugins_file(&self) -> std::path::PathBuf {
        self.root.join(PLUGINS_FILE)
    }
}

/// Turns plugin `name` on in the home's list (a new line at the end when it has none), as
/// editor mode does for a plugin it saved into the mods folder. Returns whether the list
/// changed.
pub fn turn_on(home: &EngineHome, name: &str) -> Result<bool, InstallError> {
    let path = home.plugins_file();
    let mut list = match std::fs::read_to_string(&path) {
        Ok(t) => PluginList::parse(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PluginList::default(),
        Err(e) => return Err(InstallError::Io(path, e)),
    };
    match list.entries.iter_mut().find(|e| e.name.eq_ignore_ascii_case(name)) {
        Some(e) if e.active => return Ok(false),
        Some(e) => e.active = true,
        None => list.entries.push(PluginEntry { name: name.to_string(), active: true }),
    }
    guard_write(&path, None)?;
    std::fs::create_dir_all(&home.root).map_err(|e| InstallError::Io(home.root.clone(), e))?;
    std::fs::write(&path, list.to_text()).map_err(|e| InstallError::Io(path.clone(), e))?;
    Ok(true)
}

/// Reads the home's plugin list (made when there is none), gives the plugins new in
/// `Data` or the home's mods folder their lines, writes it back when that changed it, and
/// returns what to load.
pub fn sync(home: &EngineHome, game: &GameInstall) -> Result<PluginSync, InstallError> {
    let path = home.plugins_file();
    let mut list = match std::fs::read_to_string(&path) {
        Ok(t) => PluginList::parse(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PluginList::default(),
        Err(e) => return Err(InstallError::Io(path, e)),
    };
    let mods = home.mods();
    let present = present_plugins(game, &mods)?;
    let added = list.add_new(&present);
    let written = !added.is_empty() || !path.exists();
    if written {
        guard_write(&path, Some(game))?;
        std::fs::create_dir_all(&home.root).map_err(|e| InstallError::Io(home.root.clone(), e))?;
        std::fs::write(&path, list.to_text()).map_err(|e| InstallError::Io(path.clone(), e))?;
    }
    let (load, missing, base_forced) = list.load_order(&present);
    let from_mods = load.iter().filter(|n| child_ci(&mods, n).is_some()).cloned().collect();
    Ok(PluginSync { load, from_mods, added, missing, base_forced, written })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_and_write_back() {
        let l = PluginList::parse("; c\n# c\n*FalloutNV.esm\n  * DeadMoney.esm \nMod.esp\nmod.ESP\n\n");
        assert_eq!(
            l.entries,
            vec![
                PluginEntry { name: "FalloutNV.esm".into(), active: true },
                PluginEntry { name: "DeadMoney.esm".into(), active: true },
                PluginEntry { name: "Mod.esp".into(), active: false },
            ]
        );
        assert_eq!(PluginList::parse(&l.to_text()), l);
    }

    #[test]
    fn new_plugins_masters_on_mods_off() {
        let mut l = PluginList::parse("*FalloutNV.esm\n*Old.esp\n");
        let added = l.add_new(&names(&["FalloutNV.esm", "HonestHearts.esm", "ModMaster.esm", "New.esp"]));
        assert_eq!(added, names(&["HonestHearts.esm", "ModMaster.esm", "New.esp"]));
        let text = l.to_text();
        let body: Vec<&str> = text.lines().filter(|s| !s.starts_with(';')).collect();
        assert_eq!(body, ["*FalloutNV.esm", "*HonestHearts.esm", "ModMaster.esm", "*Old.esp", "New.esp"]);
    }

    #[test]
    fn load_order_follows_lines_and_skips_missing() {
        let l = PluginList::parse("*B.esp\n*FalloutNV.esm\nC.esp\n*Gone.esp\n*a.esp\n");
        let (load, missing, forced) = l.load_order(&names(&["FalloutNV.esm", "A.esp", "B.esp", "C.esp"]));
        assert_eq!(load, names(&["FalloutNV.esm", "B.esp", "A.esp"]));
        assert_eq!(missing, names(&["Gone.esp"]));
        assert!(forced);
    }

    #[test]
    fn sync_makes_the_list_then_adds_new_files() {
        let t = std::env::temp_dir().join(format!("nvf-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let data = t.join("game").join("Data");
        std::fs::create_dir_all(&data).unwrap();
        for f in ["FalloutNV.esm", "DeadMoney.esm", "Mod.esp", "Readme.txt"] {
            std::fs::write(data.join(f), b"TES4").unwrap();
        }
        let game = GameInstall::open(t.join("game")).unwrap();
        let home = EngineHome::new(t.join("home"));
        let s = sync(&home, &game).unwrap();
        assert!(s.written);
        assert_eq!(s.load, names(&["FalloutNV.esm", "DeadMoney.esm"]));
        assert_eq!(s.added, names(&["FalloutNV.esm", "DeadMoney.esm", "Mod.esp"]));
        // The player turns the mod on; nothing new: the file is left alone.
        std::fs::write(home.plugins_file(), "*FalloutNV.esm\n*DeadMoney.esm\n*Mod.esp\n").unwrap();
        let s = sync(&home, &game).unwrap();
        assert!(!s.written);
        assert_eq!(s.load, names(&["FalloutNV.esm", "DeadMoney.esm", "Mod.esp"]));
        // A new mod appears in Data: a line, off.
        std::fs::write(data.join("Later.esp"), b"TES4").unwrap();
        let s = sync(&home, &game).unwrap();
        assert_eq!(s.added, names(&["Later.esp"]));
        assert!(std::fs::read_to_string(home.plugins_file()).unwrap().ends_with("*Mod.esp\nLater.esp\n"));
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn turn_on_adds_or_switches_a_line() {
        let t = std::env::temp_dir().join(format!("nvf-plugins-on-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let home = EngineHome::new(t.join("home"));
        assert!(turn_on(&home, "Edits.esp").unwrap());
        std::fs::write(home.plugins_file(), "*FalloutNV.esm
Edits.esp
").unwrap();
        assert!(turn_on(&home, "edits.esp").unwrap());
        assert!(!turn_on(&home, "Edits.esp").unwrap());
        let l = PluginList::parse(&std::fs::read_to_string(home.plugins_file()).unwrap());
        assert_eq!(l.entries, vec![PluginEntry { name: "FalloutNV.esm".into(), active: true }, PluginEntry { name: "Edits.esp".into(), active: true }]);
        std::fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn mods_folder_plugins_are_listed_and_win_over_data() {
        let t = std::env::temp_dir().join(format!("nvf-plugins-mods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let data = t.join("game").join("Data");
        std::fs::create_dir_all(&data).unwrap();
        for f in ["FalloutNV.esm", "Mod.esp"] {
            std::fs::write(data.join(f), b"TES4").unwrap();
        }
        let game = GameInstall::open(t.join("game")).unwrap();
        let home = EngineHome::new(t.join("home"));
        // No mods folder yet: only Data's plugins.
        assert_eq!(present_plugins(&game, &home.mods()).unwrap(), names(&["FalloutNV.esm", "Mod.esp"]));
        std::fs::create_dir_all(home.mods()).unwrap();
        for f in ["Edit.esp", "mod.esp", "Notes.txt"] {
            std::fs::write(home.mods().join(f), b"TES4").unwrap();
        }
        // Mod.esp in both: listed once, as the mods folder names it.
        assert_eq!(present_plugins(&game, &home.mods()).unwrap(), names(&["FalloutNV.esm", "Edit.esp", "mod.esp"]));
        let s = sync(&home, &game).unwrap();
        assert_eq!(s.added, names(&["FalloutNV.esm", "Edit.esp", "mod.esp"]));
        std::fs::write(home.plugins_file(), "*FalloutNV.esm\n*Mod.esp\n*Edit.esp\n").unwrap();
        let s = sync(&home, &game).unwrap();
        assert_eq!(s.load, names(&["FalloutNV.esm", "mod.esp", "Edit.esp"]));
        assert_eq!(s.from_mods, names(&["mod.esp", "Edit.esp"]));
        assert!(s.missing.is_empty());
        // The mods folder's copy is the one opened; Data's otherwise.
        // (On a case-insensitive file system the exact name exists too; either names the file.)
        let p = plugin_path(&data, Some(&home.mods()), "Mod.esp");
        assert_eq!(p.parent(), Some(home.mods().as_path()));
        assert!(p.file_name().unwrap().to_string_lossy().eq_ignore_ascii_case("mod.esp"));
        assert_eq!(plugin_path(&data, Some(&home.mods()), "FalloutNV.esm"), data.join("FalloutNV.esm"));
        assert_eq!(plugin_path(&data, None, "Mod.esp"), data.join("Mod.esp"));
        std::fs::remove_dir_all(&t).ok();
    }
}
