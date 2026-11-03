//! The UI script functions over the menus' tile tree ([`Game::ui_tree`], `fnv_ui`;
//! docs/design/ui-tile-tree.md §5, docs/spec-notes/menu-xml.md §5): GetUIFloat,
//! SetUIFloat, SetUIString, ModUIFloat, GetUIString, IsComponentLoaded, InjectUIXML,
//! InjectUIComponent, UnloadUIComponent, AddTileFromTemplate, MenuTapKey, with their Alt
//! and Ex forms; and the pointer's (task T8, `crate::ui_input`): GetActiveUIComponentName,
//! GetActiveUIComponentFullName, GetActiveUIComponentID, ClickMenuButton, ClickMenuTile.
//!
//! [`call`] takes a function by name, as the script extender's `Host::ext_call` gives
//! it (`crate::nvse::call` hands the names no other family takes here). Arguments are taken as the VM
//! gives them; the VM formats a format string and its values (SetUIStringEx,
//! SetUIStringAlt, InjectUIComponent, AddTileFromTemplate) before the call, so the text
//! arrives here in their place.
//!
//! The deprecated forms (SetUIFloat, GetUIFloat, SetUIString, SetUIStringEx) behave as
//! their Alt forms for now: the wiki calls them "bugged for templated children tiles"
//! and says MCM depends on the bug, but the bug's page is not in the export
//! (menu-xml.md §8 item 5).

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_ui::{MemSource, Source, Value};

/// GetUIFloat's answer for a path that names no trait [documented: GetUIFloat, "Returns
/// -999 if UI trait was not found"].
pub const NOT_FOUND: f64 = -999.0;

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        Some(XVal::Num(n)) => Value::Num(*n).text(),
        _ => String::new(),
    }
}

