//! The ITEMS page's hotkey panel (docs/design/weapon-wheel.md §3): hold a hotkey's key
//! (1 to 8) and left-click an item in the list to put it in that slot.
//!
//! - **The panel** [documented: menus\main\inventory_menu.xml `IM_HotKeyWheel`, 380, 52
//!   in the menus' rectangle, 290 × 250, drawn further right ([`PANEL`]);
//!   menus\prefabs\hotkeys.xml, hotkey.xml]: eight 60-square boxes in its corners and the middles of its sides (key 1 top left, 2 top
//!   middle, 3 top right, 4 left, 5 right, 6 bottom left, 7 bottom middle, 8 bottom right),
//!   each a border 5 in with the slot's icon in it and its number at its right foot; the
//!   chosen slot's box filled (`_fill_alpha` 80). Key 2's place is the Ammo Swap picture,
//!   50 square, centred at the top. The compass rose (`hot_keys_rose.dds`, 150 square at
//!   100, 80) behind, the pointed-at item's icon in its middle (`IM_HotkeyIcon`, 120.5,
//!   102, 50 square) and a line of font 2 centred at 150, 262 (`IM_HotKeyText`). While
//!   it shows, the item's picture, skill badge and the Repair and Mod hints do not
//!   (`IM_ButtonRect` is hidden with it) [documented]; the figures stay [observed: the
//!   owner's screenshot of the original].
//! - **When it shows** [design, the owner 2026-10-26: "hold your key and left click your
//!   item to assign"; the original opens it with its Hotkey button instead]: while a
//!   hotkey's key is held on the ITEMS page. A left click on a row then puts that row's
//!   item in the held slot ([`crate::hotkeys::Hotkeys::assign`]) instead of selecting or
//!   using it; Enter does the same for the selected row. Letting go of the key hides it.
//! - **The line under it**: what the held slot holds ("Empty"), or what the last click
//!   did ("Can't hotkey Wonderglue", "Key 2 is Ammo Swap").
//!
//! Log lines: `pipboy hotkeys: key <n> held`, `pipboy hotkeys: let go`,
//! `hotkeys: slot <n> holds <name> (<form>)`.

use super::frame::{Align, Ctx, LINE, MAIN, TEXT, THICK};
use crate::bindings::Bindings;
use crate::hotkeys::{Hotkeys, Refused, Slot, AMMO_SWAP, SLOTS};
use crate::interface::{amber, at};
use crate::menu::model::Row;
use crate::sim::SimState;
use bevy::prelude::*;
use fnv_sim::forms::ItemKind;
use fnv_sim::FormId;

/// The panel inside the menus' rectangle: 290 × 250 at 52 down [documented:
/// `IM_HotKeyWheel`], but 420 in rather than the file's 380, 10 clear of the list's rows
/// (32 + 393 from its left at -15), where key 1's box met the selected row [design: the
/// owner 2026-10-27, "the hotkeys need moving right so they dont overlap the list"; further
/// right, the line under it runs into the STR card].
pub const PANEL: Rect = Rect { min: Vec2::new(420.0, 52.0), max: Vec2::new(710.0, 302.0) };
/// A slot's box, its border's inset and the Ammo Swap picture's side [documented: hotkey.xml,
/// hotkeys.xml].
const BOX: f32 = 60.0;
const INSET: f32 = 5.0;
const SWAP_SIDE: f32 = 50.0;
/// The chosen box's fill: alpha 80 of 255 [documented: `HK_SelectionBox` `_fill_alpha`].
const CHOSEN_ALPHA: f32 = 80.0 / 255.0;
/// The compass rose and the icon in its middle [documented: `IM_CompassRose`, `IM_HotkeyIcon`].
const ROSE: &str = "Interface\\HotKeys\\hot_keys_rose.dds";
const ROSE_AT: Rect = Rect { min: Vec2::new(100.0, 80.0), max: Vec2::new(250.0, 230.0) };
const CENTRE_ICON: Rect = Rect { min: Vec2::new(120.5, 102.0), max: Vec2::new(170.5, 152.0) };
/// The line under the panel, centred, font 2 [documented: `IM_HotKeyText`].
const TEXT_AT: Vec2 = Vec2::new(150.0, 262.0);
const TEXT_FONT: usize = 2;
const SWAP: &str = "Interface\\Icons\\typeicons\\hot_keys_icon_ammoswap.dds";

/// Which hotkey's key is held on the ITEMS page, and what the last click there did.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct HotkeyHold {
    /// The held key's slot (0 is key 1).
    pub slot: Option<usize>,
    /// What the last assignment did, shown under the panel until the key is let go.
    pub said: Option<String>,
    /// The test aid's held key, in place of the keyboard's (`super::test_aid`).
    pub aid: Option<usize>,
}

