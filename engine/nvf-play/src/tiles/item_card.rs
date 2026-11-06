//! T15c: the selected item's figures written into a file's `item_stats_display.xml` cards
//! [documented: the prefab; the container, barter, inventory menus include it]. What a card
//! says comes from [`Info`] (`crate::pipboy::items`, `crate::menu::look::item_figures`), so
//! the hand-built drawing and the tiles agree; `tiles_in`'s hand-built places are not read.
//!
//! - A card shows through its `visible`, its figure through `_Value` ("Set in code" in the
//!   prefab), its title from the file but for `AmmoInfo`'s, the ammunition or the armour's
//!   class [observed: `AmmoInfo` title "9mm (13/37)", "Heavy", `_Value` empty].
//! - The titles' words are the file's, never written here: the load order's setting where a
//!   plugin defines it, else `fnv_ui::strings::BUILT_IN`'s label ("WEIGHT", "Value",
//!   "Condition" ...) [design: the string rule, LEGAL.md; the words the original shows on
//!   screen are not transcribed]. Those labels are wider than the card's place for them
//!   (`ConditionMeter` stands 62 in, a title 5 in [documented], and "WEIGHT" alone measures
//!   72 units [observed: this menu at 1280 x 720]); the fit is left open (barter-menu.md, "The
//!   words on the tiles").
//! - The condition: `ConditionInfo`'s `user5` 0 to 1, which its meter reads, and `user6` 1 for
//!   a weapon, 0 for armour [observed], with `CNDArrows` at y 15, 20 high once a card is
//!   written [observed; what it looks like there is an open observer check].
//! - `DAMInfo` and `DPSInfo` stand at one place in the file [documented]. The original's
//!   `DAMInfo` reads `alpha` from 0 to 255 between reads of one weapon [observed], so it
//!   seems to fade between the two [hypothesis]; the fade is not built. As the hand-built
//!   figures show them, `DPSInfo` stands a card above `DAMInfo` [owner's playtest list 7
//!   item 25] and `DAMInfo` at full strength.
//! - The effects stand on the first free row, as the hand-built figures have them
//!   (`crate::pipboy::items::tiles_in`) [observed: owner's screenshots, pipboy.md]. Mods'
//!   cards (`ModInfoOne` ...) are not written: not in [`Info`].
//! - Nothing selected: every card hidden, but `StrengthReqInfo` as the file has it, shown at
//!   `alpha` 0 [observed: at the opening of the menu its lines show and no words].

use crate::pipboy::items::Info;
use fnv_ui::{Ui, Value};

/// The cards this writes, under the prefab's root [documented: item_stats_display.xml].
pub const CARDS: [&str; 8] = ["DAMInfo", "DamageThresholdInfo", "DPSInfo", "WeightInfo", "ValueInfo", "StrengthReqInfo", "ConditionInfo", "AmmoInfo"];
/// The card of the effects' text, written with the cards.
pub const EFFECTS: &str = "EffectsInfo";
/// The condition's arrows under `ConditionInfo`.
pub const CONDITION_ARROWS: &str = "ConditionInfo/CNDArrows";
/// The condition's bar under `ConditionInfo`, which `user5` fills.
pub const CONDITION_METER: &str = "ConditionInfo/ConditionMeter";
/// What draws the meter's fill [documented: prefabs/meter.xml, which the card includes].
pub const CONDITION_METER_IMAGE: &str = "ConditionInfo/ConditionMeter/MeterImage";
/// A card's words [documented: card_info.xml]: `_Title` and `_Value` show only through them.
pub const CARD_TEXTS: [&str; 2] = ["CI_TitleText", "CI_ValueText"];

fn set(ui: &mut Ui, root: &str, card: &str, name: &str, v: Value) {
    if let Some(t) = ui.find_tile(&format!("{root}/{card}")) {
        ui.set_trait(t, name, v);
    }
}

fn show(ui: &mut Ui, root: &str, card: &str, value: Option<&str>) {
    set(ui, root, card, "visible", Value::Num(f64::from(u8::from(value.is_some()))));
    set(ui, root, card, "_Value", Value::Str(value.unwrap_or_default().to_string()));
}

