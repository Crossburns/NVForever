//! `fnvtool ui ...`: the menu files read into the tile tree (fnv-ui;
//! docs/spec-notes/menu-xml.md, docs/design/ui-tile-tree.md). Reads the owner's
//! archives and loose files; prints counts, mended spots and trait values, never the
//! files' text.

use clap::Subcommand;
use fnv_ui::{Origin, Source, TileId, Ui, Value as UiValue};
use fnv_vfs::Vfs;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum UiCmd {
    /// Every menu file under `menus\` built into the tree and worked out: tiles, traits,
    /// expressions, what the reader mended, includes not found (fnv.ui.menus/1).
    Menus {
        #[command(flatten)]
        src: UiSourceArgs,
    },
    /// One menu's tree with its traits' values (fnv.ui.tree/1).
    Tree {
        #[command(flatten)]
        src: UiSourceArgs,
        /// The menu file, e.g. `menus\main\hud_main_menu.xml`.
        #[arg(long)]
        menu: String,
        /// Only the subtree a path names, e.g. `HUDMainMenu/HitPoints`.
        #[arg(long)]
        at: Option<String>,
    },
    /// The tiles mods added to one menu that NVForever's tile renderer would draw over its
    /// own HUD (`Ui::mod_added_tiles`, `Ui::mod_tile_drawn`): injected and template tiles,
    /// and a loose replacement's tiles that the archived file does not have
    /// (fnv.ui.mod_tiles/1).
    ModTiles {
        #[command(flatten)]
        src: UiSourceArgs,
        /// The menu file, e.g. `menus\main\hud_main_menu.xml`.
        #[arg(long)]
        menu: String,
    },
}

#[derive(clap::Args)]
pub struct UiSourceArgs {
    /// The game's Data folder (its archives and loose files).
    #[arg(long)]
    data: PathBuf,
    /// A mod's folder laid over Data, e.g. an MO2 mod folder (repeatable; later ones win).
    #[arg(long)]
    overlay: Vec<PathBuf>,
    /// Active plugins for UIO ? conditions (repeatable). Without this flag, use all
    /// .esm/.esp files at the top level of Data and the overlay folders.
    #[arg(long)]
    plugin: Vec<String>,
    /// The view's size for `screen()` (width x height).
    #[arg(long, default_value = "1920x1080")]
    screen: String,
}

/// The VFS with loose overlay folders over it.
struct Files {
    vfs: Vfs,
    overlays: Vec<PathBuf>,
    paths: Vec<String>,
}

impl Files {
    fn loose(&self, path: &str) -> Option<PathBuf> {
        let rel = path.replace('\\', "/");
        self.overlays.iter().rev().find_map(|d| find_ci(d, &rel))
    }
}

/// A file under `dir` by a relative path in any case.
fn find_ci(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut at = dir.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        let exact = at.join(part);
        at = if exact.exists() {
            exact
        } else {
            std::fs::read_dir(&at)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(part)))?
        };
    }
    at.is_file().then_some(at)
}

impl Source for Files {
    fn list(&self, dir: &str) -> Vec<String> {
        let prefix = format!("{}\\", fnv_vfs::normalize(dir).trim_end_matches('\\'));
        self.paths.iter().filter(|p| p.starts_with(&prefix)).cloned().collect()
    }
    /// [design] An overlay's file, or a loose file of Data, as the game's `UiFiles`.
    fn is_loose(&self, path: &str) -> bool {
        self.loose(path).is_some() || self.vfs.source_of(path) == Some("loose")
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        if let Some(p) = self.loose(path) {
            return std::fs::read(p).ok();
        }
        self.vfs.read(path)?.ok()
    }

    /// [design] The archives alone: what an overlay's or Data's loose file replaced.
    fn read_archived(&self, path: &str) -> Option<Vec<u8>> {
        self.vfs.read_archived(path)?.ok()
    }
}

