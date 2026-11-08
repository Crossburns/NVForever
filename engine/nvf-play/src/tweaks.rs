//! The settings page's Tweaks tab (docs/design/tweaks.md): optional changes to how the
//! game plays or what it shows, each off (or at "no change") unless the player sets it.
//! The list follows the features of Stewie's Tweaks, a script-extender plugin for the
//! original, each written here from what the plugin's readme says it does; neither its
//! code nor its files are used, and the labels and notes are NVForever's own words. A
//! row's note names the plugin's INI key it stands for, so a player who knows the plugin
//! can find theirs. Rows are kept in `NVForever.ini` `[Settings]` as every row is
//! (`crate::settings`), under keys starting `tweak-`.
//!
//! The values live in [`TweakSettings`]: the game's rules' part (`fnv_game::tweaks`) is
//! handed to the running game each frame ([`sync`], as the difficulty is: the game is
//! made after the settings are read, and again for a loaded save); the engine's own
//! modules read the rest from the resource.

use crate::script::ScriptState;
use crate::settings::{add_setting, Setting, Slider};
use bevy::prelude::*;
use fnv_game::tweaks::Tweaks;

/// The settings page's group (its tab) for the tweaks.
pub const GROUP: &str = "Tweaks";

/// Every tweak's value. `Default`: the original's behaviour.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct TweakSettings {
    /// The part the game's rules read (`fnv_game::Game::tweaks`).
    pub game: Tweaks,
    /// `bNoKarmaMessages`: karma changes put up no corner message and play no sound.
    pub no_karma_messages: bool,
    /// `bNoXPMessages`: experience gains do not bring up the XP meter or its sound.
    pub no_xp_messages: bool,
    /// `bNoQuestFailedPopup`: no "Quest failed" banner.
    pub no_quest_failed_popup: bool,
    /// `bNoLocationPopup`: no banner when a place is discovered.
    pub no_location_popup: bool,
    /// `bRemoveQuestObjectiveAddedText`: no list of new objectives on the HUD.
    pub no_objective_popup: bool,
    /// `bDisableFastTravel`: fast travel is always refused.
    pub disable_fast_travel: bool,
    /// `bMidairFastTravel`: fast travel is not refused for being in the air.
    pub midair_fast_travel: bool,
    /// `bFastTravelWithEnemiesNearby`: fast travel is not refused for enemies nearby.
    pub fast_travel_enemies_nearby: bool,
    /// `bFastTravelOverencumbered`: fast travel is not refused for carrying too much.
    pub fast_travel_overencumbered: bool,
    /// `bNoRedCrosshairOnEnemies`: the crosshair does not turn red over a hostile actor.
    pub no_red_crosshair: bool,
    /// `bHideEnemyMarkers`: hostile actors are not shown on the compass.
    pub hide_enemy_markers: bool,
    /// `bDisableCombatMusic`: no battle music.
    pub no_combat_music: bool,
    /// `bDisableLoadingScreenTips`: loading screens show no tip.
    pub no_loading_tips: bool,
    /// `bNoReputationMessages`: reputation changes put up no box and play no sound.
    pub no_reputation_messages: bool,
    /// `bDisableExplosionInFaceIMOD`: a nearby explosion does not blur the screen.
    pub no_explosion_blur: bool,
    /// `bRemoveSneakLabel`: no [HIDDEN] / [DETECTED] label while sneaking.
    pub no_sneak_label: bool,
    /// `bVatsExitKey`: a right click ends a kill camera.
    pub kill_cam_exit_key: bool,
    /// `bNumberedComputerHotkeys`: 1 to 9 and 0 choose a terminal's rows.
    pub numbered_terminal_keys: bool,
    /// `bDeselectHotkeys`: assigning an item to the hotkey it has takes it off.
    pub deselect_hotkeys: bool,
    /// `bScaleMusicVolumeDuringDialogue`/`fDialogueMusicVolumeMult`: the music's volume
    /// in a conversation, as a part of its own; 1 changes nothing.
    pub dialogue_music_volume: f32,
    /// `bScaleRadioSongVolumeDuringDialogue`/`fDialogueSongVolumeMult`: the radio's.
    pub dialogue_radio_volume: f32,
    /// `bNonSelectablePlayerInConsole`: a console click never picks the player.
    pub console_skips_player: bool,
    /// `bPipboyLightAndHolsteringIgnoreTimescale`: holding Ready Item to holster counts
    /// real time (the Pip-Boy light already does).
    pub holster_real_time: bool,
    /// `bPowerArmorPreventsScreenBlood`: no blood on the screen in power armour.
    pub power_armor_no_screen_blood: bool,
    /// `bHideRedCrosshairOnDistantInvisibleTargets`: an invisible foe does not turn the
    /// crosshair red.
    pub no_red_crosshair_on_cloaked: bool,
    /// `bGodmodePreventsLegCrippleSound`: no hard-landing sound for the player in god mode.
    pub god_mode_quiet_landing: bool,
    /// `bCompressDuplicateConsoleMessages`: a console line said again right after
    /// itself is counted on the same line.
    pub console_compress_repeats: bool,
    /// `bConsolePrintsIncludeTimestamp`: console lines start with the time since the
    /// game started.
    pub console_timestamps: bool,
    /// `bLivingAnatomyShowDR`: Living Anatomy's line shows the DR too.
    pub living_anatomy_dr: bool,
    /// `bSynchronizeContainerCategories`: a container's or barter's two lists change
    /// category together.
    pub sync_categories: bool,
}

