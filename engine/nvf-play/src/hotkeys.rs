//! The player's hotkeys (docs/design/weapon-wheel.md §3, §4): eight slots on the keys
//! 1 to 8, the second of them Ammo Swap.
//!
//! - **What a slot holds**: one carried item the ITEMS page can equip or use (a weapon,
//!   apparel, an aid item, a book that teaches), with the mod flags of the unit it came
//!   from, so a modded gun comes back as the modded one [documented: menus\prefabs\
//!   hotkeys.xml, eight boxes; hypothesis: which kinds can be hotkeyed]. Slot 2 is Ammo
//!   Swap and holds nothing [documented: the same file puts `IM_AmmoSwap` in its place].
//!   An item is in one slot at most: putting it in another moves it [hypothesis].
//! - **Assigning**: on the Pip-Boy's ITEMS page, hold a hotkey's key and left-click an
//!   item (or press Enter on the selected one); the hotkey panel shows while the key is
//!   held (`crate::pipboy::hotkey_panel`) [design, the owner 2026-10-26: the game opens
//!   its panel with a Hotkey button (`IM_HotkeyButton`) instead].
//! - **Using a slot** ([`use_slot`]): as Enter on the ITEMS page, except that the weapon in
//!   hand stays in hand [design: no unequip by accident]: equip a weapon, put on or take
//!   off apparel, use one aid item. A slot whose item is no longer carried stays, drawn at
//!   half strength on the wheel, and does nothing [design until a capture says].
//!   A weapon equipped from a slot (a key or the wheel) comes in holstered: one in hand
//!   is put away first, and the player draws the new one with Ready Item [design, the
//!   owner 2026-10-31, #1106; it may not match the original]. With a weapon drawn the
//!   equip waits until its holster clip has run, so the old weapon is the one seen and
//!   heard going away (`crate::weapon_wheel`'s `SwapWait`).
//! - **Saved** under `hotkeys` in the engine's part of a save ([`SAVE_HOOK`]).
//!
//! Log lines: `hotkeys: slot <n> holds <name> (<form>)`, `hotkeys: slot <n>: <outcome>`.

use crate::bindings::Control;
use crate::save::SaveHook;
use bevy::prelude::*;
use fnv_sim::forms::{FormStore, ItemKind};
use fnv_sim::world::SimError;
use fnv_sim::{FormId, Pick, World};
use serde::{Deserialize, Serialize};

/// The number of slots.
pub const SLOTS: usize = 8;
/// The slot that is Ammo Swap (key 2) [documented: hotkeys.xml].
pub const AMMO_SWAP: usize = 1;
/// Each slot's control, slot 0 first (key 1).
pub const CONTROLS: [Control; SLOTS] = [Control::Hotkey1, Control::AmmoSwap, Control::Hotkey3, Control::Hotkey4, Control::Hotkey5, Control::Hotkey6, Control::Hotkey7, Control::Hotkey8];

/// What a slot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    /// The item's base form.
    pub item: u32,
    /// The mod flags of the unit it was assigned from (`Extra::mods`).
    #[serde(default)]
    pub mods: u8,
}

/// The eight slots, slot 0 being key 1.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Hotkeys(pub [Option<Slot>; SLOTS]);

/// Why an item cannot go in a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Slot 2 is Ammo Swap.
    AmmoSwap,
    /// Not a weapon, apparel or aid item.
    NotUsable,
}

impl Hotkeys {
    /// Put `slot` in slot `index`; returns the slot it moved from, if it was in another.
    pub fn assign(&mut self, index: usize, slot: Slot, kind: Option<ItemKind>) -> Result<Option<usize>, Refused> {
        if index == AMMO_SWAP {
            return Err(Refused::AmmoSwap);
        }
        if !kind.is_some_and(can_hold) {
            return Err(Refused::NotUsable);
        }
        let from = self.0.iter().position(|s| s.is_some_and(|s| s.item == slot.item && s.mods == slot.mods)).filter(|&i| i != index);
        if let Some(i) = from {
            self.0[i] = None;
        }
        self.0[index] = Some(slot);
        Ok(from)
    }
}