fn open(a: &UiSourceArgs) -> anyhow::Result<(Files, Ui)> {
    // [design] UI inspection has no load-order argument; discover available plugins,
    // with --plugin providing an explicit active set for disabled-plugin diagnostics.
    let mut plugins = a.plugin.clone();
    if plugins.is_empty() {
        for dir in std::iter::once(&a.data).chain(&a.overlay) {
            for entry in std::fs::read_dir(dir)?.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path.extension().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("esm") || s.eq_ignore_ascii_case("esp"))
                {
                    plugins.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        plugins.sort_by_key(|p| p.to_ascii_lowercase());
        plugins.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        // [design] Keep the existing default masters' archive order ahead of mod plugins.
        plugins.sort_by_key(|p| fnv_vfs::DEFAULT_PLUGINS.iter().position(|d| d.eq_ignore_ascii_case(p)).unwrap_or(usize::MAX));
    }
    let vfs = Vfs::open_for_plugins(&a.data, &plugins)?;
    let mut paths = vfs.paths(|_| true);
    for dir in &a.overlay {
        overlay_paths(dir, dir, &mut paths)?;
    }
    paths.sort();
    paths.dedup();
    let files = Files { vfs, overlays: a.overlay.clone(), paths };
    let mut ui = Ui::new();
    ui.configure_uio(&files, |name| plugins.iter().any(|p| p.eq_ignore_ascii_case(name)));
    ui.load_globals(&files);
    let (w, h) = a.screen.split_once(['x', 'X']).ok_or_else(|| anyhow::anyhow!("--screen: WxH"))?;
    let s = ui.screen();
    ui.set_trait(s, "width", UiValue::Num(w.trim().parse()?));
    ui.set_trait(s, "height", UiValue::Num(h.trim().parse()?));
    Ok((files, ui))
}

/// The menu files: the archives' and the overlays' `menus\*.xml` but the prefabs.
fn menu_files(files: &Files) -> Vec<String> {
    files.list("menus").into_iter().filter(|p| p.ends_with(".xml") && !p.starts_with("menus\\prefabs\\")).collect()
}

fn overlay_paths(root: &Path, dir: &Path, paths: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            overlay_paths(root, &path, paths)?;
        } else if entry.file_type()?.is_file() {
            paths.push(fnv_vfs::normalize(&path.strip_prefix(root).expect("overlay descendant").to_string_lossy()));
        }
    }
    Ok(())
}

fn uio_json(ui: &Ui) -> Value {
    let r = &ui.uio;
    let mut skipped = std::collections::BTreeMap::<&str, usize>::new();
    for d in &r.decisions {
        if d.applied == 0 {
            *skipped.entry(&d.reason).or_default() += 1;
        }
    }
    json!({ "read": r.decisions.len() + r.duplicates, "entries": r.decisions.len(), "duplicates": r.duplicates,
        "applied": r.applied(), "skipped": r.decisions.len() - r.applied(), "skip_reasons": skipped,
        "warnings": r.warnings, "decisions": r.lines() })
}

fn counts(ui: &Ui, root: TileId) -> (usize, usize, usize) {
    let (mut tiles, mut traits, mut exprs) = (0, 0, 0);
    let mut stack = vec![root];
    while let Some(t) = stack.pop() {
        let tile = ui.tile(t);
        tiles += 1;
        traits += tile.traits.len();
        exprs += tile.traits.iter().filter(|tr| tr.expr.is_some()).count();
        stack.extend(tile.children.iter().copied());
    }
    (tiles, traits, exprs)
}

fn value_json(v: &UiValue) -> Value {
    match v {
        UiValue::Num(n) => json!(n),
        UiValue::Str(s) => json!(s),
        UiValue::Sym(s) => json!(format!("&{s};")),
    }
}

fn tree_json(ui: &Ui, t: TileId) -> Value {
    let tile = ui.tile(t);
    let mut traits = Map::new();
    for tr in &tile.traits {
        traits.insert(tr.name.clone(), value_json(&tr.value));
    }
    json!({
        "kind": tile.kind,
        "name": tile.name,
        "traits": traits,
        "templates": tile.templates.iter().map(|tp| tp.name.clone()).collect::<Vec<_>>(),
        "children": tile.children.iter().map(|&c| tree_json(ui, c)).collect::<Vec<_>>(),
    })
}

