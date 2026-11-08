# ShowOff events, explosion damage, aux timer stops and the jam animation

Plain-language notes for `crates/fnv-game/src/nvse_showoff.rs`. Sources: the GECK wiki
pages of each function [documented] and the showoff1 probe set run in the original game on
2026-10-27 [observed] (`tools/capture/jip_probes.py`; goldens stay private). The pages of
the ShowOff events themselves are not in the wiki dump, so their arguments come only from
showoff1: each event had one handler taking no arguments and one taking two refs, and both
ran.

## 1. ShowOff:OnAdd and GetAddedItemRefShowOff

- The event fires when an item reaches a container. With a stimpak AddItem'd to the player,
  a two-argument handler got the item's base form (type 47) first and the player second; a
  handler taking no arguments ran as well [observed].
- Inside the handler, `GetAddedItemRefShowOff` gives a reference whose base is the stimpak
  [observed: IsReference 1, base type 47]; the page says a world item reference or an
  inventory reference [documented]. NVForever gives an inventory reference to the item in
  the container. Outside a handler it gives none.
- NVForever fires it wherever its script OnAdd block would run for an item reaching a
  container (AddItem and its variants, RemoveAllItems into a container, picking up).

## 2. ShowOff:OnExplosionHit and the explosion's damage

- The event fires when an explosion's blast reaches an actor, before the damage lands. A
  MineFragExplosion (FalloutNV.esm 07078B) placed at the player gave a two-argument handler
  the player first and none second [observed].
- `GetExplosionHitDamage` read 100 inside the handler; after `SetExplosionHitDamage 5` the
  player lost 5 Health [observed]. Both are unusable outside the event [documented]; outside
  it NVForever reads 0 and ignores a set.
- NVForever asks for the damage in the engine's blast loop, before armour is taken off, and
  deals what the handlers leave [hypothesis: whether the original's value is before or after
  armour is not measured; the player in nvfairport took exactly the set value].

## 3. SetOnAuxTimerStopHandler

- `SetOnAuxTimerStopHandler 1/0 handler timerName [owner]` adds or removes a handler for an
  aux timer of an owner, the player when none is given; the handler gets the timer's name
  and its owner [documented].
- It runs when the timer runs out: a 0.1 second timer had run out within 30 frames, its time
  left read -1 (gone) and the handler had run once, with the player as owner [observed]. It
  runs again on `AuxTimerStop name 1` [observed]; AuxTimerStop fires the event only when its
  fireEvent argument is non-0 [documented].
- NVForever checks the running timers each frame and erases and reports the ones whose time
  is up. Before this, a timer whose time was up stayed with 0 seconds left.

## 4. ForceWeaponJamAnim

- It returned 0 with the 10mm pistol away and 1 with it drawn [observed]. IsAnimPlayingEx 4
  (reload) never read 1 over the 60 frames after [observed], so the jam is not a reload as
  that function counts them.
- A weapon without a jam animation gives 0 [documented: the .357 revolver cannot jam].
  NVForever does not read which weapons have one yet, so any drawn weapon gives 1
  [hypothesis], and the engine does not play the animation [not built].

## 5. The 2026-10-30 batch (#1010, `nvse_showoff2.rs`)

Checked in the original game with the probe set `showoff2` (`tools/capture/jip_probes.py`,
lab `L5.ext.showoff2`): 49 of 50 rows match.