impl Default for TweakSettings {
    fn default() -> TweakSettings {
        TweakSettings {
            game: Tweaks::default(),
            no_karma_messages: false,
            no_xp_messages: false,
            no_quest_failed_popup: false,
            no_location_popup: false,
            no_objective_popup: false,
            disable_fast_travel: false,
            midair_fast_travel: false,
            fast_travel_enemies_nearby: false,
            fast_travel_overencumbered: false,
            no_red_crosshair: false,
            hide_enemy_markers: false,
            no_combat_music: false,
            no_loading_tips: false,
            no_reputation_messages: false,
            no_explosion_blur: false,
            no_sneak_label: false,
            kill_cam_exit_key: false,
            numbered_terminal_keys: false,
            deselect_hotkeys: false,
            dialogue_music_volume: 1.0,
            dialogue_radio_volume: 1.0,
            console_skips_player: false,
            holster_real_time: false,
            power_armor_no_screen_blood: false,
            no_red_crosshair_on_cloaked: false,
            god_mode_quiet_landing: false,
            console_compress_repeats: false,
            console_timestamps: false,
            living_anatomy_dr: false,
            sync_categories: false,
        }
    }
}

/// The value of a tweak, for modules that only have the world.
pub fn tweaks(world: &World) -> TweakSettings {
    world.get_resource::<TweakSettings>().cloned().unwrap_or_default()
}

/// An on/off row of the Tweaks tab for the field `$($f).+` of [`TweakSettings`].
macro_rules! switch {
    ($app:expr, $key:literal, $label:literal, $note:literal, $($f:ident).+) => {
        add_setting(
            $app,
            Setting {
                key: concat!("tweak-", $key),
                group: GROUP,
                label: $label,
                note: $note,
                get: |w| w.get_resource::<TweakSettings>().is_some_and(|t| t.$($f).+),
                set: |w, on| {
                    if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                        t.$($f).+ = on;
                    }
                },
                slider: None,
            },
        )
    };
}

/// A multiplier as a slider: the part `v` is the factor `2v` (0 to 2 in tenths, 1 in the
/// middle).
fn factor(v: f32) -> f64 {
    f64::from(v) * 2.0
}

fn part(f: f64) -> f32 {
    (f / 2.0).clamp(0.0, 1.0) as f32
}

fn shown_factor(v: f32) -> String {
    format!("x{:.1}", factor(v))
}

/// A slider row of the Tweaks tab for a part (0 to 100% in fives) in the field
/// `$($f).+` of [`TweakSettings`].
macro_rules! part {
    ($app:expr, $key:literal, $label:literal, $note:literal, $($f:ident).+) => {
        add_setting(
            $app,
            Setting::slider(
                concat!("tweak-", $key),
                GROUP,
                $label,
                $note,
                Slider {
                    get: |w| w.get_resource::<TweakSettings>().map_or(1.0, |t| t.$($f).+),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.$($f).+ = v;
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: None,
                },
            ),
        )
    };
}