/// A slot box's top left inside the panel [documented: hotkeys.xml's `x`/`y` traits on a
/// panel of 290 × 250].
pub fn box_at(slot: usize) -> Vec2 {
    let (w, h) = (PANEL.width(), PANEL.height());
    let (left, mid, right) = (0.0, (w - BOX) / 2.0, w - BOX);
    let (top, middle, bottom) = (0.0, (h - BOX) / 2.0, h - BOX);
    match slot {
        0 => Vec2::new(left, top),
        1 => Vec2::new((w - SWAP_SIDE) / 2.0, top),
        2 => Vec2::new(right, top),
        3 => Vec2::new(left, middle),
        4 => Vec2::new(right, middle),
        5 => Vec2::new(left, bottom),
        6 => Vec2::new(mid, bottom),
        _ => Vec2::new(right, bottom),
    }
}

/// The hotkey type icon of an item [observed: the files in `textures\interface\icons\typeicons`;
/// the original's boxes show these, not the items' own pictures (owner's screenshot)].
pub fn type_icon(kind: ItemKind, skill: Option<&str>) -> &'static str {
    match (kind, skill.unwrap_or("")) {
        (ItemKind::Weapon, "EnergyWeapons") => "Interface\\Icons\\typeicons\\hot_keys_icon_energyweapons.dds",
        (ItemKind::Weapon, "MeleeWeapons") => "Interface\\Icons\\typeicons\\hot_keys_icon_meleeweapons.dds",
        (ItemKind::Weapon, "Unarmed") => "Interface\\Icons\\typeicons\\hot_keys_icon_unarmedweapons.dds",
        (ItemKind::Weapon, "Explosives") => "Interface\\Icons\\typeicons\\hot_keys_icon_throwweapons.dds",
        (ItemKind::Weapon, "BigGuns") => "Interface\\Icons\\typeicons\\hot_keys_icon_bigguns.dds",
        (ItemKind::Weapon, _) => "Interface\\Icons\\typeicons\\hot_keys_icon_smallguns.dds",
        (ItemKind::Armor, _) => "Interface\\Icons\\typeicons\\hot_keys_icon_bodywear.dds",
        (ItemKind::Book, _) => "Interface\\Icons\\typeicons\\hot_keys_icon_books.dds",
        _ => "Interface\\Icons\\typeicons\\hot_keys_icon_stimpack.dds",
    }
}

/// An item's hotkey icon, by its kind and (a weapon's) skill.
pub fn icon_of(id: FormId, sim: &SimState, combat: Option<&crate::combat::Combat>) -> Option<&'static str> {
    let kind = sim.forms.item(id)?.kind;
    let skill = combat.and_then(|c| c.forms.weapons.get(&id)).and_then(|w| w.skill.as_deref());
    Some(type_icon(kind, skill))
}

/// The slot whose key is held now, by the player's bindings (keys only).
fn held_slot(bindings: &Bindings, keys: &ButtonInput<KeyCode>) -> Option<usize> {
    crate::hotkeys::CONTROLS.iter().position(|&c| bindings.get(c).key.is_some_and(|k| keys.pressed(k)))
}

/// Follow the held key while the ITEMS page shows (the repair list and mod menu have the
/// keys while one is open).
pub fn track(
    pip: Res<super::PipBoy>,
    (repair, modding): (Res<super::repair::PipRepair>, Res<super::item_mod::PipItemMod>),
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Option<Res<Bindings>>,
    mut hold: ResMut<HotkeyHold>,
) {
    // Laid out in the Pip-Boy's screen texture, at its size whatever the UI scale (#1631).
    let _canvas = crate::interface::original_units();
    let items = pip.open() && pip.page == super::Page::Items && !repair.is_open() && !modding.is_open();
    let now = if items { hold.aid.or_else(|| bindings.as_ref().and_then(|b| held_slot(b, &keys))) } else { None };
    if now != hold.slot {
        match now {
            Some(s) => info!("pipboy hotkeys: key {} held", s + 1),
            None => info!("pipboy hotkeys: let go"),
        }
        hold.slot = now;
        hold.said = None;
    }
}

