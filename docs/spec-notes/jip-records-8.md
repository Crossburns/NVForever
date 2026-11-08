# JIP LN NVSE, batch 8 (2026-11-08, #1006)

Actor base data, actor effects, ingestibles and base effects, weapon fields and mods,
idles, body-part names and limb models, cells and water, in
`crates/fnv-game/src/nvse_jip8.rs`. Sources: the GECK wiki export, each function's page
[documented: the page named]; the fields: xEdit's record definitions as the schema keeps
them (`schemas/fnv`). Not probed in the original; choices the pages leave open are tagged
[hypothesis]. Setters change the form for the session and are read by this batch's
getters and the earlier ones (GetBodyPartName, GetBodyPartData, GetSkeletonModel,
GetBaseEffectFlag, GetEffectFlag, GetCellFlag, GetCellWaterForm, GetWaterFormEffect,
GetWeaponItemMod, GetWeaponItemModEffect, GetWeaponItemModValue1, GetWeaponOnHitSetting,
GetWeaponSemiAutoFireDelay, GetWeaponModel). The setters return 0 [hypothesis: as the
record setters of batch 5 were observed to, jip-records-5.md, though the catalogue names
a result for some].

47 functions; JIP 248 → 201.

## 1. Actor base data

A `[baseForm]` argument, else the calling reference's base; not an NPC or creature, the
page's nothing (0, none) [documented: each page's `ref.F` and `F … baseForm` forms].