pub struct TweaksPlugin;

impl Plugin for TweaksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TweakSettings>().add_systems(Update, sync.in_set(crate::PlaySet::Input));
        // ---- experience ----
        add_setting(
            app,
            Setting::slider(
                "tweak-xp-multiplier",
                GROUP,
                "Experience Earned",
                "Every experience gain is multiplied by this, after the perks; x1.0 changes nothing. (Stewie's Tweaks bReduceXP)",
                Slider {
                    get: |w| part(w.get_resource::<TweakSettings>().map_or(1.0, |t| t.game.xp_multiplier)),
                    set: |w, v| {
                        if let Some(mut t) = w.get_resource_mut::<TweakSettings>() {
                            t.game.xp_multiplier = factor(v);
                        }
                    },
                    min: 0.0,
                    step: 0.05,
                    shown: Some(shown_factor),
                },
            ),
        );
        switch!(app, "no-xp-messages", "Quiet Experience", "Experience gains do not bring up the XP meter or its sound; levelling up still does. (Stewie's Tweaks bNoXPMessages)", no_xp_messages);
        // ---- messages ----
        switch!(app, "no-karma-messages", "Quiet Karma", "Karma changes show no corner message and play no sound. (Stewie's Tweaks bNoKarmaMessages)", no_karma_messages);
        switch!(app, "no-quest-failed", "No Quest Failed Banner", "A failed quest puts up no banner; the Pip-Boy still lists it. (Stewie's Tweaks bNoQuestFailedPopup)", no_quest_failed_popup);
        switch!(app, "no-location-popup", "No Location Banner", "Discovering a place puts up no banner; it is still marked on the map. (Stewie's Tweaks bNoLocationPopup)", no_location_popup);
        switch!(app, "no-objective-popup", "No Objective List", "New and completed quest objectives are not listed on the HUD. (Stewie's Tweaks bRemoveQuestObjectiveAddedText)", no_objective_popup);
        // ---- fast travel ----
        switch!(app, "disable-fast-travel", "No Fast Travel", "Fast travel is always refused. (Stewie's Tweaks bDisableFastTravel)", disable_fast_travel);
        switch!(app, "midair-fast-travel", "Fast Travel in the Air", "Being in the air does not stop fast travel. (Stewie's Tweaks bMidairFastTravel)", midair_fast_travel);
        switch!(app, "fast-travel-enemies", "Fast Travel Near Enemies", "Enemies nearby do not stop fast travel. (Stewie's Tweaks bFastTravelWithEnemiesNearby)", fast_travel_enemies_nearby);
        switch!(app, "fast-travel-encumbered", "Fast Travel Encumbered", "Carrying too much does not stop fast travel, as with Long Haul. (Stewie's Tweaks bFastTravelOverencumbered)", fast_travel_overencumbered);
        // ---- HUD ----
        switch!(app, "no-red-crosshair", "No Red Crosshair", "The crosshair keeps its colour over hostile actors. (Stewie's Tweaks bNoRedCrosshairOnEnemies)", no_red_crosshair);
        switch!(app, "hide-enemy-markers", "No Enemies on the Compass", "Hostile actors are left off the compass. (Stewie's Tweaks bHideEnemyMarkers)", hide_enemy_markers);
        switch!(app, "no-sneak-label", "No Sneak Label", "Sneaking shows no Hidden / Detected label on the HUD. (Stewie's Tweaks bRemoveSneakLabel)", no_sneak_label);
        switch!(app, "no-reputation-messages", "Quiet Reputation", "Reputation changes put up no box and play no sound; the Pip-Boy still shows them. (Stewie's Tweaks bNoReputationMessages)", no_reputation_messages);
        // ---- sound and screen ----
        switch!(app, "no-combat-music", "No Battle Music", "Fights do not switch the music to battle tracks. (Stewie's Tweaks bDisableCombatMusic)", no_combat_music);
        switch!(app, "no-loading-tips", "No Loading Tips", "Loading screens show their picture without the tip. (Stewie's Tweaks bDisableLoadingScreenTips)", no_loading_tips);
        switch!(app, "no-explosion-blur", "No Explosion Blur", "A blast close by does not blur the screen. (Stewie's Tweaks bDisableExplosionInFaceIMOD)", no_explosion_blur);
        switch!(app, "no-chem-wore-off", "No Chem Wore-Off Effect", "A chem wearing off plays no screen effect. (Stewie's Tweaks bRemoveChemWarnOffIMOD)", game.no_chem_wore_off_effect);
        // ---- VATS ----
        switch!(app, "kill-cam-exit-key", "Skip Kill Camera", "A right click ends a kill camera at once. (Stewie's Tweaks bVatsExitKey)", kill_cam_exit_key);
        // ---- menus ----
        switch!(app, "skip-skill-page", "Perks First When No Points", "A level-up with no skill points to spend opens on the perks. (Stewie's Tweaks bSkipSkillMenuIfNoPointsToAssign)", game.skip_skill_page_without_points);
        switch!(app, "no-empty-book-message", "No Message for Spent Books", "A skill book that gives no points (the skill at its top) shows no message. (Stewie's Tweaks bNoSkillMessageIfIncreaseIsZero)", game.no_book_message_without_gain);
        switch!(app, "recipe-caps", "Recipe Categories in Capitals", "The crafting menu's categories are shown in capitals. (Stewie's Tweaks bCapitalizeRecipeCategories)", game.capitalize_recipe_categories);
        switch!(app, "weapon-ammo-use", "Rounds per Shot", "A weapon that uses more than one round a shot says how many beside its ammo in the Pip-Boy. (Stewie's Tweaks bShowWeaponAmmoUseInMenus)", game.show_weapon_ammo_use);
        switch!(app, "book-effects", "Skill Book Effects", "A skill book says which skill it raises and by how much in the Pip-Boy. (Stewie's Tweaks bShowBookEffects)", game.show_book_effects);
        switch!(app, "deselect-hotkeys", "Unassign Hotkeys", "Assigning an item to the hotkey it already has takes it off. (Stewie's Tweaks bDeselectHotkeys)", deselect_hotkeys);
        switch!(app, "map-remembers", "Map Remembers Its Place", "The Pip-Boy maps open where they were left. (Stewie's Tweaks bMapRemembersPosition)", game.remember_map_position);
        switch!(app, "terminal-number-keys", "Number Keys on Terminals", "1 to 9 and 0 choose a terminal's rows. (Stewie's Tweaks bNumberedComputerHotkeys)", numbered_terminal_keys);
        switch!(app, "faster-hacking", "Faster Hacking Exit", "A successful hack opens the terminal after half a second. (Stewie's Tweaks bFasterHackingTransition)", game.faster_hacking_transition);
        // ---- conversations ----
        part!(app, "dialogue-music", "Music While Talking", "The music's volume in a conversation, as a part of its own; 100% changes nothing. (Stewie's Tweaks bScaleMusicVolumeDuringDialogue)", dialogue_music_volume);
        part!(app, "dialogue-radio", "Radio While Talking", "The radio's volume in a conversation, as a part of its own; 100% changes nothing. (Stewie's Tweaks bScaleRadioSongVolumeDuringDialogue)", dialogue_radio_volume);
        // ---- batch 4 ----
        switch!(app, "console-skips-player", "Console Clicks Skip the Player", "Clicking in the console never selects the player. (Stewie's Tweaks bNonSelectablePlayerInConsole)", console_skips_player);
        switch!(app, "holster-real-time", "Holster in Real Time", "Holding Ready Item to holster counts real time, so slowed time does not lengthen it. (Stewie's Tweaks bPipboyLightAndHolsteringIgnoreTimescale)", holster_real_time);
        switch!(app, "power-armor-no-screen-blood", "No Screen Blood in Power Armor", "Hits put no blood on the screen while power armor is worn. (Stewie's Tweaks bPowerArmorPreventsScreenBlood)", power_armor_no_screen_blood);
        switch!(app, "no-red-crosshair-cloaked", "Invisible Foes Stay Hidden", "An invisible foe does not turn the crosshair red. (Stewie's Tweaks bHideRedCrosshairOnDistantInvisibleTargets)", no_red_crosshair_on_cloaked);
        switch!(app, "god-mode-quiet-landing", "Quiet Landings in God Mode", "In god mode the player's hard landings make no pain sound. (Stewie's Tweaks bGodmodePreventsLegCrippleSound)", god_mode_quiet_landing);
        switch!(app, "console-compress-repeats", "Count Repeated Console Lines", "A console line said again right after itself is counted on one line (line -#2). (Stewie's Tweaks bCompressDuplicateConsoleMessages)", console_compress_repeats);
        switch!(app, "console-timestamps", "Console Times", "Console lines start with the time since the game started. (Stewie's Tweaks bConsolePrintsIncludeTimestamp)", console_timestamps);
        switch!(app, "living-anatomy-dr", "Living Anatomy Shows DR", "With Living Anatomy the target's line shows its Damage Resistance as well. (Stewie's Tweaks bLivingAnatomyShowDR)", living_anatomy_dr);
        switch!(app, "sync-categories", "Matching Categories", "In containers and barter both lists change category together. (Stewie's Tweaks bSynchronizeContainerCategories)", sync_categories);
        // ---- rest ----
        switch!(app, "sleep-in-owned-beds", "Sleep in Owned Beds", "A bed that belongs to someone else can be slept in. (Stewie's Tweaks bSleepInOwnedBeds)", game.sleep_in_owned_beds);
        // ---- items ----
        switch!(app, "power-armor-no-training", "Power Armor Without Training", "Power armor can be worn without Power Armor Training. (Stewie's Tweaks bPowerArmorNeedsNoTraining)", game.power_armor_needs_no_training);
    }
}