/// Write what the cards under `root` show of `info`, or the empty state with `None`. Every
/// card is written, so nothing stays of the item before.
pub fn write(ui: &mut Ui, root: &str, info: Option<&Info>) {
    let Some(i) = info else {
        for card in CARDS.into_iter().chain([EFFECTS]) {
            show(ui, root, card, None);
        }
        set(ui, root, "AmmoInfo", "_Title", Value::Str(String::new()));
        set(ui, root, "ConditionInfo", "user5", Value::Num(0.0));
        set(ui, root, "ConditionInfo", "user6", Value::Num(0.0));
        show(ui, root, "StrengthReqInfo", Some(""));
        set(ui, root, "StrengthReqInfo", "alpha", Value::Num(0.0));
        return;
    };
    let first = |name: &str| i.first.as_ref().filter(|(t, _)| *t == name).map(|(_, v)| v.as_str());
    show(ui, root, "DAMInfo", first("DAM"));
    set(ui, root, "DAMInfo", "alpha", Value::Num(255.0));
    show(ui, root, "DamageThresholdInfo", first("DT"));
    show(ui, root, "DPSInfo", i.dps.as_deref());
    // One card up, out of the DAM card's place (module notes).
    let high = ui.get_float(&format!("{root}/user1")).unwrap_or(0.0);
    set(ui, root, "DPSInfo", "y", Value::Num(-high));
    show(ui, root, "WeightInfo", Some(i.weight.as_deref().unwrap_or("--")));
    show(ui, root, "ValueInfo", Some(i.value.as_deref().unwrap_or("--")));
    let strength = i.strength.map(|s| s.to_string());
    show(ui, root, "StrengthReqInfo", strength.as_deref());
    set(ui, root, "StrengthReqInfo", "alpha", Value::Num(255.0));
    let weapon = i.dps.is_some() || i.strength.is_some() || matches!(&i.first, Some((t, _)) if *t == "DAM");
    show(ui, root, "ConditionInfo", i.condition.map(|_| ""));
    set(ui, root, "ConditionInfo", "user5", Value::Num(f64::from(i.condition.unwrap_or(0.0).clamp(0.0, 1.0))));
    set(ui, root, "ConditionInfo", "user6", Value::Num(f64::from(u8::from(weapon && i.condition.is_some()))));
    set(ui, root, CONDITION_ARROWS, "y", Value::Num(15.0));
    set(ui, root, CONDITION_ARROWS, "height", Value::Num(20.0));
    show(ui, root, "AmmoInfo", i.beside.as_ref().map(|_| ""));
    set(ui, root, "AmmoInfo", "_Title", Value::Str(i.beside.clone().unwrap_or_default()));
    show(ui, root, EFFECTS, i.effects.as_deref());
    let row = if i.condition.is_some() || i.beside.is_some() { 2.0 } else { 1.0 };
    set(ui, root, EFFECTS, "y", Value::Num(row * high));
}