| Function | NVForever |
|---|---|
| Get/SetDeathItem `[item] [base]` | INAM |
| GetActorTemplate `[base]` | TPLT |
| Get/SetActorLevelingData `type [value] [base]` | ACBS: 0 the fixed level, or the level mult when the actor has PC Level Mult (ACBS flag bit 7; the record keeps it × 1000), 1 calc min, 2 calc max [documented: the table and the example's "level multiplier to 0.75"]; past 2, 0 |
| GetBaseFactionRank `faction [base]` | the base's SNAM rank, or its template's when it takes its factions from one; -1 when not a member [documented; hypothesis: the template, as GetBaseFactions] |
| GetInFactionList `list` | the calling actor in one of the list's factions now (AddToFaction and the rest counted) [documented] |
| GetNumActorEffects, GetNthActorEffect `[index] [base]` | SPLO in record order; past the end none |
| Add/RemoveBaseEffectListEffect `effect [base]` | the SPLO list, an actor effect (SPEL) only, once [documented: "the specified Actor Effect"; hypothesis: once] |
| SetBodyPartData `creature bpd` | CREA PNAM [documented]; GetBodyPartData reads it |
| SetCRESkeletonModel `creature path`, SetNPCSkeletonModel `npc [path]` | MODL; an NPC's path left out, sNPCModelMale or sNPCModelFemale by its ACBS Female bit [documented]; GetSkeletonModel reads it |
| Get/SetExcludedCombatActions `[mask]` | the calling actor's mask of the 27 combat actions, bits past 26 dropped; 0 until set; not saved [documented: "reset upon loading a game"] |

The actor records keep INAM, TPLT and SPLO for these (`ACTOR_SUBS`, data.rs).

## 2. Ingestibles and base effects

| Function | NVForever |
|---|---|
| Get/SetAddictionChance `alch [chance]` | ENIT addiction chance, 0-1 [hypothesis: the page's example, 0.2] |
| Get/SetAddictionEffect `alch [effect]` | ENIT withdrawal effect, an actor effect or none |
| Get/SetBaseEffectScript `mgef [script]` | the associated script of a script-type base effect (archetype 1); any other, none and unchanged [documented: "script-type Base Effect"] |
| SetBaseEffectFlag `mgef flag set` | MGEF DATA's bit, only the IDs of the page's table (0-10, 12, 13, 19, 20, 24, 27, 28) [documented] |
| SetEffectFlag `form flag set` | an actor effect's SPIT bits 0, 2, 4-7; an object effect's ENIT 0 and 2; an ingestible's ENIT 0-2 [documented: the three tables]. GetEffectFlag now reads an actor effect's SPIT (it read ENIT for every kind) |

## 3. Weapons

| Function | NVForever |
|---|---|
| Get/SetWeaponDetectionSoundLevel `weapon [level]` | VNAM, 0 loud, 1 normal, 2 silent [documented] |
| SetWeaponItemMod `weapon slot [mod]` | WMI1-3; left out, none [documented: the examples]; a weapon mod (IMOD) only [hypothesis] |
| SetWeaponItemModEffect, SetWeaponItemModValue `weapon slot value` | DNAM effect_mod1-3 (0-14, the page's table; the JohnnyGuitar note's 15 and 16 not taken) and value_a_mod1-3 |
| SetWeaponOnHitSetting `weapon 0-3` | DNAM on_hit [documented] |
| SetWeaponSemiAutoFireDelay `weapon setMax delay` | DNAM's min or max delay, not below 0 [hypothesis] |
| SetWeaponModel `weapon mask path` | 0 MODL, 1-7 MWD1-7, as GetWeaponModel reads them [documented: the table] |
| Get/SetWeaponModReloadAnim `weapon [anim]` | DNAM reload_animation_mod, 0-22 [the enum: xEdit WEAP] |
| WeaponHasModType `weapon type` | the slot 1-3 whose effect is the type, else 0; a placed weapon by its base [documented]; the type IDs are DNAM's effect values [the enum: xEdit WEAP] |
| EquippedWeaponHasModType `type` | an attached mod of the calling actor's equipped weapon (its mod bits 1, 2, 4) has the type [documented] |
| SetEmbeddedWeaponAV `weapon av` | DNAM's embedded weapon actor value: PerceptionCondition … BrainCondition as 0-6 [the enum: xEdit WEAP; documented: the example]; any other actor value changes nothing |
| SetEmbeddedWeaponNode `weapon node` | NNAM [documented] |

## 4. Idles and body parts

| Function | NVForever |
|---|---|
| Get/SetIdleLoopTimes `idle getMax` / `idle min max` | IDLE DATA loop_min, loop_max (u8) [documented] |
| Get/SetBodyPartReplacementModel `bpd part [path]` | the part's NAM1; "" without one or without the part; a part the record lacks is not added [hypothesis] |
| SetBodyPartName `bpd part name` | the part's BPTN, read by GetBodyPartName |

A part is the group of subrecords from its BPTN (or its BPNN when it has no name) to the
next part, holding the BPND of that part type [documented: xEdit BPTD].

## 5. Cells and water

| Function | NVForever |
|---|---|
| SetCellFlag `cell flag set` | 0-7 DATA's bits (4 is not in the table and is refused), 8-16 the lighting template inherit flags, LNAM bits 0-8 [documented: the table; hypothesis: 8-16 are LNAM's, in its order, which the names match]. GetCellFlag reads 8-16 the same way now (it read DATA, so 0) |
| SetCellWaterForm `cell water` | XCWT, a water (WATR) [documented]; GetCellWaterForm reads it |
| SetWaterFormEffect `water [effect]` | the actor effect, or none; GetWaterFormEffect and drinking (`Game::drink_from`) read it |
| GetWaterTrait `water trait` | 0 opacity (ANAM), 1 the flags (FNAM), 2-50 DNAM in its order; colours 12-14 as R × 1000000 + G × 1000 + B; the table's "Unknown" 9 and 15 (unused bytes), and past 50, 0 [documented: the table] |

## 6. Not built here, and follow-ups

- SetBaseFactionRank: the base factions feed detection, combat and templates straight
  from the loaded records (`GameData::actors`, `base_factions_at`); a session change
  needs those readers moved onto one lookup first.
- Get/SetActorUnarmedEffect: the NPC and creature records' unarmed attack effect is not
  in the schema yet (`schemas/fnv` NPC_ and CREA list no EITM).
- The engine and the game rules still read these records as loaded: the combat AI does
  not skip excluded actions; spawned actors take their abilities from the record's SPLO;
  magic effects apply with the record's flags and script; combat reads the weapon's mods,
  on-hit setting, delays and sound level as loaded; the idle player its loop counts;
  dismemberment its limb models; the level calculation ACBS as loaded; drinking-water
  radiation (`radiation::water_rads`) the record's effect.
