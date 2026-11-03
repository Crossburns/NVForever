# Spec note: JIP LN batch 4 (record traits and game-state getters)

The fourth JIP batch (issue #955, `crates/fnv-game/src/nvse_jip4.rs`, 2026-10-30). The
census (`extender_census`) counted 390 of JIP LN's 768 functions unbuilt before it, and 344
after. The owner's mod lists (docs/handoff/modlist-function-counts.json) call only 40 of the
390 names, each a few times, and most of those need the engine: the cursor, NIF material
properties, the menus. So this batch takes the families that already have built siblings
(the projectile, explosion, ammo, weapon, sound and weather traits; jip-records.md,
`nvse_traits.rs`) and can be read from the records headless [decision].

Wiki statements are `[documented: <page>]` (the private export
`C:\fnv-private\geckwiki\functions.xml`). Field names are the Layer 1 schema's
(`schemas/fnv`). The probe set `jip4` (tools/capture/jip_probes.py, 281 rows, golden
`jip/jip4.json`, lab scenario `L5.ext.jip4`) ran these functions in the original game
with JIP 57.30 from the `nvftest` save on 2026-10-30, three times; the rows that two runs
share came back the same but for one GetDelayElapsed row (§10). Values marked [observed]
are from it. The example values in §2–§9 are the game's and NVForever's alike.

## 1. Rules shared by every setter

- A setter changes the record for the session only. It is kept in `Game::field_overrides`,
  so every reader of the field through `nvse::record_value` sees it. The engine's
  drawing, sky and combat go on reading the records [hypothesis: as with the other
  record setters, nothing is saved].
- An ID outside a page's table reads 0 or "" and a setter with one does nothing
  [hypothesis].
- A flag ID is a bit number of the record's flags field, as with the projectile and
  explosion flags [observed: jip-records, projflag/explflag]. Here it is
  [documented: each page's flag table, which matches the schema's bit names].

## 2. Classes (CLAS)

`GetClassTraitNumeric class n`, `SetClassTraitNumeric class n value` [documented: their
pages, IDs 0–10]:

| n | Trait (wiki) | Field |
|---|---|---|
| 0–3 | tag skill #1–#4 (actor value codes) | DATA tag_skills[n] |
| 4–10 | base Strength … Luck | ATTR strength … luck |

SetClassTraitNumeric returns the value it set [documented: returnType traitValue].

`GetClassFlag class id` / `SetClassFlag class id value` [documented: IDs 0–19]:
- 0 Playable and 1 Guard are DATA flags bits 0 and 1.
- 2–19 are the service flags from bit 0. The page names 2 weapons … 8 stimpaks,
  12 miscellaneous, 18 recharge and 19 repair; the schema's ServiceFlags has them at
  bits 0–6, 10, 16 and 17, so ID = bit + 2 [documented; the offset is read from the two
  tables].

FalloutNV.esm, the page's example class NCRRanger: tag skill #3 42, Strength 5, Luck 5.

## 3. Combat styles (CSTY)

`Get/SetCombatStyleFlag style id` [documented: IDs 0–8]: CSTD flags, whose bits the
schema names the same way (bit 3 has no name on either side).

`Get/SetCombatStyleTraitNumeric style n` [documented: IDs 0–63, their names]:
- 0–14 are CSSD in order: cover search radius … semi-auto firing delay mult max, with
  9 the weapon restrictions.
- 15–42 are CSTD in order: dodge chance … rushing attack distance mult.
- 43–63 are CSAD in the record's field order.

The page lists the dodge fatigue, block skill and attack skill pairs base first, but CSAD
keeps them mult first, and the IDs follow CSAD, not the page's names [observed: jip4,
FollowersCombatStyleRanged 43 reads -20, CSAD's first field; 44 reads 0; the same for
53/54 and 57/58]. FollowersCombatStyleRanged: cover search radius 2048, combat radius
10000, dodge chance 50.

## 4. Lights (LIGH)

`Get/SetLightTraitNumeric light n` [documented: IDs 0–6]: 0 radius, 1–3 red, green and
blue (DATA radius, color.red/green/blue), 4 falloff exponent, 5 FOV (DATA), 6 fade
(FNAM). `Get/SetLightFlag light id` [documented: IDs 0–31]: DATA flags bits. The page's
11 and 12 (colour shift) have no schema name and read the bit all the same.
BasementLightFillWarm: radius 200, red 177, fade 1.25.

## 5. Grass (GRAS)