/// A line of the visible cards, for the log (smoke reads it).
pub fn describe(ui: &Ui, root: &str) -> String {
    let mut out = Vec::new();
    for card in CARDS.into_iter().chain([EFFECTS]) {
        let path = format!("{root}/{card}");
        if ui.get_float(&format!("{path}/visible")).is_some_and(|v| v != 0.0) {
            let title = ui.get_string(&format!("{path}/_Title")).unwrap_or_default();
            let value = ui.get_string(&format!("{path}/_Value")).unwrap_or_default();
            out.push(format!("{title} {value}").trim().to_string());
        }
    }
    out.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    const ROOT: &str = "M/D";

    fn card(name: &str, extra: &str) -> String {
        format!(r#"<rect name="{name}"><_Title>{name}</_Title><_Value></_Value><visible>&false;</visible>{extra}</rect>"#)
    }

    fn card_titled(name: &str, title: &str) -> String {
        format!(r#"<rect name="{name}"><_Title>{title}</_Title><_Value></_Value><visible>&false;</visible></rect>"#)
    }

    fn ui() -> Ui {
        let cards: String = [
            card("DAMInfo", "<alpha>0</alpha>"),
            card("DamageThresholdInfo", ""),
            card("DPSInfo", ""),
            card("WeightInfo", ""),
            card("ValueInfo", ""),
            card("StrengthReqInfo", "<alpha>0</alpha>"),
            card("ConditionInfo", r#"<image name="CNDArrows"><y>17</y><height>16</height></image>"#),
            card("AmmoInfo", ""),
            card("EffectsInfo", ""),
        ]
        .concat();
        let xml = format!(r#"<menu name="M"><rect name="D"><user1>40</user1>{cards}</rect></menu>"#);
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("m.xml", &xml), "m.xml").unwrap();
        ui.update();
        ui
    }

    fn pistol() -> Info {
        Info {
            first: Some(("DAM", "9".into())),
            dps: Some("34".into()),
            weight: Some("1.50".into()),
            value: Some("100".into()),
            strength: Some(2),
            condition: Some(0.6),
            beside: Some("9mm (13/37)".into()),
            ..Info::default()
        }
    }

    fn at(ui: &Ui, path: &str) -> f64 {
        ui.get_float(&format!("{ROOT}/{path}")).unwrap_or(-1.0)
    }

    #[test]
    fn a_weapon_writes_its_cards() {
        let mut ui = ui();
        write(&mut ui, ROOT, Some(&pistol()));
        ui.update();
        assert_eq!(describe(&ui, ROOT), "DAMInfo 9, DPSInfo 34, WeightInfo 1.50, ValueInfo 100, StrengthReqInfo 2, ConditionInfo, 9mm (13/37)");
        assert_eq!((at(&ui, "ConditionInfo/user5"), at(&ui, "ConditionInfo/user6")), (f64::from(0.6f32), 1.0));
        assert_eq!((at(&ui, "ConditionInfo/CNDArrows/y"), at(&ui, "ConditionInfo/CNDArrows/height")), (15.0, 20.0));
        assert_eq!((at(&ui, "StrengthReqInfo/alpha"), at(&ui, "DAMInfo/alpha")), (255.0, 255.0));
        assert_eq!(at(&ui, "DPSInfo/y"), -40.0);
    }

    #[test]
    fn titles_stay_the_files() {
        // The file's titles read settings: a plugin's and a built-in label alike stay as the
        // file has them, nothing written over them.
        let xml = format!(
            r#"<menu name="M"><rect name="D"><user1>40</user1>{}{}{}</rect></menu>"#,
            card_titled("WeightInfo", "&-sInventoryWeightUpper;"),
            card_titled("ConditionInfo", "&-sInventoryCondition;"),
            card_titled("DPSInfo", "&-sInventoryDamagePerSecond;"),
        );
        let loaded = |built_in: &[&str], xml: &str| {
            let mut src = MemSource::default().with("m.xml", xml);
            for (name, text) in [("sInventoryWeightUpper", "WEIGHT"), ("sInventoryCondition", "Condition"), ("sInventoryDamagePerSecond", "DPS")] {
                src.strings.insert(name.into(), text.into());
            }
            src.built_in_strings = built_in.iter().map(|n| n.to_ascii_lowercase()).collect();
            let mut ui = Ui::new();
            ui.load_menu(&src, "m.xml").unwrap();
            ui.update();
            write(&mut ui, ROOT, Some(&pistol()));
            ui.update();
            ui
        };
        let title = |ui: &Ui, card: &str| ui.get_string(&format!("{ROOT}/{card}/_Title")).unwrap_or_default();
        for built_in in [&["sInventoryWeightUpper", "sInventoryCondition", "sInventoryDamagePerSecond"][..], &[]] {
            let ui = loaded(built_in, &xml);
            assert_eq!(title(&ui, "WeightInfo"), "WEIGHT");
            assert_eq!(title(&ui, "ConditionInfo"), "Condition");
            assert_eq!(title(&ui, "DPSInfo"), "DPS");
        }
    }

    #[test]
    fn changing_item_clears_previous_card() {
        let mut ui = ui();
        write(&mut ui, ROOT, Some(&pistol()));
        // A misc item: no figures of its own but "--" and the value.
        let misc = Info { value: Some("30".into()), ..Info::default() };
        write(&mut ui, ROOT, Some(&misc));
        ui.update();
        assert_eq!(describe(&ui, ROOT), "WeightInfo --, ValueInfo 30");
        assert_eq!(ui.get_string(&format!("{ROOT}/AmmoInfo/_Title")).as_deref(), Some(""));
        assert_eq!(at(&ui, "ConditionInfo/user6"), 0.0);
        // Armour: DT, the class beside the condition, effects on the next row.
        let armour = Info { first: Some(("DT", "12".into())), condition: Some(0.8), beside: Some("Heavy".into()), effects: Some("AGL -1".into()), ..Info::default() };
        write(&mut ui, ROOT, Some(&armour));
        ui.update();
        assert_eq!(describe(&ui, ROOT), "DamageThresholdInfo 12, WeightInfo --, ValueInfo --, ConditionInfo, Heavy, EffectsInfo AGL -1");
        assert_eq!(at(&ui, "ConditionInfo/user6"), 0.0, "armour");
        assert_eq!(at(&ui, "EffectsInfo/y"), 80.0);
        // An aid item: effects on the first free row.
        write(&mut ui, ROOT, Some(&Info { effects: Some("HP +40".into()), ..Info::default() }));
        ui.update();
        assert_eq!(at(&ui, "EffectsInfo/y"), 40.0);
        // Nothing selected: the file's empty state, STR shown at alpha 0 with no words.
        write(&mut ui, ROOT, None);
        ui.update();
        assert_eq!(describe(&ui, ROOT), "StrengthReqInfo");
        assert_eq!(at(&ui, "StrengthReqInfo/alpha"), 0.0);
    }
}
