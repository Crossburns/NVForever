//! The game strings table: the text of `&-sName;`, the game setting `sName`
//! (docs/spec-notes/menu-xml.md §2.4; task T10).
//!
//! Two places answer a name, in this order:
//!
//! 1. the load order's string game settings (GMST records, the last plugin's record
//!    winning), read from the owner's plugins at run time and never shipped;
//! 2. [`BUILT_IN`]: NVForever's own English labels for the settings no plugin defines
//!    [observed: 144 of the 145 names of the vanilla menu files have no GMST record in
//!    FalloutNV.esm or the nine official add-ons, 2026-10-29].
//!
//! A name in neither has no text: the reader puts in nothing, as the original reads an
//! unknown name as an empty string. Names are compared without case: both spellings the
//! vanilla files use (`sOK`/`sOk`, `sQuit`/`squit`) read alike in the original
//! [observed: menu-xml.md §2.4].
//!
//! The engine's own menus that stand in for menu files (the tutorial menu, the lockpick
//! menu and its help) read their words from here too, by the names their files use, so
//! the table is the one place those words live.

use crate::xml::Node;
use std::collections::{BTreeSet, HashMap};

/// One label: the setting's name, NVForever's English text, and where the text came from
/// (the setting's name and the file and place that use it).
pub type Entry = (&'static str, &'static str, &'static str);

/// NVForever's labels for the game settings the vanilla menu files use and the plugins
/// leave undefined, by setting name, one entry per name without case.
///
/// [design: NVForever's own labels, written from the name and the menu's use;
/// maintainer's decision 2026-10-30, LEGAL.md] Each label is written from two things
/// only: the setting's name and where the menu files use it (the tile's kind, its menu,
/// its neighbours, its place). None is looked up in the original (no screenshots, no
/// setting readings, no lists of values), and none is changed on purpose: a short
/// generic label is the word the use calls for. Menu files' comments are not copied.
/// One more source is allowed [owner decision 2026-11-06, #1740, LEGAL.md]: a name or
/// term the data files' own shown text already uses, spelled as the data spells it, with
/// the record cited in the entry (`sStatsSpecial`'s "S.P.E.C.I.A.L.").
///
/// Left out on purpose:
/// - `sMorphine`: FalloutNV.esm has its GMST, so the load order answers it;
/// - `sModffects` and `sReputationSwap`: the original reads them empty too [observed:
///   menu-xml.md §2.4, which names the original fills in, not what they say], so they
///   stay empty here; a plugin's GMST still fills them.
///
/// `sExplosive` stands only in a comment of `hud_main_menu.xml`; its entry is for mods
/// that bring the text back.
pub const BUILT_IN: &[Entry] = &[
    ("sAccept", "Accept", "setting name + barter_menu.xml, recipe_menu.xml, vats_menu.xml: a button"),
    ("sAcceptText", "Accept", "setting name + caravan_bet_menu.xml: a button"),
    ("sAddText", "Add", "setting name + caravan_builddeck_menu.xml: a button"),
    ("sAllCardsText", "All Cards", "setting name + caravan_builddeck_menu.xml: a button"),
    ("sAntePotText", "Pot", "setting name + caravan_game_menu.xml: a title beside its value"),
    ("sAutoAttempt", "Auto Attempt", "setting name + surgerymenu.xml: a button"),
    ("sAutoMatchText", "Match Bet", "setting name + caravan_bet_menu.xml: a button"),
    ("sBack", "Back", "setting name + levelup_menu.xml, start_menu.xml: a button"),
    ("sCancel", "Cancel", "setting name + quantity_menu.xml, sleep_wait_menu.xml, inventory_menu.xml and others: a button"),
    ("sCaravanBiggestAnteText", "Largest Ante", "setting name + caravan_results_menu.xml: a title beside its value"),
    ("sCaravanLossesText", "Losses", "setting name + caravan_results_menu.xml: a title beside its value"),
    ("sCaravanWinLossText", "Wins / Losses", "setting name + caravan_results_menu.xml: a title beside its value"),
    ("sCaravanWinningsText", "Winnings", "setting name + caravan_results_menu.xml: a title beside its value"),
    ("sCardsInDeckText", "Cards in Deck", "setting name + caravan_builddeck_menu.xml: a title beside its value"),
    ("sCauterize", "Cauterize", "setting name + surgerymenu.xml: a button"),
    ("sCloseButton", "Close", "setting name + tutorial_menu.xml: the button at the foot of the box"),
    ("sConfirmWarning", "Please Confirm", "setting name + start_menu.xml: the confirm box's heading over its question"),
    ("sContinue", "Continue", "setting name + levelup_menu.xml, main_menu.xml, pause_menu.xml: a button"),
    ("sCrew", "Credits", "setting name + main_menu.xml: the list entry for the people who made the game"),
    ("sCurrentAnteText", "Your Bet", "setting name + caravan_bet_menu.xml: the player's bet title beside its value"),
    ("sCurrentObjective", "Current Objective", "setting name + loading_menu.xml: the label over the objective's text"),
    ("sCWheelTitle", "Companion", "setting name + companion_wheel_menu.xml: the wheel's centre title"),
    ("sData", "Data", "setting name + map_menu.xml: the page's headline title"),
    ("sDevice", "Storage Device", "setting name + start_menu.xml: a button (the storage device)"),
    ("sDiscardCardText", "Discard Card", "setting name + caravan_game_menu.xml: a button"),
    ("sDiscardTrackText", "Discard Caravan", "setting name + caravan_game_menu.xml: a button (a track is one of the player's caravans)"),
    ("sDone", "Done", "setting name + levelup_menu.xml, trait_menu.xml, repair_services_menu.xml, char_gen_menu.xml: a button"),
    ("sEnterName", "Type a name", "setting name + texteditmenu.xml: the prompt over the text field"),
    ("sExit", "Exit", "setting name + barter_menu.xml, container_menu.xml, lockpick_menu.xml and others: a button"),
    ("sExplosive", "Explosive", "setting name + hud_main_menu.xml: a HUD text inside a comment (for mods that restore it)"),
    ("sForfeitGameText", "Forfeit", "setting name + caravan_game_menu.xml: a button"),
    ("sHackingLockout3", "LOCKED OUT", "setting name + hacking_menu.xml: the locked message's first line"),
    ("sHackingLockout4", "Too many failed attempts", "setting name + hacking_menu.xml: the locked message's second line"),
    ("sHitPointsShort", "HP", "setting name + inventory_menu.xml, reputation_menu.xml: a narrow headline card's title"),
    ("sHowMany", "How many?", "setting name + quantity_menu.xml: the question over the count"),
    ("sIngredients", "Ingredients", "setting name + recipe_menu.xml: the list's title"),
    ("sInventoryCaps", "Caps", "setting name + barter_menu.xml, inventory_menu.xml, reputation_menu.xml: a label and a headline card"),
    ("sInventoryCondition", "Condition", "setting name + item_mod_menu.xml, repair_menu.xml: the item's condition bar card"),
    ("sInventoryDamage", "Damage", "setting name + item_stats_display.xml: an item figure's card"),
    ("sInventoryDamagePerSecond", "DPS", "setting name + item_stats_display.xml: an item figure's card"),
    ("sInventoryDamageResistance", "DR", "setting name + inventory_menu.xml (a 110-wide headline card), item_stats_display.xml"),
    ("sInventoryDamageThreshold", "DT", "setting name + inventory_menu.xml (a 110-wide headline card), item_stats_display.xml"),
    ("sInventoryDrop", "Drop", "setting name + inventory_menu.xml: a button"),
    ("sInventoryEffects", "Effects", "setting name + item_stats_display.xml: an item figure's card"),
    ("sInventoryEquip", "Equip", "setting name + inventory_menu.xml: a button"),
    ("sInventoryHotKey", "Hotkey", "setting name + inventory_menu.xml: a button"),
    ("sInventoryItems", "Items", "setting name + barter_menu.xml, container_menu.xml, item_mod_menu.xml, repair_menu.xml: a title"),
    ("sInventoryMod", "Modify", "setting name + inventory_menu.xml, item_mod_menu.xml: a button"),
    ("sInventoryRepair", "Repair", "setting name + inventory_menu.xml: a button"),
    ("sInventoryStrReq", "Strength Req.", "setting name + item_stats_display.xml: an item figure's card (the strength needed)"),
    ("sInventoryValue", "Value", "setting name + item_stats_display.xml: an item figure's card"),
    ("sInventoryWeight", "Weight", "setting name + inventory_menu.xml, reputation_menu.xml: a headline card"),
    ("sInventoryWeightUpper", "WEIGHT", "setting name + item_stats_display.xml: an item figure's card (the upper-case form)"),
    ("sLevelProgress", "Level Progress", "setting name + loading_menu.xml: the label over the level bar"),
    ("sLevelUp", "Level Up!", "setting name + hud_main_menu.xml: the HUD's level-up text"),
    ("sLoad", "Load", "setting name + main_menu.xml, pause_menu.xml: a list entry"),
    ("sLoadSavedGame", "Load Game", "setting name + load_menu.xml: the corner title"),
    ("sLockLevelText", "Lock Difficulty", "setting name + lockpick_menu.xml: an information row's title"),
    ("sLockpickSkillText", "Your Lockpick Skill", "setting name + lockpick_menu.xml: an information row's title"),
    ("sMadeAt", "Crafted At", "setting name + recipe_menu.xml: a title beside the workbench's name"),
    ("sMakeActiveQuest", "Set Active", "setting name + map_menu.xml: a page's A button title"),
    ("sMedicineSkill", "Medicine", "setting name + surgerymenu.xml: the skill's card"),
    ("sNavigate", "Navigate", "setting name + companion_wheel_menu.xml, trait_select_menu.xml: a key callout"),
    ("sNavigateText", "Navigate", "setting name + caravan_builddeck_menu.xml: a key callout"),
    ("sNew", "New Game", "setting name + main_menu.xml: a list entry"),
    ("sNext", "Next", "setting name + tutorial_menu.xml: the first button of the right column"),
    ("sNextChallenge", "Next Challenge", "setting name + map_menu.xml: a page's X button title"),
    ("sNoItemsToRepair", "Nothing to repair.", "setting name + repair_services_menu.xml: the empty list's text"),
    ("sOK", "OK", "setting name + quantity_menu.xml (sOK), texteditmenu.xml (sOk): a button"),
    ("sPCMenuHintA", "[A]", "setting name + text_box.xml PCShortcutLabel: the key after a button's words"),
    ("sPCMenuHintE", "[E]", "setting name + text_box.xml PCShortcutLabel: the key after a button's words"),
    ("sPCMenuHintF", "[F]", "setting name + text_box.xml PCShortcutLabel: the key after a button's words"),
    ("sPCMenuHintQ", "[Q]", "setting name + text_box.xml PCShortcutLabel: the key after a button's words"),
    ("sPCMenuHintR", "[R]", "setting name + text_box.xml PCShortcutLabel: the key after a button's words"),
    ("sPCMenuHintRMB", "[Right Click]", "setting name + vats_menu.xml, text_box.xml PCShortcutLabel: the right mouse button after Return"),
    ("sPCMenuHintS", "[S]", "setting name + text_box.xml PCShortcutLabel: the key after a button's words"),
    ("sPCMenuHintW", "[W]", "setting name + text_box.xml PCShortcutLabel: the key after a button's words"),
    ("sPCMenuHintX", "[X]", "setting name + text_box.xml PCShortcutLabel: the key after a button's words"),
    ("sPicksRemainingText", "Picks Left", "setting name + lockpick_menu.xml: an information row's title"),
    ("sPlaceMarker", "Place Marker", "setting name + map_menu.xml: a page's X button title"),
    ("sPlayAudio", "Play", "setting name + map_menu.xml: a page's A button title"),
    ("sPlayCaravanText", "Play", "setting name + caravan_builddeck_menu.xml: a button"),
    ("sPressStart", "Press any key to begin", "setting name + start_menu.xml: the start screen's line"),
    ("sPrevious", "Previous", "setting name + tutorial_menu.xml: the second button of the right column"),
    ("sQuit", "Quit", "setting name + main_menu.xml (sQuit), pause_menu.xml (squit): a list entry"),
    ("sRaiseText", "Raise", "setting name + caravan_bet_menu.xml: a button"),
    ("sRecipes", "Recipes", "setting name + recipe_menu.xml: the list's title"),
    ("sRedeemCode", "Redeem Code", "setting name + start_menu.xml: a button"),
    ("sRemoveText", "Remove", "setting name + caravan_builddeck_menu.xml: a button"),
    ("sRepairAllItems", "Repair All", "setting name + repair_services_menu.xml: a button"),
    ("sRepairItem", "Repair", "setting name + repair_menu.xml, repair_services_menu.xml: a button"),
    ("sRepairServicesTitle", "Repairs", "setting name + repair_services_menu.xml: the headline title"),
    ("sRepairSkill", "Repair Skill", "setting name + repair_menu.xml: the player's skill headline card"),
    ("sReset", "Reset", "setting name + levelup_menu.xml, trait_menu.xml, char_gen_menu.xml: a button"),
    ("sReturn", "Return", "setting name + vats_menu.xml: a button"),
    ("sSave", "Save", "setting name + pause_menu.xml: a list entry"),
    ("sSaveGame", "Save Game", "setting name + save_menu.xml: the corner title"),
    ("sSelect", "Select", "setting name + companion_wheel_menu.xml, trait_select_menu.xml: a key callout"),
    ("sSelectCardText", "Select Card", "setting name + caravan_game_menu.xml: a button"),
    ("sSettings", "Settings", "setting name + main_menu.xml, pause_menu.xml: a list entry"),
    ("sShowActiveNotes", "Active Notes", "setting name + map_menu.xml: a page's X button title"),
    ("sShowAllNotes", "All Notes", "setting name + map_menu.xml: a page's X button title"),
    ("sShowLocation", "Show Location", "setting name + map_menu.xml: a page's X button title"),
    ("sSkillRequirement", "Skill Needed", "setting name + recipe_menu.xml: a title beside its value"),
    ("sSpace", " ", "setting name + load_menu.xml, save_menu.xml, button_basic.xml: a blank default item text"),
    ("sStatsAP", "AP", "setting name + stats_menu.xml: a headline card"),
    ("sStatsCNDAbbrev", "CND", "setting name + stats_menu.xml: a page tab"),
    ("sStatsCrippled", "Crippled", "setting name + stats_menu.xml: a limb row's state"),
    ("sStatsEFFAbbrev", "EFF", "setting name + stats_menu.xml: a page tab"),
    ("sStatsFODAbbrev", "FOD", "setting name + stats_menu.xml (a page tab), hud_main_menu.xml (the Hardcore word)"),
    ("sStatsGeneral", "General", "setting name + stats_menu.xml: a page's tail line"),
    ("sStatsH20Abbrev", "H2O", "setting name + stats_menu.xml (a page tab), hud_main_menu.xml (the Hardcore word)"),
    ("sStatsHP", "HP", "setting name + stats_menu.xml, surgerymenu.xml: a headline card"),
    ("sStatsLMBAbbrev", "LMB", "setting name + hud_main_menu.xml: the crippled-limb word"),
    ("sStatsLVLAbbrev", "LVL", "setting name + stats_menu.xml: a headline card"),
    ("sStatsNoEffects", "No effects", "setting name + stats_menu.xml: the empty effects list's text"),
    ("sStatsPerks", "Perks", "setting name + stats_menu.xml: a page's tail line"),
    ("sStatsRADAbbrev", "RAD", "setting name + stats_menu.xml (a page tab), hud_main_menu.xml (the radiation word)"),
    ("sStatsRadResist", "Rad Resist", "setting name + stats_menu.xml: the label beside the level"),
    ("sStatsSkills", "Skills", "setting name + stats_menu.xml: a page's tail line"),
    ("sStatsSLPAbbrev", "SLP", "setting name + stats_menu.xml (a page tab), hud_main_menu.xml (the Hardcore word)"),
    ("sStatsSpecial", "S.P.E.C.I.A.L.", "setting name + stats_menu.xml: a page's tail line; the system's name as FalloutNV.esm's own text spells it [observed: PERK IntenseTraining's description, MESG HelpDrugs, MESG CG04VaultExitMessage's button \"Edit S.P.E.C.I.A.L.\"] (LEGAL.md, owner 2026-11-06, #1740)"),
    ("sStatsStatus", "Status", "setting name + stats_menu.xml: a page's tail line"),
    ("sStatsXP", "XP", "setting name + stats_menu.xml (a headline card), hud_main_menu.xml (the XP label)"),
    ("sStimpak", "Use Stimpak", "setting name + surgerymenu.xml: a button"),
    ("sStopAudio", "Stop", "setting name + map_menu.xml: a page's A button title"),
    ("sSwitchLimbs", "Switch Limb", "setting name + surgerymenu.xml: a button"),
    ("sTakeAll", "Take All", "setting name + container_menu.xml: a button"),
    ("sTotalCardsText", "Total Cards", "setting name + caravan_builddeck_menu.xml: a title beside its value"),
    ("sTotalFundsText", "Funds", "setting name + caravan_bet_menu.xml: a title beside its value"),
    ("sTravel", "Travel", "setting name + map_menu.xml: a page's A button title"),
    ("sTune", "Tune In", "setting name + map_menu.xml: a page's A button title"),
    ("sVatsAiming", "Aiming...", "setting name + vats_menu.xml: the text while aiming"),
    ("sVatsBodyPart", "Body Part", "setting name + vats_menu.xml: a button"),
    ("sVatsSelect", "Select", "setting name + vats_menu.xml: a button"),
    ("sVatsTarget", "Target", "setting name + vats_menu.xml: a button"),
    ("sVDSGManual", "Survival Guide", "setting name + tutorial_menu.xml: the words after the symbol at the foot of the box"),
    ("sVDSGPlate", "Plate", "setting name + loading_menu.xml: the label before the plate's number"),
    ("sWinLossText", "Win / Loss", "setting name + caravan_game_menu.xml: a title beside its value"),
    ("sXPProgressMarker", "|", "setting name + loading_menu.xml: a one-character marker over the level bar"),
];

/// The label [`BUILT_IN`] holds for a setting, without case.
pub fn built_in(name: &str) -> Option<&'static str> {
    built_in_in(BUILT_IN, name)
}