`Get/SetGrassTraitNumeric grass n` [documented: IDs 0–9] are the DATA fields in order:
density, min slope, max slope, units from water amount, units from water type, position
range, height range, colour range, wave period, flags (1 vertex lighting, 2 uniform
scaling, 4 fit to slope, the schema's bits 0–2). `Get/SetGrassModel` is MODL
[documented]. GrassWasteland01: density 30, position range 22, model
`Landscape\Grass\GrassWasteland01.NIF`.

## 6. Texture sets (TXST)

`Get/SetTextureSetTexture set n` [documented: map IDs 0–5, diffuse … environment] read
TX00–TX05, "" when the subrecord is absent. 1stPersonSniperRifleTexture 2 (the page's
example) reads `Weapons\2HandRifle\SniperRifle_m.dds`. Not built: the texture set's
flags 1–3 and numeric traits, which belong to the decal data (DODT) that the schema does
not decode yet.

## 7. Climates (CLMT)

`Get/SetClimateTraitNumeric climate n` [documented: IDs 0–6]:
- 0–3 are the sunrise and sunset begin and end in minutes. TNAM keeps them in 10-minute
  units, so the trait is the byte × 10 [documented: the page says minutes; the schema's
  wbClmtTime note says 10-minute units]. WastelandClimate (the page's example) has
  sunset begin 960, sunrise begin 300.
- 4 is volatility (TNAM).
- 5 (moons) and 6 (phase length) share TNAM's last byte: the moons are its top two
  bits, the phase length its low six [observed: jip4, WastelandClimate reads moons 2
  and phase length 3; SetClimateTraitNumeric 6 7 reads back 7 and leaves the moons 2].

## 8. Quests

`GetQuestFlag quest id` / `SetQuestFlag quest id value` [documented: IDs 0–7]:
- 0 running, 1 completed and 6 failed are the quest's state, the same one GetQuestRunning,
  GetQuestCompleted and FailQuest use.
- 2 and 3 are the record's DATA flag bits 2 and 3 (repeated topics, repeated stages).
- 4 (set running on reset) is DATA bit 0, Start Game Enabled [documented: the page's note,
  Start Game Enabled "equals flags 0 and 4"].
- 5 (shown in the Pip-Boy) reads 1 for a named quest that runs or ran, or after a script
  sets it [observed: jip4, 1 on VCG01 and VCG02, which the save has run, 0 on VMS01 and
  VMS20, which it has not, and 0 on VDeadSea, which runs from the start with no name;
  SetQuestFlag 5 1 reads back 1; hypothesis: the rule].
- 7 is unknown and reads as a script set it.

## 9. Factions, cells, waters, sounds

- `Get/SetFactionReputationType faction`: FACT WMI1 [documented: its page; the field: the
  schema's Reputation]. NCRFactionNV reads 0F43DE.
- `Get/SetCellClimate`, `Get/SetCellLightingTemplate`, `Get/SetCellNoiseTexture`: CELL
  XCCM, LTMP and XNAM [documented: their pages]. Gomorrah00, the pages' example, has none
  of the first two (0).
- `Get/SetWaterSound`, `Get/SetWaterNoiseTexture`: WATR SNAM and NNAM [documented]. NVCleanWater:
  sound 167340, noise `Data\Textures\Water\WastelandWaterPotomac.dds`. Not built:
  GetWaterTrait's 51 IDs, many of which its page leaves "Unknown".
- `GetSoundFlag sound id` [documented: IDs 0–13]: SNDD flags (SNDX in older records),
  bits the schema names the same way. EMTMedicalBeep01: loop (4) 1, 2D (6) 0.

## 10. Game-state getters

- `GetGameDifficulty`, `SetGameDifficulty n` [documented: 0 very easy … 4 very hard]:
  NVForever's difficulty. A number outside 0–4 changes nothing [hypothesis].
- `GetSessionTime` [documented: "the game's current session duration in seconds"]: the
  seconds run since the game started, menus included, as each frame's own length added
  up (the engine's frame time changes every frame, so the frame count times the current
  one is no clock, #1297) [hypothesis: a loaded save does not start a new session].
- `GetPCDetectionState` [documented: -1 n/a, 0 hidden, 1 caution, 2 detected, 3 danger]:
  the highest band any actor has the player in (`detection::Detections::highest`): hidden 0,
  noticed 1, detected 2, and 3 when that actor's combat target is the player
  [observed: jip4, 2 in the `nvftest` save, where Ringo, not hostile, stands near the
  player; hypothesis: 3 in combat, not probed]. With no measure it reads 0. NVForever never reads
  -1, because the function runs only in a game.
- `GetRefType` [documented: the type code of the reference, not its base]: the
  reference's own record type. The player is an ACHR; an inventory reference or one made
  at run time is a REFR, an ACHR or an ACRE by its base, as the script host names it.
- `GetTeleportDoor` [documented]: the door at the other end, SetDoorTeleport's change
  first (the same reading as GetLinkedDoor).
- `GetDelayElapsed quest` [documented: "the time remaining until the next iteration of
  the quest's script"]: the script delay minus the seconds since it last ran. It goes
  below 0 for a moment before the script runs, so there is no floor [observed: a scratch
  capture read VDeadSea 1.397, 1.381, 1.365 … s, 15.6 ms less each frame; one jip4 run
  read VCG01 below 0 and the next above]. It stays within the delay (5 s here).
- `GetGlobalRef global`, `SetGlobalRef global [form]` [documented: their pages; on a
  reference with no form, the reference]. The form ID is written into the global's own
  value as the bits of its float, and GetGlobalRef reads the bits back, 0 when they name
  no form [observed: jip4, GameYear 2281 reads GetGlobalRef 0; after SetGlobalRef of
  00104C0F it reads GetGlobalValue 1.49665e-39, which is those bits].
- `GetEditorPosition` [documented: returns the cell and writes the six output variables;
  0 for a reference not placed in the editor]. The angles are in degrees [hypothesis: as
  GetStartingAngle reads them]. Not probed: none of six forms of the call (let or set,
  to a ref or a float, three outputs or six, or no result) compiled through the capture
  plugin's run-time compiler [observed, 2026-10-30].
- `GetStartingPos axis`, `GetStartingAngle axis` (vanilla, built with this batch in
  `functions/refs.rs`): the reference's placement in the plugins, the angle in degrees;
  the player reads 0 on every axis [observed: jip4, DocMitchellRef 2288, 2244, 7360 and
  angle Z 22.918; his door 2430, 1536, 7359].
- `GetIsPoisoned` [documented: "under the influence of a poison-type effect"]: an effect
  on the actor whose source is an ingestible that GetIsPoison counts, all its effects
  hostile and detrimental [hypothesis: the rule].

## 11. Still open

The detection state 3 (needs a fight), GetEditorPosition (does not compile at run time,
§10), and GetIsPoisoned on a poisoned actor. The quest flag 5 rule and flag 7 are read
from five quests only.