pub fn run(cmd: UiCmd) -> anyhow::Result<Value> {
    match cmd {
        UiCmd::Menus { src } => {
            let (files, mut ui) = open(&src)?;
            let globals_warnings = ui.warnings.len();
            let mut menus = Vec::new();
            for path in menu_files(&files) {
                ui.warnings.clear();
                match ui.load_menu(&files, &path) {
                    Ok(id) => {
                        let changed = ui.update();
                        let (tiles, traits, exprs) = counts(&ui, id);
                        menus.push(json!({
                            "file": path, "menu": ui.tile(id).name, "tiles": tiles, "traits": traits,
                            "expressions": exprs, "changed_on_update": changed,
                            "warnings": ui.warnings.clone(),
                        }));
                    }
                    Err(e) => menus.push(json!({ "file": path, "error": e, "warnings": ui.warnings.clone() })),
                }
            }
            let built = menus.iter().filter(|m| m.get("menu").is_some()).count();
            Ok(json!({ "schema": "fnv.ui.menus/1", "globals_warnings": globals_warnings, "built": built, "menus": menus, "uio": uio_json(&ui) }))
        }
        UiCmd::Tree { src, menu, at } => {
            let (files, mut ui) = open(&src)?;
            let id = ui.load_menu(&files, &menu).map_err(anyhow::Error::msg)?;
            ui.update();
            let root = match &at {
                Some(p) => ui.find_tile(p).ok_or_else(|| anyhow::anyhow!("{p}: no such tile"))?,
                None => id,
            };
            Ok(
                json!({ "schema": "fnv.ui.tree/1", "path": ui.path_of(root), "warnings": ui.warnings, "tree": tree_json(&ui, root), "uio": uio_json(&ui) }),
            )
        }
        UiCmd::ModTiles { src, menu } => {
            let (files, mut ui) = open(&src)?;
            let id = ui.load_menu(&files, &menu).map_err(anyhow::Error::msg)?;
            ui.update();
            let roots = ui.mod_added_tiles(id);
            // The archived file's names, only when a root is a loose file's [design].
            let loose = roots.iter().any(|&r| matches!(ui.tile(r).origin, Origin::File { .. }));
            let vanilla = loose.then(|| Ui::archived_name_paths(&files, &menu)).flatten();
            let mut total = 0;
            let rows: Vec<Value> = roots
                .iter()
                .map(|&r| {
                    let mut drawn = Vec::new();
                    let mut stack = vec![r];
                    while let Some(t) = stack.pop() {
                        let tile = ui.tile(t);
                        if !tile.live {
                            continue;
                        }
                        if ui.mod_tile_drawn(t, vanilla.as_ref()) {
                            drawn.push(ui.path_of(t));
                        }
                        stack.extend(tile.children.iter().rev().copied());
                    }
                    total += drawn.len();
                    let origin = match &ui.tile(r).origin {
                        Origin::File { path, loose } => json!({ "file": path, "loose": loose }),
                        Origin::Injected => json!("injected"),
                        Origin::Template => json!("template"),
                        Origin::Code => json!("code"),
                    };
                    json!({ "root": ui.path_of(r), "origin": origin, "drawn": drawn.len(), "tiles": drawn })
                })
                .collect();
            Ok(json!({
                "schema": "fnv.ui.mod_tiles/1", "menu": ui.tile(id).name, "loose_roots": loose,
                "vanilla_compared": vanilla.is_some(), "drawn": total, "roots": rows,
                "uio": uio_json(&ui),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlays_supply_plugins_conditions_and_winning_lists() {
        // [design] Synthetic files in this worktree; plugin bytes are never parsed.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/ui-tool-uio-{}", std::process::id()));
        let data = root.join("Data");
        let overlay = root.join("mod");
        std::fs::create_dir_all(data.join("menus")).unwrap();
        std::fs::create_dir_all(overlay.join("UIO/Public")).unwrap();
        std::fs::create_dir_all(overlay.join("Menus/Prefabs")).unwrap();
        std::fs::write(data.join("menus/hud.xml"), "<menu name=\"HUDMainMenu\"/>").unwrap();
        std::fs::write(overlay.join("JustAssortedMods.esp"), []).unwrap();
        std::fs::write(overlay.join("UIO/Public/test.txt"), "probe.xml::HUDMainMenu\n?JustAssortedMods.esp && #probe.xml").unwrap();
        std::fs::write(overlay.join("Menus/Prefabs/probe.xml"), "<rect name=\"Probe\"/>").unwrap();
        let mut args = UiSourceArgs { data, overlay: vec![overlay.clone()], plugin: Vec::new(), screen: "800x600".into() };
        let (files, mut ui) = open(&args).unwrap();
        assert_eq!(files.list("UiO/PuBlIc"), ["uio\\public\\test.txt"]);
        ui.load_menu(&files, "menus/hud.xml").unwrap();
        let added = ui.find_tile("HUDMainMenu/Probe").unwrap();
        assert!(ui.mod_tile_drawn(added, None));
        assert_eq!(uio_json(&ui)["applied"], 1);
        args.plugin = vec!["FalloutNV.esm".into()];
        let (files, mut ui) = open(&args).unwrap();
        ui.load_menu(&files, "menus/hud.xml").unwrap();
        assert!(ui.find_tile("HUDMainMenu/Probe").is_none());
        assert_eq!(uio_json(&ui)["skip_reasons"]["condition false"], 1);
        // [design] Delete only the synthetic files and their now-empty directories.
        for path in [
            root.join("Data/menus/hud.xml"),
            overlay.join("JustAssortedMods.esp"),
            overlay.join("UIO/Public/test.txt"),
            overlay.join("Menus/Prefabs/probe.xml"),
        ] {
            std::fs::remove_file(path).unwrap();
        }
        for path in [
            root.join("Data/menus"),
            root.join("Data"),
            overlay.join("UIO/Public"),
            overlay.join("UIO"),
            overlay.join("Menus/Prefabs"),
            overlay.join("Menus"),
            overlay,
            root,
        ] {
            std::fs::remove_dir(path).unwrap();
        }
    }
}