fn num(args: &[XVal], k: usize) -> f64 {
    match args.get(k) {
        Some(XVal::Num(n)) => *n,
        Some(XVal::Str(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(if b { 1.0 } else { 0.0 })
}

/// The functions [`call`] takes that may read or change the HUD's traits, in lower case:
/// their first call on the HUD ([`on_hud`]) makes the HUD's kept-back writes
/// (`crate::ui_hud::flush`). Not MenuTapKey, and not MCM's shorthands, which reach
/// `StartMenu/MCM/...` only, so a mod's settings menu does not start the HUD's per-frame
/// tree writes [design].
const TREE_FUNCTIONS: [&str; 15] = [
    "getuifloat", "getuifloatalt", "getuifloatinherited", "getuistring", "setuifloat", "setuifloatalt", "setuistring", "setuistringalt", "setuistringex", "moduifloat",
    "iscomponentloaded", "injectuixml", "injectuicomponent", "unloaduicomponent", "addtilefromtemplate",
];

/// The menu whose writes wait while nothing reads them (`crate::ui_hud::defer`).
const HUD_MENU: &str = "HUDMainMenu";

/// Whether a path is in the HUD: its first segment, the menu, is `HUDMainMenu` (any case;
/// segments split on `/` or `\` as `fnv_ui` splits them). A call on another menu neither
/// reads nor changes the HUD's traits, so it leaves the HUD's writes waiting [design:
/// maintainer's review of #902, #937].
pub(crate) fn on_hud(path: &str) -> bool {
    path.split(['/', '\\']).map(str::trim).find(|s| !s.is_empty()).is_some_and(|menu| menu.eq_ignore_ascii_case(HUD_MENU))
}

/// The most clicks one ClickMenuButton makes: a larger `clickTimes` makes this many, so a
/// huge count (`1e9`) cannot hang the frame [design: the maintainer's review of #957; the
/// page gives no limit].
pub const MAX_CLICK_TIMES: usize = 16;

/// ClickMenuButton's and ClickMenuTile's clicks. A click on the HUD changes its traits, so
/// it makes the HUD's kept-back writes first, as the tree functions do; one on another
/// menu leaves them waiting [design: #937's rule, by the tile's menu].
fn click(g: &mut Game, tile: Option<fnv_ui::TileId>, times: usize) -> bool {
    let Some(tile) = tile else { return false };
    if times == 0 {
        return false;
    }
    let root = g.ui_tree.root_of(tile);
    if g.ui_tree.tile(root).name.eq_ignore_ascii_case(HUD_MENU) {
        g.ui_used = true;
        crate::ui_hud::flush(g);
    }
    for _ in 0..times {
        g.ui_pointer.script_click(&mut g.ui_tree, tile);
    }
    true
}

/// The tile and trait name MCM's functions stand for: `child` and `grandchild` of the
/// `StartMenu/MCM` rect, counted from 1 in file order, 0 stopping at the level above
/// (`0 0` is the MCM rect itself, `1 0` its first child) [hypothesis, from MCM's files and
/// a mod's script, menu-xml.md §10: Option1..Option36 are the first children of
/// MCM_Options, MCM's first child, each with `_optionID` 1..36; FOV Slider reads
/// `GetMCMFloat 0 0 "_ActiveMod"`, a trait of the MCM rect, and `GetMCMFloat 1 0
/// "_optionID"`, the one MCM_Options works out from its rows]. The wiki gives `SetMCMFloat
/// 1 1` as `SetUIFloat "StartMenu/MCM/*:1/*:1/…"` [documented]; NVForever's `*:N` counts
/// from the last child [observed on the HUD, menu-xml.md §5], so it is not used here.
fn mcm_trait(ui: &fnv_ui::Ui, args: &[XVal]) -> Option<(fnv_ui::TileId, String)> {
    let mut at = ui.find_tile("StartMenu/MCM")?;
    for k in [0, 1] {
        let i = num(args, k) as i64;
        if i <= 0 {
            break;
        }
        let kids: Vec<_> = ui.tile(at).children.iter().copied().filter(|&c| ui.is_live(c)).collect();
        at = *kids.get(i as usize - 1)?;
    }
    Some((at, text(args, 2)))
}

/// GetUIFloatInherited: `x`, `y` and `depth` with the values of every parent tile whose
/// `locus` is on added, up to the menu's root, which `exclude_root` leaves out; any other
/// trait as GetUIFloatAlt reads it [documented: GetUIFloatInherited; observed: hud.md §14,
/// the page's example `HUDMainMenu/RadiationMeter/radiation_text/depth` -50, and 0 with
/// ExcludeMenuRoot]. Whether a parent without `locus` stops the sum is open
/// [hypothesis: it does not; the page counts "all parent tiles with an active locus"].
fn inherited(ui: &fnv_ui::Ui, path: &str, exclude_root: bool) -> Option<f64> {
    let (tile_path, name) = path.rsplit_once(['/', '\\'])?;
    if !["x", "y", "depth"].iter().any(|t| t.eq_ignore_ascii_case(name)) {
        return ui.get_float(path);
    }
    // A tile without the trait counts 0 [observed: radiation_text has no depth in
    // hud_main_menu.xml, and the example still gives -50].
    let mut id = ui.find_tile(tile_path)?;
    let own = ui.get_float(path).unwrap_or(0.0);
    let screen = ui.screen();
    let mut sum = own;
    while let Some(parent) = ui.tile(id).parent.filter(|&p| p != screen) {
        let p = ui.tile(parent);
        let root = p.parent.is_none_or(|pp| pp == screen);
        let locus = p.get("locus").is_some_and(|v| v.num() != 0.0);
        if locus && !(root && exclude_root) {
            sum += p.get(name).map_or(0.0, |v| v.num());
        }
        id = parent;
    }
    Some(sum)
}

/// The UI function `name` (any case) with its arguments; `None` when it is not one of
/// these.
pub fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let lname = name.to_ascii_lowercase();
    let path = text(args, 0);
    // The path names the trait or tile read or changed, the tile injected into or
    // unloaded; AddTileFromTemplate's is before its first `|` [documented:
    // AddTileFromTemplate].
    let target = if lname == "addtilefromtemplate" { path.split('|').next().unwrap_or("") } else { path.as_str() };
    if TREE_FUNCTIONS.contains(&lname.as_str()) && on_hud(target) {
        // The HUD's writes kept back while nothing read the HUD, made now; the engine
        // writes them every frame from here on (`crate::ui_hud::defer`) [design].
        g.ui_used = true;
        crate::ui_hud::flush(g);
    }
    Some(Ok(match lname.as_str() {
        "getuifloat" | "getuifloatalt" => XVal::Num(g.ui_tree.get_float(&path).unwrap_or(NOT_FOUND)),
        "getuifloatinherited" => XVal::Num(inherited(&g.ui_tree, &path, num(args, 1) != 0.0).unwrap_or(NOT_FOUND)),
        "getuistring" => XVal::Str(g.ui_tree.get_string(&path).unwrap_or_default()),
        "setuifloat" | "setuifloatalt" => yes(g.ui_tree.set_float(&path, num(args, 1))),
        "setuistring" | "setuistringalt" | "setuistringex" => yes(g.ui_tree.set_string(&path, &text(args, 1))),
        "moduifloat" => yes(g.ui_tree.mod_float(&path, num(args, 1))),
        // MCM's shorthands for the traits of `StartMenu/MCM/*:child/*:grandchild/setting`
        // [documented: SetMCMFloat and SetMCMString give the SetUIFloat and SetUIString
        // they equal]. A trait that is not there reads 0, not GetUIFloat's -999 [observed:
        // the mcm set, GetMCMFloat 1 1 "_enable" 0 and GetUIFloat on StartMenu -999 with
        // the start menu closed]; SetMCMFloat returns 0 [hypothesis].
        "getmcmfloat" => XVal::Num(mcm_trait(&g.ui_tree, args).and_then(|(t, n)| g.ui_tree.tile(t).get(&n).map(|v| v.num())).unwrap_or(0.0)),
        "setmcmfloat" => {
            if let Some((t, n)) = mcm_trait(&g.ui_tree, args) {
                g.ui_tree.set_existing(t, &n, fnv_ui::value::Value::Num(num(args, 3)));
            }
            XVal::Num(0.0)
        }
        "setmcmstring" => {
            if let Some((t, n)) = mcm_trait(&g.ui_tree, args) {
                g.ui_tree.set_existing(t, &n, fnv_ui::value::Value::Str(text(args, 3)));
            }
            XVal::Num(0.0)
        }
        "iscomponentloaded" => yes(g.ui_tree.is_loaded(&path)),
        "injectuixml" => {
            let file = text(args, 1);
            match g.ui_files.as_deref() {
                Some(src) => yes(g.ui_tree.inject_file(src, &path, &file)),
                None => yes(false),
            }
        }
        "injectuicomponent" => {
            let xml = text(args, 1);
            let empty = MemSource::default();
            let src: &dyn Source = match g.ui_files.as_deref() {
                Some(s) => s,
                None => &empty,
            };
            yes(g.ui_tree.inject_text(src, &path, &xml))
        }
        "unloaduicomponent" => match g.ui_tree.find_tile(&path) {
            Some(t) => {
                g.ui_tree.unload_tile(t);
                yes(true)
            }
            None => yes(false),
        },
        // "componentPath|templateName|altTileName" [documented: AddTileFromTemplate].
        "addtilefromtemplate" => {
            let mut parts = path.splitn(3, '|');
            let (at, template) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let alt = parts.next().map(str::trim).filter(|s| !s.is_empty());
            let empty = MemSource::default();
            let src: &dyn Source = match g.ui_files.as_deref() {
                Some(s) => s,
                None => &empty,
            };
            yes(g.ui_tree.add_from_template(src, at.trim(), template.trim(), alt).is_some())
        }
        "menutapkey" => {
            g.ui_taps.push(num(args, 0).max(0.0) as u32);
            XVal::Num(0.0)
        }
        // The tile the pointer (or a key) highlights in a menu drawn from tiles
        // (`Game::ui_pointer`): its name as the file writes it, its full path for
        // GetUIFloat, its `<id>` [documented: GetActiveUIComponentName,
        // GetActiveUIComponentFullName, GetActiveUIComponentID]. With none highlighted: an
        // empty string and -1 [hypothesis: -1 as SetOnMenuClickEventHandler passes for a
        // tile without an id; observer check]. Not tree functions: they read the
        // pointer's state, not the HUD's traits.
        "getactiveuicomponentname" => XVal::Str(g.ui_pointer.active(&g.ui_tree).map(|t| g.ui_tree.tile(t).name.clone()).unwrap_or_default()),
        "getactiveuicomponentfullname" => XVal::Str(g.ui_pointer.active(&g.ui_tree).map(|t| g.ui_tree.path_of(t)).unwrap_or_default()),
        "getactiveuicomponentid" => XVal::Num(g.ui_pointer.active(&g.ui_tree).map_or(-1.0, |t| crate::ui_input::tile_id(&g.ui_tree, t))),
        // [documented: PrintActiveTile, "Prints name of highlighted UI component for debug
        // purposes"]: the highlighted tile's name on the console; nothing with none
        // [hypothesis: the console's line is not documented, so the name alone].
        "printactivetile" => {
            if let Some(t) = g.ui_pointer.active(&g.ui_tree) {
                let n = g.ui_tree.tile(t).name.clone();
                g.console_out.push(n);
            }
            XVal::Num(0.0)
        }
        // `"Menu\Tile\…" [clickTimes]` or `"Menu#<id>"`; the tile must be a target and
        // have an `<id>` [documented: ClickMenuButton, its notes]. Clicks as a press does
        // (`fnv_ui::input::Pointer::script_click`), as many times as asked (1 when not
        // given, none for 0 or less) [hypothesis], at most [`MAX_CLICK_TIMES`] [design]; 1
        // when it clicked.
        "clickmenubutton" => {
            let times = if args.len() > 1 { num(args, 1).clamp(0.0, MAX_CLICK_TIMES as f64) as usize } else { 1 };
            let tile = fnv_ui::input::find(&g.ui_tree, &path).filter(|&t| fnv_ui::input::targets(&g.ui_tree, t) && g.ui_tree.tile(t).get("id").is_some());
            yes(click(g, tile, times))
        }
        // Any tile that takes input (`target` true) [documented: ClickMenuTile, "should work
        // with any interactable UI tile"], hidden or not [documented: Tutorial:
        // Understanding XML, a hidden element "can still be activated through script"];
        // `Menu#<id>` too [design]. The page gives no return; 1 when it clicked
        // [hypothesis].
        "clickmenutile" => {
            let tile = fnv_ui::input::find(&g.ui_tree, &path).filter(|&t| fnv_ui::input::targets(&g.ui_tree, t));
            yes(click(g, tile, 1))
        }
        _ => return None,
    }))
}