/// The kinds of item a slot can hold.
pub fn can_hold(kind: ItemKind) -> bool {
    matches!(kind, ItemKind::Weapon | ItemKind::Armor | ItemKind::Aid | ItemKind::Book)
}

/// The slot a control is, if it is a hotkey.
pub fn index_of(c: Control) -> Option<usize> {
    CONTROLS.iter().position(|&k| k == c)
}

/// What using a slot did.
#[derive(Debug, Clone, PartialEq)]
pub enum Used {
    /// Nothing in the slot.
    Empty,
    /// The slot's item is not carried.
    NotCarried,
    /// The weapon is already in hand.
    InHand,
    Equipped,
    TookOff,
    /// An aid item (or a book) was used up: its effects are the script runtime's
    /// (`crate::ItemUsed`).
    UsedUp(FormId),
    /// The simulation refused (broken, locked by a script, no Power Armor Training …).
    Refused(String),
}

impl Used {
    pub fn describe(&self, name: &str) -> String {
        match self {
            Used::Empty => "empty".into(),
            Used::NotCarried => format!("{name} is not carried"),
            Used::InHand => format!("{name} is already in hand"),
            Used::Equipped => format!("equipped {name}"),
            Used::TookOff => format!("took off {name}"),
            Used::UsedUp(_) => format!("used {name}"),
            Used::Refused(why) => format!("{name}: {why}"),
        }
    }
}

const PLAYER: FormId = FormId::PLAYER_REF;

/// How many of a slot's item the player carries: of its units with the slot's mods, else
/// of any.
pub fn carried(world: &mut World<'_>, slot: Slot) -> (i32, Pick) {
    let item = FormId(slot.item);
    let Ok(inv) = world.inventory_mut(PLAYER) else { return (0, Pick::ANY) };
    let exact: i32 = inv.groups().filter(|g| g.0 == item && g.1.mods == slot.mods).map(|g| g.2).sum();
    if exact > 0 {
        (exact, Pick::row(None, slot.mods))
    } else {
        (inv.count(item), Pick::ANY)
    }
}

/// Whether using the slot would put a new weapon in the player's hand: a carried weapon
/// that is not the one equipped (`use_slot` gives `Used::Equipped` for it).
pub fn takes_hand(world: &mut World<'_>, slot: Option<Slot>) -> bool {
    let Some(slot) = slot else { return false };
    let item = FormId(slot.item);
    if world.forms.item(item).map(|i| i.kind) != Some(ItemKind::Weapon) || carried(world, slot).0 <= 0 {
        return false;
    }
    !world.inventory_mut(PLAYER).is_ok_and(|i| i.is_equipped(item))
}

/// Use the item in a slot (module notes).
pub fn use_slot(world: &mut World<'_>, slot: Option<Slot>) -> Used {
    let Some(slot) = slot else { return Used::Empty };
    let item = FormId(slot.item);
    let forms: &FormStore = world.forms;
    let Some(kind) = forms.item(item).map(|i| i.kind) else { return Used::NotCarried };
    let (count, pick) = carried(world, slot);
    if count <= 0 {
        return Used::NotCarried;
    }
    let worn = world.inventory_mut(PLAYER).is_ok_and(|i| i.is_equipped(item));
    if worn {
        return match kind {
            ItemKind::Armor => match world.unequip(PLAYER, item) {
                Ok(_) => Used::TookOff,
                Err(e) => Used::Refused(refusal(&e)),
            },
            _ => Used::InHand,
        };
    }
    match world.equip_at(PLAYER, item, pick) {
        Ok(e) if e.consumed => Used::UsedUp(item),
        Ok(_) => Used::Equipped,
        Err(e) => Used::Refused(refusal(&e)),
    }
}

