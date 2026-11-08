# Tweaks

The settings page's **Tweaks** tab: optional changes to how the game plays or what it
shows, each off (or at "no change") unless the player sets it. The owner asked for
the features of Stewie's Tweaks, a script-extender plugin for the original, as engine
settings (2026-11-08, #1891).

## Sources and clean room

- [documented: Stewie Tweaks v9.87, its readme and its menu list `AllTweaks.json`, a private
  copy in the owner's `C:\fnv-private\stewietweaks`] say what each tweak does. Only
  that is used: the plugin's DLL is not opened, and none of its files or text are copied.
  The labels and notes of the Tweaks tab are NVForever's own words. The catalogue below
  names each tweak by its INI key, so a player who knows the plugin can find it.
- [documented: Viva New Vegas's generated `nvse_stewie_tweaks.ini`, the owner's
  modlist] says which tweaks that modlist turns on (the "VNV on" column); those go
  first in later batches.

## What is left out

- **Fixes** (the plugin's Fixes category): workarounds for the original's bugs. The
  owner: "if there are any fixes for the engine skip them we dont need fixes". NVForever
  is its own engine and does not carry those bugs over.
- **n/a**: about the original program, not about play: its error log, its keyboard
  handling, its achievements, its main menu's Downloads button, mod-author debug prints,
  the plugin's own performance work (its "inline vanilla functions" option is not even in
  the menu list).
- **unsure, skipped**: needs the plugin's own menu files or another mod's files, or the
  readme does not say clearly enough what changes to build it without guessing. Each says
  why. These wait for an owner decision or an observation.

## How it is built

- `fnv_game::tweaks::Tweaks`: the values the game's rules read (`Game::tweaks`), e.g.
  the experience multiplier in `experience::award`, power armour training in the
  frame's `set_power_armor_training`. Not saved with a game: settings.
- `nvf_play::tweaks::TweakSettings`: every value, with the game's part inside; the engine's
  modules read the rest (the karma messages in `crime`, the fast-travel refusals in
  `pipboy::data::travel`, the quest lines in `quest_messages::Hidden`, the XP meter).
  `sync` hands the game's part to the running game each frame, as the difficulty is
  handed over, and pushes the quest-line choices into `QuestMessages`.
- Each tweak is a row of the Tweaks tab, kept in `NVForever.ini` `[Settings]` under a
  `tweak-…` key (`crate::settings`). A switch is `switch!`; a multiplier is a slider
  from x0 to x2 in tenths, x1 in the middle.
- A tweak's options (the plugin's sub-settings) are added as rows of their own only
  when they change what the tweak does in a way a player would choose; the first batch
  keeps each tweak to one row and says in the catalogue which options are not built.

## First batch (2026-11-08)

| Row | INI key | Where |
|---|---|---|
| Experience Earned (x0–x2) | `bReduceXP` | `fnv_game::experience::award`, after the perks |
| Quiet Experience | `bNoXPMessages` | `xp_meter::watch`: a gain that earns no level is not shown; LEVEL UP still is |
| Quiet Karma | `bNoKarmaMessages` | `crime::show_notes` |
| No Quest Failed Banner | `bNoQuestFailedPopup` | `QuestMessages::push` |
| No Location Banner | `bNoLocationPopup` | `QuestMessages::push` (the place is still discovered and its XP given) |
| No Objective List | `bRemoveQuestObjectiveAddedText` | `QuestMessages::push` |
| No Fast Travel | `bDisableFastTravel` | `travel::tweak_checks`: refused with the script block's text |
| Fast Travel in the Air | `bMidairFastTravel` | `travel::tweak_checks` |
| Fast Travel Near Enemies | `bFastTravelWithEnemiesNearby` | `travel::tweak_checks` (combat itself still refuses) |
| Fast Travel Encumbered | `bFastTravelOverencumbered` | `travel::tweak_checks` |
| Power Armor Without Training | `bPowerArmorNeedsNoTraining` | `Game` frame tick and `SetPCCanUsePowerArmor` |
| No Red Crosshair | `bNoRedCrosshairOnEnemies` | `hud_target::aim` (`AimHostile`, read by both HUDs) |
| No Enemies on the Compass | `bHideEnemyMarkers` | `compass::gather` |
| Quiet Reputation | `bNoReputationMessages` | `reputation::watch` |
| No Battle Music | `bDisableCombatMusic` | `music::play` |
| No Loading Tips | `bDisableLoadingScreenTips` | `loading::spawn_screen` (`Loading::hide_tips`) |
| No Explosion Blur | `bDisableExplosionInFaceIMOD` | nvforever `shot_fx::explosions` |
| No Chem Wore-Off Effect | `bRemoveChemWarnOffIMOD` | `Game::drug_wore_off` |
| Sleep in Owned Beds | `bSleepInOwnedBeds` | `wait::refusal` |

Already so, no row added: `bNoDialogueZoom` is the Gameplay tab's dialogue-zoom row;
NVForever never refuses a jump for being over-encumbered (`bJumpWhileOverencumbered`)
and its main menu's Quit asks nothing (`bNoExitConfirm`).

Options not built in this batch [design]: Quiet Karma's separate good/evil switches and
thresholds, Quiet Experience's per-source switches, the quest-failed "only if started"
option, Quiet Reputation's per-kind switches, the compass's "show enemies that fire"
option.

## Third batch (2026-11-08, Viva New Vegas's first)

| Row | INI key | Where |
|---|---|---|
| No Sneak Label | `bRemoveSneakLabel` | `hud::gather` and the tile HUD (`tiles`) |
| Skip Kill Camera | `bVatsExitKey` | `kill_cam::update`: a right click (Tab is the Pip-Boy here) |
| Number Keys on Terminals | `bNumberedComputerHotkeys` | `terminal::keys`: 1-9, 0 the tenth, outside the hacking page |
| Unassign Hotkeys | `bDeselectHotkeys` | `Hotkeys::clear_if_holding` from `hotkey_panel::assign_row` |
| Music While Talking (slider) | `bScaleMusicVolumeDuringDialogue` | `music::play`, in `PlayState::Dialogue` |
| Radio While Talking (slider) | `bScaleRadioSongVolumeDuringDialogue` | `radio::play`, in `PlayState::Dialogue` |
| Perks First When No Points | `bSkipSkillMenuIfNoPointsToAssign` | `levelup::sync` |
| No Message for Spent Books | `bNoSkillMessageIfIncreaseIsZero` | `script_events::effects` (`EffectCtx::tweaks`) |
| Recipe Categories in Capitals | `bCapitalizeRecipeCategories` | `crafting::view` |
| Rounds per Shot | `bShowWeaponAmmoUseInMenus` | `pipboy::items::info_of` (WEAP ammo use) |
| Skill Book Effects | `bShowBookEffects` | `pipboy::items::info_of` (the points with the perks applied) |
| Map Remembers Its Place | `bMapRemembersPosition` | `pipboy::data::opened` |
| Faster Hacking Exit | `bFasterHackingTransition` | `terminal::timers`: 0.5 s instead of 2 s |

## Fourth batch (2026-11-08)

| Row | INI key | Where |
|---|---|---|
| Console Clicks Skip the Player | `bNonSelectablePlayerInConsole` | `console::click` |
| Holster in Real Time | `bPipboyLightAndHolsteringIgnoreTimescale` | `control::read_controls` (the light already counts real time) |
| No Screen Blood in Power Armor | `bPowerArmorPreventsScreenBlood` | `screen_splatter::take_hurts` |
| Invisible Foes Stay Hidden | `bHideRedCrosshairOnDistantInvisibleTargets` | `hud_target::aim` (Invisibility or Chameleon ≥ 1) |
| Quiet Landings in God Mode | `bGodmodePreventsLegCrippleSound` | `falls::hurt` |

Menu tweaks that code holding the game reads (the Pip-Boy's item info, crafting, the
level-up menu, the terminal's timers) are kept in `fnv_game::tweaks` too, so no system
needs another parameter; the catalogue's notes say where NVForever already does what a
tweak asks ("already so").

## Next

The "to do" rows, the ones Viva New Vegas turns on first. Not yet: reading a player's
own `nvse_stewie_tweaks.ini` as the starting values (it would need the INI read from the
game's Data folder, read-only).

## Catalogue

<!-- generated: the table below lists every tweak of AllTweaks.json (v9.87) by its INI key -->
| Status | Count |
|---|---|
| done | 67 |
| to do | 536 |
| unsure, skipped | 17 |
| n/a | 15 |
| skipped: fix | 10 |

### Gameplay

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bDisableFastTravel` | done |  |  | No Fast Travel |
| `bFastTravelOverencumbered` | done |  |  | Fast Travel Encumbered |
| `bJumpWhileOverencumbered` | done |  |  | already so: NVForever never refuses the jump when over-encumbered |
| `bNoDialogueZoom` | done |  |  | already a row: Gameplay tab, dialogue-zoom |
| `bPowerArmorNeedsNoTraining` | done |  |  | Power Armor Without Training |
| `bReduceXP` | done |  | 2 | Experience Earned (slider, x0 to x2) |
| `bHiddenFragsFix` | to do | yes |  | NPCs' own unplayable explosives are hidden from the pickpocket list |
| `bMoreFrequentNPCLightUpdates` | unsure, skipped | yes | 1 | NVForever lights actors its own way; unclear what to change |
| `bActorsStickToSlopes` | to do |  | 2 |  |
| `bAdjustableScopeZoom` | to do |  | 14 |  |
| `bAgilityScalesJumpHeight` | to do |  | 1 |  |
| `bAgilityScalesMovementSpeed` | to do |  | 3 |  |
| `bAimingSpeed` | to do |  | 2 |  |
| `bAimingUnholstersWeapons` | to do |  |  |  |
| `bAllowAidAtMaxHealth` | to do |  | 3 |  |
| `bAllowSleepWaitWithReducedMaxHealth` | to do |  |  |  |
| `bAmmoSwapWindowMatchesReloadTime` | to do |  |  |  |
| `bArmorCausesSinking` | to do |  | 4 |  |
| `bAutoUnlockTerminals` | to do |  | 2 |  |
| `bAutoWeaponJamWhileFiring` | to do |  |  |  |
| `bBarterAffectsRepairCosts` | to do |  | 3 |  |
| `bBarterCheckActorBuySellFlags` | to do |  | 2 |  |
| `bBillyGoatMode` | to do |  | 3 |  |
| `bCapSkillsBySPECIAL` | to do |  | 5 |  |
| `bCharacterRadiation` | to do |  | 1 |  |
| `bClearNearbyPlayerMarker` | to do |  | 1 |  |
| `bCompanionsDontUseAmmo` | to do |  |  |  |
| `bContactMines` | to do |  | 2 |  |
| `bCrippledLegsScaleJumpHeight` | to do |  | 2 |  |
| `bCrippledLimbsPlayPainSoundWhenFalling` | to do |  | 1 |  |
| `bCripplingDoesntDisarm` | to do |  |  |  |
| `bCritChanceIgnoresFireRate` | to do |  |  |  |
| `bCustomArmorConditionPenalty` | to do |  | 1 |  |
| `bCustomSpecialPoints` | to do |  | 1 |  |
| `bDisableReloadingNonEmptyClip` | to do |  | 2 |  |
| `bDisarmRequiresSkill` | to do |  | 2 |  |
| `bDrowningDrainsAP` | to do |  |  |  |
| `bEncumbranceIncludesGrabbedItems` | to do |  |  |  |
| `bEnduranceScalesMinFallHeight` | to do |  | 2 |  |
| `bExplosionKnockdownAvoidanceUsesStrength` | to do |  |  |  |
| `bExplosionRadiusBuff` | to do |  | 3 |  |
| `bExplosionsDontPushTakeableItems` | to do |  |  |  |
| `bExtendLockpickSpotRange` | to do |  |  |  |
| `bFallDeathsCauseDismemberment` | to do |  |  |  |
| `bFastTravelCostsSpecialNukaBottles` | to do |  | 3 |  |
| `bForceLockpickNoBreakLock` | to do |  | 3 |  |
| `bFreeWildWasteland` | to do |  |  |  |
| `bFrenzyGrantsPlayerXP` | to do |  |  |  |
| `bGodModePreventsJamAnims` | to do |  |  |  |
| `bGrabbingItemsIsCrime` | to do |  | 1 |  |
| `bHardcoreTweaks` | to do |  | 3 |  |
| `bHipFireAnimsWhileScoped` | unsure, skipped |  |  | depends on alternate animations |
| `bHittingWeaponsDoesntDamageThem` | to do |  |  |  |
| `bHoldToActivate` | to do |  | 9 |  |
| `bHoldToHolsterMeleeWeapons` | to do |  |  |  |
| `bHostilesPrioritizePlayer` | to do |  | 3 |  |
| `bImprovedHacking` | to do |  | 10 |  |
| `bJumpingCostsAP` | to do |  | 3 |  |
| `bJumpSwimsUpwards` | to do |  | 2 |  |
| `bKeepFallHeightOnLoad` | to do |  |  |  |
| `bLevelDifferenceAffectsCombatXP` | to do |  | 2 |  |
| `bLightStepAffectsCompanions` | to do |  |  |  |
| `bLockNeedsKeyShowName` | to do |  |  |  |
| `bLockpickForceBonusNearSweetSpot` | to do |  | 2 |  |
| `bLockpickRememberBobbyPinAngle` | to do |  |  |  |
| `bLootUnconsciousVictims` | to do |  |  |  |
| `bLuckDoesntAffectGambling` | to do |  | 1 |  |
| `bMapSelectableCompanions` | to do |  | 2 |  |
| `bMarkNearbyLocationsOnMap` | to do |  | 1 |  |
| `bModifySkillPointsEarned` | to do |  | 2 |  |
| `bMousewheelScrollsWeaponHotkeys` | to do |  | 1 |  |
| `bMovementPenalties` | to do |  | 11 |  |
| `bNoAmmoFromTakingNPCWeapon` | to do |  |  |  |
| `bNoAPRegenWhileOverencumbered` | to do |  | 2 |  |
| `bNoCompanionItemDamage` | to do |  |  |  |
| `bNoDamageMeleeWeaponIfTargetDead` | to do |  |  |  |
| `bNoEnvironmentRadiationInFullPowerArmor` | to do |  |  |  |
| `bNoFastTravelIfLegsCrippled` | to do |  | 1 |  |
| `bNoFreeBarterItems` | to do |  | 1 |  |
| `bNoGrabOwnedItems` | to do |  |  |  |
| `bNoLockEncounterZoneLevels` | unsure, skipped |  |  | encounter-zone level locking not yet checked in NVForever |
| `bNoLockFailedTerminals` | to do |  |  |  |
| `bNoPlayerStaggerAnims` | to do |  |  |  |
| `bNoSkillTags` | to do |  | 6 |  |
| `bNPCsDetectMineExplosions` | to do |  |  |  |
| `bNPCsDropLiveGrenades` | to do |  |  |  |
| `bNPCsEarnAmmoCasings` | to do |  |  |  |
| `bNPCsTakeLimbFallDamage` | to do |  |  |  |
| `bOnlyAllowWaitWhileSitting` | to do |  | 3 |  |
| `bOverencumberedTweak` | to do |  | 8 |  |
| `bPerBulletLoopingReloads` | to do |  | 2 |  |
| `bPerksAffectFistWeaponSpeed` | to do |  |  |  |
| `bPickLocksWithoutSkill` | to do |  | 1 |  |
| `bPickpocketCaughtCausesDetection` | to do |  |  |  |
| `bPickpocketFleeingActors` | to do |  |  |  |
| `bPickpocketOverhaul` | to do |  | 15 |  |
| `bPlantingLiveGrenadesRequiresWeaponSkill` | to do |  |  |  |
| `bPowerArmorScalesFallDamage` | to do |  | 2 |  |
| `bPowerArmorScalesLimbDamage` | to do |  | 3 |  |
| `bPowerAttackIfBlocking` | to do |  | 1 |  |
| `bPreventHittingHolsteredWeapons` | to do |  | 2 |  |
| `bPreventNPCAddiction` | to do |  |  |  |
| `bPreventReloadWeaponUnholstering` | to do |  |  |  |
| `bPreventRepairIfNotAtWorkbench` | to do |  |  |  |
| `bPreventStealingCapsAfterRepair` | to do |  | 1 |  |
| `bQuickUse` | to do |  | 9 |  |
| `bReloadingPreventsWeaponHotkeys` | to do |  |  |  |
| `bReloadingWithFullClipSwitchesAmmoType` | to do |  |  |  |
| `bReloadJamsAffectedByAgility` | to do |  |  |  |
| `bReloadWhileFiring` | to do |  |  |  |
| `bRememberBobbyPinHealth` | to do |  |  |  |
| `bRepairScalesCraftingCondition` | to do |  |  |  |
| `bRepairsRewardXP` | to do |  | 1 |  |
| `bReviveUnconsciousCompanions` | to do |  | 3 |  |
| `bRunAndGunAPInCombatOnly` | to do |  |  |  |
| `bRunningCostsAP` | to do |  | 8 |  |
| `bRunSlowerInWater` | to do |  | 1 |  |
| `bScalePlayerMeleeReach` | to do |  | 3 |  |
| `bScaleWeaponVolume` | to do |  | 2 |  |
| `bScopeHoldBreath` | to do |  | 7 |  |
| `bSellStolenItems` | to do |  |  |  |
| `bSemiAutoQueue` | to do |  | 2 |  |
| `bSeparateAlcoholEffects` | to do |  |  |  |
| `bSkipVeryEasyLocksAtMaxSkill` | to do |  | 2 |  |
| `bSleepHealingMinDuration` | to do |  | 2 |  |
| `bSleepOnChairs` | to do |  |  |  |
| `bSlowBreathRegen` | to do |  | 2 |  |
| `bSneakAttackWithoutCrouching` | to do |  |  |  |
| `bSneakingDoesntForcePowerAttacks` | unsure, skipped |  |  | depends on separate animations |
| `bSpeedMultScalesJumpHeight` | to do |  |  |  |
| `bStealingSendsNoAlarm` | to do |  |  |  |
| `bStrengthAffectsAllThrowables` | to do |  |  |  |
| `bTargetProjectilesInVATS` | to do |  | 1 |  |
| `bUnequipWeaponMods` | to do |  | 7 |  |
| `bUseArmorRepairKitsInRepairMenu` | to do |  |  |  |
| `bUseCustomBarterPriceMultipliers` | to do |  | 13 |  |
| `bUseCustomSniperZoomRate` | to do |  | 2 |  |
| `bUseWeaponRepairKitsInRepairMenu` | to do |  | 2 |  |
| `bUsingKeysRewardsXP` | to do |  | 3 |  |
| `bUsingNotesRewardsXP` | to do |  | 3 |  |
| `bVATSDequeueWeaponShotsOnDisarm` | to do |  |  |  |
| `bVATSHipFire` | to do |  |  |  |
| `bVATSIgnoreNonExplosiveProjectiles` | to do |  |  |  |
| `bVATSTargetPlayerProjectiles` | to do |  |  |  |
| `bVATSThrowablesUseVisibility` | to do |  |  |  |
| `bWeaponModdingSkillRequirement` | to do |  |  |  |
| `bWeightlessWornArmor` | to do |  | 1 |  |
| `bWeightlessWornPowerArmor` | to do |  | 3 |  |
| `iMaxCharacterLevel` | to do |  | 1 |  |
| `iPerksPerLevel` | to do |  | 1 |  |

### Combat

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bDisableCombatMusic` | done |  |  | No Battle Music |
| `bHideEnemyMarkers` | done |  | 1 | No Enemies on the Compass (the firing-enemies option is not built) |
| `bAllowTeammatesUseMeltdown` | to do |  |  |  |
| `bAutoWeaponNoFiringDelay` | to do |  | 1 |  |
| `bBloodyMessGibTargetedLimbOnly` | to do |  |  |  |
| `bCharacterMeleeDamageIgnoresScale` | to do |  |  |  |
| `bCharacterNonMeleeDamageIgnoresScale` | to do |  |  |  |
| `bChargedAttacksCostAP` | to do |  | 4 |  |
| `bClipSizeMatters` | to do |  |  |  |
| `bDifficultyDoesntAffectNPCToNPCDamage` | to do |  | 3 |  |
| `bEnteringVATSCostsAP` | to do |  | 4 |  |
| `bFatalNonSneakCritsAlwaysGib` | to do |  | 1 |  |
| `bFirstPersonVATS` | to do |  |  |  |
| `bHoldAndReleaseThrowables` | to do |  | 1 |  |
| `bIgnoreCompanionsVATS` | to do |  | 1 |  |
| `bKillsRewardAP` | to do |  | 1 |  |
| `bLocationalMeleeDamage` | to do |  | 3 |  |
| `bMaxVATSDistanceUsesWeaponRange` | to do |  | 3 |  |
| `bNoCompanionKillXP` | to do |  | 1 |  |
| `bNoDisarmCompanions` | to do |  |  |  |
| `bNoHealingInCombat` | to do |  | 1 |  |
| `bNoPipBoyInCombat` | to do |  | 5 |  |
| `bNoScaleNpcDamageByCondition` | to do |  |  |  |
| `bNoScaleNpcDamageResistanceByCondition` | to do |  |  |  |
| `bNoScaleNpcDamageThresholdByCondition` | to do |  |  |  |
| `bNoSelfExplosionDamage` | to do |  |  |  |
| `bNoSelfMeltdownDamage` | to do |  | 3 |  |
| `bNoSneakAttacksCriticals` | to do |  |  |  |
| `bNoVATSTargetInvisible` | to do |  |  |  |
| `bNoWeaponConditionDamagePenalty` | to do |  |  |  |
| `bNPCsCanDisarmPlayer` | to do |  |  |  |
| `bNPCsCanSneakCritPlayer` | to do |  |  |  |
| `bPlayerMeleeDamageIgnoresScale` | to do |  |  |  |
| `bPlayerNonMeleeDamageIgnoresScale` | to do |  |  |  |
| `bPowerAttackWhileOverencumbered` | to do |  |  |  |
| `bRemoveWeaponDamageBuffer` | to do |  |  |  |
| `bScaleCriticalDamage` | to do |  | 1 | needs a field on fnv-combat Rules; check what its serialized dumps feed first |
| `bSneakCriticalsHeadshotsOnly` | to do |  |  |  |
| `bSneakCriticalsOnlyMeleeWeapons` | to do |  |  |  |
| `bVATSAutoTargetHead` | to do |  |  |  |
| `bVATSStopBurstIfTargetDead` | to do |  |  |  |
| `bWeaponRequirementsMatter` | to do |  | 7 |  |

### Quality-of-life

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bBetterAutoWalk` | done | yes | 1 | already so: strafing does not end auto-walk |
| `bConsoleHistoryNoStoreDuplicates` | done | yes |  | already so: the console keeps no repeated line |
| `bCrouchWhileEquippingWeapons` | done | yes |  | already so: crouching is never refused while (un)equipping |
| `bDecreasedDialogueClickDelay` | done | yes |  | already so: topics can be chosen at once |
| `bDelayPostCombatLevelUp` | done | yes |  | already so: the level-up menu waits for combat to end (after_combat) |
| `bDelayPostCombatReputationPopup` | done | yes |  | already so: the reputation box waits for combat to end |
| `bDeselectHotkeys` | done | yes |  | Unassign Hotkeys |
| `bFasterEnterLockpickMenu` | done | yes |  | already so: the lockpick menu opens at once |
| `bFasterHackingTransition` | done | yes |  | Faster Hacking Exit (0.5 s instead of 2 s) |
| `bFasterSaveMenuClose` | done | yes |  | already so: saving leaves the pause menu open, no wait |
| `bGodmodePreventsLegCrippleSound` | done | yes |  | Quiet Landings in God Mode (NVForever's pain sound on a fall is the hard-landing sound) |
| `bHideCrosshairInKillcams` | done | yes |  | already so: no crosshair in a kill camera |
| `bHideHealthbarInKillcams` | done | yes |  | already so: the HUD is hidden in a kill camera |
| `bLeftAndRightCancelEachOther` | done | yes |  | already so: left and right held together cancel |
| `bLessRestrictiveVATSMenu` | done | yes |  | already so: VATS mode has no zoom phase to wait for |
| `bNonSelectablePlayerInConsole` | done | yes |  | Console Clicks Skip the Player |
| `bNoSkillMessageIfIncreaseIsZero` | done | yes |  | No Message for Spent Books (the two-decimal option is moot: book points are whole) |
| `bPipboyLightAndHolsteringIgnoreTimescale` | done | yes |  | Holster in Real Time (the Pip-Boy light already counts real time) |
| `bQueueWeaponHolsteringWhileAnimsPlay` | done | yes |  | already so: a holster is never dropped (it waits for a draw, else starts at once) |
| `bScaleMusicVolumeDuringDialogue` | done | yes | 1 | Music While Talking (slider) |
| `bScaleRadioSongVolumeDuringDialogue` | done | yes | 1 | Radio While Talking (slider) |
| `bSkipIntroVideo` | done | yes |  | already so: the intro plays only after New Game and any key skips it |
| `bVatsExitKey` | done | yes | 1 | Skip Kill Camera (right click; Tab is the Pip-Boy key here) |
| `bDisableCompanionKillcam` | done |  |  | already so: only the player's VATS kills start a kill camera |
| `bDisableExplosionInFaceIMOD` | done |  |  | No Explosion Blur |
| `bMidairFastTravel` | done |  |  | Fast Travel in the Air |
| `bNoMapMarkerAddedPopup` | done |  |  | already so: NVForever shows no marker-added popup |
| `bNoReputationMessages` | done |  | 5 | Quiet Reputation (box and sound together; per-kind options not built) |
| `bRandomizeKillcamMode` | done |  |  | n/a: NVForever has one kill camera kind |
| `bRemoveChemWarnOffIMOD` | done |  |  | No Chem Wore-Off Effect |
| `bBetterPickupPrompt` | to do | yes |  | the prompt shows one unit; needs the stack count and the encumbrance |
| `bDoubleTapReloadToChangeAmmoType` | to do | yes | 2 | the owner moved Lower Weapon off a double tap because it held reloads back; ask first |
| `bFasterControllerPOVRotate` | to do | yes |  |  |
| `bReloadingWithNoAmmoSwapsAmmoTypes` | to do | yes |  | combat::fire is at the system parameter limit |
| `bRestore2Hotkey` | unsure, skipped | yes | 2 | needs the plugin's own menu XML |
| `bSelectUnavailableRadios` | to do | yes |  | the Pip-Boy lists only stations in range |
| `bShiftIgnoresFriendlyVATS` | to do | yes | 1 | friendlies are targetable; a Shift filter belongs in vats_mode::barred |
| `bShiftScreenshotHidesMenus` | to do | yes |  | the capture is asynchronous; MenusHidden must be held a frame or two |
| `bSkipDeathcamHotkey` | to do | yes | 2 | the death camera (nvforever death_cam) has no end but a load yet |
| `bActivatingDoesntStandUp` | to do |  |  |  |
| `bAddItemUsesItemIcon` | unsure, skipped |  |  | needs an icon replacer |
| `bAddNightVisionToggle` | to do |  | 4 |  |
| `bAllowActivateWhileAnimPlays` | to do |  |  |  |
| `bAllowFiringWhileLanding` | to do |  |  |  |
| `bAllowOpenPipboyDuringCameraShake` | to do |  |  |  |
| `bAllowUnsafeFastTravel` | to do |  | 1 |  |
| `bAllowVATSWhileAnimsPlay` | to do |  |  |  |
| `bAllowWeaponHotkeysAndPipBoyWhileReloading` | to do |  |  |  |
| `bAllowWeaponHotkeysWhileEquipping` | to do |  |  |  |
| `bAllowWeaponHotkeysWhileFiring` | to do |  |  |  |
| `bDisableHitShader` | to do |  | 1 | NVForever draws no hit shader yet |
| `bDisableHolsteredWeaponFOVZoom` | to do |  |  |  |
| `bDisableInteriorFog` | to do |  | 1 |  |
| `bDisableTutorialMessages` | to do |  |  |  |
| `bDisableWeaponFOV` | to do |  | 1 |  |
| `bDontSetQuestWhenCompletingObjectives` | to do |  |  |  |
| `bDontSetQuestWhenObjectivesAdded` | to do |  |  |  |
| `bEnteringVATSDoesntUnholsterWeapon` | to do |  |  |  |
| `bEquipBrokenItems` | to do |  |  |  |
| `bFireWhileAiming` | to do |  |  |  |
| `bForceHiResWeaponModels` | to do |  |  |  |
| `bHideMiscQuestItems` | to do |  | 1 |  |
| `bHoldCrouchToSneak` | to do |  |  |  |
| `bHoldWaitKeyToShowMenu` | to do |  | 1 |  |
| `bHotkeyHolstersWeaponIfEquipped` | to do |  | 1 |  |
| `bHUDRotatesWithVanityCam` | to do |  |  |  |
| `bJumpingDoesntDropGrabbedItem` | to do |  |  |  |
| `bKeepBrokenItemsEquipped` | to do |  |  |  |
| `bKeepHolotapePlayingWhenSelectingOtherNotes` | to do |  |  |  |
| `bKeepRouletteBetAmount` | to do |  |  |  |
| `bKeepXPBarWhenClosingMenus` | to do |  |  |  |
| `bLessFrequentPlayerPainSounds` | to do |  | 1 |  |
| `bLockpickHackingMessageShowsCurrentSkill` | to do |  | 2 |  |
| `bMinBloodSplatterHealthDamage` | to do |  | 1 |  |
| `bMinigamesPlayXPSoundAtMaxLevel` | to do |  |  |  |
| `bMoveDuringOpenPipboyAnim` | to do |  |  |  |
| `bMoveDuringVATSPlayback` | to do |  |  |  |
| `bNoAutoContinueDialog` | to do |  |  |  |
| `bNoDropWeaponSoundOnPlayerDeath` | to do |  |  |  |
| `bNoEquippedWeaponMovementPenalty` | to do |  |  |  |
| `bNoExteriorLoadScreens` | to do |  |  |  |
| `bNoFoodWornOffMessage` | to do |  | 1 |  |
| `bNoKillcamKillSound` | to do |  |  |  |
| `bNoKnockdownInGodmode` | to do |  |  |  |
| `bNoLTRTOnPipboy` | n/a |  |  | the original's controller texture swap |
| `bNoPipboyIdleAnims` | to do |  |  |  |
| `bNoPoisonConfirm` | to do |  |  |  |
| `bNoScreenshotPopup` | to do |  | 3 | NVForever shows no screenshot message yet |
| `bNPCsDropWeaponHolsteredWeapon` | to do |  |  |  |
| `bPauseOnSaveLoad` | to do |  |  |  |
| `bPickLocksEvenWithKey` | to do |  | 1 |  |
| `bPopupMenusDontMoveCursor` | to do |  |  |  |
| `bPreventBushPassthroughSounds` | to do |  |  |  |
| `bQuestTextVisibleWhileAiming` | to do |  |  |  |
| `bRemoveFollowerTopics` | to do |  |  |  |
| `bRemoveLandingAnim` | to do |  |  |  |
| `bRepairRequiresWeaponSkill` | to do |  |  |  |
| `bRepeatJumping` | to do |  |  |  |
| `bRobotCompanionsHealWithScrapMetal` | to do |  | 4 |  |
| `bScaleExplosionShake` | to do |  | 2 |  |
| `bScopeVisibilityDelay` | to do |  | 4 |  |
| `bSkipVigorTesterSpecialPages` | to do |  |  |  |
| `bSlotsAutoSpinHotkey` | to do |  |  |  |
| `bSwapAmmoWithWeaponHolstered` | to do |  |  |  |
| `bToggleControllerIfAttackPressed` | to do |  | 1 |  |
| `bTurnFurtherWhileSeated` | to do |  |  |  |
| `bTurnSlowerWhileAiming` | to do |  | 2 |  |
| `bUnequipBrokenArmor` | to do |  | 1 |  |
| `bUnspokenNPCIndicator` | to do |  |  |  |
| `bUseFirstPersonEmptyClipSound` | to do |  |  |  |
| `bWeaponCycleUpDownHotkeys` | to do |  |  |  |

### HUD

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bAddRGBSliders` | done | yes | 1 | already a row: Interface tab, hud-red / hud-green / hud-blue |
| `bHideRedCrosshairOnDistantInvisibleTargets` | done | yes |  | Invisible Foes Stay Hidden (at any distance) |
| `bNoKarmaMessages` | done |  | 7 | Quiet Karma (messages and sounds together; no thresholds yet) |
| `bNoLocationPopup` | done |  |  | No Location Banner |
| `bNoQuestFailedPopup` | done |  | 1 | No Quest Failed Banner |
| `bNoRedCrosshairOnEnemies` | done |  |  | No Red Crosshair (native and tile HUD alike) |
| `bNoXPMessages` | done |  | 7 | Quiet Experience (all sources at once; LEVEL UP still shows) |
| `bRemoveQuestObjectiveAddedText` | done |  | 1 | No Objective List |
| `bRemoveSneakLabel` | done |  |  | No Sneak Label (native and tile HUD) |
| `bDescriptiveMarkerAddedMessage` | to do | yes |  | NVForever shows no marker-added popup |
| `bEnemyHealthbarShowBuffedHP` | to do | yes |  | max_health leaves out temporary modifiers; choose how bars share them |
| `bHideGrenadeIndicatorForNoDamageExplosions` | to do | yes |  |  |
| `bMorePreciseRadMeter` | to do | yes | 1 | unclear whether the HUD gain or the Pip-Boy total is meant |
| `bArmorConditionLabel` | to do |  | 9 |  |
| `bColoredHUDBars` | to do |  | 2 |  |
| `bColorWeaponCndLabel` | to do |  | 2 |  |
| `bCompanionPipColorChange` | to do |  | 1 |  |
| `bCompassFadeLeftSide` | to do |  |  |  |
| `bCompassLocationDistanceBasedAlpha` | to do |  |  |  |
| `bCompassNPCDistanceBasedAlpha` | to do |  |  |  |
| `bCompassQuestHeightIndicator` | to do |  |  |  |
| `bCriticalHitMessagesIncludeLimbName` | to do |  |  |  |
| `bCustomAddToInventoryMessageTimer` | to do |  | 1 |  |
| `bCustomSubtitleDistance` | to do |  | 1 |  |
| `bDetectedByWhom` | to do |  | 5 |  |
| `bDisableCompassEdgeAlpha` | to do |  |  |  |
| `bDisableGrenadeIndicator` | to do |  |  |  |
| `bDisableHUDCrippledLimbIndicator` | to do |  |  |  |
| `bDisableNeedsMessages` | to do |  | 4 | those messages are the game's own scripts; needs a filter by message |
| `bDistanceBasedQuestMarkerVisibilty` | to do |  | 2 |  |
| `bExplodingDestructibleIndicator` | to do |  |  |  |
| `bHideAmmoLabel` | to do |  | 1 |  |
| `bHideCapsAddedMessages` | to do |  |  |  |
| `bHideCrosshairInFirstPerson` | to do |  |  | the crosshair and the prompt share a root in reticle.rs |
| `bHideCrosshairInThirdPerson` | to do |  |  | the crosshair and the prompt share a root in reticle.rs |
| `bHideCrosshairWhileReloading` | to do |  |  |  |
| `bHideInvisibleUndiscoveredLocations` | to do |  |  |  |
| `bHideUnknownNPCNames` | to do |  | 1 |  |
| `bHostilesUseNeutralColorOnCompass` | to do |  |  |  |
| `bHUDFatigueIndicator` | to do |  |  |  |
| `bHUDMarkerNameIndicator` | to do |  | 7 |  |
| `bHUDShowRegionNames` | to do |  | 1 |  |
| `bHUDWeaponNameLabel` | unsure, skipped |  | 5 | needs the plugin's own menu XML |
| `bKeepCrosshairWhenAiming` | to do |  |  |  |
| `bLocationDiscoveredCornerMessage` | to do |  |  |  |
| `bMoveAmmoTypeLabel` | to do |  |  |  |
| `bNoCompassPipsIfNotInDanger` | to do |  |  |  |
| `bNoCrippleCriticalMessages` | to do |  | 3 |  |
| `bNoHUDHotkeyPopup` | to do |  |  |  |
| `bNoUnconciousMessage` | to do |  |  |  |
| `bNoXPBarInCombat` | to do |  | 1 |  |
| `bPatchUnseenCellName` | to do |  | 3 |  |
| `bPlacedMarkerHeightIndicator` | to do |  | 1 |  |
| `bRaceMenuAllowFemaleFacialHair` | to do |  |  |  |
| `bRecentlyDeadNPCIndicator` | to do |  | 4 |  |
| `bRemoveDeadNPCCrippleCriticalMessages` | to do |  |  |  |
| `bScopeVisibleAPHP` | to do |  | 1 |  |
| `bShowDoorsOnCompass` | to do |  | 5 |  |
| `bShowNearestUndiscoveredLocation` | to do |  | 2 |  |
| `bShowQuestObjectivesOnCellChange` | to do |  | 1 |  |
| `bShowSneakLabelWhileStanding` | to do |  |  |  |
| `bShowUsePasswordOnTerminalsWithNote` | to do |  | 1 |  |
| `bSubtitlesShowActorNames` | to do |  |  |  |
| `bTalkWhileSneakingIfShiftIsHeld` | to do |  |  |  |
| `bWaterSourcesShowH2O` | to do |  | 1 |  |
| `iDateFormat` | to do |  | 1 |  |
| `iHUDMaxCompassNPCTicks` | to do |  | 1 |  |

### Menus

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bCapitalizeRecipeCategories` | done | yes |  | Recipe Categories in Capitals |
| `bClickingShowsTerminalText` | done | yes |  | already so: terminal text is shown whole at once |
| `bConsoleNumpadSupport` | done | yes |  | already so: numpad digits type into the console (with Num Lock) |
| `bImprovedRaceMenu` | done | yes | 3 | already so: the creator camera pans and zooms close |
| `bInstantContinueButton` | done | yes |  | already so: Continue loads at once |
| `bMapRemembersPosition` | done | yes | 1 | Map Remembers Its Place (in the session; the save-persistent option not built) |
| `bNumberedComputerHotkeys` | done | yes | 1 | Number Keys on Terminals (1-9, 0 the tenth; the number prefix option not built) |
| `bNumberedDialogHotkeys` | done | yes | 2 | already so for 1-9 (the 0 key and number prefixes not built) |
| `bShowBookEffects` | done | yes | 1 | Skill Book Effects |
| `bShowWeaponAmmoUseInMenus` | done | yes |  | Rounds per Shot |
| `bSkipSkillMenuIfNoPointsToAssign` | done | yes |  | Perks First When No Points |
| `bTakeAllInCompanionContainers` | done | yes |  | already so: Take All shows for companions |
| `bDisableLoadingScreenTips` | done |  |  | No Loading Tips |
| `bNoExitConfirm` | done |  |  | already so: the main menu's Quit asks nothing |
| `bSavingDoesntClosePauseMenu` | done |  |  | already so: saving leaves the pause menu open |
| `bBarterShowCapsChange` | to do | yes | 1 | half_figure has no settings to read; pass the tweak in |
| `bChallengeMenuIcons` | unsure, skipped | yes |  | needs another mod (challenge icons) |
| `bColorRecentlyAddedMapMarkers` | to do | yes |  | needs a per-session found set (newly_found is drained by the compass) |
| `bDisplayItemEffectTotals` | to do | yes |  | aid_preview returns totals only; the per-second part is lost |
| `bHideCursorInDialog` | to do | yes | 1 | the pointer logic (player::grab_cursor) is shared with every menu |
| `bLevelUpScrollWheelSupport` | to do | yes |  | the wheel scrolls the rows; needs the hovered row to take it |
| `bLivingAnatomyShowDR` | to do | yes | 1 | Living Anatomy shows HP and DT; DR not yet |
| `bMainMenuContinueIcon` | to do | yes |  | thumbnails are on the load page only |
| `bPlaceMarkersAtLocations` | to do | yes | 1 | placing a marker on the map is not built |
| `bRecurringChallengeIndicator` | to do | yes | 1 | challenges::Row has no recurring flag yet |
| `bRememberPipboyScrollPositions` | to do | yes |  | the inventory is a new MenuModel on each opening |
| `bRemoveDownloadsButton` | n/a | yes |  | NVForever has no Downloads button |
| `bSaveCharacterSelector` | unsure, skipped | yes | 1 | needs the plugin's own menu XML |
| `bSeparateHorizontalSensitivity` | to do | yes |  | one mouse sensitivity; mouse_look is at 15 parameters |
| `bShowWeaponPoisonEffects` | to do | yes |  | NVForever has no weapon poison yet |
| `bSortEquipableAmmo` | to do | yes |  | rows sort by name (menu/model.rs sort_rows) |
| `bSynchronizeContainerCategories` | to do | yes | 1 | each side keeps its own category |
| `b12HourPipboyClock` | to do |  |  |  |
| `b24HourSleepWaitClock` | to do |  |  |  |
| `bAddInventoryDropItemHotkey` | to do |  |  |  |
| `bAllowRebindNumkeys` | to do |  |  |  |
| `bAmmoLabelUseLongName` | unsure, skipped |  | 1 | needs a UI mod |
| `bBarterQuantityMenuShowsPrice` | to do |  |  |  |
| `bBetterMapZoom` | to do |  |  |  |
| `bBlackJackTotalDisplay` | to do |  |  |  |
| `bChangingContainerCategoryScrollsToTop` | to do |  |  |  |
| `bClickingActiveTabTogglesSorting` | to do |  |  |  |
| `bCompanionsUseLocationFontOnMap` | to do |  |  |  |
| `bCompareArmorStats` | to do |  |  |  |
| `bCompareWeaponStats` | to do |  |  |  |
| `bContainerEncumbranceIndicator` | to do |  |  |  |
| `bContainerMenuStoreAllHotkey` | to do |  | 1 |  |
| `bContainerQuantityMenuShowsWeight` | to do |  |  |  |
| `bContainersShowEquippedAmmo` | to do |  |  |  |
| `bContainerTakeAllDoesntCloseMenu` | to do |  |  |  |
| `bControllerBackButtonDoesntClosePipBoy` | to do |  |  |  |
| `bCustomHackingAnswerLength` | to do |  | 8 |  |
| `bCustomPopupIcons` | to do |  | 17 |  |
| `bDialogueKeepVoiceActingNotes` | to do |  |  |  |
| `bDisableInventoryRightClickDrop` | to do |  |  |  |
| `bDisableShowQuestLocation` | to do |  |  |  |
| `bEmboldenTagSkills` | to do |  |  |  |
| `bHideCompletedQuests` | to do |  |  |  |
| `bHideCursorInMessageMenu` | to do |  |  |  |
| `bHideEquippedItemsInBarter` | to do |  |  |  |
| `bHideEquippedItemsInContainers` | to do |  |  |  |
| `bHideHotkeyedItemsInBarter` | to do |  |  |  |
| `bHideHotkeyedItemsInContainers` | to do |  |  |  |
| `bHideHotkeyedItemsInRepairLists` | to do |  |  |  |
| `bHideMapMarkerFactionReputation` | to do |  |  |  |
| `bHideQuestItemWeightAndValue` | to do |  |  |  |
| `bHideRanksInTraitMenu` | to do |  |  |  |
| `bHideReadNotes` | to do |  |  |  |
| `bHideReputationTabIfEmpty` | to do |  |  |  |
| `bHideUnavailableRadios` | to do |  |  |  |
| `bIndividialItemStats` | to do |  |  |  |
| `bInteriorExteriorMapDoorIcon` | to do |  |  |  |
| `bInvertContainerTitleScrollwheelDirection` | to do |  |  |  |
| `bInvertPipboyRepairMenuSorting` | to do |  |  |  |
| `bKarmaTitleShowsValue` | to do |  |  |  |
| `bKeyRepeatAcceleration` | to do |  | 4 |  |
| `bLocalMapRespawnedCellIndicator` | to do |  | 2 |  |
| `bLockpickAllowMouse` | to do |  |  |  |
| `bLockPickMenuKeyboardMovement` | to do |  | 3 |  |
| `bMapLocationDisplayDistance` | to do |  | 3 |  |
| `bMapMarkersShowFactionName` | to do |  | 1 |  |
| `bMapRecenterHotkey` | to do |  | 2 |  |
| `bMapShowUnconsciousCompanions` | to do |  |  |  |
| `bMarkNotesUnread` | to do |  |  |  |
| `bMoreDetailedStatsMenu` | to do |  | 1 |  |
| `bNoCapitaliseContainerCategories` | to do |  |  |  |
| `bNoExitHacking` | to do |  | 1 |  |
| `bNoMinCompanionMapDistance` | to do |  |  |  |
| `bNoPlaceMarkerPopup` | to do |  | 2 |  |
| `bNoteMenuShowSeparateMenu` | unsure, skipped |  | 1 | needs another mod (Book Menu Restored) |
| `bPauseHolotapes` | to do |  | 1 |  |
| `bPreventNoFastTravelMessage` | to do |  | 1 |  |
| `bPreviewPerksOnLevelUp` | to do |  |  |  |
| `bQuantityMenuRespectsCompanionCarryCap` | to do |  |  |  |
| `bQuestTextVisibleInMenus` | to do |  |  |  |
| `bRepairAllConfirmation` | to do |  | 2 |  |
| `bRepairItemsPreview` | to do |  | 1 |  |
| `bReputationShowsFameInfamy` | to do |  | 3 |  |
| `bRightClickChangesToPreviousContainerCategory` | to do |  |  |  |
| `bShowActiveQuestNotesShowsAllStartedQuests` | to do |  |  |  |
| `bShowAlternateAmmoTypesAvailableInMenus` | to do |  |  |  |
| `bShowBarterTotalWhenOverSellLimit` | to do |  |  |  |
| `bShowCaravanCardsInMiscTabs` | to do |  |  |  |
| `bShowCurrencyInContainers` | to do |  |  |  |
| `bShowInventorySortButton` | unsure, skipped |  | 11 | needs the plugin's own menu XML |
| `bShowModNameInPerkDescriptions` | to do |  | 3 |  |
| `bShowQuestAndNoDRDTItemsInRepairMenu` | to do |  |  |  |
| `bSkipLoadSaveConfirmationPrompt` | to do |  | 1 |  |
| `bSleepWaitSliderShowsWakeTime` | to do |  |  |  |
| `bSortMiscStats` | to do |  | 1 |  |
| `bSortPerkMenu` | to do |  | 1 |  |
| `bSortPipboyNotes` | to do |  | 1 |  |
| `bSortPipboyQuests` | to do |  | 2 |  |
| `bSortPipboyRepairMenu` | to do |  | 2 |  |
| `bSortRecipeMenu` | to do |  | 1 |  |
| `bStartMenuQuickLoad` | to do |  |  |  |
| `bStatsMenuPercentageXP` | to do |  |  |  |
| `bStatsMenuShowEffectTimeRemaining` | to do |  | 1 |  |
| `bTabBackInStartMenu` | to do |  |  |  |
| `bTabClosesPipboyFromKeyAndLimbMenus` | to do |  |  |  |
| `bTabClosesTerminals` | to do |  |  |  |
| `bTakeAllConfirmation` | to do |  | 2 |  |
| `bTerminalGreyReadNotes` | to do |  |  |  |
| `bTerminalInstantDisplayHotkey` | to do |  |  |  |
| `bTimescaleAffectsMinigames` | to do |  |  |  |
| `bVATSAPDisplayIncludesReloadCost` | to do |  |  |  |

### Hotkeys

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `iConsoleKey` | to do |  | 1 |  |
| `iDisableCollisionKey` | to do |  | 1 |  |
| `iDropEquippedWeaponKey` | to do |  | 1 |  |
| `iEquipLastWeaponKey` | to do |  | 4 |  |
| `iExitGameKey` | to do |  | 1 |  |
| `iExitToMainMenuKey` | to do |  | 1 |  |
| `iHideHUDKey` | to do |  | 6 |  |
| `iHolsterWeaponKey` | to do |  | 1 |  |
| `iNextRadioStationKey` | to do |  | 1 |  |
| `iOpenLocalMapKey` | to do |  | 1 |  |
| `iOpenMapKey` | to do |  | 2 |  |
| `iOpenQuestsKey` | to do |  | 1 |  |
| `iOpenRadioKey` | to do |  | 1 |  |
| `iPipboyDataKey` | to do |  | 1 |  |
| `iPipboyItemsKey` | to do |  | 1 |  |
| `iPipboyStatsKey` | to do |  | 1 |  |
| `iPlaceMapMarkerAtPlayerPosKey` | to do |  | 3 |  |
| `iPrevRadioStationKey` | to do |  | 1 |  |
| `iRadioVolumeDownKey` | to do |  | 1 |  |
| `iRadioVolumeUpKey` | to do |  | 1 |  |
| `iRepeatActivateKey` | to do |  | 2 |  |
| `iScreenshotKey` | to do |  | 1 |  |
| `iSkipRadioSongKey` | to do |  | 1 |  |
| `iToggleCrosshairKey` | to do |  | 1 |  |
| `iToggleMenusKey` | to do |  | 1 |  |
| `iTogglePipboyLightKey` | to do |  | 2 |  |
| `iToggleSightingNodeHotkey` | to do |  | 2 |  |
| `iToggleSmoothCameraKey` | to do |  | 2 |  |
| `iToggleSneakIndicatorKey` | to do |  | 1 |  |
| `iToggleTrueIronSightsKey` | to do |  | 1 |  |
| `iVATSAcceptKey` | to do |  | 1 |  |

### Cheat

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bFastTravelWithEnemiesNearby` | done |  |  | Fast Travel Near Enemies |
| `bSleepInOwnedBeds` | done |  |  | Sleep in Owned Beds |
| `bAddSetHomeCommand` | to do |  |  |  |
| `bAllowPickpocketIfAlreadyCaught` | to do |  |  |  |
| `bDebugLockpickMenu` | to do |  |  |  |
| `bGodModeAllowsCraftingWithoutRequirements` | to do |  |  |  |
| `bHackTerminalsWithoutSkill` | to do |  |  |  |
| `bNoCasinoBans` | to do |  |  |  |
| `bNoHackingRetryDelay` | to do |  |  |  |
| `bNoMaxCasinoBet` | to do |  |  |  |
| `bPickpocketWornItems` | to do |  | 1 |  |
| `bReallocateSkillPointsOnLevelup` | to do |  |  |  |
| `bSleepWaitAnywhere` | to do |  | 6 |  |
| `bWeightlessAidItems` | to do |  |  |  |
| `bWeightlessItems` | to do |  | 5 |  |

### Misc

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bMeleeImpactEffectsFollowTarget` | done | yes |  | already so: decals on actors hang from a bone |
| `bPowerArmorPreventsScreenBlood` | done | yes |  | No Screen Blood in Power Armor (any hit while power armor is worn on the body; hits are not told apart by body part here) |
| `bRadioStaticDecreasesSongVolume` | done | yes |  | already so: the song fades as the static rises |
| `bAllowKeyboardAndMouseWithControllerConnected` | to do | yes |  | the Input Device setting picks one device; needs both at once |
| `bArmorPreventsBloodDecals` | to do | yes |  | the impact is sent before DT is worked out |
| `bArmorSoundsPlayIn3D` | to do | yes |  | the player's foley plays at the listener (anim_sounds player_keys) |
| `bBetterFlycam` | to do | yes | 5 | up/down and wheel speed are already there; auto-move is not |
| `bCompressDuplicateConsoleMessages` | to do | yes |  | console print pushes every line |
| `bConsoleBackground` | unsure, skipped | yes |  | needs the plugin's own menu XML |
| `bConsolePrintsIncludeTimestamp` | to do | yes | 1 | the console print path also writes the scof file |
| `bConsoleTextShadow` | to do | yes |  | the console draws green text on a dark panel, no shadow |
| `bCustomScreenshotFormat` | n/a | yes | 1 | NVForever writes its own screenshots |
| `bDontAllowConsoleTillLoadingIsDone` | n/a | yes |  | the original's start-up order |
| `bHeatbeatSoundsFade` | to do | yes | 2 | NVForever plays no heartbeat yet |
| `bImprovedWeather` | to do | yes | 1 |  |
| `bModConsolePrintsIncludeName` | to do | yes | 1 | console print does not know which plugin printed |
| `bMoreResponsiveControllerAiming` | to do | yes |  | the stick curve is x times |x| (player.rs); choose the new curve |
| `bMultithreadedHackingMenu` | n/a | yes |  | performance work on the original |
| `bNoDespawnVisibleStuckProjectiles` | to do | yes |  | projectiles do not stick in NVForever yet |
| `bNoInteriorBlackLoadingScreen` | to do | yes | 1 | unsure what to leave out: NVForever shows the picture, not black |
| `bNoPlayerNameLimit` | to do | yes |  | the name box is laid out for 24 characters; check its drawing first |
| `bPartialReloads` | unsure, skipped | yes |  | depends on animation file naming conventions not yet checked |
| `bRememberConsoleHistory` | to do | yes | 1 | needs a file in the NVForever home |
| `bUseAnimVariants` | unsure, skipped | yes | 3 | depends on animation file naming conventions not yet checked |
| `bVATSStopFollowingDeadTargets` | to do | yes |  | needs the kill camera shot's start frame kept |
| `bWeaponImpactEffects` | to do | yes |  | hits on the hand weapon use the flesh impact; holstered weapons have no hit part |
| `bAllowDuplicateControlBinds` | to do |  |  |  |
| `bAlternateLevelupSounds` | to do |  | 4 |  |
| `bAnimDebugging` | n/a |  |  | a mod-author debug print |
| `bAutoContinueGame` | to do |  |  |  |
| `bAutoReadNotes` | unsure, skipped |  | 3 | needs another mod (Book Menu Restored) |
| `bChanceBasedKillcams` | to do |  | 1 |  |
| `bClickToExitLoadScreens` | to do |  | 2 |  |
| `bContainerRespawnsMessage` | to do |  | 2 |  |
| `bContinueGameHotkey` | n/a |  |  | a mod-author debug key |
| `bControllerTriggerDeadzones` | to do |  | 2 |  |
| `bCustomLocationDiscoveredSound` | to do |  | 1 |  |
| `bDetachedBeams` | to do |  |  |  |
| `bDialogMenuDebugging` | n/a |  |  | a mod-author debug print |
| `bDisableAchievements` | n/a |  |  | NVForever has no achievements |
| `bDisableCharacterRespec` | to do |  |  |  |
| `bDisableControllerDeadzones` | to do |  | 2 |  |
| `bDisableGodmodeOnLoad` | to do |  |  |  |
| `bDisableRegionBordersKeepMessage` | to do |  |  |  |
| `bDisableSellingItems` | to do |  |  |  |
| `bDisableStealthEffectInPipboy` | to do |  |  |  |
| `bDisableWeaponHotkeys` | to do |  |  |  |
| `bDisarmingMinesPicksThemUp` | to do |  |  |  |
| `bDontOpenDoorsAfterLockpick` | to do |  |  |  |
| `bDontShowContainerAfterLockpick` | to do |  |  |  |
| `bExtraConsoleDetails (enabled by default)` | to do |  | 4 |  |
| `bHapticPipboy` | to do |  |  |  |
| `bInvalidBSFadeNodePrints` | n/a |  |  | the plugin's own warning |
| `bInvertCameraX` | to do |  |  |  |
| `bKeepPipboyLightOnCellChange` | to do |  | 1 |  |
| `bKeepSelectedConsoleRef` | to do |  |  |  |
| `bLethalHitsIncreaseLimbChallenges` | to do |  |  |  |
| `bMenuFadesIgnoreTimescale` | to do |  |  |  |
| `bNoAltTabPause` | to do |  | 1 |  |
| `bNoAmmoCasings` | to do |  |  | NVForever gives no casings yet |
| `bNoAudioDistortion` | to do |  | 1 |  |
| `bNoFastTravelTimeChange` | to do |  |  |  |
| `bNoHotkeyEquipSounds` | to do |  |  |  |
| `bNoPipboyOnAltTab` | n/a |  |  | the original's alt-tab handling |
| `bNoScrollwheelPOVChange` | to do |  | 2 |  |
| `bNoSkillBooksAbove100` | to do |  |  | the refusal belongs before equip_at takes the book (Pip-Boy, hotkeys, EquipItem) |
| `bNoSkipDialog` | to do |  |  |  |
| `bNoTurningAnim` | to do |  |  |  |
| `bOnlyChangeCameraHeightIfPOVKeyHeld` | to do |  |  |  |
| `bOpenPipboyToInventoryByDefault` | to do |  | 1 |  |
| `bPipBoyTabHotkeysDontCloseMenu` | to do |  |  |  |
| `bPreventNPCComments` | to do |  | 1 |  |
| `bPreventTeammateFootstepSounds` | to do |  |  |  |
| `bPrintErrorsToConsole` | n/a |  | 5 | the original's error log |
| `bPrintNewModsOnLoad` | n/a |  |  | the plugin's own load report |
| `bRadiationCausesRadioStatic` | to do |  | 1 |  |
| `bRadioKeepPositionWhenLoading` | to do |  |  |  |
| `bReloadSoundsAffectedByTimescale` | to do |  |  |  |
| `bReplaceHelpWithConsole` | to do |  |  |  |
| `bRestoreLockedDoorSound` | to do |  |  |  |
| `bScaleAshpileSize` | to do |  | 2 |  |
| `bScaleRadioVolume` | to do |  | 2 |  |
| `bSilentSneakPowerAttacks` | to do |  |  |  |
| `bSplashDamageTorsoGibbing` | to do |  |  |  |
| `bSwapKeyboardYZKeys` | n/a |  |  | the original's keyboard handling; NVForever's keys can be rebound |
| `bTerminalFadeTime` | to do |  | 1 |  |
| `bTogglePOVWhenHolsteringWeapon` | to do |  |  |  |
| `bUKKeyboard` | n/a |  |  | the original's keyboard handling |
| `bUseConsoleOutputFile` | to do |  | 1 |  |
| `bUseFallout3AudioDistortion` | unsure, skipped |  |  | needs the original sequel's filter described first |
| `bUseHitLocationInCinematicKillcams` | to do |  |  |  |
| `bVanityCamReturnsToFirstPerson` | to do |  |  |  |
| `bWeaponVisibleDuringDialogue` | to do |  |  |  |

### Fixes

| INI key | Status | VNV on | Options | Note |
|---|---|---|---|---|
| `bAmmoBurstCaseCountFix` | skipped: fix | yes |  |  |
| `bCacheQuestAndNoteMenu` | skipped: fix | yes |  |  |
| `bExplosionKnockbackDirectionFix` | skipped: fix | yes |  |  |
| `bFixKillChallengeSourceWeapon` | skipped: fix | yes |  |  |
| `bNoGibSoundWhenEnteringCells` | skipped: fix | yes |  |  |
| `bPreventInactiveWindowScrolling` | skipped: fix | yes |  |  |
| `bSortUnavailableRadiosToBottom` | skipped: fix | yes |  |  |
| `bSpinWeaponsSoundFix` | skipped: fix | yes |  |  |
| `bFixEnhancedCameraGroundSinkBug` | skipped: fix |  |  |  |
| `bUltrawideSupport` | skipped: fix |  | 1 |  |