fn built_in_in(table: &'static [Entry], name: &str) -> Option<&'static str> {
    table.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(name)).map(|&(_, label, _)| label)
}

/// Where a name's text came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A plugin's GMST record.
    LoadOrder,
    /// [`BUILT_IN`] (or the table a test passes).
    BuiltIn,
}

/// The table one tree reads from: the load order's string settings over a built-in table.
#[derive(Clone, Debug, Default)]
pub struct GameStrings {
    /// By lower-case name.
    settings: HashMap<String, String>,
    built_in: &'static [Entry],
}

impl GameStrings {
    /// The load order's string settings, in load order (a later one of the same name
    /// replaces an earlier, as a later plugin's record wins), over [`BUILT_IN`].
    pub fn from_settings<N: AsRef<str>, V: Into<String>>(settings: impl IntoIterator<Item = (N, V)>) -> GameStrings {
        GameStrings::with_built_in(settings, BUILT_IN)
    }

    /// As [`GameStrings::from_settings`] over another built-in table (tests).
    pub fn with_built_in<N: AsRef<str>, V: Into<String>>(
        settings: impl IntoIterator<Item = (N, V)>,
        built_in: &'static [Entry],
    ) -> GameStrings {
        let settings = settings.into_iter().map(|(n, v)| (n.as_ref().to_ascii_lowercase(), v.into())).collect();
        GameStrings { settings, built_in }
    }