| Function | NVForever |
|---|---|
| GetEquippedWeapon | the equipped weapon's base, Fists (000001F4) when none [documented] |
| IsEquippedWeaponMelee, IsEquippedWeaponRanged, IsWeaponMelee `[weapon]` | melee is a weapon type (DNAM animation type) of 2 or below [documented: "type is <= 2"]; with no weapon, melee 1 and ranged 0 [documented] |
| GetItemCanHaveHealth `[item]` | a weapon or armor [documented] |
| IsLimbCrippled `[limb] [threshold]`, GetCrippledLimbsAsBitMask `[threshold]`, GetNumCrippledLimbs `[threshold]` | a limb's condition at or below the threshold (0); limbs head 0 … brain 6, bits head 1 … brain 64; -1 or none any limb [documented; observed: DamageAV 200 on the left leg gives 16, 1, 1] |
| GetActorValueDamage (GetAVDamage) `av` | the damage, no more than the value had [observed: DamageAV 200 on a limb of 100 reads 100] |
| GetActorValueName (GetAVName) `code` | the script name: 1 Confidence, 25 PerceptionCondition, 76 DamageThreshold, no spaces [observed]. The game compiles the code only in parentheses, as the page's example has it |
| GetAmmoName `[type] [ammo]`, SetAmmoName `name [type] [ammo]` | 0 FULL, 1 ONAM (the short name), 2 QNAM (the abbreviation) [documented: the page; xEdit AMMO]; a name the record lacks reads "<no name>" [observed: 10mm's abbreviation] |
| GetIngestibleConsumeSound `[aid]`, SetIngestibleConsumeSound `sound [aid]` | ENIT's consume sound [observed: the stimpak's 000296B9]; the setter true when set [documented; the game did not compile it in a `let`, so it is not observed] |
| GetCreatureFootWeight, GetCreatureTurningSpeed `[creature]` | WNAM, TNAM; -1 for no creature [documented; observed: the dog 1 and 90, Doc -1] |
| SetCreatureFootWeight, SetCreatureTurningSpeed, SetCreatureBaseScale, SetCreatureReach `value [creature]` | WNAM, TNAM, BNAM, RNAM for the session; refused below 0; 1 when set [documented; observed: turning speed 45 read back, -1 refused] |
| GetCellEncounterZone (GetCellZone) `cell` | the cell's zone [documented; observed: Doc's house none] |
| GetZoneRespawns `zone` | not "Never Resets" [documented; observed: Vault 11 0, Hoover Dam 1] |
| IsBaseForm `form`, IsReferenceCloned, IsActorAlt `[form]` | a record that is no placed reference; a reference made in play (0xFF); an NPC or creature [documented; observed: the pistol 1, the player and Doc's reference 0] |
| ModNumericGameSetting `setting by` | the setting changed by the amount; 0 for no such setting [documented; observed: fMoveRunMult 4 + 1 = 5] |
| GetShowOffDebugMode, SetShowOffDebugMode | the switch kept; nothing more is printed |

The one row that differs is not ShowOff's: a `string_var` given to SetAmmoName's name
reached it as the variable's name, "sText", in the game [observed: showoff2
ammoname.restore], as SetUIStringAlt's `%z` did in gaps1. The game's runtime compiler
seems to pass a string variable's name where a parameter takes literal text [hypothesis];
NVForever passes the value. A third case: SetWeaponWorldModelPath given a `string_var` set
the path to the variable's name, "sSave" [observed: jg1, before the step was changed to a
literal].

## 6. The 2026-11-06 batch (#1010, `nvse_showoff3.rs`)

From the pages [documented], then checked in the original on 2026-11-08 with the
`showoff3` probe set (`tools/capture/jip_probes.py`, the airport save; lab scenario
L5.ext.showoff3), settled values tagged [observed: showoff3 <row>]; the census drops from
111 ShowOff functions to 73. Tests: `crates/fnv-game/tests/showoff3.rs` on a synthetic
plugin. None of the remaining ShowOff functions is called by the owner's two mod lists
(`docs/handoff/modlist-function-counts.json`: RemoveAllItemsShowOff once, the rest never),
so the batch was chosen by how well the pages describe a function and whether NVForever
has the system it reads.

Headless on FalloutNV.esm (`fnvtool script run`, new game, 2026-11-06) [observed]: the
player starts with the slots 64 + 8 taken (the Pip-Boy, non-playable) and no playable item
(GetEquippedItems 0 entries, with mask 64 one); with the 10mm pistol equipped the mask is
104 and GetBaseEquippedWeight 3; GetCalculatedMaxCarryWeight 200; TopicInfoGetResponseStrings
on 000E8911 gives the three texts the page's example lists, the first one word for word.

