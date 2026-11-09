# Extender functions High and Dry needs: batch hnd1 (2026-11-09)

The functions High and Dry's scripts call most that were not built, ranked by a headless
new-game run of the modpack (`fnvtool script run`, 300 frames). Evidence: the parameter
and return types the original game lists (DumpDocs, `schemas/script/opcode_names.json`),
and probe set `quests2` run in the original with High and Dry's plugins from a save in
Goodsprings [observed]. Built in `crates/fnv-game/src/nvse_hnd1.rs`.

| Function | What the original answered | NVForever |
|---|---|---|
| GetQuests flag | 0: VMQ01 (running) and VCG02 (neither running nor completed); 1, 2, 3, 4, 8: VCG01 (completed) | 0: quests not completed with an objective shown; other flags: completed quests [hypothesis beyond the save's three quests] |
| GetQuestObjectives quest flag | VMQ01, flag 0: 10 30 25; flags 1-4: no array | flag 0: shown objectives, ascending (the original's order is open); other flags: 0 |
| GetObjectiveTargets quest index | VMQ01 objective 10: no array | the objective's QSTA targets, 0 when none |
| GetActorLeveledList | a placed actor (GSBigHorner): nothing | the placed record's base when it is a leveled actor list, else 0 |
| GetSequenceAnimGroup sequence | slot 0: 0 (Idle) on the player and a standing actor; slots 1-4: -1 | slot 0: the group playing (PlayGroup's, else the body's movement group); others -1 [hypothesis: weapon and idle slots] |

JIP's list functions give 0, not an empty array, when there is nothing to list
[observed: GetObjectiveTargets, GetQuestObjectives flags 1-4].

## hnd4 (2026-11-09)

Probe set hnd4 in the original (xNVSE 6.4.9, High and Dry's plugins and settings, the
bench save) [observed]:

- `GetSystemColor n`: 1 0xFFB642, 2 0xFF432A, 3 0xFFB642, 4 0x1AFF80, 5 0xFFB642 as
  0xRRGGBB; 0 and 6 read 0. NVForever gives SetSystemColor's colour when one was set.
- `GetJohnnyPatch n`: 1 and 3-8 read 1; 0, 2 and 9-12 read 0 (JohnnyGuitar's settings
  as High and Dry ships them).
- `GetRadiationLevelAlt` read as GetRadiationLevel (0 and 0 at the save).
- `GetExtraAccuracyPenaltyMult` read 1 for the equipped weapon and for a stimpak, and
  still 1 after `SetExtraAccuracyPenaltyMult 2.5` on the weapon; the setter returned 0.
- JIP's `SetObjectiveText quest index "text" [vars]` replaces the objective's display
  text for GetObjectiveText, the Pip-Boy and the HUD; for the session [hypothesis].

## hnd3 (2026-11-09): the rest of the load order's extender functions

`crates/fnv-game/src/nvse_hnd3.rs`, the last 137 functions the load order called that
had no handler (`fnvtool ext coverage`: missing 137 → 0). Three kinds [hypothesis for
each reading, from the functions' names and catalogue parameters]:

- A vanilla function with more arguments runs that function: `SetAngleEx` (SetAngle on
  each axis), `PlayIdleEx`, `PushActorAwayAlt` / `PushActorNoRagdoll` (PushActorAway
  by the force), `FireWeaponEx`, `TravelToRef` / `MoveToNode` (MoveTo, the node not
  told apart), `MoveToPosStr` / `RefToPosStr` (SetPos / GetPos as `"x y z"`),
  `GetArmorEffectiveDR/DT` (the actor's damage resistance / threshold),
  `SetBaseFactionRank`, `DismemberLimb` (the limb's condition to 0), the CCC functions
  that are a faction test, a line or a distance.
- A setter keeps its value for the session and its getter reads it back:
  `SetRefExtraData`, `SetDetonationTimer`, `SetActivatorPrompt`, `SetScriptDisabled`,
  `AttachLight` / `RemoveLight` (kept by node, not drawn yet), `AddScriptVariable` /
  `HasVariableAdded`, Companion Command & Control's saved numbers, strings and forms,
  the map and reputation icons, the on/off toggles.
- What only the original's renderer, camera or interface has answers as nothing is
  there: `WorldToScreen` (off screen, outputs 0), the UI projection, camera and
  highlight calls, the recipe, quantity and text input menus (closed).

## Source forms (compiler, 2026-11-09)

The source of these compiled where the compiler had refused it (`fnv-script` test
`compile_hnd`; source failures 14 → 7):

- A comma straight after a function's name: `ListGetNthForm, List, iPos` [observed:
  Unofficial Patch NVSE Plus].
- `GetActiveObjectives` takes an optional quest [observed: Main And Pause Menus
  Overhaul passes one and its compiled bytes hold the call].
- An extender expression counts a vanilla function's xEdit parameter types when the
  catalogue lists no parameters: `rAux.GetFactionRank Faction > 9` took no argument
  [observed: JIP Companions Command & Control].
- A console-only command that an extender also defines is the extender's in a script
  (`SetStringSetting`; `FunctionTable::script_by_name`) [observed: RG_PrimmPlayerHome].
- An arrow lambda whose body is the following lines, `({params} =>` … `)` [observed:
  FreePostMojave's `CallWhen ({} =>`].

Left: `ySIGetTagTrait` (a plugin's function the catalogue lacks), `aHUD.x` and
`rActorRef` (names from plugins not in the load order), a missing `+` in
MrShersh Functional Backpack's own line, and WeaponModCraftInfo written in xNVSE's
newer C-style syntax (`name …;`, `fn () {…}`, `for (… in …)`). They run from their
compiled bytes, as the original runs them.
