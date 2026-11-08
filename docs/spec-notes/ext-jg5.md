# JohnnyGuitar NVSE, batch 5 (2026-11-08, #1007)

Alternate textures, armour FaceGen model flags and icons, worldspace map textures and
encounter zones, region map names and the location name, camera shots, body part strings,
explosion and projectile sounds, equip types, recipe and weapon mod lookups, actor effect
types and the time played, in `crates/fnv-game/src/nvse_jg5.rs`. Sources: the GECK wiki
export [documented: the page named]; the record layouts from `schemas/fnv` (xEdit
wbDefinitionsFNV.pas); the base game's records read with `fnvtool esm decode`
[observed]. Not probed in the game: every value comes from a record, except
GetTimePlayed's clock. Setters change the record for the session, read by the getters.
Tests: `crates/fnv-game/tests/jg5.rs`.

The schema gains REGN `RDMP`, the map name: a zero-terminated string after an RDAT of
type 4 [observed: VMapGoodspringsRegion holds "Goodsprings", priority 50]. `GameData`
keeps it with the header's priority (`RegionDef::map`).

| Function | NVForever |
|---|---|
| GetWeaponAltTextures `weapon` | the texture sets of the world model's alternate textures (MODS), in record order [documented] |
| GetArmorAltTextures `armor model` | 1 male biped (MODS), 2 female biped (MO3S), 3 male world (MO2S), 4 female world (MO4S) [documented: the model IDs; the subrecords: ARMO's model order, MODL MOD3 MOD2 MOD4, as SetBipedModelPath reads it]; an empty array for another model |
| GetAltTexturesEx `form [model]` | a map from each 3D index to its texture set [documented]; an armour without a model reads model 1 [hypothesis: the page says the ID "should be used"] |
| SetWeaponAltTexture `weapon index set`, SetArmorAltTexture `armor model index set` | the entry of that 3D index takes the texture set, or a new entry is added [documented: the notes on 5.07]; 1 when done. A new entry has no shape name, and a form that is not a TXST changes nothing [hypothesis] |
| ClearWeaponAltTexture `weapon index` | that index's entry removed, every one for -1 [documented]; 1 when one was removed [hypothesis] |
| GetFacegenModelFlag, SetFacegenModelFlag `armor flag female [on]` | 0 head, 1 torso, 2 right hand, 3 left hand of MODD (male) or MOSD (female) [documented: the flag IDs; the subrecords: schema common.json; observed: leather armour's MODD and MOSD are both Torso] |
| SetBipedIconPathAlt `path female form` | ICON (male) or ICO2 (female), read by GetBipedIconPath [documented] |
| GetWorldSpaceMapTexture, SetWorldSpaceMapTexture `world [path]` | WRLD ICON [documented; observed: WastelandNV's `interface\worldmap\wasteland_nv_1024_no_map.dds`, the page's example path] |
| GetWorldspaceEncounterZone (GetWorldZone) `world` | SetWorldspaceEncounterZone's zone, else WRLD XEZN [documented] |
| SetWorldspaceEncounterZone (SetWorldZone) `world [zone]` | none removes it; 1 when done, 0 for a form that is not a zone [documented]. A cell that names no zone takes its worldspace's (`nvse_jg5::cell_zone`, read by GetCellEncounterZone, GetZone and the cell-load level fixing, `zones.rs`) [hypothesis, as encounter-zones.md §1]. SetCellEncounterZone's change now reaches the level fixing too |
| GetRegionMapName, SetRegionMapName `region [name]` | the map name (RDMP), "" without map data; the setter gives a region without map data one, at priority 0 [documented: "it will be created"; hypothesis: the priority] |
| GetLocationName `ref.` | indoors the cell's name (FULL); outdoors the map name of the regions the cell is in (CELL XCLR), the highest map priority first, an empty name passed over, else the worldspace's name; "" for none [documented: "interior cell name, worldspace name or a Region Map Data name", and the 5.18 note; hypothesis: the order, as the Pip-Boy's place in pipboy.md]. Without a reference, the player's [hypothesis] |
| SetExplosionSound `explosion which sound` | 1 sound 1, 2 sound 2 (EXPL DATA), read by GetExplosionTraitForm [documented] |
| SetProjectileSound `projectile which sound` | 1 the sound, 2 countdown, 3 disable (PROJ DATA sound, sound_countdown, sound_disable) [documented] |
| (both sound setters) | a form that is not a SOUN changes nothing [hypothesis] |
| SetEquipType `item type` | ETYP of the item (a reference's base), read by GetEquipType [documented]; a type outside the Equip Type table (-1 to 13) changes nothing [hypothesis] |
| GetRecipeCategoryFlags `category` | 1 when the category is a subcategory (RCCT DATA bit 0, `RecipeCategoryDef`) [documented] |
| GetFormRecipesAlt `item` | the recipes whose outputs hold the item, in FormID order [documented; hypothesis: the order, as GetFormRecipes] |
| GetWeaponsForMod `mod` | the weapons whose mod slots (WMI1-3) hold the mod, in FormID order [documented; hypothesis: the order]; empty for a form that is not an IMOD |
| GetActorEffectType `effect` | SPEL SPIT type: 0 actor effect, 1 disease, 2 power, 3 lesser power, 4 ability, 5 poison, 10 addiction; -1 for anything else [documented] |
| GetCameraShotFlags, SetCameraShotFlags `shot [flags]` | CAMS DATA flags [documented] |
| GetCameraShotImageSpaceModifier, SetCameraShotImageSpaceModifier `shot [imod]` | MNAM, none clearing it [documented]; a form that is not an IMAD changes nothing [hypothesis] |
| GetCameraShotTraitNumeric, SetCameraShotTraitNumeric `shot trait [value]` | 0 action, 1 location, 2 target (their DATA codes), 3-5 the player's, target's and global time multipliers, 6 max time, 7 min time, 8 target % between actors [documented: the notes]; 0 for another trait; a code outside the field's table or a number that is not finite changes nothing [hypothesis] |
| GetBodyPartTraitString `bpd part trait` | 1 part node (BPNN), 2 VATS target (BPNT), 3 IK start node (BPNI), 4 part name (BPTN), 5 gore target bone (NAM4) of the part of that type (BPND part type) [documented: the tables; the subrecords: schema combat.json]; "" for a part the data lacks or another trait. A part starts at its BPTN, or at a BPNN when the part before has its BPND [documented: xEdit BPTD order, as `GameData::body_part_name`] |
| GetTimePlayed `[unit]` | 0 milliseconds, 1 seconds, 2 minutes since the executable started [documented]; NVForever counts from the first game the process makes (`Game::new`), and another unit reads milliseconds [hypothesis] |

The kill camera (`engine/nvf-play/src/kill_cam.rs`) plays a camera shot as a script
changed it (`nvse_jg5::camera_shot`).

Follow-ups: the drawn models read their alternate textures from the plugins
(texture-swaps.md), so the alt-texture setters reach the getters only; the world map reads
WRLD ICON from the plugins; the Pip-Boy's place line does not read region map names yet;
the kill camera picks which shots of a path are hit shots from the record's action, not a
changed one; nothing reads a projectile's sounds yet.

Not in this batch, still on the census: GetRGBColor (how the three channels pack is not
documented), HSVtoRGB and RGBtoHSV (outputs into script variables), GetCustomMapMarkerIcon
and SetCustomMapMarkerIcon (the default icon path is not documented), and the functions
that need engine systems NVForever lacks (NiNode and texture queries, navmesh obstacles,
HUD and menu hooks, the event handlers).