/// Hand the game's part to the running game when it differs, and log the change.
fn sync(tweaks: Res<TweakSettings>, mut script: Option<ResMut<ScriptState>>, quests: Option<ResMut<crate::quest_messages::QuestMessages>>, loading: Option<ResMut<crate::loading::Loading>>, console: Option<ResMut<crate::console::Console>>, mut active: Option<ResMut<crate::menu::ActiveMenu>>) {
    if let Some(mut l) = loading {
        if l.hide_tips != tweaks.no_loading_tips {
            l.hide_tips = tweaks.no_loading_tips;
        }
    }
    let hidden = crate::quest_messages::Hidden { failed: tweaks.no_quest_failed_popup, discovered: tweaks.no_location_popup, objectives: tweaks.no_objective_popup };
    if let Some(m) = active.as_deref_mut().and_then(|a| a.0.as_mut()) {
        if m.sync_categories != tweaks.sync_categories {
            m.sync_categories = tweaks.sync_categories;
        }
    }
    if let Some(mut c) = console {
        if (c.compress_repeats, c.timestamps) != (tweaks.console_compress_repeats, tweaks.console_timestamps) {
            c.compress_repeats = tweaks.console_compress_repeats;
            c.timestamps = tweaks.console_timestamps;
        }
    }
    if let Some(mut q) = quests {
        if q.hidden != hidden {
            q.hidden = hidden;
        }
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    if rt.game.tweaks != tweaks.game {
        info!("tweaks: the game takes {:?}", tweaks.game);
        rt.game.tweaks = tweaks.game.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_multiplier_slider_has_one_in_the_middle() {
        assert_eq!(factor(0.5), 1.0);
        assert_eq!(part(1.0), 0.5);
        assert_eq!(shown_factor(0.75), "x1.5");
        assert_eq!(part(9.0), 1.0, "brought into the slider");
    }

    #[test]
    fn every_tweak_is_off_by_default() {
        let t = TweakSettings::default();
        assert_eq!(t.game, Tweaks::default());
        assert_eq!(t.game.xp_multiplier, 1.0);
        assert!(!t.no_karma_messages && !t.disable_fast_travel);
    }
}
