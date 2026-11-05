//! JIP LN's Script Runner (docs/spec-notes/ext-script-runner.md): script files a mod
//! ships under `Data\NVSE\plugins\scripts\` with no plugin, each run at the moment its
//! file name's prefix names [documented: GECK wiki RegisterSRScript]. A file is a bare
//! script body, statements and inline declarations without `scn` or `Begin` [observed:
//! the 203 files of the owner's mod lists, ext-script-runner.md §1]; it is compiled as a
//! result script is ([`Game::compile_statements`]), once per game, and runs with
//! temporary locals, so its arrays are not freed when it ends [documented: Ar Null,
//! "Should be called on all arrays that were created from Script Runner"].
//!
//! The engine calls [`Game::run_script_runner`] at each moment (`nvf-play` script.rs):
//! [`Moment::Restart`] when the runtime is made, then [`Moment::Load`] after a save's
//! LoadGame events or [`Moment::NewGame`] on the first frame of a run without one (after
//! the main menu's New Game; not behind the main menu, #1548). A load while
//! playing starts a new run in NVForever, so Restart comes again after every load, as
//! GetGameRestarted does.

use crate::game::{Game, RunCtx};
use crate::data::Compiled;
use fnv_script::vm::XVal;
use std::collections::HashMap;

/// The folder under Data the files are read from [documented: RegisterSRScript, "relative
/// to ..\Data\NVSE\plugins\scripts\"].
pub const FOLDER: &str = "nvse\\plugins\\scripts";

/// When the files run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// The game started (the runtime was made), before any save is in place.
    Restart,
    /// A run that starts without a save.
    NewGame,
    /// A save was loaded.
    Load,
}

impl Moment {
    pub fn name(self) -> &'static str {
        match self {
            Moment::Restart => "game restart",
            Moment::NewGame => "new game",
            Moment::Load => "game load",
        }
    }
}

/// A file name's prefix, which says when it runs (ext-script-runner.md §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefix {
    /// `gr_`: game restart [documented: RegisterSRScript, "Game Restart ('gr_' prefix)";
    /// InitExtraMiscStat, a plugin's MenuMode 4 block (the main menu) is a gr_ file's].
    Restart,
    /// `gl_`: a save loaded [hypothesis: the name; the files' own uses, ext-script-runner.md §2].
    Load,
    /// `gn_`: a new game [hypothesis: as gl_].
    NewGame,
    /// `ln_`: a save loaded or a new game [hypothesis: as gl_; the files guard work they
    /// should do once with a flag of their own].
    LoadOrNew,
    /// `gx_`: the game's exit [hypothesis: MCM Extender's file says "on game exit"]. Not
    /// run yet: the engine has no exit moment for scripts (ext-script-runner.md §4).
    Exit,
}

impl Prefix {
    /// The prefix of a file name, any case; `None` for a name without a known one.
    pub fn of(file_name: &str) -> Option<Prefix> {
        let n = file_name.to_ascii_lowercase();
        let p = n.get(..3)?;
        Some(match p {
            "gr_" => Prefix::Restart,
            "gl_" => Prefix::Load,
            "gn_" => Prefix::NewGame,
            "ln_" => Prefix::LoadOrNew,
            "gx_" => Prefix::Exit,
            _ => return None,
        })
    }

    pub fn runs_at(self, m: Moment) -> bool {
        matches!(
            (self, m),
            (Prefix::Restart, Moment::Restart) | (Prefix::Load, Moment::Load) | (Prefix::NewGame, Moment::NewGame) | (Prefix::LoadOrNew, Moment::Load | Moment::NewGame)
        )
    }
}

/// One file: its path under [`FOLDER`] as listed (or as RegisterSRScript gave it) and its
/// prefix.
#[derive(Debug, Clone)]
struct SrFile {
    path: String,
    prefix: Prefix,
}

/// What one file did at a moment, for the engine's log and the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrRun {
    pub path: String,
    /// `Ok(())` when it ran to its end; else what the compiler or the script said.
    pub result: Result<(), String>,
}

/// The Script Runner's state for a game.
#[derive(Default)]
pub struct SrState<'a> {
    /// The folder's files, read once (`None` until then), then the registered ones.
    files: Option<Vec<SrFile>>,
    /// The files RegisterSRScript added; they last for the session.
    registered: Vec<SrFile>,
    /// Each file's unit by lower-case path, or why it did not compile.
    units: HashMap<String, Result<&'a Compiled, String>>,
    /// The moment whose files are running.
    running: Option<Moment>,
}