    /// The text of `&-name;` and where it came from, or `None` when neither the load
    /// order nor the built-in table has the name.
    pub fn lookup(&self, name: &str) -> Option<(&str, Origin)> {
        if let Some(v) = self.settings.get(&name.to_ascii_lowercase()) {
            return Some((v, Origin::LoadOrder));
        }
        built_in_in(self.built_in, name).map(|v| (v, Origin::BuiltIn))
    }

    /// The text of `&-name;`, if any.
    pub fn get(&self, name: &str) -> Option<String> {
        self.lookup(name).map(|(v, _)| v.to_owned())
    }
}

/// The game string names (`sName` of `&-sName;`) a file's nodes use, as written.
pub fn names_in(nodes: &[Node]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack: Vec<&[Node]> = vec![nodes];
    while let Some(nodes) = stack.pop() {
        for n in nodes {
            match n {
                Node::Entity(e) => {
                    if let Some(name) = e.strip_prefix('-') {
                        out.insert(name.to_owned());
                    }
                }
                Node::Element(e) => stack.push(&e.children),
                Node::Text(_) => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic names and text only: no game text in tests (charter).
    const TABLE: &[Entry] = &[("sNvfBuiltOnly", "built one", "test"), ("sNvfBoth", "built two", "test")];

    #[test]
    fn load_order_first_then_built_in_last_plugin_wins() {
        let s = GameStrings::with_built_in([("sNvfBoth", "first plugin"), ("SNVFBOTH", "last plugin"), ("sNvfPlugin", "p")], TABLE);
        assert_eq!(s.lookup("snvfboth"), Some(("last plugin", Origin::LoadOrder)));
        assert_eq!(s.lookup("sNvfBuiltOnly"), Some(("built one", Origin::BuiltIn)));
        assert_eq!(s.get("SNVFPLUGIN").as_deref(), Some("p"));
        assert_eq!(s.lookup("sNvfNowhere"), None);
        assert_eq!(GameStrings::default().get("sNvfBoth"), None);
    }

    #[test]
    fn the_shipped_table_is_one_labelled_entry_per_name() {
        // NVForever's own labels (maintainer's decision 2026-10-30, menu-xml.md §2.4),
        // tested as data: one entry per name without case, each with a label and where
        // it came from.
        let mut seen = std::collections::BTreeSet::new();
        for &(name, label, from) in BUILT_IN {
            assert!(name.starts_with('s') && name.len() > 1 && name.chars().all(|c| c.is_ascii_alphanumeric()), "{name}");
            assert!(seen.insert(name.to_ascii_lowercase()), "{name} twice");
            assert!(!label.is_empty(), "{name} has no label");
            assert!(from.starts_with("setting name + ") && from.contains(".xml"), "{name}: {from}");
        }
        // The masters define sMorphine; the original leaves sModffects and
        // sReputationSwap empty, so they have no entry.
        for name in ["sMorphine", "sModffects", "sReputationSwap"] {
            assert_eq!(built_in(name), None, "{name}");
        }
        // The maintainer's examples, and lookups without case.
        assert_eq!(built_in("sCloseButton"), Some("Close"));
        assert_eq!(built_in("SNEXT"), Some("Next"));
        assert_eq!(built_in("sOk"), built_in("sOK"));
        assert_eq!(built_in("squit"), built_in("sQuit"));
        assert_eq!(built_in("sNvfNowhere"), None);
    }

    #[test]
    fn the_load_order_wins_over_the_shipped_table() {
        let s = GameStrings::from_settings([("SCLOSEBUTTON", "plugin text")]);
        assert_eq!(s.lookup("sCloseButton"), Some(("plugin text", Origin::LoadOrder)));
        assert_eq!(s.lookup("sNext"), Some(("Next", Origin::BuiltIn)));
        assert_eq!(s.lookup("sModffects"), None);
    }

    #[test]
    fn names_in_a_file() {
        let nodes = crate::xml::parse(
            "<menu name=\"M\"><text name=\"t\"><string>&-sNvfOne; and &-sNvfTwo;</string><x><copy>&-sNvfOne;</copy></x></text><visible>&true;</visible></menu>",
        )
        .nodes;
        let names: Vec<_> = names_in(&nodes).into_iter().collect();
        assert_eq!(names, ["sNvfOne", "sNvfTwo"]);
    }
}
