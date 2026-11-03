//! Files and startup loading for the tile tree [design: ui-tile-tree.md §3, §9].

use crate::{game::menus, Game};
use fnv_sim::{settings::Setting, World};
use fnv_ui::{
    value::{menu_class_name, menu_class_number},
    GameStrings, MenuIndex, Source, Ui, Value,
};
use fnv_vfs::Vfs;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

/// Where a headless run (`runner::run`, as `fnvtool ext queries`) reads the menus: the
/// files and the menu paths, made from the run's world. Without it the tree stays empty
/// and GetUIFloat reads -999 as for a menu not loaded [design].
pub type HeadlessMenus = Arc<dyn Fn(&World<'_>) -> Option<(Box<dyn Source + Send + Sync>, Vec<String>)> + Send + Sync>;

static HEADLESS: Mutex<Option<HeadlessMenus>> = Mutex::new(None);

/// Set where headless runs read the menus from (the tool's archives).
pub fn set_headless_menus(f: HeadlessMenus) {
    *HEADLESS.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
}

/// Load the always-loaded menus into a headless run's tree, as the engine does at its
/// start ([`load`]), when [`set_headless_menus`] gave the files.
pub(crate) fn load_headless(game: &mut Game<'_>) -> Option<LoadReport> {
    let f = HEADLESS.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
    let (files, paths) = f(&game.world)?;
    let report = load(game, files, paths);
    headless_hud(game);
    Some(report)
}

/// The HUD as the engine writes it in GameMode with nothing to show beyond the HUD itself
/// (no sneaking, radiation, prompt or target), once: a headless run has no frames of the
/// engine's HUD [design]. The need words from their settings; every other text as the
/// menu file left it.
fn headless_hud(game: &mut Game<'_>) {
    use crate::ui_hud::{Shown, Texts, NEED_WORDS};
    let needs = NEED_WORDS.map(|(setting, word)| match game.world.setting_value(setting) {
        Some(Setting::Str(v)) if !v.is_empty() => v.clone(),
        _ => word.to_string(),
    });
    let ui = &mut game.ui_tree;
    let text = |p: &str| ui.get_string(&format!("HUDMainMenu/{p}/string")).unwrap_or_default();
    let texts = Texts {
        radiation_word: text("RadiationMeter/radiation_text"),
        radiation_value: text("RadiationMeter/radiation_text_value"),
        xp_label: text("XPMeter/XPLabel"),
        xp_level_up: text("XPMeter/XPLevelUp"),
        needs,
        sneak: None,
        prompt_key: None,
        prompt_verb: None,
        frame: Default::default(),
    };
    let shown = Shown { root: true, corners: true, reticle: true, ..Default::default() };
    crate::ui_hud::write(ui, shown, &texts);
}

/// Menus kept for the run [documented: InjectUIXML notes, ui-tile-tree.md §3.3].
pub const ALWAYS_LOADED: &[&str] = &["HUDMainMenu", "InventoryMenu", "MapMenu", "RepairMenu", "StatsMenu", "ItemModMenu"];

/// Logging remembered until the tree is rebuilt after a load [design].
#[derive(Default)]
pub struct MenuLifetime {
    unavailable: BTreeSet<u16>,
    warnings: BTreeSet<String>,
}

#[derive(Default, Debug)]
pub struct LifetimeReport {
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

fn class_code(ui: &Ui, root: fnv_ui::TileId) -> Option<u16> {
    match ui.tile(root).get("class")? {
        Value::Sym(name) => menu_class_number(name),
        _ => None,
    }
}

fn warning_key(warning: &str) -> String {
    let (file, message) = warning.split_once(": ").unwrap_or(("tree", warning));
    format!("{}: {message}", file.replace('/', "\\").to_ascii_lowercase())
}

/// The engine calls before scripts, including while its own pause/main menu holds them
/// [design: ui-tile-tree.md §4.1; owner-review.md item 34]. No side menus or groups.
pub fn sync(game: &mut Game<'_>, paused: bool, main_menu: bool) -> LifetimeReport {
    let mut report = LifetimeReport::default();
    let Some(files) = game.ui_files.as_deref() else { return report };
    // The engine's prompt over the open menu (`Game::ui_prompt`, T15a) builds as well.
    let mut open: BTreeSet<_> = game.menu.into_iter().chain(game.menu_under).chain(game.ui_prompt).collect();
    if game.dialogue.is_some() {
        open.insert(menus::DIALOGUE);
    }
    if paused || main_menu {
        open.insert(menus::START);
    }
    open.retain(|code| !matches!(*code, menus::PIPBOY | menus::LOADING | menus::VATS));
    let roots: Vec<_> = game.ui_tree.menus().collect();
    for root in roots {
        let tile = game.ui_tree.tile(root);
        if ALWAYS_LOADED.iter().any(|name| tile.name.eq_ignore_ascii_case(name)) {
            continue;
        }
        if let Some(code) = class_code(&game.ui_tree, root) {
            if !open.contains(&code) {
                game.ui_tree.unload_tile(root);
                report.changes.push(format!("dropped {}", menu_class_name(code).unwrap()));
            }
        }
    }
    let warning_start = game.ui_tree.warnings.len();
    let uio_before: Vec<_> = game.ui_tree.uio.decisions.iter().map(|d| d.applied).collect();
    for code in open {
        if game.ui_tree.menus().any(|root| class_code(&game.ui_tree, root) == Some(code)) || game.ui_lifetime.unavailable.contains(&code) {
            continue;
        }
        let name = menu_class_name(code).map_or_else(|| format!("menu {code}"), str::to_owned);
        let result = match game.ui_menus.code_path(code) {
            Some(path) => game.ui_tree.load_menu(files, path).map(|root| (path, root)),
            None => Err(format!("{name}: no file with matching menu class")),
        };
        match result {
            Ok((path, root)) => {
                let mut count = 0;
                let mut stack = vec![root];
                while let Some(id) = stack.pop() {
                    count += 1;
                    stack.extend(game.ui_tree.tile(id).children.iter().copied());
                }
                report.changes.push(format!("built {name} ({path}, {count} tiles)"));
            }
            Err(message) => {
                game.ui_lifetime.unavailable.insert(code);
                report.warnings.push(message);
            }
        }
    }
    // New menus' expressions must be readable by this frame's scripts [design: §4.1].
    if !report.changes.is_empty() {
        game.ui_tree.update();
    }
    for warning in &game.ui_tree.warnings[warning_start..] {
        if game.ui_lifetime.warnings.insert(warning_key(warning)) {
            report.warnings.push(warning.clone());
        }
    }
    for (i, d) in game.ui_tree.uio.decisions.iter().enumerate() {
        if d.applied > uio_before[i] {
            report.changes.push(format!("UIO {}:{}: {} applied ({} applications)", d.entry.list, d.entry.line, d.entry.include, d.applied));
        }
    }
    report
}

pub struct UiFiles {
    vfs: Arc<Vfs>,
    strings: GameStrings,
}

impl UiFiles {
    /// The game strings are the load order's string settings as they are now (one value
    /// per name: the last plugin's record, or a run-time change), over the built-in table
    /// (`fnv_ui::strings`; menu-xml.md §2.4, task T10).
    pub fn new(vfs: Arc<Vfs>, world: &World<'_>) -> Self {
        // Own the strings: scripts keep this source after the world's borrow ends [design].
        let strings = GameStrings::from_settings(world.forms.settings.keys().filter_map(|name| match world.setting_value(name) {
            Some(Setting::Str(value)) => Some((name.as_str(), value.clone())),
            _ => None,
        }));
        Self { vfs, strings }
    }

    /// The table `&-sName;` reads from.
    pub fn game_strings(&self) -> &GameStrings {
        &self.strings
    }

    pub fn menu_paths(&self) -> Vec<String> {
        self.vfs.paths(|p| p.starts_with("menus\\") && p.ends_with(".xml") && !p.starts_with("menus\\prefabs\\"))
    }
}

impl Source for UiFiles {
    fn list(&self, dir: &str) -> Vec<String> {
        let prefix = format!("{}\\", fnv_vfs::normalize(dir).trim_end_matches('\\'));
        self.vfs.paths(|p| p.starts_with(&prefix))
    }
    fn is_loose(&self, path: &str) -> bool {
        self.vfs.source_of(path) == Some("loose")
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.vfs.read(path)?.ok()
    }

    fn read_archived(&self, path: &str) -> Option<Vec<u8>> {
        self.vfs.read_archived(path)?.ok()
    }

    fn game_string(&self, name: &str) -> Option<String> {
        self.strings.get(name)
    }
}

/// The text of the game setting `name` for the engine's own menus that stand in for menu
/// files (the tutorial menu, the lockpick menu): what `&-name;` reads in the tree, the load
/// order's string setting over NVForever's labels (`fnv_ui::strings::BUILT_IN`), or the
/// label alone before the menu files are read; empty for a name in neither
/// (menu-xml.md §2.4, task T10).
pub fn game_string(game: &Game<'_>, name: &str) -> String {
    game.ui_files.as_deref().and_then(|f| f.game_string(name)).or_else(|| fnv_ui::strings::built_in(name).map(str::to_owned)).unwrap_or_default()
}

#[derive(Default)]
pub struct LoadReport {
    pub menus: usize,
    pub tiles: usize,
    /// One entry per file; repeated reads of an include report its mends once [design].
    pub warnings: BTreeMap<String, Vec<String>>,
    /// The game strings (`&-sName;`) the loaded menus read, and how many of them had no
    /// text (menu-xml.md §2.4: no plugin record and no built-in text yet).
    pub game_strings: usize,
    pub game_strings_unresolved: usize,
}

impl LoadReport {
    pub fn warning_count(&self) -> usize {
        self.warnings.values().map(Vec::len).sum()
    }
}

pub fn load(game: &mut Game<'_>, files: Box<dyn Source + Send + Sync>, paths: impl IntoIterator<Item = String>) -> LoadReport {
    let mut ui = Ui::new();
    ui.configure_uio(files.as_ref(), |name| game.data.load_order.iter().any(|p| p.eq_ignore_ascii_case(name)));
    if !ui.load_globals(files.as_ref()) {
        ui.warnings.push("menus\\globals.xml: not found".into());
    }
    let index = MenuIndex::build(files.as_ref(), paths);
    ui.warnings.extend(index.warnings.iter().cloned());
    let mut report = LoadReport::default();
    for name in ALWAYS_LOADED {
        let Some(path) = index.path(name) else {
            ui.warnings.push(format!("{name}: menu not found"));
            continue;
        };
        match ui.load_menu(files.as_ref(), path) {
            Ok(root) => {
                if *name == "HUDMainMenu" {
                    crate::ui_hud::build(&mut ui);
                }
                report.menus += 1;
                let mut stack = vec![root];
                while let Some(id) = stack.pop() {
                    report.tiles += 1;
                    stack.extend(ui.tile(id).children.iter().copied());
                }
            }
            Err(e) => ui.warnings.push(e),
        }
    }
    ui.update();
    report.game_strings = ui.game_strings_read.len();
    report.game_strings_unresolved = ui.game_strings_read.values().filter(|&&found| !found).count();
    ui.warnings.extend(ui.uio.lines());
    for warning in &ui.warnings {
        let (file, message) = warning.split_once(": ").unwrap_or(("tree", warning));
        let file = if file.to_ascii_lowercase().ends_with(".xml") { file.replace('/', "\\").to_ascii_lowercase() } else { file.into() };
        let messages = report.warnings.entry(file).or_default();
        if !messages.iter().any(|m| m == message) {
            messages.push(message.into());
        }
    }
    game.ui_tree = ui;
    crate::ui_more::tree_rebuilt(game);
    // The old tree's tiles are gone with it [design].
    game.ui_pointer = fnv_ui::input::Pointer::default();
    game.ui_menus = index;
    game.ui_files = Some(files);
    game.ui_lifetime = MenuLifetime { unavailable: BTreeSet::new(), warnings: game.ui_tree.warnings.iter().map(|w| warning_key(w)).collect() };
    report
}
