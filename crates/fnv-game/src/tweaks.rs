//! The player's tweaks (docs/design/tweaks.md): optional changes to how the game plays
//! or what it shows, off unless the player turns them on in the settings page's Tweaks
//! tab. The list follows the features of a popular script-extender plugin for the
//! original (Stewie's Tweaks), each built here from what its readme says the feature
//! does, in NVForever's own words and code; none of the plugin's code or files are used.
//! The engine keeps the chosen values and hands them to the running game each frame
//! (`nvf_play::tweaks`), as it does the difficulty; they are settings, not saved with a
//! game.
//!
//! A field's name is the plugin's INI key it stands for, in snake case, so the catalogue
//! in docs/design/tweaks.md can be followed both ways.

/// Every tweak's value. `Default` is the original's behaviour: every switch off, every
/// number at the value that changes nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Tweaks {
    // ---- experience ----
    /// `bReduceXP`/`fXPMultiplier`: every experience gain is multiplied by this (after
    /// the perks), 1 changes nothing.
    pub xp_multiplier: f64,
    // ---- items ----
    /// `bPowerArmorNeedsNoTraining`: power armour can be worn without the training perk.
    pub power_armor_needs_no_training: bool,
    // ---- rest ----
    /// `bSleepInOwnedBeds`: a bed that belongs to someone else does not refuse sleep.
    pub sleep_in_owned_beds: bool,
    // ---- screen ----
    /// `bRemoveChemWarnOffIMOD`: a chem wearing off plays no screen effect.
    pub no_chem_wore_off_effect: bool,
    // ---- menus (read through the game by the engine's menus) ----
    /// `bNoSkillMessageIfIncreaseIsZero`: reading a skill book that gives no points shows
    /// no message.
    pub no_book_message_without_gain: bool,
    /// `bCapitalizeRecipeCategories`: the crafting menu's categories in capitals.
    pub capitalize_recipe_categories: bool,
    /// `bShowWeaponAmmoUseInMenus`: a weapon that uses more than one round a shot says
    /// how many beside its ammo in the Pip-Boy.
    pub show_weapon_ammo_use: bool,
    /// `bShowBookEffects`: a skill book says what it teaches in the Pip-Boy.
    pub show_book_effects: bool,
    /// `bSkipSkillMenuIfNoPointsToAssign`: a level-up with no skill points to spend but
    /// a perk to choose opens on the perks.
    pub skip_skill_page_without_points: bool,
    /// `bFasterHackingTransition`: a successful hack opens the terminal sooner.
    pub faster_hacking_transition: bool,
    /// `bMapRemembersPosition`: the Pip-Boy map opens where it was left.
    pub remember_map_position: bool,
    // ---- character ----
    /// `iPerksPerLevel`: perks chosen at a level-up's perk page; 1 changes nothing.
    pub perks_per_level: usize,
    /// `bCustomSpecialPoints`/`iNumPointsToAllocate`: the S.P.E.C.I.A.L. menu's total
    /// when no script set one; 40 changes nothing.
    pub special_points: i32,
    // ---- locks, terminals, casinos ----
    /// `bPickLocksWithoutSkill`: a lock above the player's skill can be picked.
    pub pick_locks_without_skill: bool,
    /// `bHackTerminalsWithoutSkill`: a terminal above the player's skill can be hacked.
    pub hack_terminals_without_skill: bool,
    /// `bNoLockFailedTerminals`: a failed hack does not lock the terminal.
    pub no_terminal_lockout: bool,
    /// `bNoCasinoBans`: winning big never bans the player from a casino.
    pub no_casino_bans: bool,
    // ---- actors ----
    /// `bLootUnconsciousVictims`: an unconscious actor can be searched.
    pub loot_unconscious: bool,
    /// `bPickpocketWornItems`: what an actor wears can be pickpocketed.
    pub pickpocket_worn_items: bool,
    // ---- combat ----
    /// `bNoSneakAttacksCriticals`: the player's sneak attacks are not criticals.
    pub no_sneak_criticals: bool,
    /// `bDisableReloadingNonEmptyClip`: the player reloads only an empty magazine.
    pub reload_only_when_empty: bool,
    // ---- weight ----
    /// `bWeightlessItems`: item kinds that weigh nothing (`fnv_sim::inventory`'s
    /// `WEIGHTLESS_*` bits).
    pub weightless_kinds: u8,
    /// `bWeightlessWornArmor`: worn armour weighs nothing.
    pub weightless_worn_armor: bool,
    /// `bWeightlessWornPowerArmor`: worn power armour weighs nothing.
    pub weightless_worn_power_armor: bool,
}

impl Default for Tweaks {
    fn default() -> Tweaks {
        Tweaks {
            xp_multiplier: 1.0,
            power_armor_needs_no_training: false,
            sleep_in_owned_beds: false,
            no_chem_wore_off_effect: false,
            no_book_message_without_gain: false,
            capitalize_recipe_categories: false,
            show_weapon_ammo_use: false,
            show_book_effects: false,
            skip_skill_page_without_points: false,
            faster_hacking_transition: false,
            remember_map_position: false,
            perks_per_level: crate::levelup::PERKS_PER_LEVEL,
            special_points: crate::chargen::SPECIAL_POINTS,
            pick_locks_without_skill: false,
            hack_terminals_without_skill: false,
            no_terminal_lockout: false,
            no_casino_bans: false,
            loot_unconscious: false,
            pickpocket_worn_items: false,
            no_sneak_criticals: false,
            reload_only_when_empty: false,
            weightless_kinds: 0,
            weightless_worn_armor: false,
            weightless_worn_power_armor: false,
        }
    }
}