/// Put a row's item in the held slot; returns the line the panel then shows.
pub fn assign_row(hotkeys: &mut Hotkeys, slot: usize, r: &Row) -> String {
    let s = Slot { item: r.item.0, mods: r.mods };
    match hotkeys.assign(slot, s, r.kind) {
        Ok(from) => {
            let moved = from.map_or_else(String::new, |f| format!(" (moved from slot {})", f + 1));
            info!("hotkeys: slot {} holds {} ({}){moved}", slot + 1, r.name, r.item);
            r.name.clone()
        }
        Err(Refused::AmmoSwap) => {
            info!("hotkeys: slot 2 is Ammo Swap: {} not assigned", r.name);
            "Key 2 is Ammo Swap".into()
        }
        Err(Refused::NotUsable) => {
            info!("hotkeys: {} cannot be hotkeyed", r.name);
            format!("Can't hotkey {}", r.name)
        }
    }
}

/// The panel, at the menus' rectangle's `IM_HotKeyWheel`.
pub fn draw(p: &mut ChildSpawnerCommands, c: &mut Ctx, hotkeys: &Hotkeys, hold: &HotkeyHold, pointed: Option<&Row>, sim: &SimState, combat: Option<&crate::combat::Combat>) {
    let o = MAIN.min + PANEL.min;
    let (line, bright) = (amber(LINE), amber(TEXT));
    let rose = Rect::from_corners(o + ROSE_AT.min, o + ROSE_AT.max);
    c.image(p, ROSE, at(rose.min.x, rose.min.y, rose.width(), rose.height()), bright);
    if let Some(icon) = pointed.and_then(|r| icon_of(r.item, sim, combat)) {
        let r = Rect::from_corners(o + CENTRE_ICON.min, o + CENTRE_ICON.max);
        c.image(p, icon, at(r.min.x, r.min.y, r.width(), r.height()), bright);
    }
    for slot in 0..SLOTS {
        let b = o + box_at(slot);
        if slot == AMMO_SWAP {
            if hold.slot == Some(slot) {
                c.boxed(p, Rect::from_corners(b, b + Vec2::splat(SWAP_SIDE)), line, Some(bright.with_alpha(CHOSEN_ALPHA)));
            }
            c.image(p, SWAP, at(b.x, b.y, SWAP_SIDE, SWAP_SIDE), bright);
            continue;
        }
        if hold.slot == Some(slot) {
            c.boxed(p, Rect::from_corners(b, b + Vec2::splat(BOX)), line, Some(bright.with_alpha(CHOSEN_ALPHA)));
        }
        let inner = Rect::from_corners(b + Vec2::splat(INSET), b + Vec2::splat(BOX - INSET));
        c.boxed(p, inner, line, None);
        if let Some(icon) = hotkeys.0[slot].and_then(|s| icon_of(FormId(s.item), sim, combat)) {
            c.image(p, icon, at(inner.min.x + THICK, inner.min.y + THICK, inner.width() - 2.0 * THICK, inner.height() - 2.0 * THICK), bright);
        }
        // The number at the box's right foot: its bottom 5 under the box's [documented:
        // `HK_Number` y = height - its height + 5].
        let h = c.line_height(TEXT_FONT);
        c.text(p, TEXT_FONT, &(slot + 1).to_string(), b.x + BOX, b.y + BOX - h + INSET, Align::Left, bright);
    }
    let said = hold.said.clone().or_else(|| {
        let slot = hold.slot?;
        if slot == AMMO_SWAP {
            return Some("Ammo Swap".into());
        }
        Some(hotkeys.0[slot].map_or_else(|| "Empty".into(), |s| sim.name(FormId(s.item)).unwrap_or("?").to_string()))
    });
    if let Some(s) = said {
        c.text(p, TEXT_FONT, &s, o.x + TEXT_AT.x, o.y + TEXT_AT.y, Align::Centre, bright);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boxes_sit_as_the_file_places_them() {
        assert_eq!(box_at(0), Vec2::ZERO);
        assert_eq!(box_at(1), Vec2::new(120.0, 0.0));
        assert_eq!(box_at(2), Vec2::new(230.0, 0.0));
        assert_eq!(box_at(3), Vec2::new(0.0, 95.0));
        assert_eq!(box_at(4), Vec2::new(230.0, 95.0));
        assert_eq!(box_at(5), Vec2::new(0.0, 190.0));
        assert_eq!(box_at(6), Vec2::new(115.0, 190.0));
        assert_eq!(box_at(7), Vec2::new(230.0, 190.0));
    }

    #[test]
    fn every_kind_has_a_type_icon() {
        for k in [ItemKind::Weapon, ItemKind::Armor, ItemKind::Aid, ItemKind::Book, ItemKind::Misc] {
            assert!(type_icon(k, None).ends_with(".dds"));
        }
        assert!(type_icon(ItemKind::Weapon, Some("MeleeWeapons")).contains("melee"));
    }
}
