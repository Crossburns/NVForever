//! T11: the pause menu's Mods page, a mod's part of `StartMenu` drawn from its tiles
//! (issue #976) [design: ui-tile-tree.md §7, owner-review.md #34: NVForever's column
//! stays; a "Mods" entry joins it only when a mod added one, and opens the mod's page
//! from tiles; docs/handoff/ui-tile-tree-tasks.md, T11].
//!
//! - **The switch**: `[UI] TileMenus=StartMenu` (`super::vanilla::TileMenus::start`), off
//!   by default. With it on and a mod's tiles in `StartMenu` (MCM's include, menu-xml.md
//!   §10; `fnv_ui::Ui::has_mod_added_tiles`), the column has a Mods entry
//!   (`crate::pause::ModsEntry`, [`available`]). `StartMenu` itself is built from the files
//!   whenever the pause menu is open (T3b, `fnv_game::ui_loading::sync`).
//! - **The page** (`crate::pause::Page::Mods`): the mod's tiles of `StartMenu` are drawn
//!   over the pause menu (`super::draw`, [`MODS_Z`]; only the tiles mods added, as T5 draws
//!   the HUD's: a loose replacement of the start menu's file draws its own tiles, not the
//!   archived file's), in interface units; T8's pointer and keys go to `StartMenu`
//!   (`super::input`); and the start menu's MenuMode blocks run each frame
//!   (`Begin MenuMode 1013`, [`tick`]), as they do under the original's start menu, so a
//!   mod's script fills in its page. Esc or Back leave it.
//! - **What it does not do**: MCM's own page does not open this way yet. MCM's script
//!   shows MCM only when the original's pause column, filled in by the game's code,
//!   reports `childcount` 8, which NVForever's hand-built column does not write; and its
//!   mod list is filled through MCM.dll functions no source describes (SetMCMModList,
//!   GetMCMListWidth, SetMCMFloatMass, SetMCMStringMass). See the tasks doc, T11.

use fnv_game::game::menus;
use fnv_ui::Ui;
use std::collections::HashSet;

/// The start menu's name [documented: start_menu.xml].
pub const START_MENU: &str = "StartMenu";

/// The Mods page's stacking level: over the pause menu's (110 to 113, `crate::pause`)
/// [design].
pub const MODS_Z: i32 = 115;

/// The archived start menu file's tile names, kept per file path (`super::draw`).
pub type KeptNames = Option<(String, Option<HashSet<String>>)>;

/// Whether the column has a Mods entry: the switch on and a tile in `StartMenu` that a mod
/// adds, as the HUD's mod tiles are told apart (`Ui::mod_tile_drawn`): injected, a
/// template's, a loose UIO include's, or a tile of a loose replacement that the archived
/// file does not have. A loose `start_menu.xml` that only reskins the menu adds none
/// [design]. `files` reads the archived file's names, once per path into `kept`.
pub fn available(switch: super::vanilla::TileMenus, ui: &Ui, files: Option<&dyn fnv_ui::Source>, kept: &mut KeptNames) -> bool {
    if !switch.start {
        return false;
    }
    let Some(menu) = ui.menu(START_MENU).filter(|&m| ui.has_mod_added_tiles(m)) else { return false };
    let vanilla = files.and_then(|f| super::draw::archived_names(ui, menu, f, kept));
    let mut stack = ui.mod_added_tiles(menu);
    while let Some(t) = stack.pop() {
        if !ui.tile(t).live {
            continue;
        }
        if ui.mod_tile_drawn(t, vanilla) {
            return true;
        }
        stack.extend(ui.tile(t).children.iter().copied());
    }
    false
}

/// One frame of the start menu's scripts under the Mods page: only their `MenuMode 1013`
/// blocks run (`Game::page_tick`; no GameMode block, no game time) and the open menu is
/// left alone, so a box a block shows stays open [design: the original runs them under
/// its start menu, as MCM's mods expect, menu-xml.md §10; NVForever's pause menu
/// otherwise holds every script, `crate::script`]. The caller puts the played world in
/// the game first (`crate::script::with_world`).
pub fn tick(game: &mut fnv_game::Game<'_>) {
    game.page_tick(menus::START);
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    #[test]
    fn the_entry_needs_the_switch_and_a_mods_tiles() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let off = super::super::vanilla::TileMenus::default();
        let path = r"menus\options\start_menu.xml";
        let vanilla = r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#;
        let mut kept = None;
        let mut ui = Ui::new();
        assert!(!available(on, &ui, None, &mut kept), "no start menu");
        // The vanilla file from the archive: no mod's tiles.
        let src = MemSource::default().with(path, vanilla);
        ui.load_menu(&src, path).unwrap();
        assert!(!available(on, &ui, Some(&src), &mut kept));
        // A mod's tile injected into it.
        assert!(ui.inject_text(&src, "StartMenu", r#"<rect name="MCM"/>"#));
        assert!(available(on, &ui, Some(&src), &mut kept));
        assert!(!available(off, &ui, Some(&src), &mut kept), "the switch is off by default");
    }

    #[test]
    fn a_loose_reskin_of_the_start_menu_is_no_mods_page() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let path = r"menus\options\start_menu.xml";
        let vanilla = r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#;
        // A UI overhaul's loose copy: the same tiles, other looks.
        let reskin = MemSource::default().with_loose_over(path, r#"<menu name="StartMenu"> <rect name="Panel"> <width>300</width> </rect> </menu>"#, vanilla);
        let mut ui = Ui::new();
        ui.load_menu(&reskin, path).unwrap();
        assert!(ui.has_mod_added_tiles(ui.menu(START_MENU).unwrap()), "the loose file's tiles count as loose");
        assert!(!available(on, &ui, Some(&reskin), &mut None));
        // A loose copy that adds a tile of its own.
        let added = MemSource::default().with_loose_over(path, r#"<menu name="StartMenu"> <rect name="Panel"/> <rect name="ModPage"/> </menu>"#, vanilla);
        let mut ui = Ui::new();
        ui.load_menu(&added, path).unwrap();
        assert!(available(on, &ui, Some(&added), &mut None));
    }
}