| Function (aliases) | NVForever |
|---|---|
| GetEquippedItems, GetEquippedItemRefs `[mask]` | the equipped items, bases or inventory references, each once. No mask: every playable item in a slot; slot bits 0–19 (GetEquippedObject's slots): the items in those slots, playable or not [documented: the pages' examples]. The mask's further filters are on a template the export lacks; bit 20 alone or with the weapon bit picks nothing [observed: showoff3 so3.items.bit20, .bit20.weapon]; items without a slot (ammo) do not count [hypothesis]. Listed by their lowest slot: the Vault 21 jumpsuit (upper body), the Pip-Boy glove (left hand), the Pip-Boy [observed: showoff3 so3.items.allslots]. The other equipped rows differ from the headless run by what the save wears only (the jumpsuit; no weapon in either) [observed: so3.items.*, bitmask, broken, weight.*] |
| GetEquippedItemsAsBitMask (GetOccupiedEquipSlots) | the slots taken, non-playable items too [documented]: a weapon slot 5, armour its BMDT biped flags |
| GetNumBrokenEquippedItems (GetNumBrokenEq) `[healthPerc] [mask]` | weapons and armour at or below the health percentage, 0 by default [documented] |
| GetBaseEquippedWeight, GetCalculatedEquippedWeight `[minWeight] [mask]` | the base weights; the weights after Modify Light Items, Adjust Heavy Weapon Weight and the weapon's Decrease Weight mods (the inventory's own rules) [documented]. An item counts when that weight is at least minWeight [hypothesis: which weight the minimum is held against] |
| GetCalculatedMaxCarryWeight | Carry Weight with the Get Max Carry Weight perks [documented] |
| UnequipItems `[mask] [noEquip] [hideMessage] [triggerOnUnequip]` | UnequipItem on each picked item [documented]; OnUnequip runs whatever the last argument says [not built] |
| GetEquippedWeaponRef, GetEquippedItemRefForItem `itemOrList` | the weapon, or the item or the first worn item of a form list, as an inventory reference; 0 with none [documented] |
| SetRandomizerSeed (SetSeed) `[seed]`, SetSeedUsingForm (SetFormSeed) `[form]`, GetRandomizerSeed (GetSeed), RandSeeded (GenerateSeededRandomNumber) `min max`, GetRandomPercentSeeded, AdvanceSeed `n` | one seed per calling plugin; the default 5489; -1 with no seed; min..=max both included, -1 for min above max; 1–100; AdvanceSeed 0 with no seed [documented]. The draws are MT19937's (5489 is its published default seed; checked against ISO C++ [rand.predef]'s 10000th output), scaled onto the range as min + floor(x × (max - min + 1) / 2^32), a draw whose product's low 32 bits fall under 2^32 mod (max - min + 1) thrown away for the next (0-1000000000 skips the third output for 5489) [observed: so3.seed.range.wide]: seed 5489 draws 82 14 91 84 13 from 1-100, seed 20 59 89 90 96 90 [observed: showoff3 so3.seed.default, .20.twice, .20.nextframe, .advance, .range.wide, .range.bits]. The same seed repeats its sequence, in one frame and the next [observed: so3.seed.20.twice, .20.nextframe]; SetRandomizerSeed 0 or none is 5489, and a default seed made by a first draw counts as set (AdvanceSeed 1) [observed: so3.seed.0, .noarg, .advance.afterdefault]. Seeds and bounds above 2^31 - 1 are held to it: 4294967295 reads 2147483647 and draws as that seed [observed: so3.seed.big, .range.uint]. SetSeedUsingForm takes only its argument: called on a reference with none it sets 5489 [observed: so3.seed.form.ref, .form.caps 15]. A fractional bound does not compile, and min above max is not probed (the page says the game crashes) [observed: so3.seed.range.fraction] |
| ApplyEasingAlt (EaseAlt) `input code mode` | ApplyEasing's curves by code, 0 sine … 9 bounce [documented; observed: showoff3 so3.ease.0 … .9]; -1 for another code [documented] or a mode other than 0-2 [observed: so3.ease.badmode]; an input outside 0-1 is eased as it is, 1.5 by quad-in 2.25 [observed: so3.ease.outside] |
| GetPosArray | X, Y, Z [documented] |
| IsOutsideMainThread | 0: every script runs on the main thread here |
| ConditionPrint | 1, and a console line naming the reference in NVForever's own words [documented] |
| IsAnimPlayingExCond `category [subType]` | IsAnimPlayingEx [documented] |
| IsJumping | IsInAir: the page marks it broken, it "functions equivalently to IsInAir" [documented] |
| GetNumActorsInRangeFromRef `maxDistance [flags]` | the loaded actors within the distance (3-D, same space); 1 no dead, 2 no Invisibility or Chameleon above 0, 4 only actors in combat the caller detects [documented]. The caller does not count itself [hypothesis] |
| SetIsPCAMurderer `0/1` | the flag IsPCAMurderer reads [documented] |
| GetPCHasSleepWaitOverride, SetPCHasSleepWaitOverride `0/1` | EnableFastTravel's can-wait flag, which the wait menu checks [documented: EnableFastTravel "can modify this canSleepWait override flag"] |
| GetChallengeProgress `challenge` | the progress the challenge system keeps: a non-recurring challenge done stays at its threshold, a recurring one counts again from 0 [documented] |
| GetCalculatedSkillPoints, GetCalculatedPerkPoints `[levelOverride]` (…EarnedPerLevel) | the points of the next level, or of this one while the level-up menu is open: levelup.md's skill rate with Adjust Gained Skill Points; 1 at a level iLevelsPerPerk divides, else 0 [documented: "can only return 1 or 0"] |
| SetAllyTemp (SetAllyShowOff), SetEnemyTemp (SetEnemyShowOff) `f1 f2 [flag12 flag21]` | SetAlly's and SetEnemy's reactions, not saved [documented]; they win over the saved ones until SetAlly or SetEnemy is called on the pair [hypothesis] |
| AuxTimerTimeToCountdown `name [owner]` | the time AuxTimerStart gave, -1 when the timer is not running [documented], the name in any case [observed: showoff3 so3.aux.countdown]. The owner argument is a base form: a reference named there stands for its base, so a reference's running timer reads -1 that way and its time when called on the reference [observed: so3.aux.countdown] |
| SetOnAuxTimerStartHandler (SetOnTimerStartHandler), SetOnAuxTimerUpdateHandler (SetOnTimerUpdateHandler) `1/0 handler name [owner]` | the start handlers get the name and the owner on each AuxTimerStart; the update ones, each frame, the name, the owner and the time since the last update (1 for a timer without flag 4), only for timers started with flag 256 [documented; observed: showoff3 so3.aux.update.counts, .frames, a frames timer's time 1]. A handler set or removed takes effect at the next frame: a start in the step that set it runs nothing, a restart a frame later runs it at once, and a restart in the step that removed it still runs it [observed: so3.aux.start.reg, .restart, .remove, .after]. NVForever's aux timers still count seconds whatever the flags say [not built: frames, the menu-mode flags] |
| GetRadiationExtraData, SetRadiationExtraData `rads` | XRAD; a set level reaches the player's exposure (a reference with no XRDS radius gives nothing) [documented]; for the session [not built: saving] |
| ModNumericINISetting `setting by` | GetNumericINISetting plus the amount, set back; 0 for no such setting [documented] |
| TopicInfoGetResponseStrings `info` | the response texts in order [documented; observed: showoff3 so3.responses.caravan, three of 144, 120 and 90 characters]; no array (ar_Size -1) for a form that is not a line [observed: so3.responses.stimpak] |

Left unbuilt (73), with the reason:

- No page, or an empty one, in the export: the seven Matrix functions, SayTo_GetTopic,
  SayTo_GetTopicInfo; GetActorFadeState, GetCalculatedActorSpread and IsFormLoading
  (pages marked Incomplete, with no summary).
- They write files or caches (ReadFromJSON, WriteToJSON, CreateFolder, DumpFormList,
  HasINISetting_Cached, ReloadIniCache, SaveCachedIniFile): left for a file batch that
  decides where NVForever may write (never the game folder; install-layout.md).
- A system NVForever lacks or does not expose to scripts: DisableAlt, EnableAlt and
  SetEnableParent (enable parents are fixed from the records), the level-up menu's pages
  and points (Get/SetLevelUpMenuCurrentPage, GetLevelUpMenuPoints,
  SetLevelUpMenuCanExitEarly, ShowPerkMenu, ShowSkillMenu), ShowPauseMenu, live
  explosions and projectiles (Get/SetExplosionRefRadius/Source, GetProjectileRefIsStuck,
  SpawnTracingProjectile, SetForceDrawHitscanProjectiles,
  SetProjectileTracerChanceOverride), the hotkey icons (Get/SetItemHotkeyIconPath), the
  Improved Detection entry point (IsActorInvisibleToPlayer,
  HighlightAdditionalReferenceAlt), combat allies (GetNumCombatActorsFromActor), the
  response-text setters (TopicInfoSetResponseStrings, TopicInfoSetNthResponseString: the
  dialogue reads the records directly), ForceHitStaggerReaction, ForceRecoilAnim,
  FreezeAmmoRegen, the corner-message queue's handler and ClearCinematicTextQueue,
  ToggleQuestMessages, PatchFreezeTime, ResetInteriorAlt, SetOwnershipTemp, the
  pickpocket switch (Get/SetPlayerCanPickpocketEquippedItems), ClearShowoffSavedData
  (the aux timers are not kept per plugin).
- Pages too thin to build from: GetPCHasScriptedFastTravelOverride (which flag is the
  override), GetPCCanFastTravel, KillAllHostiles ("within range" unstated),
  GetCalculatedItemValue, GetCalculatedAPCost, GetVATSMaxEngageDistance, ToANSIChar
  (keyboard layouts), GetHitLocationLingering, PlayerHasNightVisionActive,
  PlayerIsDrinkingPlacedWater, GetIsActivationPromptShown, IsPlayerLookingAround,
  GetSpellUsageNumEx, GetVATSTargetable, PlaceAtReticleAlt, RefillPlayerAmmo (deprecated
  for ITR's RefillAmmo), RemoveAllItemsShowOff (its page says its flags are buggy).