fn refusal(e: &SimError) -> String {
    match e {
        SimError::Broken(_) => "broken".into(),
        SimError::EquipLocked { .. } => "locked by a script".into(),
        SimError::NoPowerArmorTraining(_) => "needs Power Armor Training".into(),
        other => other.to_string(),
    }
}

/// The slots in a save: eight entries, `null` for an empty one.
pub const SAVE_HOOK: SaveHook = SaveHook {
    key: "hotkeys",
    save: |w| {
        let h = w.get_resource::<Hotkeys>()?;
        h.0.iter().any(Option::is_some).then(|| serde_json::to_value(h.0).ok()).flatten()
    },
    load: |w, v| {
        let slots: [Option<Slot>; SLOTS] = serde_json::from_value(v.clone()).unwrap_or_default();
        w.insert_resource(Hotkeys(slots));
    },
    // Items by FormID: moved to the current load order; those of plugins no longer loaded
    // leave their slot empty.
    remap: Some(|v, r| {
        if let Some(a) = v.as_array_mut() {
            for s in a.iter_mut() {
                let id = s.get("item").and_then(serde_json::Value::as_u64).map(|n| n as u32);
                match id.and_then(|id| r.map(id)) {
                    Some(new) => s["item"] = serde_json::Value::from(new),
                    None => *s = serde_json::Value::Null,
                }
            }
        }
    }),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(item: u32) -> Slot {
        Slot { item, mods: 0 }
    }

    #[test]
    fn an_item_takes_one_slot_and_moves() {
        let mut h = Hotkeys::default();
        assert_eq!(h.assign(0, slot(10), Some(ItemKind::Weapon)), Ok(None));
        assert_eq!(h.assign(4, slot(10), Some(ItemKind::Weapon)), Ok(Some(0)));
        assert_eq!(h.0[0], None);
        assert_eq!(h.0[4], Some(slot(10)));
        // Again in the same slot: nothing moves.
        assert_eq!(h.assign(4, slot(10), Some(ItemKind::Weapon)), Ok(None));
        // The same gun with mods is another unit: both may be hotkeyed.
        assert_eq!(h.assign(5, Slot { item: 10, mods: 1 }, Some(ItemKind::Weapon)), Ok(None));
        assert!(h.0[4].is_some() && h.0[5].is_some());
    }

    #[test]
    fn slot_two_is_ammo_swap_and_misc_items_are_refused() {
        let mut h = Hotkeys::default();
        assert_eq!(h.assign(AMMO_SWAP, slot(10), Some(ItemKind::Weapon)), Err(Refused::AmmoSwap));
        assert_eq!(h.assign(2, slot(11), Some(ItemKind::Misc)), Err(Refused::NotUsable));
        assert_eq!(h.assign(2, slot(11), Some(ItemKind::Ammo)), Err(Refused::NotUsable));
        assert_eq!(h.assign(2, slot(12), None), Err(Refused::NotUsable));
        assert_eq!(h.assign(2, slot(13), Some(ItemKind::Aid)), Ok(None));
        assert_eq!(index_of(Control::AmmoSwap), Some(AMMO_SWAP));
        assert_eq!(index_of(Control::Hotkey8), Some(7));
    }

    #[test]
    fn the_save_keeps_the_slots_and_drops_gone_plugins() {
        let mut w = bevy::ecs::world::World::new();
        let mut h = Hotkeys::default();
        h.0[0] = Some(slot(0x0000_1234));
        h.0[6] = Some(Slot { item: 0x0500_0001, mods: 3 });
        w.insert_resource(h.clone());
        let v = (SAVE_HOOK.save)(&w).expect("slots saved");
        let mut back = bevy::ecs::world::World::new();
        (SAVE_HOOK.load)(&mut back, &v);
        assert_eq!(back.resource::<Hotkeys>(), &h);
        // Nothing hotkeyed: nothing saved.
        let mut empty = bevy::ecs::world::World::new();
        empty.insert_resource(Hotkeys::default());
        assert!((SAVE_HOOK.save)(&empty).is_none());
    }
}