impl SrState<'_> {
    pub(crate) fn take_registered(&mut self) -> Vec<String> {
        std::mem::take(&mut self.registered).into_iter().map(|f| f.path).collect()
    }

    pub(crate) fn put_registered(&mut self, paths: Vec<String>) {
        self.registered = paths.into_iter().filter_map(|path| Some(SrFile { prefix: Prefix::of(file_name(&path))?, path })).collect();
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

impl<'a> Game<'a> {
    /// Run the Script Runner files of `m`: the folder's, in the order of their names
    /// [hypothesis: the order is not documented], then the registered ones in the order
    /// they were registered; a file registered while the Restart files run, with the gr_
    /// prefix, runs after them [hypothesis]. A file that does not compile is skipped and
    /// traced (`Skipped`), as are the errors of one that runs; nothing here stops the game.
    pub fn run_script_runner(&mut self, m: Moment) -> Vec<SrRun> {
        self.sr_list();
        self.sr.running = Some(m);
        let mut runs = Vec::new();
        let mut i = 0;
        loop {
            let listed = self.sr.files.as_ref().map_or(0, Vec::len);
            let file = if i < listed { self.sr.files.as_ref().map(|f| f[i].clone()) } else { self.sr.registered.get(i - listed).cloned() };
            let Some(file) = file else { break };
            i += 1;
            if !file.prefix.runs_at(m) {
                continue;
            }
            let result = self.sr_unit(&file.path).and_then(|unit| {
                if self.run_statements(unit, RunCtx::default()) {
                    Ok(())
                } else {
                    Err("the script stopped on an error".into())
                }
            });
            if let Err(e) = &result {
                self.emit(crate::trace::EventKind::Skipped { unit: format!("Script Runner {}", file.path), what: e.clone() });
            }
            runs.push(SrRun { path: file.path, result });
        }
        self.sr.running = None;
        runs
    }

    /// Compile every file of the folder without running one (`fnvtool ext script-runner`):
    /// each file with `Ok` or what the compiler said.
    pub fn compile_script_runner(&mut self) -> Vec<SrRun> {
        self.sr_list();
        let files: Vec<String> = self.sr.files.iter().flatten().map(|f| f.path.clone()).collect();
        files.into_iter().map(|path| SrRun { result: self.sr_unit(&path).map(|_| ()), path }).collect()
    }

    /// The folder's files with a known prefix, by name, read once a game.
    fn sr_list(&mut self) {
        if self.sr.files.is_none() {
            let mut files: Vec<SrFile> = crate::mod_files::data_files(FOLDER)
                .into_iter()
                .filter(|(n, _)| n.to_ascii_lowercase().ends_with(".txt"))
                .filter_map(|(n, _)| Some(SrFile { prefix: Prefix::of(&n)?, path: n }))
                .collect();
            files.sort_by_key(|f| f.path.to_ascii_lowercase());
            self.sr.files = Some(files);
        }
    }

    /// The unit of a file under [`FOLDER`], compiled the first time it is asked for.
    fn sr_unit(&mut self, path: &str) -> Result<&'a Compiled, String> {
        let key = path.to_ascii_lowercase();
        if let Some(u) = self.sr.units.get(&key) {
            return u.clone();
        }
        let u = match crate::mod_files::read_data_file(&format!("{FOLDER}\\{path}")) {
            // UTF-8 when it is, else 8-bit text in this run's code page, as CompileScript's.
            Some(bytes) => {
                let src = std::str::from_utf8(&bytes).map_or_else(|_| fnv_util::text::decode(&bytes), str::to_owned);
                self.compile_statements(&src, &format!("ScriptRunner\\{path}"), "SR").map(|(u, _)| u)
            }
            None => Err("the file could not be read".into()),
        };
        self.sr.units.insert(key, u.clone());
        u
    }

    /// RegisterSRScript `path` (`crate::nvse` dispatch): 1 when the file under [`FOLDER`]
    /// is read and compiles and no file of that name is registered yet, else 0; only
    /// while the Restart files run, else nothing happens [documented: RegisterSRScript].
    /// The path must name a subfolder [documented: "must include at least one
    /// subfolder"] and the file a known prefix [hypothesis: a file no moment runs is
    /// refused].
    fn register_sr_script(&mut self, path: &str) -> bool {
        if self.sr.running != Some(Moment::Restart) {
            return false;
        }
        let path = path.replace('/', "\\").trim_start_matches('\\').to_string();
        if !path.contains('\\') || path.split('\\').any(|c| c.is_empty() || c == "." || c == "..") {
            return false;
        }
        let Some(prefix) = Prefix::of(file_name(&path)) else { return false };
        let name = file_name(&path).to_ascii_lowercase();
        // "A file with the same name was already registered" [documented].
        if self.sr.registered.iter().any(|f| file_name(&f.path).eq_ignore_ascii_case(&name)) {
            return false;
        }
        if self.sr_unit(&path).is_err() {
            return false;
        }
        self.sr.registered.push(SrFile { path, prefix });
        true
    }
}

/// The Script Runner's script function.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    match name {
        "registersrscript" => {
            let path = match args.first() {
                Some(XVal::Str(s)) => s.clone(),
                _ => return Some(Err("RegisterSRScript needs a file path".into())),
            };
            Some(Ok(XVal::Num(if g.register_sr_script(&path) { 1.0 } else { 0.0 })))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Moment, Prefix};

    #[test]
    fn prefixes_and_their_moments() {
        assert_eq!(Prefix::of("gr_P6MeleeWeapons.txt"), Some(Prefix::Restart));
        assert_eq!(Prefix::of("GL_x.txt"), Some(Prefix::Load));
        assert_eq!(Prefix::of("readme.txt"), None);
        assert_eq!(Prefix::of("g"), None);
        assert!(Prefix::LoadOrNew.runs_at(Moment::Load) && Prefix::LoadOrNew.runs_at(Moment::NewGame));
        assert!(!Prefix::LoadOrNew.runs_at(Moment::Restart));
        assert!(Prefix::Restart.runs_at(Moment::Restart) && !Prefix::Restart.runs_at(Moment::Load));
        for m in [Moment::Restart, Moment::NewGame, Moment::Load] {
            assert!(!Prefix::Exit.runs_at(m));
        }
    }
}
